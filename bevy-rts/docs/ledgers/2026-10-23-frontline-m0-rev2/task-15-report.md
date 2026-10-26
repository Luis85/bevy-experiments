# Task 15 report: incremental spatial grid

Implemented `SpatialGrid` (sorted cells, clamped `cell_of`, `query_radius` returning candidate count), `SpatialCell` (`on_remove` hook unfiles from the grid), `rebuild_spatial` (Derived rebuild for both `core.spatial_grid` and `core.spatial_cell`; run on load), `init_spatial` (called from `setup_map`, `setup_scenario_def` and `ScaleScenario::populate`), and `track_spatial` in `PerceiveSet::Spatial` (walks `SimIndex`, no `Changed`, counts `spatial.inserts`/`spatial.relocations`). Exports added to lib.rs.

Tests: `crates/fl_sim/tests/spatial.rs`, 8 tests incl. `position_on_max_edge_maps_to_last_cell`. RED not run separately (tests and impl written together; the stub had none of the API, so they could not compile). GREEN: 8/8; full `-p fl_sim` 159/159.

Gate: `cargo xtask gate task --track sim-core`
PASS fmt, clippy-drift, claude-md, check-scope, protected, test-guard, clippy, arch-check, test-headless (67.4s), data-check; test-client SKIP.
gate task: PASS in 114s

Note: `init_spatial` is also called in `setup_scenario_def` (it inserts MapInfo too). SpatialGrid derives PartialEq/Eq (for the load test). A freshly spawned entity enters the grid in the next Perceive.

## Fix round 1 (R53)
- `setup_spawn` (shared by the Lifecycle funnel) now calls `spatial::file_spawn`: inserts the id into the grid, adds `SpatialCell`, counts `spatial.inserts`. `track_spatial` stays for relocations (and as an insert fallback).
- Minors: `debug_assert!` on the hook's `grid.remove`; `cols()`/`rows()` added, tests use them; `is_empty` documented; `core.spatial_cell` rebuild is now a no-op (`made_with_grid`) since `rebuild_spatial` makes both and runs for the grid (classification needs a fn per Derived type, so it cannot be omitted).
- New test `spawn_tick_save_loads_to_the_live_grid` (debug.spawn in tick N, save, load, equal for 10 more ticks).
- Changed `spatial_counters_recorded`: setup spawns happen outside a tick and the next tick clears counters, so "inserts == unit count on the first tick" no longer holds; it now checks setup spawns are filed immediately, inserts == 0 and relocations > 0 over ticks (inserts == 1 on the debug.spawn tick is asserted in the new test).
- `cargo nextest run --profile agent -p fl_sim`: 160/160. Gate: `gate task: PASS in 103s`.
