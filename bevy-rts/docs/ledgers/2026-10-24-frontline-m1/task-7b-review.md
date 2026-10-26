# Task 7b review: HPA* same-cluster detour check and guaranteed cost bound (R136 option B)

Diff: `review-t7b.diff` (commit fdfcfce, plus the choke_crowd `.new` diffs). I read it in two passes (lines 1–536, then 536–668). No changed file was read separately. I ran no tests.

### Spec Compliance

- ✅ Spec compliant with R136 option B.
  - **Same-cluster check:** `hpa.rs` (diff lines 76–91 `leave_bound`, 104–125 and 150–162 `route`).
  - **Guaranteed bound in the proptest:** `tests/hpa.rs` (diff lines 441–447 and 515–526).
  - **Corridor test:** documents the detour and asserts the corners are optimal (diff lines 254–294).
  - **Same-cluster test:** diff lines 341–380.
  - **Regressions file committed:** diff lines 199–211.
  - **ADR-0015 derivation:** diff lines 557–577.
  - **Densification parked with R98:** diff line 577. `transitions()` is untouched.
  - **More cases (brief step 4):** 20,000 requests, 6,059 of them searched. Worst cell ratio 1.775, worst driven ratio 1.385 (report, "Observed ratios").
- ✅ **The bound `optimal + crossings × 16 × c_max` is correct.** I checked the derivation step by step against the code:
  1. **Reach is 7.** A run of 6 or more pairs has transitions at its ends, so the farthest pair is ⌊(len−1)/2⌋ ≤ 7 away when len ≤ 16. A run shorter than 6 has its middle transition at most 2 away. Every pair in a run is passable on both sides, so walking along the a-line or b-line takes only orthogonal steps between passable cells.
  2. **Each walk step costs at most c_max.** I checked this risk in `astar.rs:66-75`. `edge_cost` for an orthogonal step is `ceil((ca+cb)/2)`, which is at most c_max. A diagonal step costs ×1.41, but the derivation never walks a diagonal: a diagonal crossing goes through its side cell, which is passable because no corner is cut. So an orthogonal crossing adds at most 7+1+7 = 15 steps, a diagonal crossing at most 16, and a corner diagonal (counted as 2 crossings) at most 30 against an allowance of 32.
  3. **Intra edges and the start/goal links never cost more than the path they replace.** The walk back on the b-line runs along the border column or row of the next cluster. So each replacement piece is an in-cluster path between two transition nodes, or between the start or goal and a node. Its in-cluster Dijkstra edge or link costs no more than that piece.
  4. **The abstract A* is optimal.** The octile heuristic is admissible and consistent (`astar.rs:80-84`: road cost is the cheapest step, a diagonal costs 141).
  5. **The same-cluster case is closed by `leave_bound`.** Any route that leaves the cluster first enters one of the 8 cells around it, and `octile(s,o)+octile(o,g)` never overestimates. So a local cost at or below the leave bound is the global optimum. Above it, the result is `min(local, abstract)`, and the abstract route is within the bound by steps 1–4.
- ✅ **The bound is not circular.** `border_crossings` (diff lines 464–473) counts crossings on the path of the test's own whole-map A* oracle, using the test constant `CLUSTER` and not the HPA graph. `max_cost` scans the passable cells of the NavGrid. Neither uses HPA data. Optimal paths that tie may cross a different number of borders, but the bound holds for each one, so whichever one the oracle returns is a valid witness.
- ✅ **Deterministic and charged (R84).** `leave_bound` is a pure function of the cluster rectangle and the start/goal cells, integer-only and saturating, and its `min` is order-independent. The decision to run the abstract search depends only on the grid. Its `dijkstra_from` calls and search use the same `stats` as every other search (diff lines 126–129). The choke_crowd `path.nodes_expanded` rise is the charged work showing up. Ties keep the local route (diff line 152). An abstract search with no target falls back to the local route (diff lines 160–162).
- ✅ **The regressions file is where proptest reads it.** `tests/hpa.rs` is an integration test. No directory above it (`crates/fl_sim/tests`, `crates/fl_sim`, `crates`, `C:\fl\a`) holds a `lib.rs` or `main.rs`, so proptest's `SourceParallel` falls back to `WithSource`, which reads `crates/fl_sim/tests/hpa.proptest-regressions`. Checks I ran:
  - git tracks the file and does not ignore it.
  - Its `cc` line matches `task-10b-hpa.proptest-regressions` byte for byte.
  - The implementer reproduced the failure deterministically from it.
- ✅ **No golden change.** The diff has no golden `.new`. Gate test-headless ran 438 tests and 437 passed; the only failure is `counters_choke_crowd`. In the snapshot `.new` only `path.budget_spent` and `path.nodes_expanded` move. `path.requests` and the move and publish totals are unchanged, so no outcome changed.
- ✅ **The choke_crowd ~2× is explained and has headroom.**
  - **Cause:** same-cluster requests near walls fail the leave bound, run the abstract search, and lose it. Totals are unchanged.
  - **Budget per tick:** the new max is 1358 against `budget_per_tick: 8000` (`assets/base/rules.ron:17`).
  - **budgets.ron.new:** each value is the observed max × 1.25, rounded up: 1086→1358, 1016→1270, 85→107, 22748→28435. Only the `choke_crowd` block changes.
- ⚠️ **Out of diff, pre-existing:** `ProptestConfig::with_cases(64)` (`tests/hpa.rs:849`) sets `cases` explicitly. That should override the nightly `PROPTEST_CASES=4096` (`xtask/src/gate.rs:373`), so the nightly run would not scale `hpa_cost_near_optimal`. The controller should confirm this with one nightly run log.
- ⚠️ **Allocations:** choke_crowd `alloc.bytes` rises 12233→28435 (+132%) and `alloc.count` 48→107. These are the per-request path buffers (R98), and R125 keeps R98 parked, so they are not R102 non-path allocations. The controller should note this when the bless commit goes in.

### Test-guard finding

- **Accepted:** `hpa.rs:772` RemovedAssert. R136 mandates replacing the old `1.3 × optimal + 40 × COST_ROAD` assert with the derived per-crossing bound. The old bound was shown false: 10 violations in 20,000 requests on the pre-change code.

### Strengths

- The root cause is shown with the abstract chain and g-values, and the other candidates in the brief were each ruled out with evidence.
- The same-cluster gap was found and closed as a separate real bug.
- The derivation appears in three consistent places: the module doc, the ADR and the doc on `CROSSING_STEPS`.
- The corridor test pins the geometry it relies on (`entrances(ClusterId(1)) == [(16,0),(16,15)]`), asserts that LOS is blocked, and states its numbers.
- `same_cluster_uses_local_astar` was re-scoped correctly. With a `y ≤ 5` wall the local route costs about 1046, below the leave bound of 1800, so the test still proves that no abstract search runs.
- No `.new` was blessed, and the reverted densification is kept as `task-7b-candidate.patch`.

### Issues

#### Critical (Must Fix)

None.

#### Important (Should Fix)

None.

#### Minor (Nice to Have)

1. **Over-long doc line.** `crates/fl_sim/src/nav/hpa.rs` doc line 44 (diff line 44) runs to about 190 characters, while the rest of the module doc wraps at 100. Rewrap it.
2. **Hardcoded reach in the tests.** `tests/hpa.rs` `CROSSING_STEPS` (diff line 447) and the corridor assertion `2 * 7 * cell` (diff line 288) hardcode reach 7. Derive it as `(u64::from(CLUSTER) - 1) / 2` so the test follows the constant it documents.
3. **Untested branches in `route`.** Two branches have no direct test:
   - the leave bound fails but the local route wins on a tie or by being cheaper (diff line 152);
   - the abstract search reaches no target, so the local route stands (diff lines 160–162).

   No test pins that the extra abstract work is charged in the losing case. The only evidence is the choke_crowd counters. Fix: keep the original `y ≤ 12` wall as a second test, and assert that the plan cost equals the local cost and that `work.nodes_expanded` is greater than the local A*'s `stats.expanded`.
4. **Possible perf improvement.** When a local route exists, the abstract search could stop once its smallest f reaches `local.cost`. Since ties keep the local route, the outcome is identical, and the stop is deterministic and charge-neutral. It would likely recover most of choke_crowd's ~2× rise. This is optional, alongside the report's tighter-leave-bound idea.
5. **The bound is loose as a regression tripwire.** It allows 16 × c_max per crossing, which is large on multi-crossing paths through forest. Consider a deterministic fixed-seed batch that asserts the empirical worst cell ratio (≤ 1.8) as an extra tripwire. The guaranteed bound stays the property test.
6. **`corner_path_cost` doc overclaims.** The doc on `corner_path_cost` (diff lines 449–450) says "what the corners a unit drives cost", but the function computes the cheapest cell path through the corner cells, which is a lower bound on the straight-leg drive. The ADR defines the term correctly (diff line 575). Align the helper doc with it.

### Assessment

**Task quality:** Approved

**Reasoning:** The derived bound is correct for the edge-cost model and is checked against an independent oracle. The same-cluster fix is deterministic and charged. The only protected-artifact change is choke_crowd's path work, which is explained and has ample budget headroom.

### Bless recommendation

- Bless `crates/fl_test_utils/tests/snapshots/perf_counters__choke_crowd.snap.new`. Only `path.budget_spent` (9194→16493) and `path.nodes_expanded` (7191→14490) change.
- Bless `docs/perf/budgets.ron.new`. Only the `choke_crowd` block changes: `alloc.bytes` 12233→28435, `alloc.count` 48→107, `path.budget_spent` 773→1358, `path.nodes_expanded` 634→1270.
- Reason: `M1 Task 7b (R136): HPA* same-cluster detour check adds an abstract search to choke-crowd requests`.
