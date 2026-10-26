# Task 9 report: fl_sim SimFeature + SimBuilder, stable registries, classification, command handlers, MatchHeader

**Status:** DONE_WITH_CONCERNS (see Concerns). Commit `1dfc139` on `m0/phase-a`.

## What I implemented

- `src/registry.rs` (new): `StableRegistry<V>`. It keeps names sorted on insert, so after `freeze()` a
  name's index is its dense id. It precomputes `stream_key(name)` per name. A duplicate panics with
  `'<name>' registered by '<a>' and '<b>' in registry '<table>'`; registering after freeze panics with
  `registry '<table>' is frozen: ...`. `freeze()` also panics when two names share a 64-bit key.
  Dense-id accessors (`id`, `name`, `name_key`, `get_by_id`, `iter`, `export`) panic before freeze;
  `get(name)` works at any time. I also added `table()`, `len()` and `is_empty()`.
- `src/feature.rs`: `SimFeature` (with `id`, `version`, `deps` and `build`), `FeatureError`
  (thiserror, `PartialEq`) and `sort_features`. Sorting uses Kahn's algorithm with a `BTreeSet` ready
  set and puts `core` first. A dependency on `"core"` always counts as met. Errors are checked in this
  order: Reserved/Duplicate, then MissingDep, then Cycle (the leftover ids, sorted).
  `CoreFeature` (`ID = "core"`, version 1) registers the core resources and streams, then calls the
  per-module `build(sim)` functions in `hash`, `spatial`, `units`, `commands` and `movement`.
- `src/classify.rs` (new): `Class<T>`, `Codec<T>` and `check_classification`.
  - `Codec<T>` has pub fn-pointer fields: `hash`, `encode` (postcard), `decode` and `dump` (JSON).
  - `Class::hashed(version)` builds the codec.
  - The crate-private `StateRegistry` resource holds two `StableRegistry`s (`sim_components`,
    `sim_resources`) and a `TypeId -> name` map. It rejects a type classified twice and a component
    registered as External ("External is for resources").
  - `check_classification` walks `world.components().iter_registered()`. In Bevy 0.20 resources are
    components too (`Resource: Component`, verified in `bevy_ecs-0.20.0/src/resource.rs:87`), so this
    covers both. It flags types whose outermost path is in `fl_sim::` or `fl_feat_*` (but not a
    `*_client` crate) and that are not classified.
- `src/builder.rs` (new): `SimBuilder` with the brief's API. It owns the `DefRegistry`, the command
  `StableRegistry`, the `StateRegistry` and the stream registry. `finish()` freezes all of them,
  exports the string tables, inserts `CommandRegistry`, `StateRegistry` and the **frozen**
  `DefRegistry` as resources, then runs `app.finish()` and `app.cleanup()`.
- `src/build.rs`:
  - New items: `MatchSetup`, `RunConfig`, `BuildError` (`From` for `FeatureError` and
    `ContentErrors`), `SIM_VERSION = 1`, `build_id()` and `MatchHeaderRes`.
  - `sim_registry`, `load_content` and `build_sim_app` now take `&[Arc<dyn SimFeature>]` and return
    `Result`.
  - The registry fingerprint mismatch is now `Err(RegistryMismatch)` instead of a panic.
  - The header combines the content string tables with every registry table.
- `src/commands.rs`:
  - The `CommandPayload` trait is gone (the `fl_protocol` one is used).
  - New: `CommandHandler<P>` and `CommandCtx` (gains `seq`).
  - `CommandRegistry` wraps the frozen `StableRegistry<CommandEntry>`; each entry has the payload
    `TypeId` and an `Arc` runner closure.
  - The handlers `MoveHandler`, `StopHandler`, `DebugSpawnHandler` and `DebugWanderHandler` are
    private and keep the rev-1 behaviour. `debug.spawn` resolves the unit name through
    `ContentDb::unit_id`, spawns for `payload.owner`, and logs a failure instead of rejecting (as
    rev 1 did).
  - `DebugDisabled` now keys off `MatchHeaderRes.0.cheats_allowed`.
  - `CommandQueue` and `submit` stay (Task 11); `CommandQueue` is now serde + `SimHash`.
- Carried item (b): **`LastSeq(pub BTreeMap<PlayerId, u32>)`**, Hashed `"core.last_seq"`, and
  `last_seqs(world)`.
  - My brief didn't place it, so I used the plan's Task 11 shape.
  - `apply_commands` raises each player's entry to the max `seq` of every command it takes from the
    queue, before validation. Task 11 adds the `DuplicateSeq` rule on top.
- Carried item (a): the live sim's `DefRegistry` resource is the builder's registry, frozen in
  `finish()` and checked equal (fingerprint) to `content.registry()`. Registering after content load
  panics; this is tested by `schedule::registering_after_content_load_panics`.
  - `DefRegistry` isn't `Clone` and `ContentDb` owns its own copy behind an `Arc`, so the resource is
    the equivalent frozen registry, not the very same instance.
- Core registrations, all as in the brief:
  - Components:
    - Hashed v1: `core.owner`, `core.unit_type`, `core.pose`, `core.health`, `core.footprint`,
      `core.mobility`, `core.move_order`, `core.wander`, `core.selectable`.
    - Derived (no-op, hooks): `core.sim_entity`.
  - Resources:
    - Hashed: `core.clock`, `core.sim_id_allocator`, `core.players`, `core.map`, `core.rng`,
      `core.command_queue`, plus `core.last_seq`.
    - Derived: `core.sim_index`.
    - External: `core.content`, `core.match_header`, `core.def_registry`,
      `core.command_registry`, plus **`core.state_registry`**.
    - Transient: `core.command_log`, `core.sim_events`, plus **`core.state_hash_log`**.
  - Commands: `core.move`, `core.stop`, `debug.spawn`, `debug.wander`.
  - Streams: `debug.wander` and `setup.scale`.
- serde and `SimHash` added to `SimClock`, `SimIdAllocator`, `Players`/`PlayerInfo` (the
  controller is hashed as its postcard bytes because `Controller` is non-exhaustive), `MapInfo`,
  `SimRng`, `Footprint`, `MoveClass`, `Mobility` and `Selectable`.
- `inspect_entity` keys components by registry name. `inspect_world` adds `"seed"` and
  `"header": {features, mods, rules_hash}`; it keeps `"rng"` until Task 10 deletes `SimRng`.
- `spatial.rs`: `SpatialPlugin` became `pub(crate) fn build(sim)` (it was only a rules-section
  registration). `events.rs`: the core buffer is `SimEvents<fl_view::SimEvent>`.
  `add_sim_event` is gone with `SimAppExt`.
- Tests ported: `schedule`, `hash`, `commands` and `movement`. New: `tests/feature.rs`,
  `tests/classify.rs`, and unit tests in `src/classify.rs` (`unclassified_sim_type_fails_build`,
  `sim_types_are_recognised_by_outermost_path`, `hashed_codec_round_trips`). Other new tests:
  `commands::debug_spawn_resolves_the_unit_name`,
  `commands::last_seq_tracks_the_highest_seq_per_player`,
  `schedule::registering_after_content_load_panics` and `feature::build_id_is_dev_without_git_rev`.

## Deviations / judgement calls (please review)

1. **The classification check runs after `SimTick` is initialized,** not before as the brief's
   arrow order says. Built this way, it also catches sim types that only appear in system queries.
   Schedule build failures still panic first, as in rev 1.
2. **Three names not in the brief's core list** (needed by the strict check):
   - `core.state_registry` (External): the classification table itself.
   - `core.state_hash_log` (Transient, observe-only, until Task 10 moves the hash into `step_tick`).
   - `core.last_seq` (Hashed, carried item b).
3. **`compute_state_hash` keeps the rev-1 formula,** because Task 10 owns the new one. Hashed
   *components* now include `core.footprint`, `core.mobility` and `core.selectable`, which rev 1 did
   not hash. Resources are classified and have codecs, but are not yet hashed by name; Task 10 does
   that. The per-entry `StateClass` is stored but not read yet. It sits behind a scoped
   `#[expect(dead_code, reason = ...)]` on `StateClass`, `ComponentEntry` and `ResourceEntry` (read
   by Task 10/13).
4. `RunConfig` has one field, so clippy's `needless_update` rejects `..Default::default()` in
   `tests/hash.rs` today. The test writes `RunConfig { hash_interval }`.
5. The rev-1 `content_fingerprint_mismatch_panics` became `content_registry_mismatch_is_error`,
   because the brief makes it `BuildError::RegistryMismatch`.
6. I edited `src/clock.rs` and `src/ids.rs` (serde derives only), which are not in the Files list.
   Both are inside the `sim-core` track; `check-scope` passed.

## Tests

### TDD evidence
- RED: `cargo nextest run --profile agent -p fl_sim --test feature --test classify`, run before any
  implementation. It failed as expected because the new API did not exist yet:
  ```
  error[E0432]: unresolved imports `fl_sim::Class`, `fl_sim::MatchHeaderRes`, `fl_sim::MatchSetup`, `fl_sim::RunConfig`, `fl_sim::SimBuilder`, `fl_sim::SimFeature`, `fl_sim::check_classification`
  error[E0432]: unresolved imports `fl_sim::FeatureError`, `fl_sim::MatchHeaderRes`, `fl_sim::MatchSetup`, `fl_sim::RunConfig`, `fl_sim::SIM_VERSION`, `fl_sim::SimBuilder`, `fl_sim::SimFeature`, `fl_sim::StableRegistry`, `fl_sim::build_id`, `fl_sim::sort_features`
  error: could not compile `fl_sim` (test "classify") due to 1 previous error
  error: could not compile `fl_sim` (test "feature") due to 1 previous error
  ```
- GREEN: `cargo nextest run --profile agent -p fl_sim`
  ```
  Starting 73 tests across 7 binaries
  Summary [   3.833s] 73 tests run: 73 passed, 0 skipped
  ```
- `cargo clippy -p fl_sim --all-targets -- -D warnings`: clean. The two `clippy.toml` warnings about
  `rand::rng`/`rand::random` not resolving come from the generated config and are not mine.

### Gate: `cargo xtask gate task --track contract --track sim-core`

The test-guard findings are the expected port churn:
- the rev-1 `SimConfig`/`CommandRegistry` seal asserts moved into `StableRegistry`, which panics on
  freeze and duplicates;
- the fingerprint panic became `Err(RegistryMismatch)`;
- `Ping` became `SimEvent::Custom`;
- `"Hp"` became `"hp.hp"`;
- the new `should_panic` tests are the brief's own.

`NewSeedOverride` at `build.rs:156` is the existing `SimRng::from_seed(...)`, now fed from
`setup.seed` instead of `config.seed`.

```
PASS fmt (2.1s)
PASS clippy-drift (3.1s)
PASS claude-md (0.0s)
PASS check-scope (0.3s)
PASS protected (0.3s)
WARN test-guard (2.7s): test-guard: 17 findings (reviewer must accept each)
    TEST-GUARD crates/fl_sim/src/build.rs:40 RemovedAssert: assert!( ids.insert(feature.id()), "feature id '{}' is used twice", feature.id() );
    TEST-GUARD crates/fl_sim/src/build.rs:92 RemovedAssert: assert!( ours == theirs, "content was loaded with a different feature set: registry fingerprint {theirs:016x}, \ this sim's is {ours:016x}" );
    TEST-GUARD crates/fl_sim/src/build.rs:156 NewSeedOverride: app.insert_resource(SimRng::from_seed(setup.seed))
    TEST-GUARD crates/fl_sim/src/commands.rs:100 RemovedAssert: assert!( !self.sealed, "command {type_name} registered after build_sim_app assigned the ids" );
    TEST-GUARD crates/fl_sim/src/commands.rs:114 RemovedAssert: assert!( self.entries.len() < usize::from(u16::MAX), "too many command types" );
    TEST-GUARD crates/fl_sim/src/commands.rs:139 RemovedAssert: assert!( self.sealed, "command ids are assigned by build_sim_app; none exist yet" );
    TEST-GUARD crates/fl_sim/tests/classify.rs:54 NewShouldPanic: #[should_panic(expected = "External is for resources")]
    TEST-GUARD crates/fl_sim/tests/feature.rs:200 NewShouldPanic: #[should_panic(expected = "is frozen")]
    TEST-GUARD crates/fl_sim/tests/feature.rs:209 NewShouldPanic: #[should_panic(expected = "registered by 'test' and 'other'")]
    TEST-GUARD crates/fl_sim/tests/hash.rs:155 RemovedTest: #[test] #[should_panic(expected = "fingerprint")] fn content_fingerprint_mismatch_panics() {
    TEST-GUARD crates/fl_sim/tests/hash.rs:172 RemovedAssert: assert_eq!( first, serde_json::json!({ "sim_id": 1, "components": { "Hp": 100 } }) );
    TEST-GUARD crates/fl_sim/tests/schedule.rs:183 RemovedAssert: assert_eq!(sim_registry(&[]).component_name::<TestDef>(), None);
    TEST-GUARD crates/fl_sim/tests/schedule.rs:196 NewShouldPanic: #[should_panic(expected = "DefRegistry is frozen")]
    TEST-GUARD crates/fl_sim/tests/schedule.rs:214 RemovedAssert: assert_eq!(pings, [&Ping(Tick(0))]);
    TEST-GUARD crates/fl_sim/tests/schedule.rs:218 RemovedAssert: assert_eq!(pings, [&Ping(Tick(1))]);
    TEST-GUARD crates/fl_sim/tests/schedule.rs:225 RemovedAssert: assert_eq!(drained, [Ping(Tick(1))]);
    TEST-GUARD crates/fl_sim/tests/schedule.rs:226 RemovedAssert: assert!(app.world().resource::<SimEvents<Ping>>().is_empty());
PASS clippy (30.9s)
PASS arch-check (4.4s)
PASS test-headless (72.6s)
SKIP test-client: no client package touched since 01c7699 (.gate-base)
SKIP data-check: crates/fl_cli/src/data_check.rs does not exist yet
gate task: PASS in 117s
```

## Files changed
- Created: `crates/fl_sim/src/{registry,builder,classify}.rs`, `crates/fl_sim/tests/{feature,classify}.rs`
- Modified: `crates/fl_sim/src/{build,clock,commands,events,feature,hash,ids,inspect,lib,map,movement,players,rng,spatial,units}.rs`,
  `crates/fl_sim/tests/{commands,hash,movement,schedule}.rs`
- Deleted: `crates/fl_sim/src/app_ext.rs`
- Unchanged though listed: `scale.rs`, `scenario.rs`, `spawn.rs` (no API change was needed; Tasks
  10-11 rework them).

## Concerns
- **`probes/dynlink/src/main.rs` no longer compiles.** It uses `GameFeature`, `SimConfig` and the old
  `build_sim_app`. The gate excludes probes and the file is in the `probes` track, outside mine, so I
  left it. The probes owner should port it or delete it; it is a throwaway probe from Task 26.
- Deviations 1-3 above are design calls the reviewer should confirm, especially the extra core names
  and the classification check running after `SimTick` is initialized.
