//! The notes methods (`agent/notes/*` for the unit's controller, `notes/*`
//! for the player) on a toy unit with a real agent folder, and the MCP tool
//! schemas.

use std::collections::BTreeMap;
use std::path::PathBuf;

use bevy_app::App;
use bevy_ecs::prelude::*;
use npc_agent_io::notes::{NotesClock, NotesDirs};
use npc_agent_io::{AgentIoPlugin, RpcError, api, mcp};
use npc_ai::{AiRegistry, Brain, ParamValue, Status, StopReason};
use npc_kernel::registry::KernelAppExt;
use npc_kernel::{NpcKernelPlugin, SimId, run_ticks, spawn_sim};
use serde_json::{Value, json};

const DATE: &str = "2026-10-25T12:00:00Z";

fn fixed_date() -> String {
    DATE.to_owned()
}

fn idle_start(_: &mut World, _: Entity, _: &ParamValue) -> Status {
    Status::Running
}
fn idle_stop(_: &mut World, _: Entity, _: StopReason) {}

/// A fresh, empty agent folder for one test.
fn folder(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("npc-notes-api-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// A kernel app with the agent plugin and two idle units (1 and 2); unit 1
/// keeps its notes in `dir` when one is given.
fn app(dir: Option<PathBuf>) -> App {
    let mut app = App::new();
    app.add_plugins(NpcKernelPlugin::default())
        .register_action("Idle", idle_start, idle_stop)
        .register_tree("toy", |b| {
            b.leaf("Idle", "Idle", ());
        })
        .add_plugins(AgentIoPlugin::default());
    npc_kernel::ensure_frozen(app.world_mut());
    let tree = app.world().resource::<AiRegistry>().tree_id("toy").unwrap();
    for _ in 0..2 {
        spawn_sim(app.world_mut(), Brain { tree });
    }
    if let Some(dir) = dir {
        app.world_mut()
            .insert_resource(NotesDirs(BTreeMap::from([(SimId(1), dir)])));
    }
    app.world_mut().insert_resource(NotesClock(fixed_date));
    app
}

fn try_call(app: &mut App, m: &str, p: Value) -> Result<Value, RpcError> {
    api::call(app.world_mut(), m, Some(p))
}

fn call(app: &mut App, m: &str, p: Value) -> Value {
    try_call(app, m, p).unwrap_or_else(|e| panic!("{m}: {e:?}"))
}

fn attach(app: &mut App, controller: &str, mode: &str) {
    call(
        app,
        "agent/attach",
        json!({"unit": 1, "controller": controller, "mode": mode, "force": true}),
    );
    run_ticks(app.world_mut(), 1);
}

#[test]
fn agent_writes_need_the_controller() {
    let dir = folder("controller");
    let mut app = app(Some(dir.clone()));
    let add = |c: &str| json!({"unit": 1, "controller": c, "book": "diary", "title": "Woke up", "body": "In the yard."});
    let r = call(&mut app, "agent/notes/add", add("me"));
    assert_eq!(r["accepted"], false, "{r}");
    assert_eq!(r["reason"], "not_attached", "{r}");
    let r = call(
        &mut app,
        "agent/notes/list",
        json!({"unit": 1, "controller": "me", "book": "diary"}),
    );
    assert_eq!(r["reason"], "not_attached", "{r}");
    assert!(!dir.join("diary").exists(), "nothing written");

    attach(&mut app, "me", "Direct");
    let r = call(&mut app, "agent/notes/add", add("intruder"));
    assert_eq!(r["accepted"], false, "{r}");
    assert_eq!(r["reason"], "not_controller", "{r}");
    assert!(r["message"].as_str().unwrap().contains("`me`"), "{r}");

    let tick = app.world().resource::<npc_kernel::SimClock>().tick;
    let r = call(&mut app, "agent/notes/add", add("me"));
    assert_eq!(r["accepted"], true, "{r}");
    assert_eq!(r["book"], "diary");
    let note = &r["note"];
    assert_eq!(note["id"], 1);
    assert_eq!(note["title"], "Woke up");
    assert_eq!(note["created"], json!({"tick": tick, "date": DATE}));
    assert!(
        std::fs::read_to_string(dir.join("diary").join("2026-10-25 Woke up.md"))
            .unwrap()
            .contains("In the yard.")
    );
    let r = call(
        &mut app,
        "agent/notes/update",
        json!({"unit": 1, "controller": "me", "book": "diary", "id": 1,
               "expected": note["updated"], "body": "In the yard, at dawn."}),
    );
    assert_eq!(r["accepted"], true, "{r}");
    let updated = r["note"]["updated"].clone();
    assert_ne!(updated, note["updated"], "every write gives a new version");
    let r = call(
        &mut app,
        "agent/notes/list",
        json!({"unit": 1, "controller": "me", "book": "diary"}),
    );
    assert_eq!(r["notes"].as_array().unwrap().len(), 1, "{r}");
    assert_eq!(r["unreadable"], json!([]), "{r}");
    assert_eq!(r["notes"][0]["body"], "In the yard, at dawn.");
    assert_eq!(r["notes"][0]["updated"], updated);

    // A stale update is a rejection with the current stamp.
    let r = call(
        &mut app,
        "agent/notes/update",
        json!({"unit": 1, "controller": "me", "book": "diary", "id": 1,
               "expected": note["updated"], "title": "Overwrite"}),
    );
    assert_eq!(r["accepted"], false, "{r}");
    assert_eq!(r["reason"], "stale", "{r}");
    assert_eq!(r["current"], updated, "{r}");
    assert!(
        r["message"]
            .as_str()
            .unwrap()
            .contains("changed since you read it"),
        "{r}"
    );

    // An Observer reads its notes but writes nothing, as for act.
    attach(&mut app, "watcher", "Observer");
    let r = call(
        &mut app,
        "agent/notes/list",
        json!({"unit": 1, "controller": "watcher", "book": "diary"}),
    );
    assert_eq!(r["notes"][0]["updated"], updated, "an Observer lists: {r}");
    let r = call(&mut app, "agent/notes/add", add("watcher"));
    assert_eq!(r["accepted"], false, "{r}");
    assert_eq!(r["reason"], "read_only", "{r}");
    let r = call(
        &mut app,
        "agent/notes/update",
        json!({"unit": 1, "controller": "watcher", "book": "diary", "id": 1,
               "expected": updated, "title": "Watched"}),
    );
    assert_eq!(r["accepted"], false, "{r}");
    assert_eq!(r["reason"], "read_only", "{r}");
    let r = call(&mut app, "notes/list", json!({"unit": 1, "book": "diary"}));
    assert_eq!(
        r["notes"].as_array().unwrap().len(),
        1,
        "nothing added: {r}"
    );
    assert_eq!(r["notes"][0]["updated"], updated, "nothing changed: {r}");
}

#[test]
fn player_methods_need_no_controller() {
    let dir = folder("player");
    let mut app = app(Some(dir.clone()));
    let r = call(
        &mut app,
        "notes/add",
        json!({"unit": 1, "book": "proposals", "title": "Lamps at the gate",
               "body": "It is dark at night.", "prompted_by": "the night walk"}),
    );
    assert_eq!(r["accepted"], true, "{r}");
    let note = r["note"].clone();
    assert_eq!(note["status"], "new");
    assert_eq!(note["prompted_by"], "the night walk");
    let r = call(
        &mut app,
        "notes/update",
        json!({"unit": 1, "book": "proposals", "id": note["id"],
               "expected": note["updated"], "status": "liked"}),
    );
    assert_eq!(r["accepted"], true, "{r}");
    assert_eq!(r["note"]["status"], "liked");
    let r = call(
        &mut app,
        "notes/list",
        json!({"unit": 1, "book": "PROPOSALS"}),
    );
    assert_eq!(r["notes"][0]["status"], "liked", "{r}");
    assert_eq!(r["unit"], 1);
    assert!(r["tick"].is_u64() && r["epoch"].is_u64(), "stamped: {r}");
    assert!(
        std::fs::read_dir(dir.join("proposals")).unwrap().any(|e| e
            .unwrap()
            .file_name()
            .to_string_lossy()
            == "Lamps at the gate.md"),
        "a proposal file"
    );
    // Bad input is an RPC error naming the problem.
    let e = try_call(
        &mut app,
        "notes/add",
        json!({"unit": 1, "book": "memory", "title": "x".repeat(201), "body": ""}),
    )
    .unwrap_err();
    assert_eq!(e.code, api::codes::INVALID_PARAMS);
    assert!(e.message.contains("at most 200"), "{e:?}");
    let e = try_call(
        &mut app,
        "notes/list",
        json!({"unit": 1, "book": "letters"}),
    )
    .unwrap_err();
    assert!(e.message.contains("diary, memory, proposals"), "{e:?}");
    let e = try_call(
        &mut app,
        "notes/update",
        json!({"unit": 1, "book": "memory", "id": 9, "expected": {"tick": 0, "date": ""}, "body": "b"}),
    )
    .unwrap_err();
    assert!(e.message.contains("no note with id 9"), "{e:?}");
}

#[test]
fn list_names_unreadable_files() {
    let dir = folder("unreadable");
    std::fs::create_dir_all(dir.join("memory")).unwrap();
    std::fs::write(
        dir.join("memory").join("Typed by hand.md"),
        "No frontmatter.
",
    )
    .unwrap();
    let mut app = app(Some(dir.clone()));
    let r = call(&mut app, "notes/list", json!({"unit": 1, "book": "memory"}));
    assert_eq!(r["notes"], json!([]), "{r}");
    assert_eq!(r["unreadable"][0]["file"], "memory/Typed by hand.md", "{r}");
    assert!(
        r["unreadable"][0]["reason"]
            .as_str()
            .unwrap()
            .contains("no frontmatter"),
        "{r}"
    );
    // The agent sees the same, and the file is never written.
    attach(&mut app, "me", "Direct");
    let r = call(
        &mut app,
        "agent/notes/list",
        json!({"unit": 1, "controller": "me", "book": "memory"}),
    );
    assert_eq!(r["unreadable"].as_array().unwrap().len(), 1, "{r}");
    call(
        &mut app,
        "agent/notes/add",
        json!({"unit": 1, "controller": "me", "book": "memory", "title": "Typed by hand", "body": "x"}),
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("memory").join("Typed by hand.md")).unwrap(),
        "No frontmatter.
"
    );
    assert!(dir.join("memory").join("Typed by hand (2).md").is_file());
}

#[test]
fn agent_cannot_pose_as_player() {
    let mut app = app(Some(folder("pose")));
    attach(&mut app, "me", "Direct");
    let r = call(
        &mut app,
        "agent/notes/add",
        json!({"unit": 1, "controller": "me", "book": "proposals", "title": "Idea", "body": ""}),
    );
    let note = r["note"].clone();
    let e = try_call(
        &mut app,
        "agent/notes/update",
        json!({"unit": 1, "controller": "me", "book": "proposals", "id": note["id"],
               "expected": note["updated"], "status": "pursuing"}),
    )
    .unwrap_err();
    assert_eq!(e.code, api::codes::INVALID_PARAMS, "{e:?}");
    assert!(e.message.contains("status"), "{e:?}");
    let r = call(
        &mut app,
        "notes/list",
        json!({"unit": 1, "book": "proposals"}),
    );
    assert_eq!(r["notes"][0]["status"], "new", "unchanged: {r}");
}

#[test]
fn unit_without_notes_is_an_error() {
    let mut app = app(Some(folder("without")));
    let e = try_call(&mut app, "notes/list", json!({"unit": 2, "book": "diary"})).unwrap_err();
    assert_eq!(e.code, api::codes::INVALID_PARAMS);
    assert_eq!(e.message, "unit 2 keeps no notes");
    // No mapping at all.
    let mut bare = self::app(None);
    let e = try_call(&mut bare, "notes/list", json!({"unit": 1, "book": "diary"})).unwrap_err();
    assert_eq!(e.message, "unit 1 keeps no notes");
    attach(&mut bare, "me", "Direct");
    let e = try_call(
        &mut bare,
        "agent/notes/list",
        json!({"unit": 1, "controller": "me", "book": "diary"}),
    )
    .unwrap_err();
    assert_eq!(e.message, "unit 1 keeps no notes");
    // An unknown unit is an unknown unit.
    let e = try_call(
        &mut bare,
        "agent/notes/list",
        json!({"unit": 99, "controller": "me", "book": "diary"}),
    )
    .unwrap_err();
    assert_eq!(e.code, api::codes::UNKNOWN_UNIT);
}

#[test]
fn note_writes_leave_the_state_hash_unchanged() {
    let mut a = app(Some(folder("hash")));
    let mut b = app(None);
    attach(&mut a, "me", "Direct");
    attach(&mut b, "me", "Direct");
    let before = npc_kernel::hash::state_hash(a.world_mut());
    assert_eq!(before, npc_kernel::hash::state_hash(b.world_mut()));
    let r = call(
        &mut a,
        "agent/notes/add",
        json!({"unit": 1, "controller": "me", "book": "memory", "title": "Gate", "body": "North."}),
    );
    let note = r["note"].clone();
    call(
        &mut a,
        "notes/update",
        json!({"unit": 1, "book": "memory", "id": 1, "expected": note["updated"], "body": "South."}),
    );
    call(&mut a, "notes/list", json!({"unit": 1, "book": "memory"}));
    assert_eq!(npc_kernel::hash::state_hash(a.world_mut()), before);
    run_ticks(a.world_mut(), 20);
    run_ticks(b.world_mut(), 20);
    assert_eq!(
        npc_kernel::hash::state_hash(a.world_mut()),
        npc_kernel::hash::state_hash(b.world_mut()),
        "the world without notes runs the same"
    );
}

#[test]
fn notes_methods_are_listed() {
    for m in [
        "agent/notes/list",
        "agent/notes/add",
        "agent/notes/update",
        "notes/list",
        "notes/add",
        "notes/update",
    ] {
        assert!(api::METHODS.contains(&m), "{m}");
    }
}

#[test]
fn mcp_lists_every_field() {
    let tools = mcp::tools();
    let tool = |name: &str| -> Value {
        tools
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["name"] == name)
            .unwrap_or_else(|| panic!("tool {name}"))
            .clone()
    };
    let props = |name: &str| -> Vec<String> {
        tool(name)["inputSchema"]["properties"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect()
    };
    assert!(props("act").contains(&"steps".to_owned()));
    assert!(props("observe").contains(&"brief".to_owned()));
    for f in ["until_idle", "dedupe", "min_events"] {
        assert!(props("events").contains(&f.to_owned()), "events.{f}");
    }
    let names: Vec<&str> = tools
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        [
            "observe",
            "act",
            "act_and_wait",
            "cancel",
            "events",
            "attach",
            "list_units",
            "notes_list",
            "notes_add",
            "notes_update"
        ]
    );
    for (name, fields, required) in [
        (
            "notes_list",
            &["unit", "controller", "book"][..],
            &["unit", "book"][..],
        ),
        (
            "notes_add",
            &["unit", "controller", "book", "title", "body", "prompted_by"][..],
            &["unit", "book", "title", "body"][..],
        ),
        (
            "notes_update",
            &[
                "unit",
                "controller",
                "book",
                "id",
                "expected",
                "title",
                "body",
            ][..],
            &["unit", "book", "id", "expected"][..],
        ),
    ] {
        let mut got = props(name);
        got.sort();
        let mut want: Vec<String> = fields.iter().map(|f| (*f).to_owned()).collect();
        want.sort();
        assert_eq!(got, want, "{name}");
        assert_eq!(
            tool(name)["inputSchema"]["required"],
            json!(required),
            "{name}"
        );
    }
    assert!(
        !props("notes_update").contains(&"status".to_owned()),
        "agents never set a status"
    );
    let act = tool("act")["description"].as_str().unwrap().to_owned();
    assert!(
        act.contains(r#"verb: "interact", args: {affordance: "VERB@ID"}"#),
        "act says how to walk to a thing and use it: {act}"
    );
    assert!(act.contains("steps"), "{act}");
}
