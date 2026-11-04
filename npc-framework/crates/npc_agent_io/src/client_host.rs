//! Operator-side bridge for a separate AI game client. Leases and monitoring
//! are host data: no simulation commands, sessions, observations or receipts.
use super::{AgentPulse, SchedulerExclusions, SchedulerPolicy, SchedulerRuntime, notifications};
use crate::api::{self, RpcError, RpcResult};
use crate::control::{ControlMode, ExternalController};
use crate::events::{self, AgentEvents};
use crate::registration::{self, AgentCredentials};
use crate::stats::wall_now;
use bevy_ecs::prelude::*;
use npc_kernel::{SimControl, SimId, SimIndex, SimPos};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

pub const LEASE_SECONDS: u64 = 30;
const MAX_CLIENTS: usize = 64;

struct Lease {
    owner: String,
    unit: SimId,
    deadline: Instant,
    credential: Option<[u8; 32]>,
    authorized_once: bool,
    revoked: bool,
    running: bool,
}

#[derive(Resource, Default)]
struct ClientLeases(BTreeMap<String, Lease>);

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    controller: String,
    owner: String,
    #[serde(default)]
    unit: Option<u32>,
    #[serde(default)]
    epoch: Option<u64>,
    #[serde(default)]
    since_seq: Option<u64>,
    #[serde(default)]
    token: Option<String>,
    #[serde(default)]
    running: Option<bool>,
}

fn error(reason: &str, message: &str) -> RpcError {
    let mut error = RpcError::invalid(message);
    error.data = Some(json!({"reason":reason}));
    error
}

fn request(params: Option<Value>) -> Result<Request, RpcError> {
    let request: Request = serde_json::from_value(params.unwrap_or(Value::Null)).map_err(|_| {
        error(
            "invalid_client_request",
            "expected controller and private client owner",
        )
    })?;
    if request.controller.is_empty()
        || request.controller.len() > 200
        || request.controller.chars().any(char::is_control)
        || !(16..=128).contains(&request.owner.len())
        || !request
            .owner
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
        || request.unit == Some(0)
    {
        return Err(error("invalid_client_request", "invalid client identity"));
    }
    Ok(request)
}

fn expire(world: &mut World) {
    if let Some(mut leases) = world.get_resource_mut::<ClientLeases>() {
        let now = wall_now();
        leases.0.retain(|_, lease| lease.deadline > now);
    }
}

pub(super) fn running_slots(world: &mut World) -> usize {
    expire(world);
    world.get_resource::<ClientLeases>().map_or(0, |leases| {
        leases.0.values().filter(|lease| lease.running).count()
    })
}

/// Legacy runners yield only while a client holds a live lease. This function
/// is called by the host frame/status path, never the simulation schedule.
pub(super) fn suppress_legacy(world: &mut World, pulses: &mut [AgentPulse]) {
    expire(world);
    let Some(leases) = world.get_resource::<ClientLeases>() else {
        return;
    };
    for pulse in pulses {
        if leases.0.contains_key(&pulse.controller) {
            pulse.blocked = Some("external client owns continuation".into());
        }
    }
}

fn target(world: &mut World, request: &Request) -> Result<(SimId, Option<[u8; 32]>), RpcError> {
    // Refresh host credentials without registering a new game session or
    // recording agent activity. Explorer controllers use their agent_id.
    let _ = registration::controller_session_active(world, &request.controller, SimId(0));
    if registration::required(world) {
        let credential = world
            .get_resource::<AgentCredentials>()
            .and_then(|credentials| credentials.get(&request.controller))
            .filter(|credential| credential.controller == request.controller)
            .ok_or_else(|| {
                error(
                    "unknown_controller",
                    "no assigned actor for this controller",
                )
            })?;
        if request
            .unit
            .is_some_and(|unit| unit != credential.assigned.0)
        {
            return Err(error("actor_changed", "the assigned actor changed"));
        }
        return Ok((credential.assigned, Some(credential.token_digest)));
    }
    let attached = world
        .query::<(&SimId, &ExternalController)>()
        .iter(world)
        .filter(|(_, controller)| controller.id == request.controller)
        .map(|(id, _)| *id)
        .min();
    let unit = request.unit.map(SimId).or(attached).ok_or_else(|| {
        error(
            "unknown_controller",
            "give the assigned actor for an unlocked world",
        )
    })?;
    if attached.is_some_and(|attached| attached != unit) {
        return Err(error(
            "actor_changed",
            "the controller is attached to another actor",
        ));
    }
    Ok((unit, None))
}

fn claim(world: &mut World, request: Request) -> RpcResult {
    expire(world);
    if let Some(lease) = world
        .get_resource::<ClientLeases>()
        .and_then(|leases| leases.0.get(&request.controller))
        && lease.owner != request.owner
    {
        return Err(error(
            "client_owned",
            "another AI game client owns this controller",
        ));
    }
    let (unit, credential) = target(world, &request)?;
    if credential.is_some_and(|digest| {
        request.token.as_deref().map(registration::token_digest) != Some(digest)
    }) {
        return Err(error(
            "client_auth_required",
            "valid agent credentials are required to claim this controller",
        ));
    }
    if world
        .get_resource::<SchedulerRuntime>()
        .is_some_and(|runtime| {
            runtime
                .runs
                .iter()
                .any(|run| run.controller == request.controller)
        })
    {
        return Err(error(
            "legacy_running",
            "wait for the host's current AI turn to finish",
        ));
    }
    let entity = world
        .get_resource::<SimIndex>()
        .and_then(|index| index.get(unit))
        .ok_or_else(|| error("actor_missing", "the assigned actor is unavailable"))?;
    if world
        .get::<ExternalController>(entity)
        .is_some_and(|controller| controller.id != request.controller)
    {
        return Err(error(
            "actor_owned",
            "the assigned actor has another controller",
        ));
    }
    let authorized = registration::controller_session_active(world, &request.controller, unit);
    let mut leases = world.get_resource_or_init::<ClientLeases>();
    if !leases.0.contains_key(&request.controller) && leases.0.len() >= MAX_CLIENTS {
        return Err(error(
            "client_capacity",
            "the game has reached its AI client limit",
        ));
    }
    let lease = leases.0.entry(request.controller).or_insert(Lease {
        owner: request.owner,
        unit,
        deadline: wall_now(),
        credential,
        authorized_once: authorized,
        revoked: false,
        running: false,
    });
    if lease.unit != unit || lease.credential != credential || lease.revoked {
        return Err(error(
            "client_revoked",
            "release this client before starting a new assignment",
        ));
    }
    lease.deadline = wall_now() + Duration::from_secs(LEASE_SECONDS);
    Ok(
        json!({"claimed":true,"protocol":1,"unit":unit.0,"lease_seconds":LEASE_SECONDS,"epoch":api::epoch(world)}),
    )
}

fn poll(world: &mut World, mut request: Request) -> RpcResult {
    expire(world);
    let (unit, credential, authorized_once, already_revoked) = {
        let lease = world
            .get_resource::<ClientLeases>()
            .and_then(|leases| leases.0.get(&request.controller))
            .filter(|lease| lease.owner == request.owner)
            .ok_or_else(|| {
                error(
                    "client_lease_lost",
                    "claim the controller before monitoring",
                )
            })?;
        (
            lease.unit,
            lease.credential,
            lease.authorized_once,
            lease.revoked,
        )
    };
    if request.unit.is_some_and(|asked| asked != unit.0) {
        return Err(error(
            "actor_changed",
            "this lease belongs to another actor",
        ));
    }
    // An unlocked actor may still be unattached during the initial provider
    // bootstrap. The established lease supplies its stable assigned unit.
    request.unit = Some(unit.0);
    let current_target = target(world, &request);
    let target_changed = !matches!(&current_target, Ok(target) if *target == (unit, credential));
    let authorized = registration::controller_session_active(world, &request.controller, unit);
    let revoked = already_revoked || target_changed || (authorized_once && !authorized);
    {
        let mut leases = world.resource_mut::<ClientLeases>();
        let lease = leases
            .0
            .get_mut(&request.controller)
            .expect("lease checked");
        lease.deadline = wall_now() + Duration::from_secs(LEASE_SECONDS);
        lease.authorized_once |= authorized;
        lease.revoked = revoked;
    }
    let policy = world
        .get_resource::<SchedulerPolicy>()
        .cloned()
        .unwrap_or_default();
    let entity = world
        .get_resource::<SimIndex>()
        .and_then(|index| index.get(unit));
    let controller = entity.and_then(|entity| world.get::<ExternalController>(entity));
    let attached = controller.is_some_and(|controller| controller.id == request.controller);
    let observer = controller.is_some_and(|controller| controller.mode == ControlMode::Observer);
    let other_owner = controller.is_some_and(|controller| controller.id != request.controller);
    let position = entity
        .and_then(|entity| world.get::<SimPos>(entity))
        .map(|p| (p.x, p.z));
    let excluded = world
        .get_resource::<SchedulerExclusions>()
        .and_then(|exclusions| exclusions.0.get(&unit))
        .cloned();
    let paused = world
        .get_resource::<SimControl>()
        .is_some_and(|control| control.is_stopped());
    let blocked = if revoked {
        Some("registration revoked or disconnected".to_owned())
    } else if entity.is_none() {
        Some("actor missing".to_owned())
    } else if let Some(excluded) = excluded {
        Some(excluded)
    } else if paused {
        Some("game paused".to_owned())
    } else if other_owner {
        Some("actor has another controller".to_owned())
    } else if observer {
        Some("observer control".to_owned())
    } else if !attached {
        Some(
            if authorized {
                "waiting for agent control"
            } else {
                "waiting for agent registration"
            }
            .to_owned(),
        )
    } else {
        None
    };
    let activity = world
        .get_resource::<crate::stats::AgentStats>()
        .and_then(|stats| stats.controllers.get(&request.controller))
        .map_or((0, 0), |stats| {
            (stats.observes.saturating_add(stats.requests), stats.acts)
        });
    let pulse = AgentPulse {
        controller: request.controller.clone(),
        unit: unit.0,
        activity,
        position,
        busy: api::busy_reason(world, unit),
        blocked,
    };
    events::harvest(world);
    let epoch = api::epoch(world);
    let events = world.get_resource::<AgentEvents>();
    let latest = events.map_or(0, AgentEvents::last_seq);
    let oldest = events.map_or(1, AgentEvents::oldest_seq);
    let reset = request.epoch != Some(epoch) || request.since_seq.is_none_or(|seq| seq > latest);
    let acknowledged_seq = notifications::acknowledged_seq(world, &request.controller, unit);
    let since = if reset {
        latest
    } else {
        request.since_seq.unwrap_or(latest).max(acknowledged_seq)
    };
    let dropped = oldest.saturating_sub(since.saturating_add(1));
    let (notifications, cursor) = if attached && authorized && !revoked {
        events.map_or_else(
            || (Vec::new(), latest),
            |events| {
                notifications::read_notifications(
                    events,
                    &request.controller,
                    unit,
                    since,
                    world
                        .get_resource::<super::SchedulerEventClassifier>()
                        .copied(),
                )
            },
        )
    } else {
        (Vec::new(), latest)
    };
    Ok(json!({
        "protocol":1,"unit":unit.0,"epoch":epoch,"last_seq":cursor,
        "latest_seq":latest,"oldest_seq":oldest,"dropped":dropped,"reset":reset,
        "acknowledged_seq":acknowledged_seq,
        "attached":attached,"pulse":pulse,"policy":policy,"notifications":notifications,
        "lease_seconds":LEASE_SECONDS
    }))
}

fn turn(world: &mut World, request: Request) -> RpcResult {
    let running = request
        .running
        .ok_or_else(|| error("invalid_client_request", "turn needs a running boolean"))?;
    let controller = request.controller.clone();
    let owner = request.owner.clone();
    if !running {
        expire(world);
        let mut leases = world
            .get_resource_mut::<ClientLeases>()
            .ok_or_else(|| error("client_lease_lost", "the AI client lease expired"))?;
        let lease = leases
            .0
            .get_mut(&controller)
            .filter(|lease| lease.owner == owner)
            .ok_or_else(|| error("client_lease_lost", "the AI client lease expired"))?;
        lease.running = false;
        lease.deadline = wall_now() + Duration::from_secs(LEASE_SECONDS);
        return Ok(json!({"running":false,"acquired":false}));
    }
    let snapshot = poll(world, request)?;
    let blocked = snapshot["pulse"]["blocked"].as_str();
    if snapshot["pulse"]["busy"].is_string()
        || blocked.is_some_and(|reason| {
            !matches!(
                reason,
                "waiting for agent registration" | "waiting for agent control"
            )
        })
    {
        return Err(error(
            "turn_blocked",
            "the assigned actor cannot start an AI turn now",
        ));
    }
    let current_running = world
        .resource::<ClientLeases>()
        .0
        .get(&controller)
        .is_some_and(|lease| lease.running);
    if !current_running {
        let clients = running_slots(world);
        let legacy = world
            .get_resource::<SchedulerRuntime>()
            .map_or(0, |runtime| runtime.runs.len());
        let cap = snapshot["policy"]["max_concurrent"].as_u64().unwrap_or(1) as usize;
        if clients.saturating_add(legacy) >= cap {
            return Err(error(
                "client_capacity",
                "waiting for another AI turn to finish",
            ));
        }
        world
            .resource_mut::<ClientLeases>()
            .0
            .get_mut(&controller)
            .expect("poll renewed lease")
            .running = true;
    }
    Ok(json!({"running":true,"acquired":true}))
}

fn release(world: &mut World, request: Request) -> RpcResult {
    expire(world);
    if let Some(mut leases) = world.get_resource_mut::<ClientLeases>() {
        if leases
            .0
            .get(&request.controller)
            .is_some_and(|lease| lease.owner != request.owner)
        {
            return Err(error(
                "client_owned",
                "another AI game client owns this controller",
            ));
        }
        leases.0.remove(&request.controller);
    }
    Ok(json!({"released":true}))
}

pub fn call(world: &mut World, method: &str, params: Option<Value>) -> RpcResult {
    let request = request(params)?;
    match method {
        "client/claim" => claim(world, request),
        "client/poll" => poll(world, request),
        "client/release" => release(world, request),
        "client/turn" => turn(world, request),
        _ => Err(error("unknown_client_method", "unknown AI client method")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AgentIoPlugin;
    use crate::registration::{Credential, RegistrationRequired};
    use crate::stats::AgentStats;
    use bevy_app::App;
    use npc_ai::{AiRegistry, Brain, ParamValue, Status, StopReason};
    use npc_kernel::commands::CommandQueue;
    use npc_kernel::journal::JournalKind;
    use npc_kernel::registry::KernelAppExt;
    use npc_kernel::{NpcKernelPlugin, SimClock, run_ticks, spawn_sim};

    const ID: &str = "client-test-agent";
    const OWNER: &str = "private-client-owner-marker";
    const TOKEN: &str = "private-game-token-marker";
    fn idle(_: &mut World, _: Entity, _: &ParamValue) -> Status {
        Status::Running
    }
    fn stop(_: &mut World, _: Entity, _: StopReason) {}
    fn app() -> App {
        let mut app = App::new();
        app.add_plugins(NpcKernelPlugin::default())
            .register_action("Idle", idle, stop)
            .register_tree("toy", |builder| {
                builder.leaf("Idle", "Idle", ());
            })
            .add_plugins(AgentIoPlugin::default());
        npc_kernel::ensure_frozen(app.world_mut());
        let tree = app.world().resource::<AiRegistry>().tree_id("toy").unwrap();
        spawn_sim(app.world_mut(), Brain { tree });
        app.insert_resource(RegistrationRequired);
        registration::set_credential(
            app.world_mut(),
            ID,
            Credential {
                token_digest: registration::token_digest(TOKEN),
                assigned: SimId(1),
                controller: ID.into(),
            },
        );
        app
    }
    fn rpc(app: &mut App, method: &str, more: Value) -> RpcResult {
        let mut params = json!({"controller":ID,"owner":OWNER,"token":TOKEN});
        for (key, value) in more.as_object().unwrap() {
            params[key] = value.clone();
        }
        api::call(app.world_mut(), method, Some(params))
    }
    fn attach(app: &mut App) -> String {
        let session = api::call(
            app.world_mut(),
            "agent/register",
            Some(json!({"agent_id":ID,"token":TOKEN})),
        )
        .unwrap()["session"]
            .as_str()
            .unwrap()
            .to_owned();
        api::call(
            app.world_mut(),
            "agent/take_control",
            Some(json!({"session":session})),
        )
        .unwrap();
        run_ticks(app.world_mut(), 1);
        session
    }

    #[test]
    fn bootstrap_infers_actor_without_registering_or_changing_simulation() {
        let mut app = app();
        let tick = app.world().resource::<SimClock>().tick;
        let hash = npc_kernel::hash::state_hash(app.world_mut());
        assert_eq!(rpc(&mut app, "client/claim", json!({})).unwrap()["unit"], 1);
        for _ in 0..3 {
            let result = rpc(&mut app, "client/poll", json!({})).unwrap();
            assert_eq!(result["attached"], false);
            assert_eq!(result["pulse"]["blocked"], "waiting for agent registration");
            let text = result.to_string();
            assert!(!text.contains(TOKEN) && !text.contains(OWNER));
        }
        assert_eq!(app.world().resource::<SimClock>().tick, tick);
        assert!(app.world().resource::<CommandQueue>().pending.is_empty());
        assert!(app.world().resource::<AgentStats>().controllers.is_empty());
        assert!(!registration::controller_session_active(
            app.world_mut(),
            ID,
            SimId(1)
        ));
        assert_eq!(npc_kernel::hash::state_hash(app.world_mut()), hash);
    }

    #[test]
    fn leases_exclude_other_clients_and_legacy_until_expiry_or_release() {
        let mut app = app();
        attach(&mut app);
        rpc(&mut app, "client/claim", json!({})).unwrap();
        let error = rpc(
            &mut app,
            "client/claim",
            json!({"owner":"other-private-owner-marker"}),
        )
        .unwrap_err();
        assert_eq!(error.data.unwrap()["reason"], "client_owned");
        assert!(
            rpc(
                &mut app,
                "client/release",
                json!({"owner":"other-private-owner-marker"})
            )
            .is_err()
        );
        let policy = SchedulerPolicy::default();
        let mut pulses = super::super::pulses(app.world_mut(), &policy);
        suppress_legacy(app.world_mut(), &mut pulses);
        assert_eq!(
            pulses[0].blocked.as_deref(),
            Some("external client owns continuation")
        );
        rpc(&mut app, "client/release", json!({})).unwrap();
        let mut pulses = super::super::pulses(app.world_mut(), &policy);
        suppress_legacy(app.world_mut(), &mut pulses);
        assert!(pulses[0].blocked.is_none());
        rpc(&mut app, "client/claim", json!({})).unwrap();
        app.world_mut()
            .resource_mut::<ClientLeases>()
            .0
            .get_mut(ID)
            .unwrap()
            .deadline = wall_now();
        assert_eq!(
            rpc(&mut app, "client/poll", json!({}))
                .unwrap_err()
                .data
                .unwrap()["reason"],
            "client_lease_lost"
        );
        rpc(
            &mut app,
            "client/claim",
            json!({"owner":"other-private-owner-marker"}),
        )
        .unwrap();
    }

    #[test]
    fn monitoring_does_not_acknowledge_notifications_or_count_as_agent_activity() {
        let mut app = app();
        let session = attach(&mut app);
        rpc(&mut app, "client/claim", json!({})).unwrap();
        let first = rpc(&mut app, "client/poll", json!({})).unwrap();
        let params = json!({"epoch":first["epoch"],"since_seq":first["last_seq"]});
        let requests = app.world().resource::<AgentStats>().controllers[ID].requests;
        {
            let mut events = app.world_mut().resource_mut::<AgentEvents>();
            let seq = events.next_seq;
            events.next_seq += 1;
            events.events.push_back(crate::AgentEvent {
                seq,
                tick: 5,
                unit: Some(SimId(1)),
                kind: "ExternalAction".into(),
                event: JournalKind::ExternalAction {
                    handle: 1,
                    status: "done".into(),
                    verb: "private-game-text-marker".into(),
                    latency_ticks: None,
                },
            });
        }
        let a = rpc(&mut app, "client/poll", params.clone()).unwrap();
        let b = rpc(&mut app, "client/poll", params.clone()).unwrap();
        assert_eq!(a["notifications"], b["notifications"]);
        assert_eq!(a["notifications"][0]["summary"], "action_completed");
        assert!(!a.to_string().contains("private-game-text-marker"));
        assert_eq!(
            app.world().resource::<AgentStats>().controllers[ID].requests,
            requests
        );
        assert_eq!(a["acknowledged_seq"], 0);
        api::call(
            app.world_mut(),
            "agent/observe",
            Some(json!({"session":session})),
        )
        .unwrap();
        let observed = rpc(&mut app, "client/poll", params).unwrap();
        assert_eq!(observed["notifications"], json!([]));
        assert_eq!(observed["acknowledged_seq"], observed["latest_seq"]);
    }

    #[test]
    fn revocation_and_changed_credentials_are_latched_until_explicit_release() {
        let mut app = app();
        attach(&mut app);
        rpc(&mut app, "client/claim", json!({})).unwrap();
        registration::revoke_agent(app.world_mut(), ID);
        let revoked = rpc(&mut app, "client/poll", json!({})).unwrap();
        assert_eq!(
            revoked["pulse"]["blocked"],
            "registration revoked or disconnected"
        );
        attach(&mut app);
        assert_eq!(
            rpc(&mut app, "client/poll", json!({})).unwrap()["pulse"]["blocked"],
            "registration revoked or disconnected"
        );
        assert!(rpc(&mut app, "client/claim", json!({})).is_err());
        rpc(&mut app, "client/release", json!({})).unwrap();
        rpc(&mut app, "client/claim", json!({})).unwrap();
        registration::set_credential(
            app.world_mut(),
            ID,
            Credential {
                token_digest: registration::token_digest("rotated-marker"),
                assigned: SimId(1),
                controller: ID.into(),
            },
        );
        assert_eq!(
            rpc(&mut app, "client/poll", json!({})).unwrap()["pulse"]["blocked"],
            "registration revoked or disconnected"
        );
    }

    #[test]
    fn event_epoch_reset_and_pause_are_explicit_without_advancing_ticks() {
        let mut app = app();
        attach(&mut app);
        rpc(&mut app, "client/claim", json!({})).unwrap();
        app.insert_resource(SimControl {
            paused: true,
            ..Default::default()
        });
        let tick = app.world().resource::<SimClock>().tick;
        let result = rpc(
            &mut app,
            "client/poll",
            json!({"epoch":u64::MAX,"since_seq":u64::MAX}),
        )
        .unwrap();
        assert_eq!(result["reset"], true);
        assert_eq!(result["pulse"]["blocked"], "game paused");
        assert_eq!(result["notifications"], json!([]));
        assert_eq!(app.world().resource::<SimClock>().tick, tick);
    }

    #[test]
    fn notification_pages_are_bounded_and_preserve_unread_cursor_and_actor_scope() {
        let mut app = app();
        attach(&mut app);
        rpc(&mut app, "client/claim", json!({})).unwrap();
        let first = rpc(&mut app, "client/poll", json!({})).unwrap();
        {
            let mut events = app.world_mut().resource_mut::<AgentEvents>();
            for index in 0..40 {
                let seq = events.next_seq;
                events.next_seq += 1;
                events.events.push_back(crate::AgentEvent {
                    seq,
                    tick: 5,
                    unit: Some(SimId(if index % 2 == 0 { 2 } else { 1 })),
                    kind: "ExternalAction".into(),
                    event: JournalKind::ExternalAction {
                        handle: 1,
                        status: if (index / 2) % 2 == 0 {
                            "done"
                        } else {
                            "failed"
                        }
                        .into(),
                        verb: "private-verb-marker".into(),
                        latency_ticks: None,
                    },
                });
            }
        }
        let page = rpc(
            &mut app,
            "client/poll",
            json!({"epoch":first["epoch"],"since_seq":first["last_seq"]}),
        )
        .unwrap();
        assert_eq!(
            page["notifications"].as_array().unwrap().len(),
            notifications::MAX_PENDING_EVENTS
        );
        assert!(page["last_seq"].as_u64().unwrap() < page["latest_seq"].as_u64().unwrap());
        assert!(!page.to_string().contains("private-verb-marker"));
        let second = rpc(
            &mut app,
            "client/poll",
            json!({"epoch":page["epoch"],"since_seq":page["last_seq"]}),
        )
        .unwrap();
        assert_eq!(second["last_seq"], second["latest_seq"]);
        assert_eq!(
            page["notifications"]
                .as_array()
                .unwrap()
                .iter()
                .chain(second["notifications"].as_array().unwrap())
                .map(|notification| notification["count"].as_u64().unwrap())
                .sum::<u64>(),
            20
        );
    }

    #[test]
    fn a_public_controller_identity_cannot_claim_without_private_credentials() {
        let mut app = app();
        for token in [Value::Null, json!("incorrect-private-secret-marker")] {
            let error = rpc(&mut app, "client/claim", json!({"token":token})).unwrap_err();
            assert_eq!(
                error.data.as_ref().unwrap()["reason"],
                "client_auth_required"
            );
            let serialized = serde_json::to_string(&error).unwrap();
            assert!(!serialized.contains(TOKEN));
            assert!(!serialized.contains("incorrect-private-secret-marker"));
            assert!(!serialized.contains(OWNER));
        }
        assert_eq!(
            rpc(&mut app, "client/poll", json!({}))
                .unwrap_err()
                .data
                .unwrap()["reason"],
            "client_lease_lost"
        );
        rpc(&mut app, "client/claim", json!({})).unwrap();
    }

    #[test]
    fn separate_client_turns_share_a_host_capacity_and_release_only_owned_slots() {
        let mut app = app();
        let tree = app.world().resource::<AiRegistry>().tree_id("toy").unwrap();
        spawn_sim(app.world_mut(), Brain { tree });
        registration::set_credential(
            app.world_mut(),
            "other-agent",
            Credential {
                assigned: SimId(2),
                controller: "other-agent".into(),
                token_digest: registration::token_digest(TOKEN),
            },
        );
        rpc(&mut app, "client/claim", json!({})).unwrap();
        rpc(
            &mut app,
            "client/claim",
            json!({"controller":"other-agent","owner":"other-client-owner-marker"}),
        )
        .unwrap();
        assert_eq!(
            rpc(&mut app, "client/turn", json!({"running":true})).unwrap()["acquired"],
            true
        );
        // An idempotent start never needs a second slot.
        assert_eq!(
            rpc(&mut app, "client/turn", json!({"running":true})).unwrap()["acquired"],
            true
        );
        let other =
            json!({"controller":"other-agent","owner":"other-client-owner-marker","running":true});
        assert_eq!(
            rpc(&mut app, "client/turn", other.clone())
                .unwrap_err()
                .data
                .unwrap()["reason"],
            "client_capacity"
        );
        assert!(
            rpc(
                &mut app,
                "client/turn",
                json!({"owner":"wrong-client-owner-marker","running":false})
            )
            .is_err()
        );
        assert_eq!(running_slots(app.world_mut()), 1);
        rpc(&mut app, "client/turn", json!({"running":false})).unwrap();
        assert_eq!(
            rpc(&mut app, "client/turn", other).unwrap()["acquired"],
            true
        );
        rpc(
            &mut app,
            "client/release",
            json!({"controller":"other-agent","owner":"other-client-owner-marker"}),
        )
        .unwrap();
        assert_eq!(running_slots(app.world_mut()), 0);
        app.insert_resource(SimControl {
            paused: true,
            ..Default::default()
        });
        assert_eq!(
            rpc(&mut app, "client/turn", json!({"running":true}))
                .unwrap_err()
                .data
                .unwrap()["reason"],
            "turn_blocked"
        );
    }

    #[test]
    fn unlocked_bootstrap_retains_the_explicit_assignment_before_attachment() {
        let mut app = app();
        app.world_mut().remove_resource::<RegistrationRequired>();
        rpc(&mut app, "client/claim", json!({"unit":1})).unwrap();
        let snapshot = rpc(&mut app, "client/poll", json!({})).unwrap();
        assert_eq!(snapshot["unit"], 1);
        assert_eq!(snapshot["pulse"]["blocked"], "waiting for agent control");
        assert_eq!(
            rpc(&mut app, "client/turn", json!({"running":true})).unwrap()["acquired"],
            true
        );
    }
}
