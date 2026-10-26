# Architect report: spec revision 2 (baseline upgrade) and ADRs 0001–0013

Date 2026-10-23. Inputs: `docs/research/baseline-recommendations.md` and the three reports behind it, the approved spec,
the M0 plan (read-only), and the code in `crates/` at `2eed139` plus Task 7's uncommitted work in progress.

Deliverables:
- `docs/superpowers/specs/2026-10-23-frontline-rts-design.md`, revised in place, with a "Revision 2" changelog at the top.
- `docs/adr/README.md` (index, rules, deprecation policy), `docs/adr/template.md` (MADR 4.0) and ADRs 0001–0013.
- This report. It is not committed (`.superpowers/` is git-ignored).

---

## 1. Spec changes by section

| Section | Change | Items |
|---|---|---|
| Header | Status "approved; revision 2 applied". The research list adds the three reports and the synthesis. ADRs are introduced, with "contract paths change only through an ADR". | B7 |
| **Revision 2** (new) | A changelog: every item with its section, its ADR and why. The user's staging decisions. B8 skipped. | all |
| §2 | G3 notes that single player already uses the lockstep envelope. G4: debug actions go through commands. G7 becomes an entity mix (3,000 units ≈ 8–12k sim entities plus projectiles). The networking non-goal clarifies that an in-process loopback lockstep driver is allowed. The "input code changes state" anti-goal now also covers debug code. | A9, A10, A12 |
| §3 | Sim crates use `bevy_app`/`bevy_ecs` only, and `fl_math`/`fl_protocol`/`fl_view` use no Bevy. A nightly toolchain only for rustdoc JSON. A per-worktree `build.build-dir` in the user config, with no shared `target-dir`. CI is local only. Determinism wording updated. cargo-deny enforces licences. | A1, A2, B6, B8, B9 |
| §4 | The baseline-upgrade decisions: staging, B8 skipped, repo-local Claude config, global installs. | — |
| §5.1 | New layout: `features/<name>/{sim,client}`, the new `fl_view` crate, `fl_catalog` and `fl_observe` listed, `xtask/`, `probes/`, and the config files (`deny.toml`, `OWNERS.toml`, `.config/nextest.toml`, `CLAUDE.md`, `.claude/`, `.mcp.json`). Dependency rules rewritten: client crates never depend on `fl_sim` directly, only `fl_game` assembles, feature-to-feature edges must match `deps()`, and `arch-check` runs one package at a time. | A1, A2, B2, B9 |
| §5.2 | `GameFeature` → object-safe `SimFeature { id, version, deps, build(&mut SimBuilder) }` + `ClientFeature { sim_feature, build }`. Topological sort; errors for cycles, missing deps, duplicates and orphan client features. `(id, version)` goes into the header. `SimBuilder` is the contract API. What a feature can register (classification, streams, journal records, view channels). Plugin hygiene. | A1, A4, A6 |
| §5.3 | Rewritten: `step_tick`; `SimRunner` (threaded and inline); `TickOutput`; `fl_view` as the client's only read model; view channels; `SimQuery`; `TickDriver` (Local, loopback Lockstep, Replay, Test); the client frame loop; failure handling. Resolves the old §14 SubApp question. | A2 |
| §5.4 | Keyed RNG (`roll`, `Rolls`, stream key = name hash, sequential RNG banned). Narrowed ordering rule plus `par_iter_mut` for entity-local updates. Explicit executor. Bevy hidden-state hazards. The ban list. Mandatory classification (Hashed/Derived/Transient/External) with a startup check. State hash computed in the driver, covering resources, with registry IDs hashed by name key. Canonical binary rules hash. Stable-string registries. | A3, A4, A5, A6, A7, A9 |
| §5.5 | The `PlayerCommand` envelope (`exec_tick`, `seq`), `(player, seq)` order, input delay, late and duplicate rejection, postcard. Payload data types vs sim handlers. `debug.*` commands gated by `cheats_allowed`. The `MatchHeader` struct and compatibility rules for handshake, replay and save. AI controllers run outside the sim. "Reveal all" is a render override. Replays are postcard with header and reload events. | A9, A10, A11 |
| §5.6 | Eight phases with a hook sub-set table, schedule settings, an explicit `ApplyDeferred` per phase, the spawn funnel, Message rotation, the driver steps, and the system-order snapshot. | A5 |
| §5.7 | Entity-mix target. The cell component is Derived. `Publish` writes changed records. The scale scenario composition (3k units, 6k followers, ~1k projectiles). Budgets enforced by work counters, not the 20% wall-clock rule. | A12, B5 |
| §5.8 (new) | Save and load: layout, versioned codecs, typed migrations, `SimId` relationships, the load procedure, the round-trip and branch tests, M0 scope, no `moonshine-save`. | A8 |
| §6.1 | `register_def_component` is on `SimBuilder`. `instantiate` is shaped like Bevy's `Template`. A pointer to C2/C6. | A1 |
| §6.2 | The scenario seed is the default match seed. | A3 |
| §6.3 | Visual defs are not in the rules hash (stated explicitly). | A9 |
| §6.4 | Hot reload: testbed-only, ID-set comparison, values in place, rebuild from a save on ID change, `ContentReload` replay events, non-reproducible marking. | A11 |
| §7 | One sentence: guesses are tracked by the provenance ledger from M2. | C2 |
| §8 | AI controller placement (outside the sim, `PlayerView` in `fl_view`). The lockstep groundwork list updated, plus `NetSession`. The campaign layer has its own save; persistent units use codecs. Modifiers point to C5. Production in the `Economy` phase. | A2, A9, D |
| §9 | Restructured into §9.1 milestones (M0 rewritten for groups A and B; M1 gains hot reload; M2 gated on entry requirements; M3 notes reveal-all), §9.2 **M2 entry requirements** (C1–C11), and §9.3 **later work with triggers** (Group D, B8, projectile pools, update skipping). | A, B, C, D |
| §10 | Units follow the `fl_view` snapshot; input goes to the `LocalDriver`. | A2, A9 |
| §11 | Rewritten around "state changes are commands": spawn panel and cheats issue `debug.*`; the player switcher and reveal-all are client-only; time controls belong to the driver; inspector reads go through `SimQuery`; save and load. | A10 |
| §12 | **Replaced** by the verification baseline: 12.1 the `xtask gate` levels table and hooks; 12.2 lints (generated clippy.toml, float_arithmetic, arch-check, cargo-deny); 12.3 determinism checks; 12.4 test layers (the old content kept and extended); 12.5 protected tests; 12.6 perf tiers; 12.7 contract snapshots. | B1–B6 |
| §13 | **Replaced**: 13.1 plans, tasks and review (old content kept; ≤ 2 builders); 13.2 ADRs, contract paths, deprecation and OWNERS; 13.3 the agent environment; 13.4 local-only CI (B8); 13.5 early probes. | B7–B10 |
| §14 | SubApp question removed (decided). I32F32 noted as chosen. Task-pool contention, snapshot layout and channel crate added as builder's calls. | A2 |
| §15 | B: the journal is Transient, kinds come from a registry, entries are sorted per tick. C: `fl/…` renamed to `fl.*`; `world.*` is client-only; writes go through commands or the driver; `bevy_brp_extras` agent tools; offscreen screenshot; continuous update mode; readiness polling; new CLI verbs (`save`, `load`, `replay dump`). D: work counters as the gate; attribution includes client crates and both threads; counter budgets fail and time budgets warn; Tracy CSV; client profiling with a recorded snapshot stream. | A4, A10, B5, B10 |

Kept unchanged: §1, §6.1 examples and rules, §6.2–6.3 content, §7, the hard determinism and dependency rules (tightened,
never loosened), never copying Nival code or data, asset licensing, and §15's requirements text.

ADRs written: 13, all `accepted`, dated 2026-10-23, decision-makers "user (via baseline upgrade), architect".

| ADR | Decision |
|---|---|
| 0001 | Two crates per feature; object-safe `SimFeature`/`ClientFeature`; topological order |
| 0002 | Threaded `SimRunner`, `fl_view` snapshots, `TickDriver` |
| 0003 | Keyed counter-based RNG; stream key = name hash |
| 0004 | Stable-string registries; string tables; name-keyed hashing |
| 0005 | Phases, explicit sync points, spawn funnel, driver housekeeping, narrowed ordering |
| 0006 | Classification, save format, migrations, round-trip and branch tests |
| 0007 | Command envelope, postcard, `MatchHeader`, canonical rules hash |
| 0008 | Debug actions as commands; BRP `fl.*` only; offscreen screenshot |
| 0009 | `xtask gate`, hooks, lints, arch-check, cargo-deny, protected tests |
| 0010 | Work-counter perf gates; entity-mix scale target; wall clock at milestones |
| 0011 | API and format snapshots, `OWNERS.toml`, ADR rule, deprecation |
| 0012 | Hot reload policy |
| 0013 | Agent dev environment; local-only CI |

---

## 2. Impact on completed M0 code

Ordered by crate. "Contract" means the change touches a snapshot-protected API and needs the ADR it implements cited in
the commit.

### 2.1 Workspace and Task 1 artifacts

- **`.cargo/config.toml`:** delete `[build] target-dir = "C:/t/fl"` (the shared target dir is what serialised T12 and
  T7, ruling R18). Add `[target.x86_64-pc-windows-msvc] linker = "rust-lld.exe"` and `[alias] xtask = "run -p xtask --"`.
  The per-worktree `build.build-dir = "C:/t/fl/{workspace-path-hash}"` goes in the **user's** `~/.cargo/config.toml`
  (or `CARGO_BUILD_BUILD_DIR`), not the repo.
- **`.gitignore` ignores `/.claude/`.** That blocks committing the approved repo-local config (rules, agents, skill,
  `settings.json` hooks). Change it to ignore only `.claude/settings.local.json` and `.claude/worktrees/`. Add `/logs/`.
- **`Cargo.toml`:**
  - members: add `crates/fl_view`, `xtask`, `probes` (with `default-members` excluding `probes`), and later
    `features/*/sim` and `features/*/client`
  - workspace deps: add `insta`, `serde-reflection`, `cargo_metadata` (xtask only), optionally `crossbeam-channel`
  - dev profile: `debug = "line-tables-only"`, and `[profile.dev.package."*"] debug = false`
  - set `default-features = false` explicitly on `bevy`, `bevy_app` and `bevy_ecs`
- **`scripts/check_deps.sh`** → `xtask arch-check` (allowed-edges table plus `cargo tree -p` per package). Delete the
  script once the xtask lands, and update `README.md` (it cites the script and the old gate).
- **`clippy.toml` (root)** stays for client crates. Per-sim-crate files are generated by `xtask gen-clippy` (spec §12.2).
- **New files:** `deny.toml`, `OWNERS.toml`, `.config/nextest.toml`, `CLAUDE.md`, `.claude/{settings.json, rules/,
  agents/, skills/bevy-api/}`, `.mcp.json`, `rust-analyzer.toml`.

### 2.2 `fl_math` (Task 2, complete)

- `src/rng.rs`:
  - **Add `pub fn roll(seed: u64, tick: u32, sim_id: u32, stream: u64, n: u32) -> u64`.** Reuse the private
    `splitmix64`: fold `seed`, `(tick << 32) | sim_id`, `stream`, then `n`. Add reference-vector tests and port the
    "each input matters / tick and sim_id not interchangeable" tests from `derive_seed`.
  - **Add `pub struct Rolls { seed, tick, sim_id, stream, n }`** with `new`, `next_u32`, `next_u64`, `range_u32`
    (Lemire; each rejection consumes the next `n`), `chance`, `unit_fx` and `range_fx`. These mirror `Pcg32`'s methods
    so call sites port mechanically; port the `range_*`/`chance` tests.
  - **Add `pub const fn stream_key(name: &str) -> u64`**, a const hash of the bytes (FNV-1a 64 plus the splitmix
    finaliser; no new dependency), with fixed reference vectors. Its output is a contract: changing it moves every draw.
  - **Remove `derive_seed`.** `roll` supersedes it: the stream is a `u64` name key, and there is a draw counter. Update
    the re-exports in `src/lib.rs` and the crate docs ("the Pcg32 random number generator" → keyed RNG, with `Pcg32`
    for tools).
  - Keep `Pcg32` (tools, content time, client); sim crates' generated `clippy.toml` bans it.
- `src/lib.rs`: add `#![deny(clippy::float_arithmetic)]` with scoped `#[expect(clippy::float_arithmetic, reason = …)]`
  on the float edge functions (`Fx::from_f64`, `to_f32`/`to_f64`, `Angle::to_radians_f32`, the human-readable serde
  visitor) and on test modules that compare against f64.
- The Fx/Angle serde needs no change: the binary branch is already a raw `i64`/`u16` (ruling R5), which is exactly what
  the canonical rules hash and saves need.
- Contract: add a public-API snapshot.

### 2.3 `fl_protocol` (Task 3, complete)

- **`PlayerCommand`**: rename `tick` → `exec_tick`; add `seq: u32`. Update `player_command_postcard_roundtrip`.
  `CommandKind` is unchanged.
- **Add `pub trait CommandPayload: Serialize + DeserializeOwned + Debug + Send + Sync + 'static { const NAME: &'static
  str; const NEEDS_UNITS: bool = true; const DEBUG_ONLY: bool = false; }`.** This is T7's planned trait minus
  `unit_can`/`apply`, which become a sim-side `CommandHandler`.
- **Move the core payloads here** from T7's `fl_sim::commands`: `MoveCmd { target: FxVec2 }`, `StopCmd`,
  `DebugSpawnCmd { unit: UnitTypeId, pos: FxVec2, heading: Angle }`, `DebugWanderCmd { radius: Fx }`. Later, with the
  debug tools: `DebugSetOwnerCmd`, `DebugInvulnerableCmd`, `DebugSetHpCmd`, `DebugTeleportCmd`. This needs a new
  `fl_math` dependency in `crates/fl_protocol/Cargo.toml` (allowed: fl_protocol → fl_math).
- **Add `MatchHeader`** (spec §5.5), **`StringTables`** (named tables of names in dense-ID order) and
  `pub const FORMAT_VERSION: u32`.
- Consider `#[non_exhaustive]` on `Controller` and future reject enums.
- Lints: generated `clippy.toml` (full ban; no floats here). Contract: public-API and `serde-reflection` snapshots of
  `PlayerCommand`, `CommandKind`, the payloads and `MatchHeader`.

### 2.4 `fl_content` (Tasks 4–5, complete)

- **`src/hash.rs` `rules_hash`: rework.** Today it is xxh64 over RON text in which `Fx` serializes as `f64`. That is
  lossy for |v| ≥ 2^21 with fine fractions, and depends on RON float formatting, so it violates ADR-0007. Replace it
  with: a `RULES_HASH_FORMAT: u32` prefix, then **postcard** bytes of the same canonical tree (mods in load order;
  units `(id, name, tags, BTreeMap<&str, &dyn DefComponent>)`; rules-hashed kinds by extension then id; rules sections
  by name). `erased_serde` serializes the trait objects through postcard unchanged, and ruling R8's serde `rc` feature
  still applies.
  - New tests: two `Fx` values differing only in low bits at a large magnitude give different hashes (this fails
    today); source field order and whitespace don't matter (keep); a faction or rules change changes the hash (a
    deferred T5 test gap).
  - Hashed content types must not use `#[serde(flatten)]` (postcard needs known lengths).
- **`src/registry.rs` `DefRegistry`:** expose the frozen name lists (def components, object kinds, content kinds,
  rules sections; already sorted `BTreeMap`s) as `StringTables` entries. `fingerprint()` can become the hash of those
  tables. Make `register_*` after freeze panic (closes T6 minor "register_* after build silently lost").
- **`src/db.rs`:** export the unit-type and kind tables (factions, maps, …) as string tables for the header. Add an
  ID-set comparison (`ContentDb::same_id_sets(&other)`) for hot reload; that can wait for M1.
- **Presentation floats:** `Faction.color` and `MapDef.ground_color` are `f32` inside rules-hashed kinds. With the
  canonical encoding they hash as IEEE bits, deterministically, so nothing breaks. Optional cleanup: move them to a
  presentation kind so colour tweaks don't change the rules hash.
- Lints: `float_arithmetic` deny plus collection and time bans. There is no `f32` type ban here (spec §12.2), so the
  presentation fields need no `#[expect]`.
- Contract: the loader phases and registry are in the `contract` track.

### 2.5 `fl_sim` (Task 6, complete)

- **`src/rng.rs`:** delete `SimRng`, `SIM_RNG_STREAM` and `reseed`. Add a `MatchSeed(u64)` resource (Hashed, set from
  the header), a `StreamRegistry` (names → `stream_key`, panics on a duplicate name or key collision, exported as a
  string table) and a small helper to open `Rolls` for `(seed, clock.tick, sim_id, stream)`.
- **`src/schedule.rs`:**
  - replace `SimSet` (9 variants) and `SimTickEnd` with `Phase` (8 variants) plus per-phase hook-set enums (spec §5.6)
  - configure `(Phase::Input, …, Phase::Publish).chain()` with an explicit `ApplyDeferred` system at the end of each phase
  - set `ScheduleBuildSettings { ambiguity_detection: LogLevel::Error, auto_insert_apply_deferred: false, .. }`
  - keep the explicit `SingleThreadedExecutor`
  - move `ClearSimEvents` under `Input::Housekeeping`, and add Message rotation there
- **`src/hash.rs`:**
  - remove `record_state_hash` and `advance_clock` from `SimTick` (the `build()` `add_systems` call); the driver
    (`step_tick`) does both
  - `compute_state_hash` drops the `SimRng` state and hashes: tick, every Hashed resource by name (`MatchSeed`,
    `SimIdAllocator` per R15, `Players`, `MapInfo`, …), the entity count, then components as today
  - registry-backed IDs (`UnitTypeId` today, more later) must hash their stable name key, not the dense `u16`. That
    needs table access: give `StateHasher` a `&HashTables` context, or map IDs in the per-type hash function. The
    builder chooses; the spec fixes the rule.
  - `HashedComponents` becomes the general **state registry** (components and resources, class, codec, hash fn, dump
    fn, version)
  - `StateHashLog` leaves the World (driver-owned, reported in `TickOutput.hash`), or is classified `Transient` and
    drained per tick
- **`src/app_ext.rs`:** `SimAppExt for App` → a `SimBuilder` type (new `src/builder.rs`, contract) with
  `register_def_component`, `register_content_kind`, `register_rules_section`, `register_component::<C>(name, Class)`,
  `register_resource::<R>(name, Class)`, `register_command::<P>(handler)`, `register_stream(name)`,
  `register_journal_record::<R>()`, `add_sim_event::<E>()` (or view-channel registration), `add_systems(set, …)`, and
  `app_mut()`. `register_hashed` becomes `register_component(name, Class::Hashed { version })` and also requires
  `DeserializeOwned` (save codec). Add a generic `StableRegistry<K>` here for every registry.
- **`src/feature.rs`:** `GameFeature` → `SimFeature { id, version, deps, build(&mut SimBuilder) }`; drop `client()`.
  `CoreFeature` (`id "core"`, version 1) classifies: `SimEntity` (Hashed via id), `SimClock`, `SimIdAllocator`,
  `Players`, `MapInfo` and `MatchSeed` (Hashed); `SimIndex` (Derived); `Content` and config (External); `SimEvents<_>`,
  journal and counters (Transient).
- **`src/build.rs`:**
  - `assemble` sorts features topologically (`deps`, ties by id, `core` first), with errors for cycles, missing
    dependencies and duplicates (today it only checks duplicates)
  - `sim_registry` and `load_content` take `&[Arc<dyn SimFeature>]`
  - `SimConfig { seed, hash_interval, allow_debug_commands }` → a `MatchSetup { seed, cheats_allowed }` that goes into
    the `MatchHeader`, plus a run config for `hash_interval`
  - `build_sim_app` builds the header (features, mods, rules hash, string tables, `build_id`, `SIM_VERSION`), freezes
    registries after `finish()`, runs the classification check over `world.components()`, then initializes `SimTick`
    (as today)
  - add `pub const SIM_VERSION: u32`
- **`src/events.rs`:** `SimEvents<E>` is drained into `TickOutput.events` by the driver and cleared there. Event types
  migrate to `fl_view` (core) or the feature's `view` module. Classified Transient.
- **`src/ids.rs`:** keep `SimEntity` hooks and `SimIndex` (Derived; hooks rebuild it during load). Make
  `SimIdAllocator::alloc` `pub(crate)`: only the spawn funnel and load call it.
- **`src/clock.rs`:** `advance_clock` becomes a plain function called by `step_tick`, not a system.
- **`src/inspect.rs`:** iterate the state registry instead of `HashedComponents`; replace `"rng": {state, inc}` with
  `"seed"`.
- **`src/players.rs`, `src/map.rs`:** derive `Deserialize` and implement `SimHash` (Hashed resources need codecs and
  hashing).
- **`src/spatial.rs`:** no change now. T9's `SpatialCell` and grid register as Derived, with a rebuild system, in
  `Perceive::Spatial`.
- **New modules:**
  - `runner.rs`: `step_tick`, `SimRunner` (threaded and inline), `ReplayDriver`, `TestDriver`, and a loopback
    `LockstepDriver` (`LocalDriver` lives in `fl_client`)
  - `save.rs`: format, codec and migration registry, fitness-test helpers
  - a classification check (in `builder.rs` or its own module)
- **Tests:** `tests/schedule.rs` (asserts `SimSet` names and `SimTickEnd` ordering) and `tests/hash.rs` (hashes the
  RNG state, steps with `run_schedule`) must be rewritten against phases and `step_tick`.
- `Cargo.toml`: add `fl_view`. Keep `bevy_ecs` `debug` (needed for `shuffle_seed`, ruling R14).

### 2.6 Task 7 (in progress: `commands`, `spawn`, `units`, `movement`, `scenario`, `scale`, `tests/commands.rs`)

Its behaviour (validation, rejection reasons, movement math, scenario setup) survives. The changes are contract and
plumbing:
- **`commands.rs`:**
  - split `CommandPayload` into the data trait in `fl_protocol` (§2.3) and a sim-side `CommandHandler` (`unit_can`,
    `apply`) registered with `SimBuilder::register_command`
  - `CommandRegistry` keeps "ids by sorted NAME" (already planned) and exports a string table
  - `CommandQueue` is keyed by `exec_tick`, and the apply order becomes `(player, seq)`, not push order
  - `RejectReason` gains `DuplicateSeq`; `DebugDisabled` now keys off `cheats_allowed`
  - `submit()` becomes a `TestDriver`/runner API that stamps `exec_tick` and `seq`
  - the apply system moves to `Input::Apply`
- **`spawn.rs`:** `spawn_unit(world, …) -> Result<SimId, SpawnError>` becomes the spawn funnel: a `SpawnRequest`
  Message handled in `Lifecycle::Spawn` (request order, `SimId` allocation), plus a setup and load-only path that runs
  the same code outside `SimTick`. `despawn_unit` becomes a `DespawnRequest` handled in `Lifecycle::Despawn`. The
  `debug.spawn` handler writes a request, so the unit appears at the end of the tick. `debug_spawn_requires_flag` must
  step once before counting units.
- **`units.rs`:** register `Owner`, `UnitType`, `Pose`, `Health`, `Footprint`, `Mobility`, `MoveOrder`, `Wander` and
  `Selectable` with classes (Hashed v1; each derives `Serialize + Deserialize` and implements `SimHash`). `UnitType`
  hashes its stable name key.
- **`movement.rs`:** `wander_system` (→ `Think::Behaviour`) draws with
  `Rolls::new(seed, tick, sim_id, stream_key("debug.wander"))` instead of `SimRng.range_fx`. `move_system`
  (→ `Act::Move`) is entity-local, so it may drop `SimIndex` ordering (it is `par_iter_mut`-eligible later). The
  `MovementSystems` label is replaced by the `Act::Move` sub-set; T9 orders against `Perceive::Spatial`.
- **`scenario.rs`:** `SimRng::reseed(scenario.seed)` → the match seed comes from `MatchSetup` (the scenario's seed is
  the default). Units spawn in file order through the funnel's setup path. R16 (setup inserts `MapInfo`) stands.
- **`scale.rs`:** draw positions with `Rolls` on stream `"setup.scale"` instead of `Pcg32::new(seed, 7)`. The entity
  mix (6k followers, ~1k projectiles) can be a follow-up in the bench task rather than T7.
- **`tests/commands.rs`:** build with `&[Arc<dyn SimFeature>]` and `MatchSetup`; `PlayerCommand { exec_tick, seq, … }`;
  step with `step_tick`/`TestDriver` instead of `run_schedule(SimTick)`. Add a duplicate-seq case to
  `bad_commands_do_not_change_state`. `command_type_ids_sorted_by_name` stays valid.
- Fixtures under `tests/fixtures/` are content and need no change.
- **Suggested sequencing** (the planner's call): let T7 finish green against today's contracts, then apply the contract
  changes above in one rework task. Most T7 logic carries over, and a mid-task contract switch would cost more.

### 2.7 Other crates and in-flight work

- **`fl_client` (stubs; T12 camera in flight in `C:/fl/t12`):** the `Cargo.toml` drops `fl_sim` and adds `fl_view`.
  The `ClientFeature` trait lives here. T12 (camera, ground) is unaffected. T13's planned `SimHost` (a World in a
  resource, `world_mut()`) is superseded by `LocalDriver` plus the runner port and snapshot interpolation.
- **`fl_catalog` (stub, T8 pending):** `features() -> Vec<Arc<dyn SimFeature>>`.
- **`fl_cli`, `fl_test_utils`, `fl_game` (stubs):** no code to rework.
- **Plan-level effects for the planner** (I did not edit the plan):
  - the Global Constraints gate becomes `cargo xtask gate task`, and the plan's Global Constraints and determinism
    bullets need restating per spec §5.4 and §12
  - T11: `TestDriver`, twin run via `step_tick`, shuffle runs, the system-order snapshot, the branch and round-trip
    tests
  - T13: rewrite (runner, `LocalDriver`, snapshots)
  - T16: entity-mix scale scenario, work counters and `budgets.ron`
  - T18: replays as postcard with `MatchHeader`; journal kinds in a registry and sorted per tick; `SetTimings` over 8
    phases
  - T19: attribution also covers client crates and the runner thread
  - T20: `save`, `load`, `replay dump --json`
  - T21: `fl.*` names, offscreen screenshot, `bevy_brp_extras`, runner instead of `SimHost`
  - topology: at most two concurrent builders
  - new tasks: the `fl_view` crate, runner and drivers, the save/load skeleton, `xtask` (gate, arch-check, gen-clippy,
    test-guard, check-scope, bless, api snapshots), agent config (CLAUDE.md, rules, hooks, agents, skill, `.mcp.json`,
    nextest profile), `deny.toml` and `OWNERS.toml`, and the two probes

---

## 3. Conflicts the user should decide

Both come from one interaction: B8 (remote + 3-OS CI) was skipped, but other adopted items assumed a Linux or hosted
runner. Everything else in the baseline is a refinement the user already approved.

### D1. Cross-OS determinism verification (G3) has no runner

The spec promises a deterministic sim ready for lockstep across machines. B8 was the only item that verified golden
replays on a second OS and CPU architecture. With local-only CI, determinism is verified on Windows x64 only. Integer
`Fx` math makes cross-OS divergence unlikely, but it is unproven.
- **Option A (default if no answer):** accept Windows-only verification until the Group D trigger (a remote, or before
  the net crate). The risk is recorded in spec §13.4 and §9.3.
- **Option B:** install WSL2 with a Linux toolchain (a system setting, so the user does it), and have `gate nightly`
  and `gate milestone` run the sim crates' tests and `replay-verify` there. Linux x64 adds a second OS, not a second
  architecture.
- **Option C:** create a private remote for CI only (reverses the B8 decision in part).

### D2. The instruction-count perf tier (B5 tier 2) has no tool on Windows

B5 adopted three tiers. Tier 2 needs `gungraun` (Valgrind, Linux) or CodSpeed (hosted; needs a remote).
- **Option A (default if no answer):** run tiers 1 (work counters, the gate) and 3 (milestone wall clock) only; tier 2
  stays parked (spec §12.6, ADR-0010).
- **Option B:** with WSL2 (D1 option B), run `gungraun` on the scale scenario in `gate nightly` and trend it.
- **Option C:** CodSpeed with a remote (needs D1 option C).

**Not conflicts, but user actions or assumptions to confirm:**
- `cargo-public-api` needs a nightly toolchain for rustdoc JSON. I treated it as covered by the approval of
  cargo-public-api; builds stay on stable. Say so if not.
- System settings the spec leaves to the user: Dev Drive or Defender exclusions (`CARGO_HOME`, the repos, `C:/t/fl`),
  and a fixed power plan for milestone benches.
- The user-level Cargo config gets `build.build-dir` (§2.1). It is in the user's home, not the repo.

---

## 4. Open questions I resolved as architect

1. **Feature traits use methods, not associated consts.** The research sketched `const ID`/`const VERSION`, but consts
   make the trait non-object-safe, and `fl_catalog` returns `Vec<Arc<dyn …>>`. Each feature type also exposes
   `pub const ID` for `deps()` lists (ADR-0001).
2. **`fl_client` must not depend on `fl_sim`.** The runner protocol types (`TickInput`, `TickOutput`, `SimQuery`) live
   in `fl_view`, and `fl_game` wires the runner to the client. Otherwise "the client never touches sim types" can't be
   checked by `arch-check` (ADR-0002).
3. **Command payload data types live in `fl_protocol`** (core) or a feature's `protocol` module; handlers stay in the
   sim. Clients must encode commands without depending on sim internals, and the wire formats become snapshot-able
   (ADR-0007).
4. **An RNG stream key is the hash of its name, not a dense registry ID.** A dense ID would shift when a feature adds
   an earlier-sorting stream, recreating the coupling A3 removes (ADR-0003).
5. **The state hash writes registry-backed IDs as stable name keys.** The same reasoning applies to every registry: an
   unrelated feature must not change existing scenarios' hashes (change class N). Saves may store dense IDs, because
   they carry string tables (ADR-0004).
6. **A fourth class, `External`, for resources only** (content, config, cheats flag). A6 named three classes, but these
   resources are neither hashed per tick, derived, nor cleared; the header identifies them (ADR-0006).
7. **`step_tick` order:** inbox → `SimTick` → hash if due → take outputs → `clear_trackers` → advance clock. The hash
   stays "end of tick, before the clock advances", matching today's `StateHashLog` semantics and T20's
   `bisect-desync` expectations (ADR-0005).
8. **Pause, step and speed are driver controls, not `DebugCommand`s** (the research listed "pause-with-orders" among
   them). They don't change what a tick does, so recording them adds nothing to replays. A lockstep pause is a
   session-level message later (ADR-0008).
9. **M0's `LockstepDriver` uses an in-process loopback `NetSession`.** A2 asks for a Lockstep implementation, and the
   no-networking non-goal forbids sockets. Loopback proves the path (input delay 2, two sims hash-identical) without a
   transport (§2, §5.3).
10. **`build_id` is strict only for the handshake.** For replays it is a warning, and `sim_version` plus features, mods
    and rules hash decide. Otherwise golden replays would fail on every commit. `sim_version` is in the header from M0;
    its bump policy arrives with C8 (ADR-0007).
11. **Replays and saves are postcard; JSON is a debug view.** postcard keeps `Fx` lossless (raw `i64`) and matches the
    wire format. T18's planned `save_json` becomes an export (`fl_cli replay dump --json`).
12. **Canonical rules-hash encoding** = a format-version prefix plus postcard over the existing canonical tree. It
    reuses T5's tree with minimal rework and is lossless; presentation `f32` hashes as bits; `flatten` is forbidden in
    hashed types (ADR-0007).
13. **Clippy ban scope.** The full `f32`/`f64` type ban applies to `fl_protocol`, `fl_view`, `fl_sim` and
    `fl_feat_*`. `fl_math` and `fl_content` get `float_arithmetic` plus the collection and time bans, with scoped
    `#[expect]` only at conversion edges, because they legitimately carry floats at the boundary. The generator emits
    only resolvable paths, so an unused ban can't fail `-D warnings` (spec §12.2).
14. **Protected `scenarios/**` means the repo-root test-scenario directory.** Content scenarios under `assets/` stay
    editable, or content tasks (T8) couldn't author the testbed scenario (spec §12.5).
15. **Who blesses snapshots and baselines:** the lead (controller) session or the user, through
    `cargo xtask bless --reason`; implementer roles never. From M2, golden replays need the user (C8, C10). With no
    remote there is no CODEOWNERS, so the hook's role split replaces it (spec §12.5).
16. **The journal is sorted per tick by `(phase, kind, sim_id, emission order)`.** T18's plan lets unordered systems
    write the journal (`allow_ambiguous_resource`). Without a sort, the journal differs across shuffle seeds and
    journal-based assertions would flake (spec §15 B).
17. **Entity-mix composition for the M0 scale scenario:** 3,000 units, 6,000 followers and about 1,000 projectiles in
    flight. That lands in A12's 8–12k range and exercises cross-entity reads and spawn churn before real squads exist
    (spec §5.7, ADR-0010).
18. **Save/load lives in `fl_sim::save` for M0,** not a new `fl_save` crate. Codecs register alongside classification
    in `SimBuilder`; split it out later if ownership demands.
19. **BRP method names are `fl.*`** (dot), replacing the spec's `fl/…`, to match Bevy 0.20's `world.*` and `rpc.*`
    naming and A10's wording.
20. **The deprecation policy starts at the end of M0.** Contracts are still being written; deprecating M0-internal
    items mid-milestone would only add churn. Changes still go through ADRs (`docs/adr/README.md`).
21. **Interim event rule until the C7 ADR:** Messages for gameplay, hooks for indices only, no observers in sim crates.
    The event-model ADR is a Group C item, but M0 code needs a rule now (spec §5.4, §5.6).
22. **`Changed<T>` is allowed only in the `Publish` phase.** The snapshot needs change filtering, while gameplay must
    not depend on change ticks, which aren't hashed or saved (spec §5.3, §5.4).
23. **Hot-reload replays are marked non-reproducible** rather than storing reloaded content. That is the cheapest
    honest option; storing content snapshots is a later extension (ADR-0012).
24. **`bevy_brp_extras` joins `fl_client`'s `debug_remote` feature,** to publish `fl.*` as agent tools with
    descriptions. `rpc.discover` lists names only (ADR-0008).
25. **The scenario seed is the default match seed;** `MatchSetup` may override it. This keeps today's "scenario carries
    a seed" behaviour (T7 `reseed`) under the header model (spec §6.2).
