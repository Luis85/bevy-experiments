# Task 6 report: `fl_sim` skeleton

**Status:** DONE_WITH_CONCERNS (concerns are follow-ups, not defects in this task)
**Commit:** `ccae65d feat(fl_sim): SimTick schedule, SimId index, clock, RNG, state hash, GameFeature`

## What was implemented

All files from the brief live under `crates/fl_sim/src`, plus a small `inspect.rs`, which the brief allows.

- **schedule.rs.** `SimTick` and `SimSet` (9 sets, chained). `configure_sim_schedule` sets up `SingleThreadedExecutor` and `ambiguity_detection: LogLevel::Error`.
  - 0.20 has no `ExecutorKind`/`set_executor_kind`, so it uses `set_executor(SingleThreadedExecutor::new())`.
  - New public set `SimTickEnd`, inside `SimSet::Hash`, holds `record_state_hash → advance_clock`. Both are exclusive and chained, with the hash first.
  - It also configures `ClearSimEvents` inside `SimSet::ApplyCommands`.
- **clock.rs.** `SimClock { tick }` and the exclusive `advance_clock`.
- **ids.rs.**
  - `SimEntity(SimId)` is `#[component(immutable, on_insert, on_discard)]`, and its hooks keep `SimIndex` in sync. `SimIndex` is a sorted `Vec<(SimId, Entity)>` and panics on a duplicate id.
  - `SimIdAllocator` hands out ids starting at 1.
- **rng.rs.** `SimRng(Pcg32)` on stream `SIM_RNG_STREAM = 0xF1`, with `from_seed` and `reseed`.
- **hash.rs.**
  - `StateHasher` is a streaming xxh64 with seed 0 over little-endian bytes.
  - `SimHash` has impls for every type the brief lists.
  - A private `HashedComponents` resource holds `(name, fn(&World, Entity, &mut StateHasher) -> bool, json dump fn)`, sorted by name. A duplicate name or type panics.
  - `compute_state_hash` covers the tick, then the rng `(state, inc)`, then the entity count, then each SimIndex entry: id, then for every registered component a `0`, or a `1` followed by its data.
  - Also here: `StateHashLog` (interval 0 = off), `StateHashRules` (section "StateHash"), and `record_state_hash`, which logs at debug level with the tick.
- **events.rs.** `SimEvents<E>` with `push`, `drain`, `iter`, `len` and `is_empty`, plus the `ClearSimEvents` set and the clear system.
- **app_ext.rs.** `SimAppExt` provides `register_def_component`, `register_content_kind`, `register_rules_section`, `register_hashed` and `add_sim_event`.
  - The registry is the `DefRegistry` resource. It is created on first use with `DefRegistry::with_builtins()`.
  - `add_sim_event` does nothing if that event type is already added.
- **feature.rs.** `GameFeature` and `CoreFeature` (id "core"). `CoreFeature` adds the schedule, clock, ids, `SimRng::from_seed(0)`, the hash, `Players` and `SpatialPlugin`.
- **build.rs.** `SimConfig`, `Content`, `sim_registry`, `load_content` and `build_sim_app`.
  - The sim uses `App::empty()`: no Main schedule and no `AppTypeRegistry`.
  - A duplicate feature id panics.
  - `build_sim_app`:
    - checks the fingerprint against the assembled app's own registry (same result as `sim_registry(features)`, without building twice)
    - removes `DefRegistry` from the sim world, since `Content` carries the registry
    - inserts `SimRng`, `StateHashLog`, `Content` and `SimConfig`
    - calls `finish()` and `cleanup()`
    - then builds `SimTick` immediately, panicking with "SimTick schedule failed to build: …". Ambiguity errors therefore surface at build time rather than on the first tick.
- **players.rs, map.rs, spatial.rs.**
  - `PlayerInfo`/`Players` with `get`.
  - `MapInfo` with `flat` (cell counts round up), `contains` (half-open) and `clamp` (into `[0, size − EPSILON]`).
  - `SpatialRules` (section "Spatial") and `SpatialPlugin`.
- **inspect.rs.**
  - `inspect_entity` returns `{"sim_id", "components": {name: value}}`.
  - `inspect_world` returns `{"tick", "rng": {"state","inc"}, "state_hash", "entities": [sim ids], "players", "map" (null if no MapInfo)}`.
- **Task 7 stubs.** `commands`, `spawn`, `units`, `movement`, `scenario` and `scale` contain a doc comment only.
- **lib.rs.** Declares every module and sets `#![deny(clippy::float_arithmetic)]`.
- **Fixture** `tests/fixtures/core_mod/{mod.ron, rules.ron}`, exactly as in the brief.
- **Ruling.**
  - Workspace `bevy_ecs = { version = "=0.20.0", features = ["trace"] }`.
  - `serde_json = "1"` added to the workspace and to fl_sim.
  - Cargo.lock gained two lines: `tracing` under bevy_ecs and `serde_json` under fl_sim.

## Tests

- `tests/schedule.rs` (6):
  - sets_run_in_declared_order
  - clock_advances_per_tick
  - sim_index_tracks_spawn_and_despawn
  - ambiguous_systems_fail_schedule_build: should_panic, expected "SimTick schedule failed to build"
  - feature_registers_def_component
  - sim_events_cleared_each_tick
- `tests/hash.rs` (9):
  - equal_worlds_hash_equal
  - component_change_changes_hash
  - rng_draw_changes_hash
  - tick_changes_hash
  - hash_log_records_every_interval
  - hash_interval_defaults_to_rules
  - content_fingerprint_mismatch_panics: should_panic, expected "fingerprint"
  - inspect_entity_dumps_registered_components
  - inspect_world_summarizes_state
- Unit tests (11):
  - ids: first id is 1, the index is sorted, replace/remove re-index, a duplicate panics
  - map: flat, contains, clamp
  - hash: the hasher is xxh64 over LE bytes, Option and Vec encodings are distinct, Fx hashes its raw bits
  - rng: reseed

### TDD evidence

- **RED.** `cargo test -p fl_sim`, with only the tests written and `lib.rs` still empty, failed as expected because the API did not exist yet:
  - `error[E0432]: unresolved imports fl_sim::GameFeature, fl_sim::SimAppExt, fl_sim::SimClock, …` (both test targets)
  - `error: could not compile fl_sim (test "hash") / (test "schedule")`
- **GREEN.** `cargo test -p fl_sim` gave `11 passed` (unit), `9 passed` (hash) and `6 passed` (schedule).
- I checked that the ambiguity test panics for the right reason. The message was:
  `SimTick schedule failed to build: 1 pairs of systems with conflicting data access have indeterminate execution order … (in set Orders) and … (in set Orders)`

### Gate (from C:\fl\a, before commit)

- `cargo fmt --check`: OK
- `cargo clippy --workspace --all-targets -- -D warnings`: Finished, no warnings
- `cargo test --workspace`: all pass, 0 warnings in the log
  - fl_content 1+24+23
  - fl_math 46+4
  - fl_protocol 4
  - fl_sim 11+9+6
- `bash scripts/check_deps.sh`: `deps OK`

## Deviations and decisions (please review)

1. **SimEntity hooks are `on_insert`/`on_discard`, not `on_add`/`on_remove`.** Re-inserting an immutable `SimEntity` on the same entity fires only discard and insert. With add/remove hooks the index would go stale. The hooks also fire on despawn, and the `replacing_and_removing_the_id_updates_the_index` test covers this.
2. **New public set `SimTickEnd` (in `SimSet::Hash`).** The hash and clock systems are exclusive, so any other system in `SimSet::Hash` is ambiguous with them unless it is ordered. Such systems must be `.before(SimTickEnd)`, as the trace test's Hash system is.
3. **New public set `ClearSimEvents` (in `ApplyCommands`).** Task 7's `apply_commands` must be `.after(ClearSimEvents)`.
4. **`build_sim_app` initializes the `SimTick` schedule** so schedule errors panic at build time.
5. **`inspect_world["entities"]` is the list of SimIds** (its length is the count).
6. The `bool` returned by the per-component hash fn is not used yet. The fn also writes the `0`/`1` presence marker itself.
7. **Small extras.**
   - `SimRng::from_seed` and the `SIM_RNG_STREAM` const
   - `StateHashLog::new` and Default
   - `SimEvents::len`/`is_empty`
   - a panic on a duplicate feature id

## Concerns

- **System names are hidden without bevy_ecs's `debug` feature.** Ambiguity errors print `<Enable the debug feature to see the name>`. Spec §15 D attributes profiler time by the module path of the system name, so Task 18–21 (profiler) will almost certainly need `bevy_ecs/debug` enabled too, at least in dev or profiling builds. The ruling covered only `trace`, so I did not enable `debug`.
- **Enabling `trace` on the workspace `bevy_ecs` forces a full rebuild of the Bevy tree.** A cold `cargo test --workspace` takes more than 10 minutes; the first attempt hit my `timeout 590` (rustc exit 143). It is not a failure, but other agents' first gate run after this commit will be slow.

## Ruling R14 follow-up: bevy_ecs `debug` feature

- **Change.** The workspace dependency is now `bevy_ecs = { version = "=0.20.0", features = ["trace", "debug"] }`. Cargo.lock gained one line, `rand` under bevy_ecs. No fl_sim sources changed.
- **fl_sim's tree.**
  - `cargo tree -p fl_sim -e normal` now includes `rand` and `rand_core`, pulled in by `bevy_ecs/debug`.
  - No banned crates are present: no bevy umbrella, bevy_render, bevy_window, bevy_input, bevy_time, bevy_winit or wgpu.
  - Sim code does not use `rand`. All randomness still goes through `SimRng`.
- **Verification.**
  - `cargo test -p fl_sim`: 11 + 9 + 6 passed, EXIT=0
  - `cargo clippy -p fl_sim --all-targets -- -D warnings`: Finished, no warnings
  - `bash scripts/check_deps.sh`: `deps OK`
  - `cargo fmt --check`: OK
- **System names are now readable.** The ambiguity panic shows `<AmbiguousFeature as GameFeature>::sim::{{closure}} (in set Orders)` and no longer prints "<Enable the debug feature …>".
