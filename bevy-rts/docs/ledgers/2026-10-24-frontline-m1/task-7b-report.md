# Task 7b report: HPA* cost-bound counterexample

**Status: BLOCKED.** I found and proved the root cause. A candidate fix works for HPA but changes the perf envelope a lot and breaks three flow/steering tests. Choosing between the options below is an architectural trade-off, so I am escalating it. Nothing is committed. The working tree of `C:\fl\a` holds the candidate (option 1+2), and the same diff is saved as `task-7b-candidate.patch` in this directory.

## Reproduction

- I copied the shrunk case into `crates/fl_sim/tests/hpa.proptest-regressions`. For `tests/hpa.rs`, proptest finds no `lib.rs`/`main.rs` above the file and falls back to `WithSource`, so that is the file name it reads.
- `cargo nextest run -p fl_sim --test hpa hpa_cost_near_optimal` fails deterministically on that seed: plan cost 3719, optimal 2361.

## Root cause (evidence)

I used a throwaway diagnostic test, plus an `eprintln!` of the abstract chain in `route`. Both were removed afterwards.

- **The request.** Profile Tracked size 2. Start (48,53) is in cluster (3,3). Goal (30,43) is in cluster (1,2).
- **The border run.** The border x=47|48 has one passable run, y=48..62 (15 pairs). Its west side, x=47, is a one-cell corridor: the lake at x=45..46 (size 2, so x=44 too) blocks x=44..46 for y=36..52.
- **The rule.** A run of 6 or more pairs gets transitions only at its ends, here y=48 and y=62.
- **The optimal path.** It crosses at y=53 and heads west at once: 2361.
- **The abstract chain.** `(48,48) g=515 → (47,48) 627 → (32,48) 3018 → (32,47) 3127 → (31,47) 3236 → goal 3719`. HPA walks 5 cells down the east side to the end transition, crosses, and walks 5 cells back up the corridor (inside cluster (2,3)) before it can turn west.
- **Smoothing is not the cause.** The smoothed corners `[(43,53),(39,50),(30,43)]` are near-optimal, because string-pulling removed the detour. But `PathPlan.cost` is the cost of the refined cell path, and that path carries the full detour.

**Conclusion.** This is HPA's inherent entrance-placement error, not a coding defect. Crossing at the nearest transition instead of where the optimal path crosses can cost up to `2 × reach × c_max` per border crossing. With end-only transitions on 16-cell runs, `reach` is 7. A start on the border makes the relative error large, which explains 1.575×. The test's `1.3 × optimal + 4 road cells` bound was never justified. Before my change, over 20,000 random requests (6,059 searched), it was violated 10 times, and the worst ratio was 1.775.

I checked the other candidates in the brief:
- Intra-edge costs use the same profile and clearance as refinement: one `dijkstra_from` with `self.profile`.
- The abstract heuristic is consistent: octile is a metric, and every edge is a real path cost.
- Refinement cost equals the abstract `g`.

**A second, latent gap.** With start and goal in one cluster, `route` returns the in-cluster A* result even when a route that leaves the cluster is far cheaper. Without fixing this, no cost bound holds. Over the 20,000 requests it happened 3 times.

## What the guaranteed bound is

The bound: `cost ≤ optimal + crossings × (2·reach + 2) × c_max`.
- `crossings` counts the cluster borders the optimal path crosses. A diagonal step through a cluster corner counts as 2.
- `c_max` is the costliest passable cell.

Proof sketch:
- Each crossing (a,b) of the optimal path lies in a run. Walk at most `reach` pairs along the run's a-line to a transition, cross, and walk back along the b-line. A diagonal crossing decomposes through its side cell, which no-corner-cutting guarantees is passable.
- Intra edges and the start/goal links are cheapest within their cluster, so each segment of this walk is bounded by an edge.
- The abstract A* is optimal.

This holds for the current entrances with reach 7, as long as the same-cluster gap is closed. The proptest in the candidate asserts it with reach 4.

## Candidate fix (in the working tree, uncommitted)

1. **Denser transitions.** A run of 6 or more pairs gets its ends, plus evenly spaced cuts so no gap exceeds 9 pairs (`start + g·(len−1)/gaps`). Runs of 6–10 pairs are unchanged; runs of 11–19 also get their middle. This makes `TRANSITION_REACH = 4`.
2. **Same-cluster check.** An in-cluster A* route stands only if its cost is at most `min over cells o just outside the cluster of octile(start,o)+octile(o,goal)`. That is a lower bound on any route that leaves the cluster. Otherwise the abstract route runs too and replaces the in-cluster one if strictly cheaper. The check is deterministic, and its work is charged.
3. **Tests (`tests/hpa.rs`).**
   - The proptest now asserts the guaranteed bound, using helpers `border_crossings` and `max_cost`.
   - New `border_corridor_crossing_stays_near_optimal`, a small replica of the counterexample. RED on the old code: cost 2666 against optimal 1546. GREEN: within optimal + 4 road cells.
   - New `same_cluster_takes_a_cheaper_route_out_of_the_cluster`. RED: 3450 against optimal 762. GREEN: equal to optimal.
   - `entrances_follow_run_rule` gains 10-, 11- and 16-pair runs.
   - `clusters_query_lists_entrances` expects 20 entrances instead of 12. This differs from the plan text, line 845.
   - `same_cluster_uses_local_astar` uses a shorter wall, so the in-cluster route still provably wins with no abstract search.
4. **ADR-0015** HPA section: the new entrance rule, the same-cluster rule, and the cost bound. Its stats sentence states the 20,000-request numbers below.

**Worst ratio over 20,000 random requests** (5,000 maps × 4 picks, the proptest's strategies, deterministic ChaCha seed; 6,059 searched):

| variant | worst cost/optimal | old-bound violations | guaranteed-bound violations |
|---|---|---|---|
| before (reach 7, no same-cluster check) | 2380/1341 = 1.775 | 10 | 0 (with reach 7 constants) |
| denser transitions only | 2905/2196 = 1.323 (a same-cluster case) | 0 | 0 |
| denser transitions + same-cluster check | 3006/2278 = 1.320 | 0 | 0 |

## Why I stopped

The full `cargo nextest run -p fl_sim -p fl_test_utils -p fl_catalog -p fl_cli` with the candidate gave 473/483 passing. The 10 failures:

- **Expected protected-artifact changes** (these would need `.new` files):
  - `golden_replays_verify`: testbed_wander diverges at tick 10.
  - `group_move`: `flowfield.abstract_nodes` and `path.budget_spent` max 15,888 against a budget of 10,555, roughly +88% on the recorded value.
  - `flow_thrash`: `path.queue_len` 19 against 6.
  - `alloc.bytes` about +30%: scale_500 166,976 against 122,415; scale_3000 259,782 against 193,690; scale_5000 681,224 against 516,650; choke_crowd 15,330 against 12,233.
- **Behaviour regressions** (not investigated to the end):
  - `flow::flow_units_reach_their_slots`: #3 ends 3.2 m from its slot.
  - `flow::flow_invalidated_by_terrain_change`: #2 ends 6.08 m from its slot.
  - `steering::flow_units_look_ahead_along_the_field`: 1 unit-tick stopped, where 0 is expected.

The cause is structural. On open ground a 16-cell border with transitions at its ends has 4 nodes per cluster, because the corners are shared. Any `reach ≤ 6` needs at least 2 transitions per border side, so 8 nodes per cluster. That doubles the HPA graph, its build and invalidation time, and every flow-field abstract Dijkstra, and it adds about 30% to per-request allocations (the per-request all-node buffers, R98). I checked the variants: inset transitions give the same 8 nodes per cluster.

As a control I set reach back to 7 and kept only the same-cluster check (fix 2). Flow, steering, golden and every scale budget then pass. Only `choke_crowd` exceeds: `path.nodes_expanded` 1,016 against 634, `path.budget_spent` 1,086 against 773, `alloc.count` 85 against 48 and `alloc.bytes` 22,748 against 12,233. The choke crowd's same-cluster requests near walls now also run the abstract search.

## Options for the controller

- **A. Accept the candidate (1+2).**
  - Re-record the golden replay and the budgets as `.new`.
  - Investigate the three flow/steering arrival tests. They are likely latent fragility in slot arrival exposed by different entrance geometry, but that is not shown.
  - Accept roughly 2× graph size, +88% flow abstract work and +30% allocations.
- **B. Same-cluster check only (2), reach 7.**
  - The test asserts the guaranteed bound `optimal + crossings × 16 × c_max`.
  - Plus the corridor test, re-scoped to document the known ≤ 2×7-cell detour, and the same-cluster test.
  - Smallest perf change: only `choke_crowd` budgets move, and they need `.new`.
  - The counterexample class stays: its cell-path cost stays about 1.5×, though the smoothed corners are near-optimal.
- **C. Neither.** Keep the entrances and replace the test's bound with the reach-7 guaranteed bound. This is pure re-justification of the test, which the brief discourages, and it leaves the same-cluster gap unbounded.

My recommendation is B, then revisit densification together with R98 (per-request buffers) when the perf work is scheduled. That is a judgment the lead should make.

## Files touched (uncommitted)

- `crates/fl_sim/src/nav/hpa.rs`
- `crates/fl_sim/tests/hpa.rs`
- `crates/fl_sim/tests/hpa.proptest-regressions` (new)
- `docs/adr/0015-path-requests-derived-caches-debug-queries.md`

The gate was not run. I wrote no `.new` files and did not dispatch any subagent.

---

## Ruling R136 (option B), implemented

The controller ruled option B. The densification is reverted: `transitions()` and the entrance docs are back to the committed rule (reach 7). The candidate is kept in `task-7b-candidate.patch` (densification plus the same-cluster check) for the R98 perf work.

### Changes

- **`crates/fl_sim/src/nav/hpa.rs`**
  - New `leave_bound`: the least `octile(start, o) + octile(o, goal)` over the cells `o` just outside the cluster. This is a lower bound on any route that leaves the cluster.
  - `route`: a same-cluster in-cluster A* result stands if its cost is at most `leave_bound`. Otherwise the abstract search runs too, and its route replaces the local one only if strictly cheaper (ties keep the local route). If the abstract search reaches no target, the local route stands. The extra work is charged as usual.
  - The module doc gains a "Cost bound (R136)" paragraph with the derivation, and `plan_path` step 4 is updated.
- **`crates/fl_sim/tests/hpa.rs`**
  - `hpa_cost_near_optimal` now asserts `cost ≤ optimal + border_crossings(optimal path) × 16 × c_max`. The derivation is in the `CROSSING_STEPS` doc. `Case.optimal` now carries the optimal path. New helpers: `border_crossings`, `max_cost`, `corner_path_cost`.
  - `border_corridor_detour_is_bounded_and_smoothed_away`: a small replica of the counterexample. Cluster 1 has entrances at `(16,0)` and `(16,15)` only.
    - The cell path costs 2666 against an optimal 1546. The test asserts `optimal < cost ≤ optimal + 2·7·cell`, which documents the known border detour.
    - The driven corners `[(11,5),(3,3)]` cost exactly the optimum: `corner_path_cost` (the cheapest cell path through the corner cells) is 1546, asserted equal.
  - `same_cluster_takes_a_cheaper_route_out_of_the_cluster`: the plan costs exactly the optimum, 762, where the in-cluster route costs 3450. RED on the pre-fix code: 3450 against 762.
  - `same_cluster_uses_local_astar`: the wall is now `y ≤ 5`, so the in-cluster route provably beats any leaving route and the test still asserts "no abstract search". With the original `y ≤ 12` wall the leave bound (1,800) is below the in-cluster cost, so the abstract search would run (and lose).
  - `entrances_follow_run_rule` and `clusters_query_lists_entrances`: unchanged from HEAD (12 entrances).
- **`crates/fl_sim/tests/hpa.proptest-regressions`** (new): the Task 10b seed. It passes under the guaranteed bound.
- **ADR-0015**, HPA section:
  - The same-cluster rule.
  - The "Cost bound (R136)" paragraph with its derivation and the observed ratios.
  - Densification parked with R98.
  - The new tests in the test list, and R136 in the rulings.

### Observed ratios (at least 2,000 cases)

Method: a throwaway harness (not committed), deterministic ChaCha `TestRunner`, the proptest's own strategies. 5,000 maps × 4 start/goal picks = 20,000 requests; 6,059 were searched (the rest were direct line or no route).

- **Cell path** (`PathPlan.cost`): worst 2380 / 1341 = **1.775**. The old `1.3×+4` bound failed 10 times. The guaranteed bound held in all 6,059.
- **Driven corners** (cheapest cell path through the corner cells): worst 2190 / 1581 = **1.385**. Smoothing ignores terrain cost, so a straight leg through forest can cost more than the refined path did.
- **Same-cluster requests costlier than optimal:** 3. In each, both the local and the abstract route exceed the optimum; both stay within the bound.

### Tests

- `cargo nextest run -p fl_sim -p fl_test_utils -p fl_catalog -p fl_cli` before recording: 482 run, 481 passed. The one failure is `counters_choke_crowd`:
  - `alloc.bytes` 22748 > 12233
  - `alloc.count` 85 > 48
  - `path.budget_spent` 1086 > 773
  - `path.nodes_expanded` 1016 > 634
- The choke crowd's same-cluster requests near its walls now fail the leave bound and also run the abstract search. Every other snapshot total is unchanged, so this adds work but no outcome change shows in the counters. The golden replay, the flow tests and every other perf scenario pass as committed.

### Protected artifacts (`.new`, untracked, not blessed)

- **`crates/fl_test_utils/tests/snapshots/perf_counters__choke_crowd.snap.new`**: `path.budget_spent` 9194 → 16493, `path.nodes_expanded` 7191 → 14490. Nothing else changes.
- **`docs/perf/budgets.ron.new`** (`FL_RECORD_BUDGETS=1`, `counters_choke_crowd` only; other scenarios carried over): `choke_crowd` `alloc.bytes` 12233 → 28435, `alloc.count` 48 → 107, `path.budget_spent` 773 → 1358, `path.nodes_expanded` 634 → 1270.
- **Suggested bless reason:** `M1 Task 7b (R136): HPA* same-cluster detour check adds an abstract search to choke-crowd requests`.

### Concern

In the choke scenario, path work per tick roughly doubles: the max goes from 634 to 1270 nodes expanded. It stays far below `Path.budget_per_tick`, but it is the cost of R136's guarantee. A tighter leave bound would cut it. For example, the cheapest real route out (an in-cluster Dijkstra to the border cells plus octile beyond) would replace the octile ring, but at Dijkstra cost.

### Gate

The first gate run's test-guard flagged that I had dropped `direct_line_costs_only_los` by mistake while reverting the entrance tests. I restored it unchanged.

Second run: `cargo xtask gate task --track contract,sim-core,perf,docs`

```
PASS fmt / clippy-drift / claude-md / check-scope / protected
WARN test-guard: 1 findings — hpa.rs:772 RemovedAssert: the old `cost * 10 <= 13·optimal + 40·COST_ROAD` prop_assert,
     replaced by the guaranteed per-crossing bound (R136)
PASS clippy / arch-check
FAIL test-headless: 438 run, 437 passed — counters_choke_crowd budgets (bless-pending budgets.ron.new + snap.new)
SKIP test-client
PASS api (no API change) / data-check
gate task: FAIL (test-headless) in 180s
```

I expect the gate to pass once `budgets.ron.new` and `perf_counters__choke_crowd.snap.new` are blessed.

---

## Fix round 1 (review Minors 1–6, PROPTEST_CASES)

Commit `285c46f`, on top of `48e070c` (the Task 19 merge).

### Changes

- **Minor 4.** `HpaGraph::search` takes a `limit` and stops, before settling, at the first popped entry whose f is at least `limit`. `route` passes the local route's cost when one exists, and `u32::MAX` otherwise.
  - The heuristic is consistent, so nothing cheaper than the limit remains, and ties already keep the local route. The outcome is therefore identical.
  - The work is still a pure function of the grid (no cache is involved, R84).
- **Minor 3.** Two new tests share the helper `local_beats_abstract`:
  - `same_cluster_local_route_wins_over_the_abstract_search` uses the original `y ≤ 12` wall. The local cost exceeds the 1,800 leave bound, and the plan equals the local route: same cost and corners. Charge pinned: `nodes_expanded` = local A* + the two endpoint Dijkstras + 1 (the start), and `los_cells` = line test + smoothing.
  - `same_cluster_local_route_stands_when_the_abstract_search_finds_none`: water on cluster 0's east and north edges leaves it no entrances. The local route stands, with the same exact charge.
- **Minor 2.** `REACH = (CLUSTER as u64 − 1) / 2`. `CROSSING_STEPS = 2 × REACH + 2`, and the corridor test asserts `≤ optimal + 2 × REACH × cell`.
- **Minor 5.** `hpa_cost_ratio_tripwire`: a fixed ChaCha seed, 200 maps × 4 requests, the proptest strategies, and one nav/HPA build per map. Every searched plan must be within the guaranteed bound, and the worst cell-path ratio must be ≤ 1.8. It runs in about 6 s.
- **Minor 1.** The module doc's cost-bound paragraph is rewrapped to ≤ 100 columns.
- **Minor 6.** The `corner_path_cost` doc now calls it a lower bound on driving the straight legs.
- **PROPTEST_CASES.** The hpa proptest block uses `ProptestConfig::with_cases(cases(64))`. `cases` reads `PROPTEST_CASES`, behind a scoped `#[expect(clippy::disallowed_methods)]`.
  - Checked: `PROPTEST_CASES=640` makes `hpa_cost_near_optimal` take 15.4 s instead of 1.9 s.
  - At the nightly 4096, `invalidate_equals_full_rebuild` will take about 15 minutes, since it ran 64 cases in about 14 s.

### TDD evidence

- **RED**, before the cutoff: `cargo nextest run -p fl_sim --test hpa same_cluster_local` gave `left: 629, right: 624`. The abstract search settled 5 nodes where the pinned charge expects 1. The no-target test passed already, since there is nothing to cut there.
- **GREEN** after the cutoff: both pass, settling 1 node (the start).

### choke_crowd

The cutoff barely moves it. Per-tick `path.nodes_expanded` max is 1016 before this round and 1014 after; the totals go 14490 → 14475.
- **Why.** The extra charge is not the abstract search. It is the two in-cluster endpoint Dijkstras (up to 256 cells each) that run before the search whenever the local route fails the leave bound.
- **What I tried.** I restricted the leave bound's ring to passable cells. That is still a valid lower bound, but it changed nothing and I reverted it.
- **What would recover the rise.** Making those fields cheaper, or using a bound derived from them, which is a design change.

### Tests

- `cargo nextest run -p fl_sim -p fl_test_utils -p fl_catalog -p fl_cli`: 486 run, 485 passed. The one failure is `counters_choke_crowd` (budgets pending):
  - `alloc.bytes` 21788
  - `alloc.count` 69
  - `path.budget_spent` 1084
  - `path.nodes_expanded` 1014
- The golden replay passes unchanged, and no other `.new` file appeared.

### `.new` files (regenerated, not blessed)

- **`perf_counters__choke_crowd.snap.new`**, against the committed snapshot: `path.budget_spent` 9194 → 16478, `path.nodes_expanded` 7191 → 14475.
- **`budgets.ron.new`**, `choke_crowd` only:
  - `alloc.bytes` 12233 → 27235
  - `alloc.count` 48 → 87
  - `path.budget_spent` 773 → 1355
  - `path.nodes_expanded` 634 → 1268

### Gate

`cargo xtask gate task --track contract,sim-core,perf,docs`:

```
PASS fmt / clippy-drift / claude-md / protected / clippy / arch-check / test-client / api / data-check
FAIL check-scope: 16 of 24 changed files outside the tracks — fl_client/fl_game files of the Task 19 merge (48e070c);
     .gate-base is still f09c5b6, so the merge is inside "this task's diff". None of them is mine.
WARN test-guard: 2 — hpa.rs:772 RemovedAssert (accepted in review); hpa.rs:950 NewSeedOverride (the fixed-seed
     tripwire Minor 5 asked for)
FAIL test-headless: 456 run, 455 passed — counters_choke_crowd (bless-pending .new)
WARN gate task took 320s (budget 300s)
gate task: FAIL (check-scope, test-headless) in 320s
```

I expect both failures to clear once `.gate-base` is advanced past the Task 19 merge and the two `.new` files are blessed.
