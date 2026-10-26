### Task 9: Incremental spatial grid (`fl_sim/src/spatial.rs`)

**Files:** Modify `crates/fl_sim/src/spatial.rs` only; create `crates/fl_sim/tests/spatial.rs`.

**Interfaces — Consumes:** `MapInfo`, `Pose`, `SimEntity`, `SimSet::Movement`, `MovementSystems`, `SpatialRules`, `ScaleScenario`. **Produces:**
```rust
#[derive(Resource, Debug, Clone)] pub struct SpatialGrid { /* cell: Fx, cols: u32, rows: u32, cells: Vec<Vec<SimId>> each sorted */ }
impl SpatialGrid {
    pub fn new(map: &MapInfo, cell: Fx) -> Self;
    pub fn cell_of(&self, pos: FxVec2) -> u32;              // clamps into the grid; pos == size maps to last cell
    pub fn insert(&mut self, id: SimId, cell: u32);
    pub fn remove(&mut self, id: SimId, cell: u32) -> bool;
    pub fn relocate(&mut self, id: SimId, from: u32, to: u32);
    pub fn members(&self, cell: u32) -> &[SimId];
    pub fn query_radius(&self, center: FxVec2, radius: Fx, out: &mut Vec<SimId>);  // broad phase: cells overlapping the circle's AABB; out cleared, sorted ascending, no dups
    pub fn len(&self) -> usize;
}
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)] pub struct SpatialCell(pub u32);
```
**Systems.** All run in `SimSet::Movement`, `.after(MovementSystems)`:
- `rebuild_on_map_change`: when `MapInfo` changes, build a new grid from every Pose and insert `SpatialCell`.
- `track_new`: `Added<Pose>` without `SpatialCell`.
- `track_moved`: `Changed<Pose>` with `SpatialCell`; relocate when the cell differs.
- Removal: an observer on `On<Remove<SpatialCell>>` removes the entity from the grid.

Iterating `Changed<Pose>` in archetype order is allowed here: each cell is kept sorted, so the final grid doesn't depend on visit order. Say so in a comment.

- [ ] **Step 1: Write the failing tests.**
  - `insert_keeps_cells_sorted`.
  - `position_on_max_edge_maps_to_last_cell`: on a 1000 m map with cell 32, `cell_of((1000,1000)) == cols*rows - 1`.
  - `relocate_moves_between_cells`.
  - `query_radius_matches_brute_force`: a proptest with random ids and positions on a 2 km map and radius 1..300. The broad-phase result contains the exact brute-force set and is sorted and unique.
  - `grid_tracks_wandering_units`: ScaleScenario with 500 units for 300 ticks. Each entity's `SpatialCell == cell_of(pose)`, it is a member of exactly that cell, and `len() == 500`.
  - `despawn_removes_from_grid`.
- [ ] **Step 2: Run** `cargo test -p fl_sim --test spatial`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test -p fl_sim --test spatial`. Expected: PASS.
- [ ] **Step 5: Gate + commit** `feat(fl_sim): incremental uniform spatial grid`.

