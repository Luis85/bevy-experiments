# Task 10 report: flow fields for group moves

**Status:** DONE_WITH_CONCERNS (all concerns are minor; see the end)
**Commit:** `44aa360 feat(fl_sim): flow fields for group moves — abstract integration per (cluster, profile), lazy tiles, outcome-neutral cache` (base `4e79841`, `Refs: ADR-0015`; no `Blessed:` trailer: the lead blesses the two `.new` files below)

## What I implemented

### `crates/fl_sim/src/nav/flow.rs` (new)
- `FlowCache` is `#[derive(Resource, Default, Debug)]` and `Derived` `"core.path_flow_cache"`. It keeps a `BTreeMap<FlowKey, Entry>`, a `max` (`Path.flow_cache_max`; 0 means unbounded), a `cold` flag and `now` (the LRU tick).
  - Each entry holds the **abstract integration**: a Dijkstra over the profile's HPA* graph from every node of the destination cluster at cost 0. It pops `(cost, node index)` and lowers a cost only on a strictly lower value. Its settled count is the cold cost.
  - Each entry also holds **lazy tiles**, `BTreeMap<ClusterId, Tile>`. A tile is a `dijkstra_seeded` inside the cluster, seeded at each node of the cluster with its cheapest exit: `min(cost(q) + w)` over the edges to nodes `q` of other clusters, ties to the lower node index. The tile records the exit `q` of each seed.
  - The destination cluster has no tile. Every passable cell in it is `Arrived`.
- `cold_cost`, `ensure_tile`, `direction`, `clear_profile`, `len`, `is_empty`, `new(max, cold)` and `is_cold` are public. `begin_tick` is `pub(crate)`.
  - `direction` returns `Arrived` for a passable cell of the destination cluster. Otherwise it returns `Toward(parent cell)`, or for a seed, `Toward(exit cell across the border)`. It returns `None` only when there is no route: the cell is blocked or off the map, nothing connects it, or the profile has no graph.
  - **R84:** a missing integration or tile is built before `direction` answers. The work goes into `FlowWork`, and the path budget is never charged for it.
- **Why the field is consistent** (documented in the module docs): a node's tile cost equals its integration cost. So every step lowers the cost to the destination by at least one road step, and following directions never revisits a cell. The proptest checks this.
- **Eviction:** after inserting a new key, entries are dropped past `max`, smallest `(last_used, key)` first, never the key just inserted. `begin_tick(tick)` empties a cold cache the first time the cache is used in a new tick. Nothing reads the cache earlier in a tick, so this is the same as clearing it at the start of every tick.
- Spans `flowfield_build` (`kind = "abstract"` or `"tile"`). Per-build logs are at `trace`.

### `nav/astar.rs` (R95)
- Added `dijkstra_seeded(nav, p, &[(CellXY, u32)], bounds, stats)`: several sources, each with its own starting cost. It skips seeds that are blocked or outside the bounds, and a cell seeded twice keeps its lower cost.
- Added `DistanceField::next_toward_source(c)`, which returns the parent cell. `edge_cost` and the corner rule are symmetric, so this is the first step toward the sources.
- `dijkstra_from` now calls `dijkstra_seeded(&[(source, 0)])`, with unchanged behaviour. All the A* and HPA* tests and the golden replays pass unchanged.

### `nav/hpa.rs`
- Added `pub(crate)` accessors that flow fields need: `grid_cells`, `cluster_nodes`, `node_cell`, `node_cluster` and `edges`. The public API is unchanged.

### `nav/mod.rs` (R74, R100, R101)
- `install_nav`, the one shared setup and load helper (R101), now ends with `reset_flow_cache`: a new empty `FlowCache::new(Path.flow_cache_max, cold)` that keeps the old cache's `cold` flag. `rebuild_nav` without terrain also resets it.
- `nav::build` does `init_resource::<FlowCache>()` and registers it as Derived.
- `on_terrain_changed`: when `HpaGraphs::invalidate` returns a non-empty union, it calls `clear_profile` for every profile of the grid (R100: every profile's graph rebuilds the touched clusters). Units on a flow field keep `PathState::Flow`, and their next read rebuilds the field from the new terrain.

### `build.rs`
- Added `RunConfig.flow_cache_cold: bool`. `build_sim_app` inserts `FlowCache::new(0, run.flow_cache_cold)`. Setup and load then replace it with the content's max and keep the flag, so a loaded save keeps the setting.

### `commands.rs` (`core.move`)
- Slots are `formation_targets`, then `space_slots` (see below).
- **Not queued:** units are grouped by profile with `unit_profile`. Each profile with at least `Path.flow_min_group` units gets one `enqueue_group_in` request toward the clamped command target. Groups are enqueued in `MoveProfile` order, then single requests in `SimId` order. Units without a profile (no `NavGrid`) ask for single requests.
- **Queued:** unchanged. Single requests for units whose move became current.
- R104: no new payload. The `MoveCmd` target was already clamped before any arithmetic. `space_slots` only works on clamped slots, and its candidates are clamped too.

### `path.rs`
- `enqueue_group_in` pushes a `RequestKind::Group { units }` request with `unit` = the first member and `goal` = the group target. It sets each member to `Waiting` with its own slot as goal and journals one `core.path_request` of kind `"group"` with `"units": [...]`. The `units` field is left out for single requests, so their JSON is unchanged.
- `process_path_queue` handles groups through `process_group`:
  - The waiting members are the ones that still exist and are `Waiting` on this id, in `SimId` order. If there are none, the request is `"stale"` with charge 0. Members that left are skipped.
  - Key = (cluster of `request.goal`, profile of the first waiting member).
  - Charge = `cold_cost`, which is pure.
  - A member whose start cell has a `direction` is set to `Flow { key }` with its goal, `planned` = this tick. The others get single requests at the back of the queue, in `SimId` order.
  - It journals `path_result` with status `"flow"`, `corners: 0`, `cost: 0` and the charge. The charge goes into `path.budget_spent` and the flow work into `flowfield.*`.
- R99: nothing compares plan costs. The group result's `cost` is 0 and documented as such.

### `movement.rs`
- `prepare_flow_tiles` is exclusive and uses a cached `QueryState`, with its scratch list in the Transient `OrderScratch.flow` (no `Local`). It runs in the `ActSet::Move` core slot, chained before `move_system`, in `SimId` order, and calls `ensure_tile` for the cluster each `Flow` unit stands in. It is a warm-up only.
- `move_system` was refactored into `speed_at`, `drive` and `to_idle`. The Following logic is the same, statement for statement.
  - A `Flow` unit reads `direction`. With `Toward(c)` it drives toward `cell_centre(c)`, snapping onto it when within one step; a blocked step counts `move.blocked` and the unit goes Idle.
  - With `Arrived`, if `line_clear(pos, goal)`, it switches to `Following [goal]` and drives in the same tick. Otherwise it goes `Idle`, and `advance_orders` asks for a single request the next tick.
  - With `None`, it goes `Idle`.

### `orders.rs`: minimum slot spacing (Task 9 review Minor 2)
The flow-field group move uses formation slots, so this carry applies.
- **`pub fn space_slots(slots, pitch, map)`**, deterministic and in slot order:
  1. A slot at least `pitch` from every placed slot keeps its place.
  2. Otherwise it takes the first free candidate `slot + (dx, dy)·pitch`, clamped to the map. Candidates go by Chebyshev ring, and within a ring by `(dx² + dy², dy, dx)`. The search stops at `⌊√n⌋ + 1` rings and falls back to the original slot, which can only happen when the clamp folds candidates together on a tiny map.
  3. Squared distances are exact `Fx`. A pitch of 0 or less does nothing, and the pitch is capped at 4096 m. Placed slots are bucketed by pitch squares in a `BTreeMap`.
- **Pitch** (`commands::slot_pitch`): the largest footprint side among the command's units, plus `Steering.separation_margin_m`. Tanks get 6.5 m.
- A dense group therefore spreads past `formation_max_spread_m`, to about `√n·pitch` across. This is documented. Existing golden replays and formation tests are unaffected, because their units are already further apart than the pitch.

### `fl_test_utils/src/builder.rs` (outside the Files list)
- Added `ScenarioBuilder::flow_cache_cold(bool)`, which sets `RunConfig.flow_cache_cold`. `reload` keeps it.

### `tests/pathing.rs` (outside the Files list)
- `rules_with_budget` now also raises `flow_min_group` to 64. Otherwise `forty_movers` (40 tanks in one command) would become one group request, and the Task 8 queue, budget and ordering tests (`queue_respects_budget_and_order`, `no_request_starves`, `path_counters_and_journal`, `branch_with_queued_requests`) would no longer test single requests. Their assertions are unchanged.

## Tests (`crates/fl_sim/tests/flow.rs`, 18)
- **Brief tests (11):**
  - `group_move_uses_one_flow_request`: one `"group"` request listing units 1–8 and profile Tracked/1, one `"flow"` result with charge > 0 equal to `path.budget_spent`. All 8 units are `Flow`, with their own slot as goal and the key cluster equal to the target's cluster.
  - `small_group_uses_single_requests`: 5 single requests.
  - `mixed_profiles_split`: 6 tanks, 6 riflemen and a truck give request 0 = Foot group, request 1 = Tracked group, request 2 = the truck's single request, and two `"flow"` results.
  - `flow_units_reach_their_slots`: 8 tanks on a 48×48 map whose river has two bridges. All are within 2 m of their slots by tick 900, never on water, and every unit crossed by the near bridge A.
  - `flow_invalidated_by_terrain_change`: bridge A becomes water at tick 10. The group stays on its field, and every unit arrives within 2 m of its slot after crossing by bridge B only, never on water.
  - `cache_does_not_change_outcomes`: 600 ticks with a cold and a warm cache, terrain change included, give equal hash streams. It also asserts the cold run built more abstract nodes, so the test is not vacuous.
  - `flow_cache_overflow_does_not_change_outcomes` (R84): `flow_cache_max: 2` and four 6-tank groups to four corner clusters. At tick 5, 4 distinct keys are in use with `FlowCache::len() ≤ 2`. Over 600 ticks the cold and warm hash streams are equal.
  - `branch_mid_flow_follow`: at least 4 units are `Flow` at n = 40. `assert_branch(n, n + 200)` runs on the plain and the bridge-lost scenarios.
  - `flow_directions_reach_destination`: proptest, 64 cases on 48×48 random maps (TANK or WIDE profile, random destination cluster). It checks that a cell has a direction iff a full-map `dijkstra_seeded` from the destination cluster's passable cells reaches it, and that the walk takes only neighbour steps, never revisits a cell and ends `Arrived` in the destination cluster.
  - `flow_counters_recorded`: `flowfield.abstract_nodes` > 0 on the request tick, and `flowfield.cells` > 0 on some later tick while units are on the field.
  - `smoke_flow_twin_and_shuffle`: twin and shuffle runs, 300 ticks, bridge-lost scenario.
- **Extra tests (7):**
  - `queued_group_move_uses_single_requests`
  - `stale_group_request_is_discarded`: move, then stop in the same tick, gives `"stale"` with charge 0.
  - `members_that_left_are_skipped`
  - `unrouted_member_gets_a_single_request`: an island tank gets a single request after the group, with result `"partial"`.
  - `space_slots_pinned`
  - `spread_group_slots_keep_minimum_spacing`: 256 units in two clumps 1 km apart all end up at least a pitch apart.
  - `move_command_spaces_formation_slots`: 64 tanks packed 2 m apart get slots at least 6.5 m apart through `core.move`.

### TDD evidence
- **RED:** I stashed the implementation (`crates/fl_sim/src`, `crates/fl_test_utils/src`), kept the new test file, and ran `cargo nextest run --profile agent -p fl_sim --test flow`. It fails to compile, as expected, since the API does not exist yet:
  ```
  error[E0432]: unresolved imports `fl_sim::nav::FlowCache`, `fl_sim::nav::FlowDir`, `fl_sim::nav::FlowWork`, `fl_sim::nav::dijkstra_seeded`
  error[E0432]: unresolved import `fl_sim::space_slots`
  error[E0599]: no method named `flow_cache_cold` found for struct `ScenarioBuilder` in the current scope
  error: could not compile `fl_sim` (test "flow") due to 4 previous errors
  ```
  Order caveat: I wrote the implementation before the tests and got this RED afterwards by stashing, so it shows the tests depend on the new code, not that I wrote them first.
- **Mutation checks of the R84 oracle** (temporary edits, reverted; `flow.rs` restored byte-identical):
  - `direction` returns `None` on a tile miss when cold: both `cache_does_not_change_outcomes` ("a cold flow cache changed the hashes") and `flow_cache_overflow_does_not_change_outcomes` ("an overflowing flow cache changed the hashes") FAIL.
  - `direction` returns `None` on a miss when `max == 2`: `flow_cache_overflow_does_not_change_outcomes` FAILs.
- **GREEN:** `cargo nextest run --profile agent -p fl_sim --test flow` gives `18 tests run: 18 passed`.

### Full suite (required command)
`cargo nextest run --no-fail-fast -p fl_sim -p fl_test_utils -p fl_catalog -p fl_cli` gives `423 tests run: 422 passed, 1 failed`. The one failure is `fl_test_utils::system_order smoke_sim_tick_system_order`, the system-order snapshot waiting for bless. Golden replays, `formats__*` snapshots, `perf_counters__*` snapshots and `check_budgets` all pass unchanged. No `budgets.ron.new` and no `*.replay.new` were produced: the scale mix issues only single (wander) requests, and the golden replays' 2-unit moves are already further apart than the pitch.

## Protected artifacts (`.new`, never blessed by me)
1. `crates/fl_test_utils/tests/snapshots/system_order__sim_tick.snap.new`. One line added:
   ```
   > Act/Move fl_sim::movement::prepare_flow_tiles     (before Act/Move fl_sim::movement::move_system)
   ```
2. `xtask/snapshots/api/fl_sim.txt.new` (`cargo xtask api`: "1 of 5 crates changed: fl_sim"). It adds:
   - `nav::flow::{FlowCache, FlowDir, FlowWork}`, with re-exports in `nav` and FlowCache's methods `new`, `is_cold`, `cold_cost`, `ensure_tile`, `direction`, `clear_profile`, `len` and `is_empty`;
   - `nav::astar::dijkstra_seeded` (re-exported in `nav`) and `DistanceField::next_toward_source`;
   - `RunConfig::flow_cache_cold`;
   - `fl_sim::space_slots`.

   Nothing was removed or changed.

Suggested bless reason: `M1 Task 10: flow fields`.

## Gate
`cargo xtask gate task --track contract,sim-core,perf`. The contract track is needed for `commands.rs`, as in Task 9; perf per R103.
```
PASS fmt (1.9s)
PASS clippy-drift (1.8s)
PASS claude-md (0.0s)
PASS check-scope (0.2s)
PASS protected (0.2s)
WARN test-guard (1.1s): test-guard: 1 findings (reviewer must accept each)
    TEST-GUARD crates/fl_sim/src/commands.rs:705 ToleranceChanged: let margin = world
PASS clippy (38.3s)
PASS arch-check (3.7s)
FAIL test-headless (165.0s)      <- only system_order__sim_tick.snap (bless pending, diff above)
SKIP test-client: no client package touched since 4e79841 (.gate-base)
FAIL api (26.7s)                 <- fl_sim changed (bless pending, .txt.new written)
PASS data-check (45.4s)
gate task: FAIL (test-headless, api) in 285s
```
The test-guard finding is a false positive. Line 705 is `let margin = world…rules::<SteeringRules>().separation_margin_m` in the new `slot_pitch`, which is sim code, not a test tolerance.

## Files changed
- `crates/fl_sim/src/nav/flow.rs` (new)
- `crates/fl_sim/tests/flow.rs` (new)
- `crates/fl_sim/src/{build,commands,lib,movement,orders,path}.rs`
- `crates/fl_sim/src/nav/{mod,astar,hpa}.rs`
- `crates/fl_sim/tests/pathing.rs` (outside the Files list)
- `crates/fl_test_utils/src/builder.rs` (outside the Files list)

## Self-review notes
- **Determinism:**
  - No `Local`, no hash maps, no floats, no `Entity` keys. Cache maps are `BTreeMap<FlowKey, _>` and `BTreeMap<ClusterId, _>`, and per-node and per-cell data are `Vec`s.
  - Every heap key is unique per live entry, and every tie-break is explicit: `(cost, node)`, `(g, y, x)`, exit ties by node index, eviction by `(last_used, key)`.
  - Outcome dependence on the cache: none. `direction` and `cold_cost` are pure, and `move_system`'s any-order iteration only fills the cache. The cold, warm and overflow oracle tests check this.
- **Counters (R86):** `flowfield.*` include read-miss rebuilds and depend on cache state. They are not hashed, and the scale-mix perf counters stay unchanged because the scale mix makes no groups.
- `move_system` computes a unit's profile lazily, only for Flow and Following units, so the idle-unit cost at scale is unchanged.

## Concerns
1. **Out-of-list edits:** `tests/pathing.rs` (`flow_min_group` raised so the Task 8 queue tests keep using single requests) and `fl_test_utils/src/builder.rs` (`flow_cache_cold` builder option, needed by the brief's cache tests). Both are in the sim-core track.
2. **Flow steering is cell-centre to cell-centre**, as the brief says. With `turn_in_place_deg` 30, every 45° bend makes a tank turn in place for about 3 ticks. That is jerky and slower than a smoothed path. A line-of-sight lookahead along the direction chain would smooth it, at some line-of-sight cost (Task 11 or later).
3. **`Arrived` means "inside the destination cluster".** A unit whose slot is hidden behind an obstacle in that cluster, or whose slot lies in a neighbouring cluster (slots spread up to about 40 m, and clusters are 64 m), goes Idle on entering the cluster and makes one extra single request. This is bounded at one request per unit.
4. **Cold cost grows with the map.** The integration settles every node reachable in the profile's graph, wherever the group stands. On a 4096² map one group request can exceed `budget_per_tick`; it is then processed alone in its tick (the at-least-one rule, as for partial plans, R98).
5. **Slot spacing ignores terrain** (as `formation_targets` does). A spaced slot can land in water; the plan then snaps it, or the unit stops at the shore. Groups denser than footprint plus margin now spread past `formation_max_spread_m`.
6. The cold cache is emptied at the first cache use of each tick, not by a separate Housekeeping system. The effect is the same, because nothing reads the cache before its first use in a tick, and it avoids a further system-order change.

---

## Fix round 1 (review: task-10-review.md; rulings R109–R112)

**Commit:** `4492a78 fix(fl_sim): Task 10 fix round 1: truthful group journal, corner-rule and budget/branch/overflow tests, R110/R112 docs`

### Changes
- **(a) Minor 3: the journal tells the truth** (`path.rs`).
  - `process_group` now returns a private `GroupOutcome { Flow { charge }, Single, Stale }`.
  - A group whose profile has no navigation grid or HPA* graph is journaled `path_result` status `"single"` with charge 0. Its members ask for single requests, as before.
  - The module docs and the `PathResult.status` docs describe the three group statuses.
- **(b) Minor 5:** `flow_directions_reach_destination` (proptest) now asserts that both side cells of every diagonal step are passable for the profile (the corner rule).
- **(c) Minor 6: new tests in `tests/flow.rs`.**
  - `group_request_waits_for_budget`: `budget_per_tick` 1, and a truck's single request ahead of an 8-tank group. After tick 0 the queue holds exactly one `RequestKind::Group` and the tanks are `Waiting`. At tick 1 the group's result is `"flow"` with `waited` 1, and all 8 tanks are `Flow`.
  - `branch_with_queued_group_request`: asserts the group is still pending after tick 0, then runs `assert_branch(group_behind_budget, 1, 201)`. This exercises a pending Hashed `Group` request through save and load.
  - `group_without_a_graph_is_journaled_single`: replaces `HpaGraphs` with Foot-only graphs between ticks. It checks 9 results (the group, then 8 single requests), the group result `"single"` with charge 0, no `"flow"` result, and no unit on a field.
  - `flow_cache_overflow_does_not_change_outcomes` is strengthened:
    - A roomy run (`flow_cache_max` 32) builds each integration exactly once, so its abstract nodes equal the sum of the 4 group charges, pinned with `assert_eq`.
    - The overflowing (max 2) and cold runs must match the roomy run on the hash stream, on `(request, charge)` of every `"flow"` result, and on the summed `path.budget_spent`. This catches a cache that re-charges or charges by cache state.
    - Each of those runs must build more abstract nodes than the sum of the charges. This catches a cache that never rebuilds evicted keys, so reads would miss.
- **(d) R110: slot spacing wins over `formation_max_spread_m`, so a dense group spans about √n × pitch.**
  - Spec §7: the "Group moves" bullet now states the minimum spacing.
  - Plan, Task 9 interface comment: amended to match.
- **(e) R112: read-miss rebuilds are not charged to the path budget.** Thrash cost is bounded by `flow_cache_max`, not by the budget, and Task 13 measures it.
  - ADR-0015 R84 bullet: new sub-bullet saying this, with the two outcome-neutral caps as options.
  - Plan, Task 10 `direction` comment and the R84 line of the self-review section: amended to match.

### TDD evidence
- **RED:** the new tests were written first and run against the unchanged implementation with `cargo nextest run --profile agent --no-fail-fast -p fl_sim --test flow`: `21 tests run: 20 passed, 1 failed`.
  ```
  FAIL fl_sim::flow group_without_a_graph_is_journaled_single
  panicked at crates\fl_sim\tests\flow.rs:630:5: assertion `left == right` failed
    left: String("flow")
   right: "single"
  ```
  This was expected, because the old code journaled `"flow"` with charge 0 in this case. The other new tests and strengthened asserts passed at once: they cover behaviour that already existed (budget hold-back, branch with a queued group, the corner rule, charge invariance). No git stash was used.
- **GREEN:** after the fix, the same command gives `21 tests run: 21 passed, 0 skipped`.

### Full suite
`cargo nextest run --no-fail-fast -p fl_sim -p fl_test_utils -p fl_catalog -p fl_cli` gives `426 tests run: 425 passed, 1 failed`. The one failure is `smoke_sim_tick_system_order`, still waiting for bless.

### `.new` files: unchanged
- `fl_sim.txt.new`: I re-ran `cargo xtask api`, and the result is byte-identical (`cmp`) to the round-0 file. `GroupOutcome` is private.
- `system_order__sim_tick.snap.new`: byte-identical after the full run. No system was added.

### Gate
`cargo xtask gate task --track contract,sim-core,perf,docs`:
```
PASS fmt (1.8s)
PASS clippy-drift (1.7s)
PASS claude-md (0.0s)
PASS check-scope (0.2s)
PASS protected (0.2s)
WARN test-guard (1.2s): test-guard: 1 findings (reviewer must accept each)
    TEST-GUARD crates/fl_sim/src/commands.rs:705 ToleranceChanged: let margin = world   (accepted, R109)
PASS clippy (30.3s)
PASS arch-check (3.2s)
FAIL test-headless (136.0s)   <- only system_order__sim_tick.snap (bless pending, unchanged .new)
SKIP test-client: no client package touched since 4e79841 (.gate-base)
FAIL api (25.6s)              <- fl_sim changed (bless pending, unchanged .new)
PASS data-check (32.8s)
gate task: FAIL (test-headless, api) in 233s
```

### Carried, not done here (per the coordinator)
- Minor 1 (eviction thrash within a tick) and Minor 2 (cold cost grows with the graph) go to Task 13, which measures them.
- Minor 4 (per-class slot pitch) goes to C7.
