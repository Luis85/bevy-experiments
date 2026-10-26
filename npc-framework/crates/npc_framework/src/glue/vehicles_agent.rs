//! Vehicles × agent-io (features `vehicles` + `agent-io`): mounting,
//! driving and unloading as agent actions, and the `vehicle` section.
//!
//! [`VehiclesAgentPlugin`] registers:
//!
//! - affordance provider `npc_vehicles`:
//!   - `mount@<vehicle>` for every vehicle the unit knows (with a
//!     perception provider: perceives or remembers, at any distance;
//!     without: within `InteractionConfig::affordance_radius_mm`) and
//!     `dismount@<vehicle>`
//!     for the one the unit sits in (`why_not` from the verbs' own checks:
//!     `"too far: 5.2 m (reach 2.0 m)"`, `"no free seat"`, ..). They are the
//!     interaction verbs, so their handler is the interaction × agent-io
//!     one (`Direct`: the `Interact` leaf; `Advisor`: the `Interact`
//!     command); vehicles the interaction provider already lists (with an
//!     `Inventory` or `Interactable`) are left to it;
//!   - [`DRIVE_TO`] (`"drive_to"`, arg `pos: [x, z]`) for a unit in the
//!     driver seat of a `ByCrew` vehicle, in the driver seat of a `Direct`
//!     vehicle that is not on [`autopilot`] (its own controller, tree or
//!     orders drive it: `why_not` says so and how to take control), and for
//!     a `Direct` vehicle itself (`why_not` while it may not move, or for a
//!     passenger);
//!   - [`UNLOAD`] (`"unload"`) for a driver or a vehicle (`why_not` when
//!     empty);
//! - handlers `drive_to` (`Direct`-mode driver of a `ByCrew` vehicle: queue
//!   `SetBlackboard goal_slot = pos` + `DriveVehicle`, because a `Direct`
//!   unit's tree is suspended; otherwise the recorded [`DriveTo`] command, i.e.
//!   `command_vehicle_to`, which a `ByCrew` driver's own tree drives) and
//!   `unload` (the `npc_vehicles::Unload` command);
//! - the recorded content command [`DriveTo`] (`{"type":"DriveTo","unit":5,
//!   "pos":[8000,0]}`: the vehicle `unit` drives, or `unit` itself when it is
//!   a vehicle, heads for `pos`);
//! - observation section [`VEHICLE_SECTION`] (`"vehicle"`): the vehicle the
//!   unit sits in (seats, occupants, driver, whether it can move and why
//!   not, the drive goal) or, for a vehicle, itself (`npc_vehicles`'
//!   `vehicle_json`), with text lines;
//! - possessions provider `npc_vehicles`: the vehicle the unit rides (always
//!   known, never "remembered");
//! - busy predicate `npc_vehicles` ([`vehicles_busy`], for `agent/events
//!   {until_idle}` and `agent/act_and_wait`): a driver while its
//!   `DriveVehicle` action runs (`"driving vehicle #4 to (8.00 m, 0.00 m)"`; an
//!   `Advisor` driver's tree starts it in the tick the [`DriveTo`] command
//!   applies) and a `Direct` vehicle while its own `MoveIntent` is `Moving`
//!   or `Blocked` (`"driving to (8.00 m, 0.00 m)"`, `"blocked on the way to
//!   (8.00 m, 0.00 m)"`), and the unit in its driver seat steering it the
//!   same way (`"driving vehicle #30 to (8.00 m, 0.00 m)"`). Positions in
//!   these texts and the `drive_to` label are metres, like observations.

#![deny(
    clippy::float_arithmetic,
    clippy::float_cmp,
    clippy::iter_over_hash_type,
    clippy::disallowed_types,
    clippy::disallowed_methods
)]

use bevy_app::{App, Plugin};
use bevy_ecs::prelude::*;
use npc_agent_io::observe::{describe, known_targets, metres_pos};
use npc_agent_io::{
    ActionPlan, ActionRequest, Affordance, AgentCtx, AgentIoAppExt, ControlMode,
    ExternalController, Rejection, Section, Step,
};
use npc_ai::{BbValue, Brain, ParamValue};
use npc_interaction::builtin::{DISMOUNT, MOUNT};
use npc_interaction::items::entity_of;
use npc_interaction::{InteractionConfig, InteractionRegistry, Interactor};
use npc_kernel::commands::{CommandError, check_pos, entity};
use npc_kernel::registry::KernelAppExt;
use npc_kernel::spatial::SpatialGrid;
use npc_kernel::{ContentCommand, SimCommand, SimId, SimPos};
use npc_locomotion::{MoveIntent, MoveStatus};
use npc_vehicles::{
    DRIVE_VEHICLE, Driving, Occupants, SeatRole, Unload, Vehicle, VehicleConfig, VehicleControl,
    can_move, command_vehicle_to, driven_vehicle, mounted_in, vehicle_json,
};
use serde::{Deserialize, Serialize};

use super::interaction_agent::{is_candidate, verb_affordance};

/// Name of the section.
pub const VEHICLE_SECTION: &str = "vehicle";
/// The driving verb.
pub const DRIVE_TO: &str = "drive_to";
/// The unloading verb.
pub const UNLOAD: &str = "unload";

/// Sends the vehicle `unit` drives (or `unit` itself, a vehicle) to `pos`
/// (`npc_vehicles::command_vehicle_to`). Recorded, so agent driving
/// replays.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DriveTo {
    pub unit: u32,
    pub pos: (i32, i32),
}

/// The vehicle `unit` steers: itself (a vehicle), the `ByCrew` vehicle it
/// sits in a driver seat of, or the `Direct` vehicle it sits in a driver
/// seat of unless that one is on [`autopilot`] (then the reason says so and
/// how to take control). Every reason matches what the `vehicle` section
/// shows.
pub fn steered_vehicle(world: &World, unit: Entity) -> Result<Entity, String> {
    if world.get::<Vehicle>(unit).is_some() {
        return Ok(unit);
    }
    if let Some((_, v)) = driven_vehicle(world, unit) {
        return Ok(v);
    }
    let Some(m) = mounted_in(world, unit) else {
        return Err("not driving a vehicle".into());
    };
    let vehicle = entity_of(world, m.vehicle)
        .ok_or_else(|| format!("riding #{}, which is gone", m.vehicle.0))?;
    let direct_driver = world.get::<Vehicle>(vehicle).is_some_and(|v| {
        v.control == VehicleControl::Direct
            && v.seats.get(usize::from(m.seat)).map(|s| s.role) == Some(SeatRole::Driver)
    });
    if !direct_driver {
        return Err(format!(
            "riding #{} but not in its driver seat",
            m.vehicle.0
        ));
    }
    match autopilot(world, vehicle) {
        Some(why) => Err(why),
        None => Ok(vehicle),
    }
}

/// Why a `Direct` vehicle drives itself, so its driver seat does not steer
/// it (`None`: the seat steers it): another controller drives it, it runs
/// its own behaviour tree, or (with `command`) it carries out orders.
/// `"wagon #30 is on autopilot: its own behaviour drives it; attach a
/// controller to #30 in Direct mode to take control"`.
pub fn autopilot(world: &World, vehicle: Entity) -> Option<String> {
    if world
        .get::<Vehicle>(vehicle)
        .is_none_or(|v| v.control != VehicleControl::Direct)
    {
        return None;
    }
    let id = *world.get::<SimId>(vehicle)?;
    let name = describe(world, id);
    if let Some(c) = world.get::<ExternalController>(vehicle)
        && c.mode != ControlMode::Observer
    {
        return Some(format!(
            "{name} is on autopilot: controller `{}` drives it ({} mode); act as #{} with that \
             controller, or detach it to steer from the driver seat",
            c.id,
            c.mode.name(),
            id.0
        ));
    }
    #[cfg(feature = "command")]
    if world
        .get::<npc_command::OrderQueue>(vehicle)
        .is_some_and(npc_command::OrderQueue::has_order)
    {
        return Some(format!(
            "{name} is on autopilot: it is carrying out its orders; give #{} a Stop order to \
             steer it yourself",
            id.0
        ));
    }
    if world.get::<Brain>(vehicle).is_some() {
        return Some(format!(
            "{name} is on autopilot: its own behaviour drives it; attach a controller to #{} \
             in Direct mode to take control",
            id.0
        ));
    }
    None
}

impl ContentCommand for DriveTo {
    const NAME: &'static str = "DriveTo";

    fn validate(&self, world: &World) -> Result<(), CommandError> {
        let unit = entity(world, self.unit)?;
        check_pos("pos", self.pos)?;
        let vehicle =
            steered_vehicle(world, unit).map_err(|reason| CommandError::Unavailable { reason })?;
        can_move(world, vehicle).map_err(|reason| CommandError::Unavailable { reason })
    }

    fn apply(self, world: &mut World) {
        let Ok(unit) = entity(world, self.unit) else {
            return;
        };
        if let Ok(vehicle) = steered_vehicle(world, unit) {
            let _ = command_vehicle_to(world, vehicle, SimPos::new(self.pos.0, self.pos.1));
        }
    }

    fn unit(&self) -> Option<SimId> {
        Some(SimId(self.unit))
    }
}

/// Possessions provider `npc_vehicles`: the vehicle the unit rides.
pub fn ridden_vehicle(world: &World, ctx: &AgentCtx) -> Vec<SimId> {
    mounted_in(world, ctx.entity)
        .map(|m| vec![m.vehicle])
        .unwrap_or_default()
}

fn vehicle_not_listed_elsewhere(world: &World, id: SimId) -> bool {
    entity_of(world, id)
        .is_some_and(|e| world.get::<Vehicle>(e).is_some() && !is_candidate(world, e))
}

fn nearby_vehicles(world: &World, ctx: &AgentCtx) -> Vec<SimId> {
    let (actor, me) = (ctx.entity, ctx.unit);
    if let Some(known) = known_targets(world, ctx) {
        return known
            .into_iter()
            .filter(|&id| id != me && vehicle_not_listed_elsewhere(world, id))
            .collect();
    }
    let radius = world
        .get_resource::<InteractionConfig>()
        .copied()
        .unwrap_or_default()
        .affordance_radius_mm;
    let (Some(grid), Some(&pos)) = (
        world.get_resource::<SpatialGrid>(),
        world.get::<SimPos>(actor),
    ) else {
        return Vec::new();
    };
    let mut out: Vec<SimId> = grid
        .in_radius(pos, radius)
        .into_iter()
        .filter(|&id| id != me && vehicle_not_listed_elsewhere(world, id))
        .collect();
    out.sort_unstable();
    out
}

fn mode_gate(a: Affordance, mode: ControlMode) -> Affordance {
    if a.ok && mode == ControlMode::Observer {
        a.why_not("observers cannot act")
    } else {
        a
    }
}

/// Affordance provider `npc_vehicles` (see the module docs).
pub fn vehicle_affordances(world: &World, ctx: &AgentCtx) -> Vec<Affordance> {
    let mut out = Vec::new();
    let mode = ctx.mode();
    let registry = world.get_resource::<InteractionRegistry>();
    if world.get::<Interactor>(ctx.entity).is_some()
        && let Some(registry) = registry
    {
        match mounted_in(world, ctx.entity) {
            Some(m) => {
                if let Some(def) = registry.verb(DISMOUNT)
                    && let Some(a) = verb_affordance(world, def, ctx.entity, m.vehicle)
                {
                    out.push(mode_gate(a, mode));
                }
            }
            None => {
                if let Some(def) = registry.verb(MOUNT) {
                    for v in nearby_vehicles(world, ctx) {
                        if let Some(a) = verb_affordance(world, def, ctx.entity, v) {
                            out.push(mode_gate(a, mode));
                        }
                    }
                }
            }
        }
    }
    let steered = steered_vehicle(world, ctx.entity);
    let is_driver_or_vehicle = steered.is_ok();
    if is_driver_or_vehicle || mounted_in(world, ctx.entity).is_some() {
        let drive = Affordance::new(DRIVE_TO, "Drive the vehicle to a point")
            .arg("pos", "[x, z] in mm")
            .requires("driver_seat");
        let drive = match &steered {
            Err(why) => drive.why_not(why.clone()),
            Ok(v) => match can_move(world, *v) {
                Err(why) => drive.why_not(why),
                Ok(()) => drive,
            },
        };
        out.push(mode_gate(drive, mode));
    }
    if let Ok(v) = steered {
        let occupants = world
            .get::<Occupants>(v)
            .map(Occupants::units)
            .unwrap_or_default();
        let id = world.get::<SimId>(v).copied().unwrap_or(ctx.unit);
        let a = Affordance::new(
            UNLOAD,
            format!(
                "Get everyone off vehicle #{} ({} aboard)",
                id.0,
                occupants.len()
            ),
        )
        .target(id);
        let a = if occupants.is_empty() {
            a.why_not("the vehicle is empty")
        } else {
            a
        };
        out.push(mode_gate(a, mode));
    }
    out
}

fn pos_arg(req: &ActionRequest) -> Result<(i32, i32), Rejection> {
    if let Some(p) = req.pos {
        return Ok(p);
    }
    let raw = req
        .args
        .get("pos")
        .cloned()
        .ok_or_else(|| Rejection::new("invalid_args", "drive_to needs pos: [x, z] (mm)"))?;
    serde_json::from_value::<(i32, i32)>(raw)
        .map_err(|e| Rejection::new("invalid_args", format!("pos must be [x, z] ({e})")))
}

/// Handler of `drive_to` (see the module docs).
pub fn drive_to_handler(
    world: &World,
    ctx: &AgentCtx,
    req: &ActionRequest,
) -> Result<ActionPlan, Rejection> {
    let pos = pos_arg(req)?;
    let cmd = DriveTo {
        unit: ctx.unit.0,
        pos,
    };
    cmd.validate(world)
        .map_err(|e| Rejection::new("unavailable", e.to_string()))?;
    let label = format!("Drive to {}", at(SimPos::new(pos.0, pos.1)));
    // A `ByCrew` driver runs the DriveVehicle leaf; a `Direct` vehicle
    // (steered from its seat, or acting itself) takes the command.
    let by_crew_driver =
        world.get::<Vehicle>(ctx.entity).is_none() && driven_vehicle(world, ctx.entity).is_some();
    if ctx.mode() == ControlMode::Direct && by_crew_driver {
        let slot = world
            .get_resource::<VehicleConfig>()
            .copied()
            .unwrap_or_default()
            .goal_slot;
        return Ok(ActionPlan::Queue {
            label,
            steps: vec![
                Step::SetBlackboard {
                    key: slot,
                    value: BbValue::Pos(pos.0, pos.1),
                },
                Step::Action {
                    action: DRIVE_VEHICLE.to_owned(),
                    param: ParamValue::None,
                },
            ],
            est_ticks: None,
        });
    }
    Ok(ActionPlan::Commands {
        label,
        commands: vec![SimCommand::content(&cmd)],
    })
}

/// Handler of `unload`.
pub fn unload_handler(
    world: &World,
    ctx: &AgentCtx,
    _: &ActionRequest,
) -> Result<ActionPlan, Rejection> {
    let v = steered_vehicle(world, ctx.entity).map_err(|why| Rejection::new("unavailable", why))?;
    let id = *world
        .get::<SimId>(v)
        .ok_or_else(|| Rejection::new("unavailable", "the vehicle is gone"))?;
    let cmd = Unload::new(id);
    cmd.validate(world)
        .map_err(|e| Rejection::new("unavailable", e.to_string()))?;
    Ok(ActionPlan::Commands {
        label: format!("Unload vehicle #{}", id.0),
        commands: vec![SimCommand::content(&cmd)],
    })
}

/// Section `vehicle` (see the module docs).
pub fn vehicle_section(world: &World, ctx: &AgentCtx) -> Option<Section> {
    let (vehicle, mine) = match mounted_in(world, ctx.entity) {
        Some(m) => (entity_of(world, m.vehicle)?, false),
        None if world.get::<Vehicle>(ctx.entity).is_some() => (ctx.entity, true),
        None => return None,
    };
    let mut value = vehicle_json(world, vehicle)?;
    if !mine
        && let (Some(me), Some(map)) = (vehicle_json(world, ctx.entity), value.as_object_mut())
        && let Some(obj) = me.as_object()
    {
        for (k, v) in obj {
            if k != "vehicle" {
                map.insert(k.clone(), v.clone());
            }
        }
    }
    let v = world.get::<Vehicle>(vehicle)?;
    let id = world.get::<SimId>(vehicle).copied()?;
    let occupants = world
        .get::<Occupants>(vehicle)
        .map(Occupants::units)
        .unwrap_or_default();
    let control = match v.control {
        VehicleControl::ByCrew => "driven by its crew",
        VehicleControl::Direct if mine => "self-propelled",
        VehicleControl::Direct if autopilot(world, vehicle).is_some() => "on autopilot",
        VehicleControl::Direct => "self-propelled, steered from its driver seat",
    };
    let mut section = Section::new(value);
    section = if mine {
        section.line(format!(
            "You are vehicle #{} ({control}), {} aboard.",
            id.0,
            occupants.len()
        ))
    } else {
        let role = value_role(world, ctx.entity).unwrap_or("Passenger");
        section.line(format!(
            "You ride vehicle #{} ({control}) in a {role} seat, {} aboard.",
            id.0,
            occupants.len()
        ))
    };
    if let Err(why) = can_move(world, vehicle) {
        section = section.line(format!("It cannot move: {why}."));
    }
    Some(section)
}

fn value_role(world: &World, actor: Entity) -> Option<&'static str> {
    let m = mounted_in(world, actor)?;
    let v = world.get::<Vehicle>(entity_of(world, m.vehicle)?)?;
    v.seats.get(usize::from(m.seat)).map(|s| s.role.name())
}

/// `p` in metres (`"(8.00 m, 0.00 m)"`).
fn at(p: SimPos) -> String {
    metres_pos(i64::from(p.x), i64::from(p.z))
}

/// Busy predicate `npc_vehicles` (see the module docs): what keeps a
/// driving unit busy besides its action queue and orders.
pub fn vehicles_busy(world: &World, e: Entity) -> Option<String> {
    if let Some(d) = world.get::<Driving>(e) {
        return Some(format!(
            "driving vehicle #{} to {}",
            d.vehicle.0,
            at(d.goal)
        ));
    }
    // In the driver seat of a `Direct` vehicle it steers (not on autopilot).
    if world.get::<Vehicle>(e).is_none()
        && let Ok(v) = steered_vehicle(world, e)
        && world
            .get::<Vehicle>(v)
            .is_some_and(|x| x.control == VehicleControl::Direct)
        && let (Some(intent), Some(status), Some(id)) = (
            world.get::<MoveIntent>(v),
            world.get::<MoveStatus>(v),
            world.get::<SimId>(v),
        )
    {
        let goal = intent.goal;
        return match status {
            MoveStatus::Moving => Some(format!("driving vehicle #{} to {}", id.0, at(goal))),
            MoveStatus::Blocked { .. } => Some(format!(
                "vehicle #{} blocked on the way to {}",
                id.0,
                at(goal)
            )),
            MoveStatus::Idle | MoveStatus::Arrived => None,
        };
    }
    if world
        .get::<Vehicle>(e)
        .is_none_or(|v| v.control != VehicleControl::Direct)
    {
        return None;
    }
    let goal = world.get::<MoveIntent>(e)?.goal;
    match world.get::<MoveStatus>(e)? {
        MoveStatus::Moving => Some(format!("driving to {}", at(goal))),
        MoveStatus::Blocked { .. } => Some(format!("blocked on the way to {}", at(goal))),
        MoveStatus::Idle | MoveStatus::Arrived => None,
    }
}

/// Vehicles × agent-io glue (see the module docs). Add it with
/// `VehiclesPlugin`, `AgentIoPlugin` and the interaction × agent-io glue
/// (whose handler runs `mount` / `dismount`). It registers the `DriveTo`
/// command.
pub struct VehiclesAgentPlugin;

impl Plugin for VehiclesAgentPlugin {
    fn build(&self, app: &mut App) {
        app.register_command::<DriveTo>()
            .register_observation(VEHICLE_SECTION, vehicle_section)
            .register_affordances("npc_vehicles", vehicle_affordances)
            .register_possessions("npc_vehicles", ridden_vehicle)
            .register_busy("npc_vehicles", vehicles_busy)
            .register_action_handler(DRIVE_TO, drive_to_handler)
            .register_action_handler(UNLOAD, unload_handler);
    }
}
