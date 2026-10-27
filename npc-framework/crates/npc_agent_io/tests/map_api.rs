//! Map methods, observations and atomic landmark orders through the same API
//! used by BRP/session hosts. The toy world has no perception dependency.
use bevy_app::App;
use bevy_ecs::prelude::*;
use npc_agent_io::map::MapEnabled;
use npc_agent_io::notes::{NotesClock, NotesDirs};
use npc_agent_io::{
    ActionPlan, ActionRequest, AgentCtx, AgentIoAppExt, AgentIoPlugin, Rejection, api, registration,
};
use npc_ai::{AiRegistry, Brain, ParamValue, Status, StopReason};
use npc_kernel::commands::{CommandLog, CommandQueue, SimCommand};
use npc_kernel::hash::state_hash;
use npc_kernel::orders::{CurrentOrder, Order};
use npc_kernel::registry::KernelAppExt;
use npc_kernel::{NpcKernelPlugin, SimId, SimPos, run_ticks, spawn_sim};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::PathBuf;

fn idle(_: &mut World, _: Entity, _: &ParamValue) -> Status {
    Status::Running
}
fn stop(_: &mut World, _: Entity, _: StopReason) {}
fn order(_: &World, ctx: &AgentCtx, request: &ActionRequest) -> Result<ActionPlan, Rejection> {
    let pos = serde_json::from_value(request.args["order"]["pos"].clone())
        .map_err(|_| Rejection::new("invalid", "missing Move position"))?;
    Ok(ActionPlan::Commands {
        label: format!("Move to {pos:?}"),
        commands: vec![SimCommand::Order {
            unit: ctx.unit.0,
            order: Order::GoTo { pos },
        }],
    })
}
fn app(tag: &str) -> (App, PathBuf) {
    let dir = std::env::temp_dir().join(format!("npc-map-api-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut app = App::new();
    app.add_plugins(NpcKernelPlugin::default())
        .register_action("Idle", idle, stop)
        .register_tree("toy", |b| {
            b.leaf("Idle", "Idle", ());
        })
        .add_plugins(AgentIoPlugin::default())
        .register_action_handler("order", order);
    npc_kernel::ensure_frozen(app.world_mut());
    let tree = app.world().resource::<AiRegistry>().tree_id("toy").unwrap();
    for _ in 0..2 {
        spawn_sim(
            app.world_mut(),
            (
                Brain { tree },
                SimPos::new(100, 200),
                CurrentOrder::default(),
            ),
        );
    }
    app.insert_resource(NotesDirs(BTreeMap::from([(SimId(1), dir.clone())])))
        .insert_resource(NotesClock(|| "2026-10-27T12:00:00Z".into()));
    call(
        &mut app,
        "agent/attach",
        json!({"unit":1,"controller":"owner"}),
    );
    run_ticks(app.world_mut(), 1);
    (app, dir)
}
fn call(app: &mut App, method: &str, params: Value) -> Value {
    api::call(app.world_mut(), method, Some(params)).unwrap_or_else(|e| panic!("{method}: {e:?}"))
}
fn add(app: &mut App, name: &str, pos: [i32; 2]) -> Value {
    call(
        app,
        "agent/map/add",
        json!({"unit":1,"controller":"owner","name":name,"place":{"pos":pos}}),
    )["landmark"]
        .clone()
}

#[test]
fn here_explicit_update_and_player_share_map_without_changing_hash() {
    let (mut app, dir) = app("placements");
    let before = state_hash(app.world_mut());
    let here=call(&mut app,"agent/map/add",json!({"unit":1,"controller":"owner","name":"Home","place":"here","kind":"shelter","note":"safe"}))["landmark"].clone();
    assert_eq!(here["pos"], json!([100, 200]));
    let player = call(
        &mut app,
        "map/add",
        json!({"unit":1,"name":"Well","place":{"pos":[800,1200]}}),
    );
    assert_eq!(player["accepted"], true);
    let changed = call(
        &mut app,
        "agent/map/update",
        json!({"unit":1,"controller":"owner","id":here["id"],"expected":here["updated"],"name":"Old home","kind":null,"pos":[300,400]}),
    );
    assert_eq!(changed["landmark"]["name"], "Old home");
    assert_eq!(changed["landmark"]["kind"], Value::Null);
    assert_eq!(changed["landmark"]["pos"], json!([300, 400]));
    let stale = call(
        &mut app,
        "map/update",
        json!({"unit":1,"id":here["id"],"expected":here["updated"],"note":"stale"}),
    );
    assert_eq!(stale["reason"], "stale");
    let listed = call(&mut app, "map/list", json!({"unit":1}));
    assert_eq!(listed["landmarks"].as_array().unwrap().len(), 2);
    assert_eq!(state_hash(app.world_mut()), before);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn observation_nearest_five_are_stable_and_shared_with_delta_text_and_act_wait() {
    let (mut app, dir) = app("observe");
    let empty = call(&mut app, "agent/observe", json!({"unit":1}));
    assert!(empty.get("map").is_none());
    for (name, pos) in [
        ("East", [1100, 200]),
        ("North", [100, 1200]),
        ("Far", [100, 9000]),
        ("South", [100, -1800]),
        ("West", [-2900, 200]),
        ("North east", [4100, 4200]),
    ] {
        add(&mut app, name, pos);
    }
    let obs = call(&mut app, "agent/observe", json!({"unit":1,"since_seq":0}));
    let map = obs["map"].as_array().unwrap();
    assert_eq!(map.len(), 5);
    assert_eq!(map[0]["name"], "East");
    assert_eq!(map[0]["distance_mm"], 1000);
    assert_eq!(map[0]["direction"], "E");
    assert_eq!(map[1]["name"], "North");
    assert_eq!(map[1]["direction"], "N");
    assert!(map.iter().all(|entry| entry["name"] != "Far"));
    let text = call(
        &mut app,
        "agent/observe",
        json!({"unit":1,"format":"text","since_seq":0}),
    );
    assert!(
        text["text"]
            .as_str()
            .unwrap()
            .contains("Your map: #1 East 1 m E, #2 North 1 m N")
    );
    let waited = call(
        &mut app,
        "agent/act_and_wait",
        json!({"unit":1,"controller":"owner","verb":"wait","args":{"ticks":1},"timeout_ms":0}),
    );
    assert_eq!(waited["observation"]["map"], obs["map"]);
    app.insert_resource(MapEnabled(false));
    assert!(
        call(&mut app, "agent/observe", json!({"unit":1}))
            .get("map")
            .is_none()
    );
    assert!(
        !call(&mut app, "agent/observe", json!({"unit":1,"format":"text"}))["text"]
            .as_str()
            .unwrap()
            .contains("Your map:")
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn landmark_moves_are_position_only_and_unknown_later_steps_submit_nothing() {
    let (mut app, dir) = app("moves");
    let landmark = add(&mut app, "Gate", [4000, 5000]);
    let before = app.world().resource::<CommandQueue>().pending.len();
    let rejected = call(
        &mut app,
        "agent/act",
        json!({"unit":1,"controller":"owner","steps":[{"verb":"order","args":{"order":{"type":"Move","landmark":landmark["id"]}}},{"verb":"order","args":{"order":{"type":"Move","landmark":999}}}]}),
    );
    assert_eq!(rejected["reason"], "unknown_landmark");
    assert_eq!(app.world().resource::<CommandQueue>().pending.len(), before);
    let accepted = call(
        &mut app,
        "agent/act",
        json!({"unit":1,"controller":"owner","verb":"order","args":{"order":{"type":"Move","landmark":landmark["id"]}}}),
    );
    assert_eq!(accepted["accepted"], true);
    let pending = app
        .world()
        .resource::<CommandQueue>()
        .pending
        .values()
        .last()
        .unwrap();
    assert_eq!(
        serde_json::to_value(pending).unwrap(),
        json!({"type":"Order","unit":1,"order":{"type":"GoTo","pos":[4000,5000]}})
    );
    run_ticks(app.world_mut(), 1);
    let log = serde_json::to_string(&app.world().resource::<CommandLog>().entries).unwrap();
    assert!(!log.contains("landmark") && !log.contains("Gate"));
    app.insert_resource(MapEnabled(false));
    let before = app.world().resource::<CommandQueue>().pending.len();
    let rejected = call(
        &mut app,
        "agent/act_and_wait",
        json!({"unit":1,"controller":"owner","verb":"order","args":{"order":{"type":"Move","landmark":1}},"timeout_ms":0}),
    );
    assert_eq!(rejected["act"]["reason"], "no_map");
    assert_eq!(app.world().resource::<CommandQueue>().pending.len(), before);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn map_registration_is_scoped_and_available_before_control_and_off_does_no_io() {
    let (mut app, dir) = app("registration");
    app.insert_resource(registration::RegistrationRequired);
    registration::set_credential(
        app.world_mut(),
        "my-agent",
        registration::Credential {
            token_digest: registration::token_digest("secret"),
            assigned: SimId(1),
            controller: "my-agent".into(),
        },
    );
    let refused = api::call(app.world_mut(), "agent/map/list", Some(json!({}))).unwrap_err();
    assert_eq!(refused.data.unwrap()["reason"], "not_registered");
    let registered = call(
        &mut app,
        "agent/register",
        json!({"agent_id":"my-agent","token":"secret"}),
    );
    let key = &registered["session"];
    let listed = call(&mut app, "agent/map/list", json!({"session":key}));
    assert!(listed.get("unit").is_none());
    let added = call(
        &mut app,
        "agent/map/add",
        json!({"session":key,"name":"Known start","place":"here"}),
    );
    assert_eq!(added["accepted"], true);
    assert!(added.get("unit").is_none());
    let err = api::call(
        app.world_mut(),
        "agent/map/list",
        Some(json!({"session":key,"unit":2})),
    )
    .unwrap_err();
    assert_eq!(err.data.unwrap()["reason"], "not_yours");
    app.insert_resource(MapEnabled(false));
    let before: Vec<_> = std::fs::read_dir(dir.join("map"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    for method in [
        "agent/map/list",
        "agent/map/add",
        "agent/map/update",
        "map/list",
        "map/add",
        "map/update",
    ] {
        let mut params = json!({"session":key,"unit":1,"name":"Hidden","place":"here","id":1,"expected":added["landmark"]["updated"]});
        if method.ends_with("list") {
            params = json!({"session":key,"unit":1});
        }
        if !method.starts_with("agent/") {
            params.as_object_mut().unwrap().remove("session");
        }
        if method.ends_with("add") {
            params.as_object_mut().unwrap().remove("id");
            params.as_object_mut().unwrap().remove("expected");
        }
        if method.ends_with("update") {
            params.as_object_mut().unwrap().remove("place");
        }
        let error = api::call(app.world_mut(), method, Some(params)).unwrap_err();
        assert_eq!(error.data.unwrap()["reason"], "no_map", "{method}");
    }
    let after: Vec<_> = std::fs::read_dir(dir.join("map"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(after, before);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn raw_coordinate_extremes_and_unknown_things_are_rejected_without_panics_or_files() {
    let (mut app, dir) = app("bad-input");
    for place in [
        json!({"pos":[i32::MIN,0]}),
        json!({"pos":[0,i32::MAX]}),
        json!({"thing":2}),
    ] {
        let error = api::call(
            app.world_mut(),
            "map/add",
            Some(json!({"unit":1,"name":"Bad","place":place})),
        )
        .unwrap_err();
        assert!(matches!(
            error.data.unwrap()["reason"].as_str(),
            Some("invalid" | "unknown_thing")
        ));
    }
    assert!(!dir.join("map").exists());
    let _ = std::fs::remove_dir_all(dir);
}
