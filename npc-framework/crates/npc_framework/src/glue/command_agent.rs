//! Command × agent-io (features `command` + `agent-io`): an agent gives RTS
//! orders like a player.
//!
//! [`CommandAgentPlugin`] registers:
//!
//! - observation section [`COMMAND_SECTION`] (`"command"`) for units with an
//!   `Owner`: the player, the current order and its progress, the queue, the
//!   squad, and the other units that player owns (`owned_units`, at most
//!   [`MAX_LISTED_UNITS`], ascending `SimId`);
//! - affordance provider `npc_command.orders` and handler [`ORDER`]
//!   (`"order"`): args `{"order": UnitOrder, "units": [SimId] (default: this
//!   unit), "queue": bool, "formation": {"kind","spacing_mm"}}`, e.g.
//!   `{"order":{"type":"Move","pos":[8000,0]},"units":[5,6,7],"formation":
//!   {"kind":"Line","spacing_mm":2000}}`. The player is the acting unit's
//!   `Owner`; the handler builds the recorded `IssueOrder` and validates it
//!   now (ownership, queue limits, executors), so a refusal comes back with
//!   the reason. Works in `Direct` and `Advisor` mode (orders are commands);
//!   orders to other owned units and squads (group moves get formation
//!   slots) are allowed;
//! - affordance/handler [`EXECUTE_ORDERS`] (`"execute_orders"`, `Direct`
//!   only): a `Direct` unit's tree is suspended, so a unit with a `Brain`
//!   runs its own orders only when the agent queues the `ExecuteOrder` leaf
//!   (`why_not` without orders or outside `Direct` mode; `Advisor` units
//!   run them through their own tree's `has_unit_order` branch, brainless
//!   units always). In an atomic multi-step act it is accepted after an
//!   `order` step for this unit (`AgentCtx::planned`);
//! - affordance/handler [`INTERACT`] (`"interact"`, args `{"verb": V}` and
//!   the target, or `{"affordance": "pick_up@26"}`): walk into the verb's
//!   reach and do it, in **one** act. It is an `Interact` order for this
//!   unit (args `queue`: shift-queue it behind the unit's orders) plus, for
//!   a `Direct` unit, `execute_orders` (an atomic
//!   [`ActionPlan::Sequence`]); the outcome is `OrderDone {order:
//!   "Interact"}` or `OrderFailed {reason}`;
//! - `agent/list_units` field `owner` (the owning player's id) and busy
//!   predicate `npc_command` (the unit holds orders).
//!
//! An order that makes a unit do a verb is refused (reason `not_allowed`)
//! when a verb gate (`npc_agent_io::verb_gate`) refuses that verb to the
//! ordered unit or, for an order to any other unit, to the acting unit
//! itself, so `order` and `interact` cannot get round what an actor may do.
//! The verb of an order ([`order_verb`]): `Interact` its verb; with the
//! `vehicles` feature, `Mount` `mount`, `Unload` `unload`, and a movement
//! order (`Move`, `AttackMove`, `Patrol`, `Follow`) `drive_to` for a vehicle
//! or a unit in a driver seat (`ByCrew` or `Direct`).

#![deny(
    clippy::float_arithmetic,
    clippy::float_cmp,
    clippy::iter_over_hash_type,
    clippy::disallowed_types,
    clippy::disallowed_methods
)]

use bevy_app::{App, Plugin};
use bevy_ecs::prelude::*;
use npc_agent_io::observe::metres_pos;
use npc_agent_io::{
    ActionPlan, ActionRequest, Affordance, AgentCtx, AgentIoAppExt, ControlMode, Rejection,
    Section, Step, verb_gate,
};
use npc_ai::{Brain, ParamValue};
use npc_command::{
    Commandable, EXECUTE_ORDER_ACTION, Formation, IssueOrder, MemberOf, OrderQueue, Owner,
    PlayerId, UnitOrder,
};
use npc_kernel::{ContentCommand, SimCommand, SimId, SimIndex};
use serde_json::{Value, json};

/// Name of the section.
pub const COMMAND_SECTION: &str = "command";
/// The order verb.
pub const ORDER: &str = "order";
/// The verb that runs a `Direct` unit's own orders.
pub const EXECUTE_ORDERS: &str = "execute_orders";
/// The walk-and-interact convenience verb.
pub const INTERACT: &str = "interact";
/// Most owned units the section lists.
pub const MAX_LISTED_UNITS: usize = 64;

const ORDER_SCHEMA: &str = "{\"type\":\"Move\",\"pos\":[x,z]} | {\"type\":\"AttackMove\",\"pos\":[x,z]} | \
     {\"type\":\"Stop\"} | {\"type\":\"Hold\"} | {\"type\":\"Follow\",\"target\":ID} | \
     {\"type\":\"Patrol\",\"points\":[[x,z],..]} | {\"type\":\"Interact\",\"verb\":V,\"target\":ID} | \
     {\"type\":\"Mount\",\"vehicle\":ID} | {\"type\":\"Unload\"}";

/// The verb `order` makes `unit` do, for the verb gates (see the module
/// docs): `Interact` its verb; with vehicles, `Mount` `mount`, `Unload`
/// `unload`, and a movement order `drive_to` for a vehicle or a unit in a
/// driver seat. `None`: no gated verb (walking, stopping, holding).
pub fn order_verb<'a>(world: &World, order: &'a UnitOrder, unit: Entity) -> Option<&'a str> {
    match order {
        UnitOrder::Interact { verb, .. } => Some(verb.as_str()),
        #[cfg(feature = "vehicles")]
        UnitOrder::Mount { .. } => Some(npc_interaction::builtin::MOUNT),
        #[cfg(feature = "vehicles")]
        UnitOrder::Unload => Some(super::vehicles_agent::UNLOAD),
        #[cfg(feature = "vehicles")]
        UnitOrder::Move { .. }
        | UnitOrder::AttackMove { .. }
        | UnitOrder::Patrol { .. }
        | UnitOrder::Follow { .. }
            if drives(world, unit) =>
        {
            Some(super::vehicles_agent::DRIVE_TO)
        }
        _ => {
            let _ = (world, unit);
            None
        }
    }
}

/// Whether a movement order to `unit` drives a vehicle: it is a vehicle, or
/// it sits in a driver seat (`ByCrew` or `Direct`).
#[cfg(feature = "vehicles")]
fn drives(world: &World, unit: Entity) -> bool {
    use npc_vehicles::{SeatRole, Vehicle};
    if world.get::<Vehicle>(unit).is_some() {
        return true;
    }
    let Some(m) = npc_vehicles::mounted_in(world, unit) else {
        return false;
    };
    world
        .get_resource::<SimIndex>()
        .and_then(|i| i.get(m.vehicle))
        .and_then(|v| world.get::<Vehicle>(v))
        .and_then(|v| v.seats.get(usize::from(m.seat)))
        .is_some_and(|s| s.role == SeatRole::Driver)
}

/// A short description of an order; positions in metres like observations
/// (`"move to (0.00 m, -0.03 m)"`: `pos` is integer millimetres).
pub fn describe_order(order: &UnitOrder) -> String {
    let at = |(x, z): (i32, i32)| metres_pos(i64::from(x), i64::from(z));
    match order {
        UnitOrder::Move { pos } => format!("move to {}", at(*pos)),
        UnitOrder::AttackMove { pos } => format!("attack-move to {}", at(*pos)),
        UnitOrder::Stop => "stop".into(),
        UnitOrder::Hold => "hold position".into(),
        UnitOrder::Follow { target } => format!("follow #{}", target.0),
        UnitOrder::Patrol { points } => format!("patrol {} points", points.len()),
        UnitOrder::Interact { verb, target } => format!("{} #{}", verb.replace('_', " "), target.0),
        UnitOrder::Mount { vehicle } => format!("mount #{}", vehicle.0),
        UnitOrder::Unload => "unload".into(),
        UnitOrder::Custom { name, .. } => name.replace('_', " "),
    }
}

fn owned_units(world: &World, player: PlayerId) -> Vec<SimId> {
    let Some(mut q) = world.try_query_filtered::<(&SimId, &Owner), With<Commandable>>() else {
        return Vec::new();
    };
    let mut v: Vec<SimId> = q
        .iter(world)
        .filter(|(_, o)| o.0 == player)
        .map(|(&id, _)| id)
        .collect();
    v.sort_unstable();
    v
}

/// Section `command` (see the module docs).
pub fn command_section(world: &World, ctx: &AgentCtx) -> Option<Section> {
    let owner = world.get::<Owner>(ctx.entity)?.0;
    let queue = world.get::<OrderQueue>(ctx.entity);
    let current = queue.and_then(|q| q.current.as_ref());
    let queued: Vec<&UnitOrder> =
        queue.map_or_else(Vec::new, |q| q.queued.iter().map(|o| &o.order).collect());
    let mut owned = owned_units(world, owner);
    let total = owned.len();
    owned.truncate(MAX_LISTED_UNITS);
    let value = json!({
        "player": owner.0,
        "commandable": world.get::<Commandable>(ctx.entity).is_some(),
        "current": current.map(|a| json!({
            "order": a.order.order, "started": a.started, "progress": a.progress,
        })),
        "queued": queued,
        "last_ok": queue.map(|q| q.last_ok),
        "squad": world.get::<MemberOf>(ctx.entity).map(|m| m.0),
        "owned_units": owned,
        "owned_total": total,
    });
    let mut section = Section::new(value).line(format!(
        "You command for player {}: {} unit(s).",
        owner.0, total
    ));
    match current {
        Some(a) => {
            let mut line = format!("Current order: {}", describe_order(&a.order.order));
            if !queued.is_empty() {
                let next: Vec<String> = queued.iter().map(|o| describe_order(o)).collect();
                line.push_str(&format!("; then {}", next.join(", then ")));
            }
            section = section.line(format!("{line}."));
        }
        None if queue.is_some() => section = section.line("No orders."),
        None => {}
    }
    Some(section)
}

/// Affordance provider `npc_command.orders`.
pub fn command_affordances(world: &World, ctx: &AgentCtx) -> Vec<Affordance> {
    let Some(owner) = world.get::<Owner>(ctx.entity).map(|o| o.0) else {
        return Vec::new();
    };
    let mode = ctx.mode();
    let order = Affordance::new(
        ORDER,
        format!("Give an RTS order to your units (player {})", owner.0),
    )
    .arg("order", ORDER_SCHEMA)
    .arg(
        "units",
        "[SimId] of units or squads you own (default: this unit)",
    )
    .arg(
        "queue",
        "true: shift-queue behind their orders (default false)",
    )
    .arg(
        "formation",
        "{\"kind\":\"Line|Box|Wedge\",\"spacing_mm\":N} for group moves (optional)",
    );
    let order = if mode == ControlMode::Observer {
        order.why_not("observers cannot act")
    } else {
        order
    };
    let interact = Affordance::new(
        INTERACT,
        "Walk into reach of a target and do a verb to it, in one act (an Interact order you carry out)",
    )
    .arg("verb", "the interaction verb (pick_up, open, unlock, use_tool_on, ..)")
    .arg(
        "target",
        "SimId of the target (or give args.affordance: \"pick_up@26\")",
    )
    .arg(
        "queue",
        "true: shift-queue it behind your other orders (default false)",
    );
    let interact = if mode == ControlMode::Observer {
        interact.why_not("observers cannot act")
    } else {
        interact
    };
    let mut out = vec![order, interact];
    let queue = world.get::<OrderQueue>(ctx.entity);
    if world.get::<Brain>(ctx.entity).is_some() && queue.is_some() {
        let a = Affordance::new(EXECUTE_ORDERS, "Carry out your own orders");
        let a = match mode {
            ControlMode::Direct if queue.is_some_and(OrderQueue::has_order) => a,
            ControlMode::Direct => a.why_not("no orders"),
            ControlMode::Advisor => {
                a.why_not("Advisor units carry out orders through their own tree")
            }
            ControlMode::Observer => a.why_not("observers cannot act"),
        };
        out.push(a);
    }
    out
}

fn bad(msg: impl Into<String>) -> Rejection {
    Rejection::new("invalid_args", msg)
}

/// Parses and validates an `order` request into the recorded command.
pub fn order_command(
    world: &World,
    ctx: &AgentCtx,
    req: &ActionRequest,
) -> Result<IssueOrder, Rejection> {
    let player = world
        .get::<Owner>(ctx.entity)
        .map(|o| o.0)
        .ok_or_else(|| Rejection::new("unavailable", "this unit is not owned by a player"))?;
    let raw = req
        .args
        .get("order")
        .cloned()
        .ok_or_else(|| bad(format!("args.order is required: {ORDER_SCHEMA}")))?;
    let order: UnitOrder = serde_json::from_value(raw)
        .map_err(|e| bad(format!("bad order ({e}); expected {ORDER_SCHEMA}")))?;
    let units = match req.args.get("units") {
        None | Some(Value::Null) => vec![ctx.unit],
        Some(v) => serde_json::from_value::<Vec<SimId>>(v.clone())
            .map_err(|e| bad(format!("units must be a list of SimIds ({e})")))?,
    };
    let queue = match req.args.get("queue") {
        None | Some(Value::Null) => false,
        Some(v) => v
            .as_bool()
            .ok_or_else(|| bad("queue must be true or false"))?,
    };
    let formation = match req.args.get("formation") {
        None | Some(Value::Null) => None,
        Some(v) => Some(
            serde_json::from_value::<Formation>(v.clone())
                .map_err(|e| bad(format!("bad formation ({e})")))?,
        ),
    };
    // Each ordered unit's verb, asked of the unit and (when it is another
    // unit) of the actor giving the order.
    let index = world.resource::<SimIndex>();
    for e in units.iter().filter_map(|&u| index.get(u)) {
        let Some(verb) = order_verb(world, &order, e) else {
            continue;
        };
        let asked = if e == ctx.entity {
            vec![e]
        } else {
            vec![e, ctx.entity]
        };
        if let Some(reason) = asked.into_iter().find_map(|a| verb_gate(world, a, verb)) {
            return Err(Rejection::new("not_allowed", reason));
        }
    }
    let mut cmd = IssueOrder::new(player, units, order).queued(queue);
    cmd.formation = formation;
    cmd.validate(world)
        .map_err(|e| Rejection::new("unavailable", e.to_string()))?;
    Ok(cmd)
}

/// Handler of `order` (see the module docs).
pub fn order_handler(
    world: &World,
    ctx: &AgentCtx,
    req: &ActionRequest,
) -> Result<ActionPlan, Rejection> {
    let cmd = order_command(world, ctx, req)?;
    let who = if cmd.units == [ctx.unit] {
        "yourself".to_owned()
    } else {
        let ids: Vec<String> = cmd.units.iter().map(|u| format!("#{}", u.0)).collect();
        ids.join(", ")
    };
    Ok(ActionPlan::Commands {
        label: format!(
            "Order {who}: {}{}",
            describe_order(&cmd.order),
            if cmd.queue { " (queued)" } else { "" }
        ),
        commands: vec![cmd.to_command()],
    })
}

/// Whether an earlier step of the same atomic act orders `unit`.
fn ordered_in_this_act(ctx: &AgentCtx) -> bool {
    ctx.planned.iter().any(|c| {
        matches!(c, SimCommand::Content { kind, payload }
        if kind == IssueOrder::NAME
            && payload.get("units").and_then(Value::as_array).is_some_and(|u| {
                u.iter().any(|x| x.as_u64() == Some(u64::from(ctx.unit.0)))
            }))
    })
}

fn execute_plan() -> ActionPlan {
    ActionPlan::Queue {
        label: "Carry out your orders".into(),
        steps: vec![Step::Action {
            action: EXECUTE_ORDER_ACTION.to_owned(),
            param: ParamValue::None,
        }],
        est_ticks: None,
    }
}

/// Handler of `execute_orders` (`Direct` only).
pub fn execute_orders_handler(
    world: &World,
    ctx: &AgentCtx,
    _: &ActionRequest,
) -> Result<ActionPlan, Rejection> {
    if !world
        .get::<OrderQueue>(ctx.entity)
        .is_some_and(OrderQueue::has_order)
        && !ordered_in_this_act(ctx)
    {
        return Err(Rejection::new("unavailable", "no orders"));
    }
    Ok(execute_plan())
}

/// Handler of `interact` (see the module docs).
pub fn interact_handler(
    world: &World,
    ctx: &AgentCtx,
    req: &ActionRequest,
) -> Result<ActionPlan, Rejection> {
    let (verb, target) = match req.opt_str("affordance")? {
        Some(id) => {
            let (v, t) = id
                .rsplit_once('@')
                .ok_or_else(|| bad(format!("affordance `{id}` is not `verb@target`")))?;
            let t: u32 = t
                .parse()
                .map_err(|_| bad(format!("affordance `{id}` has no numeric target")))?;
            (v.to_owned(), SimId(t))
        }
        None => {
            let verb = req
                .opt_str("verb")?
                .ok_or_else(|| bad("interact needs args.verb (or args.affordance)"))?
                .to_owned();
            let target = match (req.target, req.opt_int("target")?) {
                (Some(t), _) => t,
                (None, Some(t)) => {
                    SimId(u32::try_from(t).map_err(|_| bad(format!("target {t} is not a SimId")))?)
                }
                (None, None) => return Err(bad("interact needs a target")),
            };
            (verb, target)
        }
    };
    let queue = match req.args.get("queue") {
        None | Some(Value::Null) => false,
        Some(v) => v
            .as_bool()
            .ok_or_else(|| bad("queue must be true or false"))?,
    };
    let order_req = ActionRequest {
        verb: ORDER.to_owned(),
        target: None,
        pos: None,
        args: json!({
            "order": {"type": "Interact", "verb": verb, "target": target.0},
            "queue": queue,
        }),
    };
    let cmd = order_command(world, ctx, &order_req)?;
    let label = format!("Walk to and {}", describe_order(&cmd.order));
    let order = ActionPlan::Commands {
        label: format!("Order yourself: {}", describe_order(&cmd.order)),
        commands: vec![cmd.to_command()],
    };
    let can_execute =
        world.get::<Brain>(ctx.entity).is_some() && world.get::<OrderQueue>(ctx.entity).is_some();
    Ok(if ctx.mode() == ControlMode::Direct && can_execute {
        ActionPlan::Sequence {
            label,
            plans: vec![order, execute_plan()],
        }
    } else {
        order
    })
}

/// `agent/list_units` field `owner`: the owning player's id.
pub fn owner_info(world: &World, e: Entity) -> Option<Value> {
    world
        .get::<Owner>(e)
        .map(|o| json!({"owner": o.0.0, "commandable": world.get::<Commandable>(e).is_some()}))
}

/// Busy predicate `npc_command`: the unit holds orders.
pub fn orders_busy(world: &World, e: Entity) -> Option<String> {
    let q = world.get::<OrderQueue>(e)?;
    if !q.has_order() {
        return None;
    }
    Some(match &q.current {
        Some(a) => format!("order: {}", describe_order(&a.order.order)),
        None => "queued orders".to_owned(),
    })
}

/// Command × agent-io glue (see the module docs). Add it with
/// `CommandPlugin` and `AgentIoPlugin`.
pub struct CommandAgentPlugin;

impl Plugin for CommandAgentPlugin {
    fn build(&self, app: &mut App) {
        app.register_observation(COMMAND_SECTION, command_section)
            .register_affordances("npc_command.orders", command_affordances)
            .register_action_handler(ORDER, order_handler)
            .register_action_handler(EXECUTE_ORDERS, execute_orders_handler)
            .register_action_handler(INTERACT, interact_handler)
            .register_unit_info("npc_command", owner_info)
            .register_busy("npc_command", orders_busy);
    }
}
