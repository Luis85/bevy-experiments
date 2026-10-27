//! Real perception memory supplies landmark positions; recorded movement is
//! ordinary input and can replay without the map files or agent credentials.
use bevy_app::App;
use npc_framework::agent_io::{
    api,
    scenario::{ScenarioDoc, ScenarioLoaders, SessionRecord, verify_session},
};
use npc_framework::kernel::replay::Recorder;
use npc_framework::perception::PerceptionMemory;
use npc_framework::prelude::{SimId, SimIndex, SimPos, run_ticks, state_hash};
use serde_json::{Value, json};
use testbed_content::TestbedIds;
use testbed_content::explorer::host::{
    ExplorerSession, HostArgs, build_world, insert_notes, write_session_files,
};
const NOW: &str = "2026-10-27T12:00:00Z";
fn setup(tag: &str) -> (App, std::path::PathBuf, String, SimId) {
    let home = std::env::temp_dir().join(format!("npc-landmarks-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    let args = HostArgs {
        home: home.clone(),
        agent: "Wren".into(),
        world: Some("W".into()),
        resume: false,
        on_mismatch: None,
        preset: None,
        actors_dir: Some(home.join("actors")),
        world_preset: None,
        worlds_dir: Some(home.join("worlds")),
    };
    let (mut app, _, dir) = build_world(&args, 0x5eed, NOW, |_| {}).unwrap();
    let id = app.world().resource::<TestbedIds>().id("Wren");
    insert_notes(app.world_mut(), id, &dir, || NOW.into());
    let meta = app.world().resource::<ExplorerSession>().meta.clone();
    let registered = call(
        &mut app,
        "agent/register",
        json!({"agent_id":meta.agent_id,"token":meta.token}),
    );
    (
        app,
        home,
        registered["session"].as_str().unwrap().into(),
        id,
    )
}
fn call(app: &mut App, method: &str, p: Value) -> Value {
    api::call(app.world_mut(), method, Some(p)).unwrap_or_else(|e| panic!("{method}: {e:?}"))
}
fn entity(app: &App, id: SimId) -> bevy_ecs::entity::Entity {
    app.world().resource::<SimIndex>().get(id).unwrap()
}

#[test]
fn thing_placement_uses_last_percept_even_when_truth_moved_or_vanished() {
    let (mut app, home, key, id) = setup("memory");
    call(&mut app, "agent/take_control", json!({"session":key}));
    run_ticks(app.world_mut(), 8);
    let actor = entity(&app, id);
    let (&thing, &percept) = app
        .world()
        .get::<PerceptionMemory>(actor)
        .unwrap()
        .percepts
        .iter()
        .next()
        .expect("the human sees or reaches a thing");
    let remembered = percept.pos;
    let thing_entity = entity(&app, thing);
    if let Some(mut truth) = app.world_mut().get_mut::<SimPos>(thing_entity) {
        *truth = SimPos::new(900000, 900000);
    }
    let hash = state_hash(app.world_mut());
    let added = call(
        &mut app,
        "agent/map/add",
        json!({"session":key,"name":"Remembered thing","place":{"thing":thing.0}}),
    );
    assert_eq!(
        added["landmark"]["pos"],
        json!([remembered.x, remembered.z])
    );
    assert_eq!(added["landmark"]["thing"], thing.0);
    assert_eq!(state_hash(app.world_mut()), hash);
    app.world_mut()
        .get_mut::<PerceptionMemory>(actor)
        .unwrap()
        .percepts
        .remove(&thing);
    let error = api::call(
        app.world_mut(),
        "agent/map/add",
        Some(json!({"session":key,"name":"Unknown despite world truth","place":{"thing":thing.0}})),
    )
    .unwrap_err();
    assert_eq!(error.data.unwrap()["reason"], "unknown_thing");
    // A remembered id is still usable after its world entity is gone.
    app.world_mut().despawn(thing_entity);
    app.world_mut()
        .get_mut::<PerceptionMemory>(actor)
        .unwrap()
        .percepts
        .insert(thing, percept);
    let remembered_again = call(
        &mut app,
        "agent/map/add",
        json!({"session":key,"name":"Gone thing","place":{"thing":thing.0}}),
    );
    assert_eq!(
        remembered_again["landmark"]["pos"],
        json!([remembered.x, remembered.z])
    );
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn landmark_walk_arrives_and_record_verifies_after_map_folder_is_deleted() {
    let (mut app, home, key, id) = setup("walk-record");
    let doc: ScenarioDoc =
        serde_json::from_value(app.world().resource::<ExplorerSession>().scenario.clone()).unwrap();
    let mut recorder = Recorder::live(app.world_mut(), 10);
    call(&mut app, "agent/take_control", json!({"session":key}));
    run_ticks(app.world_mut(), 1);
    recorder.after_tick(app.world_mut());
    let here = *app.world().get::<SimPos>(entity(&app, id)).unwrap();
    let goal = [here.x + 500, here.z];
    let landmark = call(
        &mut app,
        "agent/map/add",
        json!({"session":key,"name":"A short walk","place":{"pos":goal}}),
    )["landmark"]
        .clone();
    let accepted = call(
        &mut app,
        "agent/act",
        json!({"session":key,"steps":[{"verb":"order","args":{"order":{"type":"Move","landmark":landmark["id"]}}},{"verb":"execute_orders"}]}),
    );
    assert_eq!(accepted["accepted"], true, "{accepted}");
    for _ in 0..80 {
        run_ticks(app.world_mut(), 1);
        recorder.after_tick(app.world_mut());
    }
    let at = *app.world().get::<SimPos>(entity(&app, id)).unwrap();
    assert!(
        at.dist2(SimPos::new(goal[0], goal[1])) <= 150 * 150,
        "{at:?}"
    );
    let replay = recorder.finish(app.world_mut()).unwrap();
    let text = serde_json::to_string(&replay).unwrap();
    assert!(!text.contains("landmark") && !text.contains("A short walk"));
    let dir = app.world().resource::<ExplorerSession>().dir.clone();
    std::fs::remove_dir_all(dir.dir.join("map")).unwrap();
    let mut loaders = ScenarioLoaders::default();
    testbed_content::register_scenario_loader(&mut loaders);
    let result = verify_session(
        &loaders,
        &SessionRecord {
            version: 1,
            scenario: doc,
            seed: 0x5eed,
            replay,
        },
    );
    assert!(result.ok, "{result:?}");
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn old_world_data_defaults_map_on_and_off_context_survives_save_and_rotation() {
    use testbed_content::explorer::host::{rotate_agent_token, save_now};
    let mut legacy = serde_json::to_value(testbed_content::worlds::plain_preset()).unwrap();
    legacy["systems"].as_object_mut().unwrap().remove("map");
    let mut no_map: testbed_content::worlds::WorldPreset = serde_json::from_value(legacy).unwrap();
    assert!(no_map.systems.map);
    no_map.systems.map = false;
    let (app, home, _key, id) = setup("off");
    let mut session = app.world().resource::<ExplorerSession>().clone();
    session.world_preset = no_map;
    session.meta.world_preset = "no_map".into();
    let mut app = testbed_content::explorer::host::reset_world(&session, 0x5eed, |_| {}).unwrap();
    insert_notes(app.world_mut(), id, &session.dir, || NOW.into());
    assert!(!npc_framework::agent_io::map::enabled(app.world()));
    let files = write_session_files(
        app.world(),
        &testbed_content::explorer::host::Start::New,
        &session.dir,
        "http://127.0.0.1:15719",
        home.join("npcctl"),
        true,
    )
    .unwrap();
    assert_eq!(files.write_error, None);
    save_now(app.world_mut(), &session.dir, NOW).unwrap();
    rotate_agent_token(&session.dir).unwrap();
    let agents = std::fs::read_to_string(session.dir.dir.join("AGENTS.md")).unwrap();
    assert!(
        !agents.contains("## Your map")
            && !agents.contains("`map_add`")
            && !agents.contains("`map_list`")
    );
    let prompt = std::fs::read_to_string(
        session
            .dir
            .prompt(testbed_content::explorer::prompt::PromptKind::Resume),
    )
    .unwrap();
    assert!(!prompt.contains("map_list"));
    let _ = std::fs::remove_dir_all(home);
}
