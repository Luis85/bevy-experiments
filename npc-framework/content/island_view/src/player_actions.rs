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
    before: SchedulerPolicy,
    applied: SchedulerPolicy,
    changed: u32,
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
        .ok_or("This world has no village hero")?;
    let entity = world
        .get_resource::<SimIndex>()
        .and_then(|index| index.get(state.played))
        .ok_or("The hero is no longer present")?;
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
        return Err(
            "Another controller owns this life; release it before choosing a successor".into(),
        );
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
            Err("The hero has another controller; release it before playing natively".into())
        }
        _ => Err("Choose Play as hero and wait for the control command to apply".into()),
    }
}
/// Registration protects remote transports. Native input is trusted in-process input;
/// it does not forge a registration, session, authorization token or notes identity.
pub(crate) fn observe(world: &World) -> Result<Value, String> {
    let id = world
        .get_resource::<VillageState>()
        .ok_or("This world has no village hero")?
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
    .ok_or_else(|| "The hero is no longer present".into())
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
    let text = if island_content::village::colony::enabled(world) {
        player_feedback(&text).into()
    } else {
        text
    };
    world.get_resource_or_init::<NativeActions>().feedback = Some(text);
}
fn player_feedback(text: &str) -> &str {
    match text {
        "Native control requested; it applies on the next simulation tick" => {
            "Getting ready to play. Resume the world if it is paused."
        }
        "Stop requested; queued approach and work cancelled on the next tick" => {
            "Stopping your current activity."
        }
        "Native control released on the next tick" => {
            "Returning to inspection. The village keeps running."
        }
        "Action submitted" => "Your activity is queued.",
        "Approach cancelled: hero, world or controller changed" => {
            "The journey was cancelled because the active character changed."
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
    let Some(before) = world.get_resource::<SchedulerPolicy>().cloned() else {
        return;
    };
    let mut applied = before.clone();
    applied.enabled = false;
    if applied != before {
        *world.resource_mut::<SchedulerPolicy>() = applied.clone();
    }
    let changed = world
        .get_resource_ref::<SchedulerPolicy>()
        .expect("existing policy")
        .last_changed()
        .get();
    let submitted = world.resource::<SimClock>().tick;
    world.resource_mut::<NativeActions>().scheduler = Some(SchedulerHold {
        actor,
        submitted,
        before,
        applied,
        changed,
    });
}
fn restore_scheduler(world: &mut World) {
    let Some(hold) = world
        .get_resource_or_init::<NativeActions>()
        .scheduler
        .take()
    else {
        return;
    };
    // An inserted/replaced/edited policy belongs to its later writer, including a reset.
    if world
        .get_resource_ref::<SchedulerPolicy>()
        .is_some_and(|policy| {
            *policy == hold.applied && policy.last_changed().get() == hold.changed
        })
        && hold.before != hold.applied
    {
        *world.resource_mut::<SchedulerPolicy>() = hold.before;
    }
}
pub(crate) fn take_control(world: &mut World) -> Result<Value, String> {
    let actor = actor(world)?;
    if matches!(control(world), ControlState::Foreign { .. }) {
        return Err("Another controller owns the hero; native input will not take it over".into());
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
    feedback(
        world,
        "Native control requested; it applies on the next simulation tick",
    );
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
        return Err("Native hero controls may act only as the current hero".into());
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
        let public = if island_content::village::colony::enabled(world) {
            progress_feedback::refusal(world, &reason)
        } else {
            let lower = reason.to_lowercase();
            if lower.contains("control") {
                "Action refused: control changed before it could apply".into()
            } else if lower.contains("buyer has") || lower.contains("buyer cannot afford") {
                "Action refused: the merchant cannot fulfill that trade now".into()
            } else {
                format!(
                    "Action refused: {}",
                    reason.chars().take(320).collect::<String>()
                )
            }
        };
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
    if !island_content::village::colony::enabled(world) {
        feedback(
            world,
            response
                .get("label")
                .and_then(Value::as_str)
                .unwrap_or("Action submitted"),
        );
    }
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
    if island_content::village::colony::enabled(world) {
        progress_feedback::track(world, actor, None, &response, mark.event_seq);
    } else {
        feedback(world, "Walking to the chosen position");
    }
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
            "Your neighbour is out of sight. Find them again before continuing the conversation or trade.",
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
                    Ok(value) if !island_content::village::colony::enabled(world) => feedback(
                        world,
                        value
                            .get("label")
                            .and_then(Value::as_str)
                            .unwrap_or("Action submitted"),
                    ),
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
mod tests {
    use super::*;
    use island_content::village::{self, ResourcePatch, economy, homestead};
    use npc_framework::agent_io::registration::RegistrationRequired;
    use npc_framework::interaction::items;
    use npc_framework::kernel::{commands::CommandLog, hash::state_hash};

    fn fixture() -> App {
        let mut app = island_content::new_sim_app(7, "living_village");
        run_ticks(app.world_mut(), 1);
        let actor = actor(app.world()).unwrap();
        let owner = app
            .world()
            .get::<ExternalController>(actor.entity)
            .unwrap()
            .id
            .clone();
        submit(
            app.world_mut(),
            None,
            SimCommand::content(&DetachController {
                unit: actor.id.0,
                controller: owner,
            }),
        )
        .unwrap();
        run_ticks(app.world_mut(), 1);
        app
    }
    fn claim_native(app: &mut App) {
        take_control(app.world_mut()).unwrap();
        assert_eq!(
            control(app.world()),
            ControlState::Free,
            "attachment is recorded, not applied in the renderer"
        );
        run_ticks(app.world_mut(), 1);
        tick(app.world_mut());
        assert_eq!(control(app.world()), ControlState::Native);
    }
    fn advance(app: &mut App, ticks: u64) {
        for _ in 0..ticks {
            run_ticks(app.world_mut(), 1);
            tick(app.world_mut());
        }
    }
    fn resource(app: &mut App, kind: &str) -> (SimId, SimPos) {
        let mut resources: Vec<_> = app
            .world_mut()
            .query::<(&SimId, &SimPos, &ResourcePatch)>()
            .iter(app.world())
            .filter(|(_, _, patch)| patch.kind == kind)
            .map(|(id, pos, _)| (*id, *pos))
            .collect();
        resources.sort_by_key(|(id, _)| *id);
        resources[0]
    }
    fn step(verb: &str, target: Option<SimId>) -> ActStep {
        ActStep {
            verb: Some(verb.into()),
            target: target.map(|id| id.0),
            ..default()
        }
    }

    #[test]
    fn scoped_observation_is_read_only_and_native_never_steals_a_controller() {
        let mut app = fixture();
        let before = state_hash(app.world_mut());
        let observed = observe(app.world()).unwrap();
        assert!(observed.pointer("/sections/village/environment").is_some());
        assert!(observed.pointer("/sections/homestead/plots").is_some());
        assert!(observed["affordances"].is_array());
        assert_eq!(before, state_hash(app.world_mut()));
        let actor = actor(app.world()).unwrap();
        submit(
            app.world_mut(),
            None,
            SimCommand::content(&AttachController {
                unit: actor.id.0,
                controller: "external-ai".into(),
                mode: ControlMode::Direct,
                on_idle: IdlePolicy::Idle,
                force: false,
            }),
        )
        .unwrap();
        run_ticks(app.world_mut(), 1);
        let before = state_hash(app.world_mut());
        assert!(take_control(app.world_mut()).is_err());
        assert_eq!(
            control(app.world()),
            ControlState::Foreign {
                controller: "external-ai".into()
            }
        );
        assert_eq!(before, state_hash(app.world_mut()));
        assert!(act(app.world_mut(), step("rest_ground", None)).is_err());
        assert!(stop(app.world_mut()).is_err());
        assert!(release(app.world_mut()).is_err());
        assert_eq!(known_position(app.world(), SimId(u32::MAX)), None);
    }

    #[test]
    fn native_registered_host_uses_shared_actions_while_remote_registration_stays_required() {
        let mut app = fixture();
        app.world_mut().insert_resource(RegistrationRequired);
        claim_native(&mut app);
        let actor = actor(app.world()).unwrap();
        assert!(
            api::call(
                app.world_mut(),
                "agent/act",
                Some(
                    json!({"unit":actor.id.0,"controller":NATIVE_CONTROLLER,"verb":"rest_ground"})
                )
            )
            .is_err()
        );
        act(app.world_mut(), step("rest_ground", None)).unwrap();
        run_ticks(app.world_mut(), 1);
        assert!(
            app.world()
                .get::<Villager>(actor.entity)
                .unwrap()
                .resting_until
                > app.world().resource::<SimClock>().tick
        );
        assert!(app.world().resource::<CommandLog>().entries.iter().any(
            |(_, _, command)| matches!(command,SimCommand::Content {kind,..} if kind=="VillageAct")
        ));
        assert!(app.world().contains_resource::<RegistrationRequired>());
    }

    #[test]
    fn zero_cash_hero_walks_and_gathers_real_wood_through_recorded_actions() {
        let mut app = fixture();
        claim_native(&mut app);
        let actor = actor(app.world()).unwrap();
        let (target, pos) = resource(&mut app, "log");
        let start = *app.world().get::<SimPos>(actor.entity).unwrap();
        let before = economy::count(app.world(), actor.entity, "log");
        assert_eq!(
            app.world()
                .get::<village::Wallet>(actor.entity)
                .unwrap()
                .coins,
            0
        );
        walk_then_act(app.world_mut(), pos, step("gather", Some(target))).unwrap();
        for _ in 0..1500 {
            advance(&mut app, 1);
            if economy::count(app.world(), actor.entity, "log") > before {
                break;
            }
        }
        assert_ne!(
            *app.world().get::<SimPos>(actor.entity).unwrap(),
            start,
            "no teleport or grant may replace travel"
        );
        assert_eq!(
            economy::count(app.world(), actor.entity, "log"),
            before + 1,
            "the shared gather must produce a physical log"
        );
        assert_eq!(
            app.world()
                .get::<village::Wallet>(actor.entity)
                .unwrap()
                .coins,
            0
        );
        assert!(app.world().resource::<CommandLog>().entries.iter().any(
            |(_, _, command)| matches!(command,SimCommand::Content {kind,..} if kind=="AgentAct")
        ));
        assert!(items::carried_matching(app.world(), actor.entity, "log").is_some());
        assert!(
            observe(app.world()).unwrap()["recent_events"]
                .as_array()
                .unwrap()
                .iter()
                .any(
                    |event| event.pointer("/event/effect/type").and_then(Value::as_str)
                        == Some("VillageGathered")
                ),
            "ordinary work must publish its real completion to native observations after the approach is over"
        );
    }

    #[test]
    fn stopped_approach_does_not_later_gather_and_rts_move_runs_without_a_second_click() {
        let mut app = fixture();
        claim_native(&mut app);
        let actor = actor(app.world()).unwrap();
        let (target, pos) = resource(&mut app, "log");
        walk_then_act(app.world_mut(), pos, step("gather", Some(target))).unwrap();
        stop(app.world_mut()).unwrap();
        advance(&mut app, 500);
        assert!(app.world().resource::<NativeActions>().pending.is_none());
        assert_eq!(economy::count(app.world(), actor.entity, "log"), 0);
        let start = *app.world().get::<SimPos>(actor.entity).unwrap();
        let goal = SimPos::new(start.x + 1000, start.z);
        shared_act(
            app.world_mut(),
            actor,
            ActStep {
                verb: Some("order".into()),
                args: Some(json!({"order":{"type":"Move","pos":[goal.x,goal.z]}})),
                ..default()
            },
        )
        .unwrap();
        advance(&mut app, 80);
        assert!(
            app.world().get::<SimPos>(actor.entity).unwrap().dist2(goal) <= 40_000,
            "native Update must start ExecuteOrder for an ordinary RTS order"
        );
    }

    #[test]
    fn arrival_rechecks_materials_and_control_loss_cancels_queued_intent() {
        let mut app = fixture();
        claim_native(&mut app);
        let actor = actor(app.world()).unwrap();
        let (plot, entity) = homestead::plots(app.world())
            .into_iter()
            .find(|(_, entity)| {
                homestead::definition(app.world(), *entity)
                    .unwrap()
                    .use_kind
                    == "housing"
            })
            .unwrap();
        let center = *app.world().get::<SimPos>(entity).unwrap();
        walk_then_act(app.world_mut(), center, step("claim_plot", Some(plot))).unwrap();
        for _ in 0..1500 {
            advance(&mut app, 1);
            if app.world().resource::<NativeActions>().pending.is_none() {
                break;
            }
        }
        advance(&mut app, 1);
        assert_eq!(
            app.world()
                .get::<homestead::HomesteadPlot>(entity)
                .unwrap()
                .owner,
            Some(actor.id)
        );
        walk_then_act(app.world_mut(), center, step("build_cottage", Some(plot))).unwrap();
        advance(&mut app, 30);
        assert!(
            app.world()
                .get::<homestead::CottageBuild>(actor.entity)
                .is_none()
        );
        assert!(
            !app.world()
                .get::<homestead::HomesteadPlot>(entity)
                .unwrap()
                .cottage_built
        );
        assert!(
            app.world()
                .resource::<NativeActions>()
                .feedback
                .as_deref()
                .is_some_and(|feedback| feedback.starts_with("At destination:")),
            "insufficient real materials must be reported after arrival"
        );
        let (target, pos) = resource(&mut app, "log");
        walk_then_act(app.world_mut(), pos, step("gather", Some(target))).unwrap();
        submit(
            app.world_mut(),
            None,
            SimCommand::content(&AttachController {
                unit: actor.id.0,
                controller: "other-ai".into(),
                mode: ControlMode::Direct,
                on_idle: IdlePolicy::Idle,
                force: true,
            }),
        )
        .unwrap();
        advance(&mut app, 1);
        assert!(app.world().resource::<NativeActions>().pending.is_none());
        assert_eq!(economy::count(app.world(), actor.entity, "log"), 0);
    }

    #[test]
    fn scheduler_hold_restores_on_release_but_respects_a_later_policy_writer() {
        let mut app = fixture();
        let before = SchedulerPolicy {
            enabled: true,
            ..default()
        };
        app.world_mut().insert_resource(before.clone());
        claim_native(&mut app);
        assert!(!app.world().resource::<SchedulerPolicy>().enabled);
        release(app.world_mut()).unwrap();
        advance(&mut app, 1);
        assert_eq!(*app.world().resource::<SchedulerPolicy>(), before);
        claim_native(&mut app);
        app.world_mut()
            .resource_mut::<SchedulerPolicy>()
            .cooldown_seconds = 321;
        release(app.world_mut()).unwrap();
        advance(&mut app, 1);
        assert_eq!(
            app.world().resource::<SchedulerPolicy>().cooldown_seconds,
            321
        );
        assert!(
            !app.world().resource::<SchedulerPolicy>().enabled,
            "a later policy edit must not be overwritten"
        );
    }

    #[test]
    fn menu_suspension_keeps_committed_cottage_materials_and_native_ownership() {
        let mut app = fixture();
        claim_native(&mut app);
        let actor = actor(app.world()).unwrap();
        let (plot, entity) = homestead::plots(app.world())
            .into_iter()
            .find(|(_, entity)| {
                homestead::definition(app.world(), *entity)
                    .unwrap()
                    .use_kind
                    == "housing"
            })
            .unwrap();
        // This fixture isolates menu persistence, not unaided material acquisition.
        let door = *app.world().get::<SimPos>(entity).unwrap();
        app.world_mut().entity_mut(actor.entity).insert(door);
        homestead::HomesteadAct::new(actor.id, "claim_plot", plot)
            .perform(app.world_mut())
            .unwrap();
        economy::grant(app.world_mut(), actor.entity, Item::new("plank").stack(8));
        economy::grant(app.world_mut(), actor.entity, Item::new("log").stack(4));
        submit(
            app.world_mut(),
            None,
            SimCommand::content(&homestead::HomesteadAct::new(
                actor.id,
                "build_cottage",
                plot,
            )),
        )
        .unwrap();
        assert_eq!(
            prepare_suspend(app.world_mut()).unwrap(),
            None,
            "an accepted construction request must also survive the menu before its first tick"
        );
        run_ticks(app.world_mut(), 1);
        let build = app
            .world()
            .get::<homestead::CottageBuild>(actor.entity)
            .unwrap()
            .clone();
        assert!(!build.materials.is_empty());
        let before = state_hash(app.world_mut());
        assert_eq!(prepare_suspend(app.world_mut()).unwrap(), None);
        assert_eq!(before, state_hash(app.world_mut()));
        assert_eq!(
            app.world().get::<homestead::CottageBuild>(actor.entity),
            Some(&build)
        );
        assert_eq!(control(app.world()), ControlState::Native);
        for material in &build.materials {
            assert!(
                app.world()
                    .resource::<Reservations>()
                    .holds(material.item, actor.id)
            );
        }
    }

    #[test]
    fn menu_suspension_records_navigation_stop_without_detaching_native_control() {
        let mut app = fixture();
        claim_native(&mut app);
        let actor = actor(app.world()).unwrap();
        let start = *app.world().get::<SimPos>(actor.entity).unwrap();
        walk(app.world_mut(), SimPos::new(start.x + 4000, start.z)).unwrap();
        advance(&mut app, 3);
        let before = state_hash(app.world_mut());
        let receipt = prepare_suspend(app.world_mut())
            .unwrap()
            .expect("movement needs a recorded stop");
        assert_eq!(before, state_hash(app.world_mut()));
        assert!(
            app.world()
                .resource::<npc_framework::kernel::commands::CommandQueue>()
                .pending
                .contains_key(&receipt)
        );
        advance(&mut app, 2);
        assert_eq!(control(app.world()), ControlState::Native);
        assert!(app.world().get::<MoveIntent>(actor.entity).is_none());
        assert!(
            app.world()
                .get::<OrderQueue>(actor.entity)
                .is_none_or(|queue| !queue.has_order())
        );
    }

    #[test]
    fn restored_pre_tick_navigation_is_stopped_before_the_actor_moves() {
        use npc_framework::kernel::snapshot;
        let mut app = fixture();
        claim_native(&mut app);
        let actor = actor(app.world()).unwrap();
        let start = *app.world().get::<SimPos>(actor.entity).unwrap();
        walk(app.world_mut(), SimPos::new(start.x + 4000, start.z)).unwrap();
        let saved = snapshot::save(app.world_mut()).unwrap();
        let mut restored = island_content::new_sim_app(7, "living_village");
        snapshot::load(restored.world_mut(), &saved, snapshot::LoadMode::Strict).unwrap();
        assert!(restored.world().get_resource::<NativeActions>().is_none());
        let receipt = prepare_suspend(restored.world_mut())
            .unwrap()
            .expect("saved pending movement is transient navigation");
        assert!(
            restored
                .world()
                .resource::<npc_framework::kernel::commands::CommandQueue>()
                .pending
                .contains_key(&receipt)
        );
        advance(&mut restored, 2);
        let current = super::actor(restored.world()).unwrap();
        assert_eq!(
            *restored.world().get::<SimPos>(current.entity).unwrap(),
            start
        );
        assert!(restored.world().get::<MoveIntent>(current.entity).is_none());
        assert_eq!(control(restored.world()), ControlState::Native);
    }

    fn moving_neighbour_fixture(speed: i32, destination: i32) -> (App, ActorStamp, SimId) {
        let mut app = fixture();
        claim_native(&mut app);
        let actor = actor(app.world()).unwrap();
        let neighbour = app
            .world()
            .resource::<island_content::TestbedIds>()
            .id("Bram");
        let entity = app.world().resource::<SimIndex>().get(neighbour).unwrap();
        // Initial placement isolates the approach race; all subsequent motion is
        // the ordinary recorded order/motor path, not test teleporting a target.
        app.world_mut()
            .entity_mut(actor.entity)
            .insert((SimPos::new(-1000, 0), Facing::new(1, 0)));
        app.world_mut()
            .entity_mut(entity)
            .insert(SimPos::new(2000, 0));
        app.world_mut()
            .get_mut::<Villager>(entity)
            .unwrap()
            .base_speed = speed;
        submit(
            app.world_mut(),
            None,
            SimCommand::content(&AttachController {
                unit: neighbour.0,
                controller: "moving-neighbour".into(),
                mode: ControlMode::Direct,
                on_idle: IdlePolicy::Idle,
                force: false,
            }),
        )
        .unwrap();
        advance(&mut app, 20);
        assert!(
            observe(app.world()).unwrap()["sections"]["perception"]["sensed"]
                .as_array()
                .unwrap()
                .iter()
                .any(|person| person["id"] == neighbour.0)
        );
        npc_framework::kernel::journal::set_journal(app.world_mut(), true, false);
        walk_then_act(
            app.world_mut(),
            SimPos::new(2000, 0),
            ActStep {
                verb: Some("talk".into()),
                target: Some(neighbour.0),
                args: Some(json!({"text":"Hello, neighbour."})),
                ..default()
            },
        )
        .unwrap();
        api::act(
            app.world_mut(),
            ActParams {
                unit: neighbour.0,
                controller: "moving-neighbour".into(),
                steps: Some(vec![
                    ActStep {
                        verb: Some("order".into()),
                        args: Some(json!({"order":{"type":"Move","pos":[destination,0]}})),
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
        .unwrap();
        (app, actor, neighbour)
    }

    #[test]
    fn conversation_reapproaches_a_sensed_moving_neighbour_with_bounded_recorded_walks() {
        let (mut app, actor, neighbour) = moving_neighbour_fixture(70, 7000);
        let mut retries = 0;
        for _ in 0..350 {
            advance(&mut app, 1);
            if let Some(pending) = app.world().resource::<NativeActions>().pending.as_ref() {
                retries = retries.max(pending.retargets);
            } else {
                break;
            }
        }
        advance(&mut app, 2);
        assert!((1..=3).contains(&retries));
        assert!(app.world().resource::<NativeActions>().pending.is_none());
        assert!(
            app.world()
                .resource::<npc_framework::kernel::journal::Journal>()
                .entries
                .iter()
                .any(|entry| {
                    entry.unit == Some(actor.id)
                        && matches!(&entry.kind,JournalKind::Effect{effect,applied:true}
                if effect.get("type").and_then(Value::as_str)==Some("VillageSpeech") && effect.get("target").and_then(Value::as_u64)==Some(u64::from(neighbour.0)))
                }),
            "the same requested conversation must occur after physical re-approach"
        );
    }

    #[test]
    fn conversation_stops_with_feedback_when_the_neighbour_leaves_sensed_knowledge() {
        let (mut app, _, _) = moving_neighbour_fixture(600, 25000);
        advance(&mut app, 150);
        assert!(app.world().resource::<NativeActions>().pending.is_none());
        assert!(
            app.world()
                .resource::<NativeActions>()
                .feedback
                .as_deref()
                .unwrap_or("")
                .contains("out of sight")
        );
    }

    fn natural_plant_input_fixture() -> (App, ActorStamp, SimId, Entity) {
        let mut app = fixture();
        claim_native(&mut app);
        let actor = actor(app.world()).unwrap();
        let (seed_source, seed_pos) = resource(&mut app, village::catalog::GRAIN);
        walk_then_act(app.world_mut(), seed_pos, step("gather", Some(seed_source))).unwrap();
        for _ in 0..1500 {
            advance(&mut app, 1);
            if economy::count(app.world(), actor.entity, village::catalog::GRAIN) > 0 {
                break;
            }
        }
        assert_eq!(
            economy::count(app.world(), actor.entity, village::catalog::GRAIN),
            1
        );
        let (plot, entity) = homestead::plots(app.world())
            .into_iter()
            .find(|(_, entity)| {
                homestead::definition(app.world(), *entity)
                    .unwrap()
                    .use_kind
                    == "housing"
            })
            .unwrap();
        let center = *app.world().get::<SimPos>(entity).unwrap();
        walk_then_act(app.world_mut(), center, step("claim_plot", Some(plot))).unwrap();
        for _ in 0..1500 {
            advance(&mut app, 1);
            if app
                .world()
                .get::<homestead::HomesteadPlot>(entity)
                .unwrap()
                .owner
                == Some(actor.id)
            {
                break;
            }
        }
        assert_eq!(
            app.world()
                .get::<homestead::HomesteadPlot>(entity)
                .unwrap()
                .owner,
            Some(actor.id)
        );
        (app, actor, plot, entity)
    }

    #[test]
    fn direct_action_apply_time_refusal_reaches_native_feedback() {
        let (mut app, actor, plot, entity) = natural_plant_input_fixture();
        let request = ActStep {
            verb: Some("plant".into()),
            target: Some(plot.0),
            args: Some(json!({"bed":0})),
            ..default()
        };
        assert_eq!(
            act(app.world_mut(), request.clone()).unwrap()["accepted"],
            true
        );
        assert_eq!(act(app.world_mut(), request).unwrap()["accepted"], true);
        advance(&mut app, 1);
        assert!(
            app.world()
                .get::<homestead::HomesteadPlot>(entity)
                .unwrap()
                .beds[0]
                .is_some()
        );
        assert_eq!(
            economy::count(app.world(), actor.entity, village::catalog::GRAIN),
            0,
            "only the one real seed can be spent"
        );
        assert!(
            app.world()
                .get_resource::<npc_framework::kernel::commands::SkippedCommands>()
                .unwrap()
                .entries
                .iter()
                .any(|entry| entry.cmd.kind() == "HomesteadAct"
                    && entry.error.to_string().contains("already contains a crop"))
        );
        assert!(
            app.world()
                .resource::<NativeActions>()
                .feedback
                .as_deref()
                .is_some_and(|message| message.contains("already contains a crop")),
            "a direct command accepted before the tick must report its actual apply-time refusal"
        );
    }

    #[test]
    fn late_older_refusal_and_foreign_events_do_not_replace_a_newer_native_action() {
        let (mut app, actor, plot, _) = natural_plant_input_fixture();
        let plant = ActStep {
            verb: Some("plant".into()),
            target: Some(plot.0),
            args: Some(json!({"bed":0})),
            ..default()
        };
        act(app.world_mut(), plant.clone()).unwrap();
        act(app.world_mut(), plant).unwrap();
        act(app.world_mut(), step("rest_ground", None)).unwrap();
        let latest = app.world().resource::<NativeActions>().feedback.clone();
        // Host event fixture: even a foreign controller's event about this hero is not native feedback.
        npc_framework::kernel::journal::record(app.world_mut(), Some(actor.id), || {
            JournalKind::ActionRejected {
                controller: "foreign-private-controller".into(),
                verb: "HomesteadAct".into(),
                reason: "FOREIGN_PRIVATE_CANARY".into(),
            }
        });
        tick(app.world_mut());
        assert_eq!(app.world().resource::<NativeActions>().feedback, latest);
        advance(&mut app, 1);
        assert!(
            app.world()
                .get_resource::<npc_framework::kernel::commands::SkippedCommands>()
                .unwrap()
                .entries
                .iter()
                .any(|entry| entry.cmd.kind() == "HomesteadAct"
                    && entry.error.to_string().contains("already contains a crop")),
            "the superseded request really must fail after the later rest input"
        );
        assert_eq!(
            app.world().resource::<NativeActions>().feedback,
            latest,
            "an old command's later refusal must not overwrite the latest input's feedback"
        );
        assert!(
            !app.world()
                .resource::<NativeActions>()
                .feedback
                .as_deref()
                .unwrap_or("")
                .contains("FOREIGN_PRIVATE_CANARY")
        );
    }
}
