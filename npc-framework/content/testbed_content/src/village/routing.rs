//! Bounded resident recovery from integer movement at an obstacle corner.
//!
//! The generic navigator remains authoritative. We only request a short clear
//! waypoint before retrying the real destination, never teleport or edit paths.
use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use npc_framework::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::json;

const BLOCKED_TICKS: u32 = 40;
const RETRY_TICKS: u64 = 100;
const MAX_ATTEMPTS: u8 = 3;

#[derive(Component, Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
#[reflect(Component)]
pub struct RouteRecovery {
    pub destination: SimPos,
    pub waypoint: Option<SimPos>,
    pub anchor: SimPos,
    pub attempts: u8,
    pub retry_after: u64,
}
impl SimHash for RouteRecovery {
    const NAME: &'static str = "island_route_recovery";
    fn sim_hash(&self, hash: &mut Fnv64) {
        self.destination.sim_hash(hash);
        hash.opt(self.waypoint.as_ref(), |hash, pos| pos.sim_hash(hash));
        self.anchor.sim_hash(hash);
        hash.u8(self.attempts);
        hash.u64(self.retry_after);
    }
}

fn waypoint(world: &World, here: SimPos, goal: SimPos) -> Option<SimPos> {
    let grid = world.get_resource::<SpatialGrid>()?;
    let cell = grid.cell_of(here);
    let clear = |candidate: SimPos| {
        candidate.dist2(here) > 250_i64.pow(2)
            && grid.bounds().contains(grid.cell_of(candidate))
            && !grid.is_blocking(grid.cell_of(candidate))
            && grid.trace_segment(here, candidate, |next| {
                next == cell || !grid.is_blocking(next)
            })
    };
    let center = grid.center(cell);
    if clear(center) {
        return Some(center);
    }
    // Cell centers avoid the sub-cell corner that caused the failed segment.
    let mut adjacent: Vec<_> = [
        (-1, -1),
        (0, -1),
        (1, -1),
        (-1, 0),
        (1, 0),
        (-1, 1),
        (0, 1),
        (1, 1),
    ]
    .into_iter()
    .map(|(dx, dz)| grid.center(CellCoord::new(cell.x + dx, cell.z + dz)))
    .filter(|candidate| clear(*candidate))
    .collect();
    adjacent.sort_by_key(|candidate| (candidate.dist2(goal), candidate.x, candidate.z));
    adjacent.into_iter().next()
}

/// Content movement intent, with a bounded physical detour for a stuck resident.
/// Call only from resident policy; player orders retain their own lifecycle.
pub(super) fn intent(world: &mut World, actor: Entity, desired: MoveIntent) -> MoveIntent {
    if super::policy::active(world).is_none() {
        return desired;
    }
    let Some(here) = world.get::<SimPos>(actor).copied() else {
        return desired;
    };
    let now = world.resource::<SimClock>().tick;
    let blocked = matches!(world.get::<MoveStatus>(actor), Some(MoveStatus::Blocked{ticks}) if *ticks >= BLOCKED_TICKS);
    let saved = world.get::<RouteRecovery>(actor).cloned();
    let mut state = saved
        .clone()
        .filter(|state| state.destination == desired.goal)
        .unwrap_or(RouteRecovery {
            destination: desired.goal,
            waypoint: None,
            anchor: here,
            attempts: 0,
            retry_after: 0,
        });
    if saved
        .as_ref()
        .is_some_and(|old| old.destination != desired.goal)
    {
        world.entity_mut(actor).remove::<RouteRecovery>();
    }
    if let Some(at) = state.waypoint {
        if here.dist2(at) <= 25_i64.pow(2) {
            state.waypoint = None;
            state.retry_after = now.saturating_add(RETRY_TICKS);
            world.entity_mut(actor).insert(state.clone());
        } else if !blocked || now < state.retry_after {
            return MoveIntent {
                goal: at,
                tolerance_mm: 0,
                ..desired
            };
        } else {
            // A changed world may block the detour. Retry the actual destination
            // first; never keep a dead waypoint alive by resetting its timer.
            state.waypoint = None;
            state.retry_after = now.saturating_add(RETRY_TICKS);
            world.entity_mut(actor).insert(state);
            return desired;
        }
    }
    if here.dist2(state.anchor) >= 2000_i64.pow(2) {
        world.entity_mut(actor).remove::<RouteRecovery>();
        state.anchor = here;
        state.attempts = 0;
        state.retry_after = 0;
    }
    if !blocked || now < state.retry_after || state.attempts >= MAX_ATTEMPTS {
        return desired;
    }
    state.attempts += 1;
    state.retry_after = now.saturating_add(RETRY_TICKS);
    state.waypoint = waypoint(world, here, desired.goal);
    let detour = state.waypoint;
    let id = world.get::<SimId>(actor).copied();
    record_effect(
        world,
        id,
        "VillageRouteRecovery",
        &json!({"destination":desired.goal,
        "waypoint":detour,"attempt":state.attempts,"bounded":MAX_ATTEMPTS}),
    );
    world.entity_mut(actor).insert(state);
    detour.map_or(desired, |at| MoveIntent {
        goal: at,
        tolerance_mm: 0,
        ..desired
    })
}

pub(super) fn arrived(world: &mut World, actor: Entity) {
    if super::policy::active(world).is_some() {
        world.entity_mut(actor).remove::<RouteRecovery>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn genuinely_enclosed_resident_has_three_attempts_and_no_teleport() {
        let mut world = World::new();
        world.insert_resource(super::super::policy::IslandAiPolicy::default());
        world.insert_resource(SimClock { tick: 100 });
        let mut grid = SpatialGrid::new(1000, GridBounds::centered(4, 4));
        for z in -2..2 {
            for x in -2..2 {
                grid.set_static(
                    CellCoord::new(x, z),
                    CellInfo {
                        flags: CellFlags {
                            blocking: true,
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                );
            }
        }
        grid.set_static(CellCoord::new(0, 0), CellInfo::default());
        world.insert_resource(grid);
        let here = SimPos::new(500, 500);
        let desired = MoveIntent::to(SimPos::new(1500, 500), 70);
        let actor = world
            .spawn((SimId(1), here, desired, MoveStatus::Blocked { ticks: 1000 }))
            .id();
        for tick in [100, 200, 300, 400, 800, 1600] {
            world.resource_mut::<SimClock>().tick = tick;
            assert_eq!(intent(&mut world, actor, desired), desired);
            assert_eq!(world.get::<SimPos>(actor), Some(&here));
        }
        let state = world.get::<RouteRecovery>(actor).unwrap();
        assert_eq!(state.attempts, MAX_ATTEMPTS);
        assert_eq!(state.waypoint, None);
        assert_eq!(
            world.get::<MoveStatus>(actor),
            Some(&MoveStatus::Blocked { ticks: 1000 })
        );
    }
}
