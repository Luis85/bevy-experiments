# Task 6 report: grid A*, line of sight, smoothing, nearest passable cell

**Status:** DONE_WITH_CONCERNS (only the expected bless-pending gate failures, plus the concerns below)
**Commit:** `07724fa` feat(fl_sim): grid A* with deterministic ties, supercover line of sight, string-pulling, nearest passable cell (trailer `Refs: ADR-0015`; no `Blessed:` trailer because the lead blesses afterwards, as in Task 5)
**Base:** 7ce8824 (.gate-base)

## What I implemented

- `crates/fl_sim/src/nav/astar.rs` (new):
  - `CellXY`, `SearchStats { expanded }`.
  - `edge_cost`: `(ca + cb).div_ceil(2)` (that is `(ca + cb + 1) / 2`), and for a diagonal that value `× 141 / 100`.
  - `octile`: `COST_ROAD × (dx + dy) − 59 × min(dx, dy)`, computed in u64 and saturated to u32.
  - `astar`, `DistanceField { cost, path_to }`, `dijkstra_from`, `nearest_passable`, `cell_centre`.
- `crates/fl_sim/src/nav/los.rs` (new): `line_clear` and `smooth`.
- `crates/fl_sim/src/nav/mod.rs`: `pub mod astar; pub mod los;`, the re-exports of every new item at `fl_sim::nav::*`, and one module-doc sentence.
- `crates/fl_sim/src/lib.rs` is unchanged. `pub mod nav` already exposes everything, so it needed no re-export.
- `crates/fl_sim/tests/astar.rs` (new): 11 unit tests and 4 proptests.

### How the code meets the determinism rules

- **Integer costs only:**
  - Costs are `u32`, and a sum saturates instead of overflowing.
  - `line_clear` compares boundary crossings by cross-multiplying raw `Fx` values as `i128`. No float and no `Fx` division is involved.
- **Tie-breaks:**
  - A* heap: `Reverse((f, h, y, x))`. Dijkstra heap: `Reverse((g, y, x))`.
  - No two live heap entries share a key. A cell is pushed again only on a strictly lower g, which gives it a strictly lower f (same h). `(y, x)` separates different cells.
  - So the `BinaryHeap` order of equal keys never matters.
  - A parent changes only on a strictly lower g. A popped cell that is already closed is a stale entry and is skipped.
  - The heuristic is consistent: every step costs at least 100 straight or 141 diagonal. So the closed set never needs to reopen a cell, and the proptest confirms A* cost equals Dijkstra cost.
- **Buffers:** flat `Vec`s (g, parent as u32 index, closed) sized to the bounds clipped to the grid. No hash maps and no `Entity`. They are plain locals inside the call, never a `Local<T>`.
- **Work counting:** `SearchStats.expanded` grows by 1 per settled cell, the goal included, and it accumulates across calls. `line_clear` and `smooth` add every cell they test to `cells_checked`, including the two side cells of a corner pass.

### Semantics I chose where the brief was silent

These are documented in the rustdoc; please confirm.

1. **`astar` endpoints.** If `start` or `goal` is outside the clipped bounds or not passable, `astar` returns `None` and expands nothing. With no route, it expands the whole reachable region. `start == goal` returns `([start], 0)` with 1 expanded. This matches Task 7's `plan_path`, which snaps a blocked start or goal before it searches.
2. **Diagonal corner cells and bounds.** Both orthogonal cells of a diagonal must be passable, but they may lie outside `bounds`. Only the cells a path walks through must be inside. This matters for Task 7's in-cluster searches at cluster edges.
3. **`dijkstra_from` with a bad source.** A source outside the bounds, or one that is not passable, reaches nothing: `cost(source) == None`.
4. **`line_clear` boundaries.**
   - Endpoints map to cells with `div_euclid` and no clamp, so a point off the map lies in a blocked cell.
   - Where the segment passes exactly through a cell corner, it tests both side cells.
   - A segment lying exactly on a cell edge tests only the cells that contain its points (floor), not the cells on the other side of the edge.
   - The walk counts the remaining steps on each axis, so it always ends exactly at the end cell. It cannot overshoot when the end point lies on a boundary.
5. **`smooth` output.**
   - The points are `start`, the centres of the inner cells and `goal`.
   - From each anchor, it extends one point at a time while `line_clear` holds (greedy farthest).
   - It returns the corners after `start`, ending exactly at `goal`.
   - The fallback step to the next point is not tested. It is valid by construction: astar paths are contiguous and never cut a corner, and `start` and `goal` lie in the first and last cells. The proptest checks this.
6. **`nearest_passable` scan size.** It scans only each ring's perimeter. The ring count is capped at the distance past which every cell is off the map, so `radius = u32::MAX` on a closed map ends.
7. **`SearchStats` derives.** It also derives `PartialEq` and `Eq`, which the brief didn't list. `astar_is_deterministic` compares stats with them.

## Tests

### TDD evidence

- **RED:** `cargo nextest run --profile agent -p fl_sim --test astar` failed before the implementation, as expected because the API did not exist yet:
  ```
  error[E0432]: unresolved imports `fl_sim::nav::CellXY`, `fl_sim::nav::SearchStats`, `fl_sim::nav::astar`, `fl_sim::nav::cell_centre`, `fl_sim::nav::dijkstra_from`, `fl_sim::nav::edge_cost`, `fl_sim::nav::line_clear`, `fl_sim::nav::nearest_passable`, `fl_sim::nav::octile`, `fl_sim::nav::smooth`
  error: could not compile `fl_sim` (test "astar") due to 1 previous error
  ```
- **GREEN:** the same command after implementation:
  ```
  Starting 15 tests across 1 binary
  Summary [   1.341s] 15 tests run: 15 passed, 0 skipped
  ```
- **Full suite:** `cargo nextest run --profile agent -p fl_sim`:
  ```
  Starting 277 tests across 19 binaries
  Summary [  23.973s] 277 tests run: 277 passed, 0 skipped
  ```
- `cargo clippy -p fl_sim --all-targets -- -D warnings`: clean. The only warnings are the existing `clippy.toml` `rand::rng`/`rand::random` path warnings.
- `cargo fmt --check`: clean.
- `cargo doc -p fl_sim --no-deps`: no warnings from the new files. The remaining warnings already existed.

### Tests in `crates/fl_sim/tests/astar.rs`

**Brief tests:**
- `astar_open_grid_pinned`: path `[(0,0),(1,1),(2,1),(3,1)]`, cost 341, and expanded pinned at 4.
- `no_corner_cutting`:
  - Water at (1,0) and (0,1) leaves `None` with 1 expanded.
  - Water at (1,0) only routes (0,0) → (0,1) → (1,1), never the direct diagonal.
- `astar_respects_bounds`:
  - A goal or start outside the bounds gives `None` with 0 expanded.
  - Bounds that cut off a wall's gap give `None`.
  - Bounds larger than the map are clipped to it.
- `unreachable_goal_expands_reachable_region`: a lake ring on a 7×7 map; `expanded == 40` for both A* and Dijkstra.
- `line_clear_detects_water_and_corners`:
  - A corner pass next to water is false; a segment through water is false.
  - A segment just above the corner is true.
  - `cells_checked == 3` is pinned for a 3-cell column.
  - A segment off the map is false.
- `nearest_passable_ring_order`:
  - Shores at (+2,0) and (0,−2) return (0,−2), the smaller y.
  - Radius 1 returns `None`.
  - Radius `u32::MAX` on an all-water map ends.
  - A side cell (dx² + dy² = 1) beats a ring corner (2).

**Extra unit tests:** `edge_cost_and_octile_are_pinned` (grass for Tracked costs 112, so 106 straight and 149 diagonal), `astar_start_is_goal`, `astar_rejects_blocked_endpoints`, `dijkstra_respects_bounds`, `smooth_pulls_the_string`.

**Proptests** (256 cases; 24×24 maps of grass, road and forest with 0–8 water rectangles; Tracked profiles of size 1 and 2):
- `astar_matches_dijkstra_cost`, over the whole map and a random sub-rect:
  - A* cost equals Dijkstra cost.
  - Both paths are contiguous, passable, inside the bounds and never cut a corner, and their edge costs sum to the returned cost.
  - A* expands no more cells than Dijkstra.
- `smooth_keeps_path_valid`: random start and goal points inside their cells. Every consecutive pair, start → first corner included, passes `line_clear`, and the last corner equals the goal.
- `astar_is_deterministic`: two independently built grids give equal astar results and stats, equal Dijkstra fields and stats, and equal smooth output and cells checked.
- `nearest_passable_is_nearest` (extra): the result equals a brute-force minimum of `(ring, d², y, x)`.

## Gate

`cargo xtask gate task --track sim-core` (run before the commit; the code was unchanged afterwards apart from a one-word doc link fix, `astar` → `astar()`):

```
PASS fmt (2.2s)
PASS clippy-drift (4.9s)
PASS claude-md (0.0s)
FAIL check-scope (0.5s)
    1 of 5 changed files are outside track(s) sim-core:
      xtask/snapshots/api/fl_sim.txt.new → contract
PASS protected (0.5s)
WARN test-guard (0.5s): test-guard: 3 findings (reviewer must accept each)
    TEST-GUARD crates/fl_sim/tests/astar.rs:428 NewAssume: prop_assume!(!cells.is_empty());
    TEST-GUARD crates/fl_sim/tests/astar.rs:459 NewAssume: prop_assume!(!cells.is_empty());
    TEST-GUARD crates/fl_sim/tests/astar.rs:493 NewAssume: prop_assume!(!cells.is_empty());
PASS clippy (41.5s)
PASS arch-check (6.9s)
PASS test-headless (184.1s)
SKIP test-client: no client package touched since 7ce8824 (.gate-base)
FAIL api (104.1s)
    api: 1 of 5 crates changed or new
      fl_sim: changed
    run `cargo xtask api` to write the .txt.new files for the lead to bless
PASS data-check (55.4s)
WARN gate task took 401s (budget 300s)
gate task: FAIL (check-scope, api) in 401s
```

- `api` and `check-scope` both fail only because of the bless-pending `xtask/snapshots/api/fl_sim.txt.new`, which is expected (brief Step 4: "API `.new` pending").
- **test-guard:** the three `prop_assume!(!cells.is_empty())` skip the rare map where water leaves no passable cell, so no start or goal can be picked. They hide no failure; the reviewer needs to accept each one.

## Protected artifacts (.new, for the lead to bless)

`xtask/snapshots/api/fl_sim.txt.new` is additive only, with no line removed or changed (+70 lines). It adds:
- `pub mod fl_sim::nav::astar` with `DistanceField` (`cost`, `path_to`, Clone/PartialEq/Debug), `SearchStats` (`expanded`, Clone/PartialEq/Default/Debug), `astar`, `cell_centre`, `dijkstra_from`, `edge_cost`, `nearest_passable`, `octile` and `type CellXY = (u32, u32)`.
- `pub mod fl_sim::nav::los` with `line_clear` and `smooth`.
- The same items re-exported at `fl_sim::nav::{CellXY, DistanceField, SearchStats, astar, cell_centre, dijkstra_from, edge_cost, line_clear, nearest_passable, octile, smooth}`.

Bless reason: `M1 Task 6: nav A*, LOS, smoothing`.

## Files changed

- `crates/fl_sim/src/nav/astar.rs` (new)
- `crates/fl_sim/src/nav/los.rs` (new)
- `crates/fl_sim/src/nav/mod.rs`
- `crates/fl_sim/tests/astar.rs` (new)
- `xtask/snapshots/api/fl_sim.txt.new` (new, protected output)

## Self-review and concerns

1. **Smoothing cost grows with the square of path length.** Greedy extend-by-one re-walks the segment from the anchor for every extension, so on a long straight run, LOS cells grow roughly as n²/2 (about 5,000 for a 100-cell straight run). That counts against `Path.budget_per_tick` (8000) once Task 8 charges `los_cells`.
   - Task 7's "direct line first" step removes the worst case, a fully straight path.
   - A long path with one bend still pays it.
   - If the budget numbers look bad in Task 8 or Task 13, the fix is a cheaper funnel or a capped look-ahead inside `smooth`, without changing its signature.
2. **The A* expanded count includes the goal pop.** I pinned 4 for the open grid. Task 7's `direct_line_costs_only_los` expects 0 expanded only on the line-of-sight path, which never calls astar, so the two do not conflict.
3. **Gate wall time.** The run took 401 s against a 300 s budget, mostly `test-headless` and `api`. Nothing in this task is slow: the astar binary runs in about 1.3 s.
4. No `Cargo.toml` or `Cargo.lock` change, no protected file edited, and no Bevy API used.

---

## Fix round 1 (R93, R94, review minors)

**Status:** DONE. **Commit:** `26c5b39` fix(fl_sim): Task 6 fix round 1: galloping string-pulling, pinned tie-break and work bounds (`Refs: ADR-0015`).

### What changed

- **R93: `smooth` (`crates/fl_sim/src/nav/los.rs`).** The signature is unchanged.
  - From each anchor `a`:
    1. Gallop: probe `a+2, a+4, a+8, …`, capped at the last point, with `line_clear`.
    2. On the first blocked probe, bisect back to the last clear point.
    3. If no probe was clear, check the segment to `a+1` as well, with a counted `line_clear` plus a `debug_assert!`.
  - Every emitted segment has therefore passed `line_clear`.
  - The rustdoc pins the rule and the bound `6·N·log₂N + 12·N`.
  - A `# Preconditions` section, enforced by `debug_assert!`s, requires `start` in `cells[0]` and `goal` in the last cell. The test profile inherits `dev`, so these asserts run in tests.
- **R94: semantics in the module docs.**
  - `astar.rs` documents endpoint, expanded, Dijkstra-source and ring-scan semantics.
  - It also documents that callers keep the bounds tight: buffers take about 9 B per bounds cell, are uncounted, and come to about 150 MB for full-map bounds on 4096².
  - `los.rs` documents the floor and no-clamp rule, the corner and edge rule, and symmetry.
- **Finding on Minor 3:** a diagonal's side cell can never lie outside the bounds. The bounds are a rectangle that holds both ends of the step, so it also holds both side cells.
  - I corrected the `neighbours` comment, which wrongly said the side cells may lie outside, and documented this in the module docs.
  - The pin `diagonal_at_the_bounds_edge` uses bounds of exactly the two cells' box: the diagonal is allowed, and a blocked side cell forbids it.
- **Tests (`crates/fl_sim/tests/astar.rs`):**
  - Work-bound pins:
    - `smooth_work_is_bounded_straight`: 200-cell row.
    - `smooth_work_is_bounded_diagonal`: 100 × 100 diagonal.
    - `smooth_work_is_bounded_round_a_bend`: 120 + 80 L-shaped corridor, 2 corners.
    - Each asserts `cells_checked ≤ 6·N·ceil(log₂N) + 12·N`, that every corner chains `line_clear`, and the corner count.
  - `astar_breaks_equal_f_and_h_by_y_then_x`: 3×3 map, water at (1,1), path (0,1) → (2,1) goes via y = 0 as `[(0,1),(0,0),(1,0),(2,0),(2,1)]`.
  - `diagonal_at_the_bounds_edge`.
  - `line_clear_is_symmetric`: a 256-case proptest whose offsets are often exactly 0 or the centre, so it covers edges and corners.
  - `astar_matches_dijkstra_cost`: the sub-rect run now draws its pair from the passable cells inside the sub-rect.
  - `smooth_keeps_path_valid`: the no-route branch now asserts that `dijkstra_from` also finds no route.
- **Left to later tasks (R95):** `nearest_passable` probe counting goes to Task 7, and the multi-source Dijkstra to Task 10.

### TDD evidence

- **RED:** `cargo nextest run --profile agent -p fl_sim --test astar`, before the `smooth` change:
  ```
  21 tests run: 19 passed, 2 failed
  thread 'smooth_work_is_bounded_diagonal' panicked at crates\fl_sim\tests\astar.rs:232:5:
  14945 > 5400
  thread 'smooth_work_is_bounded_straight' panicked at crates\fl_sim\tests\astar.rs:217:5:
  20097 > 12000
  ```
  - This was expected: greedy extend-by-one costs about 0.5·n² straight and 1.5·n² diagonal.
  - The L-bend test passed under greedy as well (its bound, 12,000 for N = 200, is loose there). It stays as a pin on the corner count and validity.
  - The tie-break, bounds-edge and symmetry tests are pins on behaviour that was already correct, so they passed at once.
  - My first run also failed the L-bend test on a wrong expected path length in the test itself (201). The path is 200 cells, because the corner diagonal is forbidden; I fixed the test.
- **GREEN:** the same command after the change:
  ```
  Summary [   2.043s] 21 tests run: 21 passed, 0 skipped
  ```
- **Full suite:** `cargo nextest run --profile agent -p fl_sim`:
  ```
  Summary [  26.427s] 283 tests run: 283 passed, 0 skipped
  ```
- `cargo clippy -p fl_sim --all-targets -- -D warnings`: clean.
- `cargo fmt --check`: clean.
- `cargo doc -p fl_sim --no-deps`: no warnings in `astar.rs` or `los.rs`.

### Gate

`cargo xtask gate task --track sim-core`:

```
PASS fmt (2.0s)
PASS clippy-drift (2.1s)
PASS claude-md (0.0s)
FAIL check-scope (0.3s)
    1 of 5 changed files are outside track(s) sim-core:
      xtask/snapshots/api/fl_sim.txt.new → contract
PASS protected (0.3s)
WARN test-guard (0.4s): test-guard: 3 findings (reviewer must accept each)
    TEST-GUARD crates/fl_sim/tests/astar.rs:537 NewAssume: prop_assume!(!cells.is_empty());
    TEST-GUARD crates/fl_sim/tests/astar.rs:580 NewAssume: prop_assume!(!cells.is_empty());
    TEST-GUARD crates/fl_sim/tests/astar.rs:635 NewAssume: prop_assume!(!cells.is_empty());
PASS clippy (35.7s)
PASS arch-check (4.8s)
PASS test-headless (128.1s)
SKIP test-client: no client package touched since 7ce8824 (.gate-base)
FAIL api (29.5s)
    api: 1 of 5 crates changed or new
      fl_sim: changed
    run `cargo xtask api` to write the .txt.new files for the lead to bless
PASS data-check (18.6s)
gate task: FAIL (check-scope, api) in 222s
```

- `api` and `check-scope` fail only because of the bless-pending `fl_sim.txt.new`. This round leaves it untouched, since no signature changed.
- test-guard lists the same 3 `prop_assume!`s the review already accepted; only their line numbers moved.
