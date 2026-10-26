# Task 10 report: fl_sim eight phases, step_tick, keyed RNG, name-keyed state hash, counters

**Status:** DONE_WITH_CONCERNS. Commit `de05d31` on `m0/phase-a`. The concerns are listed at the end.

## What I implemented

- **`schedule.rs` (rewritten).**
  - `SimTick`, `Phase` (8 variants) and the 8 hook enums (`InputSet` … `PublishSet`). All of them
    derive `SystemSet + Copy + Ord`.
  - `SimHook: SystemSet + Copy + Sealed` with `phase()`. It is sealed, so the 8 enums are the
    only implementors.
  - `configure_sim_schedule(app, shuffle_seed)` sets up:
    - `SingleThreadedExecutor`;
    - `ScheduleBuildSettings { ambiguity_detection: Error, auto_insert_apply_deferred: false, shuffle_seed, .. }`;
    - the phases chained;
    - each phase's hooks chained inside it;
    - a private `PhaseSync(phase)` set after the last hook, holding that phase's one
      `ApplyDeferred`.
  - `system_order(app)` initializes `SimTick`. It walks each executed system up the hierarchy
    graph to its nearest hook or sync set and prints `"<Phase>/<Hook> <name>"`, or
    `"<Phase>/Sync bevy_ecs::apply_deferred"` for the sync points.
  - The `pub(crate) initialize_sim_tick` helper keeps the "SimTick schedule failed to build"
    panic.
  - Removed: `SimSet`, `SimTickEnd`, `ClearSimEvents`, `CommandSystems`, `MovementSystems`.
  - Current core order:
    ```
    Input/Housekeeping fl_sim::counters::clear_counters
    Input/Housekeeping fl_sim::events::clear_sim_events<fl_view::event::SimEvent>
    Input/Apply fl_sim::commands::apply_commands
    Input/Sync bevy_ecs::apply_deferred
    Think/Behaviour fl_sim::movement::wander_system
    Think/Sync bevy_ecs::apply_deferred
    Act/Move fl_sim::movement::move_system
    Act/Sync ... (one Sync line for each of the 8 phases)
    ```
- **`builder.rs`.**
  - `add_systems(hook: impl SimHook, systems)`.
  - `add_message::<M>()`:
    - inserts `Messages<M>`;
    - classifies it Transient;
    - adds `rotate_messages::<M>` (`Messages::update`) in `InputSet::Housekeeping`;
    - does nothing if `M` is already added.
- **`step.rs` (new).**
  - `step_tick(world, TickInput) -> TickOutput`. It panics with
    `"driver out of sync: input tick {a}, sim tick {b}"`. Then, in order:
    1. sorts the commands by `(player, seq)` and `submit`s them into the rev-1 `CommandQueue`;
    2. runs `SimTick`;
    3. computes the hash if `HashInterval > 0` and `tick % HashInterval == 0`;
    4. builds the `TickOutput`, with an empty snapshot (only the tick set) and empty
       events/journal (Tasks 12 and 18);
    5. runs `world.clear_trackers()`;
    6. advances the clock.
  - All of this runs inside a `sim_tick` info span with field `tick`.
  - `HashInterval(pub u32)`: External `"core.hash_interval"`.
- **`rng.rs`.** `SimRng` and `SIM_RNG_STREAM` are deleted. New: `MatchSeed(pub u64)`, Hashed
  `"core.match_seed"`, inserted from `header.seed`; and `rolls(seed, tick, sim_id, stream_key) -> Rolls`.
- **`counters.rs` (new).** `SimCounters` with `add`, `get` and `snapshot` (a `BTreeMap`, so
  sorted). It is Transient `"core.counters"`, cleared in `InputSet::Housekeeping`.
- **`hash.rs`.**
  - `StateHasher` gained an optional table context (`Arc<NameKeys>`: table → `stream_key` of
    each name, by dense id), and `write_registry_id(table, id)`.
    - `write_registry_id` panics when the hasher has no tables or the table lacks the id.
  - `compute_state_hash` writes, in order:
    1. the tick;
    2. each Hashed resource in name order: its name key, then `0`, or `1` + its data (see
       deviation 1);
    3. the entity count;
    4. per `SimIndex` entry: the `SimId`, then each Hashed component (`0`, or `1` + data).
  - `SimHash for UnitTypeId` now goes through `write_registry_id(tables::UNITS, ..)`.
  - The name keys are built once in `build_sim_app` from the header's string tables. They live
    in the crate-private resource `HashTables`, External `"core.hash_tables"`.
  - `StateHashLog` and `record_state_hash` are deleted.
- **`classify.rs`.** `ResourceEntry` now carries `codec: Option<Box<dyn ResourceCodec>>` for
  Hashed resources.
- **`build.rs`.**
  - `RunConfig` gains `shuffle_seed: Option<u64>`.
  - `build_sim_app` now:
    1. inserts `MatchSeed`, `HashInterval` (`run.hash_interval`, else the rule) and
       `HashTables`;
    2. calls `configure_sim_schedule(&mut app, run.shuffle_seed)`;
    3. initializes `SimTick`.
  - `CoreFeature` no longer configures the schedule.
- **System placement:**
  - `apply_commands` → `InputSet::Apply`.
  - `wander_system` → `ThinkSet::Behaviour`. It is entity-local (it queries `SimEntity`) and
    draws `rolls(seed, tick, sim_id, stream_key("debug.wander"))`, x then y.
  - `move_system` → `ActSet::Move`. It is entity-local and no longer walks `SimIndex`.
  - The `SimEvents` and `SimCounters` clears → `InputSet::Housekeeping`.
- **`scale.rs`.** Positions come from `Rolls::new(seed, 0, 0, stream_key("setup.scale"))`, x then
  y per unit, and `populate` inserts `MatchSeed(seed)`.
- **`commands.rs`.** `SimHash for CommandQueue` writes the command type as its name key, through
  a crate-private `write_unchecked_registry_id` (see deviation 2).
- **Other files.**
  - `inspect.rs`: `inspect_world` drops `"rng"`, and `"seed"` now reads `MatchSeed`.
  - `clock.rs`: `advance_clock` is gone; `step_tick` advances the clock.
  - `xtask/clippy-template.toml`: the `Pcg32` pending entry is gone. I regenerated the configs,
    and `crates/fl_sim/clippy.toml` now bans `fl_math::Pcg32`.
- **Carried items:**
  1. Name-keyed IDs and Hashed resources: done.
  2. `CommandQueue` hashes the name key: done.
  3. The stale `rng.rs` doc went with the file rewrite.
  4. Minor fixes:
     - The `FeatureError::Cycle` wording is now "features {0:?} cannot be ordered (dependency
       cycle)".
     - "classified twice" names both features ("as 'a.tag' by 'a' and as 'b.tag' by 'b'"), with
       a new test in `tests/classify.rs`.
     - `debug_spawn_resolves_the_unit_name` also asserts that nothing was rejected.

## Deviations and judgement calls (please review)

1. **A missing Hashed resource writes `0`, a present one `1` + data,** after its name key.
   `MapInfo` is Hashed but absent until a map is set up, so the brief's "name key, then data"
   needed an absent case. I used the same `0`/`1` scheme as for components.
2. **`CommandQueue` can hold a command with an unknown type id** (the `UnknownType` rejection
   happens at apply time), and `write_registry_id` would panic on it. `hash_command` therefore
   uses a crate-private `write_unchecked_registry_id`: `1` + name key, or `0` + the raw id for an
   unknown one. Task 11 replaces the queue.
3. **`add_message` names the buffer by `std::any::type_name::<Messages<M>>()`.** The brief's
   signature has no name parameter, and a namespaced name would mean leaking a formatted string.
   The name lands in the `sim_resources` string table. It is Transient, so it is never hashed or
   saved, but the text depends on the compiler.
4. **Extra core names:** `"core.hash_tables"` (External, the name keys the hash uses) and
   `"core.hash_interval"`, `"core.match_seed"` and `"core.counters"` as the brief specifies.
   `"core.rng"` and `"core.state_hash_log"` are gone.
5. **`setup_scenario_def` still re-seeds,** now by inserting `MatchSeed(def.seed)` instead of
   `SimRng::reseed`. I kept rev-1 behaviour because `scenario.rs` is Task 11's ("never
   reseeds"). It is not in my Files list but had to change once `SimRng` was deleted.
   `feature.rs` also changed (core registrations).
6. **`bad_commands_do_not_change_state` excludes `LastSeq` from its hash comparison.**
   `core.last_seq` is Hashed and now really hashed, and `apply_commands` raises it for rejected
   commands too (Task 9's choice). The test now also asserts `last_seqs == {P0: 4}`.
7. **`hooks_run_in_declared_order` gives its `Input/Apply` tracer `.ambiguous_with_all()`.** The
   core `apply_commands` is exclusive and its set (`CommandSystems`) is gone, so no feature system
   in `InputSet::Apply` can order against it. See concern 1.
8. **The xtask test `pending_entry_is_omitted_with_comment` relied on the real template's Pcg32
   pending entry.** I rewrote it on the inline `MINI` template (pending `f32` for `fl_sim`) and
   added `sim_bans_pcg32` for the real template. The test-guard flags the two removed asserts.
9. **Extra tests beyond the brief:**
   - `step_output_carries_the_tick`
   - `zero_interval_turns_hashing_off`
   - `output_hash_is_taken_before_the_clock_advances`
   - `shuffled_schedule_gives_the_same_hashes` (shuffle seeds 1 and 2 vs none: 50 ticks of
     per-tick hashes are equal)
   - unit tests `rolls_are_keyed_by_every_input`, `registry_ids_hash_as_name_keys`,
     `registry_id_without_tables_panics` and `counts_add_up_and_snapshot_is_sorted`

## Tests

### TDD evidence
- **RED: not captured.** I wrote the implementation before running the rewritten tests, so
  there is no failing run on record. Against `1dfc139` the new tests cannot compile: `step_tick`,
  `InputSet`, `SimCounters`, `system_order`, `MatchSeed`, `rolls`, `HashInterval` and
  `RunConfig::shuffle_seed` don't exist there. This is a process deviation.
- First test run after implementing (`cargo nextest run --profile agent -p fl_sim`): 88/90. The
  2 failures are deviations 6 and 7 above:
  - `bad_commands_do_not_change_state`: the hash differed because `LastSeq` is now hashed.
  - `hooks_run_in_declared_order`: the schedule failed to build with
    `apply_commands ... and tracer::{{closure}} ... conflict on: bevy_ecs::world::World`.
- GREEN: `cargo nextest run --profile agent -p fl_sim`
  ```
  Starting 91 tests across 7 binaries
  Summary [   5.116s] 91 tests run: 91 passed, 0 skipped
  ```
- `cargo nextest run --profile agent -p xtask clippy_gen`: 5 passed.
- `cargo clippy -p fl_sim --all-targets -- -D warnings`: clean, with the `Pcg32` ban active.
  The only warnings are the two existing `rand::rng`/`rand::random` unresolvable-path warnings
  from the generated config.

### Gate: `cargo xtask gate task --track contract --track sim-core` (`.gate-base` = c1f0224)

The test-guard findings are the expected port churn:
- the `SimRng` tests became keyed-rolls tests;
- the `StateHashLog` tests became `TickOutput.hash` tests;
- the `SimSet` order test became the hook order test;
- `NewSeedOverride` hits are the new `shuffle_seed` plumbing and explicit test seeds.

```
PASS fmt (1.3s)
PASS clippy-drift (1.9s)
PASS claude-md (0.0s)
PASS check-scope (0.3s)
PASS protected (0.3s)
WARN test-guard (2.6s): test-guard: 30 findings (reviewer must accept each)
    TEST-GUARD crates/fl_sim/src/build.rs:41 NewSeedOverride: pub shuffle_seed: Option<u64>,
    TEST-GUARD crates/fl_sim/src/build.rs:168 NewSeedOverride: configure_sim_schedule(&mut app, run.shuffle_seed);
    TEST-GUARD crates/fl_sim/src/hash.rs:375 NewShouldPanic: #[should_panic(expected = "has no string tables")]
    TEST-GUARD crates/fl_sim/src/rng.rs:41 NewSeedOverride: let seed = MatchSeed(9);
    TEST-GUARD crates/fl_sim/src/rng.rs:42 RemovedTest: #[test] fn reseed_restarts_the_sequence() {
    TEST-GUARD crates/fl_sim/src/rng.rs:47 NewSeedOverride: assert_ne!(base, draw(&MatchSeed(10), 3, 4, key));
    TEST-GUARD crates/fl_sim/src/rng.rs:48 RemovedAssert: assert_eq!(a.0.next_u32(), first);
    TEST-GUARD crates/fl_sim/src/rng.rs:49 RemovedAssert: assert_eq!(SimRng::from_seed(3).0, Pcg32::new(3, 0xF1));
    TEST-GUARD crates/fl_sim/src/schedule.rs:170 NewSeedOverride: pub fn configure_sim_schedule(app: &mut App, shuffle_seed: Option<u64>) {
    TEST-GUARD crates/fl_sim/src/schedule.rs:177 NewSeedOverride: shuffle_seed,
    TEST-GUARD crates/fl_sim/tests/classify.rs:104 NewShouldPanic: #[should_panic(expected = "as 'a.tag' by 'a' and as 'b.tag' by 'b'")]
    TEST-GUARD crates/fl_sim/tests/hash.rs:72 NewSeedOverride: build_seeded(7, hash_interval)
    TEST-GUARD crates/fl_sim/tests/hash.rs:101 RemovedTest: #[test] fn rng_draw_changes_hash() {
    TEST-GUARD crates/fl_sim/tests/hash.rs:117 RemovedTest: #[test] fn hash_log_records_every_interval() {
    TEST-GUARD crates/fl_sim/tests/hash.rs:123 RemovedAssert: assert_eq!(ticks, [Tick(0), Tick(5), Tick(10), Tick(15)]);
    TEST-GUARD crates/fl_sim/tests/hash.rs:129 RemovedAssert: assert_eq!(app.world().resource::<StateHashLog>().interval, 20);
    TEST-GUARD crates/fl_sim/tests/hash.rs:133 RemovedAssert: assert_eq!(ticks, [Tick(0), Tick(20)]);
    TEST-GUARD crates/fl_sim/tests/hash.rs:139 NewSeedOverride: let mut a = build_seeded(7, None);
    TEST-GUARD crates/fl_sim/tests/hash.rs:140 NewSeedOverride: let mut b = build_seeded(8, None);
    TEST-GUARD crates/fl_sim/tests/hash.rs:214 RemovedAssert: assert_eq!( world["rng"], serde_json::json!({ "state": state, "inc": inc }) );
    TEST-GUARD crates/fl_sim/tests/hash.rs:222 NewSeedOverride: seed: 3,
    TEST-GUARD crates/fl_sim/tests/movement.rs:359 RemovedAssert: assert_eq!(world.resource::<fl_sim::SimClock>().tick, Tick(0));
    TEST-GUARD crates/fl_sim/tests/movement.rs:454 NewSeedOverride: fn scale_hashes(shuffle_seed: Option<u64>) -> Vec<Option<u64>> {
    TEST-GUARD crates/fl_sim/tests/movement.rs:459 NewSeedOverride: shuffle_seed,
    TEST-GUARD crates/fl_sim/tests/schedule.rs:76 RemovedTest: #[test] fn sets_run_in_declared_order() {
    TEST-GUARD crates/fl_sim/tests/schedule.rs:80 RemovedAssert: assert_eq!( app.world().resource::<Trace>().0, [ "ApplyCommands", "Orders", "Behaviour", "Movement", "Combat", "Damage", "Vision", "Lifecycle", "Hash", ] );
    TEST-GUARD crates/fl_sim/tests/schedule.rs:96 RemovedTest: #[test] fn clock_advances_per_tick() {
    TEST-GUARD crates/fl_sim/tests/schedule.rs:128 NewShouldPanic: #[should_panic(expected = "driver out of sync")]
    TEST-GUARD xtask/src/clippy_gen.rs:376 RemovedAssert: assert!(!sim.contains("\"fl_math::Pcg32\""), "{sim}");
    TEST-GUARD xtask/src/clippy_gen.rs:377 RemovedAssert: assert!( sim.contains("# pending: fl_math::Pcg32 until Task 10"), "{sim}" );
PASS clippy (4.4s)
PASS arch-check (4.5s)
PASS test-headless (21.2s)
SKIP test-client: no client package touched since c1f0224 (.gate-base)
SKIP data-check: crates/fl_cli/src/data_check.rs does not exist yet
gate task: PASS in 37s
```

The `movement.rs:359` "RemovedAssert" is the same assert with the path shortened to `SimClock`.
The `hash.rs` `RemovedAssert`s at lines 123, 129 and 133 live on as `hash_every_interval` and
`hash_interval_defaults_to_rules`, now on `TickOutput.hash` and `HashInterval`.

## Files changed
- Created:
  - `crates/fl_sim/src/{step,counters}.rs`
  - `crates/fl_sim/tests/fixtures/core_mod_extra/**`: a copy of `core_mod` plus
    `units/aaa.unit.ron`, id `core_test.aaa_first`
- Modified:
  - `crates/fl_sim/src/{schedule,hash,rng,clock,events,builder,build,movement,scale,commands,inspect,lib,classify,feature,scenario}.rs`
  - `crates/fl_sim/tests/{schedule,hash,movement,commands,classify}.rs`
  - `xtask/clippy-template.toml`, `xtask/src/clippy_gen.rs` (test), and the regenerated
    `crates/fl_sim/clippy.toml`
- Unchanged though listed: `units.rs`. The `UnitType` hash goes through `SimHash for UnitTypeId`
  in `hash.rs`, so `units.rs` needed no edit.

## Concerns
1. **`apply_commands` is an exclusive system in a public hook.** Any feature system added to
   `InputSet::Apply` fails the schedule build, because nothing can be ordered against it now that
   `CommandSystems` is gone. Task 11, which reworks command apply, should either keep it alone in
   `Apply` (and say so in the docs) or expose a set for ordering.
2. **Process: no RED run before implementing** (see TDD evidence above).
3. **`TickOutput.snapshot` is a fresh empty `Arc<SimSnapshot>` every tick** until Task 12, and
   `events` stays empty: `SimEvents` is still read from the world, as in rev 1.
4. Deviations 3 and 5 are judgement calls the reviewer should confirm.

## Fix round 1 (rulings R46, R47): commit `296046f`

### What I changed
- **R46: core and feature slots per hook.**
  - `schedule.rs` adds a private `HookSlot { hook, core }` set. Each hook gets a core slot and
    a feature slot, with the feature slot ordered after the core slot. The crate-private
    `hook_slot(hook, core)` names a slot.
  - `SimBuilder::add_systems` puts systems in the core slot when the feature being built is
    `CoreFeature`, and in the feature slot otherwise.
  - Removed the `.ambiguous_with_all()` workaround from `hooks_run_in_declared_order`.
  - `system_order` keeps its format, and core systems are listed first within a hook. Its label
    lookup already walks up through the parent sets, so it needed no change.
- **R47: named message buffers.**
  - The signature is now `add_message::<M>(name: &'static str)`. The `Messages<M>` buffer is
    classified Transient under `name`.
  - Duplicates are caught by a builder-owned `BTreeSet<TypeId>`. A second add of the same type
    is a no-op, whatever name it passes.
  - Rotation always goes in the core slot of `InputSet::Housekeeping`, even for a feature's
    message, so a feature system that writes `M` in Housekeeping never conflicts with the
    rotation.
- **ADR-0005:** added a "Refinements (2026-10-23)" section recording R46 and R47.
- **New tests in `tests/schedule.rs`:**
  - `feature_systems_run_after_the_core_systems_of_their_hook`:
    - A feature system in `InputSet::Apply` reads `CommandLog` and already sees the tick-0
      command rejected by the core apply (`UnknownPlayer`).
    - A feature system in `InputSet::Housekeeping` adds a counter, and the count survives the
      core clear that runs before it.
    - `system_order` lists `apply_commands` first among the `Input/Apply` lines.
  - `message_buffers_are_named_without_rust_paths`: `sim_resources` contains
    `message.msg.ping` but not the duplicate's name, and no name contains `::` or `<`.

### Tests
- `cargo nextest run --profile agent -p fl_sim`: `Summary [ 4.769s] 93 tests run: 93 passed, 0 skipped`.
- **Gate:** `cargo xtask gate task --track contract --track sim-core` passed. The 30 test-guard
  findings are the same as in round 1; only the line numbers in `schedule.rs` and
  `tests/schedule.rs` moved.
  ```
  PASS fmt (3.1s)
  PASS clippy-drift (2.0s)
  PASS claude-md (0.0s)
  PASS check-scope (0.3s)
  PASS protected (0.3s)
  WARN test-guard (2.4s): test-guard: 30 findings (reviewer must accept each)
      TEST-GUARD crates/fl_sim/src/build.rs:41 NewSeedOverride: pub shuffle_seed: Option<u64>,
      TEST-GUARD crates/fl_sim/src/build.rs:168 NewSeedOverride: configure_sim_schedule(&mut app, run.shuffle_seed);
      TEST-GUARD crates/fl_sim/src/hash.rs:375 NewShouldPanic: #[should_panic(expected = "has no string tables")]
      TEST-GUARD crates/fl_sim/src/rng.rs:41 NewSeedOverride: let seed = MatchSeed(9);
      TEST-GUARD crates/fl_sim/src/rng.rs:42 RemovedTest: #[test] fn reseed_restarts_the_sequence() {
      TEST-GUARD crates/fl_sim/src/rng.rs:47 NewSeedOverride: assert_ne!(base, draw(&MatchSeed(10), 3, 4, key));
      TEST-GUARD crates/fl_sim/src/rng.rs:48 RemovedAssert: assert_eq!(a.0.next_u32(), first);
      TEST-GUARD crates/fl_sim/src/rng.rs:49 RemovedAssert: assert_eq!(SimRng::from_seed(3).0, Pcg32::new(3, 0xF1));
      TEST-GUARD crates/fl_sim/src/schedule.rs:197 NewSeedOverride: pub fn configure_sim_schedule(app: &mut App, shuffle_seed: Option<u64>) {
      TEST-GUARD crates/fl_sim/src/schedule.rs:204 NewSeedOverride: shuffle_seed,
      TEST-GUARD crates/fl_sim/tests/classify.rs:104 NewShouldPanic: #[should_panic(expected = "as 'a.tag' by 'a' and as 'b.tag' by 'b'")]
      TEST-GUARD crates/fl_sim/tests/hash.rs:72 NewSeedOverride: build_seeded(7, hash_interval)
      TEST-GUARD crates/fl_sim/tests/hash.rs:101 RemovedTest: #[test] fn rng_draw_changes_hash() {
      TEST-GUARD crates/fl_sim/tests/hash.rs:117 RemovedTest: #[test] fn hash_log_records_every_interval() {
      TEST-GUARD crates/fl_sim/tests/hash.rs:123 RemovedAssert: assert_eq!(ticks, [Tick(0), Tick(5), Tick(10), Tick(15)]);
      TEST-GUARD crates/fl_sim/tests/hash.rs:129 RemovedAssert: assert_eq!(app.world().resource::<StateHashLog>().interval, 20);
      TEST-GUARD crates/fl_sim/tests/hash.rs:133 RemovedAssert: assert_eq!(ticks, [Tick(0), Tick(20)]);
      TEST-GUARD crates/fl_sim/tests/hash.rs:139 NewSeedOverride: let mut a = build_seeded(7, None);
      TEST-GUARD crates/fl_sim/tests/hash.rs:140 NewSeedOverride: let mut b = build_seeded(8, None);
      TEST-GUARD crates/fl_sim/tests/hash.rs:214 RemovedAssert: assert_eq!( world["rng"], serde_json::json!({ "state": state, "inc": inc }) );
      TEST-GUARD crates/fl_sim/tests/hash.rs:222 NewSeedOverride: seed: 3,
      TEST-GUARD crates/fl_sim/tests/movement.rs:359 RemovedAssert: assert_eq!(world.resource::<fl_sim::SimClock>().tick, Tick(0));
      TEST-GUARD crates/fl_sim/tests/movement.rs:454 NewSeedOverride: fn scale_hashes(shuffle_seed: Option<u64>) -> Vec<Option<u64>> {
      TEST-GUARD crates/fl_sim/tests/movement.rs:459 NewSeedOverride: shuffle_seed,
      TEST-GUARD crates/fl_sim/tests/schedule.rs:76 RemovedTest: #[test] fn sets_run_in_declared_order() {
      TEST-GUARD crates/fl_sim/tests/schedule.rs:80 RemovedAssert: assert_eq!( app.world().resource::<Trace>().0, [ "ApplyCommands", "Orders", "Behaviour", "Movement", "Combat", "Damage", "Vision", "Lifecycle", "Hash", ] );
      TEST-GUARD crates/fl_sim/tests/schedule.rs:96 RemovedTest: #[test] fn clock_advances_per_tick() {
      TEST-GUARD crates/fl_sim/tests/schedule.rs:127 NewShouldPanic: #[should_panic(expected = "driver out of sync")]
      TEST-GUARD xtask/src/clippy_gen.rs:376 RemovedAssert: assert!(!sim.contains("\"fl_math::Pcg32\""), "{sim}");
      TEST-GUARD xtask/src/clippy_gen.rs:377 RemovedAssert: assert!( sim.contains("# pending: fl_math::Pcg32 until Task 10"), "{sim}" );
  PASS clippy (31.7s)
  PASS arch-check (5.1s)
  PASS test-headless (160.2s)
  SKIP test-client: no client package touched since c1f0224 (.gate-base)
  SKIP data-check: crates/fl_cli/src/data_check.rs does not exist yet
  gate task: PASS in 206s
  ```

### Notes
- `CommandLog` is classified Transient but is never cleared, so it accumulates over the match.
  This is unchanged from Task 9, and the new test relies only on the tick-0 entry. Task 11
  should either clear it in Housekeeping or reclassify it.
