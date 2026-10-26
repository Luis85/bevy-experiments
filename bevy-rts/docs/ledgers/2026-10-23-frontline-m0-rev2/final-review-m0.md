# Final whole-branch review — M0 (foundation)

Range: `dced2af..56e3425` on `m0/phase-a` (4fb22bd, the M1 plan, read only as input). Read-only review; nothing
built or run. Authority: spec rev 2 + §15 + rev 2.1, ADRs 0001–0013, the M0 rev-2 plan (Global Constraints, DoD),
both SDD ledgers (R1–R69 taken as given). Already-known items not repeated: release-only failure of
`unclassified_sim_type_fails_build`, DEBUG spawn logging cost, `BRP_EXTRAS_PORT` doc mismatch, perf on a loaded
machine.

Scope of this review is what per-task reviews miss: cross-crate integration, contract consistency, and what
M1 (`docs/superpowers/plans/2026-10-24-frontline-m1.md`) builds on. I found no live determinism bug in the M0
code paths. Every M0 system is order-safe: entity-local updates, `SimIndex`-ordered shared writes,
sorted despawns, request-ordered spawns, and a sorted journal. The findings are holes in the *contracts and
guards* that M1 and later work will step into, and two compatibility paths (saves, replays) that don't do
what the spec and ADRs say.

Counts: **Critical 0 · Important 8 · Minor 18**

---

## Critical

None.

---

## Important

### I1. Dense `CommandTypeId`s in saves and replays are never remapped through the string tables

- **Where:**
  - `crates/fl_sim/src/save/load.rs:118` (`pending: save.sim.pending_commands` is returned raw)
  - `crates/fl_sim/src/replay.rs:286` (`ReplayDriver::new(r.inputs.clone(), …)`, with raw `kind.type_id`)
  - `crates/fl_protocol/src/header.rs:127-134` (`check_save` compares only format, features and mods; never the
    `"commands"` table)
  - The T30 report's ADR-0004 confirmation says the same thing: "PARTIAL: remap tested for saves only".
- **Failure scenario:** M1 Task 2 adds `core.follow`, `core.rotate` and `debug.set_terrain`, and nothing in the
  M1 plan bumps `CoreFeature::version`. The sorted `commands` table therefore shifts. In M0, `core.move` is id 0
  and `core.stop` is id 1. In M1, id 0 is `core.follow` and id 1 is `core.move`, and every `debug.*` id after
  `debug.set_owner` moves by one.
  1. A player saves an M0 testbed match through `fl.save` while a move is pending (input delay 1, so
     `LocalDriver::pending()` is non-empty).
  2. The save passes `check_save` in an M1 build, since the features are still `[("core", 1)]`.
  3. The pending `core.move` (type 0) is delivered as `core.follow`. The `MoveCmd` bytes either fail to decode
     as `FollowCmd`, which is a silent `BadPayload` rejection, or decode as the wrong command.
  4. A pending `core.stop` (type 1, empty payload) becomes `core.move` and is rejected.

  Replays have the same latent problem whenever a feature adds a command without bumping its version: the hash
  divergence then looks like a sim desync instead of a format mismatch.
- **Fix:** in `load_world`, map every pending command's `kind.type_id` through
  `LoadCtx::remap(tables::COMMANDS, id)`, and treat an unknown name as `LoadError`. In `replay_app` and
  `verify_replay`, do the same with the recorded header's tables before building the `ReplayDriver`. Add a test
  that loads a save with a pending command into a sim with an extra command name that sorts first (the
  `golden_stability` `"aaa.noop"` fixture works for this).

### I2. The save-compatibility rules contradict each other, so typed codec migrations can't run on real saves

- **Where:**
  - `crates/fl_protocol/src/header.rs:132` requires `features` to be exactly equal, versions included.
  - `crates/fl_sim/src/feature.rs:31` says to bump the version "whenever the feature's sim behaviour changes".
    Spec §5.2 adds "or save codecs change".
  - Spec §5.5 says "features and mods must match, **or have migrations**".
  - Spec §13.2 says "migrations are kept forever", with the policy applying from the end of M0.
  - `crates/fl_sim/tests/save.rs:548` (`migration_from_v1`) only passes because its feature keeps version 1
    while its codec moves to v2.
  - `crates/fl_protocol/tests/support/format_guard.rs:68` accepts a bumped codec version without checking that a
    migration from the old version exists.
- **Failure scenario:** a feature author follows the rule and bumps `version()` when changing a codec, for
  example `infantry.suppression@2` with `migrate_from::<v1>(1)`. Every older save is then rejected with
  `HeaderMismatch::Features` before any migration runs, so the migration is dead code. The other way round, an
  author who changes a codec without bumping the feature version gets loadable saves, but a lockstep peer or a
  replay on an older build passes the header checks with different save semantics.

  M1 Task 8 (`core.move_order@2` with a v1 migration) is the first real case. It only works because M1 doesn't
  bump core.
- **Fix:** decide before M1 Task 8 and record it in ADR-0006/0007.
  - Recommended: `check_save` compares feature **ids**, plus versions no newer than the build's. Per-component
    and per-resource codec versions plus migrations decide loadability. Feature versions stay strict for
    `check_replay` and `check_handshake`.
  - Make the format guard fail when a codec's version rises without a `migrate_from` (or a `custom` decode)
    covering the committed version. Exposing the migration list in `codec_formats` is enough.
  - Add a test that loads a save whose header has an older feature version.

### I3. The classification check runs once at build time and only looks at a type's outermost path

- **Where:**
  - `crates/fl_sim/src/build.rs:183-185`: the only call to `check_classification`, run after `initialize_sim_tick`.
  - `crates/fl_sim/src/classify.rs:561-565` (`is_sim_type`, outermost path only).
  - `crates/fl_sim/src/builder.rs:246` (the `app_mut()` escape hatch).
- **Failure scenarios:**
  1. **Late registration.** A feature's `DefComponent::instantiate` (or a command handler, or a `SpawnInit`)
     inserts `fl_feat_x::AmmoState`, which no system queries and which is read only through
     `world.get`/`EntityRef` in exclusive code. The type is first registered at the first spawn, after the
     check ran, so it is never flagged. It is gameplay state that is neither hashed nor saved, so the branch
     test is the only backstop, and only if a scenario exercises it.
  2. **Generic wrappers.** `sim.app_mut().add_message::<fl_feat_x::Hit>()` uses Bevy's message path instead of
     `SimBuilder::add_message`. `Messages<fl_feat_x::Hit>` passes the check (its outer path is `bevy_ecs`, and
     the unit test even pins `!is_sim_type("bevy_ecs::message::Messages<fl_sim::Ping>")`). It is also never
     rotated: Bevy's `message_update_system` lives in `First`, which the sim never runs. The buffer grows
     without bound and readers see messages from every earlier tick. The same applies to
     `bevy_ecs::hierarchy::ChildOf`/`Children`, which hold `Entity` and which C7 forbids for relationships.
- **Fix:**
  - Under `debug_assertions`, re-run `check_classification` in `step_tick` whenever
    `world.components().len()` has grown since the last check. It is cheap, and catches types registered at
    spawn time.
  - Treat a type as a sim type if *any* path segment of its name (generic arguments included) belongs to a sim
    crate. Whitelist only `Messages<M>` registered through `SimBuilder::add_message`.
  - Explicitly reject `ChildOf`/`Children` (and `Observer`) in the sim world.

### I4. `Transient` is a label with no enforcement, and late-written messages vanish in saves

- **Where:**
  - `crates/fl_sim/src/classify.rs:40`: nothing resets Transient state. Each owner clears its own (`clear_counters`,
    `clear_command_log`, `clear_sim_events`, funnel `drain()`).
  - `crates/fl_sim/src/builder.rs:235-241`: message buffers are Transient but double-buffered across a tick
    boundary.
  - `crates/fl_sim/src/save/mod.rs:102`: `assert_no_requests` covers only the spawn and despawn buffers.
  - `crates/fl_sim/tests/golden.rs`: the `"aaa.draws"` fixture is classified Transient yet accumulates across
    ticks. This shows how easily the label gets misused.
- **Failure scenarios:**
  1. M1 Task 11 adds `Steer` as a Transient *component*: "this tick's separation push". Idle units are steered
     only every `idle_every` ticks. If `move_system` reads a stale `Steer` on the ticks a unit isn't steered,
     and nothing removes it, gameplay depends on state that is never hashed or saved. A save taken between
     those ticks loads without it, so the branch test diverges, if a scenario catches it at all.
  2. Message timing. A feature writes a message in `Publish` or `Lifecycle::Despawn` that its reader in `Think`
     consumes next tick, which Bevy's double buffer allows. A save between those ticks drops the message:
     `Messages` is Transient and only spawn/despawn are asserted empty. The live run and the loaded run then
     diverge.
- **Fix:** make Transient mean what the spec says ("cleared every tick") by having the framework do it.
  - For Transient components, a generic core `Housekeeping` system removes every instance.
  - For Transient resources, require a reset fn or `Default` and reset in `Housekeeping`. Opt-outs need a named
    reason: `Journal`, `SnapshotOut`.
  - For `add_message` buffers, either clear them at the end of `step_tick` (the spec already says a message is
    read in the tick it was written), or at save time assert, under `debug_assertions`, that every registered
    buffer has no message newer than its slowest reader.

### I5. The determinism lints miss the entity-keyed and third-party hash collections

- **Where:** `xtask/clippy-template.toml:39-61` bans only `std::collections::{HashMap,HashSet}` and
  `bevy_platform::collections::{HashMap,HashSet}`.
- **Failure scenario:** `bevy_ecs::entity::{EntityHashMap, EntityHashSet, EntityIndexMap, EntityIndexSet}` are
  aliases of the **newtype structs** `EntityEquivalentHashMap` and so on (checked in `bevy_ecs-0.20.0`:
  `entity/hash_map.rs:23,26`, `hash_set.rs:28,31`, `index_map.rs:38`). Clippy's `disallowed-types` matches the
  defining path, so these pass. `hashbrown::HashMap` and `indexmap::IndexMap` with `RandomState`, and
  `BTreeMap<Entity, _>` or sorting by `Entity`, are allowed too.

  An M1 nav or path cache keyed by `Entity` and iterated to apply results gives the same order in a twin run
  and a different order after a load: `load_world` respawns entities in `SimId` order with fresh `Entity`
  bits, while the live run has holes from despawned projectiles. The branch test may catch it, but only if the
  scenario hits it. The lint is the cheap, certain guard, and §5.4 says hash maps are banned in iteration that
  affects state.
- **Fix:** add the four `bevy_ecs::entity::*` types plus `hashbrown::{HashMap,HashSet}`,
  `indexmap::{IndexMap,IndexSet}` (unless built with a fixed hasher type), `std::hash::RandomState` and
  `bevy_platform::hash::RandomState` to the `sim` profile. Add one line to `.claude/rules/sim.md`: never order or
  key sim iteration by `Entity`; use `SimId`.

### I6. Spawn-funnel bypasses are undetected, and unindexed entities are invisible to the hash and saves

- **Where:**
  - `crates/fl_sim/src/ids.rs:19`: `pub struct SimEntity(pub SimId)` can be constructed anywhere.
  - `crates/fl_sim/src/spawn.rs:31,156,160`: `SpawnInit` and `DefComponent::instantiate` get an
    `EntityWorldMut`, so `world_scope(|w| w.spawn(..))` is reachable.
  - `crates/fl_sim/src/hash.rs:286` and `save/mod.rs` iterate only `SimIndex`.
- **Failure scenarios:**
  - A feature system calls `commands.spawn((Pose{..}, Shell{..}))` without going through `SpawnRequest`. The
    entity takes part in gameplay queries but is never hashed or saved, and after a load it is gone.
  - A feature calls `commands.spawn(SimEntity(SimId(n)))`. That skips the allocator, so a later `alloc()`
    panics with "already used", or ids depend on command order.

  Spec §5.6 says nothing allocates `SimId`s outside the funnel, but nothing checks it.
- **Fix:**
  - Make `SimEntity`'s field private, with a crate-private constructor and a `pub fn id()`.
  - In `step_tick`, under `debug_assertions` (and in every test), assert that no entity carrying a classified
    component lacks `SimEntity`. Add `ComponentCodec::count(world)`, or a `Query<(), (Without<SimEntity>,
    Or<…>)>` built from the state registry.
  - In Bevy 0.20 resources are entities (`IsResource`), so don't compare raw entity counts.

### I7. `RejectReason` is a closed central enum that every feature's handler must use

- **Where:** `crates/fl_sim/src/commands.rs:82` (9 fixed variants). `CommandHandler::apply` returns
  `Result<(), RejectReason>`. `SimEvent::CommandRejected.reason` is `format!("{reason:?}")`.
- **Failure scenario:** M1 already edits the enum (`InvalidTarget`). M4's `infantry.enter`, for example, needs a
  "building full" reason. A feature crate either edits `fl_sim` or reuses an unrelated reason. That is exactly
  the anti-goal of spec §2 ("adding a mechanic … means editing … a central enum"), and it gets more expensive
  after the M0 contract freeze, because the deprecation policy applies from the end of M0.
- **Fix:** before M1 Task 2, add `RejectReason::Feature(&'static str)` (a namespaced reason code such as
  `"infantry.building_full"`) and mark the enum `#[non_exhaustive]`. Optionally carry the code in
  `SimEvent::CommandRejected` instead of the `Debug` string.

### I8. `arch-check` enforces a short denylist for sim crates instead of the spec's "bevy_app + bevy_ecs only"

- **Where:** `xtask/src/arch.rs:198-216`. `HEADLESS_BANNED` lists 8 crates. `fl_content` gets an allowlist
  (`ECS_FAMILY`), but `Sim` and `Feature` roles don't.
- **Failure scenario:** a sim or feature crate adds `bevy_math` (glam, f32), `bevy_transform`, `bevy_asset`,
  `bevy_log`, `bevy_diagnostic`, `rand`, `bevy_rand` or a float physics crate. `arch-check` passes. Spec §3 and
  §5.1 say sim crates use only `bevy_app` and `bevy_ecs`, and §5.4 bans `rand` and `bevy_rand` crates. Clippy
  catches only uses of the specific banned types and methods, not the dependency.
- **Fix:** for `Role::Sim` and `Role::Feature`, allow only `ECS_FAMILY` plus `bevy_app` and its own family
  (`bevy_app` pulls `bevy_tasks`, `bevy_utils`, `bevy_reflect` and so on; list them). Add a deny for `rand*`,
  `bevy_rand`, `rapier*` and `avian*` in every headless role. Add a fixture test like
  `headless_dev_dep_on_client_flagged`.

---

## Minor

1. **Device gate rows can pass vacuously.**
   - Where: `xtask/src/gate.rs:381-387`.
   - nextest runs with `--no-tests=warn`, and a match of zero tests is a `Warn`, which passes. The `gpu` and
     `brp` rows are the only proof of the BRP integration and offscreen screenshot. If their tests were renamed
     or `cfg`'d out (they are already gated on `cfg(all(debug_remote, debug_assertions))`), `gate milestone`
     would still pass.
   - Fix: make "no tests matched" a `Fail` for `gpu`, `brp` and the milestone-level rows.

2. **The perf gates are weak at milestone level.**
   - `gate.rs:686` (`perf_file`) checks only that `docs/perf/M<n>.json` exists, not G7 (≤ 15 ms at 3,000 units)
     or the 20% rule.
   - `docs/perf/M0.json`, which the M1 plan (line 113) names as the 20% baseline, isn't protected:
     `OWNERS.toml:39` protects only `docs/perf/baselines/**` and `budgets.ron`.
   - Fix: protect `docs/perf/M*.json` (or move them under `baselines/`), and have `perf_file` parse the file
     and check the 3,000-unit mean against 15 ms.

3. **`check_budgets` ignores a budgeted counter that disappears from the run.**
   - Where: `crates/fl_test_utils/src/perf.rs:153`.
   - The insta totals snapshot backstops the other counters, but `alloc.*` is left out of it. If allocation
     counting broke (the counting allocator dropped, or `alloc: None`), the alloc budgets would silently stop
     gating.
   - Fix: fail when a budgeted name is absent from `per_tick_max`.

4. **The codec-format snapshot covers only the core feature.**
   - Where: `crates/fl_sim/tests/formats.rs` calls `build_sim_app(&[], …)`.
   - Spec §12.7 asks for "every registered save codec", so M4+ feature codecs would go unpinned. `Codec::custom`
     encodings (players, unit type) also trace `T`, not the bytes actually written, so a change to the custom
     encoder is invisible.
   - Fix: move the codec section to `fl_test_utils` with `fl_catalog::features()`, and require `custom`
     codecs to supply their own `trace`.

5. **The system-order snapshot isn't in the task-level smoke set.**
   - `fl_test_utils/tests/system_order.rs:9` isn't a `smoke_` test. A sim-core task that touches only `fl_sim`
     adds or reorders `SimTick` systems without the snapshot running until `gate pr`, while §12.1 lists it at
     task level.
   - Fix: rename it to `smoke_sim_tick_system_order`.

6. **The contract surface isn't fully pinned.**
   - `xtask/src/api.rs:20` omits `fl_content`, whose loader and registry are a contract path under §13.2; M1
     changes `ContentKind`, `DefComponent` and `MapDef`.
   - The `contract` track in `OWNERS.toml:8-13` omits `fl_sim/src/{hash,replay,spawn,commands}.rs`: the state
     hash encoding, replay format, spawn funnel and command apply order. Golden replays backstop the hash
     stream, but not the reasons behind a change.
   - Fix: add `fl_content` to the API snapshot and those four files to the contract track.

7. **A save attempt can kill the running match.**
   - Where: `crates/fl_sim/src/runner/mod.rs:125` and `save/mod.rs:39`.
   - `SimQuery::Save` calls `save_world`, which panics on pending requests. The threaded runner turns any
     query panic into `FromRunner::Failed` and exits, so a feature that writes a `SpawnRequest` after
     `Lifecycle` would make `fl.save` end the testbed match.
   - Fix: `save_world` returns `Result`, and the reply is `SimReply::Error`.

8. **`LockstepDriver` isn't save- or late-join-ready.**
   - Where: `runner/lockstep.rs:117,177`.
   - `pending()` returns only the local player's commands, so a lockstep save loses other peers'
     received-but-unrun batches and diverges after a load.
   - A second batch for the same `(tick, player)` silently overwrites the first.
   - Commands inside a batch aren't checked for `cmd.player == batch player` or `cmd.exec_tick == tick`, so a
     peer can submit commands as another player.
   - There is no `restore` or `starting_at`.
   - Fine for the loopback, but the contract should be fixed before `fl_net`.

9. **`build_id()` is always `"dev"`.**
   - Where: `crates/fl_sim/src/build.rs:72-77`. Nothing sets `FL_GIT_REV`; there is no `build.rs` and no xtask
     env.
   - `check_handshake`'s build-id equality, ADR-0007's "a peer with different code is caught", and the replay
     build warning are therefore all vacuous.
   - Fix: a small `build.rs` in `fl_sim` with `git rev-parse --short HEAD` plus a dirty flag, or set the env
     var from xtask.

10. **RNG stream registration isn't enforced.**
    - Where: `crates/fl_sim/src/rng.rs:29`.
    - `rolls()` takes a raw `u64`, so a system can draw from `stream_key("combat.x")` without registering it,
      or two features can share one key, and the collision check never sees it.
    - Fix: `register_stream` returns a `Stream` token, and `rolls` takes the token.

11. **`SimCounters` forces an order on every counting system.**
    - Where: `crates/fl_sim/src/builder.rs:98`. Only `Journal` is ambiguity-allowed, so any two unordered
      systems in one slot that take `ResMut<SimCounters>` fail the schedule build.
    - Counter adds commute, and §15 requires counters in every mechanic, so this will force artificial
      `.chain()`s. It also blocks a future multi-threaded executor.
    - Fix: `allow_ambiguous_resource::<SimCounters>()`, or per-system local counts merged in `step_tick`.

12. **`track_spatial` has an uncounted O(N) per-tick scan.**
    - Where: `crates/fl_sim/src/spatial.rs:257`.
    - It walks all of `SimIndex` with `poses.get_mut(entity)` (random access) every tick, but counts only
      inserts and relocations. Perceive is the largest phase (2.38 / 6.09 ms at 3k / 5k units), and the work
      counters can't see this part of it.
    - Fix: count `spatial.checked`, and consider driving relocation from movers.

13. **`fl_cli` ships test frameworks.**
    - Where: `crates/fl_cli/Cargo.toml:18`.
    - `fl_test_utils` is a normal dependency but only `tests/agent.rs` uses it, so `insta`, `proptest` and
      `stats_alloc` are linked into the CLI binary.
    - Fix: move it to `[dev-dependencies]`.

14. **The snapshot schema only covers units.**
    - Where: `crates/fl_sim/src/publish.rs:121` publishes only entities with `UnitType`+`Owner`+`Pose`, and
      `EntityRecord.unit` is a mandatory `UnitTypeId`.
    - M4 buildings (a separate object kind) and any non-unit visible entity need a schema change, which is a
      `fl_view` contract bump.
    - Fix: plan an `EntityRecord.kind`/object id, or a building view channel, in the M4 plan rather than
      retrofitting it.

15. **`Derived` rebuilds run in name order.**
    - Where: `crates/fl_sim/src/save/load.rs:222-235`.
    - Dependencies between derived caches (M1: `NavGrid` → `HpaGraphs` → `FlowCache`) therefore depend on how
      the names sort.
    - The M1 plan works around this by having `rebuild_nav` build all three, but the contract should say so:
      either explicit `after` dependencies, or one rebuild per feature.

16. **`MatchHeader.features` order.**
    - Spec §5.5 says "sorted by id". The code (`build.rs`, header doc) uses topological load order.
    - The behaviour is fine and deterministic; reconcile the text (rev 2.2) so lockstep implementers don't
      re-sort.

17. **Unbounded input growth.**
    - `MoveHandler` appends to `MoveOrder.waypoints` without a cap (`commands.rs`), and `PlayerCommand.units`
      is unbounded too.
    - In lockstep this is hashed state a peer can grow at will.
    - Fix: cap the queue (a rules value) and the units per command; M1's order queue is the place.

18. **Doc pins that M1 will invalidate.**
    - `docs/debugging.md:142,147,385,388,410` hard-code the tick-20 hash `8b3418f4fcb9d9af`, which M1 Task 5
      (`SIM_VERSION` 2, terrain) changes.
    - Fix: describe the check ("equals `fl_cli hash-dump` at tick 20") instead of the value, or regenerate in
      the M1 docs task.

---

## Notes on things that hold up well (checked, no action)

- `step_tick` follows §5.6 exactly. `(player, seq)` ordering is stable on every path (driver, inbox, lockstep
  flatten). Late, early and duplicate rejections and R45/R49 `LastSeq` handling are consistent across the
  sim, `CommandStamper::restore` and the save.
- The state hash is name-keyed and prefix-free, and absent state contributes nothing. `UnitTypeId` and faction
  ids hash by name. The spawn and despawn funnels are request-ordered and SimId-sorted respectively. The
  journal is sorted with per-group ranks. Spatial filing at spawn (R53) makes save-right-after-spawn exact.
- Threaded runner failure handling: `catch_unwind`, then `Failed`, then thread exit. The client pauses and
  records the failure.
- Client: `fl.save` is consistent with in-flight ticks, because the ordered channel and `pending()` are read at
  the same point. `fl.load` restores the driver's tick, pending commands and seqs.
- `arch-check` edges, `--all-features --target all` per-package trees and the client dev-dependency checks are
  sound (apart from I8). The golden replay can't be blessed by accident: `.replay.new` isn't read, a missing
  golden fails, and insta new snapshots fail.

---

## Ready to build M1 on?

**Yes, with conditions.** No finding blocks starting M1 Phase I. The M0 code is deterministic on every path it
exercises, the gates are real, and the observability surface is complete and documented. Fix or decide these
before the M1 tasks that depend on them:

- **Before M1 Task 2** (payloads):
  - I7 (`RejectReason` extensibility).
  - I1 (remap command ids on load and replay), because Task 2 is the change that shifts the command table.
- **Before M1 Task 5** (first new Hashed and Derived state): I3, I5, I6 and I8 are small guard changes that make
  M1's large sim surface self-checking.
- **Before M1 Task 8** (`core.move_order@2`):
  - I2 (the save-compatibility rule; needs a user/ADR decision).
  - I4 (Transient enforcement), which M1 Task 11's `Steer` relies on.

Minor items can ride along with the owning M1 tasks. Minor 1–2 belong in the next xtask touch.
