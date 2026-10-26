---
status: "accepted"
date: 2026-10-24
decision-makers: "controller (accepted at the M1 Task 1 review, rulings R69, R84), lead"
consulted: "M1 planner (planner-m1-report.md, spec-gap resolutions accepted provisionally as R69); controller rulings R72, R74, R76"
informed: "M1 implementers (Tasks 2, 5–14, 18)"
---

# Path requests, derived navigation caches and debug queries

## Context and Problem Statement

Spec §5.7 asks for HPA* with about 16 × 16-cell clusters per movement class plus clearance, a per-tick budgeted
request queue whose results apply in request order, flow fields cached per (destination cluster, class), and dynamic
obstacles that invalidate only the clusters they touch. It doesn't say what a request is charged, whether the caches
are state, how they survive save and load, or when a request that lost its reason is dropped. Those choices decide
whether a cache can change a match's outcome: if a cache hit is cheaper, the budget admits more requests that tick,
and every peer would have to hold the same cache contents. The overlays (spec §11) also need to read sim-side data
such as passability, clusters and paths, which the snapshot doesn't carry, and `SimQuery` (ADR-0002) is a closed list.

## Decision Drivers

* A cache must never change outcomes, so it can stay out of the hash and the save (ADR-0006).
* The budget bounds work per tick (spec §5.7, ADR-0010), and no request starves.
* Order-sensitive work follows `SimIndex` (ADR-0005, narrowed ordering rule).
* Saves and the branch test hold mid-pathing: requests queued, groups on flow fields, right after a terrain change.
* Work is countable, for the budget and for the deterministic work counters (ADR-0010).
* Debug data reaches tools without growing a contract enum per overlay, and without touching sim state.

## Considered Options

* A Hashed FIFO with cold-cost charges, `Derived` caches, and a named debug-query registry
* Charge the work actually done, and make the caches Hashed state
* Plan synchronously in the command handler, without a queue
* The `pathfinding` crate's searches behind the queue
* One `SimQuery` variant per kind of debug data

## Decision Outcome

Chosen option: "A Hashed FIFO with cold-cost charges, `Derived` caches, and a named debug-query registry", because the
cold-cost charge, together with reads that never observe a miss (R84), keeps caches outcome-neutral, which lets them
be `Derived` and lets eviction be anything deterministic.

- **The queue.** Path requests enter a Hashed FIFO, `PathQueue` (`core.path_queue`). Every source is a system at a
  fixed place in the schedule, and each enqueues in a fixed order:
  - command handlers (`Input::Apply`), in `(player, seq)` order; within one command, its group requests first (one
    per profile, in `MoveProfile` order), then its single requests in `SimId` order;
  - `advance_orders`, then `wander_system`, chained in `ThinkSet::Orders`, each in `SimIndex` order;
  - `process_path_queue` itself, which pushes a group member whose start cell has no route to the back as a single
    request, in `SimId` order.

  Each unit's `PathFollow` (`core.path_follow`, Hashed) records the request it waits on (`Waiting { request }`), so a
  request survives between ticks and across a save.
- **Processing.** `ThinkSet::Behaviour` pops requests while `spent < Path.budget_per_tick`, processing at least one
  per tick. Each request is charged its **cold cost**: the work a from-scratch computation does (nodes expanded plus
  line-of-sight cells), whether or not a cache answered it. Its result is applied to its unit immediately, so results
  apply in request order. A request whose unit is gone, or no longer waits on it (stopped, re-ordered), is discarded
  at charge 0 and journaled `stale`. A group request is stale when none of its units exists and still waits on it;
  otherwise the members that left are skipped.
- **HPA\*.** One graph per movement profile (ADR-0014) over 16 × 16-cell clusters (`Path.cluster_cells`).
  - Entrances: along each border between adjacent clusters, each maximal run of cell pairs passable on both sides
    gets one transition at index `start + (len − 1) / 2` if it is shorter than 6, and two, at both ends, otherwise.
    Each transition makes a node in each cluster and an inter edge; intra edges come from one in-cluster Dijkstra
    per node and keep their cell paths.
  - Search: grid A* pops the smallest `(f, h, y, x)`, and the abstract A* the smallest `(f, h, node index)`; a parent
    changes only on a strictly lower g; no corner cutting. Start and goal in one cluster use in-cluster A*, which
    stands if its cost is at most the least `octile(start, o) + octile(o, goal)` over the cells `o` just outside the
    cluster (a lower bound on any route that leaves it); otherwise the abstract search runs too, and its route
    replaces the in-cluster one if strictly cheaper (R136). Refinement concatenates the stored cell paths, then
    string-pulls with a supercover line-of-sight test.
  - **Cost bound (R136).** A searched plan's cell-path cost (`PathPlan.cost`) is at most the optimal cost plus
    `16 × c_max` per cluster border the optimal path crosses (a diagonal step through a cluster corner crosses two),
    `c_max` being the costliest passable cell. Derivation:
    - Each crossing pair of the optimal path lies in a run whose nearest transition is at most 7 pairs away, since a
      16-pair run has transitions only at its ends (in general `(cluster_cells − 1) / 2`).
    - So the crossing moves to that transition by walking at most 7 steps along the border on one side and 7 back
      on the other, plus the crossing step. A diagonal crossing first steps through its side cell, which is
      passable because no corner is cut. That is at most 16 steps, each of at most `c_max`.
    - Each piece of the optimal path between two crossings lies in one cluster. So the intra edge (an in-cluster
      Dijkstra), or the start or goal link, between the moved crossings costs no more than that piece plus the walks.
    - The abstract A* finds the cheapest abstract route: octile is consistent, as every edge is a real route.
    - The same-cluster rule above makes the bound hold when start and goal share a cluster but the optimal path
      leaves it.

    The earlier `1.3 × optimal + 4 road cells` bound did not hold. A start on a one-cell corridor along a border,
    5 cells from the run's end transitions, gave 1.575 × optimal (Task 10b's counterexample): the cell path walks
    down the border and back up. String-pulling removes such detours from the corners a unit drives. Over 20,000
    random requests drawn as `hpa_cost_near_optimal` draws them (6,059 searched), the worst cell-path ratio is
    1.775 and the worst for the driven corners (the cheapest cell path through them) 1.385; the guaranteed bound
    always held, and the old one failed 10 times. Denser transitions (reach 4) would tighten the bound but double
    the nodes per open cluster; they are parked with R98.
  - A blocked goal snaps to the nearest passable cell within `Path.snap_radius_cells` (`Unreachable` if none). With
    no route, the plan goes to the reached node or cell nearest the goal (`Partial`).
- **Flow fields.** A non-queued group move of `Path.flow_min_group` or more same-profile units makes one group
  request, which uses a flow field per `(destination cluster, profile)`. The field is an abstract Dijkstra over the
  profile's HPA* graph (charged as the request's cold cost) plus per-cluster tiles built lazily, in `SimIndex` order,
  before movement (`ActSet::Move`). Smaller sets, and units whose start has no route, use single requests.
- **Caches.** `NavGrid`, `HpaGraphs` and `FlowCache` are `Derived`. A cache never changes outcomes, for two reasons:
  - Charges are cold costs, so no budget depends on what a cache holds.
  - **A cache read never observes a miss (R84).** Every cached value is a pure function of Hashed state (terrain,
    content), and a read computes a missing value before answering. `FlowCache::direction` builds an evicted
    integration or tile on the spot, and that work counts as a from-scratch build, like every build. `None` means
    only "no route from this cell", never "not cached".
  - **Read-miss rebuilds are not charged to the path budget** (R112). They count only in the observe-only
    `flowfield.*` counters, which therefore depend on what the cache held (R86). The budget does not bound them, so
    the eviction policy must (R124, correcting R112): `flow_cache_max` alone did not. While `FlowCache` evicted inside
    a read, more live keys than `flow_cache_max` evicted exactly the keys the same tick read next, and every read
    past the cap rebuilt a field; the cost grew with reads per tick (Task 13's `flow_thrash`, 36 keys against 32:
    about 91 integrations and tiles per tick; 60 keys on the 2 km map: about 350 ms a tick).
  - **Eviction runs only at the tick boundary (R124).** A read never evicts: a miss builds and inserts. At the start
    of a tick (`FlowCache::begin_tick`, run by core's `begin_flow_tick` in `InputSet::Housekeeping`, before any flow
    read of the tick; the readers check it ran) the least recently used entries, by
    `(last read tick, FlowKey)`, go down to `Path.flow_cache_max`, but never one read in the previous tick: the
    fields units follow are the live set, kept above the cap. Past `Path.flow_cache_hard_max` (at least
    `flow_cache_max`; 4 × in the base rules) entries go in the same order even if read in the previous tick. So a
    live field is built once however many are live, and any key at most once per tick. Memory: at a tick start the
    cache holds at most `flow_cache_hard_max` fields, and during a tick at most that plus the distinct keys the tick
    reads (each field is one cost per HPA* node plus the tiles read).

  Eviction is therefore free to be any deterministic policy (the cache's contents never reach an outcome), and
  there is no path-result cache. Caches use `Vec`s indexed by cell or node
  and `BTreeMap`s keyed by `FlowKey`/`MoveProfile`, never hash maps or `Entity` keys. They are rebuilt by core's one
  `Derived` rebuild, `rebuild_core`: the spatial index, then `NavGrid`, `HpaGraphs` and an empty `FlowCache`
  (ADR-0006 refinement, R74).
- **Per-tick results.** Steering pushes and similar per-unit, per-tick results live in a Transient scratch resource
  sorted by `SimId` (`SteerPushes`, `Class::TransientReset`, capacity kept), never in a Transient component: under
  R72 Housekeeping removes Transient components every tick, an archetype move per entity per tick (R76). Steering
  reads positions as of the start of the tick only, so each push is order-free.
- **Invalidation.** A terrain change goes through one hook, `on_terrain_changed`. It updates `NavGrid` for the
  touched rect (grown by the slope and clearance reach), rebuilds only the HPA* clusters it touches (plus the intra
  edges of their neighbours), clears the flow entries of every profile whose graph rebuilt a cluster, and sets every
  unit whose remaining path crosses a rebuilt cluster back to `Idle`, in `SimIndex` order, so it re-requests
  (journaled `replanned`).
- **Observability.** Journal kinds `core.path_request` and `core.path_result` (with the charge and the ticks waited);
  counters `path.requests`, `path.queue_len`, `path.nodes_expanded`, `path.los_cells`, `path.budget_spent`,
  `path.clusters_invalidated`, `flowfield.cells`, `flowfield.abstract_nodes`, `nav.cells_updated`, `steer.units`
  and `move.blocked`; spans `hpa_search`, `hpa_cluster_build` and `flowfield_build`. Per-request logs are at `trace`.
- **Debug queries.** `SimBuilder::register_debug_query(name, fn(&World, &Value) -> Result<Value, String>)`, answered
  between ticks through `SimQuery::Debug { name, args }` → `SimReply::Debug(Value)`.
  - The registry is a stable-string registry (`core.debug_queries`, External), sorted and frozen in `finish()`; a
    duplicate name panics naming both features. Its dense ids never leave the sim.
  - An unknown name answers `SimReply::Error("unknown debug query '<name>'")`; a query's own `Err` answers
    `SimReply::Error(<message>)`.
  - Queries are read-only (`&World`), observe-only (sim logic never reads them; they are never hashed) and JSON, with
    `Fx` as its raw `i64` (`Fx::raw`), so no float enters the sim.
  - M1 registers `core.debug_queries` (the sorted names), `core.passability`, `core.terrain_at`, `core.clusters` and
    `core.paths`. The overlays, BRP `fl.debug_query` and `fl_cli query` use them.

### Consequences

* Good, because caches can be dropped, evicted or rebuilt at any time without a desync, and a save carries none of
  them.
* Good, because a cold cache is a test oracle: running with `RunConfig.flow_cache_cold` must give the same hash
  stream.
* Good, because the queue and the results are Hashed, so a save taken mid-pathing resumes exactly.
* Good, because a new kind of debug data is one registration, not a contract change to `fl_view`.
* Bad, because the budget admits fewer requests than a warm cache could serve; the charge measures worst-case work,
  not wall time.
* Bad, because units wait a tick or more under load (spec §5.7 accepts that).
* Bad, because debug query replies are untyped JSON; the client decoders (`decode_paths`, …) are the schema, checked
  by tests.

### Confirmation

* Search (Task 6): `astar_open_grid_pinned`, `no_corner_cutting`, `astar_matches_dijkstra_cost`,
  `astar_is_deterministic`, `smooth_keeps_path_valid`, `nearest_passable_ring_order`,
  `unreachable_goal_expands_reachable_region`.
* HPA* (Task 7): `entrances_follow_run_rule`, `direct_line_costs_only_los`, `goal_in_water_snaps_to_nearest_shore`,
  `island_goal_is_partial`, `hpa_finds_path_iff_astar_does`, `hpa_cost_near_optimal`, `bridge_removal_reroutes`,
  `invalidate_equals_full_rebuild`; the cost bound (R136): `hpa_cost_near_optimal` against the per-crossing bound,
  `border_corridor_detour_is_bounded_and_smoothed_away`, `same_cluster_takes_a_cheaper_route_out_of_the_cluster`.
* Queue, budget and order (Task 8): `queue_respects_budget_and_order`, `no_request_starves`,
  `stale_request_is_discarded`, `despawned_unit_request_discarded`, `stop_clears_orders_and_pending_request`,
  `unreachable_goal_goes_to_closest_point`, `bridge_destroyed_mid_route_repaths`, `wander_goes_through_the_queue`,
  `branch_with_queued_requests`, `path_counters_and_journal`, `smoke_pathing_twin_and_shuffle`; at scale,
  `path_budget_holds_at_scale` (Task 13).
* Flow fields and outcome-neutral caches (Task 10): `group_move_uses_one_flow_request`,
  `small_group_uses_single_requests`, `mixed_profiles_split`, `flow_units_reach_their_slots`,
  `flow_directions_reach_destination`, `cache_does_not_change_outcomes`,
  `flow_cache_overflow_does_not_change_outcomes` (60 concurrent keys: over the cap, over the hard max, cold, against
  roomy), `branch_mid_flow_follow`, `flow_invalidated_by_terrain_change`, `flow_counters_recorded`; tick-boundary
  eviction (Task 10b, R124): `flow_cache_live_keys_never_thrash`, `flow_cache_evicts_only_at_tick_start`,
  `flow_cache_hard_max_bounds_memory`.
* Derived rebuild (Task 5): `core_registers_one_rebuild`, `incremental_nav_equals_full_rebuild`,
  `save_right_after_set_terrain`; mid-pathing saves (Task 14): `m1_save_round_trip_mid_pathing`,
  `m1_branch_mid_pathing`, `m1_scenarios_shuffle_invariant`.
* Per-tick scratch (Task 11): `steer_pushes_are_per_tick_and_sorted`, `idle_steering_is_staggered`,
  `units_never_enter_blocked_cells`.
* Debug queries: `debug_queries_listed`, `unknown_debug_query_is_error`, `duplicate_debug_query_panics_naming_both`,
  `debug_query_does_not_change_state`, `failing_query_reports_its_message`, `query_lists_debug_queries`,
  `query_unknown_name_fails` (Task 2); `terrain_debug_queries` (Task 5), `clusters_query_lists_entrances` (Task 7),
  `paths_debug_query` (Task 8); the consumers `paths_overlay_polls_selected_units`, `passability_follows_selected_unit`
  and `brp_debug_query_round_trip` (Task 18).
* The system-order snapshot shows each new core system in its hook (review).

## Pros and Cons of the Options

### Hashed FIFO, cold-cost charges, `Derived` caches, debug-query registry

* Good, because caches are outcome-neutral by construction, and the queue is save-safe.
* Good, because the charge is a deterministic work count the budget and the counters share.
* Bad, because a warm cache saves wall time but not budget.

### Charge actual work; caches as Hashed state

* Good, because a warm cache serves more requests per tick.
* Bad, because cache contents and eviction become part of the determinism contract: hashed, saved, versioned, and
  identical on every peer, which makes every cache a migration burden.

### Synchronous planning in the command handler

* Good, because it is simplest, and a unit starts moving on the command's tick.
* Bad, because forty units ordered at once plan in one tick, which breaks the tick budget (spec §5.7).
* Bad, because wander and re-plans would need their own path anyway.

### The `pathfinding` crate

* Good, because it is tested and saves writing A* and Dijkstra.
* Bad, because its loop can't charge expansions to a budget or count line-of-sight cells, and its tie-breaking isn't
  part of our contract. Spec §7 says it "can" be used, not that it must; nothing is downloaded either way.

### One `SimQuery` variant per kind of debug data

* Good, because replies are typed.
* Bad, because every overlay changes `fl_view` (a contract crate, ADR-0011), and `fl_view` can't name feature types.

## More Information

* Spec §5.3 (debug queries), §5.7 (pathfinding), §7 (movement), §11 (overlays), §15 (journal, counters, spans).
* Related: ADR-0002 (`SimQuery`), ADR-0005 (ordering and phases), ADR-0006 (classes; R72 Transient, R74 one rebuild
  per feature), ADR-0008 (BRP only through `fl.*`), ADR-0010 (work counters), ADR-0014 (profiles, blocking, costs).
* Controller rulings: R60 (`PathQueue` and `PathFollow` are Hashed), R69 (cold-cost charge, debug-query registry,
  in-house A*), R72, R74, R76, R84 (cache reads never observe a miss), R124 (flow-cache eviction at the tick
  boundary; corrects R112's cost bound), R136 (HPA* same-cluster check and guaranteed cost bound).
* Revisit: when combat adds dynamic obstacles that move often (wrecks), and if the budget proves too conservative at
  scale (`path_budget_holds_at_scale`, the M1 bench).

## Refinements (2026-10-25, M2 prelude)

Status: proposed (M2 prelude Task 3); the controller accepts it at that task's review under the user's delegation
(R85), and the plan's final report lists it for ratification. From the M1 final review (`final-review-m1.md` M1, M4,
M5, M7, M8, M12, M13). The lanes, the stall rule and the reload invalidation are one class F change, MCR-002 (Task
15, `SIM_VERSION` 4, ADR-0021).

* **Two FIFO lanes** (M5). `PathQueue { commands, background, next_id }` (Hashed `core.path_queue` v2). A request
  enqueued in `Phase::Input` (a command handler) goes to the `Command` lane; every other request (order advance,
  follow and blocked re-plans, wander, group fallbacks) goes to the `Background` lane. `process_path_queue` pops
  `Command` requests while `spent < Path.budget_per_tick` (at least one per tick), then `Background` requests while
  `spent < budget`; if no background request was processed this tick and its lane is not empty, it processes exactly
  one more, so the background lane never starves. Charges stay cold costs, and results still apply in the order
  processed. The `core.paths` entry of a waiting unit and the `core.path_request` journal record gain `"lane"`.
* **Flow stall rule** (M4). A unit on a flow field whose step is fully blocked falls back to a single path only after
  `BLOCKED_REPLAN_TICKS` (10) consecutive ticks without moving, counted from `PathFollow.moved` (the last tick it
  moved; Hashed `core.path_follow` v2), not from `planned`. Any movement sets `moved`. A short stall no longer drops a
  member off its group's field for good (R130).
* **A value reload invalidates followed paths** (M1). After `rebuild_derived`, a value reload that changed any
  terrain def or the `Movement` or `Path` rules runs `invalidate_paths` over the whole map: every unit following a
  path or a flow field re-requests (journal `replanned`), as `debug.set_terrain` already does for its rect. A reload
  that changes no nav content keeps every path.
* **`core.passability` takes `since_version`** (M8). With `"since_version": v` equal to the grid's version it answers
  `{"unchanged": true, "version": v}`; every reply carries `"version"`. The passability overlay sends the last
  version it drew and keeps its cells on an unchanged reply.
* **`core.paths` answers immobile units** (M7). A unit without `Mobility` answers `{"state": "immobile"}` instead of
  failing the whole query. Its `"orders"` list names order kinds (`"core.move"`, …; ADR-0018).
* **One path reset** (M12, M13). `PathFollow::reset()` (state `Idle`, corners cleared, `next` 0) is the one copy of
  the reset logic, and `enqueue_group_in` inserts a missing `PathQueue` as `enqueue_path_in` does.
* **Confirmation.** `command_requests_go_first`, `background_lane_never_starves`,
  `path_queue_v1_migrates_to_command_lane`, `path_follow_v1_migrates_moved`, `flow_member_survives_short_stalls`,
  `flow_member_falls_back_after_stall`, `reload_that_blocks_a_path_replans`, `reload_without_nav_change_keeps_paths`
  (Task 15); `passability_since_version_unchanged`, `passability_after_terrain_change`,
  `passability_overlay_keeps_cells_when_unchanged` (Task 16); `paths_query_answers_immobile_units`,
  `stop_clears_queue_stack_and_path` (Task 13); the MCR-002 re-bless report, checked against its prediction.
