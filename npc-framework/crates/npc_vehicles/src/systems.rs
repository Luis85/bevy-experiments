//! The per-tick vehicle systems around the movers.

use bevy_ecs::prelude::*;
use npc_ai::ThinkRequest;
use npc_kernel::journal::{self, JournalKind};
use npc_kernel::{SimId, SimPos};
use npc_locomotion::{MoveIntent, MoveStatus};

use crate::seats::{can_move, entity_of, halt, seat_pos};
use crate::vehicle::{MountedIn, Occupants, SeatSlot, Vehicle};

/// The mounted actors (a cached system param: building a `QueryState`
/// every tick costs more than these systems' work when nobody is mounted).
pub type MountedQuery = QueryState<(&'static SimId, Entity, &'static MountedIn)>;

/// The vehicles.
pub type VehicleQuery = QueryState<(&'static SimId, Entity), With<Vehicle>>;

/// Every mounted actor, ascending `SimId`.
fn mounted(world: &World, query: &mut MountedQuery) -> Vec<(SimId, Entity, MountedIn)> {
    let mut v: Vec<(SimId, Entity, MountedIn)> =
        query.iter(world).map(|(&id, e, m)| (id, e, *m)).collect();
    v.sort_unstable_by_key(|m| m.0);
    v
}

/// [`NpcVehicleSet::Gate`](crate::NpcVehicleSet::Gate), before the movers:
///
/// 1. Crew bookkeeping: an actor whose vehicle is gone (or no longer seats
///    it) is dismounted where it stands (journal `Dismounted`); seats whose
///    occupant is gone or sits elsewhere are freed.
/// 2. Mounted actors' own locomotion is suspended: their `MoveIntent` is
///    dropped (status `Idle`), so no mover moves them.
/// 3. A vehicle with a `MoveIntent` that cannot move (a `ByCrew` vehicle
///    without a driver, a required seat empty) loses the intent, its status
///    becomes `Blocked { ticks + 1 }`, journal `VehicleStalled { reason }`.
///
/// All in ascending `SimId` order.
pub fn vehicle_gate_system(
    world: &mut World,
    mounted_q: &mut MountedQuery,
    vehicles_q: &mut VehicleQuery,
) {
    let _span = tracing::info_span!("mechanic", name = "vehicles").entered();
    // 1. Actors whose vehicle lost them.
    for (id, actor, m) in mounted(world, mounted_q) {
        let seated = entity_of(world, m.vehicle)
            .and_then(|v| world.get::<Occupants>(v))
            .is_some_and(|o| o.slot(m.seat) == SeatSlot::Taken(id));
        if !seated {
            world.entity_mut(actor).remove::<MountedIn>();
            world.entity_mut(actor).insert(ThinkRequest);
            let at = world.get::<SimPos>(actor).copied().unwrap_or_default();
            journal::record(world, Some(id), || JournalKind::Dismounted {
                vehicle: m.vehicle,
                seat: m.seat,
                at,
            });
        }
    }
    let mut vehicles: Vec<(SimId, Entity)> =
        vehicles_q.iter(world).map(|(&id, e)| (id, e)).collect();
    vehicles.sort_unstable_by_key(|v| v.0);
    // 1b. Seats whose occupant left.
    for &(vid, ve) in &vehicles {
        let taken: Vec<(u8, SimId)> = world
            .get::<Occupants>(ve)
            .map(Occupants::seated)
            .unwrap_or_default();
        for (seat, unit) in taken {
            let still = entity_of(world, unit)
                .and_then(|e| world.get::<MountedIn>(e))
                .is_some_and(|m| m.vehicle == vid && m.seat == seat);
            if !still && let Some(mut occ) = world.get_mut::<Occupants>(ve) {
                occ.set(seat, SeatSlot::Free);
            }
        }
    }
    // 2. Suspend the crew's own locomotion.
    for (_, actor, _) in mounted(world, mounted_q) {
        if world.get::<MoveIntent>(actor).is_some() {
            halt(world, actor);
        }
    }
    // 3. Vehicles that may not move.
    for (vid, ve) in vehicles {
        if world.get::<MoveIntent>(ve).is_none() {
            continue;
        }
        if let Err(reason) = can_move(world, ve) {
            world.entity_mut(ve).remove::<MoveIntent>();
            let status = world.get::<MoveStatus>(ve).copied().unwrap_or_default();
            world.entity_mut(ve).insert(status.blocked_again());
            journal::record(world, Some(vid), || JournalKind::VehicleStalled { reason });
        }
    }
}

/// [`NpcVehicleSet::Crew`](crate::NpcVehicleSet::Crew), after the movers:
/// every mounted actor's `SimPos` is its vehicle's plus its seat offset
/// (exactly, every tick). Positions are independent, so the order does not
/// matter; it is still ascending `SimId`.
pub fn seat_follow_system(world: &mut World, mounted_q: &mut MountedQuery) {
    let _span = tracing::info_span!("mechanic", name = "vehicles").entered();
    for (_, actor, m) in mounted(world, mounted_q) {
        let Some(vehicle) = entity_of(world, m.vehicle) else {
            continue;
        };
        let (Some(v), Some(&at)) = (world.get::<Vehicle>(vehicle), world.get::<SimPos>(vehicle))
        else {
            continue;
        };
        let pos = seat_pos(v, at, m.seat);
        if world.get::<SimPos>(actor) != Some(&pos) {
            world.entity_mut(actor).insert(pos);
        }
    }
}
