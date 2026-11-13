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
            state.revision = grid.revision();
            state.routes.clear();
        }
        if let Some(known) = state.routes.get(&(start, goal)) {
            return known.map(|distance_mm| RouteEstimate { distance_mm });
        }
    }
    let found = search(grid, start, goal);
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
fn search(grid: &SpatialGrid, start: CellCoord, goal: CellCoord) -> Option<u32> {
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
