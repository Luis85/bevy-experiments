//! Actor presets as data (plan Task 11, spec G9–G11): the built-in human,
//! rover and sentinel files, validation with a reason per problem, the
//! explorer actor spawned from the chosen preset (and resumed as it), the
//! allowed-actions gate, and the human preset's world equal to today's.

use std::path::{Path, PathBuf};

use bevy_app::App;
use bevy_ecs::entity::Entity;
use npc_framework::interaction::Inventory;
use npc_framework::kernel::registry::content_hash;
use npc_framework::perception::{ReachSense, RevealsFog, Sensors, Vision};
use npc_framework::prelude::{
    CharacterSheet, Commandable, MountedIn, SeatRole, SimId, SimIndex, SimPos, run_ticks,
    state_hash,
};
use npc_framework::rules::template_sheet;
use serde_json::{Value, json};
use testbed_content::TestbedIds;
use testbed_content::TestbedUnit;
use testbed_content::actors::{
    ActorModel, ActorPreset, AllowedActions, BUILTIN_PRESETS, Body, SensorSpec, load_presets,
    validate,
};
use testbed_content::explorer::home::AgentMeta;
use testbed_content::explorer::host::{
    ExplorerSession, HostArgs, Mismatch, Start, build_world, reset_world, save_now,
    write_session_files,
};

const SEED: u64 = 0x5eed;
const NOW: &str = "2026-10-25T12:00:00Z";

/// The host-built explorer world (seed [`SEED`], agent `Wren`) as recorded
/// on this branch before presets existed (HEAD 65a2c66): its content hash and
/// its state hash at tick 600.
const TODAY_CONTENT: u64 = 0xd8a1_bc52_b124_7a36;
const TODAY_TICK_600: u64 = 0xe011_a567_313f_1fe8;

fn tmp(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("npc-actors-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// New `Wren` in world `W` under `home`, presets from `home/actors`.
fn new_args(home: &Path, preset: Option<&str>) -> HostArgs {
    HostArgs {
        home: home.join("agents"),
        agent: "Wren".into(),
        world: Some("W".into()),
        resume: false,
        on_mismatch: None,
        preset: preset.map(str::to_owned),
        actors_dir: Some(home.join("actors")),
    }
}

fn resume_args(home: &Path, preset: Option<&str>) -> HostArgs {
    HostArgs {
        world: None,
        resume: true,
        ..new_args(home, preset)
    }
}

/// A frozen testbed world to validate against.
fn testbed_world() -> App {
    testbed_content::scenario::try_new_sim_app(1, "explorer", 0).unwrap()
}

fn entity(app: &App, name: &str) -> (SimId, Entity) {
    let id = app.world().resource::<TestbedIds>().id(name);
    (id, app.world().resource::<SimIndex>().get(id).unwrap())
}

/// The built-in human preset, read from a fresh actors folder.
fn human(home: &Path) -> ActorPreset {
    let dir = home.join("actors");
    load_presets(&dir)
        .into_iter()
        .find(|(p, _)| p.file_stem().is_some_and(|s| s == "human"))
        .unwrap()
        .1
        .unwrap()
}

/// Writes `preset` as `<home>/actors/<id>.json` (the folder is first filled
/// with the built-ins, so it is not empty).
fn write_preset(home: &Path, id: &str, preset: &Value) {
    let dir = home.join("actors");
    let _ = load_presets(&dir);
    std::fs::write(
        dir.join(format!("{id}.json")),
        serde_json::to_string_pretty(preset).unwrap(),
    )
    .unwrap();
}

/// The built-in human as JSON with `edit` applied.
fn human_json(home: &Path, edit: impl FnOnce(&mut Value)) -> Value {
    let mut v = serde_json::to_value(human(home)).unwrap();
    edit(&mut v);
    v
}

/// A porter: the human with other numbers, a knight's model and no
/// `pick_up` or `craft`.
fn porter(home: &Path) -> Value {
    human_json(home, |v| {
        v["name"] = json!("Porter");
        v["model"] = json!("knight");
        v["speed_mm_per_tick"] = json!(50);
        v["carry_capacity"] = json!(40_000);
        v["sensors"] = json!([
            {"type": "vision", "range_mm": 12_000, "half_fov_deg": 45},
            {"type": "reach", "radius_mm": 2000}
        ]);
        let actions: Vec<Value> = v["actions"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|a| *a != "pick_up" && *a != "craft")
            .cloned()
            .collect();
        v["actions"] = Value::Array(actions);
    })
}

#[test]
fn builtins_load_and_validate() {
    let home = tmp("builtins");
    let dir = home.join("actors");
    let presets = load_presets(&dir);
    let names: Vec<String> = presets
        .iter()
        .map(|(p, _)| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, ["human.json", "rover.json", "sentinel.json"]);
    assert_eq!(
        BUILTIN_PRESETS.map(|(id, _)| id),
        ["human", "rover", "sentinel"]
    );
    let app = testbed_world();
    let world = app.world();
    for (path, preset) in &presets {
        let p = preset
            .as_ref()
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let stem = path.file_stem().unwrap().to_string_lossy();
        let reach = p.sensors.iter().find_map(|s| match s {
            SensorSpec::Reach { radius_mm } => Some(*radius_mm),
            _ => None,
        });
        assert_eq!(reach, Some(1500), "{stem}: reach 1.5 m");
        match &*stem {
            "human" => {
                assert_eq!(validate(p, world), Ok(()));
                assert_eq!(p.body, Body::Human);
                // The scout sheet, today's speed, carry and senses.
                assert_eq!(
                    p.sheet.to_sheet(),
                    template_sheet(world, "scout").unwrap(),
                    "the scout's sheet"
                );
                assert_eq!((p.speed_mm_per_tick, p.carry_capacity), (70, 30_000));
                assert_eq!(
                    &p.sensors[..2],
                    [
                        SensorSpec::Vision {
                            range_mm: 15_000,
                            half_fov_deg: 60
                        },
                        SensorSpec::Hearing { radius_mm: 10_000 }
                    ]
                );
                assert_eq!(p.actions, testbed_content::actors::known_actions(world));
            }
            // Real robot sensors since Task 12 (E2): valid, no eyes or ears.
            "rover" | "sentinel" => {
                assert_eq!(p.body, Body::Robot);
                assert_eq!(validate(p, world), Ok(()), "{stem}");
                assert!(
                    !p.sensors.iter().any(|s| matches!(
                        s,
                        SensorSpec::Vision { .. } | SensorSpec::Hearing { .. }
                    )),
                    "{stem}: a robot senses through its sensors only"
                );
                let camera = SensorSpec::Camera {
                    range_mm: 15_000,
                    half_fov_deg: 35,
                };
                assert!(p.sensors.contains(&camera), "{stem}: camera 15 m / 35°");
                let robot = if stem == "rover" {
                    SensorSpec::Lidar {
                        range_mm: 20_000,
                        fov_deg: 360,
                        beams: 360,
                        noise_ppm: 10_000,
                    }
                } else {
                    SensorSpec::Radar {
                        range_mm: 30_000,
                        half_fov_deg: 60,
                        range_res_mm: 500,
                        bearing_res_mdeg: 2000,
                    }
                };
                assert!(p.sensors.contains(&robot), "{stem}: {robot:?}");
                assert!(!p.actions.contains("craft"));
                let mut all = testbed_content::actors::known_actions(world);
                all.remove("craft");
                assert_eq!(p.actions, all, "{stem}: everything but craft");
            }
            other => panic!("unexpected built-in {other}"),
        }
    }
    // Copied once: a folder that has files is left as it is.
    std::fs::remove_file(dir.join("rover.json")).unwrap();
    assert_eq!(load_presets(&dir).len(), 2);
    assert!(!dir.join("rover.json").exists());
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn invalid_presets_are_reported() {
    let home = tmp("invalid");
    let bad_sensor = human_json(&home, |v| {
        v["sensors"] = json!([{"type": "sonar", "range_mm": 5000}]);
    });
    let negative = human_json(&home, |v| v["sensors"][0]["range_mm"] = json!(-5));
    let rich = human_json(&home, |v| {
        v["sheet"]["attributes"] = json!({"st": 18, "dx": 18, "iq": 18, "ht": 18});
    });
    let no_model = human_json(&home, |v| v["model"] = json!("dragon"));
    let no_action = human_json(&home, |v| {
        v["actions"].as_array_mut().unwrap().push(json!("fly"));
    });
    let two_reaches = human_json(&home, |v| {
        v["sensors"]
            .as_array_mut()
            .unwrap()
            .push(json!({"type": "reach", "radius_mm": 900}));
    });
    let lidar =
        json!({"type": "lidar", "range_mm": 9000, "fov_deg": 270, "beams": 90, "noise_ppm": 0});
    let two_lidars = human_json(&home, |v| {
        v["sensors"]
            .as_array_mut()
            .unwrap()
            .extend([lidar.clone(), lidar]);
    });
    for (id, doc) in [
        ("two_lidars", &two_lidars),
        ("bad_sensor", &bad_sensor),
        ("negative", &negative),
        ("rich", &rich),
        ("no_model", &no_model),
        ("no_action", &no_action),
        ("two_reaches", &two_reaches),
    ] {
        write_preset(&home, id, doc);
    }
    let app = testbed_world();
    let loaded = load_presets(&home.join("actors"));
    let get = |id: &str| {
        loaded
            .iter()
            .find(|(p, _)| p.file_stem().is_some_and(|s| s == id))
            .map(|(_, r)| r.clone())
            .unwrap()
    };
    let e = get("bad_sensor").unwrap_err();
    assert!(e.contains("sonar"), "{e}");
    for (id, want) in [
        ("negative", "vision range_mm -5"),
        ("rich", "over the budget"),
        ("no_model", "unknown model `dragon`"),
        ("no_action", "unknown action `fly`"),
        ("two_reaches", "2 reach sensors"),
    ] {
        let reasons = validate(&get(id).unwrap(), app.world()).unwrap_err();
        assert!(
            reasons.iter().any(|r| r.contains(want)),
            "{id}: {reasons:?} lacks {want:?}"
        );
    }
    // Never spawned: the host names the reasons and writes nothing.
    for (id, want) in [
        ("bad_sensor", "sonar"),
        ("negative", "vision range_mm -5"),
        ("rich", "over the budget"),
        ("no_model", "unknown model `dragon`"),
        ("no_action", "unknown action `fly`"),
        ("two_reaches", "2 reach sensors"),
        ("two_lidars", "2 lidar sensors"),
        ("nobody", "unknown actor preset `nobody`"),
    ] {
        let Err(e) = build_world(&new_args(&home, Some(id)), SEED, NOW, |_| {}) else {
            panic!("preset {id} started");
        };
        assert!(e.contains(want), "{id}: {e}");
        assert!(
            !home.join("agents").join("wren").exists(),
            "no folder for {id}"
        );
    }
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn explorer_spawns_the_chosen_preset() {
    let home = tmp("chosen");
    write_preset(&home, "porter", &porter(&home));
    let (mut app, start, dir) =
        build_world(&new_args(&home, Some("porter")), SEED, NOW, |_| {}).unwrap();
    assert!(matches!(start, Start::New));
    let check = |app: &App| {
        let (_, e) = entity(app, "Wren");
        let w = app.world();
        assert_eq!(w.get::<TestbedUnit>(e).unwrap().speed_mm_per_tick, 50);
        assert_eq!(w.get::<Commandable>(e).unwrap().speed_mm_per_tick, 50);
        assert_eq!(w.get::<Inventory>(e).unwrap().max_weight_g, 40_000);
        let visions: Vec<Vision> = w.get::<Sensors>(e).unwrap().visions().collect();
        assert_eq!(visions, [Vision::new(12_000, 45)]);
        assert_eq!(w.get::<Sensors>(e).unwrap().hearing_radius(), None);
        assert_eq!(w.get::<ReachSense>(e).unwrap().radius_mm, 2000);
        assert!(w.get::<RevealsFog>(e).is_some(), "presets keep RevealsFog");
        assert_eq!(w.get::<ActorModel>(e).unwrap().0, "knight");
        let allowed = &w.get::<AllowedActions>(e).unwrap().0;
        assert!(allowed.contains("open") && !allowed.contains("pick_up"));
        let session = w.resource::<ExplorerSession>();
        assert_eq!(session.meta.preset, "porter");
        assert_eq!(session.preset.name, "Porter");
        // The sheet stays the scout's.
        assert_eq!(
            w.get::<CharacterSheet>(e).unwrap(),
            &template_sheet(w, "scout").unwrap()
        );
    };
    check(&app);
    let meta: AgentMeta =
        serde_json::from_str(&std::fs::read_to_string(dir.dir.join("agent.json")).unwrap())
            .unwrap();
    assert_eq!(meta.preset, "porter");
    // AGENTS.md describes the porter.
    let files = write_session_files(
        app.world(),
        &start,
        &dir,
        "http://127.0.0.1:1",
        home.join("npcctl"),
        false,
    )
    .unwrap();
    assert_eq!(files.write_error, None);
    let md = std::fs::read_to_string(dir.dir.join("AGENTS.md")).unwrap();
    assert!(md.contains("Porter"), "{md}");
    assert!(md.contains("sight: 12 m, 45 degrees"), "{md}");
    assert!(md.contains("reach: 2.0 m"), "{md}");
    assert!(!md.contains("`craft`") && !md.contains("`pick_up`"), "{md}");
    assert!(md.contains("1.0 m/s"), "{md}");
    // A save and a resume keep the preset, even when its file changes.
    run_ticks(app.world_mut(), 20);
    let session = app.world().resource::<ExplorerSession>().clone();
    testbed_content::explorer::host::save_now(app.world_mut(), &dir, NOW).unwrap();
    write_preset(
        &home,
        "porter",
        &human_json(&home, |v| v["speed_mm_per_tick"] = json!(90)),
    );
    let (back, start, _) = build_world(&resume_args(&home, None), SEED, NOW, |_| {}).unwrap();
    assert!(matches!(start, Start::Resumed(_)), "{start:?}");
    check(&back);
    // Asking for another preset on resume is an error.
    let Err(e) = build_world(&resume_args(&home, Some("human")), SEED, NOW, |_| {}) else {
        panic!("a porter resumed as a human");
    };
    assert!(e.contains("porter"), "{e}");
    // world/reset rebuilds the same actor.
    let reset = reset_world(&session, 3, |_| {}).unwrap();
    check(&reset);
    let _ = std::fs::remove_dir_all(&home);
}

fn call(app: &mut App, method: &str, params: Value) -> Value {
    npc_framework::agent_io::api::call(app.world_mut(), method, Some(params))
        .unwrap_or_else(|e| panic!("{method}: {e:?}"))
}

/// The explorer's affordances of `verb` once it has looked about.
fn offered(app: &mut App, unit: SimId, verb: &str) -> Vec<Value> {
    let obs = call(
        app,
        "agent/observe",
        json!({"unit": unit.0, "controller": "explorer"}),
    );
    obs["affordances"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|a| a["verb"] == json!(verb))
        .cloned()
        .collect()
}

fn act(app: &mut App, unit: SimId, params: Value) -> Value {
    let mut p = json!({"unit": unit.0, "controller": "explorer"});
    for (k, v) in params.as_object().unwrap() {
        p[k] = v.clone();
    }
    call(app, "agent/act", p)
}

#[test]
fn disallowed_action_is_rejected() {
    let home = tmp("disallowed");
    write_preset(&home, "porter", &porter(&home));
    let (mut app, _, _) = build_world(&new_args(&home, Some("porter")), SEED, NOW, |_| {}).unwrap();
    let (wren, _) = entity(&app, "Wren");
    let (bread, _) = entity(&app, "bread");
    run_ticks(app.world_mut(), 8);
    let picks = offered(&mut app, wren, "pick_up");
    assert!(!picks.is_empty(), "the bread is seen");
    for a in &picks {
        assert_eq!(a["ok"], json!(false), "{a}");
        assert_eq!(a["why_not"], json!("this actor cannot pick_up"), "{a}");
    }
    let r = act(
        &mut app,
        wren,
        json!({"verb": "pick_up", "target": bread.0}),
    );
    assert_eq!(r["accepted"], json!(false), "{r}");
    assert_eq!(r["message"], json!("this actor cannot pick_up"), "{r}");
    // Through the walk-and-interact verb too.
    let r = act(
        &mut app,
        wren,
        json!({"verb": "interact", "target": bread.0, "args": {"verb": "pick_up"}}),
    );
    assert_eq!(r["accepted"], json!(false), "{r}");
    assert!(
        r["message"]
            .as_str()
            .unwrap()
            .contains("this actor cannot pick_up"),
        "{r}"
    );
    // Allowed verbs are not touched; the human may pick the bread up.
    assert!(
        offered(&mut app, wren, "open")
            .iter()
            .all(|a| a["why_not"] != json!("this actor cannot open"))
    );
    let home_b = tmp("disallowed-human");
    let (mut human_app, _, _) =
        build_world(&new_args(&home_b, Some("human")), SEED, NOW, |_| {}).unwrap();
    run_ticks(human_app.world_mut(), 8);
    let picks = offered(&mut human_app, wren, "pick_up");
    assert!(!picks.is_empty());
    assert!(
        picks
            .iter()
            .all(|a| a["why_not"] != json!("this actor cannot pick_up"))
    );
    let _ = std::fs::remove_dir_all(&home);
    let _ = std::fs::remove_dir_all(&home_b);
}

#[test]
fn human_preset_matches_todays_scout() {
    for (tag, preset) in [("human-default", None), ("human-named", Some("human"))] {
        let home = tmp(tag);
        let (mut app, _, _) = build_world(&new_args(&home, preset), SEED, NOW, |_| {}).unwrap();
        assert_eq!(content_hash(app.world_mut()), TODAY_CONTENT, "{tag}");
        let (_, e) = entity(&app, "Wren");
        assert_eq!(
            app.world().get::<ReachSense>(e).unwrap().radius_mm,
            testbed_content::explorer::host::EXPLORER_REACH_MM
        );
        assert!(app.world().get::<RevealsFog>(e).is_some());
        assert_eq!(app.world().get::<ActorModel>(e).unwrap().0, "rogue");
        run_ticks(app.world_mut(), 600);
        assert_eq!(state_hash(app.world_mut()), TODAY_TICK_600, "{tag}");
        let _ = std::fs::remove_dir_all(&home);
    }
}

// ---------------------------------------------------------------- fix round 1

/// A walker: the human without `drive_to` and `unload`.
fn walker(home: &Path) -> Value {
    human_json(home, |v| {
        v["name"] = json!("Walker");
        let actions: Vec<Value> = v["actions"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|a| *a != "drive_to" && *a != "unload")
            .cloned()
            .collect();
        v["actions"] = Value::Array(actions);
    })
}

/// `order` act of `unit` for `units` (default: itself).
fn order(app: &mut App, unit: SimId, order: Value, units: Option<&[SimId]>) -> Value {
    let mut args = json!({ "order": order });
    if let Some(u) = units {
        args["units"] = json!(u.iter().map(|id| id.0).collect::<Vec<_>>());
    }
    act(app, unit, json!({"verb": "order", "args": args}))
}

/// Seats `unit` in `vehicle`'s driver seat (walked there by teleport).
fn drive(app: &mut App, unit: SimId, vehicle: SimId) {
    let world = app.world_mut();
    let (e, v) = {
        let index = world.resource::<SimIndex>();
        (index.get(unit).unwrap(), index.get(vehicle).unwrap())
    };
    let at = *world.get::<SimPos>(v).unwrap();
    *world.get_mut::<SimPos>(e).unwrap() = SimPos::new(at.x + 600, at.z);
    npc_framework::vehicles::mount(
        world,
        e,
        vehicle,
        Some(SeatRole::Driver),
        npc_framework::interaction::Source::Command,
    )
    .unwrap();
    for _ in 0..200 {
        run_ticks(app.world_mut(), 1);
        if app.world().get::<MountedIn>(e).is_some() {
            return;
        }
    }
    panic!("never mounted");
}

/// Review fix 1: a preset that withholds `drive_to` and `unload` cannot get
/// round it through `order`: a movement order while in a driver seat, and
/// orders to a vehicle (checked against the actor giving them); the human
/// may do both.
#[test]
fn withheld_vehicle_verbs_cannot_be_ordered() {
    for (tag, preset, allowed) in [("walker", "walker", false), ("walker-human", "human", true)] {
        let home = tmp(tag);
        write_preset(&home, "walker", &walker(&home));
        let (mut app, _, _) =
            build_world(&new_args(&home, Some(preset)), SEED, NOW, |_| {}).unwrap();
        let (wren, _) = entity(&app, "Wren");
        let (cart, _) = entity(&app, "cart");
        let (wagon, _) = entity(&app, "wagon");
        let check = |r: &Value, verb: &str| {
            assert_eq!(r["accepted"], json!(allowed), "{tag} {verb}: {r}");
            if !allowed {
                assert_eq!(r["reason"], json!("not_allowed"), "{r}");
                assert_eq!(
                    r["message"],
                    json!(format!("this actor cannot {verb}")),
                    "{r}"
                );
            }
        };
        // Walking is no gated verb: a Move order on foot is fine for both.
        let r = order(
            &mut app,
            wren,
            json!({"type": "Move", "pos": [0, -5500]}),
            None,
        );
        assert_eq!(r["accepted"], json!(true), "{tag} walk: {r}");
        // (b) Orders to a vehicle unit: the ByCrew cart, the Direct wagon.
        for vehicle in [cart, wagon] {
            let r = order(
                &mut app,
                wren,
                json!({"type": "Move", "pos": [4500, -1500]}),
                Some(&[vehicle]),
            );
            check(&r, "drive_to");
            let r = order(&mut app, wren, json!({"type": "Unload"}), Some(&[vehicle]));
            check(&r, "unload");
        }
        // (a) A movement order while in the cart's driver seat drives it.
        drive(&mut app, wren, cart);
        for o in [
            json!({"type": "Move", "pos": [4500, -1500]}),
            json!({"type": "AttackMove", "pos": [4500, -1500]}),
        ] {
            let r = order(&mut app, wren, o, None);
            check(&r, "drive_to");
        }
        let r = order(&mut app, wren, json!({"type": "Unload"}), None);
        check(&r, "unload");
        let _ = std::fs::remove_dir_all(&home);
    }
}

/// Review fix 2: gated affordances are structural, so `brief` hides them.
#[test]
fn brief_hides_what_the_actor_cannot_do() {
    let home = tmp("brief");
    write_preset(&home, "porter", &porter(&home));
    let (mut app, _, _) = build_world(&new_args(&home, Some("porter")), SEED, NOW, |_| {}).unwrap();
    let (wren, _) = entity(&app, "Wren");
    run_ticks(app.world_mut(), 8);
    assert!(!offered(&mut app, wren, "pick_up").is_empty());
    let obs = call(
        &mut app,
        "agent/observe",
        json!({"unit": wren.0, "controller": "explorer", "brief": true}),
    );
    let affs = obs["affordances"].as_array().unwrap();
    assert!(affs.iter().all(|a| a["verb"] != json!("pick_up")), "{obs}");
    assert!(obs["hidden_affordances"].as_u64().unwrap() > 0, "{obs}");
    let _ = std::fs::remove_dir_all(&home);
}

/// Review fix 4: a resume keys the preset by the save; an `agent.json` that
/// disagrees loses (and is corrected), with a note.
#[test]
fn resume_keys_the_preset_by_the_save() {
    let home = tmp("keyed");
    write_preset(&home, "porter", &porter(&home));
    let (app, _, dir) = build_world(&new_args(&home, Some("porter")), SEED, NOW, |_| {}).unwrap();
    drop(app);
    let meta_path = dir.dir.join("agent.json");
    let mut meta: Value =
        serde_json::from_str(&std::fs::read_to_string(&meta_path).unwrap()).unwrap();
    meta["preset"] = json!("human");
    std::fs::write(&meta_path, meta.to_string()).unwrap();
    let (back, _, _) = build_world(&resume_args(&home, None), SEED, NOW, |_| {}).unwrap();
    let session = back.world().resource::<ExplorerSession>();
    assert_eq!(session.meta.preset, "porter");
    assert_eq!(session.preset.name, "Porter");
    assert!(
        session
            .notes
            .iter()
            .any(|n| n.contains("the save's is kept")),
        "{:?}",
        session.notes
    );
    let meta: AgentMeta =
        serde_json::from_str(&std::fs::read_to_string(&meta_path).unwrap()).unwrap();
    assert_eq!(meta.preset, "porter", "agent.json corrected");
    // `--preset human` is "another preset" for this world.
    let Err(e) = build_world(&resume_args(&home, Some("human")), SEED, NOW, |_| {}) else {
        panic!("resumed as another preset");
    };
    assert!(e.contains("plays the actor preset `porter`"), "{e}");
    let _ = std::fs::remove_dir_all(&home);
}

/// Rewrites the save of `Wren`'s world `W` under `home` through `edit`.
fn edit_save(home: &Path, edit: impl FnOnce(&mut Value)) {
    let path = home.join("agents").join("wren").join("w.world.json");
    let mut v: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    edit(&mut v);
    std::fs::write(&path, serde_json::to_vec(&v).unwrap()).unwrap();
}

/// Review fix 6: a save whose preset copy was edited into an invalid one is
/// never spawned.
#[test]
fn resume_with_an_invalid_saved_preset_is_refused() {
    let home = tmp("bad-header");
    let _ = build_world(&new_args(&home, None), SEED, NOW, |_| {}).unwrap();
    edit_save(&home, |v| {
        v["header"]["actor_preset"]["model"] = json!("dragon");
    });
    let Err(e) = build_world(&resume_args(&home, None), SEED, NOW, |_| {}) else {
        panic!("an invalid saved preset spawned");
    };
    assert!(e.contains("unknown model `dragon`"), "{e}");
    let _ = std::fs::remove_dir_all(&home);
}

/// Review fix 6: a save from before presets (no preset in its header, its
/// scenario or `agent.json`) resumes as the built-in human.
#[test]
fn an_old_save_resumes_as_the_human() {
    let home = tmp("old-save");
    let (mut app, _, dir) = build_world(&new_args(&home, None), SEED, NOW, |_| {}).unwrap();
    run_ticks(app.world_mut(), 20);
    save_now(app.world_mut(), &dir, NOW).unwrap();
    drop(app);
    edit_save(&home, |v| {
        let header = v["header"].as_object_mut().unwrap();
        header.remove("preset");
        header.remove("actor_preset");
        for a in header["scenario"]["actors"].as_array_mut().unwrap() {
            a.as_object_mut().unwrap().remove("preset");
        }
        for e in v["entities"].as_array_mut().unwrap() {
            let c = e["components"].as_object_mut().unwrap();
            c.remove("allowed_actions");
            c.remove("actor_model");
        }
    });
    let meta_path = dir.dir.join("agent.json");
    let mut meta: Value =
        serde_json::from_str(&std::fs::read_to_string(&meta_path).unwrap()).unwrap();
    meta.as_object_mut().unwrap().remove("preset");
    std::fs::write(&meta_path, meta.to_string()).unwrap();
    let (back, start, _) = build_world(&resume_args(&home, None), SEED, NOW, |_| {}).unwrap();
    assert!(matches!(start, Start::Resumed(_)), "{start:?}");
    let session = back.world().resource::<ExplorerSession>();
    assert_eq!(session.meta.preset, "human");
    assert_eq!(session.preset, testbed_content::actors::human_preset());
    assert!(session.notes.is_empty(), "{:?}", session.notes);
    let (_, e) = entity(&back, "Wren");
    assert_eq!(back.world().get::<ReachSense>(e).unwrap().radius_mm, 1500);
    assert!(
        back.world().get::<AllowedActions>(e).is_none(),
        "may do everything"
    );
    let _ = std::fs::remove_dir_all(&home);
}

/// Review fixes 6 and 7: `--fresh-world` reads the preset file again (an
/// edit applies) and, when the file is gone, uses the old save's copy and
/// says so.
#[test]
fn fresh_world_rereads_the_preset_file() {
    let home = tmp("fresh");
    write_preset(&home, "porter", &porter(&home));
    let _ = build_world(&new_args(&home, Some("porter")), SEED, NOW, |_| {}).unwrap();
    let fresh = HostArgs {
        on_mismatch: Some(Mismatch::FreshWorld),
        ..resume_args(&home, None)
    };
    let speed = |app: &App| {
        let (_, e) = entity(app, "Wren");
        app.world().get::<TestbedUnit>(e).unwrap().speed_mm_per_tick
    };
    write_preset(
        &home,
        "porter",
        &human_json(&home, |v| v["speed_mm_per_tick"] = json!(90)),
    );
    let (app, start, _) = build_world(&fresh, SEED, NOW, |_| {}).unwrap();
    assert!(matches!(start, Start::Rebuilt));
    assert_eq!(speed(&app), 90, "the edited file applies");
    assert!(app.world().resource::<ExplorerSession>().notes.is_empty());
    drop(app);
    std::fs::remove_file(home.join("actors").join("porter.json")).unwrap();
    let (app, start, _) = build_world(&fresh, SEED, NOW, |_| {}).unwrap();
    assert!(matches!(start, Start::Rebuilt));
    assert_eq!(speed(&app), 90, "the old save's copy");
    let session = app.world().resource::<ExplorerSession>();
    assert_eq!(session.meta.preset, "porter");
    assert!(
        session.notes.iter().any(|n| n.contains("is gone")),
        "{:?}",
        session.notes
    );
    let _ = std::fs::remove_dir_all(&home);
}
