//! Read-only, bounded route estimates. No cache state or camera affects choices.
use bevy_ecs::prelude::*;
use npc_framework::prelude::*;
use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RouteEstimate {
    pub distance_mm: u32,
}
pub const MAX_EXPANSIONS: usize = 4096;
/// Most searched routes remembered before the memo starts over.
const MEMO_ROUTES: usize = 200_000;

/// Memo of the searched part of [`estimate`]. A search depends only on the
/// start cell, the goal cell and the grid's cells, so the key is the whole
/// input: `(grid revision, start, goal)`. Not state: neither hashed nor saved,
/// and an answer is the same with or without it.
#[derive(Resource, Default)]
pub struct RouteMemo(std::sync::Mutex<MemoState>);

#[derive(Default)]
struct MemoState {
    revision: u64,
    routes: BTreeMap<(CellCoord, CellCoord), Option<u32>>,
}

pub fn estimate(world: &World, from: SimPos, to: SimPos) -> Option<RouteEstimate> {
    let _probe = crate::village::probe::lookup(
        world,
        "sup.route_estimate.start",
        "sup.route_estimate.other",
    );
    let grid = world.get_resource::<SpatialGrid>()?;
    let start = grid.cell_of(from);
    let goal = grid.cell_of(to);
    if !grid.bounds().contains(start) || !grid.bounds().contains(goal) || grid.is_blocking(goal) {
        return None;
    }
    if grid.trace_segment(from, to, |c| c == start || !grid.is_blocking(c)) {
        return Some(RouteEstimate {
            distance_mm: (from.dist2(to) as u64).isqrt().min(u64::from(u32::MAX)) as u32,
        });
    }
    let memo = world.get_resource::<RouteMemo>();
    if let Some(mut state) = memo.and_then(|memo| memo.0.lock().ok()) {
        if state.revision != grid.revision() {
            let _probe = crate::village::probe::probe(world, "sup.route_memo_cleared");
            state.revision = grid.revision();
            state.routes.clear();
        }
        if let Some(known) = state.routes.get(&(start, goal)) {
            return known.map(|distance_mm| RouteEstimate { distance_mm });
        }
    }
    let found = {
        let _probe = crate::village::probe::lookup(
            world,
            "sup.route_search.start",
            "sup.route_search.other",
        );
        if crate::village::lookup::brute_force(world) {
            search_reference(grid, start, goal)
        } else {
            search(grid, start, goal)
        }
    };
    if let Some(mut state) = memo.and_then(|memo| memo.0.lock().ok())
        && state.revision == grid.revision()
    {
        if state.routes.len() >= MEMO_ROUTES {
            state.routes.clear();
        }
        state.routes.insert((start, goal), found);
    }
    found.map(|distance_mm| RouteEstimate { distance_mm })
}

/// Bounded A* over free cells: the walking distance in millimetres.
///
/// The best cost per cell is kept in a dense per-thread scratch array, with a
/// stamp per search instead of clearing it (S4: the search was most of the
/// supplier lookup). Pops, pushes and comparisons are those of the map it
/// replaced ([`search_reference`], kept for the equality test), so the
/// expansion order, the expansion cap and the answer are the same.
fn search(grid: &SpatialGrid, start: CellCoord, goal: CellCoord) -> Option<u32> {
    let bounds = grid.bounds();
    let (Some(start_index), Some(_)) = (bounds.index(start), bounds.index(goal)) else {
        return search_reference(grid, start, goal);
    };
    SCRATCH.with(|scratch| {
        let mut scratch = scratch.borrow_mut();
        scratch.begin(bounds.len());
        let heuristic = |c: CellCoord| {
            c.x.abs_diff(goal.x)
                .saturating_add(c.z.abs_diff(goal.z))
                .saturating_mul(1000)
        };
        let mut open = std::mem::take(&mut scratch.open);
        open.clear();
        open.push(Reverse((
            heuristic(start),
            Reverse(0_u32),
            start.x,
            start.z,
        )));
        scratch.set(start_index, 0);
        let mut expanded = 0;
        let mut found = None;
        while let Some(Reverse((_, Reverse(cost), x, z))) = open.pop() {
            let cell = CellCoord::new(x, z);
            let index = bounds
                .index(cell)
                .expect("cells in the open list are inside the bounds");
            if scratch.get(index).is_some_and(|best| best < cost) {
                continue;
            }
            if cell == goal {
                found = Some(cost);
                break;
            }
            expanded += 1;
            if expanded > MAX_EXPANSIONS {
                break;
            }
            for (dx, dz) in [(0, -1), (-1, 0), (1, 0), (0, 1)] {
                let next = CellCoord::new(x + dx, z + dz);
                let Some(next_index) = bounds.index(next) else {
                    continue;
                };
                if grid.cell_at(next_index).flags.blocking {
                    continue;
                }
                let proposed = cost.saturating_add(1000);
                if scratch.get(next_index).is_none_or(|known| proposed < known) {
                    scratch.set(next_index, proposed);
                    open.push(Reverse((
                        proposed.saturating_add(heuristic(next)),
                        Reverse(proposed),
                        next.x,
                        next.z,
                    )));
                }
            }
        }
        scratch.open = open;
        found
    })
}

/// Per-thread scratch of [`search`]: the best cost per dense cell index,
/// valid where the cell's stamp is the current search's. Not state.
#[derive(Default)]
struct Scratch {
    stamp: u32,
    stamps: Vec<u32>,
    costs: Vec<u32>,
    open: BinaryHeap<Reverse<OpenEntry>>,
}

/// An open-list entry: `(cost + heuristic, Reverse(cost), x, z)`.
type OpenEntry = (u32, Reverse<u32>, i32, i32);

impl Scratch {
    fn begin(&mut self, cells: usize) {
        if self.stamps.len() != cells {
            self.stamps = vec![0; cells];
            self.costs = vec![0; cells];
            self.stamp = 0;
        }
        self.stamp = self.stamp.wrapping_add(1);
        if self.stamp == 0 {
            self.stamps.fill(0);
            self.stamp = 1;
        }
    }
    fn get(&self, index: u32) -> Option<u32> {
        (self.stamps[index as usize] == self.stamp).then(|| self.costs[index as usize])
    }
    fn set(&mut self, index: u32, cost: u32) {
        self.stamps[index as usize] = self.stamp;
        self.costs[index as usize] = cost;
    }
}

thread_local! {
    static SCRATCH: std::cell::RefCell<Scratch> = std::cell::RefCell::new(Scratch::default());
}

/// The map-based search [`search`] replaced (and its fallback for cells
/// outside the bounds). Kept as the reference of the equality test.
fn search_reference(grid: &SpatialGrid, start: CellCoord, goal: CellCoord) -> Option<u32> {
    let heuristic = |c: CellCoord| {
        c.x.abs_diff(goal.x)
            .saturating_add(c.z.abs_diff(goal.z))
            .saturating_mul(1000)
    };
    let mut open = BinaryHeap::from([Reverse((
        heuristic(start),
        Reverse(0_u32),
        start.x,
        start.z,
    ))]);
    let mut costs = BTreeMap::from([(start, 0_u32)]);
    let mut expanded = 0;
    while let Some(Reverse((_, Reverse(cost), x, z))) = open.pop() {
        let cell = CellCoord::new(x, z);
        if costs.get(&cell).is_some_and(|best| *best < cost) {
            continue;
        }
        if cell == goal {
            return Some(cost);
        }
        expanded += 1;
        if expanded > MAX_EXPANSIONS {
            return None;
        }
        for (dx, dz) in [(0, -1), (-1, 0), (1, 0), (0, 1)] {
            let next = CellCoord::new(x + dx, z + dz);
            if !grid.bounds().contains(next) || grid.is_blocking(next) {
                continue;
            }
            let proposed = cost.saturating_add(1000);
            if costs.get(&next).is_none_or(|known| proposed < *known) {
                costs.insert(next, proposed);
                open.push(Reverse((
                    proposed.saturating_add(heuristic(next)),
                    Reverse(proposed),
                    next.x,
                    next.z,
                )));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use npc_framework::kernel::spatial::{CellInfo, GridBounds};

    /// A small grid with blocking cells from a counter-based pattern.
    fn grid(seed: u64, width: u32, height: u32, permille: u64) -> SpatialGrid {
        let mut grid = SpatialGrid::new(1000, GridBounds::centered(width, height));
        let bounds = grid.bounds();
        for index in 0..u32::try_from(bounds.len()).expect("small grid") {
            let roll = npc_framework::kernel::rng::sim_rand(seed, SimId(index), 0, 0x5_4_1) % 1000;
            if roll < permille {
                let mut info = CellInfo::default();
                info.flags.blocking = true;
                grid.set_static(bounds.coord(index), info);
            }
        }
        grid
    }

    #[test]
    fn dense_search_equals_the_map_search() {
        let mut compared = 0;
        let mut found = 0;
        for seed in 0..12_u64 {
            // Dense mazes, open fields and grids larger than the expansion cap.
            let (width, height, permille) = [(24, 24, 300), (40, 30, 150), (90, 90, 250)]
                [usize::try_from(seed % 3).expect("index")];
            let grid = grid(seed, width, height, permille);
            let bounds = grid.bounds();
            let cells = u32::try_from(bounds.len()).expect("small grid");
            for pair in 0..60_u32 {
                let pick = |purpose: u32| {
                    let roll = npc_framework::kernel::rng::sim_rand(
                        seed,
                        SimId(pair),
                        u64::from(pair),
                        purpose,
                    );
                    bounds.coord(u32::try_from(roll % u64::from(cells)).expect("index"))
                };
                let (start, goal) = (pick(1), pick(2));
                let dense = search(&grid, start, goal);
                assert_eq!(
                    dense,
                    search_reference(&grid, start, goal),
                    "{seed} {start:?} {goal:?}"
                );
                compared += 1;
                found += usize::from(dense.is_some());
            }
        }
        assert_eq!(compared, 720);
        assert!(
            found > 100 && found < compared,
            "{found} of {compared} found"
        );
    }
}
