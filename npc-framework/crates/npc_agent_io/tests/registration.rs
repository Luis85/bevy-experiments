//! Host-only registration on a small content-free world.
use std::collections::BTreeMap;

use bevy_app::App;
use bevy_ecs::prelude::*;
use npc_agent_io::notes::{NotesClock, NotesDirs};
use npc_agent_io::registration::{self, Credential, RegistrationRequired};
use npc_agent_io::{AgentIoPlugin, ExternalController, ExternalQueue, RpcError, api};
use npc_ai::{AiRegistry, Brain, ParamValue, Status, StopReason};
use npc_kernel::commands::CommandQueue;
use npc_kernel::registry::KernelAppExt;
use npc_kernel::{NpcKernelPlugin, SimId, run_ticks, spawn_sim};
use serde_json::{Value, json};

const ID: &str = "test-agent";
const TOKEN: &str = "registration-test-secret-never-journaled";
fn idle(_: &mut World, _: Entity, _: &ParamValue) -> Status {
    Status::Running
}
fn stop(_: &mut World, _: Entity, _: StopReason) {}
fn build_app(locked: bool) -> App {
    let mut app = App::new();
    app.add_plugins(NpcKernelPlugin::default())
        .register_action("Idle", idle, stop)
        .register_tree("toy", |b| {
            b.leaf("Idle", "Idle", ());
        })
        .add_plugins(AgentIoPlugin::default());
    npc_kernel::ensure_frozen(app.world_mut());
    let tree = app.world().resource::<AiRegistry>().tree_id("toy").unwrap();
    for _ in 0..2 {
        spawn_sim(app.world_mut(), Brain { tree });
    }
    if locked {
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
        registration::set_world_name(app.world_mut(), "A world");
    }
    app
}
fn call(app: &mut App, method: &str, params: Value) -> Result<Value, RpcError> {
    api::call(app.world_mut(), method, Some(params))
}
fn register(app: &mut App) -> String {
    call(app, "agent/register", json!({"agent_id":ID,"token":TOKEN})).unwrap()["session"]
        .as_str()
        .unwrap()
        .into()
}
fn reason(error: RpcError) -> Value {
    error.data.unwrap()["reason"].clone()
}

#[test]
fn unlocked_worlds_are_unchanged() {
    let mut app = build_app(false);
    assert!(call(&mut app, "agent/list_units", json!({})).is_ok());
    assert!(
        call(
            &mut app,
            "agent/attach",
            json!({"unit":1,"controller":"legacy"})
        )
        .is_ok()
    );
}

#[test]
fn calls_before_register_are_refused() {
    let mut app = build_app(true);
    for method in api::METHODS
        .into_iter()
        .filter(|m| m.starts_with("agent/") && *m != "agent/register")
    {
        assert_eq!(
            reason(call(&mut app, method, json!({})).unwrap_err()),
            "not_registered",
            "{method}"
        );
    }
}

#[test]
fn register_enter_take_control_is_pending_and_idempotent() {
    let mut app = build_app(true);
    let session = register(&mut app);
    let entered = call(&mut app, "agent/enter_world", json!({"session":session})).unwrap();
    assert!(entered.get("unit").is_none());
    assert_eq!(
        reason(call(&mut app, "agent/observe", json!({"session":session})).unwrap_err()),
        "not_in_control"
    );
    assert_eq!(
        reason(call(&mut app, "agent/list_units", json!({"session":session})).unwrap_err()),
        "not_in_control"
    );
    let a = call(&mut app, "agent/take_control", json!({"session":session})).unwrap();
    assert_eq!(a["pending"], true);
    let count = app.world().resource::<CommandQueue>().pending.len();
    let b = call(&mut app, "agent/take_control", json!({"session":session})).unwrap();
    assert_eq!(b["pending"], true);
    assert_eq!(app.world().resource::<CommandQueue>().pending.len(), count);
    run_ticks(app.world_mut(), 1);
    let ready = call(&mut app, "agent/take_control", json!({"session":session})).unwrap();
    assert_eq!(ready["in_control"], true);
    let observed = call(&mut app, "agent/observe", json!({"session":session})).unwrap();
    assert_eq!(observed["unit"], 1);
    let units = call(&mut app, "agent/list_units", json!({"session":session})).unwrap();
    assert_eq!(units["units"].as_array().unwrap().len(), 1);
    assert!(
        app.world_mut()
            .query::<&ExternalController>()
            .iter(app.world())
            .all(|c| c.id == ID)
    );
}

#[test]
fn credentials_and_assignment_cannot_be_bypassed() {
    let mut app = build_app(true);
    let e = call(
        &mut app,
        "agent/register",
        json!({"agent_id":ID,"token":"wrong"}),
    )
    .unwrap_err();
    assert_eq!(e.data.as_ref().unwrap()["reason"], "bad_credentials");
    assert!(e.data.unwrap()["retry_after_ms"].as_u64().unwrap() >= 1000);
    let throttled = call(
        &mut app,
        "agent/register",
        json!({"agent_id":ID,"token":"wrong"}),
    )
    .unwrap_err();
    assert!(throttled.data.unwrap()["retry_after_ms"].as_u64().unwrap() > 0);
    let mut app = build_app(true);
    let session = register(&mut app);
    for method in ["agent/take_control", "agent/attach", "agent/notes/list"] {
        assert_eq!(
            reason(
                call(
                    &mut app,
                    method,
                    json!({"session":session,"unit":2,"book":"diary"})
                )
                .unwrap_err()
            ),
            "not_yours"
        );
    }
}

#[test]
fn reregister_revokes_parked_events_and_new_host_sessions_are_unknown() {
    let mut app = build_app(true);
    let old = register(&mut app);
    call(&mut app, "agent/take_control", json!({"session":old})).unwrap();
    run_ticks(app.world_mut(), 1);
    let params = api::parse_events_for(
        app.world_mut(),
        Some(json!({"session":old,"timeout_ms":1000})),
    )
    .unwrap();
    let new = register(&mut app);
    assert_ne!(old, new);
    assert!(api::events_ready(app.world_mut(), &params));
    let e = api::events_result(app.world_mut(), &params, false).unwrap_err();
    assert_eq!(e.data.as_ref().unwrap()["reason"], "unknown_session");
    assert_eq!(e.data.unwrap()["revoked"], true);
    let mut fresh = build_app(true);
    let e = call(&mut fresh, "agent/enter_world", json!({"session":old})).unwrap_err();
    assert_ne!(e.data.unwrap()["revoked"], true);
}

#[test]
fn evicted_old_sessions_stay_revoked_on_their_original_host() {
    let mut app = build_app(true);
    let old = register(&mut app);
    for _ in 0..4100 {
        let _ = register(&mut app);
    }
    let e = call(&mut app, "agent/enter_world", json!({"session":old})).unwrap_err();
    assert_eq!(
        e.data.unwrap()["revoked"],
        true,
        "an evicted old key must never reconnect and steal control"
    );
    let mut other = build_app(true);
    let _ = register(&mut other);
    let e = call(&mut other, "agent/enter_world", json!({"session":old})).unwrap_err();
    assert_ne!(
        e.data.unwrap()["revoked"],
        true,
        "a different host allows transparent re-registration"
    );
}

#[test]
fn auth_metadata_does_not_change_state_or_content_hashes() {
    let mut app = build_app(true);
    let content = npc_kernel::registry::content_hash(app.world_mut());
    let state = npc_kernel::hash::state_hash(app.world_mut());
    let session = register(&mut app);
    let _ = call(&mut app, "agent/enter_world", json!({"session":session})).unwrap();
    assert_eq!(npc_kernel::registry::content_hash(app.world_mut()), content);
    assert_eq!(npc_kernel::hash::state_hash(app.world_mut()), state);
    let mut p = Some(json!({"session":session}));
    let permit = registration::authorize(app.world_mut(), "agent/enter_world", &mut p)
        .unwrap()
        .unwrap();
    assert!(!format!("{permit:?}").contains(&session));
    assert!(
        !format!(
            "{:?}",
            app.world().resource::<registration::AgentSessions>()
        )
        .contains(&session)
    );
}

#[derive(Resource)]
struct Rotate(bool);
fn reload_rotated_token(world: &mut World) {
    if world.get_resource::<Rotate>().is_some_and(|r| r.0) {
        world.resource_mut::<Rotate>().0 = false;
        registration::set_credential(
            world,
            ID,
            Credential {
                token_digest: registration::token_digest("rotated-token"),
                assigned: SimId(1),
                controller: ID.into(),
            },
        );
    }
}

#[test]
fn rotated_token_is_revoked_by_the_next_call() {
    let mut app = build_app(true);
    app.insert_resource(Rotate(false));
    registration::set_reload(app.world_mut(), reload_rotated_token);
    let old = register(&mut app);
    app.world_mut().resource_mut::<Rotate>().0 = true;
    let e = call(&mut app, "agent/enter_world", json!({"session":old})).unwrap_err();
    assert_eq!(e.data.unwrap()["revoked"], true);
    let new = call(
        &mut app,
        "agent/register",
        json!({"agent_id":ID,"token":"rotated-token"}),
    )
    .unwrap();
    assert_ne!(new["session"], old);
    assert_eq!(
        reason(
            call(
                &mut app,
                "agent/register",
                json!({"agent_id":ID,"token":TOKEN})
            )
            .unwrap_err()
        ),
        "bad_credentials"
    );
}

#[test]
fn release_clears_the_running_action_and_hands_back_idle_control() {
    let mut app = build_app(true);
    let session = register(&mut app);
    call(&mut app, "agent/take_control", json!({"session":session})).unwrap();
    run_ticks(app.world_mut(), 1);
    let act = call(
        &mut app,
        "agent/act",
        json!({"session":session,"verb":"wait","args":{"ticks":1000}}),
    )
    .unwrap();
    assert_eq!(act["accepted"], true);
    run_ticks(app.world_mut(), 1);
    let entity = app
        .world()
        .resource::<npc_kernel::SimIndex>()
        .get(SimId(1))
        .unwrap();
    assert!(app.world().get::<ExternalQueue>(entity).unwrap().busy());
    call(&mut app, "agent/release", json!({"session":session})).unwrap();
    assert_eq!(
        reason(call(&mut app, "agent/observe", json!({"session":session})).unwrap_err()),
        "not_in_control"
    );
    run_ticks(app.world_mut(), 1);
    assert!(app.world().get::<ExternalController>(entity).is_none());
    assert!(
        app.world()
            .get::<ExternalQueue>(entity)
            .is_none_or(|q| q.current.is_none() && q.pending.is_empty())
    );
}

#[test]
fn release_and_retake_in_one_tick_do_not_resume_old_work() {
    let mut app = build_app(true);
    let session = register(&mut app);
    call(&mut app, "agent/take_control", json!({"session":session})).unwrap();
    run_ticks(app.world_mut(), 1);
    let act = call(
        &mut app,
        "agent/act",
        json!({"session":session,"verb":"wait","args":{"ticks":1000}}),
    )
    .unwrap();
    assert_eq!(act["accepted"], true);
    run_ticks(app.world_mut(), 1);
    let entity = app
        .world()
        .resource::<npc_kernel::SimIndex>()
        .get(SimId(1))
        .unwrap();
    assert!(app.world().get::<ExternalQueue>(entity).unwrap().busy());
    call(&mut app, "agent/release", json!({"session":session})).unwrap();
    let retake = call(&mut app, "agent/take_control", json!({"session":session})).unwrap();
    assert_eq!(retake["pending"], true);
    run_ticks(app.world_mut(), 1);
    assert!(app.world().get::<ExternalController>(entity).is_some());
    assert!(
        app.world()
            .get::<ExternalQueue>(entity)
            .is_none_or(|q| q.current.is_none() && q.pending.is_empty())
    );
    assert_eq!(
        call(&mut app, "agent/take_control", json!({"session":session})).unwrap()["in_control"],
        true
    );
}

#[test]
fn enabling_the_lock_rejects_a_legacy_parked_poll() {
    let mut app = build_app(false);
    let legacy = api::parse_events(Some(json!({"unit":1,"timeout_ms":1000}))).unwrap();
    app.insert_resource(RegistrationRequired);
    assert!(api::events_ready(app.world_mut(), &legacy));
    assert_eq!(
        reason(api::events_result(app.world_mut(), &legacy, false).unwrap_err()),
        "not_registered"
    );
}

#[test]
fn registered_notes_work_before_control_and_tokens_stay_out_of_state() {
    let mut app = build_app(true);
    let dir = std::env::temp_dir().join(format!("registration-notes-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    app.insert_resource(NotesDirs(BTreeMap::from([(SimId(1), dir.clone())])));
    app.insert_resource(NotesClock(|| "2026-10-27T12:00:00Z".into()));
    let session = register(&mut app);
    let note = call(
        &mut app,
        "agent/notes/add",
        json!({"session":session,"book":"diary","title":"Arrival","body":"Here"}),
    )
    .unwrap();
    assert_eq!(note["accepted"], true);
    assert!(note.get("unit").is_none());
    assert!(
        call(
            &mut app,
            "agent/notes/list",
            json!({"session":session,"book":"diary"})
        )
        .is_ok()
    );
    let journal = format!("{:?}", app.world().resource::<npc_kernel::Journal>());
    assert!(!journal.contains(TOKEN));
    let snapshot = npc_kernel::snapshot::save(app.world_mut()).unwrap();
    assert!(!serde_json::to_string(&snapshot).unwrap().contains(TOKEN));
    registration::clear_credentials(app.world_mut());
    let e = call(
        &mut app,
        "agent/notes/list",
        json!({"session":session,"book":"diary"}),
    )
    .unwrap_err();
    assert_eq!(e.data.unwrap()["revoked"], true);
    let _ = std::fs::remove_dir_all(dir);
}
