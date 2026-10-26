# PATH.QUEUE — Budgeted path requests and HPA* plans
status: approved (lead, R148) · spec-version: 1 · crate: fl_sim::path · milestone: M1
Depends on: MOVE.SPEED · Emits journal records: core.path_request, core.path_result, core.order_changed

## Intent

A unit that needs a path asks for one; the request waits in a first-in, first-out queue that is part of the hashed
game state, and each tick spends a fixed work budget planning requests in the order they were made. Every request
is charged the work a from-scratch plan does, so caches never change who gets a path when. Plans go round obstacles
with HPA* over per-profile cluster graphs, snap a blocked goal to the nearest shore, and drive to the closest
reachable point when the goal cannot be reached. This spec describes M1 as built (spec §5.7, §7, ADR-0015, rulings
R84, R93–R99, R136; `crates/fl_sim/src/path.rs`, `crates/fl_sim/src/nav/{hpa,astar,los}.rs`); group moves on flow
fields are FLOW.GROUP.

## Rules

- PATH.QUEUE.1  When a unit needs a path (a move becoming current with no path, a follower's re-plan, a wanderer's new target, a group member with no route), the sim shall push a single request with the next request id onto the back of the Hashed `PathQueue`, set the unit's `PathFollow` to wait on that request with no corners, and journal `core.path_request`; any earlier request of the unit turns stale.
- PATH.QUEUE.2  The sim shall process the queue in the core slot of `ThinkSet::Behaviour`, so a request made in `Input` or `ThinkSet::Orders` is processed in the tick it was made (budget permitting) and a request made by a feature system in `ThinkSet::Behaviour` or a later hook waits for the next tick.
- PATH.QUEUE.3  While requests are pending, the sim shall pop them from the front of the queue each tick for as long as the work charged this tick is below `Path.budget_per_tick`, and shall process at least one request every tick.
- PATH.QUEUE.4  The sim shall charge each processed request its cold cost, the cells and abstract nodes its plan settled plus the cells its goal and start snaps probed plus the cells its line-of-sight tests walked, whatever a navigation cache held, and journal `core.path_result` with the charge and the ticks the request waited.
- PATH.QUEUE.5  If a popped request's unit no longer exists or no longer waits on that request (stopped, re-ordered or despawned), then the sim shall discard it at charge 0 and journal its result as `stale`.
- PATH.QUEUE.6  When a request is planned, the sim shall apply the plan to its unit in the same tick (`Following` the plan's corners with `planned` set to this tick, or `Failed` with no corners when the plan is `Unreachable`), so results apply in request order.
- PATH.QUEUE.7  If a request's goal, or its unit's start, lies in a cell the unit's profile cannot pass, then the planner shall snap it to the centre of the nearest passable cell within `Path.snap_radius_cells` (Chebyshev ring by ring, then the smallest `(dx² + dy², y, x)`), and if no ring holds one the plan shall be `Unreachable`.
- PATH.QUEUE.8  If no route joins the start and the goal, then the planner shall return a `Partial` plan ending at the centre of a reachable cell near the goal (the settled abstract node or start-cluster cell with the smallest octile distance to the goal, then the cell of that node's cluster it reaches nearest the goal), and the unit's move shall complete there.
- PATH.QUEUE.9  The planner shall return the goal alone when the start sees it (line of sight for the profile), and otherwise a route whose cell-path cost is at most the optimal route's cost plus `(2 × ⌊(Path.cluster_cells − 1) / 2⌋ + 2) × c_max` per cluster border the optimal route crosses, where cell costs use `Movement.path_terrain_factor` and `c_max` is the costliest passable cell (R136).
- PATH.QUEUE.10  When a terrain change rebuilds HPA* clusters, the sim shall set every unit following a path whose remaining route touches a rebuilt cluster back to `Idle` and journal `core.order_changed` with reason `replanned`, so its order asks for a new path in the same tick.

## Formulas

```text
queue (per tick):  spent = 0, processed = 0
                   loop: if processed > 0 and spent ≥ Path.budget_per_tick: stop
                         pop the front request (none: stop); processed += 1
                         spent += charge(request)                         (saturating u32)
charge             = nodes_expanded + los_cells                           (saturating u32; 0 for a stale request)
nodes_expanded     = grid cells settled (A*, Dijkstra) + abstract nodes settled + nearest_passable probes
                     (8r cells for ring r ≥ 1, off-map cells included)
waited             = this tick − the tick the request was made

cell cost          = ceil(COST_ROAD / factor),  COST_ROAD = 100            (u16; integer ceiling on raw Fx)
factor             = clamp((ONE − pass) × Movement.path_terrain_factor[class] + pass, Movement.min_speed_frac, ONE)
edge cost (a → b)  = ceil((cost(a) + cost(b)) / 2), × 141 / 100 (floor) for a diagonal step   (u32)
octile(a, b)       = 100 × (dx + dy) − 59 × min(dx, dy)                   (A* heuristic; a lower bound)
cost bound (R136)  = plan.cost ≤ optimal + crossings × (2 × REACH + 2) × c_max,  REACH = ⌊(cluster_cells − 1) / 2⌋
                     (16 × c_max per crossing with 16-cell clusters; a direct-line plan's cost is an octile lower
                     bound, never compared as a true cost, R99)
```

Plan steps (`plan_path`): snap the goal, snap the start (its snapped centre is the first corner); a direct line if
clear; start and goal in one cluster: in-cluster A* then string-pulling, kept if its cost is at most the least
`octile(start, o) + octile(o, goal)` over cells `o` just outside the cluster, else the abstract route replaces it if
strictly cheaper; otherwise the abstract A* over entrance nodes (`(f, h, node index)` ties), refined by the stored
cell paths and string-pulled; with no route, `Partial` as in PATH.QUEUE.8. Grid A* pops the smallest
`(f, h, y, x)`, never cuts a corner and changes a parent only on a strictly lower cost.

## Parameters

| Name | Path key | Unit | Default | Valid range | Provenance | Confidence |
|---|---|---|---|---|---|---|
| cluster_cells | `rules/Path.cluster_cells` | cells | 16 | ≥ 1 | Guess | Medium |
| budget_per_tick | `rules/Path.budget_per_tick` | work units (cells, nodes) per tick | 8000 | ≥ 1 | Guess | Low |
| max_clearance | `rules/Path.max_clearance` | cells | 3 | 1..=255 | Guess | Low |
| snap_radius_cells | `rules/Path.snap_radius_cells` | cells | 8 | any `u32` | Guess | Low |
| path_terrain_factor | `rules/Movement.path_terrain_factor.*` | fraction | Foot 1.0, Wheeled 0.2, HalfTrack 0.45, Tracked 0.5 | [−1, 1] for each entry; one per class | Guess | Low |
| min_speed_frac (owned by MOVE.SPEED) | `rules/Movement.min_speed_frac` | fraction of road speed | 0.05 | (0, 1] and ≥ 100 / 65535 | Guess | Low |

`min_speed_frac` is owned by MOVE.SPEED (guess G-MOVE-1); the cell cost uses it as its clamp too. `snap_radius_cells`
has no range check in M1.

## Acceptance criteria

- AC-1 (PATH.QUEUE.1, PATH.QUEUE.4): one move journals one `single` request with the unit's profile and a `found` result that waited 0 ticks, and counts `path.requests`, `path.nodes_expanded` and `path.budget_spent` → crates/fl_sim/tests/pathing.rs::path_counters_and_journal
- AC-2 (PATH.QUEUE.1, PATH.QUEUE.5): a second move in the same tick makes the first request `stale` at charge 0, and the unit follows the second → crates/fl_sim/tests/pathing.rs::stale_request_is_discarded
- AC-3 (PATH.QUEUE.2): a request made in `ThinkSet::Orders` is processed in its tick and one made by a feature in `ThinkSet::Behaviour` the next tick, under any system shuffle → crates/fl_sim/tests/orders.rs::path_request_timing_follows_the_hook
- AC-4 (PATH.QUEUE.3, PATH.QUEUE.6): forty requests under a budget of 300 are all processed, in request order, spread over several ticks, never spending past the budget before a tick's last request → crates/fl_sim/tests/pathing.rs::queue_respects_budget_and_order
- AC-5 (PATH.QUEUE.3, PATH.QUEUE.6): with request ids running against sim ids, each tick processes the next requests in queue order → crates/fl_sim/tests/pathing.rs::queue_pops_in_request_order_not_sim_id_order
- AC-6 (PATH.QUEUE.3): no request waits longer than its share of the total charge over the budget, plus one tick → crates/fl_sim/tests/pathing.rs::no_request_starves
- AC-7 (PATH.QUEUE.4, PATH.QUEUE.9): a goal in sight is planned as the goal alone, charged only its line-of-sight cells, with an octile cost → crates/fl_sim/tests/hpa.rs::direct_line_costs_only_los
- AC-8 (PATH.QUEUE.4): plans and their work are identical from graphs invalidated live three times, from a loaded save and from a cold build → crates/fl_sim/tests/hpa.rs::hpa_cold_warm_and_loaded_agree
- AC-9 (PATH.QUEUE.4, PATH.QUEUE.7): a goal in a lake snaps to the nearest shore cell, the snap's probes are charged, and a goal with no shore within the radius is `Unreachable` → crates/fl_sim/tests/hpa.rs::goal_in_water_snaps_to_nearest_shore
- AC-10 (PATH.QUEUE.5): a request whose unit despawned while queued is `stale` at charge 0 → crates/fl_sim/tests/pathing.rs::despawned_unit_request_discarded
- AC-11 (PATH.QUEUE.5): a stop while the unit's request is queued makes it `stale` at charge 0 and the unit stays where it is → crates/fl_sim/tests/pathing.rs::stop_clears_orders_and_pending_request
- AC-12 (PATH.QUEUE.7): a start in a pond snaps to its nearest shore (the plan's first corner), a start off the map snaps onto it, and a start with no passable cell in reach is `Unreachable` → crates/fl_sim/tests/hpa.rs::start_on_blocked_cell_recovers
- AC-13 (PATH.QUEUE.8): a goal on an island is `Partial`, ending at the reachable cell nearest the goal, across clusters and within one → crates/fl_sim/tests/hpa.rs::island_goal_is_partial
- AC-14 (PATH.QUEUE.6, PATH.QUEUE.8): a unit ordered onto an island drives to the nearest reachable cell and its order completes there → crates/fl_sim/tests/pathing.rs::unreachable_goal_goes_to_closest_point
- AC-15 (PATH.QUEUE.8): on random maps a plan is `Found` exactly when grid A* finds a route, and otherwise `Partial` ending at a reachable cell centre → crates/fl_sim/tests/hpa.rs::hpa_finds_path_iff_astar_does
- AC-16 (PATH.QUEUE.9): on random maps a searched plan never costs less than the optimum nor more than the R136 bound → crates/fl_sim/tests/hpa.rs::hpa_cost_near_optimal
- AC-17 (PATH.QUEUE.9): a fixed-seed batch of 200 random maps with 4 requests each stays within the R136 bound, with a worst cell-path ratio of at most 1.8 → crates/fl_sim/tests/hpa.rs::hpa_cost_ratio_tripwire
- AC-18 (PATH.QUEUE.9): a border-corridor detour stays within `2 × REACH` cells of the optimum and string-pulling drives the optimal cost → crates/fl_sim/tests/hpa.rs::border_corridor_detour_is_bounded_and_smoothed_away
- AC-19 (PATH.QUEUE.9): cell costs are the integer ceiling of `100 / factor` with the class's path terrain factor → crates/fl_sim/tests/terrain.rs::nav_cost_is_integer_ceiling
- AC-20 (PATH.QUEUE.10): a truck whose bridge drowns mid-route is journaled `replanned` that tick, never enters the river and arrives over the other bridge → crates/fl_sim/tests/pathing.rs::bridge_destroyed_mid_route_repaths
- AC-21 (PATH.QUEUE.10): drowning a bridge invalidates exactly the two clusters its grown rect touches, and the graphs then equal a fresh build → crates/fl_sim/tests/hpa.rs::bridge_removal_reroutes

## Non-goals · Open questions · Guesses

- Non-goal: priorities between requests. M1 has one FIFO lane; Task 15 (MCR-002) adds command and background lanes.
- Non-goal: a path-result cache. Every request plans from scratch; only the navigation data is cached.
- Open question: the per-request search buffers allocate (R98, R125); parked until a map over 1024 cells per side or
  a profile showing HPA* among the top systems.
- Open question: no test drives a unit to `Failed` through the queue (PATH.QUEUE.6's `Unreachable` branch is pinned
  only at plan level, AC-9 and AC-12). A candidate for the test-author.
- Open question: `Path.snap_radius_cells` has no range check; a huge radius only costs probe work, charged to the
  request.
- Guess G-PATH-1: `rules/Path.cluster_cells` = 16, the HPA* cluster side; spec §5.7 names 16 × 16 clusters, with no
  measurement behind it. Replace by profiling search work against cluster rebuild cost on the 2 km map.
- Guess G-PATH-2: `rules/Path.budget_per_tick` = 8000 work units per tick, the path budget. Replace by measuring
  path time per work unit and fitting it to the tick budget (G7).
- Guess G-PATH-3: `rules/Path.max_clearance` = 3, the largest unit size in cells the navigation data distinguishes
  (12 m on 4 m cells). Replace when the roster gains hulls or buildings wider than that.
- Guess G-PATH-4: `rules/Path.snap_radius_cells` = 8, how far a blocked goal or start looks for a passable cell.
  Replace by tuning against goals clicked into lakes and rivers on the testbed.
- Guess G-PATH-5: `rules/Movement.path_terrain_factor` (Foot 1.0, Wheeled 0.2, HalfTrack 0.45, Tracked 0.5), the
  per-class terrain factor that path costs use (one per class, not per unit, so one cost grid serves every unit of a
  class). Replace by tuning route choice on the testbed.

## Changelog

- v1 (2026-10-25, M2 prelude Task 4): first version, backfilled from M1 as built; approved by the lead (R148), on the
  M2 prelude ratification list.
