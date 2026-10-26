//! Grid search over the [`NavGrid`] (ADR-0015): A* ([`astar`]), the bounded Dijkstra
//! [`DistanceField`] from one cell ([`dijkstra_from`]) or from several cells with starting
//! costs ([`dijkstra_seeded`], which flow fields use, R95) and the nearest passable cell
//! ([`nearest_passable`]).
//!
//! Every cost is an integer. Both searches are 8-connected with no corner cutting: a diagonal
//! step needs both orthogonal cells it passes passable. Ties are broken by an explicit key
//! (A*: `(f, h, y, x)`; Dijkstra: `(g, y, x)`), unique per live heap entry, so the
//! `BinaryHeap` order of equal keys never matters; a parent changes only on a strictly lower
//! g. Work is counted in [`SearchStats::expanded`]: one per cell settled.
//!
//! Settled semantics (R94):
//! - [`astar`] returns `None` with nothing expanded when `start` or `goal` is outside the
//!   bounds or not passable (callers snap a blocked endpoint first, with
//!   [`nearest_passable`]); with no route it expands the whole region reachable from
//!   `start` inside the bounds. `start == goal` is `([start], 0)` with 1 expanded.
//! - A diagonal step needs both side cells passable. The side cells of a step between two
//!   cells inside the bounds always lie inside the bounds too, since the bounds are a
//!   rectangle.
//! - [`SearchStats::expanded`] counts the goal, and every search adds to it.
//! - [`dijkstra_from`] from a source outside the bounds or not passable reaches nothing;
//!   [`dijkstra_seeded`] skips such seeds.
//! - [`edge_cost`] and the corner rule are symmetric, so a field from a set of sources is
//!   also the field toward them: [`DistanceField::next_toward_source`] walks it back.
//! - [`nearest_passable`] scans only each ring's perimeter, and stops at the ring past which
//!   every cell is off the map. It counts the cells it probes (R95).
//!
//! **Keep the bounds tight.** Each search allocates its g, parent and closed buffers for the
//! whole bounds rect (clipped to the grid), about 9 bytes per cell, whatever it expands; that
//! allocation is not counted as work. Cluster or corridor bounds are cheap; full-map bounds on
//! a 4096 × 4096 map allocate about 150 MB per call.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use fl_content::MoveClass;
use fl_math::{Fx, FxVec2};

use super::grid::{COST_ROAD, MoveProfile, NavGrid};
use crate::terrain::CellRect;

/// A cell `(x, y)` of the grid.
pub type CellXY = (u32, u32);

/// The work a search did.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchStats {
    /// Cells settled (popped for the first time), the goal included. Searches add to it.
    pub expanded: u32,
}

/// A diagonal step costs its orthogonal cost × `DIAG_NUM / DIAG_DEN` (√2 ≈ 1.41).
const DIAG_NUM: u32 = 141;
const DIAG_DEN: u32 = 100;
/// What a diagonal saves over two orthogonal road steps: `2 × COST_ROAD − 141`.
const OCTILE_SAVING: u32 = 2 * COST_ROAD as u32 - DIAG_NUM;

/// "No parent" in a parent buffer.
const NONE: u32 = u32::MAX;

/// The cost of the step between neighbouring cells `a` and `b` for `class`: the rounded-up
/// mean of the two cells' costs, `(ca + cb + 1) / 2`, and for a diagonal that `× 141 / 100`.
///
/// # Panics
/// If `a` or `b` is off the map.
pub fn edge_cost(nav: &NavGrid, class: MoveClass, a: CellXY, b: CellXY) -> u32 {
    let ca = u32::from(nav.cost(class, a.0, a.1));
    let cb = u32::from(nav.cost(class, b.0, b.1));
    let mean = (ca + cb).div_ceil(2);
    if a.0 != b.0 && a.1 != b.1 {
        mean * DIAG_NUM / DIAG_DEN
    } else {
        mean
    }
}

/// The octile distance between `a` and `b` at road cost: `COST_ROAD × (dx + dy) − 59 ×
/// min(dx, dy)`. No step costs less than a road step, so it never overestimates (and is
/// consistent): A*'s heuristic.
pub fn octile(a: CellXY, b: CellXY) -> u32 {
    let (dx, dy) = (u64::from(a.0.abs_diff(b.0)), u64::from(a.1.abs_diff(b.1)));
    let h = u64::from(COST_ROAD) * (dx + dy) - u64::from(OCTILE_SAVING) * dx.min(dy);
    u32::try_from(h).unwrap_or(u32::MAX)
}

/// The centre of cell `c`, in metres.
pub fn cell_centre(nav: &NavGrid, c: CellXY) -> FxVec2 {
    let cell = nav.cell_m().raw();
    let axis = |i: u32| Fx::from_raw(i64::from(i) * cell + cell / 2);
    FxVec2::new(axis(c.0), axis(c.1))
}

/// The cells a search may visit: `bounds` clipped to the grid, with a flat index
/// `(y − min.y) × w + (x − min.x)` into its buffers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Area {
    min: CellXY,
    w: u32,
    h: u32,
}

impl Area {
    /// `bounds` clipped to `nav`'s grid; `None` if nothing is left.
    fn new(nav: &NavGrid, bounds: CellRect) -> Option<Area> {
        let (cells_x, cells_y) = nav.cells();
        let max_x = bounds.max.0.min(cells_x.checked_sub(1)?);
        let max_y = bounds.max.1.min(cells_y.checked_sub(1)?);
        if bounds.min.0 > max_x || bounds.min.1 > max_y {
            return None;
        }
        Some(Area {
            min: bounds.min,
            w: max_x - bounds.min.0 + 1,
            h: max_y - bounds.min.1 + 1,
        })
    }

    fn len(self) -> usize {
        self.w as usize * self.h as usize
    }

    /// The index of `c`, if it is inside.
    fn index(self, c: CellXY) -> Option<usize> {
        let (dx, dy) = (c.0.checked_sub(self.min.0)?, c.1.checked_sub(self.min.1)?);
        (dx < self.w && dy < self.h).then(|| dy as usize * self.w as usize + dx as usize)
    }

    fn cell(self, i: usize) -> CellXY {
        let w = self.w as usize;
        let (dx, dy) = (i % w, i / w);
        (
            self.min.0 + u32::try_from(dx).expect("inside the area"),
            self.min.1 + u32::try_from(dy).expect("inside the area"),
        )
    }

    /// The cells from the search's source to `i`, following `parent`.
    fn walk(self, parent: &[u32], i: usize) -> Vec<CellXY> {
        let mut cells = vec![self.cell(i)];
        let mut at = parent[i];
        while at != NONE {
            let j = at as usize;
            cells.push(self.cell(j));
            at = parent[j];
        }
        cells.reverse();
        cells
    }
}

fn passable(nav: &NavGrid, p: MoveProfile, c: CellXY) -> bool {
    nav.passable(p, i64::from(c.0), i64::from(c.1))
}

/// The 8 neighbour offsets, in `(dy, dx)` order.
const STEPS: [(i64, i64); 8] = [
    (-1, -1),
    (0, -1),
    (1, -1),
    (-1, 0),
    (1, 0),
    (-1, 1),
    (0, 1),
    (1, 1),
];

/// Calls `visit(neighbour, its index)` for every neighbour of `c` that is inside `area` and
/// passable for `p` and, for a diagonal, whose two side cells are passable too (they lie
/// inside the area whenever both ends do).
fn neighbours(
    nav: &NavGrid,
    p: MoveProfile,
    area: Area,
    c: CellXY,
    mut visit: impl FnMut(CellXY, usize),
) {
    let (x, y) = (i64::from(c.0), i64::from(c.1));
    for (dx, dy) in STEPS {
        let (nx, ny) = (x + dx, y + dy);
        let (Ok(ux), Ok(uy)) = (u32::try_from(nx), u32::try_from(ny)) else {
            continue;
        };
        let Some(i) = area.index((ux, uy)) else {
            continue;
        };
        if !nav.passable(p, nx, ny) {
            continue;
        }
        if dx != 0 && dy != 0 && !(nav.passable(p, nx, y) && nav.passable(p, x, ny)) {
            continue;
        }
        visit((ux, uy), i);
    }
}

/// The cheapest path for `p` from `start` to `goal` through cells inside `bounds` (clipped
/// to the grid): 8-connected, no corner cutting, costs from [`edge_cost`] with `p.class`, the
/// [`octile`] heuristic. Pops the smallest `(f, h, y, x)`; a parent changes only on a strictly
/// lower g. Returns the cells `start..=goal` and the cost, or `None` if `start` or `goal` is
/// outside the bounds or not passable (nothing is expanded) or there is no route (every cell
/// reachable from `start` is expanded). Adds the cells settled to `stats.expanded`.
pub fn astar(
    nav: &NavGrid,
    p: MoveProfile,
    start: CellXY,
    goal: CellXY,
    bounds: CellRect,
    stats: &mut SearchStats,
) -> Option<(Vec<CellXY>, u32)> {
    let area = Area::new(nav, bounds)?;
    let (s, target) = (area.index(start)?, area.index(goal)?);
    if !passable(nav, p, start) || !passable(nav, p, goal) {
        return None;
    }
    let n = area.len();
    let mut g = vec![u32::MAX; n];
    let mut parent = vec![NONE; n];
    let mut closed = vec![false; n];
    // Keys are unique per live entry: a cell is pushed again only with a strictly lower g,
    // hence a lower f; `(y, x)` separates different cells.
    let mut open = BinaryHeap::new();
    g[s] = 0;
    let h = octile(start, goal);
    open.push(Reverse((h, h, start.1, start.0)));
    while let Some(Reverse((_, _, y, x))) = open.pop() {
        let c = (x, y);
        let i = area.index(c).expect("pushed cells are inside the area");
        if closed[i] {
            // A stale entry: the cell was settled from a cheaper one.
            continue;
        }
        closed[i] = true;
        stats.expanded = stats.expanded.saturating_add(1);
        if i == target {
            return Some((area.walk(&parent, i), g[i]));
        }
        let gc = g[i];
        neighbours(nav, p, area, c, |nc, ni| {
            if closed[ni] {
                return;
            }
            let ng = gc.saturating_add(edge_cost(nav, p.class, c, nc));
            if ng < g[ni] {
                g[ni] = ng;
                parent[ni] = u32::try_from(i).expect("an area fits u32 indices");
                let nh = octile(nc, goal);
                open.push(Reverse((ng.saturating_add(nh), nh, nc.1, nc.0)));
            }
        });
    }
    None
}

/// The cheapest cost and path from one source cell to every cell it reaches inside a
/// bounds rect, as [`dijkstra_from`] computed it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DistanceField {
    /// `None` when the bounds miss the grid.
    area: Option<Area>,
    /// Per area cell: the cost from the source, `u32::MAX` if unreached.
    cost: Vec<u32>,
    /// Per area cell: the index of the previous cell, [`NONE`] for the source or unreached.
    parent: Vec<u32>,
}

impl DistanceField {
    fn slot(&self, c: CellXY) -> Option<usize> {
        let i = self.area?.index(c)?;
        (self.cost[i] != u32::MAX).then_some(i)
    }

    /// The cost from the source to `c`; `None` if `c` is unreached or outside the bounds.
    pub fn cost(&self, c: CellXY) -> Option<u32> {
        self.slot(c).map(|i| self.cost[i])
    }

    /// The cells from the source to `c`, both included; `None` if `c` is unreached or
    /// outside the bounds.
    pub fn path_to(&self, c: CellXY) -> Option<Vec<CellXY>> {
        let i = self.slot(c)?;
        Some(self.area?.walk(&self.parent, i))
    }

    /// The neighbour of `c` one step back toward its source: the cell `c` was reached from.
    /// `None` if `c` is a source (a seed no other seed reached more cheaply), unreached or
    /// outside the bounds. The costs are symmetric, so it is also the first step of a
    /// cheapest path from `c` to the sources.
    pub fn next_toward_source(&self, c: CellXY) -> Option<CellXY> {
        let i = self.slot(c)?;
        let parent = self.parent[i];
        (parent != NONE).then(|| self.area.expect("a reached cell").cell(parent as usize))
    }
}

/// The cheapest cost and path from `source` to every cell reachable for `p` inside `bounds`
/// (clipped to the grid), with [`astar`]'s steps and costs. Pops the smallest `(g, y, x)`; a
/// parent changes only on a strictly lower g. A `source` outside the bounds or not passable
/// reaches nothing. Adds the cells settled to `stats.expanded`. [`dijkstra_seeded`] with the
/// one seed `(source, 0)`.
pub fn dijkstra_from(
    nav: &NavGrid,
    p: MoveProfile,
    source: CellXY,
    bounds: CellRect,
    stats: &mut SearchStats,
) -> DistanceField {
    dijkstra_seeded(nav, p, &[(source, 0)], bounds, stats)
}

/// The cheapest cost to every cell reachable for `p` inside `bounds` (clipped to the grid)
/// from any of `seeds`, each a source cell with its own starting cost: a cell's cost is the
/// least `seed cost + path cost` over the seeds (R95). [`astar`]'s steps and costs; pops the
/// smallest `(g, y, x)`; a parent changes only on a strictly lower g, so a seed keeps no
/// parent unless another seed reaches it strictly more cheaply. A seed outside the bounds or
/// not passable is skipped; a cell seeded twice keeps its lower cost. Adds the cells settled
/// to `stats.expanded`.
pub fn dijkstra_seeded(
    nav: &NavGrid,
    p: MoveProfile,
    seeds: &[(CellXY, u32)],
    bounds: CellRect,
    stats: &mut SearchStats,
) -> DistanceField {
    let area = Area::new(nav, bounds);
    let n = area.map_or(0, Area::len);
    let mut field = DistanceField {
        area,
        cost: vec![u32::MAX; n],
        parent: vec![NONE; n],
    };
    let Some(area) = area else {
        return field;
    };
    let mut closed = vec![false; n];
    let mut open = BinaryHeap::new();
    for &(source, cost) in seeds {
        let Some(s) = area.index(source) else {
            continue;
        };
        if passable(nav, p, source) && cost < field.cost[s] {
            field.cost[s] = cost;
            open.push(Reverse((cost, source.1, source.0)));
        }
    }
    while let Some(Reverse((_, y, x))) = open.pop() {
        let c = (x, y);
        let i = area.index(c).expect("pushed cells are inside the area");
        if closed[i] {
            // A stale entry: the cell was settled from a cheaper one.
            continue;
        }
        closed[i] = true;
        stats.expanded = stats.expanded.saturating_add(1);
        let gc = field.cost[i];
        neighbours(nav, p, area, c, |nc, ni| {
            if closed[ni] {
                return;
            }
            let ng = gc.saturating_add(edge_cost(nav, p.class, c, nc));
            if ng < field.cost[ni] {
                field.cost[ni] = ng;
                field.parent[ni] = u32::try_from(i).expect("an area fits u32 indices");
                open.push(Reverse((ng, nc.1, nc.0)));
            }
        });
    }
    field
}

/// The passable cell for `p` nearest `cell`: Chebyshev rings `0..=radius` in turn (ring 0 is
/// `cell` itself), and within the first ring holding one, the smallest `(dx² + dy², y, x)`.
/// `None` if no ring up to `radius` holds a passable cell. Adds every cell probed (each
/// perimeter cell of every ring scanned, off the map included: `8r` for ring `r ≥ 1`) to
/// `probed`; path requests charge them as work (R95).
pub fn nearest_passable(
    nav: &NavGrid,
    p: MoveProfile,
    cell: CellXY,
    radius: u32,
    probed: &mut u32,
) -> Option<CellXY> {
    let (cx, cy) = (i64::from(cell.0), i64::from(cell.1));
    let (cells_x, cells_y) = nav.cells();
    // Past this ring every cell is off the map.
    let farthest = cell.0.max(cells_x).max(cell.1).max(cells_y);
    for r in 0..=i64::from(radius.min(farthest)) {
        let mut best: Option<(i64, i64, i64)> = None;
        for dy in -r..=r {
            // The ring's top and bottom rows in full; its sides at dx = ±r only.
            let step = if dy.abs() == r { 1 } else { 2 * r };
            let mut dx = -r;
            while dx <= r {
                let (x, y) = (cx + dx, cy + dy);
                let key = (dx * dx + dy * dy, y, x);
                *probed = probed.saturating_add(1);
                if nav.passable(p, x, y) && best.is_none_or(|b| key < b) {
                    best = Some(key);
                }
                dx += step;
            }
        }
        if let Some((_, y, x)) = best {
            let cell = (u32::try_from(x).ok()?, u32::try_from(y).ok()?);
            return Some(cell);
        }
    }
    None
}
