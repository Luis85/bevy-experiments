//! `npc_vehicles`: opt-in vehicles (actor-capabilities spec §4.5, plan
//! wave F2).
//!
//! Add [`VehiclesPlugin`] next to `NpcPlugins` and `InteractionPlugin`;
//! put [`Vehicle`] (seats, speed, control mode) on a sim entity with a
//! `SimPos`, and `Interactor` on the units that ride. Without the plugin
//! nothing here is registered, hashed or run.
//!
//! ## Seats and mounting
//!
//! A [`Vehicle`] has [`Seat`]s (`Driver`, `Passenger`, `Gunner`; each
//! optionally `required` and with an integer offset). [`MountedIn`] on the
//! actor and [`Occupants`] on the vehicle are the two sides of the "sits
//! in" relationship, by `SimId` (deterministic order, hashed); only this
//! crate changes them. Mounting and dismounting are the interaction verbs
//! `mount` / `dismount` (reserved for this crate by `npc_interaction`;
//! [`verbs`]), so the `Interact` leaf and command, agent affordances and
//! this crate's [`Mount`] / [`Dismount`] commands all take one path. Seats
//! are claimed one at a time, so two units never take one seat (the lower
//! `SimId` wins a tie). [`Unload`] gets everyone off at once. Dismounting
//! puts the actor on a free cell next to the vehicle ([`dismount_spot`]:
//! grid-aware, never through walls, deterministic).
//!
//! While mounted, the actor's `SimPos` is the vehicle's plus its seat
//! offset (every tick, after the movers), its own `MoveIntent` is dropped
//! (no mover moves it), and its tree can branch on the conditions
//! `Mounted`, `MountedAsDriver` and `HasDriveGoal`.
//!
//! ## Control
//!
//! - [`VehicleControl::ByCrew`]: only the driver moves it. The driver's
//!   tree runs the `DriveVehicle` action ([`drive`]), which writes the
//!   vehicle's `MoveIntent` toward the driver's goal ([`DriveGoal`], its
//!   standing order, or blackboard slot `VehicleConfig::goal_slot`). With no
//!   driver aboard the vehicle cannot move: its intent is dropped, its
//!   `MoveStatus` is `Blocked`, journal `VehicleStalled`.
//! - [`VehicleControl::Direct`]: the vehicle is a unit itself (its own
//!   `MoveIntent`, `NavAgent`, orders, brain) moving through locomotion and
//!   nav like any agent; the crew rides along (capacity or cosmetic).
//!
//! Orders addressed to a vehicle are facade glue (`npc_command`, agent
//! affordances; ADR 0011/0012); the hooks they use are
//! [`command_vehicle_to`], [`driver_of`], [`can_move`], [`set_drive_goal`],
//! [`mount`] / [`dismount`] and [`unload`].
//!
//! ## Schedule (`NpcSimTick`)
//!
//! - [`NpcVehicleSet::Drive`] then [`NpcVehicleSet::Gate`] (after
//!   `NpcSet::Actions`, before `NpcSet::Locomotion`): drivers steer, crew
//!   bookkeeping, mounted actors' intents dropped, vehicles that may not
//!   move stopped.
//! - [`NpcVehicleSet::Crew`] (after `NpcSet::Locomotion`, before
//!   `NpcInteractionSet::Complete` and `NpcSet::Effects`): the crew follows
//!   its vehicle, then seat claims for mounts in progress.
//!
//! Perf mechanic `vehicles`.
//!
//! ## Determinism and hashing
//!
//! Integer-only; vehicles and actors in `SimId` order; seats by index. The
//! plugin registers its components, `vehicle_config` and the locomotion
//! state ([`register_vehicle_hash`]), its verbs, commands, action and
//! conditions in the content hash. That changes the content hash of any app
//! that adds it, which is why it is opt-in.
//!
//! ## Presentation
//!
//! Nothing to do: a mounted actor's `SimPos` moves in the same tick as its
//! vehicle's and both copy `PrevSimPos` in `NpcSet::Inputs`, so the usual
//! interpolation keeps the rider on its seat. `MountedIn` tells a view who
//! rides where (e.g. to play a sitting pose).

#![deny(
    clippy::float_arithmetic,
    clippy::float_cmp,
    clippy::iter_over_hash_type,
    clippy::disallowed_types,
    clippy::disallowed_methods
)]

pub mod command;
pub mod drive;
pub mod hash;
pub mod inspect;
pub mod seats;
pub mod systems;
pub mod vehicle;
pub mod verbs;

use bevy_app::prelude::*;
use bevy_ecs::prelude::*;
use npc_interaction::builtin::MOUNT;
use npc_interaction::{
    InteractionAppExt, InteractionPlugin, Item, NpcInteractionSet, Source, begin,
};
use npc_kernel::journal::{self, JournalKind};
use npc_kernel::registry::KernelAppExt;
use npc_kernel::{NpcSet, NpcSimTick, SimId};

pub use command::{Dismount, Mount, Unload};
pub use drive::{
    COND_HAS_DRIVE_GOAL, COND_MOUNTED, COND_MOUNTED_AS_DRIVER, DRIVE_VEHICLE, DriveGoal, Driving,
    GoalSource, clear_drive_goal, command_vehicle_to, drive_goal, driven_vehicle, set_drive_goal,
};
pub use hash::register_vehicle_hash;
pub use inspect::{vehicle_json, vehicle_json_of};
pub use seats::{
    can_move, choose_seat, crew_allows_move, default_seat, dismount_now, dismount_spot, driver_of,
    mounted_in, seat_pos, unload,
};
pub use vehicle::{
    MountedIn, Occupants, Seat, SeatPreference, SeatRole, SeatSlot, Vehicle, VehicleControl,
};

/// Tuning. Insert before adding the plugin to change it (the plugin uses
/// `init_resource`). Hashed as `vehicle_config`.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct VehicleConfig {
    /// How close an actor must be to a vehicle to mount it, in mm.
    pub mount_reach_mm: i32,
    /// Ticks a `mount` takes (0: in the tick it starts).
    pub mount_ticks: u32,
    /// Ticks a `dismount` takes.
    pub dismount_ticks: u32,
    /// How many cell rings around the vehicle a dismount searches.
    pub dismount_radius_cells: u8,
    /// Blackboard slot `DriveVehicle` reads a `BbValue::Pos` goal from.
    pub goal_slot: u8,
    /// `MoveIntent::tolerance_mm` of driven vehicles.
    pub drive_tolerance_mm: i32,
}

impl Default for VehicleConfig {
    fn default() -> Self {
        Self {
            mount_reach_mm: 2000,
            mount_ticks: 0,
            dismount_ticks: 0,
            dismount_radius_cells: 3,
            goal_slot: 5,
            drive_tolerance_mm: 0,
        }
    }
}

/// The vehicle systems' sets in `NpcSimTick` (see the crate docs).
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NpcVehicleSet {
    /// After `NpcSet::Actions`: drivers steer their vehicles.
    Drive,
    /// After `Drive`, before `NpcSet::Locomotion`: crew bookkeeping,
    /// suspended crew locomotion, vehicles that may not move.
    Gate,
    /// After `NpcSet::Locomotion`, before `NpcInteractionSet::Complete`:
    /// the crew follows its vehicle; seat claims.
    Crew,
}

/// Starts `mount` for `actor` on `vehicle` (seat of `role` strictly, or
/// without one [`default_seat`]): claims the seat, then
/// `npc_interaction::begin`. `Err` (journaled `rejected`) when it cannot.
pub fn mount(
    world: &mut World,
    actor: Entity,
    vehicle: SimId,
    role: Option<SeatRole>,
    source: Source,
) -> Result<(), String> {
    let actor_id = *world
        .get::<SimId>(actor)
        .ok_or_else(|| "the actor is not a sim entity".to_owned())?;
    let vehicle_e =
        seats::entity_of(world, vehicle).ok_or_else(|| format!("#{} is gone", vehicle.0))?;
    let seat = match role {
        Some(r) => choose_seat(world, vehicle_e, actor_id, Some(r)),
        None => default_seat(world, vehicle_e, actor),
    };
    let seat = match seat {
        Ok(seat) => seat,
        Err(reason) => {
            journal::record(world, Some(actor_id), || JournalKind::Interacted {
                verb: MOUNT.to_owned(),
                target: vehicle,
                item: None,
                outcome: "rejected".to_owned(),
                reason: Some(reason.clone()),
                dropped: Vec::new(),
            });
            return Err(reason);
        }
    };
    let had_claim = world
        .get::<Occupants>(vehicle_e)
        .and_then(|o| o.claim_of(actor_id))
        .is_some();
    seats::claim_seat(world, vehicle_e, actor_id, seat);
    let started = begin(world, actor, MOUNT, vehicle, None, source);
    if started.is_err() && !had_claim {
        seats::release_claim(world, vehicle_e, actor_id);
    }
    started
}

/// Starts `dismount` for `actor` (from the vehicle it sits in).
pub fn dismount(world: &mut World, actor: Entity, source: Source) -> Result<(), String> {
    let m = mounted_in(world, actor).ok_or_else(|| "not mounted".to_owned())?;
    begin(
        world,
        actor,
        npc_interaction::builtin::DISMOUNT,
        m.vehicle,
        None,
        source,
    )
}

fn kind_name(world: &World, entity: Entity) -> Option<&'static str> {
    world.get::<Vehicle>(entity).map(|_| "vehicle")
}

/// The vehicles capability (see the crate docs). Needs `InteractionPlugin`
/// (checked in `finish`).
#[derive(Default)]
pub struct VehiclesPlugin;

impl Plugin for VehiclesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<VehicleConfig>();
        // Components the free-cell search filters on must exist in the
        // world (`World::try_query` needs them registered).
        app.world_mut().register_component::<MountedIn>();
        app.world_mut().register_component::<Item>();
        register_vehicle_hash(app);
        app.register_interaction(verbs::mount_verb())
            .register_interaction(verbs::dismount_verb())
            .register_action(DRIVE_VEHICLE, drive::drive_start, drive::drive_stop)
            .register_cond(COND_MOUNTED, drive::cond_mounted)
            .register_cond(COND_MOUNTED_AS_DRIVER, drive::cond_mounted_as_driver)
            .register_cond(COND_HAS_DRIVE_GOAL, drive::cond_has_drive_goal)
            .register_command::<Mount>()
            .register_command::<Dismount>()
            .register_command::<Unload>()
            .register_kind_name("~npc_vehicles", kind_name)
            .configure_sets(
                NpcSimTick,
                (
                    (NpcVehicleSet::Drive, NpcVehicleSet::Gate)
                        .chain()
                        .after(NpcSet::Actions)
                        .before(NpcSet::Locomotion),
                    NpcVehicleSet::Crew
                        .after(NpcSet::Locomotion)
                        .before(NpcInteractionSet::Complete)
                        .before(NpcSet::Effects),
                ),
            )
            .add_systems(
                NpcSimTick,
                (
                    drive::drive_vehicle_system.in_set(NpcVehicleSet::Drive),
                    systems::vehicle_gate_system.in_set(NpcVehicleSet::Gate),
                    (systems::seat_follow_system, verbs::claim_seats_system)
                        .chain()
                        .in_set(NpcVehicleSet::Crew),
                ),
            );
    }

    fn finish(&self, app: &mut App) {
        assert!(
            app.is_plugin_added::<InteractionPlugin>(),
            "VehiclesPlugin needs InteractionPlugin (its mount/dismount are interaction verbs)"
        );
    }
}
