### Task 8: Path requests, order queue and path following

**Track:** sim-core + perf · **Refs:** ADR-0015, ADR-0006, ADR-0007 (input caps), ADR-0010 · **Lane 1**

**Files:**
- Create: `crates/fl_sim/src/{path,orders}.rs`, `crates/fl_sim/tests/pathing.rs`
- Modify:
  - `crates/fl_sim/src/{units,commands,movement,publish,debug_cmds,nav/mod,lib}.rs` (`debug_cmds.rs`: `debug.teleport` clears `MoveOrder.orders` and sets `PathFollow` to `Idle`, in place of clearing `waypoints`)
  - `crates/fl_test_utils/tests/m0_save.rs` (R78 fills for `core.path_queue` and `core.path_follow`)
  - `crates/fl_sim/tests/{movement,save,commands,observe,runner}.rs`: ports from `MoveOrder.waypoints` and from the `core.move_order` journal kind (`observe.rs`, `runner.rs` assert it). List each test-guard finding in the report.
- Blessed: system order, `crates/fl_test_utils/tests/snapshots/formats__save_codecs.snap` (`core.move_order@2 reads 1`, `core.path_follow@1`, `core.path_queue@1`), API, golden, `docs/perf/budgets.ron`, perf counter snapshots.

**Interfaces:**
- Consumes: `plan_path`, `PathWork`, `PlanStatus` (Task 7); `NavGrid::{passable, speed_factor, cell_of}`, `profile_of`, `on_terrain_changed`, `MovementRules`, `PathRules` (Task 5).
- Produces:
```rust
// R78 fills (Codec::fill_missing, Task 5): "core.path_queue" → PathQueue::default() (empty); "core.path_follow" → PathFollow::default() (Idle)
//   on every entity with Mobility, in SimIndex order. They run only for a save of an older core version (the M0 fixture)
// units.rs: MoveOrder v2 (Hashed "core.move_order"; Codec::new(2).migrate_from::<v1::MoveOrder>(1): each v1 waypoint → Order::Move;
//   the format guard requires this migration for the bump, R70; v1 held at most MAX_WAYPOINTS waypoints, so v2 orders stay within the cap)
#[derive(Component, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)] pub struct MoveOrder { pub orders: Vec<Order> }   // orders[0] is current;
                                                                                                      //   len ≤ MAX_WAYPOINTS (R77)
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)] pub enum Order { Move { target: FxVec2 }, Rotate { facing: Angle }, Follow { leader: SimId, offset: FxVec2 } }
    // an interim closed enum of core movement orders until the C7 order queue + activity stack (M2 prelude) replaces it
// path.rs
#[derive(Component, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathFollow { pub state: PathState, pub goal: FxVec2, pub corners: Vec<FxVec2>, pub next: u16, pub planned: Tick }   // Hashed "core.path_follow" v1
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PathState { #[default] Idle, Waiting { request: u32 }, Following, Flow { key: FlowKey }, Done, Failed }   // Flow used from Task 10
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)] pub struct FlowKey { pub cluster: ClusterId, pub profile: MoveProfile }
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathQueue { pub pending: VecDeque<PathRequest>, pub next_id: u32 }   // Hashed "core.path_queue" v1
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathRequest { pub id: u32, pub unit: SimId, pub goal: FxVec2, pub kind: RequestKind, pub issued: Tick }
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)] pub enum RequestKind { Single, Group { units: Vec<(SimId, FxVec2)> } }   // Group from Task 10
pub fn enqueue_path(world: &mut World, unit: SimId, goal: FxVec2) -> u32;   // push back, PathFollow → Waiting, journal "core.path_request"
// systems (core slots): ThinkSet::Orders: orders::advance_orders, then movement::wander_system, chained (wander moves here from ThinkSet::Behaviour;
//   it keeps its "debug.wander" Stream through with_input and draws x then y exactly as in M0, now as an exclusive system
//   fn(InMut<Stream>, &mut World) so it can call enqueue_path; check with_input on exclusive systems with the bevy-api skill);
//   ThinkSet::Behaviour: path::process_path_queue; ActSet::Move: movement::move_system
// journal "core.path_request" { request: u32, goal: FxVec2, profile: MoveProfile, kind: "single"|"group" };
//   "core.path_result" { request: u32, status: "found"|"partial"|"unreachable"|"stale", corners: usize, cost: u32, charge: u32, waited: u32 };
//   "core.order_changed" { orders: usize, current: "move"|"rotate"|"follow"|"none", reason: "command"|"completed"|"failed"|"stopped"|"replanned" }
//   (replaces "core.move_order")
// counters: "path.requests" (processed), "path.queue_len" (after processing), "path.nodes_expanded", "path.los_cells", "path.budget_spent", "move.blocked"
// debug query "core.paths" args {"units":[ids]} → {"<id>": {"state","goal":[xr,yr],"corners":[[xr,yr],..],"next","orders":[{"move":[xr,yr]}|{"rotate":raw}|{"follow":id}],
//   "profile":{"class","size"}}}
```
- **Handlers** (Input). `core.move`:
  - Without queue: the orders become `[Move]`, and the handler calls `enqueue_path` per unit in `SimId` order. Any older request turns stale.
  - With queue: append the order; enqueue only if the unit had no orders. If any unit already holds `MAX_WAYPOINTS` orders, the whole command is rejected with `core.queue_full` and nothing changes (M0's rule, now over `orders`).

  `core.stop` clears the orders, and `PathFollow` returns to `Idle` with no corners.
- **`advance_orders`** walks `SimIndex` in order:
  - It pops every completed current order (`Done`, or a `Failed` move) and journals `order_changed`.
  - When a queued `Move` becomes current, it enqueues a request.
  - It re-requests units whose `PathFollow` returned to `Idle` while a `Move` is current: blocked, or invalidated.
  - Wander (`wander_system`, chained right after): a unit with `Wander` and no orders gets `[Move { drawn target }]` and a request. The draws are as in M0: `rolls(seed, tick, sim_id, "debug.wander" Stream)`, x then y.
- **`process_path_queue`.**
  - Pop requests while `spent < budget_per_tick`, processing at least one per tick.
  - A request is stale, journaled `stale` at charge 0, unless its unit exists and is `Waiting { request: id }`.
  - Otherwise `plan_path` runs from the unit's current pose with its profile. The request is charged `PathWork::charge()`, and the plan is applied at once: `Following` with the corners, or `Failed` for `Unreachable`, with `planned = tick`.
- **`move_system`** (entity-local):
  - `Following`: turn toward `corners[next]` by at most `turn_rate`. While the heading error exceeds `turn_in_place_deg`, turn without moving. Otherwise move `speed × speed_factor`, snapping to a corner within one step, and advancing `next`. Within `arrive_m` of the goal, the state becomes `Done`.
  - Current `Rotate`: turn toward `facing`.
  - A step into an impassable cell is skipped, counts `move.blocked`, and sets `Idle`.
- **Invalidation.** `on_terrain_changed` then invalidates HPA* (Task 7). In `SimIndex` order, every unit `Following` whose remaining segments cross a rebuilt cluster's rect is set to `Idle`, journaled `replanned`.
- **`publish`** sets `flags::MOVING` while `PathFollow` is `Following` or `Flow`.

- [ ] **Step 1: Write the failing tests** (`crates/fl_sim/tests/pathing.rs`):
```rust
#[test] fn tank_paths_around_lake()                 // arrives within 600 ticks; every tick the unit's cell is passable for t.tank
#[test] fn crosses_river_only_by_bridge()           // t.truck crosses "~~~=~~~" rows; some tick's cell is a '=' cell; never a '~' cell
#[test] fn unreachable_goal_goes_to_closest_point()  // island goal: path_result "partial"; final cell == the nearest reachable cell; order popped ("completed")
#[test] fn hull_rotates_before_moving()             // tank heading 0, goal due west: pos unchanged while |heading error| > 30°, then moves
#[test] fn queued_moves_run_in_order()              // Shift-move A then B: reaches A, then B; order_changed reasons "command", "command", "completed", "completed"
#[test] fn stop_clears_orders_and_pending_request() // move, stop next tick before processing (budget 1): result "stale"; unit Idle, no orders
#[test] fn stale_request_is_discarded()            // two moves for the same unit in one tick (seq 0, 1): the first request's result is "stale", charge 0
#[test] fn despawned_unit_request_discarded()
#[test] fn queue_respects_budget_and_order()       // 40 tanks moved at tick 1 with Path.budget_per_tick 300 (ascii_rules): path_result request ids ascending
                                                    //   across ticks; per tick (sum of charges − the last charge) < 300
#[test] fn no_request_starves()                    // every request's waited ≤ ceil(total charge / 300) + 1
#[test] fn bridge_destroyed_mid_route_repaths()    // set_terrain water over bridge A while a truck is en route to it → "replanned"; arrives via bridge B; never on '~'
#[test] fn wander_goes_through_the_queue()         // wander_all on an ASCII map: "core.path_request" journal entries for every wanderer within 50 ticks
#[test] fn branch_with_queued_requests()           // budget 300, 40 movers: assert_branch(n = 2, m = 300) while the queue is non-empty at tick 2
#[test] fn move_order_v1_migrates()                // v1 bytes { waypoints: [a, b] } decode to orders [Move a, Move b]
// crates/fl_sim/tests/commands.rs: queued_waypoints_are_capped (M0) is ported: a queued core.move onto 32 orders → "core.queue_full", nothing changed
// crates/fl_test_utils/tests/m0_save.rs (R78), extended: m0_testbed_save_loads_under_m1 also asserts PathQueue empty, every Mobility unit has
//   PathFollow Idle, v1 MoveOrders migrated; within 50 ticks a moving or wandering unit journals "core.path_request"
#[test] fn path_counters_and_journal()             // "path.requests", "path.nodes_expanded" > 0 on a lake detour, "path.queue_len" while backlogged
#[test] fn paths_debug_query()                     // "core.paths" {"units":[1]} lists state "following" and the corners
#[test] fn smoke_pathing_twin_and_shuffle()        // twin + shuffle on the ASCII river scenario with 6 movers, 300 ticks
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_sim --test pathing`. Expected: FAIL.
- [ ] **Step 3: Implement**, and port the M0 movement tests to `MoveOrder.orders`. Every ported or changed assertion goes in the report for test-guard.
- [ ] **Step 4: Run** `cargo nextest run --profile agent -p fl_sim -p fl_test_utils`. Expected: PASS, apart from the protected artifacts.
- [ ] **Step 5: Regenerate** system order, formats, API, the golden, and `FL_RECORD_BUDGETS=1 cargo nextest run --profile agent -p fl_test_utils --test perf_counters -E 'test(/scale/)'` (new `path.*` counters). The lead blesses them: `M1 Task 8: path queue, MoveOrder v2, wander through the queue`.
- [ ] **Step 6: Gate** `cargo xtask gate task --track sim-core --track perf`. Expected: PASS, with `smoke_counters_scale_500` and `smoke_pathing_twin_and_shuffle` run.
- [ ] **Step 7: Commit** `feat(fl_sim): budgeted path request queue, MoveOrder v2 order queue, path following with hull rotation` with trailers `Refs: ADR-0015` and `Blessed: …`.

