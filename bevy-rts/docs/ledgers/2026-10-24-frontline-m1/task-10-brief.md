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
        //   to the path budget); None means only that the cell has no route
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

