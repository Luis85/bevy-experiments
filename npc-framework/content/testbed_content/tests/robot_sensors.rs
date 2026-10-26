//! Plan Task 12 (spec G10): the robot presets get real LIDAR, camera and
//! radar sensors; their readings show up as the `lidar`, `camera` and
//! `radar` observation sections (JSON = the ROS-shaped readings, text = a
//! short summary); the readings survive a save and resume; and no world
//! without robots changes its hashes.

use std::path::{Path, PathBuf};

use bevy_app::App;
use npc_framework::interaction::{Door, Inventory};
use npc_framework::kernel::registry::content_hash;
use npc_framework::kernel::snapshot::{LoadMode, load, save};
use npc_framework::perception::{Facing, PerceptionMemory, SenseKind, SensorReadings, Sensors};
use npc_framework::prelude::{SimId, SimIndex, SimPos, run_ticks, state_hash};
use npc_framework::tools::snapshot_check::assert_resume_equivalent;
use serde_json::{Value, json};
use testbed_content::TestbedIds;
use testbed_content::explorer::host::{HostArgs, build_world};

const SEED: u64 = 0x5eed;
const NOW: &str = "2026-10-25T12:00:00Z";

fn tmp(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("npc-robot-sensors-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn args(home: &Path, preset: &str) -> HostArgs {
    HostArgs {
        home: home.join("agents"),
        agent: "Wren".into(),
        world: Some("W".into()),
        resume: false,
        on_mismatch: None,
        preset: Some(preset.to_owned()),
        actors_dir: Some(home.join("actors")),
    }
}

/// A New explorer world whose actor is built from `preset`.
fn explorer(preset: &str, tag: &str) -> (App, SimId, PathBuf) {
    let home = tmp(tag);
    let (app, _, _) = build_world(&args(&home, preset), SEED, NOW, |_| {})
        .unwrap_or_else(|e| panic!("{preset}: {e}"));
    let id = app.world().resource::<TestbedIds>().id("Wren");
    (app, id, home)
}

fn observe(app: &mut App, unit: SimId, format: Option<&str>) -> Value {
    let mut params = json!({"unit": unit.0});
    if let Some(f) = format {
        params["format"] = json!(f);
    }
    npc_framework::agent_io::api::call(app.world_mut(), "agent/observe", Some(params)).unwrap()
}

fn text(app: &mut App, unit: SimId) -> String {
    observe(app, unit, Some("text"))["text"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[test]
fn robot_observation_has_sensor_sections() {
    // The rover: LIDAR + camera.
    let (mut app, wren, home) = explorer("rover", "rover-obs");
    run_ticks(app.world_mut(), 40);
    let obs = observe(&mut app, wren, None);
    let s = &obs["sections"];
    let lidar = &s["lidar"];
    assert_eq!(lidar["angle_min_mdeg"], json!(-180_000), "{lidar}");
    assert_eq!(lidar["angle_increment_mdeg"], json!(1000));
    assert_eq!(lidar["range_max_mm"], json!(20_000));
    let ranges = lidar["ranges_mm"].as_array().unwrap();
    assert_eq!(ranges.len(), 360);
    assert!(
        ranges.iter().any(|r| r.as_i64().unwrap() <= 20_000),
        "the compound's walls are within 20 m"
    );
    assert!(s["camera"]["detections"].is_array(), "{}", s["camera"]);
    assert!(s.get("radar").is_none(), "a rover has no radar");
    let t = text(&mut app, wren);
    // The robot sensors' lines come together right after "You perceive".
    let lines: Vec<&str> = t.lines().collect();
    let at = lines
        .iter()
        .position(|l| l.starts_with("You perceive"))
        .expect(&t);
    assert!(lines[at + 1].starts_with("LIDAR"), "{t}");
    assert!(lines[at + 2].starts_with("Camera"), "{t}");
    let line = t.lines().find(|l| l.starts_with("LIDAR")).expect(&t);
    for dir in ["N ", "NE ", "E ", "SE ", "S ", "SW ", "W ", "NW "] {
        assert!(line.contains(dir), "{line}");
    }
    assert!(t.lines().any(|l| l.starts_with("Camera")), "{t}");
    println!("rover text:\n{t}");
    let _ = std::fs::remove_dir_all(&home);

    // The sentinel: radar + camera.
    let (mut app, wren, home) = explorer("sentinel", "sentinel-obs");
    run_ticks(app.world_mut(), 40);
    let obs = observe(&mut app, wren, None);
    let s = &obs["sections"];
    assert!(s["radar"]["tracks"].is_array(), "{}", s["radar"]);
    assert!(s["camera"]["detections"].is_array());
    assert!(s.get("lidar").is_none());
    let t = text(&mut app, wren);
    assert!(t.lines().any(|l| l.starts_with("Radar")), "{t}");
    println!("sentinel text:\n{t}");
    let _ = std::fs::remove_dir_all(&home);

    // The human has none of them.
    let (mut app, wren, home) = explorer("human", "human-obs");
    run_ticks(app.world_mut(), 40);
    let obs = observe(&mut app, wren, None);
    for name in ["lidar", "camera", "radar"] {
        assert!(obs["sections"].get(name).is_none(), "{name}");
    }
    let _ = std::fs::remove_dir_all(&home);
}

/// The robots' sensors come from their presets.
#[test]
fn robots_get_their_sensors() {
    let (app, wren, home) = explorer("rover", "rover-sensors");
    let e = app.world().resource::<SimIndex>().get(wren).unwrap();
    let sensors = app.world().get::<Sensors>(e).unwrap();
    assert!(sensors.visions().next().is_none(), "no eyes");
    assert_eq!(sensors.hearing_radius(), None, "no ears");
    assert!(sensors.lidar_sensor().is_some() && sensors.camera_sensor().is_some());
    assert!(sensors.radar_sensor().is_none());
    let _ = std::fs::remove_dir_all(&home);
}

/// Worlds without robots keep the hashes recorded before Task 12 (HEAD
/// cad901f): the testbed scenarios and the human explorer world, content
/// hash and state hash at tick 600 (the village's are pinned by
/// `village_perception::fog_does_not_change_existing_hashes`).
#[test]
fn existing_hashes_unchanged() {
    for (scenario, content, state) in [
        (
            "testbed",
            0xd8a1_bc52_b124_7a36_u64,
            0xb9f3_94ce_322c_6b4e_u64,
        ),
        ("perception", 0xd8a1_bc52_b124_7a36, 0xe347_bced_dfe4_796e),
        ("smithy", 0x7d7a_7460_c281_6513, 0xd826_f1d6_9075_5af3),
    ] {
        let mut app = testbed_content::scenario::try_new_sim_app(1, scenario, 0).unwrap();
        assert_eq!(content_hash(app.world_mut()), content, "{scenario}");
        run_ticks(app.world_mut(), 600);
        assert_eq!(state_hash(app.world_mut()), state, "{scenario}");
    }
    let (mut app, _, home) = explorer("human", "human-hash");
    assert_eq!(content_hash(app.world_mut()), 0xd8a1_bc52_b124_7a36);
    run_ticks(app.world_mut(), 600);
    assert_eq!(state_hash(app.world_mut()), 0xe011_a567_313f_1fe8);
    let _ = std::fs::remove_dir_all(&home);
}

fn readings(app: &App, id: SimId) -> SensorReadings {
    let e = app.world().resource::<SimIndex>().get(id).unwrap();
    app.world().get::<SensorReadings>(e).cloned().unwrap()
}

/// A rover world resumes equivalently: the state hash, and the readings
/// (snapshot-only, so compared by value).
#[test]
fn rover_resume_is_equivalent() {
    for preset in ["rover", "sentinel"] {
        let homes = std::sync::Mutex::new(Vec::new());
        let make = || {
            let n = homes.lock().unwrap().len();
            let (app, _, home) = explorer(preset, &format!("{preset}-resume-{n}"));
            homes.lock().unwrap().push(home);
            app
        };
        assert_resume_equivalent(make, 150, 300, 25);

        let (mut never, wren, h1) = explorer(preset, &format!("{preset}-never"));
        let (mut saved, _, h2) = explorer(preset, &format!("{preset}-saved"));
        let (mut loaded, _, h3) = explorer(preset, &format!("{preset}-loaded"));
        run_ticks(never.world_mut(), 150);
        run_ticks(saved.world_mut(), 150);
        let snap = save(saved.world_mut()).unwrap();
        let snap = serde_json::from_str(&serde_json::to_string(&snap).unwrap()).unwrap();
        load(loaded.world_mut(), &snap, LoadMode::Strict).unwrap();
        assert_eq!(readings(&loaded, wren), readings(&never, wren), "{preset}");
        run_ticks(never.world_mut(), 97);
        run_ticks(loaded.world_mut(), 97);
        assert_eq!(readings(&loaded, wren), readings(&never, wren), "{preset}");
        for h in homes.into_inner().unwrap().into_iter().chain([h1, h2, h3]) {
            let _ = std::fs::remove_dir_all(&h);
        }
    }
}

fn ent(app: &App, id: SimId) -> bevy_ecs::entity::Entity {
    app.world().resource::<SimIndex>().get(id).unwrap()
}

/// The rover's LIDAR range straight ahead.
fn ahead(app: &App, wren: SimId) -> i32 {
    let scan = readings(app, wren).lidar.unwrap();
    scan.ranges_mm[usize::try_from(-scan.angle_min_mdeg / scan.angle_increment_mdeg).unwrap()]
}

/// Fix round 1: in the testbed a closed door stops the rover's beams and an
/// open one lets them through (the door is left to its cell: blocking
/// while closed); items are not LIDAR obstacles.
#[test]
fn lidar_sees_through_an_open_door_only() {
    let (mut app, wren, home) = explorer("rover", "rover-door");
    let door = app.world().resource::<TestbedIds>().id("workshop_door");
    let e = ent(&app, wren);
    // Inside the workshop, 1.5 m north of its door's cell (-6, 3), facing
    // the door (south).
    *app.world_mut().get_mut::<SimPos>(e).unwrap() = SimPos::new(-5500, 5500);
    run_ticks(app.world_mut(), 1);
    *app.world_mut().get_mut::<Facing>(e).unwrap() = Facing::new(0, -1);
    run_ticks(app.world_mut(), 8);
    assert!(!app.world().get::<Door>(ent(&app, door)).unwrap().open);
    let closed = ahead(&app, wren);
    // 1.5 m to the door's cell, ±1 % noise.
    assert!((1485..=1515).contains(&closed), "closed: {closed}");
    let door_e = ent(&app, door);
    app.world_mut().get_mut::<Door>(door_e).unwrap().open = true;
    run_ticks(app.world_mut(), 8);
    let open = ahead(&app, wren);
    assert!(open > 2500, "open: the beam goes into the yard ({open})");
    let _ = std::fs::remove_dir_all(&home);
}

fn call(app: &mut App, method: &str, params: Value) -> Value {
    npc_framework::agent_io::api::call(app.world_mut(), method, Some(params))
        .unwrap_or_else(|e| panic!("{method}: {e:?}"))
}

/// Runs until `done` holds (at most `max` ticks).
fn run_until(app: &mut App, max: u64, done: impl Fn(&App) -> bool) -> bool {
    for _ in 0..max / 10 {
        if done(app) {
            return true;
        }
        run_ticks(app.world_mut(), 10);
    }
    done(app)
}

/// Fix round 1: a rover (camera and reach, no eyes or ears) perceives the
/// bread and the workshop door through its camera, then picks the bread up
/// and opens the door through its affordances.
#[test]
fn rover_picks_up_and_opens_through_its_affordances() {
    let (mut app, wren, home) = explorer("rover", "rover-act");
    let ids = app.world().resource::<TestbedIds>().clone();
    let (bread, door) = (ids.id("bread"), ids.id("workshop_door"));
    let e = ent(&app, wren);
    let sensors = app.world().get::<Sensors>(e).unwrap();
    assert!(sensors.visions().next().is_none() && sensors.hearing_radius().is_none());
    run_ticks(app.world_mut(), 8);
    let memory = app.world().get::<PerceptionMemory>(e).unwrap().clone();
    for id in [bread, door] {
        let p = memory.get(id).unwrap_or_else(|| panic!("{id:?} unseen"));
        assert_eq!(p.via, SenseKind::Camera, "{id:?}");
    }
    let obs = call(
        &mut app,
        "agent/observe",
        json!({"unit": wren.0, "controller": "explorer"}),
    );
    let targets = |verb: &str| -> Vec<u64> {
        obs["affordances"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|a| a["verb"] == json!(verb))
            .filter_map(|a| a["target"].as_u64())
            .collect()
    };
    assert!(targets("pick_up").contains(&u64::from(bread.0)), "{obs}");
    assert!(targets("open").contains(&u64::from(door.0)), "{obs}");
    let act = |app: &mut App, target: SimId, verb: &str| {
        call(
            app,
            "agent/act",
            json!({"unit": wren.0, "controller": "explorer", "verb": "interact",
                   "target": target.0, "args": {"verb": verb}}),
        )
    };
    let r = act(&mut app, bread, "pick_up");
    assert_eq!(r["accepted"], json!(true), "{r}");
    assert!(
        run_until(&mut app, 1200, |app| app
            .world()
            .get::<Inventory>(ent(app, wren))
            .unwrap()
            .contains(bread)),
        "the bread is carried"
    );
    let r = act(&mut app, door, "open");
    assert_eq!(r["accepted"], json!(true), "{r}");
    assert!(
        run_until(&mut app, 1200, |app| app
            .world()
            .get::<Door>(ent(app, door))
            .unwrap()
            .open),
        "the door is open"
    );
    let _ = std::fs::remove_dir_all(&home);
}
