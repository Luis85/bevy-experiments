//! A successor's authenticated notebook access must not depend on a later host frame.
use island_content::{
    explorer::{
        home,
        host::{self, ExplorerSession, HostArgs},
    },
    village::VillageState,
};
use npc_framework::agent_io::{api, notes::NotesDirs};
use npc_framework::prelude::*;
use serde_json::{Value, json};
use std::path::PathBuf;

struct TemporaryHome(PathBuf, PathBuf);
impl TemporaryHome {
    fn new() -> Self {
        let parent = std::env::temp_dir().canonicalize().unwrap();
        let path = parent.join(format!("island-notes-lifecycle-{}", std::process::id()));
        std::fs::create_dir(&path).unwrap();
        Self(path, parent)
    }
}
impl Drop for TemporaryHome {
    fn drop(&mut self) {
        if let Ok(path) = self.0.canonicalize()
            && path.is_absolute()
            && path.starts_with(&self.1)
            && path != self.1
        {
            let _ = std::fs::remove_dir_all(path);
        }
    }
}

fn call(app: &mut bevy_app::App, method: &str, params: Value) -> Value {
    let tick = app.world().resource::<SimClock>().tick;
    let bindings = app
        .world()
        .get_resource::<NotesDirs>()
        .map_or(0, |dirs| dirs.0.len());
    api::call(app.world_mut(), method, Some(params)).unwrap_or_else(|error| {
        panic!("lifecycle method {method} failed at tick {tick} with {bindings} notebook bindings (RPC code {})", error.code)
    })
}

#[test]
fn successor_notebooks_are_available_at_registration_before_control_or_host_frame() {
    let temp = TemporaryHome::new();
    let args = HostArgs {
        home: temp.0.clone(),
        agent: "Lifecycle".into(),
        world: Some("Island".into()),
        resume: false,
        on_mismatch: None,
        preset: Some("human".into()),
        actors_dir: Some(temp.0.join("actors")),
        world_preset: Some("island_village".into()),
        worlds_dir: Some(temp.0.join("worlds")),
    };
    let (mut app, _, dir) = host::build_world(&args, 7, "2026-10-30T12:00:00Z", |_| {}).unwrap();
    let meta = app.world().resource::<ExplorerSession>().meta.clone();
    let old = app.world().resource::<VillageState>().played;
    host::insert_notes(app.world_mut(), old, &dir, || "2026-10-30T12:00:00Z".into());
    let session = call(
        &mut app,
        "agent/register",
        json!({"agent_id":meta.agent_id,"token":meta.token}),
    )["session"]
        .as_str()
        .unwrap()
        .to_owned();
    call(&mut app, "agent/enter_world", json!({"session":session}));
    call(&mut app, "agent/take_control", json!({"session":session}));
    run_ticks(app.world_mut(), 1);
    for book in ["diary", "memory", "proposals"] {
        assert_eq!(
            call(
                &mut app,
                "agent/notes/add",
                json!({"session":session,"book":book,"title":"First life","body":"FIRST_LIFE_NOTEBOOK_CANARY"})
            )["accepted"],
            true
        );
    }
    // Operator-only death arrangement. No host frame runs after the new-life
    // command: the next registration/notes calls exercise the publication seam.
    let old_body = app.world().resource::<SimIndex>().get(old).unwrap();
    let thirst = app.world().resource::<NeedRegistry>().id("Thirst").unwrap();
    app.world_mut()
        .get_mut::<Needs>(old_body)
        .unwrap()
        .get_mut(thirst)
        .value = 0;
    {
        let mut vitality = app
            .world_mut()
            .get_mut::<island_content::village::Vitality>(old_body)
            .unwrap();
        vitality.health = 1;
        vitality.dehydrated_ticks = 601;
    }
    run_ticks(app.world_mut(), 1);
    assert_eq!(
        call(
            &mut app,
            "agent/act",
            json!({"session":session,"verb":"new_life"})
        )["accepted"],
        true
    );
    run_ticks(app.world_mut(), 1);
    let successor = app.world().resource::<VillageState>().played;
    assert_ne!(successor, old);
    assert!(
        !app.world()
            .resource::<NotesDirs>()
            .0
            .contains_key(&successor),
        "fixture reaches the assignment boundary before the periodic host frame"
    );
    let before = state_hash(app.world_mut());
    let current = home::credential_snapshot(&dir).unwrap().0;
    let fresh = call(
        &mut app,
        "agent/register",
        json!({"agent_id":current.agent_id,"token":current.token}),
    )["session"]
        .as_str()
        .unwrap()
        .to_owned();
    let revoked = api::call(
        app.world_mut(),
        "agent/observe",
        Some(json!({"session":session})),
    )
    .unwrap_err();
    assert_eq!(revoked.data.unwrap()["revoked"], true);
    call(&mut app, "agent/enter_world", json!({"session":fresh}));
    for book in ["diary", "memory", "proposals"] {
        let notes = call(
            &mut app,
            "agent/notes/list",
            json!({"session":fresh,"book":book}),
        );
        assert!(
            notes["notes"].as_array().unwrap().is_empty(),
            "each successor notebook starts fresh"
        );
        assert!(!notes.to_string().contains("FIRST_LIFE_NOTEBOOK_CANARY"));
    }
    assert_eq!(
        app.world().resource::<NotesDirs>().0.get(&old),
        Some(&dir.dir)
    );
    assert_eq!(
        app.world().resource::<NotesDirs>().0.get(&successor),
        Some(&dir.dir.join("lives/2"))
    );
    assert_eq!(
        call(
            &mut app,
            "agent/notes/add",
            json!({"session":fresh,"book":"diary","title":"Second life","body":"Successor notebooks are usable before taking control."})
        )["accepted"],
        true
    );
    assert_eq!(
        state_hash(app.world_mut()),
        before,
        "host notebook assignment and I/O never alter simulation state"
    );
}
