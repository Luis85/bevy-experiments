# Task 6 re-review, fix round 1 (26c5b39, base 07724fa)

Read-only. I read the diff file, the report's "Fix round 1" section, and `los.rs` / `astar.rs` as committed. I ran nothing.

### Finding Verdicts

- **Important 1: `smooth` work grows with the square of path length.** ADDRESSED (`los.rs:147-213`).
  - The gallop probes `a+2, a+4, …` capped at `last`. A clear probe sets `lo`; the first blocked probe is `hi`. Bisection runs while `hi - lo > 1`.
  - `lo` only ever takes `a+1` or a probe that passed `line_clear`. When `lo` is still `a+1` after the search, the `a+1` segment is checked by a counted `clear` call with a `debug_assert!`.
  - Gallop cost is at most about 12L and bisection at most about 6L·log₂L for a corner advancing L points. That fits `6N·ceil(log₂N) + 12N`.
  - The RED run (20097 > 12000 straight, 14945 > 5400 diagonal) and the GREEN run show the pins discriminate on those two shapes.
- **Minor 1: sub-rect half of `astar_matches_dijkstra_cost` was vacuous.** ADDRESSED (`tests/astar.rs`, `inner` pair).
  - A second pair is drawn from the passable cells inside `sub` and checked against `sub` as bounds.
- **Minor 2: `(y, x)` tie-break not pinned.** ADDRESSED (`astar_breaks_equal_f_and_h_by_y_then_x`).
  - Both routes round the water cost 4·grass with equal h. Diagonals are blocked by the water side cell. Only the y-then-x order separates them.
- **Minor 3: diagonal side cell outside the bounds.** ADDRESSED, and the implementer's reasoning is correct.
  - A diagonal is generated only when both `c` and `(nx, ny)` are inside the area, and the area is a rectangle.
  - The side cells `(x+dx, y)` and `(x, y+dy)` therefore lie inside the box of the two cells, so inside the area.
  - The `neighbours` comment is corrected, the module doc says so, and `diagonal_at_the_bounds_edge` pins both the allowed and the blocked-side case on tight bounds.
- **Minor 4: no `line_clear` symmetry test.** ADDRESSED (`line_clear_is_symmetric`).
  - Offsets are often exactly 0 or the centre, so edges and corners are hit. The endpoint cells may be any cells; a blocked endpoint returns false in both directions.
- **Minor 5: uncounted O(bounds area) buffers.** ADDRESSED (module doc, "Keep the bounds tight").
- **Minor 6: `nearest_passable` probes not counted.** NOT ADDRESSED, by ruling. R95 carries it to Task 7, so it does not block this round.
- **Minor 7: unstated preconditions for `reach = anchor + 1`.** ADDRESSED (`# Preconditions` plus two `debug_assert!`s, and the fallback `clear` with its own assert).
  - Validity of the first segment: it runs from a point in `cells[0]` to the centre of an adjacent cell, so it touches only cells of the path's 2×2 block, which are passable by the corner rule. The last segment is symmetric.
  - Task 7's blocked-start snap now trips the assert in debug. See New 3.
- **Minor 8: no-route `Ok(())` in `smooth_keeps_path_valid`.** ADDRESSED. The no-route branch now asserts Dijkstra also reaches nothing.
- **Minor 9: gate time warning.** N/A, not a code finding.

### Focus checks on `smooth()`

- **Subsequence:** every emitted corner is `points[lo]` with `lo > anchor` (initially `anchor + 1`, only raised). The anchor strictly increases, so the loop terminates and the output is an ordered subsequence of `points[1..]`.
- **Endpoints:** `start` is `points[0]` and the anchor origin. The loop ends at `anchor == last`, so the last corner is `points[last]` = `goal`. When `cells.len() <= 2`, `inner` is empty (`get(1..0)` gives `None`, so `unwrap_or`).
- **Every segment clear:** `lo` becomes a probe only after `line_clear` returned true. The `a+1` fallback is checked in every build, and the result is asserted only in debug. It is clear whenever the preconditions hold.
- **Non-monotone visibility:** the algorithm never assumes monotonicity for validity.
  - The invariant is "`lo` clear (or `a+1`), `hi` blocked". Bisection ends with `hi = lo + 1`, so the corner is a clear point whose successor is blocked.
  - The result can differ from the first-failure greedy corner, and can stop short of a farther visible point beyond a blocked one. It is still a valid path.
  - The rustdoc states this.
- **Determinism:** only integer indices, a fixed probe order, and no dependence on the container or hash order.
- **Bound test measures real cells:**
  - `smooth_work` passes `checked` into `smooth`, which threads it through the `clear` closure into `line_clear`'s counter. The closure increments on every tested cell, including the corner side cells.
  - The validation loop uses a separate `&mut 0`, so it does not pollute `checked`.

### New Breakage in the Fix Diff

None Critical or Important.

1. **Minor, `tests/astar.rs` (`smooth_work_is_bounded_round_a_bend`):** the bend test does not discriminate. The report admits that greedy also passed it. Greedy costs about 0.5·119² + 0.5·80² ≈ 10k cells, against the bound of 12,000 for N = 200. It pins only validity and the corner count. A tighter explicit cap (the new algorithm needs roughly 1.5k) would make it a real regression pin for a bend.
2. **Minor, `tests/astar.rs`:** the bound is pinned only on three monotone-visibility shapes.
   - `smooth_keeps_path_valid` already has random maps with non-monotone visibility, where bisection is exercised. It does not assert `checked <= smooth_bound(path.len())`.
   - Adding that one assert covers the case the focus list raises, at no extra cost.
3. **Minor, `los.rs:155-162` with R95 / Task 7:** the new `debug_assert!`s turn the blocked-start snap, which minor 7 flagged, from a silent unchecked first segment into a debug panic.
   - That is the right direction.
   - Task 7 must decide whether to pass the snapped cell's centre as `start` or to handle the first segment itself, and record it in the R95 carry.
4. **Minor, records:** R94(b) and the R95 note about `invalidate` growing the rect by 1 cell assert that side cells may lie outside the bounds. That is false for A* and Dijkstra, as the fix shows. Amend the ledger so Task 7 does not add a needless 1-cell margin or rely on it. It is harmless if left.
5. **Nit, `los.rs:130-140`:** the rustdoc bound `6·N·log₂N + 12·N` should say `ceil(log₂N)`, as the test does. Step 3's wording "if no probe was clear" is really "if `lo` is still `a+1` after the search".

### Out-of-Scope Observations

None.

### Verdict

**Fix round:** All findings addressed, no new Critical/Important breakage. Minor 6 stays open under R95 (Task 7). New 1-4 are optional minors.
