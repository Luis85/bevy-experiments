//! Opt-in host authentication. Credentials, sessions, failure deadlines and
//! callbacks are never registered for hashing or world snapshots.
use std::collections::{BTreeMap, VecDeque};
use std::fmt;
use std::time::{Duration, Instant};

use bevy_ecs::prelude::*;
use npc_kernel::commands::{CommandQueue, SimCommand, submit, validate};
use npc_kernel::{SimClock, SimId, SimIndex};
use serde_json::{Value, json};

use crate::api::{self, RpcError, RpcResult};
use crate::control::{
    AgentClear, AttachController, ControlMode, DetachController, ExternalController, IdlePolicy,
};
use crate::stats::wall_now;

/// Worlds without this marker retain the legacy agent interface.
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct RegistrationRequired;

#[derive(Clone, PartialEq, Eq)]
pub struct Credential {
    pub token_digest: [u8; 32],
    pub assigned: SimId,
    pub controller: String,
}

impl fmt::Debug for Credential {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Credential")
            .field("assigned", &self.assigned)
            .field("controller", &self.controller)
            .field("token_digest", &"<redacted>")
            .finish()
    }
}

#[derive(Resource, Default)]
pub struct AgentCredentials {
    entries: BTreeMap<String, Credential>,
}

impl AgentCredentials {
    pub fn get(&self, agent_id: &str) -> Option<&Credential> {
        self.entries.get(agent_id)
    }
}

#[derive(Clone)]
struct LiveSession {
    agent_id: String,
    control_requested: bool,
}

/// Host-memory session registry. Its Debug representation exposes no keys.
#[derive(Resource, Default)]
pub struct AgentSessions {
    // An ephemeral origin prefix distinguishes an evicted/revoked key on
    // this host from a key presented after restart or a world handover.
    host_epoch: Option<String>,
    live: BTreeMap<String, LiveSession>,
    by_agent: BTreeMap<String, String>,
    revoked: BTreeMap<String, ()>,
    revocation_order: VecDeque<String>,
    /// Deadline and whether the failed id was a configured identity.
    /// Unknown-id churn may evict only unknown-id cooldowns.
    failures: BTreeMap<String, (Instant, bool)>,
}

impl fmt::Debug for AgentSessions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AgentSessions")
            .field("live", &self.live.len())
            .field("revoked", &self.revoked.len())
            .finish()
    }
}

pub type ReloadFn = fn(&mut World);
pub type EnterWorldFn = fn(&World, &str, SimId) -> Value;

#[derive(Resource, Default)]
struct Hooks {
    reload: Option<ReloadFn>,
    enter: Option<EnterWorldFn>,
    world_name: String,
}

/// OS entropy is used only for host credentials/session keys, never sim RNG.
pub fn new_secret() -> Result<String, String> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes).map_err(|_| "OS randomness is unavailable".to_owned())?;
    use std::fmt::Write as _;
    let mut out = String::with_capacity(64);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    Ok(out)
}

pub fn token_digest(token: &str) -> [u8; 32] {
    *blake3::hash(token.as_bytes()).as_bytes()
}

pub fn set_reload(world: &mut World, hook: ReloadFn) {
    world.get_resource_or_init::<Hooks>().reload = Some(hook);
}
pub fn set_enter_world(world: &mut World, hook: EnterWorldFn) {
    world.get_resource_or_init::<Hooks>().enter = Some(hook);
}
pub fn set_world_name(world: &mut World, name: impl Into<String>) {
    world.get_resource_or_init::<Hooks>().world_name = name.into();
}

/// Updating a credential closes its old sessions; unchanged metadata does not.
pub fn set_credential(world: &mut World, agent_id: impl Into<String>, credential: Credential) {
    let id = agent_id.into();
    let changed = world
        .get_resource::<AgentCredentials>()
        .and_then(|c| c.entries.get(&id))
        .is_some_and(|old| old != &credential);
    if changed {
        revoke_agent(world, &id);
    }
    world
        .get_resource_or_init::<AgentCredentials>()
        .entries
        .insert(id, credential);
}

pub fn remove_credential(world: &mut World, agent_id: &str) {
    revoke_agent(world, agent_id);
    world
        .get_resource_or_init::<AgentCredentials>()
        .entries
        .remove(agent_id);
}

/// Hosts use this on unreadable or invalid credential files: fail closed.
pub fn clear_credentials(world: &mut World) {
    let ids: Vec<_> = world
        .get_resource_or_init::<AgentSessions>()
        .by_agent
        .keys()
        .cloned()
        .collect();
    for id in ids {
        revoke_agent(world, &id);
    }
    world
        .get_resource_or_init::<AgentCredentials>()
        .entries
        .clear();
}

pub fn revoke_agent(world: &mut World, agent_id: &str) {
    let mut sessions = world.get_resource_or_init::<AgentSessions>();
    if let Some(key) = sessions.by_agent.remove(agent_id) {
        sessions.live.remove(&key);
        sessions.revoked.insert(key.clone(), ());
        sessions.revocation_order.push_back(key);
        // Bounded host state even if a client continually registers again.
        while sessions.revocation_order.len() > 4096 {
            if let Some(old) = sessions.revocation_order.pop_front() {
                sessions.revoked.remove(&old);
            }
        }
    }
}

pub fn required(world: &World) -> bool {
    world.contains_resource::<RegistrationRequired>()
}

fn reload(world: &mut World) {
    if let Some(hook) = world.get_resource::<Hooks>().and_then(|h| h.reload) {
        hook(world);
    }
}

fn error(reason: &str, message: &str) -> RpcError {
    let mut err = RpcError::invalid(message);
    err.data = Some(json!({"reason":reason}));
    err
}

/// A validated caller. The private key is deliberately absent from Debug.
#[derive(Clone)]
pub struct Authorization {
    key: String,
    pub agent_id: String,
    pub assigned: SimId,
    pub controller: String,
}

impl fmt::Debug for Authorization {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Authorization")
            .field("agent_id", &self.agent_id)
            .field("assigned", &self.assigned)
            .field("session", &"<redacted>")
            .finish()
    }
}

fn session(world: &World, key: &str) -> Result<Authorization, RpcError> {
    let live = world
        .get_resource::<AgentSessions>()
        .and_then(|s| s.live.get(key));
    let Some(live) = live else {
        let revoked = world.get_resource::<AgentSessions>().is_some_and(|s| {
            s.revoked.contains_key(key)
                || s.host_epoch.as_ref().is_some_and(|epoch| {
                    key.strip_prefix(epoch.as_str())
                        .is_some_and(|rest| rest.starts_with('.'))
                })
        });
        let mut e = error(
            "unknown_session",
            "the registration session is no longer valid",
        );
        if revoked {
            e.data.as_mut().expect("auth error data")["revoked"] = json!(true);
        }
        return Err(e);
    };
    let Some(c) = world
        .get_resource::<AgentCredentials>()
        .and_then(|c| c.get(&live.agent_id))
    else {
        return Err(error(
            "unknown_session",
            "the registration session is no longer valid",
        ));
    };
    Ok(Authorization {
        key: key.into(),
        agent_id: live.agent_id.clone(),
        assigned: c.assigned,
        controller: c.controller.clone(),
    })
}

fn pending(world: &World, kind: &str, permit: &Authorization) -> Option<u64> {
    world
        .get_resource::<CommandQueue>()?
        .pending
        .iter()
        .find_map(|(&(tick, _), cmd)| match cmd {
            SimCommand::Content {
                kind: name,
                payload,
            } if name == kind
                && payload.get("unit").and_then(Value::as_u64)
                    == Some(u64::from(permit.assigned.0))
                && payload.get("controller").and_then(Value::as_str)
                    == Some(permit.controller.as_str()) =>
            {
                Some(tick)
            }
            _ => None,
        })
}

fn controlled(world: &World, permit: &Authorization) -> bool {
    let requested = world
        .get_resource::<AgentSessions>()
        .and_then(|s| s.live.get(&permit.key))
        .is_some_and(|s| s.control_requested);
    requested
        && pending(world, "AttachController", permit).is_none()
        && pending(world, "DetachController", permit).is_none()
        && world
            .get_resource::<SimIndex>()
            .and_then(|i| i.get(permit.assigned))
            .and_then(|e| world.get::<ExternalController>(e))
            .is_some_and(|c| c.id == permit.controller && c.mode == ControlMode::Direct)
}

/// Revalidate a parked request, including credential rotation while it waited.
pub fn validate_permit(
    world: &mut World,
    permit: &Authorization,
    needs_control: bool,
) -> Result<(), RpcError> {
    reload(world);
    let current = session(world, &permit.key)?;
    if current.agent_id != permit.agent_id
        || current.assigned != permit.assigned
        || current.controller != permit.controller
    {
        return Err(error(
            "unknown_session",
            "the registration session is no longer valid",
        ));
    }
    if needs_control && !controlled(world, &current) {
        let mut e = error(
            "not_in_control",
            "take_control must finish before using this tool",
        );
        e.data.as_mut().expect("auth error data")["pending"] =
            json!(pending(world, "AttachController", &current).is_some());
        return Err(e);
    }
    Ok(())
}

/// Authorize and normalize a raw agent request. Unlocked worlds are untouched.
/// The session field stays in transport nonce keys, but is removed before
/// legacy deny_unknown_fields deserialization.
pub fn authorize(
    world: &mut World,
    method: &str,
    params: &mut Option<Value>,
) -> Result<Option<Authorization>, RpcError> {
    if !method.starts_with("agent/") || !required(world) {
        return Ok(None);
    }
    reload(world);
    let key = params
        .as_ref()
        .and_then(|p| p.get("session"))
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| error("not_registered", "register before using agent tools"))?;
    let permit = session(world, key)?;
    let map = params
        .as_mut()
        .and_then(Value::as_object_mut)
        .ok_or_else(|| RpcError::invalid("agent parameters must be an object"))?;
    if map
        .get("unit")
        .is_some_and(|v| v.as_u64() != Some(u64::from(permit.assigned.0)))
        || map
            .get("controller")
            .is_some_and(|v| v.as_str() != Some(permit.controller.as_str()))
    {
        return Err(error(
            "not_yours",
            "the requested actor is not assigned to this agent",
        ));
    }
    let needs_control = !matches!(
        method,
        "agent/enter_world" | "agent/take_control" | "agent/attach"
    ) && !method.starts_with("agent/notes/")
        && !method.starts_with("agent/map/");
    validate_permit(world, &permit, needs_control)?;
    map.remove("session");
    map.remove("unit");
    map.remove("controller");
    match method {
        "agent/list_units" => {}
        "agent/stats" => {
            map.insert("controller".into(), json!(permit.controller));
        }
        _ => {
            map.insert("unit".into(), json!(permit.assigned.0));
            map.insert("controller".into(), json!(permit.controller));
        }
    }
    Ok(Some(permit))
}

pub fn bootstrap(method: &str) -> bool {
    matches!(
        method,
        "agent/register" | "agent/enter_world" | "agent/take_control" | "agent/release"
    )
}

pub fn call(world: &mut World, method: &str, mut params: Option<Value>) -> RpcResult {
    if !required(world) {
        return Err(error(
            "registration_not_required",
            "this world uses the legacy agent interface",
        ));
    }
    if method == "agent/register" {
        return register(world, params);
    }
    let permit = authorize(world, method, &mut params)?.expect("registration required");
    match method {
        "agent/enter_world" => {
            let value = if let Some(hook) = world.get_resource::<Hooks>().and_then(|h| h.enter) {
                hook(world, &permit.agent_id, permit.assigned)
            } else {
                json!({"character":{"name":permit.agent_id,"body":"actor","description":"","senses":[],"actions":[]},"scene":"You arrive in the world."})
            };
            api::stamp(world, value)
        }
        "agent/take_control" => take_control(world, &permit),
        "agent/release" => {
            let clear = AgentClear {
                unit: permit.assigned.0,
                controller: permit.controller.clone(),
            };
            let detach = DetachController {
                unit: permit.assigned.0,
                controller: permit.controller.clone(),
            };
            let commands = [SimCommand::content(&clear), SimCommand::content(&detach)];
            // Clear is explicit so a same-tick re-take cannot preserve old
            // queued work by restoring Direct mode before Dispatch sees it.
            for cmd in &commands {
                validate(world, cmd).map_err(|e| RpcError::command(&e))?;
            }
            let mut tick = 0;
            for cmd in commands {
                tick = submit(world, None, cmd)
                    .map_err(|e| RpcError::command(&e))?
                    .0;
            }
            world
                .resource_mut::<AgentSessions>()
                .live
                .get_mut(&permit.key)
                .expect("authorized session")
                .control_requested = false;
            api::stamp(world, json!({"accepted":true,"applies_at":tick}))
        }
        _ => Err(RpcError::invalid("unknown registration method")),
    }
}

fn register(world: &mut World, params: Option<Value>) -> RpcResult {
    reload(world);
    let id = params
        .as_ref()
        .and_then(|p| p.get("agent_id"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let token = params
        .as_ref()
        .and_then(|p| p.get("token"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let now = wall_now();
    if let Some((deadline, _)) = world
        .get_resource::<AgentSessions>()
        .and_then(|s| s.failures.get(id))
        && *deadline > now
    {
        return Err(bad_credentials(
            deadline
                .duration_since(now)
                .as_millis()
                .saturating_add(1)
                .min(1000) as u64,
        ));
    }
    let credential = world
        .get_resource::<AgentCredentials>()
        .and_then(|c| c.get(id))
        .cloned();
    let expected = credential.as_ref().map_or([0; 32], |c| c.token_digest);
    // blake3::Hash equality uses constant_time_eq_32, unlike array ==.
    let matched =
        blake3::Hash::from_bytes(expected) == blake3::Hash::from_bytes(token_digest(token));
    if credential.is_none() || !matched || token.is_empty() || id.is_empty() || id.len() > 64 {
        let mut sessions = world.get_resource_or_init::<AgentSessions>();
        sessions.failures.retain(|_, (deadline, _)| *deadline > now);
        // Bound untrusted ids as well as the number of failure records.
        if id.len() <= 64 {
            let known = credential.is_some();
            if !known
                && sessions
                    .failures
                    .values()
                    .filter(|(_, known)| !known)
                    .count()
                    >= 256
                && let Some(old) = sessions
                    .failures
                    .iter()
                    .filter(|(_, (_, known))| !known)
                    .min_by_key(|(_, (deadline, _))| *deadline)
                    .map(|(id, _)| id.clone())
            {
                sessions.failures.remove(&old);
            }
            sessions
                .failures
                .insert(id.into(), (now + Duration::from_secs(1), known));
        }
        return Err(bad_credentials(1000));
    }
    let entropy_error =
        |_| RpcError::new(api::codes::INTERNAL, "cannot create a registration session");
    let epoch = match world
        .get_resource::<AgentSessions>()
        .and_then(|s| s.host_epoch.clone())
    {
        Some(epoch) => epoch,
        None => new_secret().map_err(entropy_error)?,
    };
    let key = format!("{epoch}.{}", new_secret().map_err(entropy_error)?);
    revoke_agent(world, id);
    let mut sessions = world.get_resource_or_init::<AgentSessions>();
    sessions.host_epoch = Some(epoch);
    sessions.failures.remove(id);
    sessions.live.insert(
        key.clone(),
        LiveSession {
            agent_id: id.into(),
            control_requested: false,
        },
    );
    sessions.by_agent.insert(id.into(), key.clone());
    let world_name = world
        .get_resource::<Hooks>()
        .map(|h| h.world_name.clone())
        .unwrap_or_else(|| "world".into());
    api::stamp(
        world,
        json!({"session":key,"agent_id":id,"world":world_name}),
    )
}

fn bad_credentials(retry: u64) -> RpcError {
    let mut e = error("bad_credentials", "registration credentials were refused");
    e.data.as_mut().expect("auth error data")["retry_after_ms"] = json!(retry);
    e
}

fn take_control(world: &mut World, permit: &Authorization) -> RpcResult {
    let tick = world.get_resource::<SimClock>().map_or(0, |c| c.tick);
    let attach = pending(world, "AttachController", permit);
    let detach = pending(world, "DetachController", permit);
    let already = world
        .get_resource::<SimIndex>()
        .and_then(|i| i.get(permit.assigned))
        .and_then(|e| world.get::<ExternalController>(e))
        .is_some_and(|c| c.id == permit.controller && c.mode == ControlMode::Direct);
    let applies_at = if let Some(at) = attach {
        at
    } else if already && detach.is_none() {
        tick
    } else {
        let cmd = AttachController {
            unit: permit.assigned.0,
            controller: permit.controller.clone(),
            mode: ControlMode::Direct,
            on_idle: IdlePolicy::Idle,
            force: false,
        };
        submit(world, None, SimCommand::content(&cmd))
            .map_err(|e| RpcError::command(&e))?
            .0
    };
    world
        .resource_mut::<AgentSessions>()
        .live
        .get_mut(&permit.key)
        .expect("authorized session")
        .control_requested = true;
    let ready = controlled(world, permit);
    api::stamp(
        world,
        json!({"accepted":true,"in_control":ready,"pending":!ready,"unit":permit.assigned.0,"controller":permit.controller,"applies_at":applies_at}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_id_flood_cannot_evict_a_configured_agents_cooldown() {
        let mut world = World::new();
        world.insert_resource(RegistrationRequired);
        set_credential(
            &mut world,
            "known",
            Credential {
                token_digest: token_digest("correct"),
                assigned: SimId(1),
                controller: "known".into(),
            },
        );
        register(
            &mut world,
            Some(json!({"agent_id":"known","token":"wrong"})),
        )
        .unwrap_err();
        // Keep this test independent of host load, while exercising the
        // actual failed-registration path and its configured-id marker.
        let deadline = wall_now() + Duration::from_secs(60);
        world
            .resource_mut::<AgentSessions>()
            .failures
            .get_mut("known")
            .unwrap()
            .0 = deadline;
        for i in 0..300 {
            let id = format!("unknown-{i}");
            register(&mut world, Some(json!({"agent_id":id,"token":"wrong"}))).unwrap_err();
            world
                .resource_mut::<AgentSessions>()
                .failures
                .get_mut(&id)
                .unwrap()
                .0 = deadline;
        }
        let sessions = world.resource::<AgentSessions>();
        assert_eq!(sessions.failures.get("known"), Some(&(deadline, true)));
        assert_eq!(
            sessions
                .failures
                .values()
                .filter(|(_, known)| !known)
                .count(),
            256
        );
        register(
            &mut world,
            Some(json!({"agent_id":"known","token":"another guess"})),
        )
        .unwrap_err();
        assert_eq!(
            world.resource::<AgentSessions>().failures.get("known"),
            Some(&(deadline, true)),
            "a throttled guess must not be checked as a new failure"
        );
    }
}
