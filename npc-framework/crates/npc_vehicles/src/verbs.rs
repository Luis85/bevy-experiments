//! The `mount` and `dismount` verbs (reserved for this crate by
//! `npc_interaction::builtin::RESERVED_VERBS`) and the seat claims of
//! mounts in progress.
//!
//! Both run through `npc_interaction::begin` like every verb, so the
//! `Interact` leaf, the `Interact` command, agent affordances and this
//! crate's `Mount`/`Dismount` commands take one path and journal the same
//! `Interacted` entries.
//!
//! - **`mount`** (target: the vehicle). Offered for vehicles. Needs the
//!   actor within `VehicleConfig::mount_reach_mm`, not mounted already, not
//!   a vehicle itself, and a seat: the one it claimed, else the first free
//!   seat of its [`SeatPreference`](crate::SeatPreference) role, else the
//!   first free seat ([`default_seat`](crate::default_seat); the `Mount`
//!   command with an explicit role claims that role's seat first, strictly). No target reservation: seats are claimed one by one
//!   instead ([`claim_seats_system`]), so two units mount different seats of
//!   one vehicle at once but never the same seat. Takes
//!   `VehicleConfig::mount_ticks`.
//! - **`dismount`** (target: the vehicle the actor sits in). Needs a free
//!   cell next to the vehicle ([`dismount_spot`](crate::dismount_spot)).
//!   Takes `VehicleConfig::dismount_ticks`.

use bevy_ecs::prelude::*;
use npc_ai::{ActiveLeaf, Status};
use npc_interaction::builtin::{DISMOUNT, MOUNT};
use npc_interaction::run::metres;
use npc_interaction::{Ctx, InteractLeaf, Interacting, InteractionDef};
use npc_kernel::{SimId, SimPos};

use crate::seats::{
    claim_seat, config, default_seat, dismount_now, dismount_spot, entity_of, mounted_in,
    release_claim, seat_actor,
};
use crate::vehicle::{MountedIn, Occupants, SeatSlot, Vehicle};

fn is_vehicle(world: &World, ctx: &Ctx) -> bool {
    world.get::<Vehicle>(ctx.target).is_some()
}

fn mount_precondition(world: &World, ctx: &Ctx) -> Result<(), String> {
    if ctx.actor == ctx.target {
        return Err("cannot mount itself".into());
    }
    if world.get::<Vehicle>(ctx.actor).is_some() {
        return Err("a vehicle cannot mount".into());
    }
    if let Some(m) = mounted_in(world, ctx.actor) {
        return Err(format!("already mounted in #{}", m.vehicle.0));
    }
    let reach = config(world).mount_reach_mm;
    let (Some(a), Some(b)) = (
        world.get::<SimPos>(ctx.actor).copied(),
        world.get::<SimPos>(ctx.target).copied(),
    ) else {
        return Err("out of reach".into());
    };
    let d2 = a.dist2(b);
    if d2 > i64::from(reach) * i64::from(reach) {
        return Err(format!(
            "too far: {} (reach {})",
            metres(d2.isqrt()),
            metres(i64::from(reach))
        ));
    }
    mount_seat(world, ctx).map(|_| ())
}

/// The seat a mount in progress goes to.
fn mount_seat(world: &World, ctx: &Ctx) -> Result<u8, String> {
    default_seat(world, ctx.target, ctx.actor)
}

fn mount_effect(world: &mut World, ctx: &Ctx) {
    if let Ok(seat) = mount_seat(world, ctx) {
        seat_actor(world, ctx.actor, ctx.target, seat);
    }
}

fn mount_ticks(world: &World, _: &Ctx) -> u32 {
    config(world).mount_ticks
}

fn offers_dismount(world: &World, ctx: &Ctx) -> bool {
    is_vehicle(world, ctx)
        && mounted_in(world, ctx.actor).is_some_and(|m| m.vehicle == ctx.target_id)
}

fn dismount_precondition(world: &World, ctx: &Ctx) -> Result<(), String> {
    match mounted_in(world, ctx.actor) {
        Some(m) if m.vehicle == ctx.target_id => {}
        _ => return Err(format!("not mounted in #{}", ctx.target_id.0)),
    }
    dismount_spot(world, ctx.target)
        .map(|_| ())
        .ok_or_else(|| "no free cell to dismount".into())
}

fn dismount_effect(world: &mut World, ctx: &Ctx) {
    let _ = dismount_now(world, ctx.actor);
}

fn dismount_ticks(world: &World, _: &Ctx) -> u32 {
    config(world).dismount_ticks
}

/// The `mount` verb.
pub fn mount_verb() -> InteractionDef {
    InteractionDef::new(MOUNT, mount_precondition, mount_effect)
        .offer(is_vehicle)
        .no_reach()
        .no_reservation()
        .duration_fn(mount_ticks)
}

/// The `dismount` verb.
pub fn dismount_verb() -> InteractionDef {
    InteractionDef::new(DISMOUNT, dismount_precondition, dismount_effect)
        .offer(offers_dismount)
        .no_reach()
        .no_reservation()
        .duration_fn(dismount_ticks)
}

/// Cancels `actor`'s interaction (journal `Interacted { outcome:
/// "cancelled" }`) and fails the `Interact` leaf that started it, if any.
pub fn cancel_interaction(world: &mut World, actor: Entity, reason: &str) {
    let from_leaf = world.get::<InteractLeaf>(actor).is_some()
        && world
            .get::<Interacting>(actor)
            .is_some_and(|i| i.source == npc_interaction::Source::Tree);
    npc_interaction::cancel(world, actor, reason);
    if from_leaf && let Some(mut leaf) = world.get_mut::<ActiveLeaf>(actor) {
        leaf.status = Status::Failure;
    }
}

/// Units in an interaction ([`claim_seats_system`]'s cached query).
pub type MountingQuery = QueryState<(&'static SimId, Entity, &'static Interacting)>;

/// Vehicles and their seats ([`claim_seats_system`]'s cached query).
pub type ClaimsQuery = QueryState<(&'static SimId, Entity, &'static Occupants), With<Vehicle>>;

/// [`NpcVehicleSet::Crew`](crate::NpcVehicleSet::Crew), before the
/// interaction completions: keeps seat claims in step with the mounts in
/// progress. Claims whose claimant is no longer mounting that vehicle are
/// dropped; then every mount in progress without a claim (ascending
/// `SimId`) claims a seat, or is cancelled with the reason when none is
/// left. So of two units mounting the last seat in one tick, the lower
/// `SimId` gets it.
pub fn claim_seats_system(
    world: &mut World,
    mounting_q: &mut MountingQuery,
    claims_q: &mut ClaimsQuery,
) {
    let _span = tracing::info_span!("mechanic", name = "vehicles").entered();
    let mut mounting: Vec<(SimId, Entity, SimId)> = mounting_q
        .iter(world)
        .filter(|(_, _, i)| i.verb == MOUNT)
        .map(|(&id, e, i)| (id, e, i.target))
        .collect();
    mounting.sort_unstable_by_key(|m| m.0);
    // Stale claims.
    let mut vehicles: Vec<(SimId, Entity)> = claims_q
        .iter(world)
        .filter(|(_, _, o)| o.seats.iter().any(|s| matches!(s, SeatSlot::Claimed(_))))
        .map(|(&id, e, _)| (id, e))
        .collect();
    vehicles.sort_unstable_by_key(|v| v.0);
    for (vid, ve) in vehicles {
        let claims: Vec<SimId> = world
            .get::<Occupants>(ve)
            .map(|o| {
                o.seats
                    .iter()
                    .filter_map(|s| match s {
                        SeatSlot::Claimed(id) => Some(*id),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default();
        for claimant in claims {
            let live = mounting
                .iter()
                .any(|&(id, _, target)| id == claimant && target == vid);
            if !live {
                release_claim(world, ve, claimant);
            }
        }
    }
    // New mounts.
    for (id, actor, target) in mounting {
        let Some(vehicle) = entity_of(world, target) else {
            continue;
        };
        let claimed = world
            .get::<Occupants>(vehicle)
            .is_some_and(|o| o.claim_of(id).is_some());
        if claimed {
            continue;
        }
        match default_seat(world, vehicle, actor) {
            Ok(seat) => {
                claim_seat(world, vehicle, id, seat);
            }
            Err(why) => cancel_interaction(world, actor, &why),
        }
    }
}

/// Whether `actor` is mounted in `vehicle`.
pub fn is_mounted_in(world: &World, actor: Entity, vehicle: SimId) -> bool {
    world
        .get::<MountedIn>(actor)
        .is_some_and(|m| m.vehicle == vehicle)
}
