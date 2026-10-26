//! Driving a `ByCrew` vehicle: the `DriveVehicle` action, where its goal
//! comes from, and the conditions content trees branch on.
//!
//! The driver's goal, first that exists:
//! 1. [`DriveGoal`] on the driver (set with [`set_drive_goal`], or by
//!    [`command_vehicle_to`]: what the facade glue forwards vehicle orders
//!    to);
//! 2. the driver's standing order (`CurrentOrder` `GoTo`, i.e. a
//!    `SimCommand::Order` given to the driver);
//! 3. a `BbValue::Pos` in blackboard slot `VehicleConfig::goal_slot`.
//!
//! `DriveVehicle` writes the vehicle's `MoveIntent` (at the vehicle's
//! speed) and is `Running` until the vehicle's `MoveStatus` is `Arrived`:
//! then it succeeds and clears the goal it used. A new goal while driving
//! retargets the vehicle. It fails when the driver is not in a driver seat
//! of a `ByCrew` vehicle, has no goal, or the vehicle cannot move (journal
//! `VehicleStalled`). Aborting it stops the vehicle.

use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use npc_ai::{
    ActiveLeaf, BbKey, BbValue, Blackboard, ParamValue, Status, StopReason, ThinkRequest,
};
use npc_kernel::journal::{self, JournalKind};
use npc_kernel::orders::clear_order;
use npc_kernel::{CurrentOrder, OrderKind, SimId, SimPos};
use npc_locomotion::{MoveIntent, MoveStatus, set_move_intent};
use serde::{Deserialize, Serialize};

use crate::seats::{can_move, config, driver_of, entity_of, halt, mounted_in, seat_role};
use crate::vehicle::{SeatRole, Vehicle, VehicleControl};

/// The registered action name.
pub const DRIVE_VEHICLE: &str = "DriveVehicle";
/// Condition: the agent is mounted in any seat.
pub const COND_MOUNTED: &str = "Mounted";
/// Condition: the agent sits in a driver seat.
pub const COND_MOUNTED_AS_DRIVER: &str = "MountedAsDriver";
/// Condition: the agent drives a `ByCrew` vehicle and has a goal for it.
pub const COND_HAS_DRIVE_GOAL: &str = "HasDriveGoal";

/// A destination for the vehicle this unit drives (see the module docs).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
#[reflect(Component)]
pub struct DriveGoal(pub SimPos);

/// The `DriveVehicle` action component (on the driver).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
#[component(storage = "SparseSet")]
#[reflect(Component)]
pub struct Driving {
    pub vehicle: SimId,
    pub goal: SimPos,
}

/// Where a drive goal came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GoalSource {
    DriveGoal,
    Order,
    Blackboard,
}

/// The driver's goal and where it came from (see the module docs).
pub fn drive_goal(world: &World, driver: Entity) -> Option<(SimPos, GoalSource)> {
    if let Some(g) = world.get::<DriveGoal>(driver) {
        return Some((g.0, GoalSource::DriveGoal));
    }
    if let Some(CurrentOrder(Some(OrderKind::GoTo { pos }))) = world.get::<CurrentOrder>(driver) {
        return Some((*pos, GoalSource::Order));
    }
    let slot = config(world).goal_slot;
    let bb = world.get::<Blackboard>(driver)?;
    if usize::from(slot) >= bb.slots.len() {
        return None;
    }
    match bb.get(BbKey(slot)) {
        BbValue::Pos(x, z) => Some((SimPos::new(x, z), GoalSource::Blackboard)),
        _ => None,
    }
}

fn clear_goal(world: &mut World, driver: Entity, source: GoalSource) {
    match source {
        GoalSource::DriveGoal => {
            world.entity_mut(driver).remove::<DriveGoal>();
        }
        GoalSource::Order => clear_order(world, driver),
        GoalSource::Blackboard => {
            let slot = config(world).goal_slot;
            if let Some(mut bb) = world.get_mut::<Blackboard>(driver) {
                bb.set(BbKey(slot), BbValue::None);
            }
        }
    }
}

/// Gives the unit `driver` a destination for the vehicle it drives (or
/// will drive): inserts [`DriveGoal`] and asks it to re-think.
pub fn set_drive_goal(world: &mut World, driver: Entity, goal: SimPos) {
    if let Ok(mut e) = world.get_entity_mut(driver) {
        e.insert((DriveGoal(goal), ThinkRequest));
    }
}

/// Drops `driver`'s [`DriveGoal`].
pub fn clear_drive_goal(world: &mut World, driver: Entity) {
    if let Ok(mut e) = world.get_entity_mut(driver) {
        e.remove::<DriveGoal>();
    }
}

/// Sends `vehicle` to `goal`, the way its control mode says: `ByCrew` →
/// the driver gets a [`DriveGoal`] (its tree drives); `Direct` → the
/// vehicle's own `MoveIntent` at its speed. `Err` when it is not a vehicle,
/// a `ByCrew` vehicle has no driver, or it cannot move. This is the hook
/// for orders addressed to a vehicle (facade glue).
pub fn command_vehicle_to(world: &mut World, vehicle: Entity, goal: SimPos) -> Result<(), String> {
    let v = world
        .get::<Vehicle>(vehicle)
        .cloned()
        .ok_or_else(|| "not a vehicle".to_owned())?;
    can_move(world, vehicle)?;
    match v.control {
        VehicleControl::ByCrew => {
            let (_, driver) = driver_of(world, vehicle).ok_or_else(|| "no driver".to_owned())?;
            set_drive_goal(world, driver, goal);
        }
        VehicleControl::Direct => {
            let tolerance_mm = config(world).drive_tolerance_mm;
            set_move_intent(
                world,
                vehicle,
                MoveIntent {
                    goal,
                    speed_mm_per_tick: v.speed_mm_per_tick,
                    tolerance_mm,
                },
            );
        }
    }
    Ok(())
}

/// The `ByCrew` vehicle `driver` sits in a driver seat of.
pub fn driven_vehicle(world: &World, driver: Entity) -> Option<(SimId, Entity)> {
    let m = mounted_in(world, driver)?;
    let vehicle = entity_of(world, m.vehicle)?;
    let by_crew = world
        .get::<Vehicle>(vehicle)
        .is_some_and(|v| v.control == VehicleControl::ByCrew);
    (by_crew && seat_role(world, vehicle, m.seat) == Some(SeatRole::Driver))
        .then_some((m.vehicle, vehicle))
}

fn stalled(world: &mut World, vehicle: Entity, reason: &str) {
    if let Some(&id) = world.get::<SimId>(vehicle) {
        journal::record(world, Some(id), || JournalKind::VehicleStalled {
            reason: reason.to_owned(),
        });
    }
}

/// Writes (or retargets) the vehicle's intent toward `goal`.
fn steer(world: &mut World, vehicle: Entity, goal: SimPos) {
    let speed = world
        .get::<Vehicle>(vehicle)
        .map_or(0, |v| v.speed_mm_per_tick);
    let intent = MoveIntent {
        goal,
        speed_mm_per_tick: speed,
        tolerance_mm: config(world).drive_tolerance_mm,
    };
    if world.get::<MoveIntent>(vehicle) != Some(&intent) {
        set_move_intent(world, vehicle, intent);
    }
}

/// `DriveVehicle` start.
pub fn drive_start(world: &mut World, agent: Entity, _: &ParamValue) -> Status {
    let Some((vehicle_id, vehicle)) = driven_vehicle(world, agent) else {
        return Status::Failure;
    };
    let Some((goal, source)) = drive_goal(world, agent) else {
        return Status::Failure;
    };
    if let Err(why) = can_move(world, vehicle) {
        stalled(world, vehicle, &why);
        return Status::Failure;
    }
    let at = world.get::<SimPos>(vehicle).copied().unwrap_or_default();
    let tolerance = i64::from(config(world).drive_tolerance_mm.max(0));
    if at.dist2(goal) <= tolerance * tolerance {
        clear_goal(world, agent, source);
        return Status::Success;
    }
    steer(world, vehicle, goal);
    world.entity_mut(agent).insert(Driving {
        vehicle: vehicle_id,
        goal,
    });
    Status::Running
}

/// `DriveVehicle` stop: an aborted drive stops the vehicle.
pub fn drive_stop(world: &mut World, agent: Entity, reason: StopReason) {
    let Some(d) = world.get::<Driving>(agent).copied() else {
        return;
    };
    world.entity_mut(agent).remove::<Driving>();
    if reason == StopReason::Aborted
        && let Some(vehicle) = entity_of(world, d.vehicle)
        && world
            .get::<MoveIntent>(vehicle)
            .is_some_and(|i| i.goal == d.goal)
    {
        halt(world, vehicle);
    }
}

fn finish(world: &mut World, agent: Entity, status: Status) {
    if let Some(mut leaf) = world.get_mut::<ActiveLeaf>(agent)
        && leaf.status == Status::Running
    {
        leaf.status = status;
    }
}

/// The driving agents ([`drive_vehicle_system`]'s cached query).
pub type DriverQuery = QueryState<(
    &'static SimId,
    Entity,
    &'static Driving,
    &'static ActiveLeaf,
)>;

/// [`NpcVehicleSet::Drive`](crate::NpcVehicleSet::Drive): every driving
/// agent, ascending `SimId`. Success when the vehicle arrived (the goal it
/// used is cleared), Failure when it lost its seat, its goal or the
/// ability to move; otherwise the vehicle's intent follows the current
/// goal.
pub fn drive_vehicle_system(world: &mut World, drivers_q: &mut DriverQuery) {
    let _span = tracing::info_span!("mechanic", name = "vehicles").entered();
    let mut drivers: Vec<(SimId, Entity, Driving)> = drivers_q
        .iter(world)
        .filter(|(_, _, _, leaf)| leaf.status == Status::Running)
        .map(|(&id, e, d, _)| (id, e, *d))
        .collect();
    drivers.sort_unstable_by_key(|d| d.0);
    for (_, agent, d) in drivers {
        let Some((vehicle_id, vehicle)) = driven_vehicle(world, agent) else {
            finish(world, agent, Status::Failure);
            continue;
        };
        if vehicle_id != d.vehicle {
            finish(world, agent, Status::Failure);
            continue;
        }
        if let Err(why) = can_move(world, vehicle) {
            stalled(world, vehicle, &why);
            halt(world, vehicle);
            finish(world, agent, Status::Failure);
            continue;
        }
        let Some((goal, source)) = drive_goal(world, agent) else {
            halt(world, vehicle);
            finish(world, agent, Status::Failure);
            continue;
        };
        let arrived = goal == d.goal
            && world.get::<MoveStatus>(vehicle) == Some(&MoveStatus::Arrived)
            && world
                .get::<MoveIntent>(vehicle)
                .is_some_and(|i| i.goal == goal);
        if arrived {
            halt(world, vehicle);
            clear_goal(world, agent, source);
            finish(world, agent, Status::Success);
            continue;
        }
        if goal != d.goal {
            world.entity_mut(agent).insert(Driving {
                vehicle: vehicle_id,
                goal,
            });
        }
        steer(world, vehicle, goal);
    }
}

/// Condition `Mounted`.
pub fn cond_mounted(world: &World, agent: Entity) -> bool {
    mounted_in(world, agent).is_some()
}

/// Condition `MountedAsDriver`: in a driver seat (either control mode).
pub fn cond_mounted_as_driver(world: &World, agent: Entity) -> bool {
    mounted_in(world, agent).is_some_and(|m| {
        entity_of(world, m.vehicle)
            .and_then(|v| seat_role(world, v, m.seat))
            .is_some_and(|r| r == SeatRole::Driver)
    })
}

/// Condition `HasDriveGoal`: drives a `ByCrew` vehicle and has a goal.
pub fn cond_has_drive_goal(world: &World, agent: Entity) -> bool {
    driven_vehicle(world, agent).is_some() && drive_goal(world, agent).is_some()
}
