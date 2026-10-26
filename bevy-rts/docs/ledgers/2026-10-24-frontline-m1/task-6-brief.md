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

