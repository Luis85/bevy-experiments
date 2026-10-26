# Task 6 review: grid A*, LOS, smoothing, nearest passable (07724fa, base 7ce8824)

Single pass over `review-t6.diff` (all hunks complete; no changed file re-read). Focused out-of-diff checks are listed under "Checks run".

### Spec Compliance

- ✅ Spec compliant.
  - All brief signatures match: `crates/fl_sim/src/nav/astar.rs:43,57,179,266,320` and `src/nav/los.rs:19,79`.
  - The heap key is `Reverse((f, h, y, x))` (`astar.rs:201,224`), and a parent changes only on a strictly lower g (`astar.rs:220`).
  - The flat `Vec` g, parent and closed buffers are sized to the clipped bounds (`astar.rs:193-195`).
  - All 9 brief tests are present (proptests at 256 cases, 24×24 maps with water rectangles), plus 6 extra tests.
  - `nav/mod.rs` gets `pub mod astar; pub mod los;` and the re-exports.
- Extras (accepted):
  - `SearchStats` derives `PartialEq` and `Eq`.
  - `DistanceField` derives `Clone`, `PartialEq`, `Eq` and `Debug`.
  - 5 extra unit tests and 1 extra proptest.
- `lib.rs` is unchanged even though the brief listed it. This is fine: `lib.rs:62` has only `pub mod nav`, and Task 5 did not re-export nav items at the crate root either, so leaving it unchanged is consistent.
- ⚠️ The commit has no `Blessed:` trailer (Step 7). By the Task 5 precedent, the controller's bless commit carries it.
- ⚠️ Consumer gap for Task 10. `FlowCache::ensure_tile` needs a cluster field seeded from several entrances, each with its abstract cost. `dijkstra_from` handles one source at cost 0 only. Task 10 must add a seeded multi-source variant, with `dijkstra_from` as its one-seed case, and a "next cell toward the source" accessor (`parent` is private today). The reverse search is valid for this: `edge_cost` is symmetric (`(ca + cb)`, the same diagonal factor) and so is the corner rule.
- ⚠️ Consumer note for Task 7 Partial within one cluster. `astar` returns `None` without the reached set. Task 7 can run `dijkstra_from` over the cluster and scan `cluster_rect` with `cost(c).is_some()` for the reached cell nearest the goal. This works with the current API and is bounded by the cluster area.

### Strengths

- The determinism argument is correct and written down (`astar.rs:196-198`).
  - A cell is pushed again only with a strictly lower g, so with the same h its f is strictly lower. The `(y, x)` part separates different cells. So live heap keys are unique, and the `BinaryHeap` order of equal keys can never matter.
  - A stale entry is skipped through `closed`.
  - Only integers are used: costs are `u32` and products are `i128`. There is no `HashMap`, no `Entity` and no `Local`.
- The heuristic is admissible and consistent.
  - `octile = 100·max + 41·min` is the exact metric of the uniform 8-grid with weights 100/141, so the triangle inequality holds.
  - Every real step costs at least that: per-cell cost ≥ `COST_ROAD` because the speed factor is clamped to ≤ 1 (`grid.rs:83-94`), and a diagonal is `mean × 141 / 100 ≥ 141`.
  - The proptest `astar_matches_dijkstra_cost` confirms optimality and also checks `expanded(A*) ≤ expanded(Dijkstra)`.
- No corner cutting.
  - A diagonal step requires both side cells passable (`astar.rs:166`).
  - `NavGrid::passable` is false off the map (`grid.rs:332-334`), so out-of-map side cells are rejected with no panic.
  - `edge_cost` is only called on cells inside the area.
- `line_clear` is an exact supercover.
  - Boundary crossings are compared by cross-multiplying in `i128`, with no division.
  - At a corner it tests both side cells (`los.rs:58`), so it matches A*'s corner rule.
  - The walk counts down the remaining steps on each axis, so it always terminates exactly at the end cell.
  - It honours `MoveProfile` clearance through `passable`.
- `nearest_passable` has a closed ring bound (`astar.rs:329`), and the brute-force proptest pins the full `(ring, d², y, x)` order.
- The tests are strong.
  - Every A* or Dijkstra path is re-validated: contiguous, passable, inside the bounds, no corner cut, and its edge costs sum to the returned cost.
  - `expanded` is pinned at 4 for the open grid and at 40 for the lake ring.
  - Off-map, start == goal and blocked-endpoint cases are covered.

### Issues

#### Critical (Must Fix)

None.

#### Important (Should Fix)

1. **`los.rs:96-104`: `smooth` costs grow with the square of path length, against a per-tick budget.**
   - **The problem.** Greedy extend-by-one re-walks the whole segment from the anchor for every candidate point.
     - Cells checked are about 0.5·n² on an orthogonal run (a walk of k+1 cells per probe) and about 1.5·n² on a diagonal run. Each centre-to-centre diagonal passes exactly through corners, which costs 3 cells per step.
     - That is about 5,000 or 15,000 cells for n = 100, and 0.5 to 1.5 million for n = 1,000 (an HPA* refined path across a large map).
   - **Why it matters.** Task 8 charges `los_cells` against `Path.budget_per_tick` (8,000) and processes at least one request per tick.
     - One long request then takes up a whole tick.
     - With hundreds of movers, the queue drains at one request per tick. That breaks Task 8's throughput tests and Task 13's budgets.
     - Task 7's line-of-sight-first step only removes the fully straight case.
     - Fixing it now is cheap. Once Task 8's golden replay bakes corners in, any change to `smooth`'s output means re-blessing it.
   - **Fix: galloping then bisection, deterministic, same signature.** From anchor `a`:
     1. Set `lo = a + 1`. The adjacent point is valid by construction, so it is not probed.
     2. Probe offsets `a + 2, a + 4, a + 8, …`, capped at `last`, while `line_clear` holds. Each success raises `lo`.
     3. On the first failure `hi`, bisect: `mid = (lo + hi) / 2`; if clear, `lo = mid`, else `hi = mid`; stop when `hi == lo + 1`.
     4. Emit `points[lo]` and set `anchor = lo`.
   - **Properties.**
     - Every emitted segment except the adjacent fallback has been verified by `line_clear`.
     - Visibility along a path is not monotone, so the result can differ from greedy-first-failure. It is still valid and fully deterministic.
     - Pin the new rule in the rustdoc.
   - **Cost bound.** A probe at offset m costs at most 3m + 1 cells. For a corner that advances L points:
     - galloping costs ≤ 12L, and bisection costs ≤ 6L·log₂L;
     - so the total is ≤ 6N·log₂N + 12N + O(corners·log N);
     - when everything up to `last` is visible, a corner costs ≤ 6L, with no bisection.
     - Example: about 1,100 cells for a 100-point orthogonal path with one bend, against 5,000 or more today. At most about 72,000 for n = 1,000, against about 1.5 million.
   - **Test to add.** A pinned work bound, for example a 200-cell L-shaped corridor where `cells_checked ≤ 6N·log₂N + 12N`. Keep `smooth_keeps_path_valid`.

#### Minor (Nice to Have)

1. **`tests/astar.rs:434`: the sub-rect half of `astar_matches_dijkstra_cost` is nearly vacuous.**
   - `start` and `goal` are drawn from the whole map, so both lie inside the random sub-rect in only about 1–2% of cases. Otherwise both searches return `None` or reach nothing, and the comparison passes trivially.
   - Draw a second pair from the passable cells inside `sub`.
2. **No test pins the `(y, x)` tie-break.** `astar_open_grid_pinned` pins only f, then h.
   - Suggested pin: a 3×3 map with water at (1,1), path (0,1) → (2,1). The expected route is via y = 0: `[(0,1),(0,0),(1,0),(2,0),(2,1)]`.
3. **Semantic choice 2 is not pinned** (a diagonal's side cell may lie outside `bounds`, `astar.rs:144-146`).
   - Task 7's in-cluster searches depend on it.
   - Add a unit test where a diagonal at the edge of the bounds uses a side cell outside the bounds.
4. **No symmetry test for `line_clear`.**
   - I traced these cases by hand: an endpoint on a corner, a start on an edge heading negative (the t = 0 crossing), a start corner heading diagonally, and an endpoint on an edge. In each, the set of touched cells is equal in both directions.
   - Add a proptest `line_clear(a, b) == line_clear(b, a)` with raw offsets that include 0, so edge and corner points are covered.
5. **`astar.rs:193-195,289`: the buffers are O(bounds area) per call and that cost is not counted.**
   - Only settled cells count toward `expanded`.
   - This is fine for cluster bounds. A full-map call on a 4096² map allocates about 150 MB.
   - Document that callers keep the bounds tight.
6. **`astar.rs:320-351`: `nearest_passable` does O(radius²) `passable` probes that are not counted.**
   - Carry to Task 7: charge them to `nodes_expanded`, or keep `snap_radius_cells` small.
7. **`los.rs:96`: the fallback `reach = anchor + 1` is never checked.**
   - Its validity rests on two unstated preconditions: `nav.cell_of(start) == cells[0]` and `nav.cell_of(goal) == cells.last()`.
   - State them as `# Preconditions` in the rustdoc, or add a `debug_assert!`.
   - Task 7's blocked-start snap breaks the first precondition: the unit stands in water while `cells[0]` is the snapped cell. The first segment then crosses unchecked cells. Carry to Task 7 to decide and document.
8. **`tests/astar.rs:463`: `smooth_keeps_path_valid` returns `Ok(())` when `astar` is `None`.**
   - Those cases pass without testing anything, and nothing counts them.
   - This is acceptable, because most random pairs are connected.
9. **Gate `WARN gate task took 401s (budget 300s)`.** This is the existing pattern (Tasks 15 and 20 ran 420–427 s) and not caused by this task's code: the `astar` binary runs in about 1.3 s.

### Checks run (named risk → what I checked)

- **Heuristic admissibility** (a cost below 100 would break optimality): `grid.rs:83-94` clamps the factor to `min..=1` and computes `cost_of = ceil(100·2³²/raw)`, so cost is in `100..=65535`. Admissible.
- **`edge_cost` overflow:** the worst case is 2·65535·141 ≈ 1.85·10⁷, well inside `u32`.
- **Side cells off the map:** `grid.rs:332-342` returns false off the map, so there is no panic and no corner cut at the map edge.
- **Consistency with movement:** `movement.rs:194-196` uses `cell_of` (floor, clamped) and `passable`.
  - `line_clear` uses the same floor convention, without the clamp. The clamp matters only for points off the map, where LOS is conservatively false.
  - A segment lying on an edge is lenient, consistent with movement's floor convention.
- **Unneeded `lib.rs` re-export:** `lib.rs:62` has only `pub mod nav`.
- **`HashMap` and floats:** none in the diff.
- **test-guard, 3 × `prop_assume!(!cells.is_empty())`** at `tests/astar.rs:428,459,493`:
  - At most 8 lakes of up to 8×8 cover at most 512 of 576 cells, so size 1 always has passable cells.
  - Size 2 is empty only in a near-impossible scatter.
  - The guards hide no failure. **I accept all 3.**

### Recommended rulings

- **R93: fix the smoothing now.** In Task 6 fix round 1, `smooth` switches to the galloping-then-bisection rule above, with the pinned rustdoc and a work-bound test.
  - Bound: ≤ 6N·log₂N + 12N LOS cells, and ≤ 6L for a corner when everything up to the last point is visible.
  - Cost: about 20 lines now. The alternative is re-blessing the golden replay after Task 8.
- **R94: accept the implementer's semantics.**
  - (a) A start or goal outside the bounds or blocked returns `None` with 0 expanded. Task 7 snaps first.
  - (b) A diagonal's side cells must be passable but may lie outside the bounds. Task 7's `invalidate`, which grows the rect by 1 cell, covers that dependency.
  - (c) `expanded` counts the goal and adds up across calls.
  - (d) `SearchStats` derives `PartialEq` and `Eq`.
  - (e) A `dijkstra_from` source outside the bounds or blocked reaches nothing.
  - (f) `line_clear` maps endpoints with floor and no clamp, and a segment lying on an edge tests only the floor cells.
  - (g) `nearest_passable` scans each ring's perimeter, with the ring count capped.
- **R95: carries.**
  - Task 10 adds a seeded multi-source Dijkstra (`dijkstra_from` becomes its one-seed case) and a next-toward-source accessor on `DistanceField`. These are additive API.
  - Task 7 documents how `smooth` is called after a blocked-start snap, and charges the `nearest_passable` probes.

### Bless

Yes. `fl_sim.txt.new` adds 70 lines and removes or changes none, and it matches the brief's signatures. The R93 fix does not change any signature.

### Assessment

**Task quality:** Needs fixes

**Reasoning:** The search, tie-breaks, heuristic and LOS are correct, deterministic and well tested. Only `smooth`'s work grows quadratically against the per-tick path budget. It should be fixed before Task 8's golden replay captures corners.
