//! Seats: choosing, claiming, taking and leaving them, finding a free cell
//! to dismount to, and whether a vehicle may move. The one place that
//! changes [`MountedIn`] and [`Occupants`], so both sides stay in step.

use std::collections::BTreeSet;

use bevy_ecs::prelude::*;
use npc_ai::ThinkRequest;
use npc_interaction::Item;
use npc_kernel::journal::{self, JournalKind};
use npc_kernel::spatial::{CellCoord, GridBlocker, SpatialGrid};
use npc_kernel::{SimId, SimIndex, SimPos};
use npc_locomotion::{MoveIntent, MoveStatus};

use crate::VehicleConfig;
use crate::vehicle::{
    MountedIn, Occupants, SeatPreference, SeatRole, SeatSlot, Vehicle, VehicleControl,
};

/// The entity of sim id `id`.
pub fn entity_of(world: &World, id: SimId) -> Option<Entity> {
    world.get_resource::<SimIndex>()?.get(id)
}

/// The config (default when the plugin is not installed).
pub fn config(world: &World) -> VehicleConfig {
    world
        .get_resource::<VehicleConfig>()
        .copied()
        .unwrap_or_default()
}

/// Where `actor` sits, if mounted.
pub fn mounted_in(world: &World, actor: Entity) -> Option<MountedIn> {
    world.get::<MountedIn>(actor).copied()
}

/// The seat's role, if the vehicle has that seat.
pub fn seat_role(world: &World, vehicle: Entity, seat: u8) -> Option<SeatRole> {
    world
        .get::<Vehicle>(vehicle)?
        .seats
        .get(usize::from(seat))
        .map(|s| s.role)
}

/// The position of seat `seat` of a vehicle standing at `at`.
pub fn seat_pos(vehicle: &Vehicle, at: SimPos, seat: u8) -> SimPos {
    let off = vehicle
        .seats
        .get(usize::from(seat))
        .map_or(SimPos::new(0, 0), |s| s.offset);
    SimPos::new(at.x.saturating_add(off.x), at.z.saturating_add(off.z))
}

/// The seat `actor` would get in `vehicle`: the one it already claimed,
/// else the first seat (index order) of `role` (any role when `None`) that
/// is neither taken nor claimed by someone else. `Err` says why not.
pub fn choose_seat(
    world: &World,
    vehicle: Entity,
    actor: SimId,
    role: Option<SeatRole>,
) -> Result<u8, String> {
    let Some(v) = world.get::<Vehicle>(vehicle) else {
        return Err("not a vehicle".into());
    };
    let occ = world.get::<Occupants>(vehicle).cloned().unwrap_or_default();
    if let Some(seat) = occ.claim_of(actor) {
        let ok = role.is_none_or(|r| v.seats.get(usize::from(seat)).is_some_and(|s| s.role == r));
        if ok {
            return Ok(seat);
        }
    }
    for (i, s) in v.seats.iter().enumerate() {
        let seat = u8::try_from(i).unwrap_or(u8::MAX);
        if role.is_some_and(|r| r != s.role) {
            continue;
        }
        if occ.slot(seat) == SeatSlot::Free {
            return Ok(seat);
        }
    }
    Err(match role {
        Some(r) if v.seats.iter().any(|s| s.role == r) => format!("no free {} seat", r.name()),
        Some(r) => format!("no {} seat", r.name()),
        None => "no free seat".into(),
    })
}

/// The role an actor asks for when it does not name one.
pub fn preferred_role(world: &World, actor: Entity) -> Option<SeatRole> {
    world.get::<SeatPreference>(actor).map(|p| p.0)
}

/// The seat `actor` gets when it names no role: its claim, else the first
/// free seat of its [`SeatPreference`] role, else the first free seat.
pub fn default_seat(world: &World, vehicle: Entity, actor: Entity) -> Result<u8, String> {
    let id = *world
        .get::<SimId>(actor)
        .ok_or_else(|| "the actor is not a sim entity".to_owned())?;
    if let Some(seat) = world.get::<Occupants>(vehicle).and_then(|o| o.claim_of(id)) {
        return Ok(seat);
    }
    match preferred_role(world, actor) {
        Some(role) => choose_seat(world, vehicle, id, Some(role))
            .or_else(|_| choose_seat(world, vehicle, id, None)),
        None => choose_seat(world, vehicle, id, None),
    }
}

/// Claims `seat` of `vehicle` for `actor` (it must be free or already
/// `actor`'s); drops any other claim `actor` holds on this vehicle.
pub fn claim_seat(world: &mut World, vehicle: Entity, actor: SimId, seat: u8) -> bool {
    let Some(mut occ) = world.get_mut::<Occupants>(vehicle) else {
        return false;
    };
    match occ.slot(seat) {
        SeatSlot::Free => {}
        SeatSlot::Claimed(id) if id == actor => return true,
        _ => return false,
    }
    release_claim_in(&mut occ, actor);
    occ.set(seat, SeatSlot::Claimed(actor));
    true
}

fn release_claim_in(occ: &mut Occupants, actor: SimId) {
    for s in &mut occ.seats {
        if *s == SeatSlot::Claimed(actor) {
            *s = SeatSlot::Free;
        }
    }
}

/// Drops `actor`'s claim on `vehicle`, if any.
pub fn release_claim(world: &mut World, vehicle: Entity, actor: SimId) {
    let holds = world
        .get::<Occupants>(vehicle)
        .is_some_and(|o| o.claim_of(actor).is_some());
    if holds && let Some(mut occ) = world.get_mut::<Occupants>(vehicle) {
        release_claim_in(&mut occ, actor);
    }
}

/// The driver: the occupant of the first taken `Driver` seat.
pub fn driver_of(world: &World, vehicle: Entity) -> Option<(SimId, Entity)> {
    let v = world.get::<Vehicle>(vehicle)?;
    let occ = world.get::<Occupants>(vehicle)?;
    occ.seated().into_iter().find_map(|(seat, id)| {
        let driver = v
            .seats
            .get(usize::from(seat))
            .is_some_and(|s| s.role == SeatRole::Driver);
        driver
            .then(|| entity_of(world, id).map(|e| (id, e)))
            .flatten()
    })
}

/// Whether `vehicle` may move now: every required seat is taken, and a
/// `ByCrew` vehicle has a driver aboard. `Err` is the reason.
pub fn can_move(world: &World, vehicle: Entity) -> Result<(), String> {
    let Some(v) = world.get::<Vehicle>(vehicle) else {
        return Err("not a vehicle".into());
    };
    let occ = world.get::<Occupants>(vehicle).cloned().unwrap_or_default();
    if v.control == VehicleControl::ByCrew && driver_of(world, vehicle).is_none() {
        return Err("no driver".into());
    }
    crew_allows_move(v, &occ)
}

/// [`can_move`] from the components alone (what a view can check).
pub fn crew_allows_move(v: &Vehicle, occ: &Occupants) -> Result<(), String> {
    let mut driver = false;
    for (i, s) in v.seats.iter().enumerate() {
        let seat = u8::try_from(i).unwrap_or(u8::MAX);
        let taken = matches!(occ.slot(seat), SeatSlot::Taken(_));
        driver |= taken && s.role == SeatRole::Driver;
        if s.required && !taken {
            return Err(format!("required {} seat {seat} is empty", s.role.name()));
        }
    }
    if v.control == VehicleControl::ByCrew && !driver {
        return Err("no driver".into());
    }
    Ok(())
}

/// Stops `agent`'s own movement: drops its intent, status `Idle`.
pub(crate) fn halt(world: &mut World, agent: Entity) {
    let Ok(mut e) = world.get_entity_mut(agent) else {
        return;
    };
    if e.contains::<MoveIntent>() {
        e.remove::<MoveIntent>();
    }
    if let Some(mut status) = e.get_mut::<MoveStatus>() {
        status.set_if_neq(MoveStatus::Idle);
    }
}

/// Seats `actor` in `seat` of `vehicle` now (the `mount` verb's effect;
/// checks were done): `MountedIn`, the seat taken, its own movement
/// stopped, its position on the seat, a re-think, journal `Mounted`.
pub(crate) fn seat_actor(world: &mut World, actor: Entity, vehicle: Entity, seat: u8) {
    let (Some(&actor_id), Some(&vehicle_id)) =
        (world.get::<SimId>(actor), world.get::<SimId>(vehicle))
    else {
        return;
    };
    let Some(v) = world.get::<Vehicle>(vehicle).cloned() else {
        return;
    };
    let at = world.get::<SimPos>(vehicle).copied().unwrap_or_default();
    if let Some(mut occ) = world.get_mut::<Occupants>(vehicle) {
        release_claim_in(&mut occ, actor_id);
        occ.set(seat, SeatSlot::Taken(actor_id));
    }
    halt(world, actor);
    world.entity_mut(actor).insert((
        MountedIn {
            vehicle: vehicle_id,
            seat,
        },
        seat_pos(&v, at, seat),
        ThinkRequest,
    ));
    let role = v
        .seats
        .get(usize::from(seat))
        .map_or(SeatRole::Passenger, |s| s.role);
    journal::record(world, Some(actor_id), || JournalKind::Mounted {
        vehicle: vehicle_id,
        seat,
        role: role.name().to_owned(),
    });
}

/// Takes `actor` out of its seat and puts it at `at` (journal
/// `Dismounted`). A `ByCrew` vehicle that loses its last driver stops.
pub(crate) fn unseat(world: &mut World, actor: Entity, at: SimPos) {
    let Some(m) = mounted_in(world, actor) else {
        return;
    };
    let Some(&actor_id) = world.get::<SimId>(actor) else {
        return;
    };
    world.entity_mut(actor).remove::<MountedIn>();
    world.entity_mut(actor).insert((at, ThinkRequest));
    if let Some(vehicle) = entity_of(world, m.vehicle) {
        if let Some(mut occ) = world.get_mut::<Occupants>(vehicle)
            && occ.slot(m.seat) == SeatSlot::Taken(actor_id)
        {
            occ.set(m.seat, SeatSlot::Free);
        }
        let by_crew = world
            .get::<Vehicle>(vehicle)
            .is_some_and(|v| v.control == VehicleControl::ByCrew);
        if by_crew && driver_of(world, vehicle).is_none() {
            halt(world, vehicle);
        }
    }
    journal::record(world, Some(actor_id), || JournalKind::Dismounted {
        vehicle: m.vehicle,
        seat: m.seat,
        at,
    });
}

/// Cells a vehicle covers: its own cell plus its `GridBlocker` footprint.
fn own_cells(
    world: &World,
    grid: &SpatialGrid,
    vehicle: Entity,
    at: SimPos,
) -> BTreeSet<CellCoord> {
    let mut cells = BTreeSet::from([grid.cell_of(at)]);
    if let Some(b) = world.get::<GridBlocker>(vehicle) {
        cells.extend(grid.footprint_cells(at, b.0));
    }
    cells
}

/// Cells where something stands on its own (not mounted, not an item),
/// other than `vehicle`.
fn standing_cells(world: &World, grid: &SpatialGrid, vehicle: Entity) -> BTreeSet<CellCoord> {
    let Some(mut q) = world
        .try_query_filtered::<(Entity, &SimPos), (With<SimId>, Without<MountedIn>, Without<Item>)>(
        )
    else {
        return BTreeSet::new();
    };
    q.iter(world)
        .filter(|(e, _)| *e != vehicle)
        .map(|(_, p)| grid.cell_of(*p))
        .collect()
}

/// Where an occupant of `vehicle` gets off: the centre of a free cell next
/// to it. Cells are searched ring by ring around the vehicle's cell (up to
/// `VehicleConfig::dismount_radius_cells`), nearest centre first, ties by
/// `(z, x)`. A cell qualifies when it is inside the grid bounds, not
/// blocking, and the straight line from the vehicle to it crosses no
/// blocking cell other than the vehicle's own (so nobody gets off through a
/// wall or a wall corner). Cells nobody stands in are preferred over cells
/// that are walkable but occupied. `None` when no cell qualifies.
pub fn dismount_spot(world: &World, vehicle: Entity) -> Option<SimPos> {
    let grid = world.get_resource::<SpatialGrid>()?;
    let at = *world.get::<SimPos>(vehicle)?;
    let radius = i32::from(config(world).dismount_radius_cells.max(1));
    let own = own_cells(world, grid, vehicle, at);
    let standing = standing_cells(world, grid, vehicle);
    let origin = grid.cell_of(at);
    let mut candidates: Vec<(u32, i64, i32, i32, CellCoord)> = Vec::new();
    for r in 1..=radius {
        for dz in -r..=r {
            for dx in -r..=r {
                if dx.abs().max(dz.abs()) != r {
                    continue;
                }
                let cell = CellCoord::new(origin.x + dx, origin.z + dz);
                if own.contains(&cell) || !grid.bounds().contains(cell) || grid.is_blocking(cell) {
                    continue;
                }
                let centre = grid.center(cell);
                let clear = grid.trace_segment(at, centre, |c| {
                    own.contains(&c) || c == cell || !grid.is_blocking(c)
                });
                if !clear {
                    continue;
                }
                let ring = u32::try_from(r).unwrap_or(u32::MAX);
                candidates.push((ring, at.dist2(centre), cell.z, cell.x, cell));
            }
        }
    }
    candidates.sort_unstable();
    candidates
        .iter()
        .find(|c| !standing.contains(&c.4))
        .or_else(|| candidates.first())
        .map(|c| grid.center(c.4))
}

/// Gets `actor` off its vehicle now, onto [`dismount_spot`]. `Err` when it
/// is not mounted or there is no free cell (it stays aboard).
pub fn dismount_now(world: &mut World, actor: Entity) -> Result<SimPos, String> {
    let m = mounted_in(world, actor).ok_or_else(|| "not mounted".to_owned())?;
    let vehicle = entity_of(world, m.vehicle).ok_or_else(|| format!("#{} is gone", m.vehicle.0))?;
    let at = dismount_spot(world, vehicle).ok_or_else(|| "no free cell to dismount".to_owned())?;
    unseat(world, actor, at);
    Ok(at)
}

/// Every occupant gets off (the `Unload` command), in `SimId` order, each
/// onto its own free cell. Occupants busy with an interaction have it
/// cancelled. Returns each occupant with where it went (or why it stayed).
pub fn unload(world: &mut World, vehicle: Entity) -> Vec<(SimId, Result<SimPos, String>)> {
    let units = world
        .get::<Occupants>(vehicle)
        .map(Occupants::units)
        .unwrap_or_default();
    let mut out = Vec::new();
    for id in units {
        let Some(actor) = entity_of(world, id) else {
            continue;
        };
        crate::verbs::cancel_interaction(world, actor, "unloaded");
        out.push((id, dismount_now(world, actor)));
    }
    out
}
