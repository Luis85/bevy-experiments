//! Command × vehicles (features `command` + `vehicles`): RTS orders to and
//! from vehicles (actor-capabilities spec §4.4, §4.5).
//!
//! [`CommandVehiclesPlugin`] registers:
//!
//! - order redirect `npc_vehicles.crew`: an order given to a `ByCrew`
//!   vehicle goes to its **driver's** order queue when the driver is
//!   `Commandable` (spec: "forwarded to the driver's order queue");
//! - order executor `npc_vehicles.drive` for `Move` and `AttackMove`:
//!   - a unit driving a `ByCrew` vehicle (the redirected case) drives it:
//!     the order writes the **vehicle's** `MoveIntent` (at the vehicle's
//!     speed, so a vehicle `NavAgent` path-finds) and is done when the
//!     vehicle arrives; it fails when the driver leaves the seat or the
//!     vehicle may not move (`VehicleStalled`), or after
//!     `CommandConfig::block_fail_ticks` blocked ticks. A driver with a
//!     `Brain` runs this inside its `ExecuteOrder` leaf, like any order;
//!   - a `ByCrew` vehicle ordered itself (its driver is not `Commandable`,
//!     e.g. an AI villager whose tree runs `DriveVehicle`): the move goes
//!     through `npc_vehicles::command_vehicle_to`, i.e. the driver's
//!     `DriveGoal`, and is done when the vehicle arrives;
//!   - a rider that is not a `ByCrew` driver fails (`"riding #9 (not its
//!     driver): dismount first"`);
//!   - everyone else (and `Direct` vehicles) uses the built-in move;
//! - order executor `npc_vehicles.mount` for `Mount { vehicle }`: walk into
//!   `VehicleConfig::mount_reach_mm`, then `npc_vehicles::mount` (seat
//!   claims, the `mount` verb); done when seated (the shared engine of
//!   [`command_interaction`](super::command_interaction));
//! - order executor `npc_vehicles.unload` for `Unload`: given to a vehicle
//!   (or, redirected, to its driver), every occupant gets off at once
//!   (`npc_vehicles::unload`); fails with the first occupant that could not;
//! - with feature `view-helpers` too: the context-order resolver
//!   `npc_vehicles.mount` (right-click a vehicle → `Mount`).
//!
//! **Content setup.** A `Direct` vehicle is a unit: give it `Commandable`
//! (its order speed; use the vehicle's speed) and an `Owner`, and it takes
//! every order the built-in executors know (`Move`, `Patrol`, `Follow`,
//! group moves, `Unload`); the gate still stops it while a required seat
//! is empty. A `ByCrew` vehicle is addressed like a unit too, so it also
//! needs `Commandable` + `Owner` (ownership is checked on the vehicle);
//! its driver needs `Commandable` for the order to be forwarded to it.
//!
//! **Schedule.** Orders run in `NpcCommandSet::Execute` (after `Sense`,
//! before `Think`); the vehicle sets run after `Actions`. Both are ordered
//! through the `NpcSet` chain, so the intent the engine writes is seen by
//! the gate and the movers in the same tick (no extra ordering needed).

#![deny(
    clippy::float_arithmetic,
    clippy::float_cmp,
    clippy::iter_over_hash_type,
    clippy::disallowed_types,
    clippy::disallowed_methods
)]

use bevy_app::{App, Plugin};
use bevy_ecs::prelude::*;
use npc_command::{
    CommandAppExt, CommandConfig, Commandable, OrderExecutor, OrderQueue, OrderStep, PlayerId,
    UnitOrder,
};
use npc_interaction::builtin::MOUNT;
use npc_interaction::items::entity_of;
use npc_interaction::{Interactor, Source};
use npc_kernel::{SimId, SimPos};
use npc_locomotion::{MoveIntent, MoveStatus, clear_move_intent, set_move_intent};
use npc_vehicles::{
    MountedIn, Vehicle, VehicleConfig, VehicleControl, can_move, clear_drive_goal,
    command_vehicle_to, default_seat, drive_goal, driven_vehicle, driver_of, mounted_in, unload,
};

use super::command_interaction::{
    InteractionOrderSpec, interaction_order_cancel, interaction_order_start, interaction_order_step,
};

/// Name of the redirect.
pub const CREW_REDIRECT: &str = "npc_vehicles.crew";
/// Name of the `Move` / `AttackMove` executor.
pub const DRIVE_EXECUTOR: &str = "npc_vehicles.drive";
/// Name of the `Mount` executor.
pub const MOUNT_EXECUTOR: &str = "npc_vehicles.mount";
/// Name of the `Unload` executor.
pub const UNLOAD_EXECUTOR: &str = "npc_vehicles.unload";

fn vehicle_config(world: &World) -> VehicleConfig {
    world
        .get_resource::<VehicleConfig>()
        .copied()
        .unwrap_or_default()
}

/// Redirect `npc_vehicles.crew`: a `ByCrew` vehicle → its `Commandable`
/// driver.
pub fn crew_redirect(world: &World, unit: Entity) -> Option<Entity> {
    let v = world.get::<Vehicle>(unit)?;
    if v.control != VehicleControl::ByCrew {
        return None;
    }
    let (_, driver) = driver_of(world, unit)?;
    world.get::<OrderQueue>(driver).is_some().then_some(driver)
}

/// How a `Move` order runs on `unit`.
enum Drive {
    /// A `ByCrew` driver steering `vehicle`.
    Driver { vehicle: Entity },
    /// A `ByCrew` vehicle ordered itself (through its driver's goal).
    Vehicle,
    /// Riding without driving.
    Rider { vehicle: SimId },
    /// The built-in move (unmounted units, `Direct` vehicles).
    Builtin,
}

fn classify(world: &World, unit: Entity) -> Drive {
    if let Some(v) = world.get::<Vehicle>(unit) {
        return match v.control {
            VehicleControl::ByCrew => Drive::Vehicle,
            VehicleControl::Direct => Drive::Builtin,
        };
    }
    if let Some((_, vehicle)) = driven_vehicle(world, unit) {
        return Drive::Driver { vehicle };
    }
    match mounted_in(world, unit) {
        Some(m) => Drive::Rider { vehicle: m.vehicle },
        None => Drive::Builtin,
    }
}

fn tolerance(world: &World, unit: Entity) -> i32 {
    let arrive = world.get::<Commandable>(unit).map_or(0, |c| c.arrive_mm);
    vehicle_config(world).drive_tolerance_mm.max(arrive).max(0)
}

fn arrived(world: &World, vehicle: Entity, goal: SimPos, tolerance: i32) -> bool {
    let t = i64::from(tolerance);
    world
        .get::<SimPos>(vehicle)
        .is_some_and(|p| p.dist2(goal) <= t * t)
}

fn blocked_too_long(world: &World, vehicle: Entity) -> bool {
    let limit = world
        .get_resource::<CommandConfig>()
        .copied()
        .unwrap_or_default()
        .block_fail_ticks;
    matches!(
        world.get::<MoveStatus>(vehicle),
        Some(MoveStatus::Blocked { ticks }) if *ticks >= limit
    )
}

fn steer(world: &mut World, vehicle: Entity, goal: SimPos, tolerance: i32) {
    let speed = world
        .get::<Vehicle>(vehicle)
        .map_or(0, |v| v.speed_mm_per_tick);
    let intent = MoveIntent {
        goal,
        speed_mm_per_tick: speed,
        tolerance_mm: tolerance,
    };
    if world.get::<MoveIntent>(vehicle) != Some(&intent) {
        set_move_intent(world, vehicle, intent);
    }
}

fn drive_step(world: &mut World, unit: Entity, goal: SimPos, starting: bool) -> OrderStep {
    let tol = tolerance(world, unit);
    match classify(world, unit) {
        Drive::Driver { vehicle } => {
            if let Err(why) = can_move(world, vehicle) {
                return OrderStep::Failed(why);
            }
            if arrived(world, vehicle, goal, tol) {
                clear_move_intent(world, vehicle);
                return OrderStep::Done;
            }
            if !starting && blocked_too_long(world, vehicle) {
                clear_move_intent(world, vehicle);
                return OrderStep::Failed("blocked".into());
            }
            steer(world, vehicle, goal, tol);
            OrderStep::Running
        }
        Drive::Vehicle => {
            if arrived(world, unit, goal, tol) {
                return OrderStep::Done;
            }
            let Some((_, driver)) = driver_of(world, unit) else {
                return OrderStep::Failed("no driver".into());
            };
            if starting {
                return match command_vehicle_to(world, unit, goal) {
                    Ok(()) => OrderStep::Running,
                    Err(why) => OrderStep::Failed(why),
                };
            }
            if drive_goal(world, driver).map(|(g, _)| g) != Some(goal) {
                return OrderStep::Failed("the driver dropped the goal".into());
            }
            if blocked_too_long(world, unit) {
                return OrderStep::Failed("blocked".into());
            }
            OrderStep::Running
        }
        Drive::Rider { vehicle } => OrderStep::Failed(format!(
            "riding #{} (not its driver): dismount first",
            vehicle.0
        )),
        Drive::Builtin if starting => OrderStep::Builtin,
        Drive::Builtin => OrderStep::Failed("no longer driving".into()),
    }
}

fn start_drive(world: &mut World, unit: Entity, order: &UnitOrder) -> OrderStep {
    match order.move_goal() {
        Some(goal) => drive_step(world, unit, goal, true),
        None => OrderStep::Builtin,
    }
}

fn tick_drive(world: &mut World, unit: Entity, order: &UnitOrder) -> OrderStep {
    match order.move_goal() {
        Some(goal) => drive_step(world, unit, goal, false),
        None => OrderStep::Failed("not a move".into()),
    }
}

fn cancel_drive(world: &mut World, unit: Entity, order: &UnitOrder) {
    let Some(goal) = order.move_goal() else {
        return;
    };
    match classify(world, unit) {
        Drive::Driver { vehicle } => {
            if world
                .get::<MoveIntent>(vehicle)
                .is_some_and(|i| i.goal == goal)
            {
                clear_move_intent(world, vehicle);
            }
        }
        Drive::Vehicle => {
            if let Some((_, driver)) = driver_of(world, unit)
                && drive_goal(world, driver).map(|(g, _)| g) == Some(goal)
            {
                clear_drive_goal(world, driver);
            }
        }
        Drive::Rider { .. } | Drive::Builtin => {}
    }
}

/// The `Move` / `AttackMove` executor (see the module docs).
pub fn drive_executor() -> OrderExecutor {
    OrderExecutor::new(start_drive, tick_drive).with_cancel(cancel_drive)
}

fn mount_target(order: &UnitOrder) -> Option<SimId> {
    match order {
        UnitOrder::Mount { vehicle } => Some(*vehicle),
        _ => None,
    }
}

fn mount_precheck(world: &World, unit: Entity, _: &str, vehicle: SimId) -> Result<(), String> {
    let ve = entity_of(world, vehicle).ok_or_else(|| format!("#{} is gone", vehicle.0))?;
    if world.get::<Vehicle>(ve).is_none() {
        return Err(format!("#{} is not a vehicle", vehicle.0));
    }
    if let Some(m) = mounted_in(world, unit) {
        return Err(format!("already mounted in #{}", m.vehicle.0));
    }
    default_seat(world, ve, unit).map(|_| ())
}

fn mount_begin(world: &mut World, unit: Entity, _: &str, vehicle: SimId) -> Result<(), String> {
    npc_vehicles::mount(world, unit, vehicle, None, Source::Command)
}

fn mount_spec(world: &World, vehicle: SimId) -> InteractionOrderSpec<'static> {
    InteractionOrderSpec {
        verb: MOUNT,
        target: vehicle,
        reach_mm: Some(vehicle_config(world).mount_reach_mm),
        precheck: mount_precheck,
        begin: mount_begin,
    }
}

fn validate_mount(world: &World, unit: Entity, order: &UnitOrder) -> Result<(), String> {
    let vehicle = mount_target(order).ok_or("not a Mount order")?;
    if world.get::<Interactor>(unit).is_none() {
        return Err("the unit cannot mount (no Interactor)".into());
    }
    if world.get::<Vehicle>(unit).is_some() {
        return Err("a vehicle cannot mount".into());
    }
    let ve = entity_of(world, vehicle).ok_or_else(|| format!("#{} is gone", vehicle.0))?;
    if world.get::<Vehicle>(ve).is_none() {
        return Err(format!("#{} is not a vehicle", vehicle.0));
    }
    Ok(())
}

fn start_mount(world: &mut World, unit: Entity, order: &UnitOrder) -> OrderStep {
    let Some(vehicle) = mount_target(order) else {
        return OrderStep::Failed("not a Mount order".into());
    };
    if world
        .get::<MountedIn>(unit)
        .is_some_and(|m| m.vehicle == vehicle)
    {
        return OrderStep::Done;
    }
    let spec = mount_spec(world, vehicle);
    interaction_order_start(world, unit, spec)
}

fn tick_mount(world: &mut World, unit: Entity, order: &UnitOrder) -> OrderStep {
    let Some(vehicle) = mount_target(order) else {
        return OrderStep::Failed("not a Mount order".into());
    };
    let spec = mount_spec(world, vehicle);
    interaction_order_step(world, unit, spec)
}

fn cancel_mount(world: &mut World, unit: Entity, _: &UnitOrder) {
    interaction_order_cancel(world, unit);
}

/// The `Mount` executor (see the module docs).
pub fn mount_executor() -> OrderExecutor {
    OrderExecutor::new(start_mount, tick_mount)
        .with_validate(validate_mount)
        .with_cancel(cancel_mount)
}

/// The vehicle an `Unload` given to `unit` empties: itself, or the one it
/// sits in.
fn unload_vehicle(world: &World, unit: Entity) -> Option<Entity> {
    if world.get::<Vehicle>(unit).is_some() {
        return Some(unit);
    }
    mounted_in(world, unit).and_then(|m| entity_of(world, m.vehicle))
}

fn validate_unload(world: &World, unit: Entity, _: &UnitOrder) -> Result<(), String> {
    unload_vehicle(world, unit)
        .map(|_| ())
        .ok_or_else(|| "not a vehicle and not mounted in one".to_owned())
}

fn start_unload(world: &mut World, unit: Entity, _: &UnitOrder) -> OrderStep {
    let Some(vehicle) = unload_vehicle(world, unit) else {
        return OrderStep::Failed("not a vehicle and not mounted in one".into());
    };
    let failed = unload(world, vehicle)
        .into_iter()
        .find_map(|(id, r)| r.err().map(|why| format!("#{} stayed aboard: {why}", id.0)));
    match failed {
        Some(why) => OrderStep::Failed(why),
        None => OrderStep::Done,
    }
}

fn tick_unload(_: &mut World, _: Entity, _: &UnitOrder) -> OrderStep {
    OrderStep::Done
}

/// The `Unload` executor (see the module docs).
pub fn unload_executor() -> OrderExecutor {
    OrderExecutor::new(start_unload, tick_unload).with_validate(validate_unload)
}

/// Right-click resolver: a vehicle → `Mount`.
pub fn context_vehicle_order(world: &World, _: PlayerId, target: Entity) -> Option<UnitOrder> {
    let id = *world.get::<SimId>(target)?;
    world
        .get::<Vehicle>(target)
        .map(|_| UnitOrder::Mount { vehicle: id })
}

/// Command × vehicles glue (see the module docs). Add it with
/// `CommandPlugin`, `VehiclesPlugin` and
/// [`CommandInteractionPlugin`](super::CommandInteractionPlugin) (whose
/// finished hook ends `Mount` orders).
pub struct CommandVehiclesPlugin;

impl Plugin for CommandVehiclesPlugin {
    fn build(&self, app: &mut App) {
        app.register_order_redirect(CREW_REDIRECT, crew_redirect)
            .register_order_executor("Move", DRIVE_EXECUTOR, drive_executor())
            .register_order_executor("AttackMove", DRIVE_EXECUTOR, drive_executor())
            .register_order_executor("Mount", MOUNT_EXECUTOR, mount_executor())
            .register_order_executor("Unload", UNLOAD_EXECUTOR, unload_executor());
        #[cfg(feature = "view-helpers")]
        app.world_mut()
            .get_resource_or_init::<npc_view_helpers::command_ui::ContextOrders>()
            .add("npc_vehicles.mount", context_vehicle_order);
    }

    fn finish(&self, app: &mut App) {
        assert!(
            app.is_plugin_added::<super::CommandInteractionPlugin>(),
            "CommandVehiclesPlugin needs CommandInteractionPlugin (it ends Mount orders)"
        );
    }
}
