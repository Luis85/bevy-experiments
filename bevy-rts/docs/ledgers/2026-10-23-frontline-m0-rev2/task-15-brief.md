### Task 15: Incremental spatial grid (rev-1 T9)

**Track:** sim-core · **Lane 1**

**Files:**
- Modify: `crates/fl_sim/src/spatial.rs`, plus a one-line `init_spatial` call each in `scenario.rs` (`setup_map`) and `scale.rs`
- Create: `crates/fl_sim/tests/spatial.rs`

**Interfaces:**
- Consumes: `MapInfo`, `Pose`, `SimIndex`, `SpatialRules`, `SimCounters`, `PerceiveSet::Spatial`, `Class`, `ScaleScenario`.
- Produces:
```rust
#[derive(Resource, Debug, Clone)] pub struct SpatialGrid { .. }   // Derived "core.spatial_grid", rebuild = rebuild_spatial
impl SpatialGrid {
    pub fn new(map: &MapInfo, cell: Fx) -> Self;
    pub fn cell_of(&self, pos: FxVec2) -> u32;             // clamps into the grid; pos == size maps to the last cell
    pub fn insert(&mut self, id: SimId, cell: u32);
    pub fn remove(&mut self, id: SimId, cell: u32) -> bool;
    pub fn relocate(&mut self, id: SimId, from: u32, to: u32);
    pub fn members(&self, cell: u32) -> &[SimId];          // sorted ascending
    pub fn query_radius(&self, center: FxVec2, radius: Fx, out: &mut Vec<SimId>) -> u32;   // broad phase over cells overlapping the circle's AABB;
                                                           //   out cleared, sorted, unique; returns the candidate count for "spatial.query_candidates"
    pub fn len(&self) -> usize;
}
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)] #[component(on_remove = …)] pub struct SpatialCell(pub u32);   // Derived "core.spatial_cell"
pub fn rebuild_spatial(world: &mut World);   // a new grid from MapInfo + every Pose; also the load rebuild
pub(crate) fn init_spatial(world: &mut World);
```
- **`track_spatial`** (`PerceiveSet::Spatial`) walks `SimIndex` in order. It inserts a `SpatialCell` where one is missing and relocates when the cell changed. It counts `spatial.inserts` and `spatial.relocations`.
- No `Changed<Pose>` and no observer is used. The `SpatialCell` `on_remove` hook removes the entity from the grid; that is index maintenance, which hooks may do.

- [ ] **Step 1: Write the failing tests:**
```rust
#[test] fn insert_keeps_cells_sorted()
#[test] fn position_on_max_edge_maps_to_last_cell()   // 1000 m map, cell 32: cell_of((1000,1000)) == cols * rows − 1
#[test] fn relocate_moves_between_cells()
#[test] fn query_radius_matches_brute_force()         // proptest: random ids and positions on a 2 km map, radius 1..300 → contains the exact set; sorted, unique
#[test] fn grid_tracks_wandering_units()              // ScaleScenario with 500 units, 300 ticks: every SpatialCell == cell_of(pose), membership exact, len() == 500
#[test] fn despawn_removes_from_grid()
#[test] fn grid_rebuilt_after_load()                  // save at tick 100, load → the grid equals the live grid
#[test] fn spatial_counters_recorded()                // on the first tick after setup: counter "spatial.inserts" == unit count
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_sim --test spatial`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo nextest run --profile agent -p fl_sim`. Expected: PASS.
- [ ] **Step 5: Gate** `cargo xtask gate task --track sim-core`. Expected: PASS.
- [ ] **Step 6: Commit** `feat(fl_sim): incremental uniform spatial grid (derived, rebuilt on load)`.

