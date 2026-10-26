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
#[test] fn clusters_query_lists_entrances()      // "core.clusters" on a 32×32 open map: clusters [2,2], 16 entrances; {"unit": id} of a t.tank
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

