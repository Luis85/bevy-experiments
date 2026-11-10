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
            return Some(RouteEstimate { distance_mm: cost });
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
