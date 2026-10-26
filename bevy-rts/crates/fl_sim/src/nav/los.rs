//! Line of sight on the [`NavGrid`] ([`line_clear`], a supercover walk on exact integers) and
//! string-pulling a cell path into corners ([`smooth`]) (ADR-0015).
//!
//! [`line_clear`] maps each endpoint to the cell holding it (floor, with no clamp: a point off
//! the map is in a blocked cell). A corner the segment passes exactly through tests both side
//! cells; a segment lying exactly on a cell edge tests only the cells holding its points
//! (R94). The result is the same in both directions.

use std::cmp::Ordering;

use fl_math::FxVec2;

use super::astar::{CellXY, cell_centre};
use super::grid::{MoveProfile, NavGrid};

/// Whether every cell the segment `from → to` touches is passable for `p`: a supercover walk
/// from `from`'s cell to `to`'s cell (each the cell [`NavGrid::cell_of`] gives, without its
/// clamp: a point off the map is in a blocked cell). Where the segment passes exactly
/// through a cell corner, both cells beside the corner count as touched. A segment lying
/// exactly on a cell edge walks only the cells that contain its points.
///
/// Boundary crossings are ordered by cross-multiplying raw `Fx` integers, so no rounding
/// enters. Stops at the first blocked cell. Adds every cell tested to `cells_checked`.
pub fn line_clear(
    nav: &NavGrid,
    p: MoveProfile,
    from: FxVec2,
    to: FxVec2,
    cells_checked: &mut u32,
) -> bool {
    let cell = nav.cell_m().raw().max(1);
    let floor = |v: i64| v.div_euclid(cell);
    let (x0, y0) = (from.x.raw(), from.y.raw());
    let (mut cx, mut cy) = (floor(x0), floor(y0));
    let (ex, ey) = (floor(to.x.raw()), floor(to.y.raw()));
    let mut clear = |x: i64, y: i64| {
        *cells_checked = cells_checked.saturating_add(1);
        nav.passable(p, x, y)
    };
    if !clear(cx, cy) {
        return false;
    }
    let (sx, sy) = ((ex - cx).signum(), (ey - cy).signum());
    let dx = (i128::from(to.x.raw()) - i128::from(x0)).abs();
    let dy = (i128::from(to.y.raw()) - i128::from(y0)).abs();
    while (cx, cy) != (ex, ey) {
        let order = if cx == ex {
            Ordering::Greater
        } else if cy == ey {
            Ordering::Less
        } else {
            // The segment parameter of the next x and y boundary, t = dist / d, compared as
            // dist_x × dy against dist_y × dx.
            let bx = (if sx > 0 { cx + 1 } else { cx }) * cell;
            let by = (if sy > 0 { cy + 1 } else { cy }) * cell;
            let tx = (i128::from(bx) - i128::from(x0)).abs() * dy;
            let ty = (i128::from(by) - i128::from(y0)).abs() * dx;
            tx.cmp(&ty)
        };
        match order {
            Ordering::Less => cx += sx,
            Ordering::Greater => cy += sy,
            Ordering::Equal => {
                if !clear(cx + sx, cy) || !clear(cx, cy + sy) {
                    return false;
                }
                cx += sx;
                cy += sy;
            }
        }
        if !clear(cx, cy) {
            return false;
        }
    }
    true
}

/// Whether a unit of profile `p` may step from `from` to `to` (movement, Task 11): from a cell
/// `p` can pass, every cell the segment touches must be passable, as [`line_clear`] says, so
/// a step never cuts a diagonal corner between blocked cells nor hops a wall thinner than the
/// step (Task 5 review). From a cell `p` cannot pass (the terrain changed under the unit), the
/// step may stay in that cell or end in a passable one, so a stranded unit can still leave.
/// Adds every cell tested to `cells_checked`.
pub(crate) fn step_clear(
    nav: &NavGrid,
    p: MoveProfile,
    from: FxVec2,
    to: FxVec2,
    cells_checked: &mut u32,
) -> bool {
    let start = nav.cell_of(from);
    *cells_checked = cells_checked.saturating_add(1);
    if nav.passable(p, i64::from(start.0), i64::from(start.1)) {
        return line_clear(nav, p, from, to, cells_checked);
    }
    let end = nav.cell_of(to);
    *cells_checked = cells_checked.saturating_add(1);
    end == start || nav.passable(p, i64::from(end.0), i64::from(end.1))
}

/// String-pull the cell path `cells` into corners. Returns the corners after `start`, ending
/// exactly at `goal`, and adds every line-of-sight cell tested to `cells_checked`.
///
/// The points are `start`, the centres of the inner cells and `goal`. From each anchor `a`,
/// the next corner is found by a deterministic galloping search (R93):
///
/// 1. `lo = a + 1`. Probe `a + 2, a + 4, a + 8, …` (capped at the last point) with
///    [`line_clear`] from the anchor; each clear probe raises `lo`, and the first blocked
///    probe is `hi`.
/// 2. If a probe was blocked, bisect: `mid = lo + (hi − lo) / 2`, clear → `lo = mid`, else
///    `hi = mid`, until `hi = lo + 1`.
/// 3. If `lo` is still `a + 1` after the search, the segment to `a + 1` is checked too.
/// 4. Emit `points[lo]`; it becomes the anchor.
///
/// Every emitted segment has passed [`line_clear`]. Visibility along a path is not monotone,
/// so a corner need not be the farthest visible point, only a visible one. The work is at
/// most `6·N·ceil(log₂N) + 12·N` cells for `N` points.
///
/// # Preconditions
/// `cells` is a path of neighbouring cells without corner cutting, as
/// [`astar`](super::astar::astar) returns it, with `start` in `cells[0]` and `goal` in the
/// last cell ([`NavGrid::cell_of`]). Then every segment between consecutive points is clear,
/// which the fallback in step 3 relies on (checked by a `debug_assert!`). A caller whose start
/// point lies in a blocked cell snaps it first and passes the snapped cell's centre as
/// `start`, as [`plan_path`](super::hpa::plan_path) does (R95).
pub fn smooth(
    nav: &NavGrid,
    p: MoveProfile,
    start: FxVec2,
    cells: &[CellXY],
    goal: FxVec2,
    cells_checked: &mut u32,
) -> Vec<FxVec2> {
    debug_assert!(
        cells.first().is_none_or(|&c| nav.cell_of(start) == c),
        "smooth: start is not in the first cell"
    );
    debug_assert!(
        cells.last().is_none_or(|&c| nav.cell_of(goal) == c),
        "smooth: goal is not in the last cell"
    );
    let inner = cells.get(1..cells.len().saturating_sub(1)).unwrap_or(&[]);
    let mut points = Vec::with_capacity(inner.len() + 2);
    points.push(start);
    points.extend(inner.iter().map(|&c| cell_centre(nav, c)));
    points.push(goal);

    let last = points.len() - 1;
    let mut clear =
        |a: usize, b: usize| line_clear(nav, p, points[a], points[b], &mut *cells_checked);
    let mut corners = Vec::new();
    let mut anchor = 0;
    while anchor < last {
        let mut lo = anchor + 1;
        let mut hi = None;
        let mut offset = 2;
        while lo < last {
            let probe = (anchor + offset).min(last);
            if clear(anchor, probe) {
                lo = probe;
                offset *= 2;
            } else {
                hi = Some(probe);
                break;
            }
        }
        if let Some(mut hi) = hi {
            while hi - lo > 1 {
                let mid = lo + (hi - lo) / 2;
                if clear(anchor, mid) {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
        }
        if lo == anchor + 1 {
            let ok = clear(anchor, lo);
            debug_assert!(
                ok,
                "smooth: consecutive points {anchor} → {lo} are not clear"
            );
        }
        corners.push(points[lo]);
        anchor = lo;
    }
    corners
}
