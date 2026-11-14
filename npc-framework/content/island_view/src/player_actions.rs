//! Trusted native input, using the shared scoped observations and recorded agent commands.
//! Pending approaches and scheduler holds are presentation/host state, never simulation state.
mod progress_feedback;
use bevy::prelude::*;
use island_content::village::{VillageState, Villager, Vitality};
use npc_framework::agent_io::{
    api::{self, ActParams, ActStep},
    control::{AttachController, ControlMode, DetachController, ExternalController, IdlePolicy},
    events::{self, AgentEvents},
    observe::{self, ObserveOptions},
    registry::AgentRegistry,
    scheduler::SchedulerPolicy,
};
use npc_framework::command::OrderQueue;
use npc_framework::kernel::journal::JournalKind;
use npc_framework::prelude::*;
use serde_json::{Value, json};

pub(crate) const NATIVE_CONTROLLER: &str = "native-player";
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ControlState {
    Unavailable,
    Free,
    Native,
    Foreign { controller: String },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ActorStamp {
    id: SimId,
    entity: Entity,
    epoch: u64,
    life: u32,
}
#[derive(Clone, Debug)]
pub(crate) struct PendingAction {
    pub label: String,
    actor: ActorStamp,
    action: ActStep,
    handle: u32,
    seq: u64,
    order_seen: bool,
    started: u64,
    last_progress: u64,
    last_position: SimPos,
    retargets: u8,
}
#[derive(Clone, Debug)]
struct SchedulerHold {
    actor: ActorStamp,
    submitted: u64,
}
/// The view's wish that the host's idle supervisor (ADR 0023) leave the
/// played character alone while native input drives it. The view only sets
/// this; the host owns `SchedulerPolicy` and applies the wish
/// ([`apply_scheduler_hold`] is the reference way to do it). Host data: never
/// simulation state, never saved.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SchedulerHoldRequest {
    /// True while a native character is played.
    pub hold: bool,
}
/// What the host applied for [`SchedulerHoldRequest`]: the policy it
/// replaced, and the one it set (a later writer's change is never undone).
#[derive(Resource, Clone, Debug, Default)]
pub struct SchedulerHoldApplied(Option<(SchedulerPolicy, SchedulerPolicy)>);
/// A host system: pause the idle supervisor while [`SchedulerHoldRequest`]
/// asks for it and put the earlier policy back after, unless something else
/// changed the policy in between. Run it in the host's schedule.
pub fn apply_scheduler_hold(world: &mut World) {
    let hold = world
        .get_resource::<SchedulerHoldRequest>()
        .is_some_and(|request| request.hold);
    let Some(policy) = world.get_resource::<SchedulerPolicy>().cloned() else {
        return;
    };
    let applied = world
        .get_resource_or_init::<SchedulerHoldApplied>()
        .0
        .clone();
    match (hold, applied) {
        (true, None) => {
            let mut held = policy.clone();
            held.enabled = false;
            if held != policy {
                *world.resource_mut::<SchedulerPolicy>() = held.clone();
            }
            world.resource_mut::<SchedulerHoldApplied>().0 = Some((policy, held));
        }
        (false, Some((before, held))) => {
            world.resource_mut::<SchedulerHoldApplied>().0 = None;
            if policy == held && before != held {
                *world.resource_mut::<SchedulerPolicy>() = before;
            }
        }
        _ => {}
    }
}
fn request_hold(world: &mut World, hold: bool) {
    let mut request = world.get_resource_or_init::<SchedulerHoldRequest>();
    if request.hold != hold {
        request.hold = hold;
    }
}
#[derive(Clone, Debug)]
struct RefusalWatch {
    actor: ActorStamp,
    seq: u64,
    commands: Vec<(u64, u32)>,
}
#[derive(Clone, Copy)]
struct InputMark {
    actor: ActorStamp,
    event_seq: u64,
    tick: u64,
    first_command: u32,
}
#[derive(Resource, Default)]
pub(crate) struct NativeActions {
    pub pending: Option<PendingAction>,
    pub feedback: Option<String>,
    scheduler: Option<SchedulerHold>,
    auto_attempt: Option<(ActorStamp, u64)>,
    refusal: Option<RefusalWatch>,
    progress: Option<progress_feedback::Watch>,
    sales: Option<progress_feedback::Sales>,
}
fn actor(world: &World) -> Result<ActorStamp, String> {
    let state = world
        .get_resource::<VillageState>()
        .ok_or("This island has no character to play.")?;
    let entity = world
        .get_resource::<SimIndex>()
        .and_then(|index| index.get(state.played))
        .ok_or("Your character is no longer on the island.")?;
    if world
        .get::<island_content::village::colony::AwaitingArrival>(entity)
        .is_some()
    {
        return Ok(ActorStamp {
            id: state.played,
            entity,
            epoch: api::epoch(world),
            life: state.life,
        });
    }
    let person = world
        .get::<Villager>(entity)
        .ok_or("The assigned actor is not a villager")?;
    if world
        .get::<Vitality>(entity)
        .is_none_or(|v| v.died_at.is_some() || v.health == 0)
    {
        return Err("This life has ended; choose a new life before playing".into());
    }
    Ok(ActorStamp {
        id: state.played,
        entity,
        epoch: api::epoch(world),
        life: person.life,
    })
}
pub(crate) fn available(world: &World) -> bool {
    actor(world).is_ok()
}
pub(crate) fn clear_view(world: &mut World) {
    if world.contains_resource::<NativeActions>() {
        restore_scheduler(world);
        *world.resource_mut::<NativeActions>() = NativeActions::default();
        progress_feedback::reset_sales(world);
    }
}
/// Explicit postmortem native action. It creates a new identity through the same
/// recorded domain command, never revives a corpse or steals foreign control.
pub(crate) fn begin_successor(world: &mut World) -> Result<Value, String> {
    let id = world
        .get_resource::<VillageState>()
        .ok_or("There is no island life to continue")?
        .played;
    let entity = world
        .resource::<SimIndex>()
        .get(id)
        .ok_or("The previous life is missing")?;
    if world
        .get::<ExternalController>(entity)
        .is_some_and(|controller| controller.id != NATIVE_CONTROLLER)
    {
        return Err("This life is being played from somewhere else.".into());
    }
    let command = island_content::village::VillageAct::new(id, "new_life");
    let (tick, seq) =
        submit(world, None, SimCommand::content(&command)).map_err(|error| error.to_string())?;
    Ok(json!({"accepted":true,"applies_at":tick,"command_seq":seq}))
}
pub(crate) fn control(world: &World) -> ControlState {
    let Ok(actor) = actor(world) else {
        return ControlState::Unavailable;
    };
    match world.get::<ExternalController>(actor.entity) {
        None => ControlState::Free,
        Some(controller)
            if controller.id == NATIVE_CONTROLLER && controller.mode == ControlMode::Direct =>
        {
            ControlState::Native
        }
        Some(controller) => ControlState::Foreign {
            controller: controller.id.clone(),
        },
    }
}
fn native(world: &World) -> Result<ActorStamp, String> {
    let actor = actor(world)?;
    match control(world) {
        ControlState::Native => Ok(actor),
        ControlState::Foreign { .. } => {
            Err("Your character is being played from somewhere else.".into())
        }
        _ => Err("Choose Live to play your character.".into()),
    }
}
/// Registration protects remote transports. Native input is trusted in-process input;
/// it does not forge a registration, session, authorization token or notes identity.
pub(crate) fn observe(world: &World) -> Result<Value, String> {
    let id = world
        .get_resource::<VillageState>()
        .ok_or("This island has no character to play.")?
        .played;
    observe::observe_with(
        world,
        id,
        ObserveOptions {
            recent: 32,
            brief: false,
        },
    )
    .map(|(value, _)| value)
    .ok_or_else(|| "Your character is no longer on the island.".into())
}
pub(crate) fn known_position(world: &World, target: SimId) -> Option<SimPos> {
    let id = world.get_resource::<VillageState>()?.played;
    let ctx = observe::ctx(world, id)?;
    if target == id {
        return world.get::<SimPos>(ctx.entity).copied();
    }
    if observe::known_targets(world, &ctx).is_some_and(|known| !known.contains(&target)) {
        return None;
    }
    world
        .get_resource::<AgentRegistry>()?
        .thing_positions
        .values()
        .find_map(|provider| provider(world, &ctx, target))
}
fn feedback(world: &mut World, text: impl Into<String>) {
    let text = text.into();
    if text == SWITCHING {
        // While time runs the switch is immediate: nothing to say.
        let paused = crate::game_ui::words::paused(world);
        world.get_resource_or_init::<NativeActions>().feedback =
            paused.then(|| "Switching to Live when time runs (Space).".to_owned());
        return;
    }
    let text = player_feedback(&text).into();
    world.get_resource_or_init::<NativeActions>().feedback = Some(text);
}
/// The internal note of a requested switch to native control.
const SWITCHING: &str = "Native control requested; it applies on the next simulation tick";
fn player_feedback(text: &str) -> &str {
    match text {
        "Stop requested; queued approach and work cancelled on the next tick" => "Stopping…",
        "Native control released on the next tick" => {
            "Returning to inspection. The village keeps running."
        }
        "Action submitted" => "Your activity is queued.",
        "Approach cancelled: hero, world or controller changed" => {
            "You stopped walking: something changed."
        }
        "Approach interrupted; follow-up action cancelled" => {
            "The journey was interrupted. Choose your next activity."
        }
        "Approach made no progress; stopped without performing the action" => {
            "The route is blocked. Choose a nearby destination."
        }
        _ => text,
    }
}
fn pause_scheduler(world: &mut World, actor: ActorStamp) {
    if world
        .get_resource_or_init::<NativeActions>()
        .scheduler
        .is_some()
    {
        return;
    }
    let submitted = world.resource::<SimClock>().tick;
    world.resource_mut::<NativeActions>().scheduler = Some(SchedulerHold { actor, submitted });
    request_hold(world, true);
}
fn restore_scheduler(world: &mut World) {
    if world
        .get_resource_or_init::<NativeActions>()
        .scheduler
        .take()
        .is_some()
    {
        request_hold(world, false);
    }
}
pub(crate) fn take_control(world: &mut World) -> Result<Value, String> {
    let actor = actor(world)?;
    if matches!(control(world), ControlState::Foreign { .. }) {
        return Err("Your character is being played from somewhere else.".into());
    }
    if control(world) == ControlState::Native {
        pause_scheduler(world, actor);
        return Ok(json!({"accepted":true,"already_native":true}));
    }
    if world
        .get::<island_content::village::colony::AwaitingArrival>(actor.entity)
        .is_some()
    {
        submit(
            world,
            None,
            SimCommand::content(&island_content::village::colony::JoinIsland { unit: actor.id.0 }),
        )
        .map_err(|error| error.to_string())?;
    }
    let (at, seq) = submit(
        world,
        None,
        SimCommand::content(&AttachController {
            unit: actor.id.0,
            controller: NATIVE_CONTROLLER.into(),
            mode: ControlMode::Direct,
            on_idle: IdlePolicy::Idle,
            force: false,
        }),
    )
    .map_err(|error| error.to_string())?;
    pause_scheduler(world, actor);
    feedback(world, SWITCHING);
    Ok(json!({"accepted":true,"applies_at":at,"command_seq":seq}))
}
fn shared_act(world: &mut World, actor: ActorStamp, request: ActStep) -> Result<Value, String> {
    let mark = mark_input(world, actor);
    let target = request.target.map(SimId);
    if let Some(units) = request.args.as_ref().and_then(|args| args.get("units"))
        && units.as_array().is_some_and(|units| {
            units
                .iter()
                .any(|id| id.as_u64() != Some(u64::from(actor.id.0)))
        })
    {
        return Err("Choose Live to play your character.".into());
    }
    let response = api::act(
        world,
        ActParams {
            unit: actor.id.0,
            controller: NATIVE_CONTROLLER.into(),
            affordance: request.affordance,
            verb: request.verb,
            target: request.target,
            pos: request.pos,
            args: request.args,
            expect_tick: Some(world.resource::<SimClock>().tick),
            ..default()
        },
    )
    .map_err(|error| progress_feedback::refusal(world, &error.message))?;
    if response.get("accepted").and_then(Value::as_bool) != Some(true) {
        return Err(progress_feedback::refusal(
            world,
            response
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("The shared action was refused"),
        ));
    }
    watch_input(world, mark);
    progress_feedback::track(world, actor, target, &response, mark.event_seq);
    Ok(response)
}
fn mark_input(world: &mut World, actor: ActorStamp) -> InputMark {
    use npc_framework::kernel::commands::CommandQueue;
    events::harvest(world);
    world.get_resource_or_init::<NativeActions>().refusal = None;
    let now = world.resource::<SimClock>().tick;
    let queue = world.get_resource::<CommandQueue>();
    let tick = queue.map_or(now, |queue| queue.next_open_tick(now));
    let first_command = queue
        .and_then(|queue| {
            queue
                .pending
                .range((tick, 0)..=(tick, u32::MAX))
                .next_back()
        })
        .map_or(0, |((_, seq), _)| seq.saturating_add(1));
    InputMark {
        actor,
        event_seq: events::last_seq(world),
        tick,
        first_command,
    }
}
fn watch_input(world: &mut World, mark: InputMark) {
    let commands = world
        .get_resource::<npc_framework::kernel::commands::CommandQueue>()
        .map(|queue| {
            queue
                .pending
                .range((mark.tick, mark.first_command)..=(mark.tick, u32::MAX))
                .map(|(key, _)| *key)
                .collect()
        })
        .unwrap_or_default();
    world.get_resource_or_init::<NativeActions>().refusal = Some(RefusalWatch {
        actor: mark.actor,
        seq: mark.event_seq,
        commands,
    });
}
fn apply_refusal_feedback(world: &mut World, current: Option<ActorStamp>, native_now: bool) {
    use npc_framework::kernel::commands::{CommandQueue, SkippedCommands};
    let Some(mut watch) = world.resource_mut::<NativeActions>().refusal.take() else {
        return;
    };
    if !native_now || current != Some(watch.actor) {
        return;
    }
    let message = world.get_resource::<SkippedCommands>().and_then(|skips| {
        let events = world.get_resource::<AgentEvents>()?;
        skips.entries.iter().rev().find_map(|skip| {
            if !watch.commands.contains(&(skip.tick, skip.seq))
                || skip.cmd.unit_in(world) != Some(watch.actor.id)
            {
                return None;
            }
            let reason = format!("skipped at tick {}: {}", skip.tick, skip.error);
            events.events.iter().rev().find_map(|event| {
                if event.seq <= watch.seq || event.unit != Some(watch.actor.id) {
                    return None;
                }
                match &event.event {
                    JournalKind::ActionRejected {
                        controller,
                        verb,
                        reason: event_reason,
                    } if controller == NATIVE_CONTROLLER
                        && verb == skip.cmd.kind()
                        && event_reason == &reason =>
                    {
                        Some(reason.clone())
                    }
                    _ => None,
                }
            })
        })
    });
    if let Some(reason) = message {
        let public = progress_feedback::refusal(world, &reason);
        world.resource_mut::<NativeActions>().progress = None;
        feedback(world, public);
    }
    watch.seq = events::last_seq(world);
    watch.commands.retain(|key| {
        world
            .get_resource::<CommandQueue>()
            .is_some_and(|queue| queue.pending.contains_key(key))
    });
    if !watch.commands.is_empty() {
        world.resource_mut::<NativeActions>().refusal = Some(watch);
    }
}
pub(crate) fn act(world: &mut World, request: ActStep) -> Result<Value, String> {
    let actor = native(world)?;
    world.get_resource_or_init::<NativeActions>().pending = None;
    let response = shared_act(world, actor, request)?;
    Ok(response)
}
pub(crate) fn stop(world: &mut World) -> Result<Value, String> {
    let actor = native(world)?;
    world.get_resource_or_init::<NativeActions>().pending = None;
    let response = shared_act(
        world,
        actor,
        ActStep {
            verb: Some("stop_work".into()),
            ..default()
        },
    )?;
    feedback(
        world,
        "Stop requested; queued approach and work cancelled on the next tick",
    );
    Ok(response)
}
/// A menu suspends navigation intent without abandoning committed physical work.
/// The caller owns pause/save timing and waits for the returned recorded receipt.
pub(crate) fn committed_work(world: &World) -> bool {
    let Ok(actor) = native(world) else {
        return false;
    };
    if matches!(
        island_content::village::activity(world, actor.entity).0,
        "building" | "crafting" | "gathering" | "hunting" | "resting" | "restoring"
    ) {
        return true;
    }
    if world
        .get::<npc_framework::agent_io::control::ExternalQueue>(actor.entity)
        .is_some_and(|queue| {
            queue.current.as_ref().is_some_and(|action| {
                action.item.verb.starts_with("craft:")
                    || island_content::village::frontier::activities::VERBS
                        .contains(&action.item.verb.as_str())
            }) || queue.pending.iter().any(|action| {
                action.verb.starts_with("craft:")
                    || island_content::village::frontier::activities::VERBS
                        .contains(&action.verb.as_str())
            })
        })
    {
        return true;
    }
    // A button can commit a work request before its application tick. Treat
    // those actor-owned requests like running work during a mode transition.
    world
        .get_resource::<npc_framework::kernel::commands::CommandQueue>()
        .is_some_and(|queue| {
            queue.pending.values().any(|command| {
                if command.unit_in(world) != Some(actor.id) {
                    return false;
                }
                let SimCommand::Content { kind, payload } = command else {
                    return false;
                };
                let verb = payload.get("verb").and_then(Value::as_str).unwrap_or("");
                match kind.as_str() {
                    "Craft" => true,
                    "VillageAct" => matches!(
                        verb,
                        "gather" | "hunt" | "rest" | "rest_home" | "rest_ground"
                    ),
                    "HomesteadAct" => verb == "build_cottage",
                    "FrontierAct" => {
                        island_content::village::frontier::activities::VERBS.contains(&verb)
                    }
                    "AgentAct" => {
                        payload.get("controller").and_then(Value::as_str) == Some(NATIVE_CONTROLLER)
                            && (verb.starts_with("craft:")
                                || island_content::village::frontier::activities::VERBS
                                    .contains(&verb))
                    }
                    _ => false,
                }
            })
        })
}

pub(crate) fn prepare_suspend(world: &mut World) -> Result<Option<(u64, u32)>, String> {
    let held = crate::island_control::clear_navigation_input(world);
    let approach = world
        .get_resource_or_init::<NativeActions>()
        .pending
        .take()
        .is_some();
    if let Some(receipt) = crate::island_control::pending_receipt(world) {
        return Ok(Some(receipt));
    }
    if control(world) != ControlState::Native {
        return Ok(None);
    }
    let actor = native(world)?;
    let activity = island_content::village::activity(world, actor.entity).0;
    // Activity is the domain-owned projection. Committed work owns its materials,
    // reservations and completion; a host menu is not a cancellation command.
    if committed_work(world) {
        return Ok(None);
    }
    let ordered = world
        .get::<OrderQueue>(actor.entity)
        .is_some_and(OrderQueue::has_order);
    let queued_navigation = world
        .get_resource::<npc_framework::kernel::commands::CommandQueue>()
        .is_some_and(|queue| {
            queue.pending.values().any(|command| {
                let SimCommand::Content { kind, payload } = command else {
                    return false;
                };
                match kind.as_str() {
                    "IssueOrder" => payload
                        .to_typed::<npc_framework::command::IssueOrder>()
                        .ok()
                        .is_some_and(|order| {
                            order.units.contains(&actor.id)
                                && matches!(
                                    order.order,
                                    npc_framework::command::UnitOrder::Move { .. }
                                )
                        }),
                    "AgentAct" => payload
                        .to_typed::<npc_framework::agent_io::control::AgentAct>()
                        .ok()
                        .is_some_and(|action| {
                            action.unit == actor.id.0
                                && action.controller == NATIVE_CONTROLLER
                                && action.verb == "execute_orders"
                        }),
                    _ => false,
                }
            })
        });
    if !held
        && !approach
        && !ordered
        && !queued_navigation
        && !matches!(activity, "walking" | "blocked")
    {
        return Ok(None);
    }
    stop(world)?;
    world
        .resource::<NativeActions>()
        .refusal
        .as_ref()
        .and_then(|watch| watch.commands.last().copied())
        .map(Some)
        .ok_or_else(|| "The navigation stop did not produce a command receipt".into())
}
pub(crate) fn release(world: &mut World) -> Result<Value, String> {
    let actor = native(world)?;
    stop(world)?;
    let (at, seq) = submit(
        world,
        None,
        SimCommand::content(&DetachController {
            unit: actor.id.0,
            controller: NATIVE_CONTROLLER.into(),
        }),
    )
    .map_err(|error| error.to_string())?;
    feedback(world, "Native control released on the next tick");
    Ok(json!({"accepted":true,"applies_at":at,"command_seq":seq}))
}

pub(crate) fn refusal_message(world: &World, reason: &str) -> String {
    progress_feedback::refusal(world, reason)
}

pub(crate) fn walk(world: &mut World, pos: SimPos) -> Result<Value, String> {
    let actor = native(world)?;
    stop(world)?;
    let mark = mark_input(world, actor);
    let response = api::act(
        world,
        ActParams {
            unit: actor.id.0,
            controller: NATIVE_CONTROLLER.into(),
            steps: Some(vec![
                ActStep {
                    verb: Some("order".into()),
                    args: Some(json!({"order":{"type":"Move","pos":[pos.x,pos.z]}})),
                    ..default()
                },
                ActStep {
                    verb: Some("execute_orders".into()),
                    ..default()
                },
            ]),
            ..default()
        },
    )
    .map_err(|error| progress_feedback::refusal(world, &error.message))?;
    if response.get("accepted").and_then(Value::as_bool) != Some(true) {
        return Err(response
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("The destination was refused")
            .into());
    }
    watch_input(world, mark);
    progress_feedback::track(world, actor, None, &response, mark.event_seq);
    Ok(response)
}
pub(crate) fn walk_then_act(
    world: &mut World,
    pos: SimPos,
    request: ActStep,
) -> Result<Value, String> {
    let actor = native(world)?;
    events::harvest(world);
    let seq = events::last_seq(world);
    let response = walk(world, pos)?;
    let handle = response
        .get("handle")
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .ok_or("Shared movement supplied no completion handle")?;
    let label = request
        .verb
        .clone()
        .or_else(|| request.affordance.clone())
        .unwrap_or_else(|| "chosen action".into());
    let now = world.resource::<SimClock>().tick;
    let last_position = *world
        .get::<SimPos>(actor.entity)
        .ok_or("The hero has no position")?;
    world.resource_mut::<NativeActions>().pending = Some(PendingAction {
        label: format!("Approaching: {label}"),
        actor,
        action: request,
        handle,
        seq,
        order_seen: false,
        started: now,
        last_progress: now,
        last_position,
        retargets: 0,
    });
    Ok(response)
}

/// A person may move while we approach. Retry only a currently sensed actor's
/// observed position, with a bounded number of ordinary recorded walks.
fn retarget_person(world: &mut World, pending: &PendingAction) -> bool {
    const MAX_RETARGETS: u8 = 3;
    if !matches!(
        pending.action.verb.as_deref(),
        Some("talk" | "buy" | "sell")
    ) {
        return false;
    }
    let Some(target) = pending.action.target else {
        return false;
    };
    let position = observe(world).ok().and_then(|observation| {
        observation
            .pointer("/sections/perception/sensed")?
            .as_array()?
            .iter()
            .find(|person| {
                person["id"].as_u64() == Some(u64::from(target)) && person["kind"] == "actor"
            })
            .and_then(|person| {
                let pos = person["pos"].as_array()?;
                Some(SimPos::new(
                    i32::try_from(pos.first()?.as_i64()?).ok()?,
                    i32::try_from(pos.get(1)?.as_i64()?).ok()?,
                ))
            })
    });
    let Some(position) = position else {
        feedback(
            world,
            "Your neighbour is out of sight. Find them again before you talk or buy and sell again.",
        );
        return true;
    };
    if world
        .get::<SimPos>(pending.actor.entity)
        .is_some_and(|here| here.dist2(position) <= 2_250_000)
    {
        return false; // A price, inventory or other refusal must keep its real reason.
    }
    if pending.retargets >= MAX_RETARGETS
        || world
            .resource::<SimClock>()
            .tick
            .saturating_sub(pending.started)
            > 24_000
    {
        feedback(
            world,
            "Your neighbour keeps moving. Meet them at a workplace or try again when they stop.",
        );
        return true;
    }
    match walk_then_act(world, position, pending.action.clone()) {
        Ok(_) => {
            if let Some(next) = world.resource_mut::<NativeActions>().pending.as_mut() {
                next.started = pending.started;
                next.retargets = pending.retargets + 1;
                next.label = pending.label.clone();
            }
            feedback(world, "Your neighbour moved. Walking closer to continue.");
        }
        Err(reason) => feedback(
            world,
            format!("Could not approach your neighbour: {reason}"),
        ),
    }
    true
}

/// Call between simulation ticks (Update), before the native panel. No simulation tick is run here.
pub(crate) fn tick(world: &mut World) {
    if world
        .get_resource::<crate::FrontendGate>()
        .is_some_and(|gate| gate.0)
    {
        return;
    }
    world.init_resource::<NativeActions>();
    // Native observation is deliberately read-only; keep its shared event buffer
    // fresh here even when no approach or remote tool call is in progress.
    events::harvest(world);
    let current = actor(world).ok();
    let native_now = control(world) == ControlState::Native;
    apply_refusal_feedback(world, current, native_now);
    progress_feedback::update(world, current, native_now);
    let now = world
        .get_resource::<SimClock>()
        .map_or(0, |clock| clock.tick);
    let lost = world
        .resource::<NativeActions>()
        .scheduler
        .as_ref()
        .is_some_and(|hold| current != Some(hold.actor) || (!native_now && now > hold.submitted));
    if lost {
        restore_scheduler(world);
    }
    if native_now && let Some(actor) = current {
        pause_scheduler(world, actor);
    }
    let pending = world.resource_mut::<NativeActions>().pending.take();
    if let Some(mut pending) = pending {
        if !native_now || current != Some(pending.actor) || now < pending.started {
            feedback(
                world,
                "Approach cancelled: hero, world or controller changed",
            );
            return;
        }
        events::harvest(world);
        let mut terminal = None;
        let mut interrupted = false;
        if let Some(events) = world.get_resource::<AgentEvents>() {
            if events.dropped > 0 && events.oldest_seq() > pending.seq.saturating_add(1) {
                interrupted = true;
            }
            for event in events
                .events
                .iter()
                .filter(|event| event.seq > pending.seq && event.unit == Some(pending.actor.id))
            {
                match &event.event {
                    JournalKind::ExternalAction { handle, status, .. }
                        if *handle == pending.handle
                            && matches!(
                                status.as_str(),
                                "done" | "failed" | "cancelled" | "rejected"
                            ) =>
                    {
                        terminal = Some(status.clone())
                    }
                    JournalKind::OrderIssued { .. } if pending.order_seen => interrupted = true,
                    JournalKind::OrderIssued { .. } => pending.order_seen = true,
                    JournalKind::ControllerDetached { .. } => interrupted = true,
                    _ => {}
                }
            }
            pending.seq = events.last_seq();
        }
        if interrupted {
            feedback(world, "Approach interrupted; follow-up action cancelled");
            return;
        }
        if let Some(status) = terminal {
            if status == "done" {
                let result = shared_act(world, pending.actor, pending.action.clone());
                if result.is_err() && retarget_person(world, &pending) {
                    return;
                }
                match result {
                    Err(reason) => feedback(world, format!("At destination: {reason}")),
                    _ => {}
                }
            } else {
                feedback(
                    world,
                    format!("Approach {status}; follow-up action cancelled"),
                );
            }
            return;
        }
        if let Some(position) = world.get::<SimPos>(pending.actor.entity).copied()
            && position.dist2(pending.last_position) >= 10_000
        {
            pending.last_position = position;
            pending.last_progress = now;
        }
        if now.saturating_sub(pending.last_progress) > 400
            || now.saturating_sub(pending.started) > 24_000
        {
            let _ = stop(world);
            feedback(
                world,
                "Approach made no progress; stopped without performing the action",
            );
            return;
        }
        world.resource_mut::<NativeActions>().pending = Some(pending);
        return;
    }
    // The incumbent RTS UI records IssueOrder but a Direct hero's own tree is suspended.
    // Start its executor only when no other registered activity/command owns the actor.
    let Some(actor) = current.filter(|_| native_now) else {
        return;
    };
    let has_orders = world
        .get::<OrderQueue>(actor.entity)
        .is_some_and(OrderQueue::has_order);
    let other_busy = world
        .get_resource::<AgentRegistry>()
        .is_some_and(|registry| {
            registry.busy.iter().any(|(name, provider)| {
                *name != "npc_command" && provider(world, actor.entity).is_some()
            })
        });
    let queued = world
        .get::<npc_framework::agent_io::control::ExternalQueue>(actor.entity)
        .is_some_and(|queue| queue.busy());
    let command_waiting = world
        .get_resource::<npc_framework::kernel::commands::CommandQueue>()
        .is_some_and(|queue| {
            queue
                .pending
                .values()
                .any(|command| command.unit_in(world) == Some(actor.id))
        });
    if has_orders
        && !other_busy
        && !queued
        && !command_waiting
        && world.resource::<NativeActions>().auto_attempt != Some((actor, now))
    {
        world.resource_mut::<NativeActions>().auto_attempt = Some((actor, now));
        if let Err(reason) = shared_act(
            world,
            actor,
            ActStep {
                verb: Some("execute_orders".into()),
                ..default()
            },
        ) {
            feedback(world, format!("Movement unavailable: {reason}"));
        }
    }
}

#[cfg(test)]
mod tests {}
