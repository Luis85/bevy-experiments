# Task 7 report: HPA* (clusters, entrances, search, refinement, cluster invalidation)

Base `ad7c4d1` (`.gate-base`), lane 1, branch `m1/phase-a`. Track sim-core.

## What I implemented

**`crates/fl_sim/src/nav/hpa.rs` (new):**
- `ClusterId(u32)` (`cy × clusters_x + cx`).
- `HpaGraph`, one per profile:
  - Per-cluster data: the east and north border transitions, the entrances (sorted by `(y, x)`), and the intra edges with their cell paths.
  - The flat abstract graph assembled from that data: nodes in a `Vec` sorted by `(cluster, y, x)`, adjacency lists sorted by target node.
  - Accessors: `cluster_of` (clamped), `cluster_rect`, `entrances`, `node_count`, `clusters`, plus `profile`.
- Entrance rule as briefed:
  - Maximal runs of pairs passable on both sides.
  - A run shorter than 6 gets one transition at `start + (len − 1) / 2`; a longer run gets two, at both ends.
  - The inter edge is costed with `edge_cost`.
  - Intra edges come from one `dijkstra_from` per node over its cluster, and store the paths.
  - Each `build_cluster` runs inside the `hpa_cluster_build` span (fields `cx`, `cy`, `size`).
- `HpaGraphs` (`Resource`, Derived `"core.nav_hpa"`): `build`, `graph`, `cluster_cells`, `invalidate`.
- `invalidate` per graph:
  1. Extend `rect` toward +x/+y by the `(size − 1) / 2` cells that `passable` reads toward −x/−y, then grow it by 1.
  2. Recompute all four borders of every intersecting ("touched") cluster.
  3. Rebuild entrances and intra edges for the touched clusters and their 4-neighbours.
  4. Relink.
  5. Return the touched ids, as the sorted union over profiles. A grid-size change rebuilds the whole graph.
- `PlanStatus`, `PathPlan`, `PathWork { nodes_expanded, los_cells }` with `charge()`.
- `plan_path` follows the brief's steps 1–6 inside the `hpa_search` span, with a `trace!` record per request:
  - Snapping: an endpoint off the map or in a blocked cell snaps through `nearest_passable` from its clamped cell, and its probes are added to `nodes_expanded` (R95). A goal or start with no passable cell within the radius gives `Unreachable`.
  - Step 3: a direct `line_clear` gives `[goal]` and `Found`. Its `cost` is the octile distance of the end cells, which never exceeds the optimum.
  - Step 4: in the same cluster, A* inside the cluster, then `smooth`. If that finds nothing, it falls through to step 5 (a route may leave the cluster).
  - Step 5: start and goal are inserted by in-cluster Dijkstra. The abstract A* pops `(f, h, node index)` with the octile heuristic and changes a parent only on a strictly lower g. The route is refined by concatenating the stored paths, then smoothed.
  - Step 6 (Partial), see Concern 2: the target is the settled node or start-cluster cell with the smallest octile distance (key `(octile, nodes-before-cells, node index, y, x)`). From a node, one more in-cluster Dijkstra continues to the cell of that cluster nearest the goal.
  - Every Dijkstra, A* and abstract node settled counts in `nodes_expanded`, and every LOS cell in `los_cells`.
- **R95, how `smooth`'s preconditions hold after a blocked-start snap.**
  - When the start snaps, the snapped cell's centre becomes the first corner. The direct-line test, the search and `smooth` then all start from that centre, which lies in `cells[0]`.
  - The goal point is either `to` (in a passable on-map cell, which is the last cell) or the snapped cell's centre.
  - This is documented in `plan_path`'s rustdoc and in `smooth`'s `# Preconditions`. The debug build exercises it in `start_on_blocked_cell_recovers` (blocked start, off-map start).

**`nav/mod.rs`:**
- `rebuild_nav` builds `HpaGraphs` after `NavGrid` (`Path.cluster_cells`) and removes both without a map.
- `on_terrain_changed` invalidates with the rect `update_rect` returned and adds the rebuilt count to the counter `path.clusters_invalidated`.
- `HpaGraphs` is registered as Derived `"core.nav_hpa"`. There is no rebuild of its own (R74), because core's one rebuild covers it.
- Debug query `core.clusters` (args as `core.passability`) → `{"cluster_cells","clusters":[cx,cy],"entrances":[[x,y],..],"nodes":n}`. Bad args, and a profile without a graph, return `Err`, never a panic.
- Re-exports and module docs.

**`nav/astar.rs`:** `nearest_passable(.., probed: &mut u32)` counts every perimeter cell it probes (R95), and its rustdoc says so.

**`nav/los.rs`:** the doc nits from the Task 6 re-review: the bound reads `6·N·ceil(log₂N) + 12·N`, and step 3 reads "if `lo` is still `a + 1` after the search". The `# Preconditions` section gains the snapped-start note.

**`tests/astar.rs`** (Task 6 re-review minors):
- `smooth_work_is_bounded_round_a_bend` now has a 2,000-cell cap, which greedy smoothing (about 10k) would fail.
- `smooth_keeps_path_valid` asserts `checked <= smooth_bound(path.len())`.
- `nearest_passable_ring_order` pins the probe counts: 1 + 8 + 16, then accumulation, then ring 0 only.

**`lib.rs`: not changed.** Its crate docs point to `nav`, whose module docs now cover HPA*, and `nav` is already `pub`, so there was nothing to export. `grid.rs` is not changed either: the passability reach is handled in `HpaGraphs::invalidate`.

## Tests (`crates/fl_sim/tests/hpa.rs`)

- `entrances_follow_run_rule`: the briefed 5 → `(15,5)`/`(16,5)` and 8 → ends. It also pins the run-length edges, 6 → two transitions and 1 → one, plus `clusters`, `cluster_of` and `cluster_rect`.
- `direct_line_costs_only_los`: `corners == [to]`, `nodes_expanded == 0`, `los_cells > 0`, `charge == los_cells`.
- `same_cluster_uses_local_astar`: `PathWork` equals exactly the in-cluster `astar` expansions plus the direct-line and `smooth` LOS cells. So no abstract search ran, and the corners and cost match.
- `goal_in_water_snaps_to_nearest_shore`: the last corner is `cell_centre(nearest_passable(goal))`, the probes are charged, and no shore within 8 cells gives `Unreachable`.
- `island_goal_is_partial`: two cases, checked against a full-map oracle (the reachable cell with the smallest `(octile, y, x)`):
  - Island in another cluster: exercises the node path and the in-cluster continuation.
  - Island in the start cluster: exercises the start-cluster cell candidates.
- `start_on_blocked_cell_recovers`: the first corner is the snapped centre, the chain is clear, an off-map start snaps into the map, and a start with no passable cell in the radius gives `Unreachable`.
- `bridge_removal_reroutes`: through the sim, using `debug.set_terrain` and the hook:
  - Before, the plan goes straight over bridge A.
  - After, it goes via bridge B.
  - `path.clusters_invalidated == 2`.
  - The live graphs equal a fresh build.
- `wide_profile_avoids_narrow_gap`: `t.tank` goes straight through the 1-cell gap. `t.wide_tank` (size 2) detours by the 4-cell gap, its chain is clear for size 2, and its cost is higher.
- `clusters_query_lists_entrances`: the exact entrance list. `{"unit":1}` answers the same as `{"class":"Tracked","size":1}`. Bad args return `Err`. **12 entrances, not the brief's 16; see Concern 1.**
- `hpa_cold_warm_and_loaded_agree`: three `set_terrain` invalidations through the sim, then a save and reload.
  - Warm (invalidated in place) == cold `HpaGraphs::build` == post-load graphs.
  - `plan_path` gives an identical `(PathPlan, PathWork)` on all three, for 3 requests × 2 profiles.
- Proptests (64 cases, 64×64 maps, random water rects, cluster 16):
  - `hpa_finds_path_iff_astar_does`: Found iff a full-map A* finds a route, and the corners chain `line_clear`. Otherwise the plan is Partial, ending at the centre of a cell reachable from the start.
  - `hpa_cost_near_optimal`: `10·cost ≤ 13·optimal + 40·COST_ROAD`.
  - `invalidate_equals_full_rebuild`: random `set_rect` edits (mostly small, some large) go through `NavGrid::update_rect` then `HpaGraphs::invalidate`. The graphs must equal a cold build after every edit, the ids must be sorted and unique, and they must include the edited corners' clusters.
- Stress runs, done locally and then reverted to 64 cases:
  - `hpa_finds_path_iff_astar_does` and `hpa_cost_near_optimal` passed at 1,500 cases each.
  - `invalidate_equals_full_rebuild` passed at 500 cases (103 s with `cargo test`).

### TDD evidence

- **RED:** `cargo nextest run --profile agent -p fl_sim --test hpa`
  ```
  error[E0432]: unresolved imports `fl_sim::nav::ClusterId`, `fl_sim::nav::HpaGraphs`, `fl_sim::nav::PathPlan`, `fl_sim::nav::PathWork`, `fl_sim::nav::PlanStatus`, `fl_sim::nav::plan_path`
  error[E0061]: this function takes 4 arguments but 5 arguments were supplied   (×2, nearest_passable's probe counter)
  error: could not compile `fl_sim` (test "hpa") due to 3 previous errors
  ```
  This failure was expected: the HPA* API and the probe counter did not exist yet.
- **GREEN:** `cargo nextest run --profile agent -p fl_sim --test hpa --test astar` gave `34 tests run: 34 passed`.
- **Full crate:** `cargo nextest run --profile agent -p fl_sim` gave `296 tests run: 296 passed, 0 skipped`.
- **Clippy:** `cargo clippy -p fl_sim --all-targets -- -D warnings` is clean. The only output is the existing clippy.toml notes about `rand::rng`/`rand::random`.

### Gate

I ran `cargo xtask gate task --track sim-core` before committing, on the same content:
```
PASS fmt (5.5s)
PASS clippy-drift (2.9s)
PASS claude-md (0.0s)
FAIL check-scope (0.6s)
    1 of 7 changed files are outside track(s) sim-core:
      xtask/snapshots/api/fl_sim.txt.new → contract
PASS protected (0.6s)
WARN test-guard (0.9s): test-guard: 5 findings (reviewer must accept each)
    TEST-GUARD crates/fl_sim/tests/astar.rs:386 RemovedAssert: assert_eq!(nearest_passable(&nav, TANK, (2, 2), 2), Some((2, 0)));
    TEST-GUARD crates/fl_sim/tests/astar.rs:387 RemovedAssert: assert_eq!(nearest_passable(&nav, TANK, (2, 2), 1), None);
    TEST-GUARD crates/fl_sim/tests/astar.rs:388 RemovedAssert: assert_eq!(nearest_passable(&nav, TANK, (4, 2), 3), Some((4, 2)));
    TEST-GUARD crates/fl_sim/tests/astar.rs:391 RemovedAssert: assert_eq!(nearest_passable(&sea, TANK, (0, 0), u32::MAX), None);
    TEST-GUARD crates/fl_sim/tests/astar.rs:394 RemovedAssert: assert_eq!(nearest_passable(&ring, TANK, (1, 1), 1), Some((2, 1)));
PASS clippy (43.8s)
PASS arch-check (5.5s)
PASS test-headless (193.8s)
SKIP test-client: no client package touched since ad7c4d1 (.gate-base)
FAIL api (40.1s)
    api: 1 of 5 crates changed or new
      fl_sim: changed
    run `cargo xtask api` to write the .txt.new files for the lead to bless
PASS data-check (105.1s)
WARN gate task took 400s (budget 300s)
gate task: FAIL (check-scope, api) in 400s
```
- **The two FAILs are the pending API bless and nothing else.**
  - `api`: `fl_sim.txt.new` is waiting for the lead.
  - `check-scope`: its only out-of-track file is that `.new` file, which is uncommitted.
- **The 5 test-guard `RemovedAssert` findings are not removals.** All five asserts are still there, rewritten for the new `&mut probed` parameter, and they now also pin the probe counts.
- **The gate ran over its time budget** (400 s against 300 s). Most of it is test-headless; data-check took 105 s.

## Protected artifacts

- `xtask/snapshots/api/fl_sim.txt.new` is written by `cargo xtask api` (`api: 1 of 5 crates changed or new / fl_sim: changed`). It awaits the lead's bless with the reason `M1 Task 7: HPA*`. The delta:
  - `nearest_passable` gains `&mut u32`.
  - New `pub mod fl_sim::nav::hpa`: `ClusterId`, `HpaGraph` (`cluster_of`, `cluster_rect`, `clusters`, `entrances`, `node_count`, `profile`), `HpaGraphs` (`build`, `cluster_cells`, `graph`, `invalidate`, `Resource`), `PathPlan`, `PathWork` (`charge`), `PlanStatus`, `plan_path`, with re-exports in `fl_sim::nav`.
- No other protected file changed. There are no new systems, no codecs, and no save or hash-stream change, because `HpaGraphs` is Derived with no codec.

## Commit

`ecd2cc0 feat(fl_sim): HPA* per movement profile — entrances, abstract search, refinement, cluster invalidation`. It has a `Refs: ADR-0015` trailer and no `Blessed:` trailer, because the lead blesses the API afterwards, as in Task 6.

## Files changed

- `crates/fl_sim/src/nav/hpa.rs` (new)
- `crates/fl_sim/tests/hpa.rs` (new)
- `crates/fl_sim/src/nav/mod.rs`
- `crates/fl_sim/src/nav/astar.rs`
- `crates/fl_sim/src/nav/los.rs`
- `crates/fl_sim/tests/astar.rs`
- `xtask/snapshots/api/fl_sim.txt.new`, for bless; not committed.

## Concerns

1. **The brief's "16 entrances" conflicts with one node per cell.** On a 32×32 open map, each of the 4 borders has one 16-cell run and so two transitions at its ends, which makes 16 transition ends.
   - Four of those ends are shared cluster-corner cells. For example, `(15,15)` ends both cluster 0's east and north transitions.
   - I keep one node per cell, so nodes are unique under the briefed `(cluster, y, x)` order and there are no zero-cost duplicate edges. That gives 12 entrances and nodes.
   - The test asserts 12, and the full list.
   - Getting 16 would need two nodes on one cell, joined by a zero-cost edge, plus a tie-break beyond `(cluster, y, x)`.
   - Ruling requested.
2. **Partial goes one step past the brief's wording.** Step 6 says to plan "to the reached abstract node or cell".
   - Literally, a goal on an island in another cluster would end at a cluster entrance, possibly far from the shore.
   - When the best candidate is a node, I run one more in-cluster Dijkstra from it, charged as work, to the cell of that cluster nearest the goal.
   - That makes `island_goal_is_partial`'s "reached cell nearest the goal" hold, and Review Focus 1 asks to "drive to the closest reachable point".
   - It is still not a global optimum: only the start cluster and the best node's cluster are scanned cell by cell.
3. **Search allocations at map scale.**
   - The abstract search allocates g, parent and closed buffers for all nodes per request. That is about 9 bytes per node; at 4096² cells, roughly 600k nodes per profile.
   - `invalidate` relinks the whole flat graph, O(nodes + edges), per terrain change.
   - The full build runs about 8 Dijkstras of up to 256 cells per cluster per profile.
   - All of this is fine at test and testbed sizes. Task 13's scale bench should look at it, and a `TransientReset` scratch resource could hold the buffers later.
4. **`PathPlan.cost` for a direct line is the octile estimate.** No cell path exists when there is no search. It is documented; Task 8 only journals the cost.
5. **The `path.clusters_invalidated` counter is added here.** The plan lists it without naming an owning task; it counts the touched cluster ids (the union over profiles) per terrain change.
