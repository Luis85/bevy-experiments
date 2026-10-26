//! Command × interaction (features `command` + `interaction`): the RTS
//! `Interact { verb, target }` order, and right-click context orders.
//!
//! [`CommandInteractionPlugin`] registers:
//!
//! - order executor `npc_interaction.interact` for kind `Interact`: the unit
//!   walks into the verb's reach through the locomotion seam (`MoveIntent`
//!   at its `Commandable` speed, so a `NavAgent` path-finds), then starts
//!   the verb with `npc_interaction::begin` (`Source::Command`, journaled
//!   like every interaction) and waits for it to end. The order is
//!   `OrderDone` when the interaction succeeds and `OrderFailed { reason }`
//!   with the interaction's own reason otherwise (`"door is locked"`,
//!   `"Lockpicking check failed"`, `"too far: .."`). Checks that do not
//!   depend on where the unit stands are made before it walks, so a hopeless
//!   order fails at once; a unit busy with another interaction (an auto-opened
//!   door, a craft) waits. Validation when the order is issued is only
//!   structural (known verb, `Interactor`, the target offers the verb), so a
//!   shift-queue such as *pick up the key → unlock → open* is accepted while
//!   the door is still locked;
//! - the finished hook `npc_command.interact_order` (an
//!   `npc_interaction::InteractionListeners` hook) that reports the outcome
//!   to the order, and the hashed per-unit state [`InteractOrder`];
//! - with feature `view-helpers` too: the `command_ui` context-order
//!   resolvers `npc_interaction.item` (an item lying in the world →
//!   `pick_up`) and `npc_interaction.door` (a closed door → `open`, a
//!   locked one → `unlock`). The resolver functions are public
//!   ([`context_item_order`], [`context_door_order`]) for hosts with their
//!   own UI.
//!
//! [`interaction_order_step`] is the shared engine (also the `Mount` order of
//! the command × vehicles glue); its walk into reach is
//! `npc_command::approach_step` (the one approach engine, also open to game
//! executors). An `Interact` verb with its own executor
//! (`CommandAppExt::register_interact_verb_executor`) bypasses this one. All
//! sim code: integers only, one unit at a time in the command engine's
//! `SimId` order.

#![deny(
    clippy::float_arithmetic,
    clippy::float_cmp,
    clippy::iter_over_hash_type,
    clippy::disallowed_types,
    clippy::disallowed_methods
)]

use bevy_app::{App, Plugin};
use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use npc_command::UnitOrder;
use npc_command::{
    ApproachSpec, ApproachStep, CommandAppExt, OrderExecutor, OrderStep, PlayerId, approach_step,
};
use npc_kernel::hash::{Fnv64, SimHash};
use npc_kernel::registry::KernelAppExt;
use npc_kernel::{SimId, SimPos};
use npc_locomotion::clear_move_intent;
use serde::{Deserialize, Serialize};

use npc_interaction::builtin::{OPEN, PICK_UP, UNLOCK};
use npc_interaction::items::{entity_of, position_of};
use npc_interaction::run::{busy_with, check_def, ctx_for, reach_of};
use npc_interaction::{
    CarriedBy, Door, Interactable, Interacting, InteractionAppExt, InteractionRegistry, Interactor,
    Item, Source, begin, cancel,
};

/// Name of the `Interact` order executor.
pub const INTERACT_EXECUTOR: &str = "npc_interaction.interact";
/// Name of the finished hook that ends `Interact` (and `Mount`) orders.
pub const ORDER_FINISHED_HOOK: &str = "npc_command.interact_order";

/// Where an interaction order stands.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
pub enum InteractPhase {
    /// Walking into reach (or about to start the verb).
    Approach,
    /// The verb runs (`Interacting` on the unit).
    Running,
    /// The verb ended (reported by the finished hook).
    Finished { ok: bool, reason: Option<String> },
}

/// The running interaction order of a unit (`Interact`, `Mount`): which
/// verb on which target, and its phase. Present only while such an order
/// runs. Hashed as `interact_order`.
#[derive(Component, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
#[component(storage = "SparseSet")]
#[reflect(Component)]
pub struct InteractOrder {
    pub verb: String,
    pub target: SimId,
    pub phase: InteractPhase,
}

impl SimHash for InteractOrder {
    const NAME: &'static str = "interact_order";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.str(&self.verb);
        h.u32(self.target.0);
        match &self.phase {
            InteractPhase::Approach => h.u8(0),
            InteractPhase::Running => h.u8(1),
            InteractPhase::Finished { ok, reason } => {
                h.u8(2);
                h.u8(u8::from(*ok));
                h.opt(reason.as_ref(), |h, r| h.str(r));
            }
        }
    }
}

/// Starts the verb (after the checks): `npc_interaction::begin` for
/// `Interact`, `npc_vehicles::mount` for `Mount`.
pub type BeginFn = fn(&mut World, Entity, &str, SimId) -> Result<(), String>;
/// The checks that do not depend on where the unit stands (`Err`: the
/// order fails before walking).
pub type PrecheckFn = fn(&World, Entity, &str, SimId) -> Result<(), String>;

/// One interaction order, as [`interaction_order_step`] runs it.
#[derive(Clone, Copy)]
pub struct InteractionOrderSpec<'a> {
    pub verb: &'a str,
    pub target: SimId,
    /// Walk until within this distance of the target (`None`: no walk,
    /// e.g. verbs on carried items).
    pub reach_mm: Option<i32>,
    pub precheck: PrecheckFn,
    pub begin: BeginFn,
}

fn begin_command(world: &mut World, unit: Entity, verb: &str, target: SimId) -> Result<(), String> {
    begin(world, unit, verb, target, None, Source::Command)
}

/// Every check of `verb` except reach, reservation and "busy".
pub fn interact_precheck(
    world: &World,
    unit: Entity,
    verb: &str,
    target: SimId,
) -> Result<(), String> {
    let mut def = *world
        .get_resource::<InteractionRegistry>()
        .and_then(|r| r.verb(verb))
        .ok_or_else(|| format!("unknown verb `{verb}`"))?;
    def.needs_reach = false;
    def.reserve_target = false;
    let ctx = ctx_for(world, unit, target, None)?;
    check_def(world, &def, &ctx, false)
}

fn set_phase(world: &mut World, unit: Entity, phase: InteractPhase) {
    if let Some(mut o) = world.get_mut::<InteractOrder>(unit) {
        o.phase = phase;
    }
}

fn finish(world: &mut World, unit: Entity, step: OrderStep) -> OrderStep {
    if let Ok(mut e) = world.get_entity_mut(unit) {
        e.remove::<InteractOrder>();
    }
    step
}

/// Starts an interaction order: records [`InteractOrder`] and takes the
/// first step.
pub fn interaction_order_start(
    world: &mut World,
    unit: Entity,
    spec: InteractionOrderSpec,
) -> OrderStep {
    world.entity_mut(unit).insert(InteractOrder {
        verb: spec.verb.to_owned(),
        target: spec.target,
        phase: InteractPhase::Approach,
    });
    interaction_order_step(world, unit, spec)
}

/// One engine pass of an interaction order (see the module docs).
pub fn interaction_order_step(
    world: &mut World,
    unit: Entity,
    spec: InteractionOrderSpec,
) -> OrderStep {
    let Some(state) = world.get::<InteractOrder>(unit).cloned() else {
        return OrderStep::Failed("order state lost".into());
    };
    match state.phase {
        InteractPhase::Finished { ok: true, .. } => finish(world, unit, OrderStep::Done),
        InteractPhase::Finished { ok: false, reason } => finish(
            world,
            unit,
            OrderStep::Failed(reason.unwrap_or_else(|| format!("{} failed", spec.verb))),
        ),
        InteractPhase::Running => {
            let running = world
                .get::<Interacting>(unit)
                .is_some_and(|i| i.verb == spec.verb && i.target == spec.target);
            if running {
                OrderStep::Running
            } else {
                finish(world, unit, OrderStep::Failed("interrupted".into()))
            }
        }
        InteractPhase::Approach => approach(world, unit, spec),
    }
}

fn approach(world: &mut World, unit: Entity, spec: InteractionOrderSpec) -> OrderStep {
    if let Err(why) = (spec.precheck)(world, unit, spec.verb, spec.target) {
        clear_move_intent(world, unit);
        return finish(world, unit, OrderStep::Failed(why));
    }
    let Some(target) = entity_of(world, spec.target) else {
        return finish(
            world,
            unit,
            OrderStep::Failed(format!("#{} is gone", spec.target.0)),
        );
    };
    let (Some(_), Some(there)) = (
        world.get::<SimPos>(unit).copied(),
        position_of(world, target),
    ) else {
        return finish(world, unit, OrderStep::Failed("no position".into()));
    };
    match spec.reach_mm {
        Some(reach) => match approach_step(world, unit, &ApproachSpec::to_point(there, reach)) {
            ApproachStep::InReach => {}
            ApproachStep::Walking => return OrderStep::Running,
            ApproachStep::Blocked => {
                return finish(world, unit, OrderStep::Failed("blocked".into()));
            }
        },
        None => clear_move_intent(world, unit),
    }
    if busy_with(world, unit).is_some() {
        // Another interaction (an auto-opened door) or a craft: wait.
        return OrderStep::Running;
    }
    match (spec.begin)(world, unit, spec.verb, spec.target) {
        Ok(()) => {
            // The hook may already have reported (a verb that completes
            // in the tick it starts never does: completions run later).
            if world
                .get::<InteractOrder>(unit)
                .is_some_and(|o| o.phase == InteractPhase::Approach)
            {
                set_phase(world, unit, InteractPhase::Running);
            }
            OrderStep::Running
        }
        Err(why) => finish(world, unit, OrderStep::Failed(why)),
    }
}

/// Cancels a running interaction order (replaced, stopped, paused): the
/// verb is cancelled, the walk stopped, the state dropped.
pub fn interaction_order_cancel(world: &mut World, unit: Entity) {
    let Some(state) = world.get::<InteractOrder>(unit).cloned() else {
        return;
    };
    match state.phase {
        InteractPhase::Approach => clear_move_intent(world, unit),
        InteractPhase::Running => {
            let ours = world.get::<Interacting>(unit).is_some_and(|i| {
                i.verb == state.verb && i.target == state.target && i.source == Source::Command
            });
            if ours {
                cancel(world, unit, "order cancelled");
            }
        }
        InteractPhase::Finished { .. } => {}
    }
    world.entity_mut(unit).remove::<InteractOrder>();
}

/// The finished hook: reports how the order's interaction ended.
pub fn on_interaction_finished(
    world: &mut World,
    actor: Entity,
    i: &Interacting,
    outcome: &str,
    reason: Option<&str>,
) {
    let mine = world.get::<InteractOrder>(actor).is_some_and(|o| {
        o.phase == InteractPhase::Running && o.verb == i.verb && o.target == i.target
    });
    if mine {
        let ok = outcome == "success";
        let reason =
            (!ok).then(|| reason.map_or_else(|| format!("{} {outcome}", i.verb), str::to_owned));
        set_phase(world, actor, InteractPhase::Finished { ok, reason });
    }
}

fn interact_parts(order: &UnitOrder) -> Option<(&str, SimId)> {
    match order {
        UnitOrder::Interact { verb, target } => Some((verb.as_str(), *target)),
        _ => None,
    }
}

fn interact_spec<'a>(world: &World, verb: &'a str, target: SimId) -> InteractionOrderSpec<'a> {
    let reach_mm = world
        .get_resource::<InteractionRegistry>()
        .and_then(|r| r.verb(verb))
        .filter(|d| d.needs_reach)
        .map(|d| reach_of(world, d));
    InteractionOrderSpec {
        verb,
        target,
        reach_mm,
        precheck: interact_precheck,
        begin: begin_command,
    }
}

fn validate_interact(world: &World, unit: Entity, order: &UnitOrder) -> Result<(), String> {
    let (verb, target) = interact_parts(order).ok_or("not an Interact order")?;
    let def = *world
        .get_resource::<InteractionRegistry>()
        .and_then(|r| r.verb(verb))
        .ok_or_else(|| format!("unknown verb `{verb}`"))?;
    if world.get::<Interactor>(unit).is_none() {
        return Err("the unit cannot interact (no Interactor)".into());
    }
    let ctx = ctx_for(world, unit, target, None)?;
    if let Some(i) = world.get::<Interactable>(ctx.target)
        && !i.offers(verb)
    {
        return Err(format!("#{} does not offer {verb}", target.0));
    }
    if !(def.offer)(world, &ctx) {
        return Err(format!("cannot {verb} #{}", target.0));
    }
    Ok(())
}

fn start_interact(world: &mut World, unit: Entity, order: &UnitOrder) -> OrderStep {
    let Some((verb, target)) = interact_parts(order) else {
        return OrderStep::Failed("not an Interact order".into());
    };
    let spec = interact_spec(world, verb, target);
    interaction_order_start(world, unit, spec)
}

fn tick_interact(world: &mut World, unit: Entity, order: &UnitOrder) -> OrderStep {
    let Some((verb, target)) = interact_parts(order) else {
        return OrderStep::Failed("not an Interact order".into());
    };
    let spec = interact_spec(world, verb, target);
    interaction_order_step(world, unit, spec)
}

fn cancel_interact(world: &mut World, unit: Entity, _: &UnitOrder) {
    interaction_order_cancel(world, unit);
}

/// The `Interact` order executor (see the module docs).
pub fn interact_executor() -> OrderExecutor {
    OrderExecutor::new(start_interact, tick_interact)
        .with_validate(validate_interact)
        .with_cancel(cancel_interact)
}

/// Right-click resolver: an item lying in the world → `pick_up`.
pub fn context_item_order(world: &World, _: PlayerId, target: Entity) -> Option<UnitOrder> {
    let id = *world.get::<SimId>(target)?;
    (world.get::<Item>(target).is_some() && world.get::<CarriedBy>(target).is_none()).then(|| {
        UnitOrder::Interact {
            verb: PICK_UP.to_owned(),
            target: id,
        }
    })
}

/// Right-click resolver: a closed door → `open`, a locked one → `unlock`.
pub fn context_door_order(world: &World, _: PlayerId, target: Entity) -> Option<UnitOrder> {
    let id = *world.get::<SimId>(target)?;
    let door = world.get::<Door>(target)?;
    if door.open {
        return None;
    }
    let verb = if door.is_locked() { UNLOCK } else { OPEN };
    Some(UnitOrder::Interact {
        verb: verb.to_owned(),
        target: id,
    })
}

/// Command × interaction glue (see the module docs). Add it with
/// `CommandPlugin` and `InteractionPlugin`.
pub struct CommandInteractionPlugin;

impl Plugin for CommandInteractionPlugin {
    fn build(&self, app: &mut App) {
        app.register_sim_component::<InteractOrder>()
            .register_order_executor("Interact", INTERACT_EXECUTOR, interact_executor())
            .add_finished_hook(ORDER_FINISHED_HOOK, on_interaction_finished);
        #[cfg(feature = "view-helpers")]
        {
            use npc_view_helpers::command_ui::CommandUiAppExt;
            app.add_context_order("npc_interaction.door", context_door_order)
                .add_context_order("npc_interaction.item", context_item_order);
        }
    }
}
