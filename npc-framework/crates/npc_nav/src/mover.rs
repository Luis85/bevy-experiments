//! Following a [`Path`]: the `GridPathMover`'s movement half.

use bevy_ecs::prelude::*;
use npc_kernel::spatial::{GridBlocker, SpatialGrid};
use npc_kernel::{SimId, SimPos};
use npc_locomotion::{MoveIntent, MoveStatus};

use crate::{NavAgent, NavState, Path};

/// Moves `pos` along `path` by up to `speed` mm (several waypoints if the
/// step allows), never entering a blocking cell other than the one it
/// stands in or one blocked only by its own blocker (`own`: sorted dense
/// indices, see [`crate::plan::own_cells`]). Returns false when blocked
/// before using any of the step.
pub fn follow(
    pos: &mut SimPos,
    path: &mut Path,
    intent: &MoveIntent,
    grid: &SpatialGrid,
    own: &[u32],
) -> bool {
    let bounds = grid.bounds();
    let open = |c| {
        !grid.is_blocking(c)
            || bounds.index(c).is_some_and(|i| {
                own.binary_search(&i).is_ok() && !grid.cell_without_one_blocker(i).flags.blocking
            })
    };
    let mut left = i64::from(intent.speed_mm_per_tick.max(0));
    let mut moved = false;
    while left > 0 && !intent.reached(*pos) {
        let Some(&wp) = path.waypoints.get(path.next as usize) else {
            break;
        };
        let step = i32::try_from(left).unwrap_or(i32::MAX);
        let next = pos.step_towards(wp, step);
        if grid.has_blocking() && next != *pos {
            let here = grid.cell_of(*pos);
            if !grid.trace_segment(*pos, next, |c| c == here || open(c)) {
                return moved;
            }
        }
        let used = pos.dist2(next).isqrt();
        *pos = next;
        moved = true;
        if next == wp {
            path.next += 1;
            left -= used.max(1);
        } else {
            break;
        }
    }
    true
}

type FollowRow<'a> = (
    &'a SimId,
    Option<&'a GridBlocker>,
    &'a mut SimPos,
    &'a MoveIntent,
    &'a mut MoveStatus,
    &'a NavState,
    Option<&'a mut Path>,
);

/// `NpcMoverSet`, after the planner: moves every `NavAgent` with a path and
/// sets its `MoveStatus`: `Arrived` within tolerance, `Moving` while
/// following or waiting for a path, `Blocked { ticks + 1 }` when a blocking
/// cell is ahead or the last request failed. Agents are independent, so
/// query order does not matter.
pub fn grid_path_mover_system(
    grid: Option<Res<SpatialGrid>>,
    mut agents: Query<FollowRow, With<NavAgent>>,
) {
    let Some(grid) = grid else {
        return;
    };
    for (&id, blocker, mut pos, intent, mut status, state, path) in &mut agents {
        let new_status = if intent.reached(*pos) {
            MoveStatus::Arrived
        } else {
            match (state, path) {
                (NavState::Following { .. }, Some(mut path)) => {
                    let mut p = *pos;
                    // Its own blocker's cells, where the grid has them.
                    let own = blocker.map_or_else(Vec::new, |b| {
                        grid.footprint_indices(grid.position_of(id).unwrap_or(p), b.0)
                    });
                    let ok = follow(&mut p, &mut path, intent, &grid, &own);
                    if p != *pos {
                        *pos = p;
                    }
                    if !ok {
                        status.blocked_again()
                    } else if intent.reached(p) {
                        MoveStatus::Arrived
                    } else {
                        MoveStatus::Moving
                    }
                }
                (NavState::Failed { .. }, _) => status.blocked_again(),
                _ => MoveStatus::Moving,
            }
        };
        status.set_if_neq(new_status);
    }
}
