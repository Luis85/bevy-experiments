//! The agent methods on Bevy's BRP (feature `remote`), for windowed games:
//! add [`AgentRemotePlugin`] after `npc_tools`' `NpcRemotePlugin`.
//!
//! Instant methods ([`api::METHODS`] minus `agent/events` and
//! `agent/act_and_wait`) are ordinary BRP methods. `agent/events` must not
//! block: a BRP handler runs inside the game's frame (`RemoteLast`), so a
//! handler that waited would stall the game loop. Instead it is a *watching* method (`bevy_remote` runs it once
//! per frame until it returns `Some`): over HTTP a method name without
//! `+watch` gets that first `Some` as its single response, so the client's
//! request simply stays open, a long-poll served one cheap check per frame.
//!
//! Per-request state (a poll's deadline, a stream's last sent `seq`) is
//! kept server-side. `bevy_remote` 0.20 does not hand a watching handler
//! the JSON-RPC request id, so a request is identified by its method and
//! params **plus its position among the requests with the same params in
//! this frame** (`bevy_remote` runs every listed watcher once per frame):
//! two identical concurrent polls or streams get separate state. The order
//! is not stable (when one request ends, `bevy_remote` swap-removes it, so
//! identical ones behind it may trade places), so clients should send a
//! unique `nonce` param (the CLI and the MCP server send one per call).
//! `agent/events+watch` streams every new batch of events as server-sent
//! events until the client disconnects.
//!
//! `agent/act_and_wait` is a watching method too, and it **needs a `nonce`**
//! param (invalid params without one; the CLI and the MCP server send one
//! per call): its state is keyed by method and params (nonce included), not
//! by position. Its first run acts (at once, like `agent/act`) and keeps the
//! [`api::ActWait`]; later runs only check the wait. An act must never run
//! twice, yet `bevy_remote` keeps re-running a watcher until the HTTP side
//! has dropped the answered request, which is usually a frame or more after
//! the answer. So every finished act-and-wait (also a rejected act or
//! `timeout_ms: 0`, answered on the first run) leaves a tombstone holding
//! its answer: a re-run gets the same answer again, never a second act.
//! Entries are forgotten [`ACT_KEEP`] after their last run (an in-flight
//! wait is run every frame, so it never expires while its request lives).
//!
//! `world/load_scenario` and `world/reset` need the session host (they
//! rebuild the app); here they return error `-23601`.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use bevy_app::prelude::*;
use bevy_ecs::prelude::*;
use npc_tools::remote::{BrpError, BrpResult, RemoteAppExt};
use serde_json::Value;

use crate::api::{self, RpcError, codes};
use crate::stats::wall_now;

/// Watching requests unseen for this long are forgotten.
const FORGET_AFTER: Duration = Duration::from_secs(5);

fn to_brp(e: RpcError) -> BrpError {
    BrpError {
        code: i16::try_from(e.code).unwrap_or(i16::MIN),
        message: e.message,
        data: e.data,
    }
}

/// `agent/act_and_wait` entries (in flight or finished) are forgotten this
/// long after their last run.
pub const ACT_KEEP: Duration = Duration::from_secs(60);

/// Per-request state of the `agent/events` watching methods, keyed by
/// params and the request's position among identical ones this frame
/// (module docs).
#[derive(Resource, Default)]
struct Watches {
    /// `key -> (first seen, last seen, last seq sent)`.
    entries: BTreeMap<String, (Instant, Instant, u64)>,
    /// The frame [`Watches::seen`] counts for ([`WatchFrame`]).
    frame: u64,
    /// `method + params -> handler runs so far this frame`.
    seen: BTreeMap<String, u32>,
}

/// One `agent/act_and_wait` request's state.
enum ActSlot {
    /// Acted; waiting since `first`.
    Waiting { first: Instant, wait: api::ActWait },
    /// Answered: the tombstone that answers every re-run the same way.
    Done(Result<Value, RpcError>),
}

/// `agent/act_and_wait` state, keyed by method and params (with the
/// required nonce): `key -> (last run, state)`.
#[derive(Resource, Default)]
struct ActWaits {
    slots: BTreeMap<String, (Instant, ActSlot)>,
}

/// The base of a watching request's key (method and params).
fn base(method: &str, p: &Option<Value>) -> String {
    format!(
        "{method} {}",
        p.as_ref().map_or_else(String::new, Value::to_string)
    )
}

/// Frames counted in `First`, so the watching handlers can tell "the same
/// request in a new frame" from "an identical second request".
#[derive(Resource, Default)]
struct WatchFrame(u64);

fn count_frame(mut frame: ResMut<WatchFrame>) {
    frame.0 = frame.0.wrapping_add(1);
}

/// The state key of this run of a watching handler.
fn key(world: &mut World, method: &str, p: &Option<Value>) -> String {
    let base = base(method, p);
    let frame = world.get_resource_or_init::<WatchFrame>().0;
    let mut w = world.get_resource_or_init::<Watches>();
    if w.frame != frame {
        w.frame = frame;
        w.seen.clear();
    }
    let n = w.seen.entry(base.clone()).or_insert(0);
    *n += 1;
    format!("{base} #{n}")
}

fn long_poll(In(p): In<Option<Value>>, world: &mut World) -> BrpResult<Option<Value>> {
    let params = api::parse_events(p.clone()).map_err(to_brp)?;
    let now = wall_now();
    let k = key(world, "agent/events", &p);
    let timeout = Duration::from_millis(api::events_timeout_ms(world, &params));
    let first = {
        let mut w = world.get_resource_or_init::<Watches>();
        w.entries
            .retain(|_, (_, seen, _)| now.duration_since(*seen) < FORGET_AFTER);
        let e = w.entries.entry(k.clone()).or_insert((now, now, 0));
        e.1 = now;
        e.0
    };
    let ready = api::events_ready(world, &params);
    if ready || now.duration_since(first) >= timeout {
        world.resource_mut::<Watches>().entries.remove(&k);
        return api::events_result(world, &params, !ready)
            .map(Some)
            .map_err(to_brp);
    }
    Ok(None)
}

fn stream(In(p): In<Option<Value>>, world: &mut World) -> BrpResult<Option<Value>> {
    let mut params = api::parse_events(p.clone()).map_err(to_brp)?;
    let now = wall_now();
    let k = key(world, "agent/events+watch", &p);
    let sent = {
        let mut w = world.get_resource_or_init::<Watches>();
        w.entries
            .retain(|_, (_, seen, _)| now.duration_since(*seen) < FORGET_AFTER);
        let e = w
            .entries
            .entry(k.clone())
            .or_insert((now, now, params.since_seq));
        e.1 = now;
        e.2
    };
    params.since_seq = sent;
    if !api::events_ready(world, &params) {
        return Ok(None);
    }
    let result = api::events_result(world, &params, false).map_err(to_brp)?;
    let next = result["next_seq"].as_u64().unwrap_or(sent);
    if let Some(e) = world.resource_mut::<Watches>().entries.get_mut(&k) {
        e.2 = next;
    }
    Ok(Some(result))
}

/// What a run of [`act_and_wait`] found in its slot.
enum Found {
    Nothing,
    Waiting(Instant, api::ActWait),
    Done(Result<Value, RpcError>),
}

/// Stores the answer of `k` as its tombstone and returns it.
fn finish(
    world: &mut World,
    k: String,
    now: Instant,
    result: Result<Value, RpcError>,
) -> BrpResult<Option<Value>> {
    world
        .resource_mut::<ActWaits>()
        .slots
        .insert(k, (now, ActSlot::Done(result.clone())));
    result.map(Some).map_err(to_brp)
}

/// `agent/act_and_wait` (watching; needs a `nonce`): acts on the first run
/// of its key, then answers once [`api::act_wait_ready`] or at the timeout
/// (a rejected act or `timeout_ms: 0` on the first run). Every later run of
/// the same key gets the stored answer (module docs).
fn act_and_wait(In(p): In<Option<Value>>, world: &mut World) -> BrpResult<Option<Value>> {
    let has_nonce = p
        .as_ref()
        .and_then(|v| v.get("nonce"))
        .is_some_and(|n| !n.is_null());
    if !has_nonce {
        return Err(to_brp(RpcError::invalid(
            "act_and_wait over BRP needs a nonce: add a unique \"nonce\" param per call \
             (it keys the request's state, so a re-run never acts twice)",
        )));
    }
    let now = wall_now();
    let k = base("agent/act_and_wait", &p);
    let found = {
        let mut w = world.get_resource_or_init::<ActWaits>();
        w.slots
            .retain(|_, (last, _)| now.duration_since(*last) < ACT_KEEP);
        match w.slots.get_mut(&k) {
            None => Found::Nothing,
            Some((last, slot)) => {
                *last = now;
                match slot {
                    ActSlot::Waiting { first, wait } => Found::Waiting(*first, wait.clone()),
                    ActSlot::Done(r) => Found::Done(r.clone()),
                }
            }
        }
    };
    let (first, wait) = match found {
        Found::Done(r) => return r.map(Some).map_err(to_brp),
        Found::Waiting(first, wait) => (first, wait),
        Found::Nothing => {
            // Bad params act on nothing: no tombstone needed.
            let wait = api::act_and_wait_start(world, p).map_err(to_brp)?;
            if wait.answers_at_once() {
                let r = api::act_wait_result(world, &wait, false);
                return finish(world, k, now, r);
            }
            world.resource_mut::<ActWaits>().slots.insert(
                k.clone(),
                (
                    now,
                    ActSlot::Waiting {
                        first: now,
                        wait: wait.clone(),
                    },
                ),
            );
            (now, wait)
        }
    };
    let ready = api::act_wait_ready(world, &wait);
    if ready || now.duration_since(first) >= Duration::from_millis(wait.timeout_ms) {
        let r = api::act_wait_result(world, &wait, !ready);
        return finish(world, k, now, r);
    }
    Ok(None)
}

/// Registers the agent methods on BRP (after `NpcRemotePlugin`).
pub struct AgentRemotePlugin;

impl Plugin for AgentRemotePlugin {
    fn build(&self, app: &mut App) {
        for &m in &api::METHODS {
            if m == "agent/events" || m == "agent/act_and_wait" {
                continue;
            }
            app.add_remote_method(
                m,
                move |In(p): In<Option<Value>>, world: &mut World| -> BrpResult {
                    api::call(world, m, p).map_err(to_brp)
                },
            );
        }
        for m in ["world/load_scenario", "world/reset"] {
            app.add_remote_method(
                m,
                move |In(_): In<Option<Value>>, _world: &mut World| -> BrpResult {
                    Err(to_brp(RpcError::new(
                        codes::NEEDS_SESSION,
                        format!("{m} needs the session host (npcctl serve)"),
                    )))
                },
            );
        }
        app.init_resource::<WatchFrame>()
            .init_resource::<ActWaits>()
            .add_systems(First, count_frame)
            .add_remote_watching_method("agent/events", long_poll)
            .add_remote_watching_method("agent/events+watch", stream)
            .add_remote_watching_method("agent/act_and_wait", act_and_wait);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_requests_in_one_frame_get_separate_state() {
        let mut world = World::new();
        world.init_resource::<WatchFrame>();
        let p = Some(serde_json::json!({"unit": 1}));
        let a = key(&mut world, "agent/events+watch", &p);
        let b = key(&mut world, "agent/events+watch", &p);
        assert_ne!(a, b, "two identical streams in one frame");
        let other = key(
            &mut world,
            "agent/events+watch",
            &Some(serde_json::json!({"unit": 2})),
        );
        assert!(other.ends_with("#1"), "{other}");
        // Next frame: the same two requests map to the same two slots.
        world.resource_mut::<WatchFrame>().0 += 1;
        assert_eq!(key(&mut world, "agent/events+watch", &p), a);
        assert_eq!(key(&mut world, "agent/events+watch", &p), b);
        // A nonce separates them for good.
        let n = key(
            &mut world,
            "agent/events",
            &Some(serde_json::json!({"unit": 1, "nonce": "x"})),
        );
        assert!(n.contains("nonce"), "{n}");
    }

    fn idle_start(_: &mut World, _: Entity, _: &npc_ai::ParamValue) -> npc_ai::Status {
        npc_ai::Status::Running
    }
    fn idle_stop(_: &mut World, _: Entity, _: npc_ai::StopReason) {}

    /// A toy world: unit 1 (idle) attached to controller `c`, Direct.
    fn toy() -> App {
        use npc_kernel::registry::KernelAppExt;
        let mut app = App::new();
        app.add_plugins(npc_kernel::NpcKernelPlugin::default())
            .register_action("Idle", idle_start, idle_stop)
            .register_tree("toy", |b| {
                b.leaf("Idle", "Idle", ());
            })
            .add_plugins(crate::AgentIoPlugin::default());
        npc_kernel::ensure_frozen(app.world_mut());
        let tree = app
            .world()
            .resource::<npc_ai::AiRegistry>()
            .tree_id("toy")
            .unwrap();
        let world = app.world_mut();
        npc_kernel::spawn_sim(world, npc_ai::Brain { tree });
        world.init_resource::<WatchFrame>();
        api::call(
            world,
            "agent/attach",
            Some(serde_json::json!({"unit": 1, "controller": "c"})),
        )
        .unwrap();
        npc_kernel::run_ticks(world, 2);
        app
    }

    fn rejections(world: &mut World) -> usize {
        api::call(
            world,
            "agent/events",
            Some(serde_json::json!({"unit": 1, "kinds": "ActionRejected"})),
        )
        .unwrap()["events"]
            .as_array()
            .unwrap()
            .len()
    }

    /// Runs the handler like `bevy_remote` does: once per frame (a tick in
    /// between) until the first `Some`, then `extra` frames more, as the
    /// answered request stays listed until the HTTP side drops it. Returns
    /// the first answer and every later one.
    fn drive(world: &mut World, p: &Value, extra: usize) -> (Value, Vec<Value>) {
        let mut first = None;
        for _ in 0..60 {
            world.resource_mut::<WatchFrame>().0 += 1;
            if let Some(v) = act_and_wait(In(Some(p.clone())), world).unwrap() {
                first = Some(v);
                break;
            }
            npc_kernel::run_ticks(world, 1);
        }
        let first = first.expect("answered");
        let mut again = Vec::new();
        for _ in 0..extra {
            world.resource_mut::<WatchFrame>().0 += 1;
            npc_kernel::run_ticks(world, 1);
            again.extend(act_and_wait(In(Some(p.clone())), world).unwrap());
        }
        (first, again)
    }

    #[test]
    fn act_and_wait_never_acts_twice_when_re_run_after_its_answer() {
        let mut app = toy();
        let world = app.world_mut();
        // A normal wait, a timeout_ms: 0 one and a rejected act, each run
        // for 5 more frames after its answer.
        for (p, acts, rejected) in [
            (
                serde_json::json!({"unit": 1, "controller": "c", "verb": "wait",
                                   "args": {"ticks": 5}, "timeout_ms": 60000, "nonce": "a"}),
                1,
                0,
            ),
            (
                serde_json::json!({"unit": 1, "controller": "c", "verb": "wait",
                                   "args": {"ticks": 5}, "timeout_ms": 0, "nonce": "b"}),
                1,
                0,
            ),
            (
                serde_json::json!({"unit": 1, "controller": "x", "verb": "wait", "nonce": "c"}),
                0,
                1,
            ),
        ] {
            let (log, rej) = (api::command_log(world).len(), rejections(world));
            let (first, again) = drive(world, &p, 5);
            // Let a submitted act apply before counting.
            npc_kernel::run_ticks(world, 1);
            assert_eq!(api::command_log(world).len(), log + acts, "{p}: {first}");
            assert_eq!(rejections(world), rej + rejected, "{p}: {first}");
            assert_eq!(again.len(), 5, "{p}: every re-run is answered");
            for v in &again {
                assert_eq!(v, &first, "{p}: the stored answer");
            }
        }
        let normal = &world.resource::<ActWaits>().slots;
        assert_eq!(normal.len(), 3, "three tombstones");
    }

    #[test]
    fn act_and_wait_answers_when_idle() {
        let mut app = toy();
        let world = app.world_mut();
        let p = serde_json::json!({"unit": 1, "controller": "c", "verb": "wait",
                                   "args": {"ticks": 5}, "nonce": 1});
        let (v, _) = drive(world, &p, 0);
        assert_eq!(v["act"]["accepted"], true, "{v}");
        assert_eq!(v["events"]["idle"], true, "{v}");
        assert_eq!(v["observation"]["unit"], 1, "{v}");
        // A rejected act answers on the first run.
        world.resource_mut::<WatchFrame>().0 += 1;
        let r = act_and_wait(
            In(Some(
                serde_json::json!({"unit": 1, "controller": "x", "verb": "wait", "nonce": 2}),
            )),
            world,
        )
        .unwrap()
        .expect("at once");
        assert_eq!(r["act"]["reason"], "not_controller", "{r}");
    }

    #[test]
    fn act_and_wait_over_brp_needs_a_nonce() {
        let mut app = toy();
        let world = app.world_mut();
        let before = api::command_log(world).len();
        for p in [
            serde_json::json!({"unit": 1, "controller": "c", "verb": "wait"}),
            serde_json::json!({"unit": 1, "controller": "c", "verb": "wait", "nonce": null}),
        ] {
            let e = act_and_wait(In(Some(p)), world).unwrap_err();
            assert_eq!(i64::from(e.code), codes::INVALID_PARAMS, "{e:?}");
            assert!(e.message.contains("needs a nonce"), "{}", e.message);
        }
        npc_kernel::run_ticks(world, 1);
        assert_eq!(api::command_log(world).len(), before, "nothing acted");
    }
}
