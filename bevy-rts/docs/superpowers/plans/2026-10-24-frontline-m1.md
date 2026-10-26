# Frontline M1 (Control & Movement) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make units controllable and movable on real terrain: PNG height and terrain layers with roads, forest, river and bridges; the speed formula and per-class blocking with clearance; HPA* paths through a budgeted request queue, flow fields for group moves, steering and hull rotation; selection, orders (`core.move`, `core.stop`, `core.rotate`, `core.follow`, Shift-queue) and testbed controls; path, passability and cell overlays; hot reload under ADR-0012; and the AI seam, with every new algorithm counted, budgeted, journaled and proven deterministic.

**Architecture:** Contracts land first in lane 1: three ADRs, then the new payloads, the debug-query mechanism and the AI seam types, then the terrain content format. The sim then grows a `nav` module (static and derived navigation data: `NavGrid`, A*, HPA*, flow fields) and a `path` module (the Hashed request queue and per-unit path state). Movement reads only these, so every cache is `Derived` and outcome-neutral. The client never sees `fl_sim`: it reads terrain from `ContentDb`, unit state from snapshots, and paths, passability and clusters through registered debug queries (`SimQuery::Debug`). It issues every order as a command through `LocalDriver`, which lets selection, input, terrain rendering and overlays run in lane 2 alongside the sim work.

**Tech Stack:** Rust 1.99.1 (edition 2024), Bevy `=0.20.0` (`bevy_app` + `bevy_ecs` only in sim crates; `bevy_picking` with `MeshPickingPlugin` and gizmos in the client), bevy_egui 0.43, `fixed` (I32F32), serde + ron 0.12 + postcard 1, serde_json, `png` 0.18 (already in `Cargo.lock` through `bevy_image`), proptest, insta, crossbeam-channel. Pathfinding is in-house. The `pathfinding` crate is not added: its A* can't count expansions or stop on a budget, and adding it would mean a download.

**Spec:** `docs/superpowers/specs/2026-10-23-frontline-rts-design.md` (revisions 2, 2.1 and 2.2; where they disagree, 2.2 wins): §5.3–§5.8, §6.2, §6.4, §7 (map, terrain, movement), §8 (AI seam), §9.1 (M1), §10, §11, §12, §15. ADRs: 0002 runner and `fl_view`, 0004 registries (Refinement R54), 0005 phases (Refinements R46, R47; 2026-10-24: Housekeeping holds the Transient resets), 0006 classification and saves (Refinements 2026-10-24: R70, R72, R73, R74), 0007 envelope and payloads (Refinements 2026-10-24: R70, R71, R73, input caps, `build_id`), 0008 debug commands, 0010 work counters, 0011 contract governance, 0012 hot reload. This plan adds ADR-0014 (terrain layers and movement classes), ADR-0015 (path requests, derived caches and debug queries) and ADR-0016 (AI controller seam), plus M1 refinements to 0007 and 0012 (Task 1). Other inputs:
- `docs/superpowers/plans/2026-10-23-frontline-m0-rev2.md`: its Global Constraints, the M1 roadmap row and the definition of done for every milestone.
- `.superpowers/sdd/2026-10-23-frontline-m0-rev2/progress.md`: the controller's rulings R20–R80 (R78: M0 saves load under M1 through `fill_missing`; R79: `core.invalid_target` is `RejectReason::Feature`; R80: `ToRunner` becomes `#[non_exhaustive]` in Task 12; R69: this plan's spec-gap resolutions are provisional until the user accepts the ADRs at Task 1; R70–R74: the final M0 review's contract changes; R76: steering output is a scratch resource; R77: command caps stay code constants). Rulings that affect how tasks are written are folded into the constraints below. `fix-a-report.md` and `fix-b-report.md` (same directory) describe the final M0 fix waves this plan was aligned with (`m1-preflight.md` lists every change).
- `.claude/rules/sim.md`: the sim rules as of the final M0 fix waves (Transient, messages, RNG `Stream`, reject codes, bans).
- `docs/research/blitzkrieg-mechanics.md` §1.8–1.9 (rules only, never code or data) and `docs/research/architecture.md` §4.
- Bevy API facts: the `bevy-api` skill, never memory.

## Global Constraints

**The M0 rev-2 Global Constraints** (`docs/superpowers/plans/2026-10-23-frontline-m0-rev2.md`, section "Global Constraints") apply unchanged to every task, except where this section changes them. Below are only the additions and changes.

**Where and how to build** (changes)
- **Lane 1** is `C:\fl\a` on branch `m1/phase-a`. The controller cuts it from `main` once the M0 final fix waves (A and B, through `f247a85` or later) have fast-forwarded `main` (`git -C C:\fl\a switch -c m1/phase-a main`).
- **Lane 2** worktrees are `C:\fl\m1tNN`, on branch `m1/tNN-<slug>`, cut from `m1/phase-a` at the commit their dependencies need.
- **Shared lane-2 build dir (R31).** Every cargo command in a lane-2 worktree runs with `CARGO_BUILD_BUILD_DIR=C:/t/fl/28/2710d09957a748`. Only one lane-2 worktree builds at a time. Lane 1 keeps the per-worktree dir from the repo config.
- **At most two builders.** Those are the lane-1 task and the lane-2 task. The controller's gate and merge runs use the slot of the lane they gate. A docs-only task that never runs cargo (Task 1) doesn't count as a builder.
- **`.gate-base` (R33).** At every dispatch the controller writes the task's base commit to `<worktree>/.gate-base`. Implementers never edit it. `check-scope` and `test-guard` judge only the task's own diff.
- `SCCACHE_BASEDIRS` stays unset (R59).
- **Dependencies.**
  - Task 3 adds `png = "0.18"` to `[workspace.dependencies]` and to `fl_content`. It is already in the lock, so nothing is downloaded. Task 3 may commit the resulting `Cargo.lock` delta (R50).
  - **PNG decoding lives in `fl_content` only** (role `Content` in `xtask/src/arch.rs`). `fl_content` is the only crate with a direct `png` dependency; it decodes the layers once, at content load, into integer `MapLayers`. `png` and its dependencies (`crc32fast`, `fdeflate`, `flate2`, `miniz_oxide`, `simd-adler32`, `bitflags`) are pure Rust with no Bevy crate, so the `Content` denylist and the `Sim`/`Feature` allowlist (`bevy_app` + the `bevy_ecs` family; it constrains only `bevy*` crates) both accept them in `fl_sim`'s tree. No sim crate, no `fl_test_utils` and no `fl_cli` code calls `png`; they use `fl_content::{decode_layers, write_layers}`. The client never decodes the layers either: it reads `MapDef.layers` from `ContentDb`.
  - Task 5 adds `fl_test_utils.workspace = true` to `fl_sim`'s `[dev-dependencies]`: the `crates/fl_sim/tests/*.rs` files of Tasks 5–12 use `fl_test_utils` (`ascii_terrain_content`, `ScenarioBuilder`, `assert_deterministic`, …). `arch-check` allows dev edges between non-client crates. Only integration tests (`tests/`) may use it; a `#[cfg(test)]` unit test inside `fl_sim/src` would see a second copy of `fl_sim`.
  - No other task edits `Cargo.toml` or `Cargo.lock` unless its **Files** list says so.
  - `tempfile` (already a workspace dependency) may be added as a dev-dependency where a task's Files list says so, and as a normal dependency of `fl_test_utils` (Task 5).
  - `fl_sim`'s `build.rs` sets `build_id` from `git rev-parse --short HEAD`, so `fl_sim` and its dependents rebuild after every commit. That's expected; it isn't a cache miss to investigate.

**Determinism** (additions)
- **Gameplay-driving state is `Hashed` (R60):** `TerrainGrid`, `PathQueue`, `PathFollow` and the `MoveOrder` queue.
- **Navigation data and caches are `Derived`:** `NavGrid`, `HpaGraphs` and `FlowCache`.
- **A Derived cache is outcome-neutral (ADR-0015).** It may change wall-clock time and observe-only counters. It never changes state, the budget a request is charged, or the tick on which a request completes. Every path request is charged its cold cost, the work a from-scratch computation does, whether or not a cache answered it.
- **Names, never dense ids, in hashed and saved state (R52, R54).** Terrain types are stored as a palette of terrain ids (strings). `MoveClass` is an enum.
- **Derived state stays consistent at mutation boundaries (R53).** A spawn, a `debug.set_terrain` or a reload updates every derived index it affects before the next system runs, so a save taken right after it loads to equal derived state.
- **Integer costs.** Path, flow and clearance costs are `u32`/`u16`/`u8` integers with explicit tie-breaks. Positions stay `Fx`, and nothing in `nav` uses floats.
- **Core systems** go into the core slot of their hook (R46). Order among core systems in one hook is explicit (`.chain()`, `.before`, `.after`). New messages are named `"core.msg.<x>"` (R47) and are added only with `SimBuilder::add_message`.
- **Transient is per-tick and framework-enforced (R72, ADR-0006 refinement).**
  - `Input::Housekeeping` removes every `Class::Transient` component from every entity and resets every Transient resource at the start of each tick. A Transient resource is `Class::transient()` (reset to `Default`), `Class::TransientReset(fn)` (for example a `clear` that keeps capacity), or `Class::TransientKept("reason")` when its owner empties it elsewhere. Plain `Class::Transient` on a resource panics.
  - **No per-tick Transient components on units (R76).** Removing one is an archetype move per entity per tick. Per-tick per-unit results go into a Transient scratch resource keyed and sorted by `SimId` (Task 11's `SteerPushes`).
  - Scratch buffers kept between ticks to save allocations (search heaps, `g`/parent arrays) are resources classified `Class::TransientReset(<clear that keeps capacity>)`, or plain locals inside one system run. Never `Local<T>`.
  - `step_tick` clears every `add_message` buffer at the end of the tick, so a message is read in the tick it was written. Anything that must reach a later tick (a path request, a replan flag) is `Hashed` state, as `PathQueue` and `PathFollow` are.
- **One `Derived` rebuild per feature (R74).** A feature registers at most one rebuild with `SimBuilder::register_rebuild`, and a second call panics. Core already registers `rebuild_spatial` (`crate::spatial::build`). From Task 5, core's one rebuild is `rebuild_core`, registered in `CoreFeature::build`: `rebuild_spatial`, then `nav::rebuild_nav` (`NavGrid`, then `HpaGraphs` from Task 7, then an empty `FlowCache` from Task 10). `load_world` and Task 12's reload run the rebuilds in feature load order. `Class::Derived` names no rebuild.
- **Randomness (`Stream` tokens).** Draws come only from `fl_sim::rolls(seed, tick, sim_id, stream)`, where `stream` is the `Stream` that `SimBuilder::register_stream(name)` returned. It is captured in a closure or passed with `IntoSystem::with_input`, as `wander_system` and `scale_fire` do. `fl_math::Rolls::new`/`roll` are clippy-banned in sim crates. Setup code that draws from a scenario's own seed (`ScaleScenario::populate`) keeps its existing scoped `#[expect(clippy::disallowed_methods)]`. M1 adds no stream: `"debug.wander"` keeps its draws.
- **Hash collections.** In sim crates, clippy bans std, `bevy_platform`, `hashbrown` and `indexmap` hash maps and sets, `RandomState`, and the `Entity`-keyed maps and sets. `fl_content` (edge profile) and the client crates ban the std ones. Nav caches use `Vec`s indexed by cell or node, and `BTreeMap`s keyed by `FlowKey`/`MoveProfile`. Nothing is keyed or ordered by `Entity`.
- **Spawn funnel and `SimEntity` (R73).** `SimEntity`'s field is private. Every unit is spawned through `SpawnRequest` (in a tick) or `setup_spawn` (setup and tests between ticks), and despawned through `DespawnRequest` or `setup_despawn`. In debug builds `step_tick` panics on an entity that holds a classified component but no `SimEntity`, and on a type first registered mid-match outside the builder.
- **`debug.*` commands** are issued as `LocalPlayer`, and the sim skips the ownership filter for `DEBUG_ONLY` kinds (R44).
- **Reject codes (R71).** `RejectReason` is `#[non_exhaustive]`, and every rejection is reported by its stable code (`RejectReason::code()`, in `SimEvent::CommandRejected` and the `core.command_rejected` journal record). M1 adds no `RejectReason` variant. Its one new reason is `RejectReason::Feature("core.invalid_target")`, code `"core.invalid_target"` (Tasks 5, 9). Tests assert codes, never `Debug` names.
- **Command input caps (R77).** `fl_sim::MAX_COMMAND_UNITS` (256 units per command, `core.too_many_units`) and `fl_sim::MAX_WAYPOINTS` (32) stay code constants until C7. From Task 8, `MAX_WAYPOINTS` bounds `MoveOrder.orders`, every kind counted. A queued move, rotate or follow onto a unit whose orders are full rejects the whole command with `core.queue_full`, changing nothing.
- **Saved and recorded command ids are remapped by name (R73/I1).** New command names (`core.follow` sorts before `core.move`) shift the dense command ids. `load_world` and `replay_app`/`replay_inputs` map them through the recorded `commands` table, so a new command name never by itself needs a golden re-record.
- **Save compatibility (R70).** `check_save` needs the same feature ids, each saved at a version no newer than the build's, and equal mods. It ignores the rules hash, which Task 12's rebuild path relies on. Per-codec versions and migrations decide what decodes, and the format guard fails a codec whose version rose without a `migrate_from` the committed version. `save_world` returns `Result<_, SaveError>` (`#[non_exhaustive]`; `PendingRequests` while spawn or despawn requests wait), and `SimQuery::Save` can answer `SimReply::Error`.
- **Bad-command tests** assert that gameplay state is unchanged and that `LastSeq` advanced (R45). Late and early commands don't advance `LastSeq` (R49).
- **`SIM_VERSION`** (`crates/fl_sim/src/build.rs`) becomes 2 in Task 5, the first task that changes the hash stream of existing matches. It changes no further in M1; the bump policy is C8, in M2.
- **M0 saves load under M1 (R78).**
  - Task 5 adds `Codec::fill_missing(fn(&mut World) -> Result<(), String>)`. When an older-feature-version save lacks a state name, `load_world` fills it, and otherwise behaves as before. Task 5 also bumps `CoreFeature::version()` to 2 and registers the fill for `core.terrain`.
  - Task 8 registers fills for `core.path_queue` (empty) and `core.path_follow` (`Idle` on every unit with `Mobility`, so a loaded unit equals a live one).
  - The evidence is a protected M0 testbed save fixture generated from the M0 tip of `main` before Task 5's changes (Task 5 Step 0), and `crates/fl_test_utils/tests/m0_save.rs` loading it.

**Protected artifacts in M1** (additions to the M0 bless rule)
- **Which artifacts.** A task that adds sim systems, Hashed codecs, payloads or public API items changes protected snapshots:
  - `crates/fl_test_utils/tests/snapshots/system_order__sim_tick.snap`
  - `crates/fl_test_utils/tests/snapshots/formats__save_codecs.snap`: the codec block of every catalog feature (`<name>@<v> reads …`), traced by `fl_test_utils/tests/formats.rs` since the final M0 fix wave. A new or bumped Hashed codec changes this file.
  - `crates/{fl_protocol,fl_view,fl_sim}/tests/snapshots/formats__*.snap`. `fl_sim`'s `formats__save_codec_formats.snap` now holds only the save and replay layouts, such as `ReplayStart`.
  - `xtask/snapshots/api/*.txt`, for the five API crates `fl_math`, `fl_protocol`, `fl_view`, `fl_content` and `fl_sim`. With all five unchanged, `cargo xtask api --check` prints `api: 5 crates unchanged`.
  - `docs/perf/M*.json` (protected since the final M0 fix wave). Task 21 writes `docs/perf/M1.json.new`, and the lead blesses it.

  A task that changes the testbed hash stream or the scale counters also changes `crates/fl_test_utils/tests/replays/*.replay`, `docs/perf/budgets.ron` and `crates/fl_test_utils/tests/snapshots/perf_counters__*.snap`. `check_budgets` fails a budgeted counter that a run didn't measure, so budgets come only from `FL_RECORD_BUDGETS=1` runs and are never hand-added.
- **The implementer** regenerates them as `.new` files:
  - insta writes `*.snap.new`
  - `cargo xtask api` writes `*.txt.new`
  - `FL_RECORD_GOLDEN=1` writes `*.replay.new`
  - `FL_RECORD_BUDGETS=1` writes `budgets.ron.new`

  The report pastes their diffs (or, for binary replays, the hash-stream change). The implementer never edits a protected file.
- **The lead** blesses with `cargo xtask bless <paths> --reason "M1 Task N: <why>"` before the task's commit. The commit carries `Blessed: <reason>`. When two branches both moved a protected file, the lead regenerates it after the second merge and never hand-merges it.
- **New ADRs** (Task 1) start `proposed`. Only the user accepts them; the lead asks. Tasks that change contract paths cite them in a `Refs:` trailer.

**Client** (additions)
- **Client tests** use `MinimalPlugins`, `TimeUpdateStrategy::ManualDuration` and a fake runner on `fl_view::runner_channel` that acknowledges each tick. Tests wait on those acknowledgements and never sleep; this lesson comes from the flaky despawn test fixed after M0 Task 21.
- **Picking and `Camera::world_to_viewport`** need a window and GPU. Put their logic behind pure functions, test those headless, and cover the wiring with the demo checklist and `brp_` tests.
- **Visuals follow the receive-timeline interpolation (R42).** Rings, paths and markers read the unit visuals' `Transform`, never a separately interpolated position.
- **Input is ignored** while egui wants the pointer or keyboard (bevy_egui 0.43's wants-input state; read its source in `~/.cargo/registry`) and while `SpawnPanel.placing` is true.
- **Every order** is a `PlayerCommand` issued as `LocalPlayer` through `LocalDriver`. The client never mutates sim state.
- **Clippy in client crates** (the root `clippy.toml`, profile `client`) bans the `std::time::Instant` and `std::time::SystemTime` types and std `HashMap`/`HashSet`. Use Bevy's `Time`, `Duration` and `BTreeMap`/`BTreeSet`.

**Observability** (M1 additions; the definition of done requires all of them)
- **Journal kinds:**
  - `core.path_request`
  - `core.path_result`
  - `core.order_changed` (replaces M0's `core.move_order`)
  - `core.terrain_changed`
  - `core.content_reload`
- **Counters:**
  - `path.requests`, `path.queue_len`, `path.nodes_expanded`, `path.los_cells`, `path.budget_spent`, `path.clusters_invalidated`
  - `flowfield.cells`, `flowfield.abstract_nodes`
  - `nav.cells_updated`, `steer.units`, `move.blocked`
- **Spans** (`tracing::info_span!`): `hpa_search` per request, `hpa_cluster_build` per rebuilt cluster, `flowfield_build` per abstract field and per tile.
- **Debug queries** (ADR-0015): `core.debug_queries`, `core.passability`, `core.terrain_at`, `core.clusters`, `core.paths`. Their JSON carries `Fx` as raw `i64` (`Fx::raw`), so no float enters the sim.
- **Log cost.** Per-request and per-unit log records (path requests, steering, flow tiles) are at `trace` level. In M0, `DEBUG` records per spawn cost about 30 ms per tick at 3,000 units, until `debug.ron`'s default filter (`info,fl_sim=debug,fl_sim::spawn=info`) kept them out. The journal and counters carry the per-request facts.

## Review Focus

These are the five failure modes most likely to bite a person using M1 that the spec implies but no roadmap item names, most likely first. Each one's tests are added to the owning task.

1. **Unreachable or blocked goals.** A target in water, off the map or on an island; a tank ordered across a river with no bridge; a bridge removed mid-route. The unit drives to the closest reachable point (`Partial`) or stops (`Unreachable`), with a journal record. It never oscillates, never enters a blocked cell and never panics. Tests: Task 7 `island_goal_is_partial`, `goal_in_water_snaps_to_nearest_shore`; Task 8 `unreachable_goal_goes_to_closest_point`, `bridge_destroyed_mid_route_repaths`; Task 11 `units_never_enter_blocked_cells`.
2. **Queue pressure.** Forty units ordered in one tick, an order replaced before its result arrives, a unit stopped or despawned while queued. The budget holds, results apply in request order, stale requests are discarded, and no request starves. Tests: Task 8 `queue_respects_budget_and_order`, `stale_request_is_discarded`, `despawned_unit_request_discarded`, `no_request_starves`.
3. **Save and load mid-pathing.** A save while requests are queued, while a group follows a flow field, or right after `debug.set_terrain`. Branch hashes are equal, and derived caches rebuild from scratch. Tests: Task 5 `save_right_after_set_terrain`; Task 8 `branch_with_queued_requests`; Task 10 `branch_mid_flow_follow`, `cache_does_not_change_outcomes`; Task 14 suite.
4. **Selection input edges.** Behaviour a player expects in each case:
   - A click on empty ground clears the selection (unless Shift is held).
   - A drag below the threshold is a click.
   - Double-clicking an enemy selects nothing.
   - Control groups lose dead units.
   - Ctrl+digit with an empty selection clears the group.
   - Keys typed into the console don't select.
   - Orders given while paused apply on resume.

   Tests: Task 15 `click_empty_ground_clears_unless_shift`, `tiny_drag_is_a_click`, `control_group_drops_dead_units`, `ctrl_digit_with_empty_selection_clears_group`, `input_ignored_while_console_open`; Task 16 `orders_while_paused_apply_on_resume`.
5. **Hot reload at awkward moments.**
   - A malformed RON file saved mid-edit: the match keeps running and the error is shown.
   - A reload in a match without cheats is refused.
   - A unit type removed while instances live: those instances are dropped and logged, and survivors keep their `SimId`s.
   - A reload while paused works.

   Tests: Task 12 `reload_refused_without_cheats`, `rebuild_drops_vanished_unit_types`; Task 19 `malformed_ron_keeps_match_running`, `removing_a_unit_type_drops_its_units`, `reload_while_paused_applies`.

---

## Preconditions

- M0 Task 30 and the M0 final fix waves A and B are merged (`f247a85` or later), and `main` is fast-forwarded with `gate milestone --milestone M0` green.
- `docs/perf/M0.json` exists (protected, `meta.head` `56e3425`). It is the baseline for the 20% rule.
- `main`'s tip is still M0 when Task 5 starts, so that Step 0 can generate the M0 save fixture from it (R78).
- The golden replay `testbed_wander.replay` is blessed.

## Execution topology

At most two builders run at once. **Lane 1** is `C:\fl\a` (sequential; contracts, sim, determinism). **Lane 2** is one worktree at a time (client, content tools, perf), building in the shared lane-2 dir (R31). The controller merges lane-2 branches into `m1/phase-a` (`--no-ff`) and re-runs `gate task`. `main` is fast-forwarded with `gate pr` green after Tasks 3, 11, 14 and 21.

```
Phase I   lane 1: 1 (docs, no cargo) → 2 → 3
          lane 2: 15 ────────────────────────→ 4 (after 3)
Phase II  lane 1: 5 → 6 → 7 → 8 → 9 → 10 → 11
          lane 2: 16 (after 2) → 17 (after 4) → 20 (after 2) → 18 (after 8)
Phase III lane 1: 12 (after 11) → 14 (after 12, 13)
          lane 2: 13 (after 11) → 19 (after 12)
Phase IV  lane 1: 21
```

Ruling R40 applies: a dependent may start while its dependency is in review, when the dependency's API is unlikely to change. `fl_sim/src/lib.rs` and `fl_client/src/lib.rs` re-export lines are the only expected merge conflicts; the controller resolves them.

| Parallel pair | Lane-1 task owns | Lane-2 task owns |
|---|---|---|
| 1–3 ∥ 15 | `docs/adr/**`, `docs/superpowers/specs/**`; `crates/fl_protocol/**`, `crates/fl_view/**`, `crates/fl_sim/src/{builder,debug_query,units,lib}.rs`, `crates/fl_sim/src/runner/mod.rs`, `crates/fl_sim/tests/debug_query.rs`; `crates/fl_cli/src/{main,lib,query_cmd}.rs`, `crates/fl_cli/tests/agent.rs`; `Cargo.toml`, `Cargo.lock`, `crates/fl_content/**` | `crates/fl_client/src/{selection,lib}.rs`, `crates/fl_client/src/debug/remote.rs`, `crates/fl_client/tests/{selection,remote}.rs` |
| 5–11 ∥ 4, 16, 17, 20, 18 | `crates/fl_sim/**` (except `scale.rs` after Task 5), `crates/fl_test_utils/**`, `Cargo.lock` (Task 5), `assets/base/rules.ron`, `crates/fl_sim/tests/fixtures/**`, `crates/fl_cli/tests/fixtures/**` | 4: `assets/base/{terrain,maps}/**`, `crates/fl_cli/src/{map_gen,main,lib}.rs`, `crates/fl_cli/tests/map_gen.rs`. 16: `crates/fl_client/src/{orders,lib}.rs`, `crates/fl_client/src/debug/{stats,console}.rs`, `crates/fl_client/tests/orders.rs`. 17: `crates/fl_client/src/{terrain,map_view,view,lib}.rs`, `crates/fl_client/src/debug/spawn_panel.rs`, `crates/fl_client/tests/terrain.rs`, `crates/fl_game/src/builder.rs`. 20: `crates/fl_client/src/{ai,lib}.rs`, `crates/fl_client/tests/ai.rs`, `crates/fl_game/src/builder.rs`, `crates/fl_game/tests/ai.rs`, `assets/base/scenarios/testbed_ai.scenario.ron`. 18: `crates/fl_client/src/{overlays,lib}.rs`, `crates/fl_client/src/debug/remote.rs`, `crates/fl_client/tests/{overlays,remote}.rs` |
| 12 ∥ 13 | `crates/fl_content/src/{db,def}.rs`, `crates/fl_content/tests/load.rs`, `crates/fl_view/src/{runner,lib}.rs`, `crates/fl_view/tests/view.rs`, `crates/fl_sim/src/{reload,units,lib}.rs`, `crates/fl_sim/src/runner/**`, `crates/fl_sim/src/save/**`, `crates/fl_sim/tests/reload.rs`, `crates/fl_client/tests/{debug,view}.rs` (fake-runner arms for `ToRunner::Control`) | `crates/fl_sim/src/{scale,replay}.rs`, `crates/fl_sim/tests/{scale_mix,replay}.rs`, `crates/fl_test_utils/src/perf.rs`, `crates/fl_test_utils/tests/perf_counters.rs`, `crates/fl_cli/src/bench.rs`, `crates/fl_cli/tests/bench.rs`, `crates/fl_game/src/builder.rs`, `crates/fl_game/tests/builder.rs`, `docs/perf/budgets.ron(.new)`, perf snapshots |
| 14 ∥ 19 | `crates/fl_test_utils/tests/{golden,movement_determinism}.rs`, `crates/fl_test_utils/tests/replays/**` | `crates/fl_game/**`, `crates/fl_client/src/{placeholder,link,view}.rs`, `crates/fl_client/src/debug/{mod,console}.rs`, `crates/fl_observe/src/config.rs`, `debug.ron` |

## File map (new or substantially changed)

```
docs/adr/{0014-terrain-layers-and-movement-classes,0015-path-requests-derived-caches-debug-queries,0016-ai-controller-seam}.md
docs/adr/{0007,0012}-*.md (Refinements)  docs/adr/README.md  docs/superpowers/specs/2026-10-23-frontline-rts-design.md
crates/fl_protocol/src/payloads.rs                         crates/fl_view/src/{runner,ai}.rs
crates/fl_content/src/{terrain,layers,builtin,kinds,mods,db,def}.rs
crates/fl_sim/src/{debug_query,terrain,path,orders,steer,reload,movement,commands,debug_cmds,units,feature,spatial,scale,replay}.rs
crates/fl_sim/src/nav/{mod,grid,astar,los,hpa,flow}.rs     crates/fl_sim/src/{runner/mod,runner/threaded,save/load}.rs
crates/fl_sim/tests/{debug_query,terrain,astar,hpa,pathing,orders,flow,steering,reload}.rs
crates/fl_test_utils/src/terrain.rs   crates/fl_test_utils/tests/{movement_determinism,golden}.rs
crates/fl_cli/src/{query_cmd,map_gen,bench}.rs
assets/base/terrain/*.terrain.ron   assets/base/maps/{testbed_1km,scale_2km}.{map.ron,height.png,terrain.png}   assets/base/rules.ron
crates/fl_client/src/{selection,orders,terrain,overlays,ai}.rs   crates/fl_game/src/{reload,replay_capture}.rs
docs/{m1-demo-checklist.md,debugging.md}   docs/perf/{M1.md,M1.json,budgets.ron}
```

---

## Phase I — contracts (lane 1) and selection (lane 2)

### Task 1: ADRs 0014–0016, refinements to ADR-0007 and ADR-0012, spec sync

**Track:** contract (docs only) · **Lane 1** · No cargo commands.

**Files:**
- Create: `docs/adr/0014-terrain-layers-and-movement-classes.md`, `docs/adr/0015-path-requests-derived-caches-debug-queries.md`, `docs/adr/0016-ai-controller-seam.md` (from `docs/adr/template.md`, MADR 4.0, status `proposed`)
- Modify: `docs/adr/0006-component-classification-and-save-format.md`, `docs/adr/0007-lockstep-command-envelope-and-match-header.md`, `docs/adr/0012-hot-reload-policy.md` (a new `## Refinements (2026-10-24, M1)` section each; ADR-0006 and ADR-0007 already have the final M0 review's `## Refinements (2026-10-24)` sections, which stay as they are), `docs/adr/README.md` (index rows), `docs/superpowers/specs/2026-10-23-frontline-rts-design.md` (§5.3, §5.5, §5.7, §6.2, §6.4, §8, §11, as listed below, plus a new revision section)

**Decisions each ADR records** (these are the contracts later tasks implement):
- **ADR-0014, terrain layers and movement classes.**
  - **Terrain kind.** `*.terrain.ron` is a built-in `TerrainDef { id, name, pass, blocks, color }`, rules-hashed.
  - **Map layers.** `MapDef` gains `terrain: Option<MapTerrain>`, naming a 16-bit (or 8-bit) grayscale height PNG and an RGB terrain PNG next to the map file, a height range in metres, and an RGB → terrain-id legend.
    - There is one pixel per cell, and pixel row 0 is the north edge.
    - The loader decodes the layers through a new `ContentKind::load_sidecars` hook into a non-RON `MapDef.layers` field, which the rules hash covers.
    - A map without `terrain` is open ground: pass 1, blocks nothing, height 0.
    - **Where PNG decoding lives.** Only `fl_content` decodes images (the `png` crate: pure Rust, no Bevy, accepted by `arch-check` for content and sim trees). Sim crates and the client read the decoded integer `MapLayers`, never a PNG.
  - **`MoveClass`** moves to `fl_content` (re-exported by `fl_sim`).
  - **Profiles.** A movement profile is `(class, size)`, where `size = ceil(footprint width / cell)` clamped to `1..=Path.max_clearance`.
  - **Blocking.** A cell is blocked for a class if its terrain lists the class in `blocks`, or if the height step to a 4-neighbour exceeds `Movement.max_slope_m[class]`. The map edge counts as blocked.
  - **Clearance.** Clearance is the largest k ≤ `max_clearance` for which the k×k square anchored at the cell (+x, +y) is unblocked.
  - **Speed** = `road_speed × clamp(pass + (1 − pass) × terrain_factor, Movement.min_speed_frac, 1)` (spec §7). `terrain_factor` may lie in [−1, 1] (research §1.8: trucks are negative).
  - **Path cost.** Path costs use a per-class `Movement.path_terrain_factor`, because one class's unit types differ and HPA* is shared per class.
  - **Terrain is Hashed state** (`core.terrain`), mutable by `debug.set_terrain` in M1, and later by craters, flattened forest and bridges.
- **ADR-0015, path requests, derived caches and debug queries.**
  - **The queue.** Requests enter a Hashed FIFO `PathQueue`: from command handlers in `(player, seq)` then `SimId` order, and from `advance_orders` in `SimIndex` order.
  - **Processing.** `ThinkSet::Behaviour` pops requests while `spent < Path.budget_per_tick` (at least one per tick). Each one is charged its cold cost, applied to its unit immediately, and so applied in request order. A request whose unit is gone, or no longer waits on it, is discarded at charge 0.
  - **HPA\*.** 16×16-cell clusters per profile, with the entrance rule of Task 7, A* refinement and string-pulling.
  - **Flow fields.** Group moves of `Path.flow_min_group` or more same-profile units use a flow field per `(destination cluster, profile)`. It is an abstract Dijkstra over the HPA* graph plus per-cluster tiles built lazily, in `SimIndex` order, before movement.
  - **Caches.** `NavGrid`, `HpaGraphs` and `FlowCache` are `Derived`. A cache never changes outcomes, because charges are cold costs and every cached value is a pure function of Hashed state. Eviction is therefore free to be any deterministic policy. They are rebuilt by core's one `Derived` rebuild (`rebuild_core`: spatial index, then `NavGrid`, `HpaGraphs`, an empty `FlowCache`; ADR-0006 refinement R74).
  - **Per-tick results.** Steering pushes and similar per-unit, per-tick results live in a Transient scratch resource sorted by `SimId`, never in a Transient component (R72: the archetype move per entity per tick; R76).
  - **Invalidation.** A terrain change updates `NavGrid` for the touched rect, rebuilds only the HPA* clusters it touches, clears the profile's flow entries, and makes units whose remaining path crosses a rebuilt cluster re-request.
  - **Debug queries.** `SimBuilder::register_debug_query(name, fn(&World, &Value) -> Result<Value, String>)`, answered between ticks through `SimQuery::Debug { name, args }` → `SimReply::Debug(Value)`. They are read-only, observe-only and JSON, with `Fx` as raw `i64`. Overlays, BRP `fl.debug_query` and `fl_cli query` use them.
- **ADR-0016, AI controller seam (stub scope).**
  - **Types.** `fl_view::ai::{PlayerView, AiController, NullAi, ai_controller}`.
  - **Building the view.** `PlayerView` is built from the snapshot on the client side until vision exists. M3 moves it into the sim's `Publish` phase.
  - **Hosting.** Players whose controller is `Controller::Ai(id)` are hosted by `fl_client::ai::AiHost`. It thinks once per new snapshot tick, and its drafts go through `LocalDriver` as that player.
  - **Scope.** The full AI placement ADR stays deferred to the first AI faction (§9.3).
- **ADR-0006, M1 refinement (R78).**
  - `Codec::fill_missing(fn(&mut World) -> Result<(), String>)` fills a Hashed or Derived state name that is absent from a save whose owning feature version is older than the build's. Fills run after every saved value is restored and before the Derived rebuilds; an `Err` is `LoadError::Codec`. Without a fill, or in a save of the same version, loading is unchanged.
  - Core becomes version 2 in M1. It registers fills for `core.terrain` (from the content map matching the saved `MapInfo`), `core.path_queue` (empty) and `core.path_follow` (`Idle`).
  - M0 saves therefore load under M1, as the `m0_testbed.save` fixture shows.
- **ADR-0007, M1 refinement.**
  - New core payloads `core.rotate { toward }` and `core.follow { leader }`, and the debug payload `debug.set_terrain { min_cell, max_cell, terrain }`. Adding a payload type needs no `FORMAT_VERSION` bump; the format guard allows added types. Their dense command ids may shift existing ones, and saves and replays remap them by name (the 2026-10-24 refinement, R73).
  - The input cap `MAX_WAYPOINTS` (32) bounds `MoveOrder.orders`, every order kind counted. A queued move, rotate or follow onto a full order is `core.queue_full` (R77: a code constant until C7).
  - A target the rules forbid (a blocked cell, a missing leader, a rect outside the map) is rejected with `RejectReason::Feature("core.invalid_target")`. That's a stable code, and no new variant (R71, R79).
- **ADR-0012, refinement.**
  - **Watcher.** Content is loaded with `std::fs`, not the AssetServer, so the testbed polls mod-file modification times (`debug.ron`: `hot_reload`, `hot_reload_poll_ms`) instead of using Bevy's `file_watcher`.
  - **Value reloads** reach the runner as `ToRunner::Control { id, control: RunnerControl::ReloadContent(OpaqueContent) }`, answered with `SimReply::Reloaded { tick, rules_hash }`. `ToRunner` becomes `#[non_exhaustive]` with that variant, like `SimQuery`/`SimReply` (R80).
  - **Derived state after a value reload.** It runs every feature's `Derived` rebuild in feature load order, as `load_world` does (R74).
  - **ID-set rebuilds** use `load_world_with(.., LoadOptions { drop_unknown_units: true })`.
  - **Replays.** The client captures a replay of the running match (`replay save <path>`), so reloads have a replay to record their `ContentReload` event in.
  - **Def components** gain `DefComponent::reload`, which keeps state ratios.

**Spec edits** (same commit; "an ADR that changes a contract updates the spec"):
- §5.3: `SimQuery::Debug`.
- §5.5: the core payload list names its fixed payloads.
- §5.7: the cold-cost charge, lazy flow tiles, Derived caches.
- §6.2: the terrain type fields and map layer format.
- §6.4: the polling watcher and the runner control message.
- §8: `PlayerView` is a client-built stub until M3.
- §11: overlays read debug queries.

- [ ] **Step 1: Write the three ADRs** with the decisions above, each with Context, Decision Drivers, at least two Considered Options with pros and cons, Consequences and Confirmation. Confirmation bullets name the tests of Tasks 5–20 that check each decision, by the test names this plan gives.
- [ ] **Step 2: Write the three refinement sections** (ADR-0006, ADR-0007, ADR-0012) and the README index rows (status `proposed`).
- [ ] **Step 3: Edit the spec sections listed.** Then add a section `## Revision 2.3 (2026-10-24, M1 contracts)` above `## Revision 2.2`, in 2.2's format: one bullet per contract with its ADR (ADR-0014, ADR-0015, ADR-0016, and the M1 refinements of ADR-0007 and ADR-0012). Revisions 2.1 and 2.2 already exist (M0 implementation and final M0 review), so don't reuse those numbers. Update the status line at the top of the spec to name revisions 2.2 and 2.3.
- [ ] **Step 4: Verify.** Run `git diff --stat`. Expected: only the files above. Then `grep -c "^status: \"proposed\"" docs/adr/001[456]-*.md`. Expected: `1` for each file.
- [ ] **Step 5: Acceptance (lead).** The lead asks the user to accept ADR-0014, ADR-0015 and ADR-0016 and the three refinements (ADR-0006, ADR-0007, ADR-0012). On acceptance, set `status: "accepted"` and the date. On change requests, revise and ask again; no later task starts before acceptance.
- [ ] **Step 6: Commit** `docs(adr): ADR-0014 terrain layers, ADR-0015 path requests and debug queries, ADR-0016 AI seam; refine 0007, 0012` with trailer `Refs: ADR-0011`.

### Task 2: Contract additions — payloads, debug queries, `PlayerView` and the AI seam types

**Track:** contract + tools · **Refs:** ADR-0007, ADR-0015, ADR-0016 · **Lane 1**

**Files:**
- Modify:
  - `crates/fl_protocol/src/{payloads,lib}.rs`, `crates/fl_protocol/tests/{protocol,formats}.rs`
  - `crates/fl_view/src/{runner,lib}.rs`, `crates/fl_view/tests/view.rs`
  - `crates/fl_sim/src/{builder,lib}.rs`, `crates/fl_sim/src/runner/mod.rs`
  - `crates/fl_cli/src/{main,lib}.rs`, `crates/fl_cli/tests/agent.rs`
- Create:
  - `crates/fl_view/src/ai.rs`
  - `crates/fl_sim/src/debug_query.rs`, `crates/fl_sim/tests/debug_query.rs`
  - `crates/fl_cli/src/query_cmd.rs`
- Blessed:
  - `crates/fl_protocol/tests/snapshots/formats__protocol_formats.snap`
  - `crates/fl_view/tests/snapshots/formats__view_formats.snap` (if it changes)
  - `xtask/snapshots/api/{fl_protocol,fl_view,fl_sim}.txt`

**Interfaces:**
- Consumes: `CommandPayload`, `EntityRecord`, `SimSnapshot`, `CommandDraft`, `StableRegistry`, `SimRunner::query`, the `inspect` command's source parsing in `fl_cli`.
- Produces:
```rust
// fl_protocol/src/payloads.rs (deny_unknown_fields like the others)
pub struct RotateCmd { pub toward: FxVec2 }        // "core.rotate": each unit turns its hull to face `toward`
pub struct FollowCmd { pub leader: SimId }         // "core.follow": follow `leader` at the unit's current offset
pub struct DebugSetTerrainCmd { pub min_cell: (u32, u32), pub max_cell: (u32, u32), pub terrain: String }
                                                   // "debug.set_terrain": cells min..=max (x, y) become terrain id `terrain`; DEBUG_ONLY, NEEDS_UNITS = false
// fl_view/src/runner.rs
SimQuery::Debug { name: String, args: serde_json::Value }
SimReply::Debug(serde_json::Value)
// fl_view/src/ai.rs
#[derive(Debug, Clone, PartialEq)] pub struct PlayerView { pub tick: Tick, pub player: PlayerId, pub team: TeamId,
    pub own: Vec<EntityRecord>, pub visible: Vec<EntityRecord> }
impl PlayerView { pub fn from_snapshot(snapshot: &SimSnapshot, player: PlayerId, team: TeamId) -> PlayerView; }
    // own: owner == player; visible: owner != player and visible_to bit `team` set; both in sim_id order
pub trait AiController: Send + Sync + 'static {
    fn id(&self) -> &'static str;
    fn think(&mut self, view: &PlayerView, tables: &StringTables) -> Vec<CommandDraft>;
}
pub struct NullAi;                                 // id "null"; think returns no drafts
pub fn ai_controller(id: &str) -> Option<Box<dyn AiController>>;   // "null" → NullAi; anything else → None
// fl_sim/src/debug_query.rs
pub type DebugQueryFn = fn(&World, &serde_json::Value) -> Result<serde_json::Value, String>;
#[derive(Resource)] pub struct DebugQueries { .. }  // External "core.debug_queries"; names sorted, frozen in finish()
impl SimBuilder { pub fn register_debug_query(&mut self, name: &'static str, f: DebugQueryFn) -> &mut Self; }
    // panics on a duplicate name, naming both features (StableRegistry)
// built-in query "core.debug_queries" → {"queries": [every registered name, sorted]}
// SimRunner::query(SimQuery::Debug { name, args }) → SimReply::Debug(v), or SimReply::Error("unknown debug query '<name>'"),
//   or SimReply::Error(<the query's message>)
// fl_cli: `fl_cli query <replay|scenario|save> --tick T --name <query> [--args <json>] [--json]`
//   runs to tick T exactly as `inspect` does, then prints the reply (exit 0) or the error (exit 1)
```

- [ ] **Step 1: Write the failing tests:**
```rust
// crates/fl_protocol/tests/protocol.rs
#[test] fn payload_names()                       // extended: RotateCmd "core.rotate", FollowCmd "core.follow", DebugSetTerrainCmd "debug.set_terrain"
#[test] fn debug_set_terrain_is_debug_only_without_units()   // DEBUG_ONLY && !NEEDS_UNITS; RotateCmd and FollowCmd are neither DEBUG_ONLY nor unit-free
#[test] fn new_payloads_round_trip_postcard()    // RotateCmd { toward: (Fx::from_raw(-7), Fx::from_int(3)) }, FollowCmd { leader: SimId(9) }, DebugSetTerrainCmd { (1,2), (3,4), "base.water" }
// crates/fl_protocol/tests/formats.rs: protocol_formats also traces the three new payloads (snapshot .new)
// crates/fl_view/tests/view.rs
#[test] fn player_view_splits_own_and_visible()  // records #1 owner 0, #2 owner 1 visible_to 0b01, #3 owner 1 visible_to 0b10;
                                                 //   from_snapshot(P0, team 0) → own [#1], visible [#2]
#[test] fn null_ai_issues_nothing()              // NullAi.think(any view) is empty; id() == "null"
#[test] fn ai_controller_by_id()                 // "null" → Some(id "null"); "nope" → None
// crates/fl_sim/tests/debug_query.rs
#[test] fn debug_queries_listed()                // SimRunner::query(Debug "core.debug_queries") → {"queries": [..]} containing "core.debug_queries", sorted
#[test] fn unknown_debug_query_is_error()        // name "nope" → SimReply::Error("unknown debug query 'nope'")
#[test] #[should_panic(expected = "registered by 'test' and 'other'")] fn duplicate_debug_query_panics_naming_both()
#[test] fn debug_query_does_not_change_state()   // state hash before and after 10 queries is equal
#[test] fn failing_query_reports_its_message()   // a test feature's query returning Err("bad args") → SimReply::Error("bad args")
// crates/fl_cli/tests/agent.rs
#[test] fn query_lists_debug_queries()           // `query base.testbed --tick 0 --name core.debug_queries --json` → exit 0, JSON "queries" contains "core.debug_queries"
#[test] fn query_unknown_name_fails()            // --name nope → exit 1, stderr contains "unknown debug query 'nope'"
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_protocol -p fl_view -p fl_sim -p fl_cli -E 'test(/payload|player_view|ai_|debug_quer|query_/)'`. Expected: FAIL (types and names missing).
- [ ] **Step 3: Implement.** The `DebugQueries` registry is a `StableRegistry<DebugQueryFn>`. Like the other registries, it is not exported to the string tables (no dense id ever leaves the sim). `fl_cli query` reuses `inspect`'s source handling. For a replay, that's `replay_app`, then a `ReplayDriver` over `fl_sim::replay_inputs`, which remaps recorded command ids by name (R73/I1), as `fl_cli/src/agent.rs` does since the final M0 fix wave.
- [ ] **Step 4: Regenerate protected snapshots.** Run the tests to write `formats__protocol_formats.snap.new` (and the view one if it changes), and run `cargo xtask api` for the three `.txt.new` files. Paste their diffs in the report.
- [ ] **Step 5: Bless (lead)** the `.new` files with the reason `M1 Task 2: core.rotate, core.follow, debug.set_terrain payloads; debug queries; AI seam types`.
- [ ] **Step 6: Run** the step 2 command and `cargo xtask api --check`. Expected: PASS, and `api: 5 crates unchanged`.
- [ ] **Step 7: Gate** `cargo xtask gate task --track contract --track tools`. Expected: last line `gate task: PASS in <n>s`.
- [ ] **Step 8: Commit** `feat(contract): core.rotate/core.follow/debug.set_terrain payloads, SimQuery::Debug registry, PlayerView and AiController seam, fl_cli query` with trailers `Refs: ADR-0007, ADR-0015, ADR-0016` and `Blessed: …`.

### Task 3: Terrain content format — `MoveClass`, `TerrainDef`, PNG map layers, sidecar hook

**Track:** contract + content + sim-core · **Refs:** ADR-0014 · **Lane 1**

**Files:**
- Modify:
  - `Cargo.toml` (`png = "0.18"` in `[workspace.dependencies]`), `Cargo.lock` (R50)
  - `crates/fl_content/Cargo.toml` (`png.workspace = true`; dev-dependency `tempfile.workspace = true`)
  - `crates/fl_content/src/{builtin,kinds,mods,registry,lib}.rs`
  - `crates/fl_sim/src/{units,lib}.rs` (re-export `MoveClass`; its `SimHash` impl stays in `fl_sim`)
  - `xtask/src/arch.rs`: a direct `png` dependency is allowed only in `fl_content` (the `Content` role), with a unit test (ADR-0014 Confirmation)
- Create: `crates/fl_content/src/{terrain,layers}.rs`, `crates/fl_content/tests/terrain.rs` (the test writes its mods into temp dirs; no binary fixtures are committed)
- Blessed: `xtask/snapshots/api/{fl_content,fl_sim}.txt` (`fl_content` is an API crate since the final M0 fix wave); the golden replay (base map shape and rules hash change: re-record `testbed_wander.replay.new`)

**Interfaces:**
- Consumes: `ContentKind`, `KindTable`, `LinkCtx`, `load_dirs`, `MapDef`.
- Produces:
```rust
// fl_content/src/terrain.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum MoveClass { Foot, Wheeled, HalfTrack, Tracked }       // moved from fl_sim; fl_sim keeps `pub use fl_content::MoveClass`
impl MoveClass { pub const ALL: [MoveClass; 4]; pub fn index(self) -> usize; }   // declaration order, 0..4
/// `*.terrain.ron`; IN_RULES_HASH = true
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)] #[serde(deny_unknown_fields)]
pub struct TerrainDef { pub id: String, pub name: String, pub pass: Fx, pub blocks: Vec<MoveClass>, pub color: (f32, f32, f32) }
    // link: pass in [0, 1]; blocks has no duplicates
// fl_content/src/layers.rs
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)] #[serde(deny_unknown_fields)]
pub struct MapTerrain { pub height_png: String, pub terrain_png: String, pub height_m: (Fx, Fx), pub legend: Vec<((u8, u8, u8), String)> }
    // file names relative to the map file's directory; height_m = (value at pixel 0, value at pixel max)
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MapLayers { pub cells_x: u32, pub cells_y: u32, pub palette: Vec<String>, pub types: Vec<u8>, pub heights: Vec<u16> }
    // palette: the legend's terrain ids, sorted and unique; types[i] indexes it; index = y * cells_x + x, y north
impl MapLayers { pub fn index(&self, x: u32, y: u32) -> usize; }
pub fn decode_layers(terrain: &MapTerrain, dir: &Path, cells: (u32, u32)) -> Result<MapLayers, String>;
pub fn write_layers(terrain: &MapTerrain, dir: &Path, layers: &MapLayers) -> std::io::Result<()>;   // the inverse: 16-bit gray + RGB8
// builtin.rs: MapDef gains
#[serde(default)] pub terrain: Option<MapTerrain>,
#[serde(skip_deserializing, default)] pub layers: Option<MapLayers>,   // set by load_sidecars; serialized, so the rules hash covers every pixel
// kinds.rs: ContentKind gains
fn load_sidecars(&mut self, _file: &Path) -> Result<(), String> { Ok(()) }   // called by the loader after parsing, before the link pass;
                                                                             //   an Err becomes a ContentError on `_file`
// kinds.rs: RulesSection gains `fn link(&self, _ctx: &mut LinkCtx<'_>) {}`: value-range checks in the link pass, reported at the field like ContentKind::link
//   (Task 5's Movement and Path sections implement it, ADR-0014)
// registry: at most 255 TerrainDefs (a cell type is a u8; TerrainGrid::OPEN = u8::MAX); a 256th is a load error
```
- **Pixel mapping.**
  - Pixel `(px, py)` is cell `(px, cells_y − 1 − py)`.
  - Heights accept 16-bit grayscale, or 8-bit, where a value v counts as v × 257 (grayscale without alpha only).
  - The terrain PNG is 8-bit RGB, or 8-bit RGBA with every alpha 255.
  - Any other format (indexed/`PLTE`, gray with alpha, bit depths below 8, 16-bit RGB/RGBA, a colour height PNG, a gray terrain PNG) is an error naming the file, colour type and bit depth. No palette expansion or bit-depth transformation; samples are raw, and `gAMA`/`sRGB`/`iCCP`/`cHRM` and other ancillary chunks are ignored (ADR-0014).
  - Two legend entries with the same RGB are an error at the second: `terrain.legend[i]`.
  - An image size other than `size_m / cell_m` is an error: `"<file>: expected 250x250 pixels, found 249x250"`.
  - A colour missing from the legend is an error: `"<file>: pixel (12, 3) colour #ff00ff is not in the legend"`.
- **Link checks** report legend ids that are not `TerrainDef`s at `terrain.legend[i]`.

- [ ] **Step 1: Write the failing tests** (`crates/fl_content/tests/terrain.rs`, with a local helper `write_mod(dir, terrains, map_ron, layers)` built on `write_layers`):
```rust
#[test] fn terrain_kind_loads()                      // grass + water kinds, a 4x3-cell map with layers → kind::<TerrainDef>().len() == 2; map.layers is Some
#[test] fn layers_are_north_up()                     // a red pixel at image (1, 0) → types at cell (1, 2) is the red legend entry
#[test] fn layer_size_mismatch_is_error()            // a 4x2 image for a 4x3-cell map → error contains "expected 4x3 pixels, found 4x2"
#[test] fn unknown_legend_colour_is_error()          // pixel #ff00ff → error contains "colour #ff00ff is not in the legend"
#[test] fn legend_naming_unknown_terrain_is_error()  // legend id "t.nope" → link error at "terrain.legend[1]"
#[test] fn heights_accept_16_and_8_bit()             // 16-bit 65535 and 8-bit 255 both decode to 65535; 8-bit 1 → 257
#[test] fn rules_hash_changes_with_one_pixel()       // two otherwise equal mods, one terrain pixel different → rules hashes differ
#[test] fn map_without_terrain_has_no_layers()
#[test] fn layers_cannot_be_written_in_ron()         // a map file with `layers: Some(..)` → parse error naming `layers`
#[test] fn pass_out_of_range_is_error()              // pass 1.5 → error at "pass"
#[test] fn duplicate_blocks_is_error()               // blocks: [Tracked, Tracked] → error at "blocks"
// proptest: random 1..16 × 1..16 layers, write_layers then decode_layers → equal
#[test] fn layers_round_trip()
#[test] fn unsupported_png_formats_are_errors()      // terrain PNG indexed (PLTE), gray+alpha, 4-bit gray, RGB16; height PNG RGB8 or 4-bit gray →
                                                     //   error naming the file, colour type and bit depth
#[test] fn ancillary_colour_chunks_are_ignored()     // a terrain PNG with gAMA, sRGB and iCCP chunks decodes to the same types as without them
#[test] fn duplicate_legend_colour_is_error()        // two legend entries with (255, 0, 0) → error at "terrain.legend[1]"
#[test] fn too_many_terrain_kinds_is_error()         // 256 terrain kinds → error naming the 255 cap
#[test] fn rules_section_link_errors_are_reported()  // a test rules section whose link rejects a value → load error at that field
// xtask/src/arch.rs unit test, with fixture edges like headless_direct_rand_and_physics_deps_flagged:
#[test] fn png_direct_dep_only_in_content()          // png as a direct dependency of fl_content → OK; of fl_sim, fl_cli or fl_client → flagged
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_content --test terrain` and `cargo nextest run --profile agent -p xtask -E 'test(/png_direct/)'`. Expected: FAIL.
- [ ] **Step 3: Implement.** Register `TerrainDef` as a built-in kind next to `MapDef`. `MapDef::load_sidecars` decodes the layers, sizing them from `size_m / cell_m`. The loader calls `load_sidecars` for every kind item, right after parsing.
- [ ] **Step 4: Run** `cargo nextest run --profile agent -p fl_content -p fl_sim -p fl_cli -p fl_catalog`. Expected: PASS, apart from the golden replay test, which fails on the rules hash.
- [ ] **Step 5: Regenerate protected artifacts.** Run `cargo xtask api` (it writes `fl_content.txt.new` and `fl_sim.txt.new`) and `FL_RECORD_GOLDEN=1 cargo nextest run --profile agent -p fl_test_utils --test golden`. The lead blesses them with the reason `M1 Task 3: MoveClass moved to fl_content; MapDef gains terrain layers (rules hash)`.
- [ ] **Step 6: Gate** `cargo xtask gate task --track contract --track content --track sim-core`. Expected: PASS. `cargo deny check` passes too: `png` is MIT/Apache. `cargo xtask arch-check` prints OK: its new `png` rule finds a direct `png` dependency in `fl_content` only, and no `bevy*` crate joins `fl_content`'s or `fl_sim`'s tree.
- [ ] **Step 7: Commit** `feat(fl_content)!: terrain kind, PNG map layers through a sidecar hook, MoveClass in fl_content` with trailers `Refs: ADR-0014` and `Blessed: …`. The controller runs `gate pr` and fast-forwards `main`.

### Task 15: Selection — click, box, Shift-add, double-click, control groups

**Track:** client · **Refs:** spec §10, research §1.9 · **Lane 2** (from `main` at Phase I start; no contract dependency)

**Files:**
- Create: `crates/fl_client/src/selection.rs`, `crates/fl_client/tests/selection.rs`
- Modify:
  - `crates/fl_client/src/lib.rs` (`SelectionPlugin` in `ClientPlugins`; `MeshPickingPlugin` added if absent)
  - `crates/fl_client/src/debug/remote.rs` (`fl.select`, `fl.selection`)
  - `crates/fl_client/tests/remote.rs`

**Interfaces:**
- Consumes: `UnitView`, `ViewIndex`, `SnapshotBuffer`, `LocalPlayer`, `RtsCamera`, `ConsoleState`, `SpawnPanel`.
- Produces:
```rust
#[derive(Resource, Default, Debug, Clone, PartialEq, Eq)] pub struct Selection(pub BTreeSet<SimId>);
#[derive(Resource, Default, Debug)] pub struct ControlGroups(pub [BTreeSet<SimId>; 10]);
#[derive(Resource, Default, Debug, Clone, Copy)] pub struct ClickedUnit { pub primary: Option<SimId>, pub secondary: Option<SimId> }
    // set by the On<Pointer<Click>> observer on unit visual roots (attached by an Added<UnitView> system); cleared in Last
#[derive(Resource, Default, Debug)] pub struct OnScreen(pub Vec<(SimId, Vec2)>);
    // own units' viewport positions; filled lazily (only in frames with a box release or a double-click) via Camera::world_to_viewport
#[derive(Message, Clone, Copy, Debug, PartialEq)]
pub enum SelectInput { Click { sim_id: Option<SimId>, shift: bool, at: Duration }, Box { min: Vec2, max: Vec2, shift: bool },
                       Group { slot: u8, action: GroupAction } }
#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum GroupAction { Assign, Recall, Add, Center }
pub const DOUBLE_CLICK: Duration = Duration::from_millis(350);
pub const DRAG_THRESHOLD_PX: f32 = 6.0;
pub fn box_select(min: Vec2, max: Vec2, points: impl IntoIterator<Item = (SimId, Vec2)>) -> BTreeSet<SimId>;   // corners normalized; edges inclusive
pub fn classify_release(press: Vec2, release: Vec2) -> ReleaseKind;   // pub enum ReleaseKind { Click, Box { min: Vec2, max: Vec2 } }; < DRAG_THRESHOLD_PX → Click
pub struct SelectionPlugin;
// BRP (debug_remote): fl.select { "sim_ids": [..] } replaces the selection (own units only) → {"selected": [..]}; fl.selection → {"selected": [..]}
```
- **Rules.**
  - Only units owned by `LocalPlayer` are selected.
  - `Click(Some(own))`: with Shift, toggle that unit; otherwise, select only it.
  - A second click on the same unit within `DOUBLE_CLICK` selects every on-screen own unit of the same `UnitTypeId` (with Shift, adds them).
  - `Click(None)`: without Shift, clear; with Shift, keep.
  - `Box`: own units inside the box replace the selection, or are added with Shift.
  - Groups:
    - Ctrl+digit assigns (an empty selection clears the group).
    - Digit recalls (only units still in the snapshot).
    - Shift+digit adds the group to the selection.
    - Alt+digit recalls and sets `RtsCamera.focus` to the centroid of the group's visuals.
  - Pruning: units missing from `SnapshotBuffer.curr` leave the selection and the groups. A `LocalPlayer` change clears the selection.
  - Rings: a gizmo circle at every selected unit's `Transform`, radius = half the larger placeholder side + 0.5 m.

- [ ] **Step 1: Write the failing tests** (World level: unit visuals spawned by the fake-runner snapshot path, `ManualDuration` time, `SelectInput` messages written directly, `OnScreen` inserted directly):
```rust
#[test] fn click_selects_own_unit_only()               // click on enemy #6 → empty; click on own #1 → {#1}
#[test] fn shift_click_toggles()                        // {#1} + shift-click #2 → {#1,#2}; shift-click #1 → {#2}
#[test] fn click_empty_ground_clears_unless_shift()
#[test] fn double_click_selects_same_type_on_screen()  // OnScreen: own pz3 #1 #2 #3, own tiger #4, enemy pz3 #6; clicks on #1 at 0 and 200 ms → {#1,#2,#3}
#[test] fn slow_second_click_is_a_single_click()        // second click at 400 ms → {#1}
#[test] fn box_select_inclusive_and_normalized()        // pure: corners (10,10)-(0,0); points (0,0) and (10,5) in, (11,5) out
#[test] fn box_shift_adds()
#[test] fn tiny_drag_is_a_click()                       // classify_release((0,0), (3,4)) == Click; ((0,0), (6,0)) == Box
#[test] fn control_group_assign_recall_add()
#[test] fn control_group_drops_dead_units()             // #2 vanishes from the snapshot → group 1 {#1}, selection {#1}
#[test] fn ctrl_digit_with_empty_selection_clears_group()
#[test] fn alt_digit_centers_camera()                   // group visuals at x 10 and 30 → RtsCamera.focus.x == 20
#[test] fn player_switch_clears_selection()
#[test] fn input_ignored_while_console_open()           // ConsoleState.open + Digit1 press → no SelectInput written
// crates/fl_client/tests/remote.rs (brp_, debug_remote)
#[test] fn brp_select_and_read_selection()              // fl.select {sim_ids:[1,6]} → {"selected":[1]}; fl.selection → [1]
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_client --test selection`. Expected: FAIL.
- [ ] **Step 3: Implement.** Check `Pointer<Click>`, `On<…>`, `MeshPickingPlugin`, `PointerButton` and `Camera::world_to_viewport` with the `bevy-api` skill.
- [ ] **Step 4: Run** `cargo nextest run --profile agent -p fl_client`. Expected: PASS.
- [ ] **Step 5: Gate** `cargo xtask gate task --track client`. Expected: `gate task: PASS`.
- [ ] **Step 6: Commit** `feat(fl_client): selection — click and box picking, Shift-add, double-click same type, control groups, rings`.

---

## Phase II — sim navigation (lane 1); content, input, terrain view, AI host, overlays (lane 2)

### Task 4: Base terrain content, `fl_cli map-gen`, testbed and scale map layers

**Track:** content + tools · **Lane 2** (after Task 3 is merged)

**Files:**
- Create:
  - `assets/base/terrain/{grass,road,field,forest,deep_forest,swamp,water,bridge}.terrain.ron`
  - `assets/base/maps/testbed_1km.{height,terrain}.png`, `assets/base/maps/scale_2km.{height,terrain}.png`
  - `crates/fl_cli/src/map_gen.rs`, `crates/fl_cli/tests/map_gen.rs`
- Modify: `assets/base/maps/{testbed_1km,scale_2km}.map.ron` (add `terrain`), `crates/fl_cli/src/{main,lib}.rs`
- Blessed: the golden replay (the testbed map changes: re-record).

**Interfaces:**
- Consumes: `TerrainDef`, `MapTerrain`, `MapLayers`, `write_layers` (Task 3); `fl_math::Pcg32` (tools may use it).
- Produces:
```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)] pub enum MapPreset { Testbed, Scale }
pub fn generate(preset: MapPreset) -> (MapTerrain, MapLayers);   // pure and deterministic
// `fl_cli map-gen <testbed|scale> --out <dir> [--json]` writes both PNGs; --json prints {"files": [...], "cells": [x, y]}
```
- **Terrain values** (documented guesses from research §1.8; each file carries a `// guess` comment):

  | id | pass | blocks | colour |
  |---|---|---|---|
  | `base.grass` | 0.8 | — | (0.33, 0.45, 0.20) |
  | `base.road` | 1.0 | — | (0.45, 0.40, 0.32) |
  | `base.field` | 0.5 | — | (0.42, 0.33, 0.20) |
  | `base.forest` | 0.7 | — | (0.15, 0.32, 0.12) |
  | `base.deep_forest` | 0.6 | Wheeled, HalfTrack, Tracked | (0.08, 0.22, 0.08) |
  | `base.swamp` | 0.0 | all four | (0.30, 0.35, 0.25) |
  | `base.water` | 0.0 | all four | (0.15, 0.30, 0.55) |
  | `base.bridge` | 1.0 | — | (0.50, 0.42, 0.30) |

- **Testbed layout** (250 × 250 cells of 4 m, height range (−4, 16) m). All rectangles are inclusive cell ranges `x0..=x1, y0..=y1`. Ground is grass at height 0. Terrain is painted in this order, a later layer overwriting an earlier one: forest, deep forest, field, swamp, roads, river, bridges. So the N–S roads stop at the river and the bridge cells carry them across, as `testbed_layout_matches_plan` expects at (71, 124). Heights come from the hill, the ridge and the river (−2 m), independently of the terrain types, except that bridge cells stay at 0 m.

  | Feature | Cells |
  |---|---|
  | River (water, height −2 m) | full width, y 122..=127 |
  | Bridges (bridge, height 0) | x 70..=72 and x 180..=182, over the river rows |
  | N–S roads | x 70..=72 and x 180..=182, full height |
  | E–W roads | y 60..=62 and y 188..=190, full width |
  | Forest | 20..=50, 140..=170 and 200..=230, 150..=175 |
  | Deep forest | 100..=130, 30..=55 |
  | Field | 130..=160, 200..=230 |
  | Swamp | 210..=235, 60..=80 |
  | Hill | centre (60, 200), radius 25 cells, 12 m peak with a cosine falloff in integer math |
  | Ridge | x 140..=143, y 140..=170, a 6 m step (steep for wheeled vehicles) |

  Every testbed scenario unit stays on grass.
- **Scale layout** (500 × 500 cells, 2 km):
  - Two E–W rivers 4 cells wide, at y 160..=163 and y 330..=333.
  - Bridges 3 cells wide every 62 cells along x, starting at x 30.
  - Forest blocks of 8 × 8 cells at 120 positions drawn by `Pcg32::seed(1)`, skipping river rows.
  - Height 0 everywhere.

- [ ] **Step 1: Write the failing tests** (`crates/fl_cli/tests/map_gen.rs`):
```rust
#[test] fn testbed_layers_are_generated()            // generate(Testbed) into a temp dir == the committed PNG bytes, both files
#[test] fn scale_layers_are_generated()
#[test] fn testbed_layout_matches_plan()             // types at (71,124) bridge, (10,124) water, (25,150) forest, (110,40) deep_forest, (0,0) grass
#[test] fn testbed_scenario_units_on_passable_terrain()   // every base.testbed unit's cell: terrain doesn't block its MobilityDef class
#[test] fn base_content_loads_terrain()              // load_content(catalog, base): 8 terrain kinds; both maps have layers; data-check exit 0
#[test] fn map_gen_cli_json()                        // `fl_cli map-gen testbed --out <tmp> --json` → exit 0, "files" has 2 entries, "cells" == [250, 250]
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_cli --test map_gen`. Expected: FAIL.
- [ ] **Step 3: Implement** `generate` and the subcommand. Then run `cargo run -p fl_cli -- map-gen testbed --out assets/base/maps` and the same for `scale`, and add `terrain: Some(MapTerrain(..))` to both map files (legend: the colours of the table, the ids above).
- [ ] **Step 4: Run** `cargo nextest run --profile agent -p fl_cli -p fl_catalog` and `cargo run -p fl_cli -- data-check assets/base`. Expected: PASS; data-check exit 0, `10 units`.
- [ ] **Step 5: Re-record the golden replay** (`FL_RECORD_GOLDEN=1`). The lead blesses it: `M1 Task 4: testbed map gains terrain layers (rules hash)`.
- [ ] **Step 6: Gate** `cargo xtask gate task --track content --track tools`. Expected: PASS.
- [ ] **Step 7: Commit** `feat(content): base terrain kinds, generated testbed and scale map layers, fl_cli map-gen` with trailer `Blessed: …`.

### Task 5: Sim terrain — `TerrainGrid`, `NavGrid`, speed formula, class blocking, `debug.set_terrain`, ASCII fixtures

**Track:** contract + sim-core + content + tools · **Refs:** ADR-0014, ADR-0006 (M1 refinement: R78 `fill_missing`) · **Lane 1**

**Files:**
- Create:
  - `crates/fl_sim/src/terrain.rs`, `crates/fl_sim/src/nav/{mod,grid}.rs`, `crates/fl_sim/tests/terrain.rs`
  - `crates/fl_test_utils/src/terrain.rs`, `crates/fl_test_utils/tests/m0_save.rs`
  - `crates/fl_test_utils/tests/replays/m0_testbed.save.new`: the M0 save fixture (R78). It is protected, so the lead blesses it into `m0_testbed.save`.
- Modify:
  - `crates/fl_sim/src/{lib,scenario,scale,spawn,movement,units,debug_cmds,commands,build}.rs` (`lib.rs` declares `pub mod nav` and `mod terrain`; `build.rs` holds `SIM_VERSION`)
  - `crates/fl_sim/src/classify.rs` (`Codec::fill_missing`), `crates/fl_sim/src/save/load.rs` (calls the fills), `crates/fl_sim/tests/save.rs` (fill tests). These are contract paths (R78, ADR-0006 M1 refinement).
  - `crates/fl_sim/src/{feature,spatial}.rs`: `CoreFeature::build` registers core's one rebuild, `rebuild_core`. The `.register_rebuild(rebuild_spatial)` call moves out of `spatial::build`, and `rebuild_spatial` stays a plain function that `rebuild_core` calls (R74).
  - `crates/fl_sim/Cargo.toml` (`[dev-dependencies] fl_test_utils.workspace = true`), `Cargo.lock`
  - `crates/fl_test_utils/src/{lib,builder}.rs`, `crates/fl_test_utils/Cargo.toml` (`tempfile.workspace = true` in `[dependencies]`; it has no `tempfile` today)
  - the `rules.ron` of `assets/base`, `crates/fl_sim/tests/fixtures/{core_mod,core_mod_extra,core_mod_factions,absurd_speed}` and `crates/fl_cli/tests/fixtures/bad_mod`
- Blessed: system order, `crates/fl_test_utils/tests/snapshots/formats__save_codecs.snap` (`core.terrain@1`), API (`fl_sim`), golden replay, `crates/fl_test_utils/tests/replays/m0_testbed.save`.

**Interfaces:**
- Consumes: `MapDef.layers`, `TerrainDef`, `MoveClass` (Task 3); `register_debug_query` (Task 2); `DebugSetTerrainCmd` (Task 2).
- Produces:
```rust
// R78: filling state that an older save lacks (classify.rs, save/load.rs)
impl<T> Codec<T> { pub fn fill_missing(self, fill: fn(&mut World) -> Result<(), String>) -> Self; }
    // load_world calls `fill` when the save lacks this Hashed (or Derived) state name AND the save's version of the owning feature is older than
    // the build's (MatchHeader.features). Fills run after every saved value is restored, before the Derived rebuilds, in feature load order,
    // then by state name. An Err is LoadError::Codec { name, message }.
    // Without a fill, a missing name behaves as it does today (save/load.rs step 5 removes a missing Hashed resource; R78 says "LoadError",
    // but the code removes it, and saves without a map rely on that). A save of the same feature version never fills.
// CoreFeature::version() becomes 2 here (stays 2 through M1), so an M0 save is an older core save and its fills run
// core.terrain fill: TerrainGrid from the content's MapDef whose size_m and cell_m equal the restored MapInfo (saves carry no map id):
//   exactly one such map with layers → TerrainGrid::from_layers; none with layers → TerrainGrid::open(MapInfo cells);
//   two or more → Err("the save's map matches several maps: <ids>"); no MapInfo → no TerrainGrid (as a live match without a map)
// rules sections registered by core, with every field later tasks read (values in assets/base/rules.ron, marked "guess"):
//   "Movement": MovementRules { min_speed_frac: Fx (0.05), turn_in_place_deg: Fx (30), arrive_m: Fx (0.5), formation_max_spread_m: Fx (40),
//               follow_distance_m: Fx (8), follow_repath_ticks: u32 (20), max_slope_m: BTreeMap<MoveClass, Fx> {Foot 100, Wheeled 1.5, HalfTrack 2, Tracked 2.5},
//               path_terrain_factor: BTreeMap<MoveClass, Fx> {Foot 1.0, Wheeled 0.2, HalfTrack 0.45, Tracked 0.5} }
//   "Path":     PathRules { cluster_cells: u32 (16), budget_per_tick: u32 (8000), max_clearance: u8 (3), snap_radius_cells: u32 (8),
//               flow_min_group: u32 (6), flow_cache_max: u32 (32) }
//   "Steering": SteeringRules { separation_gain: Fx (0.5), separation_margin_m: Fx (0.5), idle_every: u32 (5), max_push_frac: Fx (0.5) }
//   MovementRules and PathRules implement RulesSection::link (Task 3), ranges per ADR-0014: min_speed_frac in (0, 1] and ≥ COST_ROAD / u16::MAX;
//   max_slope_m and path_terrain_factor each cover MoveClass::ALL; slopes ≥ 0; path_terrain_factor in [−1, 1]; cluster_cells, max_clearance,
//   budget_per_tick, flow_min_group, flow_cache_max and idle_every ≥ 1
// terrain.rs
#[derive(Resource, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerrainGrid { pub cells_x: u32, pub cells_y: u32, pub palette: Vec<String>, pub types: Vec<u8>, pub heights: Vec<u16>, pub height_m: (Fx, Fx) }
    // Hashed "core.terrain" v1 (palette hashed as names); empty types/heights = every cell OPEN at height_m.0
impl TerrainGrid {
    pub const OPEN: u8 = u8::MAX;                                      // no terrain kind: pass 1, blocks nothing
    pub fn open(cells_x: u32, cells_y: u32) -> Self;
    pub fn from_layers(layers: &MapLayers, height_m: (Fx, Fx)) -> Self;
    pub fn terrain_at(&self, x: u32, y: u32) -> Option<&str>;
    pub fn height_at(&self, x: u32, y: u32) -> Fx;                     // lo + (hi − lo) × v / 65535
    pub fn set_rect(&mut self, rect: CellRect, terrain: &str);         // new names are appended to the palette (existing indices never move)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)] pub struct CellRect { pub min: (u32, u32), pub max: (u32, u32) }   // inclusive
pub(crate) fn init_terrain(world: &mut World, map: Option<&MapDef>);   // next to every init_spatial call (setup_map, setup_scenario_def, scale populate)
// nav/grid.rs
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)] pub struct MoveProfile { pub class: MoveClass, pub size: u8 }
pub fn profile_of(mobility: &Mobility, footprint: Option<&Footprint>, cell: Fx, max_clearance: u8) -> MoveProfile;   // size = ceil(width / cell) in 1..=max
pub fn content_profiles(content: &ContentDb, cell: Fx, max_clearance: u8) -> Vec<MoveProfile>;   // sorted, unique, over units with Mobility
pub const COST_ROAD: u16 = 100;
#[derive(Resource, Clone, Debug)] pub struct NavGrid { .. }   // Derived "core.nav_grid", rebuilt by rebuild_core → rebuild_nav
impl PartialEq for NavGrid { .. }  impl Eq for NavGrid {}   // by hand: compares every field except `version` (observe-only), so a
                                                                // loaded or freshly built grid equals the live one
impl NavGrid {
    pub fn build(terrain: &TerrainGrid, content: &ContentDb, rules: &MovementRules, profiles: &[MoveProfile], max_clearance: u8) -> Self;
    pub fn cells(&self) -> (u32, u32);
    pub fn cell_of(&self, pos: FxVec2) -> (u32, u32);                 // clamped into the grid
    pub fn blocked(&self, class: MoveClass, x: u32, y: u32) -> bool;
    pub fn clearance(&self, class: MoveClass, x: u32, y: u32) -> u8;
    pub fn passable(&self, p: MoveProfile, x: i64, y: i64) -> bool;    // (x, y) is the unit's centre cell; false off-map; clearance at the anchor
                                                                       //   (x − (size − 1) / 2, y − (size − 1) / 2) >= p.size (ADR-0014; the anchor is (x, y) for size ≤ 2)
    pub fn cost(&self, class: MoveClass, x: u32, y: u32) -> u16;       // ceil(COST_ROAD / factor), factor = clamp(pass + (1 − pass) × path_terrain_factor[class], min_speed_frac, 1);
                                                                       //   Fx ops in that order; the ceiling on raw integers: ((COST_ROAD << 32) + factor.raw − 1) / factor.raw
    pub fn speed_factor(&self, terrain_factor: Fx, x: u32, y: u32) -> Fx;   // clamp(pass + (1 − pass) × terrain_factor, min_speed_frac, 1)
    pub fn update_rect(&mut self, terrain: &TerrainGrid, content: &ContentDb, rules: &MovementRules, rect: CellRect) -> CellRect;
        // recomputes blocking for rect grown by 1 (slope), clearance for that rect grown by max_clearance − 1 toward −x/−y; returns the changed rect
    pub fn profiles(&self) -> &[MoveProfile];
    pub fn version(&self) -> u64;                                      // +1 per update; 0 after a rebuild (observe-only)
}
pub fn rebuild_nav(world: &mut World);                                // NavGrid now; HpaGraphs (Task 7) and FlowCache (Task 10) join it
pub(crate) fn rebuild_core(world: &mut World);                        // core's one Derived rebuild (R74): rebuild_spatial, then rebuild_nav;
                                                                      //   registered once, in CoreFeature::build
pub(crate) fn on_terrain_changed(world: &mut World, rect: CellRect);  // the single hook Tasks 7, 8 and 10 extend
// RejectReason::Feature("core.invalid_target"): code "core.invalid_target", no new variant (R71);
// SpawnError::BlockedCell ("the position is blocked for this unit")
// SIM_VERSION = 2
// debug queries: "core.passability" args {"class":"Tracked","size":1} or {"unit":7} → {"class","size","cells":[x,y],"version":n,"blocked":[[start,len],..]}
//   (row-major run lengths of impassable cells); "core.terrain_at" args {"cell":[x,y]} → {"cell":[x,y],"terrain":id|null,"height_raw":i64,
//   "blocked":{"Foot":bool,..},"clearance":{..},"cost":{..}}
// journal "core.terrain_changed" { min: (u32,u32), max: (u32,u32), terrain: String }; counter "nav.cells_updated"
// fl_test_utils/src/terrain.rs
pub fn ascii_terrain_content(rows: &[&str], cell_m: i32) -> Arc<ContentDb>;
pub fn ascii_terrain_content_with(rows: &[&str], cell_m: i32, rules_ron: Option<&str>) -> Arc<ContentDb>;   // rules default: assets/base/rules.ron
    // legend: '.' grass 0.8, 'r' road 1.0, 'f' forest 0.7, 'F' deep forest (blocks vehicles), '~' water (blocks all), '=' bridge 1.0,
    //   's' swamp (blocks all), '1'..'9' grass at that height in metres; row 0 is the north edge
    // units: "t.tank" (Tracked, 36 km/h, factor 0.5, width 3.0), "t.truck" (Wheeled, 72 km/h, factor 0.2, width 2.4),
    //   "t.rifleman" (Foot, 7.2 km/h, factor 1.0, width 0.6), "t.wide_tank" (Tracked, 36 km/h, factor 0.5, width 7.0); factions "t.red", "t.blue"
impl ScenarioBuilder { pub fn ascii_map(self, rows: &[&str], cell_m: i32) -> Self; pub fn ascii_rules(self, rules_ron: &str) -> Self; }   // map "t.ascii"
```
- **`MobilityDef` validation** widens `terrain_factor` to [−1, 1] (ADR-0014).
- **Movement.** `move_system` multiplies the step by `speed_factor` of the cell under the unit. It never steps into a cell that isn't passable for the unit's profile; such a step is skipped and counts `move.blocked`.
- **Spawns.**
  - `check_spawn` rejects a unit with `Mobility` on a cell blocked for its profile with `SpawnError::BlockedCell`. That covers setup and the funnel.
  - The `debug.spawn` handler checks the same cell first and rejects with `core.invalid_target`, so its documented rule "an applied command always spawns" still holds.
  - `debug.teleport` onto a blocked cell is rejected with `core.invalid_target`.
- **`debug.set_terrain`.** The handler rejects:
  - an unknown terrain id with `UnknownContent` (`core.unknown_content`)
  - `min > max`, or a rect outside the map, with `core.invalid_target`

  Otherwise it sets `TerrainGrid`, calls `on_terrain_changed` (which runs `NavGrid::update_rect`), counts `nav.cells_updated`, and journals `core.terrain_changed`.

- [ ] **Step 0: Generate the M0 save fixture (R78), before any Task 5 change.**
  1. In a temporary worktree of the M0 tip of `main` (`git -C C:\fl\a worktree add <scratch>/m0main main`), run `cargo run -p fl_cli -- save base.testbed --tick 40 --out <scratch>/m0_testbed.save --json`.
  2. Copy it to `crates/fl_test_utils/tests/replays/m0_testbed.save.new`, and record the `hash`, `tick`, `entities` JSON and the `main` commit in the report.
  3. Remove the worktree (`git worktree remove`).

  The implementer never writes the protected `m0_testbed.save`; the lead blesses the `.new` with `M1 Task 5: M0 testbed save fixture (R78)`.
- [ ] **Step 1: Write the failing tests** (`crates/fl_sim/tests/terrain.rs`; hash interval 1):
```rust
#[test] fn speed_formula_matches_spec()        // exact Fx: (pass, factor) → (1.0, 0.5) 1; (0.8, 0.5) 9/10; (0.7, 0.2) 19/25; (0.5, 1.0) 1; (0.0, 0.0) clamped to the RON-parsed Movement.min_speed_frac (0.05 → raw 214748365; never Fx::from_ratio(1, 20), which truncates to 214748364); (0.8, −0.18) 191/250
#[test] fn nav_cost_is_integer_ceiling()       // cost: a road cell → 100; pass 0.5 with Tracked path factor 0.5 → 134; with Wheeled 0.2 → 167
#[test] fn movement_rules_ranges_are_validated() // min_speed_frac 0, 1.5 or below 100/65535; max_slope_m or path_terrain_factor missing a class;
                                               //   path_terrain_factor 1.5; MobilityDef terrain_factor −1.5 → load errors at the field
#[test] fn path_rules_ranges_are_validated()   // cluster_cells 0, max_clearance 0, idle_every 0 → load errors at the field
#[test] fn profile_anchor_is_centred()         // pure NavGrid: a size-3 profile at centre cell (5, 5) is passable iff clearance(4, 4) ≥ 3;
                                               //   sizes 1 and 2 anchor at the centre cell
#[test] fn truck_slows_down_off_road()         // spec §12.4: t.truck 40 ticks on "rrrr…" vs "...." rows → distance ratio in [0.83, 0.85] (0.8 + 0.2 × 0.2 = 0.84)
#[test] fn water_blocks_every_class()
#[test] fn deep_forest_blocks_vehicles_not_foot()
#[test] fn bridge_cells_pass_over_water()
#[test] fn steep_slope_blocks_wheeled_not_foot()   // rows "..9.." : t.truck profile blocked at the step, t.rifleman not
#[test] fn clearance_blocks_wide_units_in_narrow_gaps()   // a 1-cell gap in a water wall: t.tank (size 1) passable, t.wide_tank (size 2 at 4 m) not
#[test] fn units_stop_at_blocked_cells()       // a straight move into water: the unit stops at the shore; "move.blocked" counted; never on water
#[test] fn spawn_on_blocked_cell_is_rejected() // debug.spawn on '~' → rejected, code "core.invalid_target", no unit; a scenario unit on '~' →
                                               //   SetupError::Unit { error: SpawnError::BlockedCell, .. }
#[test] fn teleport_into_water_is_rejected()   // code "core.invalid_target"; gameplay state unchanged; LastSeq advanced (R45)
#[test] fn set_terrain_blocks_a_bridge()       // '=' cells set to "t.water" → passable false for every profile; journal "core.terrain_changed"
#[test] fn set_terrain_rejects_bad_input()     // unknown terrain → "core.unknown_content"; min > max or outside → "core.invalid_target";
                                               //   state unchanged (R45)
#[test] fn set_terrain_needs_cheats()          // cheats off → "core.debug_disabled"
// proptest: random 32×32 ASCII maps; 1..8 random set_terrain rects → NavGrid after update_rect == NavGrid::build from scratch
#[test] fn incremental_nav_equals_full_rebuild()
#[test] fn save_right_after_set_terrain()      // save after the applying tick; load → TerrainGrid and NavGrid equal to the live ones (NavGrid's
                                               //   PartialEq ignores `version`); branch hashes equal to tick +50
#[test] fn core_registers_one_rebuild()        // the core feature's rebuild runs once per load and leaves SpatialGrid and NavGrid equal to the live ones
// crates/fl_sim/tests/save.rs (R78; test features "aaa" v1 → v2 as save_of_an_older_feature_version_loads does)
#[test] fn fill_missing_runs_for_an_older_feature_save()   // v2 adds Hashed "aaa.extra" with a fill → a v1 save loads with the filled value
#[test] fn fill_missing_skipped_for_the_same_version()      // a v2 save lacking "aaa.extra" → the fill does not run (M0 behaviour)
#[test] fn fill_error_is_a_codec_load_error()               // a fill returning Err("x") → LoadError::Codec { name: "aaa.extra", message: "x" }
// crates/fl_test_utils/tests/m0_save.rs (base content, fl_catalog::features())
#[test] fn m0_testbed_save_loads_under_m1()    // load m0_testbed.save → Ok; TerrainGrid == from_layers(base.testbed_1km); NavGrid == a fresh build;
                                               //   tick 40 kept; runs 200 ticks without panic; two loads give equal hash streams
#[test] fn m0_save_round_trips_after_load()    // load, save at once, load again → equal state hash (fills ran once; the new save is core v2)
#[test] fn open_map_moves_at_road_speed()      // core_mod flat map (no layers): tank moves exactly Mobility.speed per tick (M0 behaviour)
#[test] fn terrain_debug_queries()             // "core.passability" for t.tank on "~~..": runs [[0,2]] in row 0; "core.terrain_at" names the terrain
#[test] fn smoke_terrain_twin_and_shuffle()    // assert_deterministic + assert_shuffle_invariant on an ASCII river scenario with moves, 200 ticks
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_sim --test terrain --test save` and `cargo nextest run --profile agent -p fl_test_utils --test m0_save`. Expected: FAIL.
- [ ] **Step 3: Implement.** `NavGrid` stores, per class, a blocked bit and a clearance byte per cell, plus a cost `u16` per cell and per class, and the pass `Fx` per palette entry. Clearance uses the recurrence `c = 0` if blocked, else `1 + min(c(x+1,y), c(x,y+1), c(x+1,y+1))`, capped at `max_clearance`, with cells off the map counting 0. `CoreFeature` registers the queries and `rebuild_core`. Classify `TerrainGrid` as `Class::hashed(1)` and `NavGrid` as `Class::Derived`.
- [ ] **Step 4: Run** `cargo nextest run --profile agent -p fl_sim -p fl_test_utils -p fl_cli`. Expected: PASS, apart from the protected snapshots and the golden.
- [ ] **Step 5: Regenerate** system order, save formats, API and the golden as `.new`. The lead blesses them with the reason `M1 Task 5: Hashed terrain grid, derived NavGrid, SIM_VERSION 2`.
- [ ] **Step 6: Gate** `cargo xtask gate task --track contract --track sim-core --track content --track tools`. Expected: PASS.
- [ ] **Step 7: Commit** `feat(fl_sim): terrain grid, NavGrid with class blocking and clearance, speed formula, debug.set_terrain; fill_missing loads M0 saves` with trailers `Refs: ADR-0014, ADR-0006` and `Blessed: …`.

### Task 6: Grid A*, line of sight, smoothing, nearest passable cell

**Track:** sim-core · **Refs:** ADR-0015 · **Lane 1**

**Files:**
- Create: `crates/fl_sim/src/nav/{astar,los}.rs`, `crates/fl_sim/tests/astar.rs`
- Modify: `crates/fl_sim/src/nav/mod.rs` (`pub mod astar; pub mod los;` and re-exports), `crates/fl_sim/src/lib.rs` (re-exports only: Task 5 already declared `pub mod nav`)
- Blessed: API.

**Interfaces:**
- Consumes: `NavGrid`, `MoveProfile`, `CellRect`, `COST_ROAD` (Task 5).
- Produces:
```rust
pub type CellXY = (u32, u32);
#[derive(Default, Debug, Clone, Copy)] pub struct SearchStats { pub expanded: u32 }
pub fn edge_cost(nav: &NavGrid, class: MoveClass, a: CellXY, b: CellXY) -> u32;   // orthogonal (ca + cb + 1) / 2; diagonal that × 141 / 100
pub fn octile(a: CellXY, b: CellXY) -> u32;                                      // COST_ROAD × (dx + dy) − 59 × min(dx, dy)
pub fn astar(nav: &NavGrid, p: MoveProfile, start: CellXY, goal: CellXY, bounds: CellRect, stats: &mut SearchStats) -> Option<(Vec<CellXY>, u32)>;
    // 8-connected, no corner cutting (a diagonal needs both orthogonal cells passable), inside bounds;
    // pops the smallest (f, h, y, x); a parent changes only on a strictly lower g; returns cells start..=goal and the cost
pub struct DistanceField { .. }
impl DistanceField { pub fn cost(&self, c: CellXY) -> Option<u32>; pub fn path_to(&self, c: CellXY) -> Option<Vec<CellXY>>; }   // path from the source
pub fn dijkstra_from(nav: &NavGrid, p: MoveProfile, source: CellXY, bounds: CellRect, stats: &mut SearchStats) -> DistanceField;
pub fn line_clear(nav: &NavGrid, p: MoveProfile, from: FxVec2, to: FxVec2, cells_checked: &mut u32) -> bool;
    // supercover walk of the segment: every touched cell (corner touches count both neighbours) is passable
pub fn smooth(nav: &NavGrid, p: MoveProfile, start: FxVec2, cells: &[CellXY], goal: FxVec2, cells_checked: &mut u32) -> Vec<FxVec2>;
    // string pulling over cell centres; ends exactly at `goal`
pub fn nearest_passable(nav: &NavGrid, p: MoveProfile, cell: CellXY, radius: u32) -> Option<CellXY>;
    // Chebyshev rings 0..=radius; within a ring the smallest (dx² + dy², y, x)
pub fn cell_centre(nav: &NavGrid, c: CellXY) -> FxVec2;
```

- [ ] **Step 1: Write the failing tests** (`crates/fl_sim/tests/astar.rs`, maps from `ascii_terrain_content` with `NavGrid::build`):
```rust
#[test] fn astar_open_grid_pinned()          // 4×2 road map, (0,0) → (3,1): path [(0,0),(1,1),(2,1),(3,1)], cost 341
#[test] fn no_corner_cutting()               // water at (1,0) and (0,1): (0,0) → (1,1) is not a direct diagonal
#[test] fn astar_respects_bounds()           // goal outside bounds → None
#[test] fn unreachable_goal_expands_reachable_region()   // goal inside a closed lake ring → None; stats.expanded == the start region's cell count
#[test] fn line_clear_detects_water_and_corners()        // a segment through a water cell's corner → false
#[test] fn nearest_passable_ring_order()     // pinned: from a water cell with shores at (+2,0) and (0,−2) → the y-smaller one first
// proptests (256 cases; 24×24 maps with random water rectangles; random passable pairs):
#[test] fn astar_matches_dijkstra_cost()     // astar cost == dijkstra_from cost; path contiguous, every cell passable, no corner cut
#[test] fn smooth_keeps_path_valid()         // every consecutive corner pair line_clear; last corner == goal
#[test] fn astar_is_deterministic()          // two runs return equal paths and stats
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_sim --test astar`. Expected: FAIL.
- [ ] **Step 3: Implement** with a `BinaryHeap` of `Reverse((f, h, y, x))` and flat `Vec` g and parent buffers sized to the bounds. No hash maps.
- [ ] **Step 4: Run** `cargo nextest run --profile agent -p fl_sim`. Expected: PASS (API `.new` pending).
- [ ] **Step 5: Bless (lead)** API with the reason `M1 Task 6: nav A*, LOS, smoothing`.
- [ ] **Step 6: Gate** `cargo xtask gate task --track sim-core`. Expected: PASS.
- [ ] **Step 7: Commit** `feat(fl_sim): grid A* with deterministic ties, supercover line of sight, string-pulling, nearest passable cell` with trailer `Blessed: …`.

### Task 7: HPA* — clusters, entrances, search, refinement, cluster invalidation

**Track:** sim-core · **Refs:** ADR-0015 · **Lane 1**

**Files:**
- Create: `crates/fl_sim/src/nav/hpa.rs`, `crates/fl_sim/tests/hpa.rs`
- Modify: `crates/fl_sim/src/nav/{mod,grid}.rs` (`rebuild_nav` builds `HpaGraphs` after `NavGrid`; `on_terrain_changed` invalidates), `crates/fl_sim/src/lib.rs`
- Blessed: API. No save format changes: `HpaGraphs` is `Derived` and has no codec.

**Interfaces:**
- Consumes: Task 6's functions; `PathRules.{cluster_cells, snap_radius_cells}`.
- Produces:
```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)] pub struct ClusterId(pub u32);   // cy × clusters_x + cx
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)] pub struct HpaGraphs { .. }   // Derived "core.nav_hpa"; rebuilt inside rebuild_nav, after NavGrid
                                                                                          //   (core's one rebuild, R74: no rebuild of its own)
impl HpaGraphs {
    pub fn build(nav: &NavGrid, cluster_cells: u32) -> Self;            // one HpaGraph per nav.profiles()
    pub fn graph(&self, p: MoveProfile) -> Option<&HpaGraph>;
    pub fn invalidate(&mut self, nav: &NavGrid, rect: CellRect) -> Vec<ClusterId>;
        // rebuilds clusters intersecting rect grown by 1 cell, and recomputes the intra edges of their 4-neighbours; returns the rebuilt ids, sorted
}
impl HpaGraph {
    pub fn cluster_of(&self, c: CellXY) -> ClusterId;  pub fn cluster_rect(&self, id: ClusterId) -> CellRect;
    pub fn entrances(&self, id: ClusterId) -> &[CellXY];  pub fn node_count(&self) -> usize;  pub fn clusters(&self) -> (u32, u32);
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)] pub enum PlanStatus { Found, Partial, Unreachable }
#[derive(Clone, Debug, PartialEq, Eq)] pub struct PathPlan { pub corners: Vec<FxVec2>, pub cost: u32, pub status: PlanStatus }
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)] pub struct PathWork { pub nodes_expanded: u32, pub los_cells: u32 }
impl PathWork { pub fn charge(&self) -> u32; }                         // nodes_expanded + los_cells: the request's cold cost
pub fn plan_path(nav: &NavGrid, hpa: &HpaGraphs, p: MoveProfile, from: FxVec2, to: FxVec2, rules: &PathRules, work: &mut PathWork) -> PathPlan;
// debug query "core.clusters" args {"class","size"} or {"unit": id} (the unit's profile, as "core.passability" takes them)
//   → {"cluster_cells":16,"clusters":[cx,cy],"entrances":[[x,y],..],"nodes":n}
```
- **Entrance rule.** For each border between adjacent clusters, find the maximal runs of cell pairs (a in A, b in B) where both are passable. A run shorter than 6 gets one transition at index `start + (len − 1) / 2`; a longer run gets two, at both ends. Each transition makes a node in each cluster and an inter edge between them, costed with `edge_cost`. Intra edges come from one `dijkstra_from` per node over its cluster and store their cell paths.
- **`plan_path` steps:**
  1. The goal snaps to `nearest_passable` within `snap_radius_cells`; if there is none, the result is `Unreachable`.
  2. A blocked start snaps the same way.
  3. If `line_clear(from, goal)`, the corners are `[goal]` and the result is `Found`.
  4. Same cluster: in-cluster `astar`, then smooth.
  5. Otherwise insert the start and goal by in-cluster Dijkstra, run A* over the abstract nodes with the octile heuristic and the `(f, h, node index)` tie-break, refine by concatenating the stored cell paths, then smooth.
  6. With no route, plan to the reached abstract node or cell with the smallest octile distance to the goal (ties by node index, then `(y, x)`); the result is `Partial`.

  Every cell or node settled counts in `nodes_expanded`, and every LOS cell in `los_cells`. Each call runs inside the `hpa_search` span, and each rebuilt cluster inside `hpa_cluster_build`.

- [ ] **Step 1: Write the failing tests** (`crates/fl_sim/tests/hpa.rs`):
```rust
#[test] fn entrances_follow_run_rule()          // a 32×16 map, a water wall at x 15/16 with a 5-cell gap → 1 transition (gap index 2); an 8-cell gap → 2 (ends)
#[test] fn direct_line_costs_only_los()          // open map: corners == [goal]; nodes_expanded == 0; los_cells > 0
#[test] fn same_cluster_uses_local_astar()
#[test] fn goal_in_water_snaps_to_nearest_shore()   // last corner == cell_centre(nearest_passable(goal cell))
#[test] fn island_goal_is_partial()              // goal on an island → Partial; last corner is the reached cell nearest the goal
#[test] fn start_on_blocked_cell_recovers()
#[test] fn bridge_removal_reroutes()             // two bridges; set_terrain water over bridge A → invalidate → path crosses bridge B
#[test] fn wide_profile_avoids_narrow_gap()      // t.wide_tank detours around a 1-cell gap t.tank takes
#[test] fn clusters_query_lists_entrances()      // "core.clusters" on a 32×32 open map: clusters [2,2], 12 entrances (one node per border cell; R96); {"unit": id} of a t.tank
                                                 //   answers the same as {"class":"Tracked","size":1}
// proptests (64 cases; 64×64 maps with random water rectangles; cluster 16):
#[test] fn hpa_finds_path_iff_astar_does()       // Found iff a full-map astar finds a path; corners chain line_clear
#[test] fn hpa_cost_near_optimal()               // refined cell-path cost ≤ 1.3 × optimal + 4 × COST_ROAD
#[test] fn invalidate_equals_full_rebuild()      // after random set_terrain rects: invalidated graphs == HpaGraphs::build from scratch
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_sim --test hpa`. Expected: FAIL.
- [ ] **Step 3: Implement.** The abstract graph keeps nodes in a `Vec`, sorted by `(cluster, y, x)`, with adjacency lists sorted by target node.
- [ ] **Step 4: Run** `cargo nextest run --profile agent -p fl_sim`. Expected: PASS.
- [ ] **Step 5: Bless (lead)** API with the reason `M1 Task 7: HPA*`.
- [ ] **Step 6: Gate** `cargo xtask gate task --track sim-core`. Expected: PASS.
- [ ] **Step 7: Commit** `feat(fl_sim): HPA* per movement profile — entrances, abstract search, refinement, cluster invalidation` with trailer `Blessed: …`.

### Task 8: Path requests, order queue and path following

**Track:** sim-core + perf · **Refs:** ADR-0015, ADR-0006, ADR-0007 (input caps), ADR-0010 · **Lane 1**

**Files:**
- Create: `crates/fl_sim/src/{path,orders}.rs`, `crates/fl_sim/tests/pathing.rs`
- Modify:
  - `crates/fl_sim/src/{units,commands,movement,publish,debug_cmds,nav/mod,lib}.rs` (`debug_cmds.rs`: `debug.teleport` clears `MoveOrder.orders` and sets `PathFollow` to `Idle`, in place of clearing `waypoints`)
  - `crates/fl_test_utils/tests/m0_save.rs` (R78 fills for `core.path_queue` and `core.path_follow`)
  - `crates/fl_sim/tests/{movement,save,commands,observe,runner}.rs`: ports from `MoveOrder.waypoints` and from the `core.move_order` journal kind (`observe.rs`, `runner.rs` assert it). List each test-guard finding in the report.
- Blessed: system order, `crates/fl_test_utils/tests/snapshots/formats__save_codecs.snap` (`core.move_order@2 reads 1`, `core.path_follow@1`, `core.path_queue@1`), API, golden, `docs/perf/budgets.ron`, perf counter snapshots.

**Interfaces:**
- Consumes: `plan_path`, `PathWork`, `PlanStatus` (Task 7); `NavGrid::{passable, speed_factor, cell_of}`, `profile_of`, `on_terrain_changed`, `MovementRules`, `PathRules` (Task 5).
- Produces:
```rust
// R78 fills (Codec::fill_missing, Task 5): "core.path_queue" → PathQueue::default() (empty); "core.path_follow" → PathFollow::default() (Idle)
//   on every entity with Mobility, in SimIndex order. They run only for a save of an older core version (the M0 fixture)
// units.rs: MoveOrder v2 (Hashed "core.move_order"; Codec::new(2).migrate_from::<v1::MoveOrder>(1): each v1 waypoint → Order::Move;
//   the format guard requires this migration for the bump, R70; v1 held at most MAX_WAYPOINTS waypoints, so v2 orders stay within the cap)
#[derive(Component, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)] pub struct MoveOrder { pub orders: Vec<Order> }   // orders[0] is current;
                                                                                                      //   len ≤ MAX_WAYPOINTS (R77)
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)] pub enum Order { Move { target: FxVec2 }, Rotate { facing: Angle }, Follow { leader: SimId, offset: FxVec2 } }
    // an interim closed enum of core movement orders until the C7 order queue + activity stack (M2 prelude) replaces it
// path.rs
#[derive(Component, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathFollow { pub state: PathState, pub goal: FxVec2, pub corners: Vec<FxVec2>, pub next: u16, pub planned: Tick }   // Hashed "core.path_follow" v1
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PathState { #[default] Idle, Waiting { request: u32 }, Following, Flow { key: FlowKey }, Done, Failed }   // Flow used from Task 10
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)] pub struct FlowKey { pub cluster: ClusterId, pub profile: MoveProfile }
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathQueue { pub pending: VecDeque<PathRequest>, pub next_id: u32 }   // Hashed "core.path_queue" v1
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathRequest { pub id: u32, pub unit: SimId, pub goal: FxVec2, pub kind: RequestKind, pub issued: Tick }
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)] pub enum RequestKind { Single, Group { units: Vec<(SimId, FxVec2)> } }   // Group from Task 10
pub fn enqueue_path(world: &mut World, unit: SimId, goal: FxVec2) -> u32;   // push back, PathFollow → Waiting, journal "core.path_request"
// systems (core slots): ThinkSet::Orders: orders::advance_orders, then movement::wander_system, chained (wander moves here from ThinkSet::Behaviour;
//   it keeps its "debug.wander" Stream through with_input and draws x then y exactly as in M0, now as an exclusive system
//   fn(InMut<Stream>, &mut World) so it can call enqueue_path; check with_input on exclusive systems with the bevy-api skill);
//   ThinkSet::Behaviour: path::process_path_queue; ActSet::Move: movement::move_system
// journal "core.path_request" { request: u32, goal: FxVec2, profile: MoveProfile, kind: "single"|"group" };
//   "core.path_result" { request: u32, status: "found"|"partial"|"unreachable"|"stale", corners: usize, cost: u32, charge: u32, waited: u32 };
//   "core.order_changed" { orders: usize, current: "move"|"rotate"|"follow"|"none", reason: "command"|"completed"|"failed"|"stopped"|"replanned" }
//   (replaces "core.move_order")
// counters: "path.requests" (processed), "path.queue_len" (after processing), "path.nodes_expanded", "path.los_cells", "path.budget_spent", "move.blocked"
// debug query "core.paths" args {"units":[ids]} → {"<id>": {"state","goal":[xr,yr],"corners":[[xr,yr],..],"next","orders":[{"move":[xr,yr]}|{"rotate":raw}|{"follow":id}],
//   "profile":{"class","size"}}}
```
- **Handlers** (Input). `core.move`:
  - Without queue: the orders become `[Move]`, and the handler calls `enqueue_path` per unit in `SimId` order. Any older request turns stale.
  - With queue: append the order; enqueue only if the unit had no orders. If any unit already holds `MAX_WAYPOINTS` orders, the whole command is rejected with `core.queue_full` and nothing changes (M0's rule, now over `orders`).

  `core.stop` clears the orders, and `PathFollow` returns to `Idle` with no corners.
- **`advance_orders`** walks `SimIndex` in order:
  - It pops every completed current order (`Done`, or a `Failed` move) and journals `order_changed`.
  - When a queued `Move` becomes current, it enqueues a request.
  - It re-requests units whose `PathFollow` returned to `Idle` while a `Move` is current: blocked, or invalidated.
  - Wander (`wander_system`, chained right after): a unit with `Wander` and no orders gets `[Move { drawn target }]` and a request. The draws are as in M0: `rolls(seed, tick, sim_id, "debug.wander" Stream)`, x then y.
- **`process_path_queue`.**
  - Pop requests while `spent < budget_per_tick`, processing at least one per tick.
  - A request is stale, journaled `stale` at charge 0, unless its unit exists and is `Waiting { request: id }`.
  - Otherwise `plan_path` runs from the unit's current pose with its profile. The request is charged `PathWork::charge()`, and the plan is applied at once: `Following` with the corners, or `Failed` for `Unreachable`, with `planned = tick`.
- **`move_system`** (entity-local):
  - `Following`: turn toward `corners[next]` by at most `turn_rate`. While the heading error exceeds `turn_in_place_deg`, turn without moving. Otherwise move `speed × speed_factor`, snapping to a corner within one step, and advancing `next`. Within `arrive_m` of the goal, the state becomes `Done`.
  - Current `Rotate`: turn toward `facing`.
  - A step into an impassable cell is skipped, counts `move.blocked`, and sets `Idle`.
- **Invalidation.** `on_terrain_changed` then invalidates HPA* (Task 7). In `SimIndex` order, every unit `Following` whose remaining segments cross a rebuilt cluster's rect is set to `Idle`, journaled `replanned`.
- **`publish`** sets `flags::MOVING` while `PathFollow` is `Following` or `Flow`.

- [ ] **Step 1: Write the failing tests** (`crates/fl_sim/tests/pathing.rs`):
```rust
#[test] fn tank_paths_around_lake()                 // arrives within 600 ticks; every tick the unit's cell is passable for t.tank
#[test] fn crosses_river_only_by_bridge()           // t.truck crosses "~~~=~~~" rows; some tick's cell is a '=' cell; never a '~' cell
#[test] fn unreachable_goal_goes_to_closest_point()  // island goal: path_result "partial"; final cell == the nearest reachable cell; order popped ("completed")
#[test] fn hull_rotates_before_moving()             // tank heading 0, goal due west: pos unchanged while |heading error| > 30°, then moves
#[test] fn queued_moves_run_in_order()              // Shift-move A then B: reaches A, then B; order_changed reasons "command", "command", "completed", "completed"
#[test] fn stop_clears_orders_and_pending_request() // move, stop next tick before processing (budget 1): result "stale"; unit Idle, no orders
#[test] fn stale_request_is_discarded()            // two moves for the same unit in one tick (seq 0, 1): the first request's result is "stale", charge 0
#[test] fn despawned_unit_request_discarded()
#[test] fn queue_respects_budget_and_order()       // 40 tanks moved at tick 1 with Path.budget_per_tick 300 (ascii_rules): path_result request ids ascending
                                                    //   across ticks; per tick (sum of charges − the last charge) < 300
#[test] fn no_request_starves()                    // every request's waited ≤ ceil(total charge / 300) + 1
#[test] fn bridge_destroyed_mid_route_repaths()    // set_terrain water over bridge A while a truck is en route to it → "replanned"; arrives via bridge B; never on '~'
#[test] fn wander_goes_through_the_queue()         // wander_all on an ASCII map: "core.path_request" journal entries for every wanderer within 50 ticks
#[test] fn branch_with_queued_requests()           // budget 300, 40 movers: assert_branch(n = 2, m = 300) while the queue is non-empty at tick 2
#[test] fn move_order_v1_migrates()                // v1 bytes { waypoints: [a, b] } decode to orders [Move a, Move b]
// crates/fl_sim/tests/commands.rs: queued_waypoints_are_capped (M0) is ported: a queued core.move onto 32 orders → "core.queue_full", nothing changed
// crates/fl_test_utils/tests/m0_save.rs (R78), extended: m0_testbed_save_loads_under_m1 also asserts PathQueue empty, every Mobility unit has
//   PathFollow Idle, v1 MoveOrders migrated; within 50 ticks a moving or wandering unit journals "core.path_request"
#[test] fn path_counters_and_journal()             // "path.requests", "path.nodes_expanded" > 0 on a lake detour, "path.queue_len" while backlogged
#[test] fn paths_debug_query()                     // "core.paths" {"units":[1]} lists state "following" and the corners
#[test] fn smoke_pathing_twin_and_shuffle()        // twin + shuffle on the ASCII river scenario with 6 movers, 300 ticks
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_sim --test pathing`. Expected: FAIL.
- [ ] **Step 3: Implement**, and port the M0 movement tests to `MoveOrder.orders`. Every ported or changed assertion goes in the report for test-guard.
- [ ] **Step 4: Run** `cargo nextest run --profile agent -p fl_sim -p fl_test_utils`. Expected: PASS, apart from the protected artifacts.
- [ ] **Step 5: Regenerate** system order, formats, API, the golden, and `FL_RECORD_BUDGETS=1 cargo nextest run --profile agent -p fl_test_utils --test perf_counters -E 'test(/scale/)'` (new `path.*` counters). The lead blesses them: `M1 Task 8: path queue, MoveOrder v2, wander through the queue`.
- [ ] **Step 6: Gate** `cargo xtask gate task --track sim-core --track perf`. Expected: PASS, with `smoke_counters_scale_500` and `smoke_pathing_twin_and_shuffle` run.
- [ ] **Step 7: Commit** `feat(fl_sim): budgeted path request queue, MoveOrder v2 order queue, path following with hull rotation` with trailers `Refs: ADR-0015` and `Blessed: …`.

### Task 9: `core.rotate`, `core.follow`, Shift-queue semantics, formation offsets

**Track:** sim-core · **Refs:** ADR-0007 (refinement), spec §7 · **Lane 1**

**Files:**
- Modify: `crates/fl_sim/src/{commands,orders,movement,path}.rs`, `crates/fl_test_utils/src/builder.rs` (`Scenario::world_mut`)
- Create: `crates/fl_sim/tests/orders.rs`
- Blessed: system order (if it changes), API, golden.

**Interfaces:**
- Consumes: `RotateCmd`, `FollowCmd` (Task 2); `Order`, `enqueue_path`, `PathFollow` (Task 8); `MovementRules.{formation_max_spread_m, follow_distance_m, follow_repath_ticks}`.
- Produces:
```rust
pub fn formation_targets(positions: &[FxVec2], target: FxVec2, max_spread: Fx, map: &MapInfo) -> Vec<FxVec2>;
    // centroid c = Σ pos / n; offset_i = pos_i − c; R = max |offset_i|; if R > max_spread every offset × max_spread / R; target_i = map.clamp(target + offset_i)
    // Task 10 (R110): core.move then spaces the slots with space_slots(slots, pitch, map), pitch = the largest footprint side of the units +
    //   Steering.separation_margin_m; the spacing wins over max_spread, so a dense group spans about √n × pitch
impl Scenario { pub fn world_mut(&mut self) -> &mut World; }   // fl_test_utils, for between-tick setup_despawn in tests
// "core.rotate": unit_can = Mobility; toward clamped to the map first (R104); facing_i = (toward − from_i).angle() (current heading if equal), from_i = pos_i,
//   or for a queued rotate the target of the last Move already in the unit's orders, if any (R105); not queued → orders [Rotate], PathFollow Idle; queued → append
// queued rotate and follow obey MAX_WAYPOINTS like a queued move: any unit with full orders → the whole command "core.queue_full", nothing changed
// "core.follow": leader must exist with a Pose, else RejectReason::Feature("core.invalid_target"); the leader is removed from the units; offset_i = (pos_i − leader) scaled to
//   length min(|pos_i − leader|, follow_distance_m) (a unit exactly on its leader: follow_distance_m behind the leader's heading);
//   advance_orders re-plans to goal = leader + offset (R106): Idle → at once; Waiting → never; any other state only once
//   tick − planned ≥ follow_repath_ticks, then Following/Flow if |goal − planned goal| > one cell, Done if > Movement.arrive_m,
//   Failed always; a missing leader pops the order ("completed")
// every wire-supplied coordinate in a payload is clamped or validated before any arithmetic (R104); "core.stop" is never queued
// "core.move" with more than one unit uses formation_targets (queued moves too, from the positions at command time)
```

- [ ] **Step 1: Write the failing tests** (`crates/fl_sim/tests/orders.rs`):
```rust
#[test] fn formation_keeps_relative_offsets()    // 3 tanks 10 m apart on an open map, move 300 m: final pairwise offsets within 1 m of the starting ones
#[test] fn formation_spread_is_capped()          // units 200 m apart: |target_i − target| ≤ 40 m for all i
#[test] fn formation_targets_pinned()            // pure: positions (0,0),(10,0) → target (100,50): [(95,50),(105,50)]
#[test] fn rotate_turns_in_place()               // position unchanged; heading reaches facing within ceil(diff / turn_rate) ticks; the order pops
#[test] fn rotate_queued_after_move()            // Shift: move, then rotate → arrives, then faces the point
#[test] fn follow_tracks_moving_leader()         // leader moves 200 m: follower ends within follow_distance + 4 m; path requests ≤ ticks / 20 + 2
#[test] fn follow_ends_when_leader_is_gone()     // setup_despawn(leader) between ticks → the follower's order pops; it stops
#[test] fn follow_self_or_missing_leader_rejected()   // FollowCmd { leader: own SimId } alone → "core.no_valid_units"; a missing leader → "core.invalid_target";
                                                      //   state unchanged, LastSeq advanced
#[test] fn shift_queue_mixed_orders()            // move, rotate, move queued → executed in order; order_changed "completed" three times
#[test] fn queued_orders_share_the_cap()         // 32 queued orders of mixed kinds; a queued rotate or follow → "core.queue_full"; orders unchanged
#[test] fn orders_twin_and_shuffle()
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_sim --test orders`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo nextest run --profile agent -p fl_sim -p fl_test_utils`. Expected: PASS (protected `.new` pending).
- [ ] **Step 5: Regenerate and bless (lead)** the golden (the testbed moves two units, so formation applies) and API: `M1 Task 9: rotate, follow, formation offsets`.
- [ ] **Step 6: Gate** `cargo xtask gate task --track sim-core`. Expected: PASS.
- [ ] **Step 7: Commit** `feat(fl_sim): core.rotate, core.follow, Shift-queue for every order, group moves keep formation offsets` with trailer `Blessed: …`.

### Task 10: Flow fields for group moves

**Track:** sim-core · **Refs:** ADR-0015 · **Lane 1**

**Files:**
- Create: `crates/fl_sim/src/nav/flow.rs`, `crates/fl_sim/tests/flow.rs`
- Modify: `crates/fl_sim/src/{path,commands,movement,orders,build,nav/mod,lib}.rs`
- Blessed: system order, API.

**Interfaces:**
- Consumes: `HpaGraphs`, `FlowKey`, `RequestKind::Group`, `PathState::Flow` (Tasks 7, 8); `PathRules.{flow_min_group, flow_cache_max}`.
- Produces:
```rust
#[derive(Resource, Default)] pub struct FlowCache { .. }   // Derived "core.path_flow_cache" (reset to empty inside rebuild_nav, the last step of
                                                           //   core's one rebuild, R74); outcome-neutral; entries in a BTreeMap<FlowKey, _>
                                                           //   (hash maps are clippy-banned in sim crates)
#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum FlowDir { Toward(CellXY), Arrived }   // Arrived: inside the destination cluster
#[derive(Default, Debug, Clone, Copy)] pub struct FlowWork { pub abstract_nodes: u32, pub cells: u32 }
impl FlowCache {
    pub fn cold_cost(&mut self, key: FlowKey, nav: &NavGrid, hpa: &HpaGraphs, work: &mut FlowWork) -> u32;
        // builds the abstract integration (Dijkstra over the profile's abstract nodes from the destination cluster's entrances) if missing;
        // returns its settled-node count, the same whether built now or cached
    pub fn ensure_tile(&mut self, key: FlowKey, cluster: ClusterId, nav: &NavGrid, hpa: &HpaGraphs, work: &mut FlowWork);
        // the cluster's integration field seeded from its entrances' abstract costs (the destination cluster: every cell 0)
    pub fn direction(&mut self, key: FlowKey, cell: CellXY, nav: &NavGrid, hpa: &HpaGraphs, work: &mut FlowWork) -> Option<FlowDir>;
        // R84: a read never observes a miss. A missing integration or tile is built first (pure; its work counted like any build, never charged
        //   to the path budget, so a thrash with more live keys than flow_cache_max is bounded by flow_cache_max, not by the budget: R112, Task 13
        //   measures it); None means only that the cell has no route
    pub fn clear_profile(&mut self, p: MoveProfile);
    pub fn len(&self) -> usize;   // entries; over flow_cache_max, evict the least recently used (last-used tick, then key); eviction may run at any
                                  //   point, because reads rebuild what it removed (R84)
}
pub struct RunConfig { .., pub flow_cache_cold: bool }   // clears FlowCache at the start of every tick (diagnostic: outcomes must not change)
// "core.move" without queue: per profile, a set of ≥ flow_min_group units → one Group request (key = destination cluster of `target`, units with their
//   formation targets); the rest → Single requests. Enqueue order within the command: Group requests in MoveProfile order, then Single requests
//   in SimId order
// processing a Group: charge = cold_cost (pure); each unit (SimId order) still Waiting on it gets PathFollow { state: Flow { key }, goal: target_i };
//   a unit whose start cell has no route → a Single request at the back (SimId order); journal path_result status "flow";
//   a Group none of whose units exists and still waits on it is stale (charge 0); members that left are skipped
// movement::prepare_flow_tiles (ActSet::Move core, before move_system, SimIndex order): ensure_tile for each Flow unit's current cluster
//   (a warm-up only: move_system reads through direction(), which rebuilds anything evicted since, R84)
// move_system for Flow: Toward(c) → steer to cell_centre(c); Arrived → if line_clear(pos, goal) then Following [goal] else Idle (a Single request follows)
// counters "flowfield.abstract_nodes", "flowfield.cells"; span "flowfield_build"
// on_terrain_changed: clear_profile for every profile whose graph rebuilt a cluster
```

- [ ] **Step 1: Write the failing tests** (`crates/fl_sim/tests/flow.rs`):
```rust
#[test] fn group_move_uses_one_flow_request()    // 8 tanks → exactly one "core.path_request" kind "group" and one path_result "flow"
#[test] fn small_group_uses_single_requests()    // 5 tanks → 5 single requests
#[test] fn mixed_profiles_split()                // 6 tanks + 6 riflemen → 2 group requests
#[test] fn flow_units_reach_their_slots()        // 8 tanks across a river with 2 bridges: all within 2 m of their formation targets by tick 900; never on '~'
#[test] fn cache_does_not_change_outcomes()      // the same 600-tick scenario with RunConfig.flow_cache_cold true and false → equal hash streams
#[test] fn flow_cache_overflow_does_not_change_outcomes()   // R84: ascii_rules flow_cache_max 2; four 6-tank groups to four destination clusters at once
                                                 //   (more concurrent keys than flow_cache_max), 600 ticks: flow_cache_cold true vs false → equal hash streams
#[test] fn branch_mid_flow_follow()              // assert_branch(n at a tick where ≥ 4 units are Flow, m = n + 200)
#[test] fn flow_invalidated_by_terrain_change()  // set_terrain water over the bridge in use → the group arrives via the other bridge; never on '~'
// proptest (64 cases, 48×48 maps): from every cell with a route, following direction() reaches the destination cluster without revisiting a cell;
//   a cell has a direction iff a full-map Dijkstra from the destination cluster reaches it
#[test] fn flow_directions_reach_destination()
#[test] fn flow_counters_recorded()              // "flowfield.abstract_nodes" > 0 on the request tick, "flowfield.cells" > 0 while moving
#[test] fn smoke_flow_twin_and_shuffle()
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_sim --test flow`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo nextest run --profile agent -p fl_sim -p fl_test_utils`. Expected: PASS.
- [ ] **Step 5: Bless (lead)** system order and API: `M1 Task 10: flow fields`.
- [ ] **Step 6: Gate** `cargo xtask gate task --track sim-core`. Expected: PASS.
- [ ] **Step 7: Commit** `feat(fl_sim): flow fields for group moves — abstract integration per (cluster, profile), lazy tiles, outcome-neutral cache` with trailers `Refs: ADR-0015` and `Blessed: …`.

### Task 11: Steering and separation

**Track:** sim-core + perf · **Refs:** spec §7, §5.4 (pure compute, ordered apply), ADR-0006 (R72 Transient), R76 · **Lane 1**

**Files:**
- Create: `crates/fl_sim/src/steer.rs`, `crates/fl_sim/tests/steering.rs`
- Modify: `crates/fl_sim/src/{movement,lib}.rs`
- Blessed: system order, API, golden, budgets and perf snapshots (`steer.units`).

**Interfaces:**
- Consumes: `SpatialGrid::query_radius`, `Footprint`, `SteeringRules`, `NavGrid::passable`.
- Produces (R76: no per-tick Transient component; a Transient component is removed from every entity each tick, an archetype move per unit):
```rust
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)] pub struct SteerPushes { .. }   // Transient "core.steer",
    // Class::TransientReset(SteerPushes::clear): emptied at the start of each tick by Housekeeping, capacity kept (no per-tick allocation)
impl SteerPushes {
    pub fn clear(&mut self);
    pub fn get(&self, id: SimId) -> FxVec2;      // this tick's separation push in metres; FxVec2::ZERO for a unit not steered (binary search)
    pub fn len(&self) -> usize;
    pub fn is_empty(&self) -> bool;
}
    // one (SimId, FxVec2) entry per steered unit, pushed in SimIndex order, so sorted by SimId
pub(crate) fn compute_steering(..);   // ThinkSet::Behaviour core, after process_path_queue; the only writer of SteerPushes
```
- **Who is steered.** Units with `Mobility` and `Footprint` that are moving, plus idle units whose `sim_id % idle_every == tick % idle_every`.
- **Neighbours.** `query_radius(pos, r_max)`, where `radius(u) = max(width, length) / 2` and the pair distance is `r = radius(u) + radius(v) + separation_margin_m`.
- **Push.** The sum, over neighbours v ≠ u in `SimId` order with `d < r`, of `(u − v) / d × (r − d) × separation_gain`. A coincident pair (d = 0) pushes along +x for the higher `SimId` and −x for the lower. The push is capped at `max_push_frac × speed` per tick the push stands for: one tick for a moving unit, `idle_every` ticks for an idle one (R113). `idle_every` lies in 1..=64.
- **Positions read.** Only positions as of the start of the tick (nothing moves before `ActSet::Move`), so each push is order-free. `compute_steering` walks `SimIndex` and appends one entry per steered unit to `SteerPushes`.
- **`move_system` integration.** Add `SteerPushes::get(sim_id)` to the step. Try the whole move (step plus push), then the step without the push, then the whole move's x component alone, then its y component alone, else stay (count `move.blocked`) (R114). A move fits only if every cell its segment sweeps is passable for the profile. `steer.units` counts the units steered.

- [ ] **Step 1: Write the failing tests** (`crates/fl_sim/tests/steering.rs`):
```rust
#[test] fn overlapping_idle_units_separate()     // two tanks spawned on one point → by tick 40 their distance ≥ r − 0.1 m
#[test] fn separation_never_pushes_into_water()  // 10 tanks crowded on a 1-cell shore strip, 200 ticks → every tick every unit's cell passable
// proptest (64 cases; milestone_ variant at 1024): random ASCII maps, random units and random move/stop/rotate commands, 300 ticks →
//   every tick, every unit with Mobility stands on a cell passable for its profile
#[test] fn units_never_enter_blocked_cells()
#[test] fn milestone_units_never_enter_blocked_cells_long()
#[test] fn idle_steering_is_staggered()          // 100 idle units: "steer.units" ≤ moving + ceil(100 / 5) per tick
#[test] fn steer_pushes_are_per_tick_and_sorted() // after a tick: SteerPushes entries strictly ascending by SimId, len == "steer.units";
                                                  //   after a later tick that steers no unit it is empty (Housekeeping reset it);
                                                  //   the units' archetypes are the same before and after a steering tick
#[test] fn smoke_steering_twin_and_shuffle()
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_sim --test steering`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo nextest run --profile agent -p fl_sim -p fl_test_utils`. Expected: PASS (protected `.new` pending).
- [ ] **Step 5: Regenerate** system order, API, the golden, budgets and perf snapshots. The lead blesses them: `M1 Task 11: steering and separation`.
- [ ] **Step 6: Gate** `cargo xtask gate task --track sim-core --track perf`. Expected: PASS. The controller then runs `gate pr` and fast-forwards `main` (end of Phase II, lane 1).
- [ ] **Step 7: Commit** `feat(fl_sim): separation steering, axis sliding at blocked cells, staggered idle steering` with trailer `Blessed: …`.

### Task 16: Orders input and testbed controls

**Track:** client · **Refs:** spec §10, §11, ADR-0008 · **Lane 2** (after Tasks 2 and 15)

**Files:**
- Create: `crates/fl_client/src/orders.rs`, `crates/fl_client/tests/orders.rs`
- Modify: `crates/fl_client/src/lib.rs`, `crates/fl_client/src/debug/stats.rs` (time-control window lists the hotkeys), `crates/fl_client/src/debug/console.rs` (console `terrain <x0> <y0> <x1> <y1> <id>` issues `debug.set_terrain` as `LocalPlayer`)

**Interfaces:**
- Consumes: `Selection`, `ClickedUnit` (Task 15); `CursorGround`; `RotateCmd`, `FollowCmd`, `MoveCmd`, `StopCmd`; `encode_command`, `LocalDriver`, `LocalPlayer`, `PlayerColors`.
- Produces:
```rust
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)] pub enum OrderMode { #[default] Normal, Rotate, Follow }
#[derive(Message, Clone, Copy, Debug, PartialEq)]
pub enum OrderInput { RightClick { ground: Option<Vec3>, unit: Option<SimId>, shift: bool, alt: bool },
                      LeftClickInMode { ground: Option<Vec3>, unit: Option<SimId>, shift: bool }, Stop }
#[derive(Clone, Copy, Debug, PartialEq)] pub enum PlannedOrder { Move { target: FxVec2, queue: bool }, Follow { leader: SimId, queue: bool },
                                                                 Rotate { toward: FxVec2, queue: bool }, Stop }
pub fn order_for(input: &OrderInput, mode: OrderMode, selection: &Selection, is_own: impl Fn(SimId) -> bool) -> Option<PlannedOrder>;
    // empty selection → None. RightClick: an own unit and !alt → Follow; otherwise the ground point → Move (an enemy unit's position when no ground hit).
    // LeftClickInMode: Rotate mode → Rotate at the ground point; Follow mode → Follow the clicked unit. Stop → Stop.
pub fn ground_to_sim(p: Vec3) -> FxVec2;   // (x, z) metres via Fx::from_f64
#[derive(Resource, Clone, Debug)] pub struct TestbedKeys { pub pause: KeyCode, pub speed_up: KeyCode, pub speed_down: KeyCode, pub cycle_player: KeyCode,
                                                           pub stop: KeyCode, pub rotate: KeyCode, pub follow: KeyCode }
    // defaults: P, Equal, Minus, F4, H, R, F (M6's command grid will rebind them)
pub const SPEED_STEPS: [f32; 6] = [0.25, 0.5, 1.0, 2.0, 4.0, 8.0];
pub struct OrdersPlugin;   // one PlayerCommand per order: every selected unit, sorted, as LocalPlayer; the mode resets to Normal after an order unless Shift
```

- [ ] **Step 1: Write the failing tests** (fake runner; header tables list `core.follow`, `core.move`, `core.rotate`, `core.stop`):
```rust
#[test] fn right_click_ground_issues_move()      // selection {#2,#1}: one core.move, units [#1,#2], target decodes to the ground point, queue false
#[test] fn shift_right_click_queues()
#[test] fn right_click_own_unit_follows()        // FollowCmd { leader: #3 }
#[test] fn alt_right_click_unit_forces_move()
#[test] fn right_click_enemy_moves_to_it()       // M1 has no attack: Move to the enemy's position
#[test] fn rotate_mode_left_click_issues_rotate()
#[test] fn stop_key_issues_stop()
#[test] fn no_selection_no_command()
#[test] fn orders_while_paused_apply_on_resume() // pause at next 10; right-click → exec_tick 11; resume → the TickInput for 11 carries it
#[test] fn speed_keys_step_through_speeds()      // 1 → 2 → 4 → 8 → 8; and down to 0.25, which holds
#[test] fn pause_key_toggles()
#[test] fn cycle_player_wraps()                  // PlayerColors {0, 1}: F4 → LocalPlayer 1 → 0; the selection clears
#[test] fn unknown_command_name_is_reported()    // a header without "core.rotate": rotate order → no command, one warn log, no panic
#[test] fn console_terrain_issues_debug_set_terrain()   // "terrain 70 122 72 127 base.water" → one debug.set_terrain { (70,122), (72,127), "base.water" }
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_client --test orders`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo nextest run --profile agent -p fl_client`. Expected: PASS.
- [ ] **Step 5: Gate** `cargo xtask gate task --track client`. Expected: PASS.
- [ ] **Step 6: Commit** `feat(fl_client): right-click orders, rotate/follow/stop keys, Shift-queue, testbed pause/speed/player hotkeys`.

### Task 17: Terrain view — heightmap mesh, unit heights, ground picking

**Track:** client · **Refs:** spec §10 · **Lane 2** (after Task 4)

**Files:**
- Create: `crates/fl_client/src/terrain.rs`, `crates/fl_client/tests/terrain.rs`
- Modify:
  - `crates/fl_client/src/{map_view,view,lib}.rs`
  - `crates/fl_client/src/debug/spawn_panel.rs` (`update_cursor_ground` uses `ray_ground`)
  - `crates/fl_game/src/builder.rs` (`MapView.terrain` from the scenario's map)

**Interfaces:**
- Consumes: `MapDef.{terrain, layers}`, `TerrainDef` (Task 3); base layers (Task 4); `MapView`, `sim_to_world`, `CursorGround`.
- Produces:
```rust
#[derive(Clone, Debug, PartialEq)] pub struct TerrainView { pub cells: UVec2, pub cell_m: f32, pub heights_m: Vec<f32>, pub colors: Vec<[f32; 3]> }   // per cell, y north
impl TerrainView {
    pub fn from_content(map: &MapDef, terrains: &KindTable<TerrainDef>) -> Option<TerrainView>;   // None without layers
    pub fn height_at(&self, x_m: f32, y_m: f32) -> f32;   // bilinear between cell centres, clamped at the edges
}
pub struct MapView { pub size_m: Vec2, pub ground_color: Color, pub terrain: Option<TerrainView> }   // existing resource gains `terrain`
pub fn terrain_mesh(t: &TerrainView) -> Mesh;   // a vertex per cell centre (x = (cx + 0.5) × cell, z = (cy + 0.5) × cell, y = height), vertex colours, normals, 2 triangles per quad
pub fn ray_ground(ray: Ray3d, terrain: Option<&TerrainView>) -> Option<Vec3>;   // y = 0 plane without terrain; else march 1 m from the max-height plane, then bisect 8 times
// MapViewPlugin spawns the mesh in place of the flat plane when `terrain` is Some; unit visuals stand at height_at(x, y)
```

- [ ] **Step 1: Write the failing tests:**
```rust
#[test] fn terrain_view_from_base_testbed()     // cells (250, 250); the colour at cell (10, 124) == base.water's
#[test] fn height_at_bilinear_and_clamped()     // a 2×1 view with heights 0 and 2: the midpoint → 1.0; beyond the edge → the edge height
#[test] fn terrain_mesh_counts()                // a 4×3 view: 12 vertices, 36 indices
#[test] fn ray_ground_hits_hill()               // a straight-down ray at the testbed hill centre → y within 0.05 of height_at
#[test] fn ray_ground_flat_without_terrain()
#[test] fn units_stand_on_terrain()             // fake-runner record at the hill → Transform.translation.y within 0.05 of height_at
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_client --test terrain`. Expected: FAIL.
- [ ] **Step 3: Implement.** Check `Mesh`, `PrimitiveTopology`, `Mesh::ATTRIBUTE_COLOR` and `Ray3d` with the `bevy-api` skill.
- [ ] **Step 4: Run** `cargo nextest run --profile agent -p fl_client -p fl_game`. Expected: PASS.
- [ ] **Step 5: Gate** `cargo xtask gate task --track client`. Expected: PASS.
- [ ] **Step 6: Commit** `feat(fl_client): heightmap terrain mesh coloured by terrain type, units on the ground, heightfield cursor picking`.

### Task 20: AI seam — the controller host

**Track:** client + content · **Refs:** ADR-0016, spec §8 · **Lane 2** (after Task 2)

**Files:**
- Create: `crates/fl_client/src/ai.rs`, `crates/fl_client/tests/ai.rs`, `crates/fl_game/tests/ai.rs`, `assets/base/scenarios/testbed_ai.scenario.ron` (the testbed with player 1 `Ai("null")`)
- Modify: `crates/fl_client/src/lib.rs`, `crates/fl_game/src/builder.rs`

**Interfaces:**
- Consumes: `PlayerView`, `AiController`, `ai_controller` (Task 2); `SnapshotBuffer`, `LocalDriver`, `RunnerLink`.
- Produces:
```rust
#[derive(Resource, Default)] pub struct AiHost { pub players: BTreeMap<PlayerId, (TeamId, Box<dyn AiController>)>, pub thinks: u64 }
pub struct AiPlugin;   // Update, after SimViewSystems: when SnapshotBuffer.curr's tick advanced this frame, each controller thinks once on
                       // PlayerView::from_snapshot; each draft is issued through LocalDriver if draft.player is that AI's player, else dropped with a warn
// fl_game: GameBuilder fills AiHost from Players (Controller::Ai(id)); an unknown id fails prepare(): "unknown AI controller 'x' for player 1"
```

- [ ] **Step 1: Write the failing tests:**
```rust
// crates/fl_client/tests/ai.rs (fake runner)
#[test] fn null_ai_issues_nothing()             // 20 ticks: LocalDriver.pending() empty; AiHost.thinks == ticks observed
#[test] fn ai_thinks_once_per_new_tick()        // 3 TickOutputs in one frame → one think that frame
#[test] fn draft_for_another_player_is_dropped()   // a test AI for player 1 drafting for player 0 → nothing issued
// crates/fl_game/tests/ai.rs (prepare_headless)
#[test] fn ai_scenario_builds_host()            // scenario "base.testbed_ai": AiHost has player 1 with id "null"
#[test] fn unknown_ai_is_error()                // a scenario with Ai("nope") → Err containing "unknown AI controller 'nope'"
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_client --test ai -p fl_game --test ai`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo nextest run --profile agent -p fl_client -p fl_game` and `cargo run -p fl_cli -- data-check assets/base`. Expected: PASS; exit 0.
- [ ] **Step 5: Gate** `cargo xtask gate task --track client --track content`. Expected: PASS.
- [ ] **Step 6: Commit** `feat(fl_client): AI controller host on PlayerView with a do-nothing Controller::Ai` with trailer `Refs: ADR-0016`.

### Task 18: Overlays — paths, passability, cells; BRP `fl.debug_query`

**Track:** client · **Refs:** ADR-0015, spec §11 · **Lane 2** (after Tasks 7 and 8 are merged)

**Files:**
- Create: `crates/fl_client/src/overlays.rs`, `crates/fl_client/tests/overlays.rs`
- Modify: `crates/fl_client/src/{lib,debug/remote}.rs`, `crates/fl_client/tests/remote.rs`

**Interfaces:**
- Consumes: `SimQuery::Debug`, `SimReply::Debug` (Task 2); the JSON of `core.paths`, `core.passability`, `core.clusters` (Tasks 5, 7, 8); `OverlayRegistry`, `PendingReplies`, `send_query`, `Selection`, `CursorGround`.
- Produces:
```rust
#[derive(Resource, Default, Debug)] pub struct OverlayState { pub paths: bool, pub passability: bool, pub cells: bool }
#[derive(Clone, Debug, PartialEq)] pub struct BlockedCells { pub cells: UVec2, pub version: u64, pub runs: Vec<(u32, u32)> }
#[derive(Clone, Debug, PartialEq)] pub struct ClusterView { pub cluster_cells: u32, pub clusters: UVec2, pub entrances: Vec<UVec2> }
pub fn decode_paths(v: &serde_json::Value) -> Result<BTreeMap<SimId, Vec<Vec2>>, String>;   // raw Fx → metres
pub fn decode_passability(v: &serde_json::Value) -> Result<BlockedCells, String>;
pub fn decode_clusters(v: &serde_json::Value) -> Result<ClusterView, String>;
pub fn grid_lines(centre: Vec2, radius_m: f32, step_m: f32) -> Vec<(Vec2, Vec2)>;   // lines on multiples of step_m within the square centre ± radius
pub struct OverlaysPlugin;
    // overlays "paths" (F5), "passability" (F6), "cells" (F7) in OverlayRegistry; each active overlay polls its query every 15 frames;
    // paths: the selected units; passability and cells: {"unit": the first selected unit}, else {"class":"Tracked","size":1};
    // gizmos: path polylines from each unit's Transform through its corners; blocked cells within 120 m of the camera focus;
    //   cell lines within 60 m of the cursor, cluster borders in view, entrance markers
// BRP (debug_remote): fl.debug_query { "name": .., "args": .. } → the reply JSON, or a JSON-RPC error with the query's message
```

- [ ] **Step 1: Write the failing tests:**
```rust
#[test] fn decode_paths_json()                     // {"3": {"corners": [[raw(10), raw(20)]], ..}} → {#3: [(10, 20)]}
#[test] fn decode_passability_runs()               // {"cells":[4,2],"version":7,"blocked":[[0,2],[6,1]]} → runs [(0,2),(6,1)]
#[test] fn decode_clusters_json()
#[test] fn paths_overlay_polls_selected_units()    // overlay on, selection {#1,#2}: the fake runner gets SimQuery::Debug { name "core.paths", args {"units":[1,2]} } within 15 frames
#[test] fn passability_follows_selected_unit()     // args {"unit": 1}
#[test] fn overlay_toggles_through_registry()      // OverlayRegistry toggle "cells" → OverlayState.cells true
#[test] fn grid_lines_count()                      // centre (50,50), radius 10, step 4 → 6 vertical + 6 horizontal lines (multiples of 4 in [40,60]:
                                                   //   40, 44, 48, 52, 56, 60)
// crates/fl_client/tests/remote.rs
#[test] fn brp_debug_query_round_trip()            // fl.debug_query core.debug_queries → JSON "queries"
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_client --test overlays`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo nextest run --profile agent -p fl_client`. Expected: PASS.
- [ ] **Step 5: Gate** `cargo xtask gate task --track client`. Expected: PASS.
- [ ] **Step 6: Commit** `feat(fl_client): path, passability and cell/cluster overlays from debug queries; fl.debug_query` with trailer `Refs: ADR-0015`.

---

## Phase III — hot reload, scale and determinism

### Task 12: Hot reload, sim side — ID-set comparison, in-place values, runner control, lenient load

**Track:** contract + sim-core + content + client · **Refs:** ADR-0012 (refinement), ADR-0006 · **Lane 1** (after Task 11)

**Files:**
- Modify:
  - `crates/fl_content/src/{db,def}.rs`, `crates/fl_content/tests/load.rs`
  - `crates/fl_view/src/{runner,lib}.rs`, `crates/fl_view/tests/view.rs`
  - `crates/fl_sim/src/{units,lib}.rs`, `crates/fl_sim/src/runner/{mod,threaded}.rs`, `crates/fl_sim/src/save/{mod,load}.rs` (`rebuild_derived` becomes `pub(crate)` so `reload.rs` runs the same rebuilds)
  - `crates/fl_client/tests/{debug,view}.rs`: their fake runners match `ToRunner` exhaustively and fail to compile once it gains a variant, so each gets a `_ => {}` arm. This is the only client change.
- Create: `crates/fl_sim/src/reload.rs`, `crates/fl_sim/tests/reload.rs`
- Blessed: API (`fl_content`, `fl_view`, `fl_sim`), view formats (if `SimReply` is traced).

**Interfaces:**
- Consumes: `ContentDb::string_tables`, `DefRegistry` tables, `rebuild_derived` (save/load: every feature's one `Derived` rebuild in feature load order, R74), `MatchHeaderRes`, `Content`, `save_world` (returns `Result<_, SaveError>`).
- Produces:
```rust
// fl_content
impl ContentDb {
    pub fn same_id_sets(&self, other: &ContentDb) -> bool;            // every content and registry string table holds the same names
    pub fn id_set_diff(&self, other: &ContentDb) -> Vec<String>;      // sorted lines "content.unit: +base.de.pz4", "content.terrain: -base.sand"
}
trait DefComponent { fn reload(&self, ctx: &SpawnCtx<'_>, entity: &mut EntityWorldMut<'_>) { self.instantiate(ctx, entity) } }
// fl_sim units.rs: HullDef::reload keeps the HP ratio — hp = if old hp == 0 { 0 } else { max(1, old_hp × new_max / old_max) }; Footprint replaced.
//                  MobilityDef::reload replaces Mobility only (MoveOrder and PathFollow kept). SelectableDef keeps the default.
// fl_sim/src/reload.rs
pub fn apply_content_values(world: &mut World, content: Arc<ContentDb>) -> Result<ReloadReport, ReloadError>;
    // needs cheats_allowed and same_id_sets; reloads every unit's def components (SimIndex order, component-name order); replaces Content and
    // MatchHeaderRes.rules_hash; runs rebuild_derived (every feature's one rebuild, feature load order); journals "core.content_reload"
    // { old_rules_hash, rules_hash, units }
#[derive(Debug, Clone, PartialEq, Eq)] pub struct ReloadReport { pub tick: Tick, pub old_rules_hash: u64, pub rules_hash: u64, pub units_reloaded: u32 }
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)] pub enum ReloadError { #[error("hot reload needs cheats_allowed")] CheatsOff,
    #[error("the content's ids changed: {0:?}")] IdSetsDiffer(Vec<String>), #[error("the content was loaded with another feature set")] RegistryMismatch }
// fl_view runner.rs
#[non_exhaustive] pub enum ToRunner { Tick(TickInput), Query { id: u64, query: SimQuery }, Control { id: u64, control: RunnerControl }, Shutdown }
    // non-exhaustive from now on (ADR-0012 refinement), like SimQuery and SimReply; matches outside fl_view need a wildcard arm
#[derive(Debug, Clone, PartialEq)] #[non_exhaustive] pub enum RunnerControl { ReloadContent(OpaqueContent) }
#[derive(Clone)] pub struct OpaqueContent(pub Arc<dyn Any + Send + Sync>);   // Debug prints "OpaqueContent"; PartialEq is Arc::ptr_eq
SimReply::Reloaded { tick: Tick, rules_hash: u64 }
// fl_sim runner
impl SimRunner { pub fn control(&mut self, control: RunnerControl) -> SimReply; }   // ReloadContent: downcast to ContentDb → apply_content_values →
                                                                                     // Reloaded, or Error(text); the threaded loop answers FromRunner::Reply
// fl_sim save
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)] pub struct LoadOptions { pub drop_unknown_units: bool }
pub fn load_world_with(features: &[Arc<dyn SimFeature>], content: Arc<ContentDb>, bytes: &[u8], run: RunConfig, opts: LoadOptions) -> Result<Loaded, LoadError>;
    // load_world(..) == load_world_with(.., LoadOptions::default()); with drop_unknown_units an entity whose UnitType name is unknown is skipped
    // (its SimId stays unused) and listed in Loaded.dropped, with a warn log. check_save ignores the rules hash (R70), so a save of the
    // running match loads under the edited content; no SimEntity is created for a skipped entity (the load path is the only other creator, R73)
pub struct Loaded { .., pub dropped: Vec<(SimId, String)> }
// any load: a TerrainGrid palette name with no TerrainDef in the content → LoadError::Codec { name: "core.terrain", message: "unknown terrain '<id>'" }
// apply_content_values, after the rebuilds: in SimIndex order, every unit whose movement profile changed (class or size) and whose PathFollow is
//   Waiting, Following or Flow → Idle, journaled order_changed "replanned"; advance_orders re-requests it (the old request turns stale)
// neither reload path rewrites TerrainGrid: edited map layers change the content and rules hash only (ADR-0012 refinement)
```

- [ ] **Step 1: Write the failing tests:**
```rust
// crates/fl_content/tests/load.rs
#[test] fn same_id_sets_true_for_value_change()
#[test] fn same_id_sets_false_when_unit_added()   // diff == ["content.unit: +a.gamma"]
#[test] fn terrain_kind_added_changes_id_set()
// crates/fl_view/tests/view.rs
#[test] fn opaque_content_eq_is_identity()
// crates/fl_sim/tests/reload.rs
#[test] fn value_reload_keeps_ids_and_hp_ratio()   // tank 50/100 → content with hp 200 → 100/200; SimIds, poses, MoveOrder unchanged
#[test] fn value_reload_changes_speed()            // road_speed 36 → 72 km/h: the next tick's step doubles
#[test] fn value_reload_updates_passability()      // forest gains blocks [Tracked]: passable() flips for t.tank; HpaGraphs == a fresh build
#[test] fn reload_refused_without_cheats()         // Err(CheatsOff); state hash unchanged
#[test] fn reload_refused_when_ids_differ()        // Err(IdSetsDiffer(["content.unit: +t.extra"]))
#[test] fn reload_journals_content_reload()
#[test] fn reload_is_deterministic()               // two sims reloaded at tick 50 → equal hash streams to tick 200
#[test] fn rebuild_drops_vanished_unit_types()     // save_world(..).expect("no pending spawns"); content without t.truck; load_world_with(drop) → trucks in
                                                   //   `dropped`; survivors keep SimIds; runs on (twin-equal)
#[test] fn load_without_drop_rejects_unknown_unit() // load_world → Err(LoadError::Codec { name: "core.unit_type", .. })
#[test] fn threaded_runner_applies_reload()        // spawn_threaded; Control ReloadContent → Reply Reloaded { rules_hash: new }; the next tick uses the new speed
#[test] fn rebuild_with_vanished_terrain_is_load_error()   // a save whose TerrainGrid names "t.swamp"; content without it → Err(LoadError::Codec { name: "core.terrain", .. })
#[test] fn reload_changing_profile_replans()       // t.tank width 3.0 → 7.0 while following a path: "replanned"; its next plan uses size 2; HpaGraphs == a fresh build
#[test] fn value_reload_keeps_terrain_grid()       // new content with one map pixel changed: TerrainGrid unchanged, rules hash changed
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_content -p fl_view -p fl_sim -E 'test(/reload|same_id|id_set|opaque|drop|terrain_kind_added/)'`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo nextest run --profile agent -p fl_content -p fl_view -p fl_sim -p fl_client`. Expected: PASS. `fl_client`'s sources only send `ToRunner` values and never match one, so they compile unchanged. Its two test fake runners gain the wildcard arm (Files) and never receive a `Control`.
- [ ] **Step 5: Bless (lead)** API (and formats if they changed): `M1 Task 12: hot reload runner control, lenient load`.
- [ ] **Step 6: Gate** `cargo xtask gate task --track contract --track sim-core --track content --track client`. Expected: PASS.
- [ ] **Step 7: Commit** `feat(fl_sim): hot reload — ID-set check, in-place value reload keeping ratios, runner control message, lenient load` with trailers `Refs: ADR-0012` and `Blessed: …`.

### Task 13: Scale scenario on terrain with pathing; counters, budgets, bench

**Track:** sim-core + tools + perf + client · **Refs:** ADR-0010, ADR-0006 (replay format), spec §5.7 · **Lane 2** (after Task 11)

**Files:**
- Modify:
  - `crates/fl_sim/src/{scale,replay}.rs` (`REPLAY_FORMAT_VERSION` → 3, R62), `crates/fl_sim/tests/scale_mix.rs`, `crates/fl_sim/tests/replay.rs` (only if a test pins the replay format version)
  - `crates/fl_test_utils/src/perf.rs`, `crates/fl_test_utils/tests/perf_counters.rs`
  - `crates/fl_cli/src/bench.rs`, `crates/fl_cli/tests/bench.rs`
  - `crates/fl_game/src/builder.rs`, `crates/fl_game/tests/builder.rs`: the stress mode runs `ScaleScenario::on_terrain(units)`, and its `MapView.terrain` comes from that map through Task 17's `TerrainView::from_content`, so Task 21's render stress shows the terrain mesh.
- Blessed:
  - `docs/perf/budgets.ron`, `crates/fl_test_utils/tests/snapshots/perf_counters__*.snap`
  - `crates/fl_sim/tests/snapshots/formats__save_codec_formats.snap` (the replay layout: `ReplayStart::Scale`), API (`fl_sim`), golden (format version)

**Interfaces:**
- Consumes: `init_terrain`, `NavGrid`, `nearest_passable`, `profile_of` (Tasks 5–6); the `base.scale_2km` layers (Task 4); `TerrainView`, `MapView.terrain` (Task 17).
- Produces:
```rust
pub struct ScaleScenario { .., pub map: Option<String> }   // Default (and with_units): None = M0's open map, so the many existing tests that build
                                                           //   ScaleScenario::default() on fixture mods without base.scale_2km keep working
pub const SCALE_MAP: &str = "base.scale_2km";
impl ScaleScenario { pub fn on_terrain(units: u32) -> Self; }   // with_units(units) with map Some(SCALE_MAP)
    // populate: with a map, its layers set up TerrainGrid (map side must equal map_m: SetupError::MapSizeMismatch { map, map_m });
    // a unit's drawn position is redrawn up to 16 times until passable for its profile, then nearest_passable. The redraws come from the
    // same setup `placement` Rolls (the scenario's own seed, under populate's existing scoped #[expect(clippy::disallowed_methods)]),
    // never from rolls() or a new Stream
// fl_cli bench: runs ScaleScenario::on_terrain by default; --flat runs with_units (map None, the M0-comparable workload);
//   rows report "map": "base.scale_2km" | "open"
// fl_test_utils perf (perf_counters scenarios scale_500/3000/5000): ScaleScenario::on_terrain
```
- **Budgets.** The scale mix issues only single path requests (wander), so `flowfield.*` is never measured there and gets no budget. `check_budgets` fails a budgeted counter that a run doesn't measure.

- [ ] **Step 1: Write the failing tests:**
```rust
// crates/fl_sim/tests/scale_mix.rs
#[test] fn scale_units_start_on_passable_cells()   // on_terrain(3000): every unit's cell passable for its profile
#[test] fn scale_pathing_reaches_targets()         // on_terrain(500), 400 ticks: ≥ 1 path_result per unit; no unit on a blocked cell at any tick
#[test] fn flat_scale_has_open_terrain()           // ScaleScenario::default() (map None) → TerrainGrid::open
#[test] fn scale_map_size_must_match()             // on_terrain(100) with map_m 1000 → Err(MapSizeMismatch)
#[test] fn scale_mix_twin_run()                    // (existing) now on_terrain
// crates/fl_test_utils/tests/perf_counters.rs: smoke_counters_scale_500, counters_scale_3000, milestone_counters_scale_5000 (existing) re-recorded on
//   on_terrain with the measured path.*, steer.* and move.* counters (no flowfield.*: no group moves at scale); plus
#[test] fn path_budget_holds_at_scale()            // scale_3000: per tick, path.budget_spent − (the largest single charge that tick) < Path.budget_per_tick
// crates/fl_cli/tests/bench.rs
#[test] fn bench_flat_flag_uses_open_map()
// crates/fl_game/tests/builder.rs (prepare_headless)
#[test] fn stress_map_view_has_scale_terrain()     // stress(500): MapView.terrain is Some with cells (500, 500)
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_sim --test scale_mix` and `cargo nextest run --profile agent -p fl_game --test builder -E 'test(/stress_map_view/)'`. Expected: FAIL.
- [ ] **Step 3: Implement.** Then record: `FL_RECORD_BUDGETS=1 cargo nextest run --profile agent -p fl_test_utils --test perf_counters -E 'test(/scale/)'` (delete any stale `budgets.ron.new` first), and `FL_RECORD_GOLDEN=1` for the golden (format 3).
- [ ] **Step 4: Diagnostic wall clock (not a gate).** Run `cargo run -p fl_cli --release -- bench --units 3000 --ticks 200 --warmup 40 --repeat 1 --json <scratch>/m1-t13.json`, and the same with `--flat`. Paste both 3000-unit means in the report. Over 15 ms: profile with `--profile` and report the top systems.
- [ ] **Step 5: Bless (lead)** budgets, perf snapshots, formats, API and the golden: `M1 Task 13: scale mix on the 2 km terrain with pathing`.
- [ ] **Step 6: Run** `cargo nextest run --profile agent -p fl_sim -p fl_test_utils -p fl_cli -p fl_game`. Expected: PASS. Also add a temporary system that enqueues every unit every tick, and expect `smoke_counters_scale_500` to fail naming `path.budget_spent` or `path.queue_len`. Remove it afterwards.
- [ ] **Step 7: Gate** `cargo xtask gate task --track sim-core --track tools --track perf --track client`. Expected: PASS.
- [ ] **Step 8: Commit** `feat(perf): scale entity mix on the 2 km terrain with pathing; M1 counter budgets; bench --flat` with trailers `Refs: ADR-0010` and `Blessed: …`.

### Task 19: Hot reload, game side — watcher, both reload paths, visuals, replay capture

**Track:** client + tools · **Refs:** ADR-0012 (refinement) · **Lane 2** (after Task 12)

**Files:**
- Create: `crates/fl_game/src/{reload,replay_capture}.rs`, `crates/fl_game/tests/reload.rs`
- Modify:
  - `crates/fl_game/src/{builder,lib}.rs`
  - `crates/fl_client/src/{placeholder,link,view}.rs` (`view.rs`: `drive_runner` appends to `InputLog`), `crates/fl_client/src/debug/{mod,console}.rs` (`LoadRequest::Rebuild`)
  - `crates/fl_observe/src/config.rs` (and its `repo_debug_ron_parses` test), `debug.ron` (`hot_reload: true`, `hot_reload_poll_ms: 500`; the log filter stays `info,fl_sim=debug,fl_sim::spawn=info`)

**Interfaces:**
- Consumes: `apply_content_values` through `RunnerControl::ReloadContent`, `load_world_with`, `LoadOptions`, `ContentDb::{same_id_sets, id_set_diff}` (Task 12); `load_match`, `ScenarioLoader`; `ReplayRecorder`, `Replay`, `ReplayEvent::ContentReload`.
- Produces:
```rust
// fl_client
pub enum LoadRequest { Scenario(String), Save(Vec<u8>), Rebuild { save: Vec<u8>, content: Arc<ContentDb> } }   // Rebuild loads with drop_unknown_units
#[derive(Resource, Default)] pub struct InputLog { pub inputs: Vec<TickInput>, pub hashes: Vec<(Tick, u64)> }   // drive_runner appends when present
// placeholder.rs: PlaceholderAssets rebuild when ClientContent changes; existing visuals get the new parts
// fl_game/src/reload.rs
#[derive(Resource)] pub struct ContentWatcher { .. }
impl ContentWatcher { pub fn new(dirs: Vec<PathBuf>, every: Duration) -> Self; pub fn scan(&mut self) -> bool; }   // (mtime, len) of every file under the dirs, sorted
    // mtime kept as a Duration since UNIX_EPOCH: the client clippy profile bans naming the std::time::SystemTime type
#[derive(Debug, Clone, PartialEq)] pub enum ReloadOutcome { Values { tick: Tick, rules_hash: u64 }, Rebuilt { tick: Tick, dropped: Vec<(SimId, String)> },
                                                            VisualsOnly, Failed(String), Refused(String) }
pub fn reload_content(world: &mut World) -> ReloadOutcome;
    // load_content(features, mods): Err → Failed(errors) and the match keeps running; cheats off → Refused; same ids and same rules hash → VisualsOnly
    // (ClientContent swapped); same ids → Control ReloadContent, blocking on the reply like load_match → Values; ids differ → SimQuery::Save { pending }
    // → LoadRequest::Rebuild → Rebuilt. A SimReply::Error to the save (SaveError, for example spawn requests still waiting) → Failed(text), and the
    // match keeps running. Each outcome is logged and printed to the console; Values and Rebuilt push ReplayEvent::ContentReload
// automatic in dev builds when debug.ron hot_reload is true: the watcher scans every hot_reload_poll_ms; console "reload" forces one
// fl_game/src/replay_capture.rs
#[derive(Resource)] pub struct ReplayCapture { .. }   // header + ReplayStart of the running match, InputLog, events; reset by load_match
// console "replay save <path>" writes Replay::to_bytes (a replay with a reload is not reproducible: replay-verify reports "non-reproducible")
```

- [ ] **Step 1: Write the failing tests** (`crates/fl_game/tests/reload.rs`; `prepare_headless` on a temp copy of `assets/base`):
```rust
#[test] fn value_edit_reloads_in_place()             // edit pz3 hp 110 → 220 → Values; SimQuery::Entity(#2) shows max 220 and hp at the same ratio
#[test] fn adding_a_unit_rebuilds_with_same_sim_ids() // add a unit file (+ visual) → Rebuilt; snapshot sim ids identical; the tick unchanged
#[test] fn removing_a_unit_type_drops_its_units()    // remove tiger from units, visuals, roster and scenario → Rebuilt { dropped: [(#3, "base.de.tiger")] }
#[test] fn malformed_ron_keeps_match_running()       // a truncated pz3.unit.ron → Failed containing "pz3.unit.ron"; ticks keep advancing
#[test] fn reload_refused_without_cheats()           // match with cheats_allowed false → Refused
#[test] fn reload_while_paused_applies()             // paused: Values; still paused; tick unchanged
#[test] fn visual_only_change_swaps_client_content() // edit a visual's scale → VisualsOnly; PlaceholderAssets rebuilt
#[test] fn reload_records_replay_event()             // replay save → Replay.events == [ContentReload { tick, rules_hash }]; !reproducible()
#[test] fn watcher_detects_change()                  // scan() false; rewrite a file with new length → true; then false
#[test] fn failed_rebuild_keeps_match_running()      // a kind no map uses (base.test_mud, added to the mod copy before the match; the testbed PNG
                                                     //   paints every base kind, so removing one is a content error, R132): debug.set_terrain
                                                     //   paints it; remove test_mud.terrain.ron → Failed containing "core.terrain"; the match
                                                     //   keeps its state and ticks keep advancing
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_game --test reload`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo nextest run --profile agent -p fl_game -p fl_client -p fl_observe`. Expected: PASS.
- [ ] **Step 5: Gate** `cargo xtask gate task --track client --track tools`. Expected: PASS.
- [ ] **Step 6: Commit** `feat(fl_game): hot reload — file watcher, in-place value reload, ID-set rebuild via save/load, visuals, replay capture` with trailer `Refs: ADR-0012`.

### Task 14: Determinism suite and golden replays for movement

**Track:** sim-core · **Refs:** ADR-0005, ADR-0006, spec §12.3 · **Lane 1** (after Tasks 12 and 13)

**Files:**
- Create: `crates/fl_test_utils/tests/movement_determinism.rs`
- Modify: `crates/fl_test_utils/tests/golden.rs`
- Blessed: `crates/fl_test_utils/tests/replays/testbed_paths.replay` (new) and `testbed_wander.replay` (if moved)

**Interfaces:**
- Consumes: everything from Tasks 5–13; `assert_deterministic`, `assert_shuffle_invariant`, `assert_save_round_trip`, `assert_branch`, `verify_replay`.
- Produces:
  - `record_testbed_paths() -> Replay`. The base testbed for 900 ticks, cheats on:
    - tick 5: player 0 moves #1–#4 to (720, 760). They cross the river; four units is below the group threshold, so these are single requests.
    - tick 5: player 1 moves #6–#13 (#11–#13 are T-34s spawned by `debug.spawn` at tick 1) to (300, 250). Its six Tracked units form a flow group; the truck and the rifleman get single requests.
    - tick 120: `debug.set_terrain` turns bridge x 70..=72 into water.
    - tick 200: player 0 Shift-queues rotate toward (720, 900), then a move to (650, 800).
    - tick 300: #5 follows #1.

    Hashes every tick.
  - Golden file `tests/replays/testbed_paths.replay`.

- [ ] **Step 1: Write the failing tests:**
```rust
// crates/fl_test_utils/tests/movement_determinism.rs
#[test] fn smoke_m1_testbed_paths_twin()            // assert_deterministic(testbed paths builder, 300)
#[test] fn m1_scenarios_shuffle_invariant()         // assert_shuffle_invariant on: testbed paths (600), an ASCII flow scenario (400),
                                                    //   ScaleScenario::on_terrain(500) (200)
#[test] fn m1_save_round_trip_mid_pathing()         // assert_save_round_trip at ticks 6 (requests queued), 130 (after set_terrain), 320 (follow active)
#[test] fn m1_branch_mid_pathing()                  // assert_branch(testbed paths, 6, 400) and (130, 500)
// crates/fl_test_utils/tests/golden.rs
#[test] fn testbed_paths_replay_verifies()          // fresh record verifies; ticks 900; inputs at ticks [1, 5, 120, 200, 300]
#[test] fn testbed_paths_semantics()                // C8-style semantic checks: the units player 0 sent cross on a bridge cell; no unit is ever on a water cell;
                                                    //   #1–#4 end within 3 m of their formation targets. It runs the builder on past the replay's
                                                    //   900 ticks until #1–#4 are idle (≈1650 ticks; R137); the golden itself stays 900 ticks
// golden_replays_verify (existing) now also requires testbed_paths.replay
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_test_utils`. Expected: FAIL (the golden file is missing).
- [ ] **Step 3: Record.** `FL_RECORD_GOLDEN=1 cargo nextest run --profile agent -p fl_test_utils --test golden` writes `testbed_paths.replay.new` (and `testbed_wander.replay.new`). The lead blesses: `M1 Task 14: movement golden replay`.
- [ ] **Step 4: Run** `FL_SHUFFLE_SEEDS=3 cargo nextest run --profile agent -p fl_sim -p fl_test_utils` and `cargo run -p fl_cli -- replay-verify crates/fl_test_utils/tests/replays/testbed_paths.replay --json`. Expected: PASS, and `{"status":"verified", …}`. The replay's `build_id` names the commit it was recorded at, and `fl_sim`'s `build.rs` names the current one. Replay verification only warns on a `build_id` mismatch (ADR-0007 refinement R70), so a later commit still verifies.
- [ ] **Step 5: Gate** `cargo xtask gate pr`. Expected: `gate pr: PASS`. The controller fast-forwards `main`.
- [ ] **Step 6: Commit** `test(fl_test_utils): M1 determinism suite and the testbed_paths golden replay` with trailer `Blessed: …`.

---

## Phase IV — integration (lane 1)

### Task 21: Integration — demo, milestone gate, perf, debugging guide

**Track:** docs + perf · **Refs:** ADR-0009, ADR-0010, ADR-0012, ADR-0014–0016 · **Lane 1** (after every task is merged)

**Files:**
- Create: `docs/m1-demo-checklist.md`, `docs/perf/M1.md`, `docs/perf/M1.json.new` (protected: the lead blesses it into `docs/perf/M1.json`)
- Modify: `docs/debugging.md`, `README.md` (M1 status line)

- [ ] **Step 1: Merge and gate.** With every branch merged into `m1/phase-a`, run `cargo xtask gate pr`. Expected: `gate pr: PASS`.
- [ ] **Step 2: Update `docs/debugging.md`.** Add:
  - every new journal kind, counter and span
  - the reject codes M1 uses (`core.invalid_target`, `core.queue_full` for every queued order kind), as `SimEvent::CommandRejected` and the `core.command_rejected` journal record report them
  - the five debug queries with request and response JSON
  - `fl_cli query` and `fl_cli map-gen`, each with a `--json` example
  - BRP `fl.debug_query`, `fl.select` and `fl.selection`, with example JSON
  - the console commands `reload` and `replay save`
  - the `debug.ron` keys `hot_reload` and `hot_reload_poll_ms`
  - the overlay keys F5/F6/F7 and the order and testbed hotkeys
  - a "why didn't my unit move" recipe: `core.paths` → `journal --kind core.path_result --sim-id N` → `core.passability {unit}` → `core.terrain_at`

  Run every documented command and check its output matches the document.
- [ ] **Step 3: Content and determinism.**
  - `cargo run -p fl_cli --release -- data-check assets/base` → exit 0, `10 units`, 8 terrain kinds.
  - `FL_SHUFFLE_SEEDS=3 cargo nextest run --profile agent -p fl_sim -p fl_test_utils` → PASS.
  - `replay-verify` on both golden replays → exit 0.
- [ ] **Step 4: Perf (tier 3).** Ask the user to confirm the fixed power plan first.
  - Run `cargo run -p fl_cli --release -- bench --units 500,3000,5000 --ticks 400 --warmup 40 --repeat 5 --out docs/perf/M1.md --json docs/perf/M1.json.new` (on terrain).
  - Run the same with `--flat`, writing `--out <scratch>/m1-flat.md --json <scratch>/m1-flat.json`. Paste its table into `M1.md` as a second table. Never write it into `M1.json`: the gate's `perf-file` row reads the first `rows` entry with `units` 3000, which must be the terrain run.
  - Expected: the 3000-unit mean is **≤ 15 ms** with pathing on terrain (G7). `gate milestone`'s `perf-file` row checks this from `docs/perf/M1.json`.
  - Expected: the `--flat` row is within 20% of `docs/perf/M0.json` at every size. If it isn't, write the justification into `M1.md` naming the systems responsible (from `--profile`), or fix it.
  - Add the machine, rustc, commit and power plan to `M1.md`. `M1.json.new`'s `meta` names the measured commit as `"head"`, as `M0.json`'s does. If `bench` doesn't write it, add it to the `.new` file before the bless.
  - The lead blesses `docs/perf/M1.json` (`cargo xtask bless docs/perf/M1.json --reason "M1 Task 21: M1 perf baseline"`) before Step 8, because the milestone gate fails while it is missing.
- [ ] **Step 5: Render stress.** `cargo run -p fl_game --release -- --stress 3000 --exit-after 25 --agent` (terrain mesh on). Expected: `avg_fps` ≥ 60. Append the line to `docs/perf/M1.md`.
- [ ] **Step 6: BRP smoke through `bevy_brp_mcp`.** Launch `cargo run -p fl_game --features debug_remote -- --remote --remote-port 15702 --agent`, then:
  - poll `fl.sim_state` until tick ≥ 1
  - `fl.select {sim_ids: [1, 2]}`
  - `fl.issue_command` `core.move` to (720, 760)
  - `fl.step {n: 200}`
  - `fl.debug_query core.paths {"units": [1, 2]}`
  - `fl.toggle_overlay paths`, then `fl.screenshot`

  Record the results in the checklist.
- [ ] **Step 7: Demo checklist.** Write `docs/m1-demo-checklist.md` and walk it with `cargo run -p fl_game --release`:
  - The terrain shows the river, two bridges, roads, forest, the hill and the ridge.
  - Units stand on the ground.
  - Click, box, Shift-add, double-click on a Pz III, Ctrl+1 / 1 / Shift+1 / Alt+1.
  - Right-click move across the river goes over a bridge; the tanks turn before driving; the group keeps its shape; Shift-queue works.
  - H stops, R+click rotates, a right-click on an own unit follows.
  - `debug.set_terrain` (console `terrain 70 122 72 127 base.water`) on the bridge in use makes the units repath.
  - F5/F6/F7 overlays show paths, blocked cells for the selected class, cells and clusters.
  - Pause (P), give orders, resume: the orders apply; `=`/`-` change the speed; F4 switches the player.
  - Editing `pz3.unit.ron` HP reloads in place; adding a unit file rebuilds; a broken file shows an error and the match continues.
  - `testbed_ai` runs with player 1 idle.
  - ADR confirmations: one row per Confirmation bullet of ADR-0012 and ADR-0014–0016, naming the passing test or gate step, plus any M0 rows M1 changed.
- [ ] **Step 8: Milestone gate.** Run `cargo xtask gate milestone --milestone M1`. Expected: `gate milestone: PASS`. At milestone level every nextest row, and the `gpu` and `brp` rows, fail when their filter matches no test. So a renamed or `cfg`'d-out `milestone_`, `gpu_` or `brp_` test fails the gate. M1's `brp_` tests are in Tasks 15 and 18, and its `milestone_` tests in Tasks 11 and 13.
- [ ] **Step 9: Review and commit.** Run the final whole-branch review (`superpowers:requesting-code-review`), fix the findings, and re-run `cargo xtask gate pr`. Commit `docs: M1 debugging guide, demo checklist and perf results`. The controller fast-forwards `main`.

---

## Builder's calls and deviations recorded here

- **Hot reload watcher.** Content is loaded with `std::fs`, so the testbed polls mtimes instead of Bevy's `file_watcher` (spec §6.4; ADR-0012 refinement).
- **In-house pathfinding.** No `pathfinding` crate: budget and expansion counting need our own loop, and nothing is downloaded (spec §7 says "can", not "must").
- **Path cost per class.** It uses `Movement.path_terrain_factor`, because unit types in one class differ and the graphs are shared per class. The unit's own `terrain_factor` drives its actual speed.
- **`terrain_factor` range** widens to [−1, 1], clamped by `min_speed_frac` (research §1.8: trucks are negative).
- **Height** drives per-class slope blocking (a guess in `rules.ron`) and rendering. Vision height bonuses come in M3.
- **Bridges** are terrain cells in M1, changed by `debug.set_terrain`. That exercises the dynamic-obstacle path (cluster invalidation). Destructible bridge objects with HP arrive with combat and engineering.
- **Order queue.** `MoveOrder` v2 holds a closed `Order` enum (`Move`, `Rotate`, `Follow`) as an interim. C7's registry-based order queue and activity stack replace it in the M2 prelude.
- **No path-result cache.** Only flow fields and HPA*'s intra-cluster paths are cached. The cold-cost charge keeps both outcome-neutral.
- **`PlayerView`** is built client-side from the snapshot until vision (M3) builds it in `Publish`.
- **The client records replays** (`replay save`), so ADR-0012's reload events have a home.
- **Key bindings** (H, R, F, P, `=`, `-`, F4, F5–F7) are provisional until M6's command grid.
- **No new unit types.** The base roster already covers Foot, Wheeled and Tracked. HalfTrack has no iteration-1 unit (spec §9.1), so the ASCII fixtures exercise it and wide clearance.
- **Golden replays and budgets move in several tasks.** Each re-records and the lead blesses, rather than one rebaseline at the end, so every commit's gate passes.
- **PNG decoding in `fl_content`.** It is the content loader, already does file I/O at load, and `arch-check` gives it the `Content` role (`bevy_ecs` family only). `png` is pure Rust with no Bevy crate. Sim crates receive integer layers, never images.
- **`core.invalid_target` is a code, not a variant.** It is `RejectReason::Feature("core.invalid_target")` (R71: add no variants; ruled R79), with the same stable code a variant would report.
- **M0 saves load under M1 (R78).** `Codec::fill_missing` (Task 5) fills `core.terrain`, `core.path_queue` and `core.path_follow` for older core saves. A protected M0 testbed save fixture, generated from `main`'s M0 tip, proves it. Saves carry no map id, so the terrain fill picks the content map whose size and cell match the saved `MapInfo`. Without a fill, the code's current behaviour stays: a missing Hashed resource is removed. R78 says "LoadError", but saves without a map rely on the removal.
- **Steering output is a scratch resource** (`SteerPushes`, Transient, sorted by `SimId`), not a Transient component (R76).
- **Core's one Derived rebuild** is `rebuild_core` (spatial, then nav), because a feature may register only one (R74).
- **`ScaleScenario` stays on the open map by default.** `ScaleScenario::on_terrain` opts in, so the M0 tests that build `ScaleScenario::default()` on fixture mods keep working. Bench, perf counters and the `fl_game` stress mode use the terrain.
- **`ToRunner` becomes `#[non_exhaustive]`** when it gains `Control` (Task 12), like `SimQuery`/`SimReply` (ruled R80; ADR-0012 refinement).
- **Cache reads never observe a miss (R84).** `FlowCache::direction` builds an evicted integration or tile before answering, so the eviction policy can't change outcomes; `flow_cache_overflow_does_not_change_outcomes` (Task 10) checks it with more concurrent keys than `flow_cache_max`. Read-miss rebuilds are never charged to the path budget, so their cost is bounded by `flow_cache_max` (how many keys stay warm), not by the budget; Task 13 measures a thrash with more live keys than `flow_cache_max` (R112).
- **Task 1 review fixes.** ADR-0014 pins the PNG formats, the unit anchor cell, the speed and cost evaluation and the rules ranges; the matching tests are in Tasks 3, 5, 12 and 19, and the `png` dependency rule is an `arch-check` rule from Task 3.

## Self-review

**Spec coverage** (M1 scope → task):

| Spec item | Task(s) |
|---|---|
| §7 terrain grid: height and terrain-type PNG layers, road, forest, river, bridge | 3, 4, 5 |
| §7 speed formula; movement-class blocking; clearance | 5 |
| §5.7 / §7 HPA* (16×16, per class + clearance); A* refinement | 6, 7 |
| §5.7 budgeted request queue, results in request order | 8 |
| §5.7 flow fields cached per (destination cluster, class) | 10 |
| §5.7 dynamic obstacles invalidate only touched clusters | 5, 7, 8, 10 |
| §7 steering and separation; hull rotation; formation offsets | 11, 8, 9 |
| §9.1 / §10 selection: click, box (`world_to_viewport`), Shift-add, double-click, control groups | 15 |
| §9.1 / §5.5 commands Move, Stop, Rotate, Follow, Shift-queue | 2, 8, 9, 16 |
| §11 testbed: pause with orders, speed, player switch | 16 (verified and extended) |
| §11 overlays: paths, passability, cells | 18 (queries in 5, 7, 8) |
| §6.4 / ADR-0012 hot reload | 12, 19 |
| §8 AI seam: `PlayerView` stub, do-nothing `Controller::Ai` | 2, 20 |
| §15 journal kinds, counters, spans; `docs/debugging.md` | 5, 7, 8, 10, 11, 12; 21 |
| §12.6 work-counter budgets; M1 bench; G7 | 8, 11, 13, 21 |
| §12.3 twin and shuffle, save and branch on pathing, golden replay | every sim task, 14 |
| §13.2 contracts through ADRs | 1, 2, 3, 12 |

- **Type consistency.** These names are checked across tasks: `MoveProfile { class, size }`, `CellRect`, `CellXY`, `NavGrid::{passable, speed_factor, cost, cell_of}`, `HpaGraphs::{build, invalidate}`, `plan_path` → `PathPlan`, `PathWork::charge`, `PathFollow`, `PathState::{Idle, Waiting, Following, Flow, Done, Failed}`, `FlowKey { cluster: ClusterId, profile }`, `PathQueue`, `RequestKind::{Single, Group}`, `MoveOrder { orders }`, `Order::{Move, Rotate, Follow}`, `SteerPushes`, `rebuild_core` / `rebuild_nav`, `ScaleScenario::on_terrain`, `RejectReason::Feature("core.invalid_target")`, `SimQuery::Debug` / `SimReply::Debug`, `RunnerControl::ReloadContent(OpaqueContent)`, `SimReply::Reloaded`, `LoadOptions`, `LoadRequest::Rebuild`, `Selection`, `ClickedUnit`, `TerrainView`, `MapView.terrain`.
- **Proportion.** The plan carries signatures, rules values, layouts and test assertions. Algorithm bodies are left to the implementer, apart from the tie-break, entrance and charge rules, which are contracts (ADR-0015).
