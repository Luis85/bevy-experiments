# Task 7 review: HPA* (clusters, entrances, search, refinement, cluster invalidation)

Diff: `review-t7.diff` (ecd2cc0 on ad7c4d1, plus API `.new`). I read it in one pass. I read code outside the diff only for the named-risk checks listed at the end.

### Spec Compliance

- ✅ **Spec compliant.**
  - Every interface in the brief exists with the briefed signature and derives:
    - `ClusterId`, `HpaGraphs`, `HpaGraph` accessors, `PlanStatus`, `PathPlan`, `PathWork::charge`, `plan_path`.
    - The `core.clusters` query, with args as for `core.passability`.
  - Derived `"core.nav_hpa"` is built inside `rebuild_nav` after `NavGrid`, with no rebuild of its own (R74): `nav/mod.rs:93-95`, `mod.rs:319`.
  - `on_terrain_changed` invalidates the graphs: `mod.rs:~140-152`.
  - The entrance run rule is implemented: `hpa.rs:139-171`.
  - Nodes are a `Vec` sorted by `(cluster, y, x)`, and the adjacency lists are sorted by target: `hpa.rs:346-394`.
  - `plan_path` follows steps 1–6 inside the `hpa_search` span: `hpa.rs:868-946`.
  - Each rebuilt cluster runs inside `hpa_cluster_build`: `hpa.rs:296-297`.
  - All 12 briefed tests are present, plus `hpa_cold_warm_and_loaded_agree`.
  - Deviations, each argued in the report and judged below:
    - 12 entrances instead of 16 (Ruling 1).
    - The extra Dijkstra on a partial plan (Ruling 2).
    - Step 4 falls through to the abstract search when the in-cluster A* fails: `hpa.rs:498-513`. This is needed, or `hpa_finds_path_iff_astar_does` cannot hold. Accept it.
    - `invalidate` also extends the rect by the passability anchor reach. The brief's "grown by 1" alone would be wrong for size ≥ 3. Accept it.
  - Extras: `HpaGraphs::cluster_cells()` and `HpaGraph::profile()`. Both are small, and the debug query and tests use them. Accept them.
  - `lib.rs` and `grid.rs` are listed as files to modify but are untouched. That is justified: `nav` is already `pub`, and the passability reach is handled in `invalidate`.
- ✅ **R95 carries.**
  - `nearest_passable` counts every probed perimeter cell, off-map cells included: `astar.rs:71`, documented at `astar.rs:47-49`.
  - `plan_path` charges the probes: `hpa.rs:836-838`.
  - Smooth after a blocked-start snap: the snapped centre is the first corner and the search origin (`hpa.rs:909-912`). This is documented in `plan_path` (`hpa.rs:851-853`) and in `smooth`'s `# Preconditions` (`los.rs`, the hunk at diff 1063-1065).
  - Exercised by `start_on_blocked_cell_recovers` (`tests/hpa.rs:303-344`).
- ✅ **Task 6 re-review minors are folded in.**
  - Tight-bend cap of 2,000: `tests/astar.rs:~262`.
  - The work bound in `smooth_keeps_path_valid`: `tests/astar.rs:~616-620`.
  - The `ceil(log₂N)` bound and the step-3 wording: `los.rs:~92,97`.
  - Pinned probe counts: `tests/astar.rs:390-412`.
- ✅ **Test-guard: the 5 `RemovedAssert` findings are rewrites, not removals.**
  - `tests/astar.rs` old lines 386, 387, 388, 391 and 394 are each still present, rewritten for the new `&mut probed` parameter (diff 1344-1372).
  - Three of them gained probe-count pins.
  - Accept all 5.
- ⚠️ **Cannot verify from the diff:**
  - The gate claims "only the bless is pending" (the `check-scope` and `api` FAILs). Controller: after the bless, re-run `gate task` and confirm it passes.

### Strengths

- **Determinism by construction.**
  - Only `BTreeMap`/`BTreeSet` are used: `hpa.rs:28, 432-433, 704`.
  - Entrances are sorted by `(y, x)` and deduped. There is one node per cell, so the `(cluster, y, x)` order is a total order: `hpa.rs:308-309`.
  - Adjacency is sorted by target, and target ids are unique per list: intra edges stay inside a cluster and inter edges cross one, so no two edges share a target.
  - The heap key `(f, h, node index)` is total, a parent changes only on a strictly lower g, and the partial key is fully specified: `hpa.rs:546-551`.
  - No floats, no HashMap, no iteration over pointer order.
- **R84 is shown, not just argued.**
  - `plan_path` is a pure function of `(nav, hpa, request)`, and `HpaGraphs: Eq` covers all its state.
  - `invalidate_equals_full_rebuild` checks the invalidated graphs against a cold build after every random edit: `tests/hpa.rs:753-773`.
  - `hpa_cold_warm_and_loaded_agree` (`tests/hpa.rs:499-540`) checks that warm, cold and post-load graphs are equal. It also checks they give identical `(PathPlan, PathWork)` over 3 requests × 2 profiles, at least two of which need the abstract search. So the charged work cannot depend on cache state.
- **Invalidation scope is exact and minimal.**
  - Touched clusters are the rect intersection only: `hpa.rs:420-430`.
  - All 4 borders of each touched cluster are recomputed: `hpa.rs:432-457`.
  - Touched clusters and their 4-neighbours get their entrances and intra edges rebuilt. Diagonal neighbours are correctly left alone, because no shared border or in-cluster cell changes for them.
  - The `+back` extension matches `NavGrid::passable`'s anchor (see the checks below).
- **Refinement is a valid cell path.**
  - Inter steps are orthogonal border pairs.
  - Intra, start and goal segments are in-cluster Dijkstra paths, and corner cutting is symmetric, so the reversed goal path is valid. `smooth`'s preconditions therefore hold.
  - Since `edge_cost` is symmetric, `PathPlan.cost` for a searched plan equals the refined path's real cost, so it is ≥ the optimal cost.
- **Good test design.**
  - `same_cluster_uses_local_astar` pins the exact `PathWork`, which proves no abstract search ran.
  - The partial tests use a full-map oracle.
  - The proptests were stress-run at 1.5k and 500 cases.

### Issues

#### Critical (Must Fix)
None.

#### Important (Should Fix)
None.

#### Minor (Nice to Have)

1. **A partial plan is not the global nearest reachable cell.** `hpa.rs:541-576`
   - Only the start cluster and the best node's cluster are scanned. The best node by octile need not lie in the cluster that holds the nearest reachable cell.
   - Task 8's `unreachable_goal_goes_to_closest_point` asserts "final cell == the nearest reachable cell". That is global wording, so it can fail depending on the map. `island_goal_is_partial` passes only on maps where the two coincide.
   - Fix (pick one): record the bound in the spec, and carry it to Task 8 so its test asserts against the same candidate set; or make the plan exact by pruning. To prune, scan the settled clusters in increasing order of the octile lower bound from cluster rect to goal, and stop when the bound reaches the best so far. That is deterministic and still charged.
2. **The proptests never exercise the `+back` invalidation branch (size ≥ 3).** `tests/hpa.rs:593` (`profile()`) and the test content only have sizes 1 and 2, so `back` is 0 (`hpa.rs:414`).
   - The branch is correct by inspection (see the checks), but nothing pins it.
   - Base content is size 1 today, so nothing ships with it.
   - Fix: add a size-3 unit to the ASCII test content, or a `MoveProfile { size: 3 }` built directly, to `invalidate_equals_full_rebuild`.
3. **No lower-bound check on cost.** `hpa_cost_near_optimal` (`tests/hpa.rs:744-748`) checks only the upper bound.
   - A searched plan's `cost ≥ optimal` (when `corners.len() > 1` or the plan is not a direct line) would catch refinement or cost-accounting regressions, such as an asymmetric `edge_cost` later.
   - Add one line.
4. **Stored intra paths grow with map size.** `hpa.rs:64-68`
   - At the R89 cap (4096², 65k clusters), about 56 paths × about 20 cells × 8 B per cluster comes to roughly 0.5 GB per profile.
   - At Task 13's `scale_2km` (500², 1024 clusters) it is about 9 MB per profile, which is fine.
   - Fold this into Ruling 3's carry.
5. **`hpa.rs` is 946 lines, mixing graph build and invalidation with request planning.**
   - Split `plan_path`, `endpoint`, `PathPlan` and `PathWork` into `nav/plan.rs` when Task 8 touches it. Not now.
6. **Gate WARN: the task gate took 400 s against a 300 s budget** (report). This is process noise, not code. Controller awareness only.
7. **`wide_profile_avoids_narrow_gap` compares two different kinds of cost.** It asserts `wide.cost > tank.cost` (`tests/hpa.rs:453`), but the tank's plan is a direct line, so its cost is the octile estimate.
   - The assertion holds, because the estimate is ≤ the real cost, but it compares unlike quantities.
   - Compare against `octile` explicitly, or drop the assertion.

### Recommended rulings

1. **12 entrances (one node per shared corner cell): accept.**
   - Two nodes on one cell would need a zero-cost self-edge and a tie-break beyond `(cluster, y, x)`, which only adds search work.
   - The brief's 16 counted transition ends, not nodes.
   - Amend the plan text of `clusters_query_lists_entrances` to 12.
   - Cost: none.
2. **The extra in-cluster Dijkstra for partial plans: accept.**
   - It is charged as work and deterministic, and it serves the "closest reachable point" intent.
   - Record in the spec that a partial plan ends at the nearest cell among the start cluster and the best settled node's cluster.
   - Carry Minor 1 to Task 8: either match its test wording to that, or adopt the pruned exact scan.
   - Cost: in rare maps, a partial plan stops short of the global nearest point.
3. **Per-request all-node buffers and a full relink per terrain change: accept now, and do not make them a fix before Task 13.**
   - Task 13's map is `scale_2km`: 500² cells, 1024 clusters, roughly 8k nodes per profile. That is about 72 KB of buffers per request and a relink of O(10k) per change, which is negligible.
   - Park until a map > 1024 cells per side, or until Task 13's profile shows `hpa_search` or `invalidate` among the top systems. The fix would then be generation-stamped search buffers in a `TransientReset` scratch resource, plus a two-level `(cluster, local)` node index so relinking stays local. Include Minor 4's path storage in the same parking.
   - Determinism is unaffected either way.
   - Note for Task 8: a partial request for an unreachable goal expands the whole reachable abstract component. Its charge can exceed `budget_per_tick`, so "process at least one per tick" must stay.
4. **Direct-line `PathPlan.cost` = the octile estimate: accept.**
   - It is documented as a lower bound (`hpa.rs:767-769`), and Task 8 only journals it.
   - Carry to Task 8/10: never compare `PathPlan.cost` across plans as a true cost without checking whether the plan was a direct line.
   - Cost: none.
5. **The new counter `path.clusters_invalidated`: accept.**
   - It counts the union over profiles of touched clusters, and it is deterministic. Counters are not hashed (R86).
   - Carry to Task 13: do not budget it unless the scale mix changes terrain (`check_budgets` fails a budgeted counter a run doesn't measure).
   - Carry to Task 10: `invalidate` returns the union, not ids per profile. Any non-empty rect touches every profile's graph, so "every profile whose graph rebuilt a cluster" means all profiles.
   - Cost: none.

### Bless

**Bless the API `.new`: yes.** The delta matches the code exactly:
- `nearest_passable` gains `&mut u32`.
- `pub mod nav::hpa` with `ClusterId`, `HpaGraph`, `HpaGraphs` (Resource, no Serialize), `PathPlan`, `PathWork`, `PlanStatus` and `plan_path`, plus the `nav::` re-exports.
- Nothing else changed.

Reason: `M1 Task 7: HPA*`.

### Assessment

**Task quality:** Approved

**Reasoning:** Graph construction and search are deterministic by construction. Invalidation is exact, proven equal to a cold build, and R84 cache-independence of the plan and the charged work is tested across warm, cold and loaded graphs. The open items are Minor and carry-overs.

### Named-risk checks (outside the diff)

- **Asymmetric `edge_cost` would make the reverse inter edges, the reversed goal path and `PathPlan.cost` wrong.** Checked `astar.rs:61-70`: the cost is the mean of the two cells' costs, so it is symmetric. No issue.
- **The octile heuristic might be inconsistent on the abstract graph.** Checked `astar.rs:72-79`: it is at road cost, and no step is cheaper, so every abstract edge cost is ≥ the octile distance between its ends. It is consistent, and the closed-set skip is safe.
- **The invalidation margin might miss passability changes.**
  - Checked `grid.rs:332-343`: `passable` reads clearance at the anchor `(x − back, y − back)`.
  - Checked `grid.rs:371-410`: `update_rect` returns `clear`, which is grown by 1 for slope and cost and by `max_clearance − 1` toward −x/−y.
  - So the changed passability and cost set is `[clear.min, clear.max + back]`, which `hpa.rs:416-423` covers exactly, plus the briefed margin of 1.
- **Consumer fit (plan Tasks 8, 10, 13).**
  - Task 8 processes at least one request per tick, which tolerates a single large charge.
  - Task 8's `unreachable_goal_goes_to_closest_point` uses global wording (Minor 1).
  - Task 10 needs the abstract nodes and adjacency. They are private, but they are in the same crate (`pub(crate)` is enough).
  - Task 13's map is `scale_2km` (`assets/base/maps/scale_2km.map.ron`, 4 m cells, so 500²). That sized Ruling 3.
- **Base content profile sizes.** All base hull widths are ≤ 3.6 m on 4 m cells, so every profile is size 1. That is why Minor 2 is Minor.
