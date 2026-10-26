//! Actor Testbed scenario tests (wave G1, actor-capabilities spec §4.9):
//! one or more headless tests per feature, each on an embedded scenario file
//! (`content/testbed_content/scenarios/`). The wave F4 glue (orders to
//! vehicles, `Interact` orders, crafting XP, study/teach, agent affordances
//! for command/vehicles/crafting) is tested in `tests/glue.rs` (wave G2b).

use bevy_app::App;
use bevy_ecs::prelude::*;
use npc_framework::agent_io::{self, ScenarioLoaders};
use npc_framework::agent_io::{ControlMode, ExternalController};
use npc_framework::kernel::hash::state_hash;
use npc_framework::kernel::journal::{JournalEntry, JournalKind};
use npc_framework::prelude::*;
use serde_json::{Value, json};
use testbed_content::layout::ascii_map;
use testbed_content::{PropKind, SCENARIO_FILES, TestbedDrive, TestbedIds, try_new_sim_app_with};

// ---------------------------------------------------------------- helpers

/// Scenario `name` with seed 1, the journal on, finished.
fn app(name: &str) -> App {
    app_seeded(name, 1)
}

fn app_seeded(name: &str, seed: u64) -> App {
    let mut app = try_new_sim_app_with(seed, name, |app| {
        app.add_plugins(JournalPlugin {
            verbose: false,
            capacity: 400_000,
        });
    })
    .unwrap_or_else(|e| panic!("{name}: {e}"));
    app.finish();
    app.cleanup();
    app
}

fn id(app: &App, name: &str) -> SimId {
    app.world().resource::<TestbedIds>().id(name)
}

fn ent(app: &App, id: SimId) -> Entity {
    app.world()
        .resource::<SimIndex>()
        .get(id)
        .expect("in the sim")
}

fn get<T: Component + Clone>(app: &App, id: SimId) -> T {
    app.world()
        .get::<T>(ent(app, id))
        .cloned()
        .unwrap_or_else(|| panic!("#{} has no {}", id.0, std::any::type_name::<T>()))
}

fn pos(app: &App, id: SimId) -> SimPos {
    get::<SimPos>(app, id)
}

fn run(app: &mut App, n: u64) {
    run_ticks(app.world_mut(), n);
}

fn content<C: ContentCommand>(app: &mut App, cmd: &C) -> Result<(), CommandError> {
    submit(app.world_mut(), None, SimCommand::content(cmd)).map(|_| ())
}

fn interact(app: &mut App, unit: SimId, verb: &str, target: SimId) -> Result<(), CommandError> {
    content(app, &Interact::new(unit, verb, target))
}

fn order(app: &mut App, player: u32, units: &[SimId], o: UnitOrder) -> Result<(), CommandError> {
    content(app, &IssueOrder::new(PlayerId(player), units.to_vec(), o))
}

fn reason(err: CommandError) -> String {
    match err {
        CommandError::Unavailable { reason } => reason,
        other => format!("{other:?}"),
    }
}

fn journal(app: &App) -> Vec<JournalEntry> {
    app.world()
        .resource::<Journal>()
        .entries
        .iter()
        .cloned()
        .collect()
}

fn senses(app: &App, eye: SimId, target: SimId) -> bool {
    app.world()
        .get::<PerceptionMemory>(ent(app, eye))
        .and_then(|m| m.get(target))
        .is_some_and(|p| p.sensed)
}

/// `(verb, outcome)` of every finished `Interacted` entry of `unit` (not
/// the `"started"` ones).
fn interactions(app: &App, unit: SimId) -> Vec<(String, String)> {
    journal(app)
        .into_iter()
        .filter(|e| e.unit == Some(unit))
        .filter_map(|e| match e.kind {
            JournalKind::Interacted { verb, outcome, .. } if outcome != "started" => {
                Some((verb, outcome))
            }
            _ => None,
        })
        .collect()
}

/// Items of `kind` lying in the world (with a position), as `(id, stack)`.
fn ground_items(app: &mut App, kind: &str) -> Vec<(SimId, u32)> {
    let world = app.world_mut();
    let mut v: Vec<(SimId, u32)> = world
        .query::<(&SimId, &Item, &SimPos)>()
        .iter(world)
        .filter(|(_, i, _)| i.kind == kind)
        .map(|(&id, i, _)| (id, i.stack))
        .collect();
    v.sort_unstable();
    v
}

/// The carried items of `unit` as `(kind, stack, quality)`.
fn carried(app: &App, unit: SimId) -> Vec<(String, u32, i32)> {
    let inv = get::<Inventory>(app, unit);
    let mut ids = inv.items.clone();
    ids.extend(get::<Equipment>(app, unit).items());
    ids.into_iter()
        .map(|i| {
            let item = get::<Item>(app, i);
            (item.kind, item.stack, item.quality)
        })
        .collect()
}

fn call(app: &mut App, method: &str, params: Value) -> Value {
    agent_io::call(app.world_mut(), method, Some(params))
        .unwrap_or_else(|e| panic!("{method}: {e:?}"))
}

fn ids_of(list: &Value) -> Vec<u64> {
    list.as_array()
        .map(|a| a.iter().filter_map(|p| p["id"].as_u64()).collect())
        .unwrap_or_default()
}

// ---------------------------------------------------------------- content

#[test]
fn every_scenario_file_loads_through_the_agent_io_registry() {
    let mut loaders = ScenarioLoaders::default();
    testbed_content::register_scenario_loader(&mut loaders);
    for (name, text) in SCENARIO_FILES {
        let doc = agent_io::ScenarioDoc::parse(text).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(doc.content, "testbed", "{name}");
        let mut loaded = agent_io::scenario::load(&loaders, &doc, None)
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(loaded.actors.len(), doc.actors.len(), "{name}");
        loaded.app.finish();
        loaded.app.cleanup();
        run_ticks(loaded.app.world_mut(), 3);
    }
}

#[test]
fn the_default_testbed_has_every_piece() {
    let mut app = app("testbed");
    let ids = app.world().resource::<TestbedIds>().clone();
    assert_eq!(ids.units.len(), 8);
    let owners: Vec<u32> = ids
        .units
        .iter()
        .map(|&u| get::<Owner>(&app, u).0.0)
        .collect();
    assert_eq!(owners, [1, 1, 1, 1, 2, 2, 2, 2]);
    // Every actor: sensors, a sheet from a template, XP, an inventory.
    let mut templates = Vec::new();
    for &u in &ids.units {
        let e = ent(&app, u);
        let w = app.world();
        assert!(w.get::<Sensors>(e).is_some());
        assert!(w.get::<CharacterSheet>(e).is_some());
        assert!(w.get::<Experience>(e).is_some());
        assert!(w.get::<Inventory>(e).is_some());
        assert!(w.get::<Commandable>(e).is_some());
        templates.push(get::<testbed_content::TestbedUnit>(&app, u).template);
    }
    templates.dedup();
    assert!(templates.len() >= 6, "{templates:?}");
    // Skills differ between templates.
    let carp = effective_skill(app.world(), id(&app, "carpenter"), "Carpentry").unwrap();
    let smith = effective_skill(app.world(), id(&app, "smith"), "Carpentry").unwrap();
    assert!(carp > smith + 3, "carpenter {carp}, smith {smith}");
    // World pieces.
    assert!(get::<Door>(&app, id(&app, "store_door")).is_locked());
    assert!(!get::<Door>(&app, id(&app, "gate")).open);
    assert_eq!(
        get::<Vehicle>(&app, id(&app, "cart")).control,
        VehicleControl::ByCrew
    );
    assert_eq!(
        get::<Vehicle>(&app, id(&app, "wagon")).control,
        VehicleControl::Direct
    );
    assert!(get::<Workstation>(&app, id(&app, "workbench")).is("workbench"));
    assert!(get::<Workstation>(&app, id(&app, "anvil")).is("anvil"));
    for name in [
        "axe",
        "store_key",
        "bread",
        "logs",
        "book",
        "pickaxe",
        "hammer",
    ] {
        get::<Item>(&app, id(&app, name));
    }
    assert_eq!(
        get::<ToolTarget>(&app, id(&app, "tree_0")).required_tag,
        "tool:axe"
    );
    // The woodcutter starts with the axe in hand.
    let wc = id(&app, "woodcutter");
    assert_eq!(carried(&app, wc), vec![("axe".to_owned(), 1, 0)]);
    run(&mut app, 2);
    let map = ascii_map(app.world(), -13, 19, 11, -16);
    println!("{map}");
    let row = |z: i32| -> String {
        map.lines()
            .find(|l| l.starts_with(&format!("{z:>4} ")))
            .unwrap()
            .to_owned()
    };
    // Column of cell x is 5 + (x + 13).
    let at = |z: i32, x: i32| {
        row(z)
            .chars()
            .nth(usize::try_from(5 + x + 13).unwrap())
            .unwrap()
    };
    assert_eq!(at(3, 6), 'L', "store door locked");
    assert_eq!(at(3, -6), 'D', "workshop door");
    assert_eq!(at(-10, 0), 'D', "gate");
    assert_eq!(at(-10, 5), '#', "south wall");
    assert_eq!(at(7, -9), 'W');
    assert_eq!(at(-6, 4), 'c');
    assert_eq!(at(-15, 0), 'V');
}

#[test]
fn bad_scenarios_are_rejected_with_a_reason() {
    let mut loaders = ScenarioLoaders::default();
    testbed_content::register_scenario_loader(&mut loaders);
    let load = |v: Value| {
        let doc = agent_io::ScenarioDoc::from_value(v).unwrap();
        agent_io::scenario::load(&loaders, &doc, None)
            .err()
            .unwrap()
    };
    let e = load(json!({"version":1,"content":"testbed",
        "actors":[{"kind":"unit","template":"wizard","pos":[0,0]}]}));
    assert!(e.contains("unknown sheet template `wizard`"), "{e}");
    let e = load(json!({"version":1,"content":"testbed",
        "actors":[{"kind":"villager","pos":[0,0]}]}));
    assert!(e.contains("unknown kind `villager`"), "{e}");
    let e = load(
        json!({"version":1,"content":"testbed","world":{"furnish":false,
        "items":[{"kind":"gold","pos":[0,0]}]}}),
    );
    assert!(e.contains("unknown item kind `gold`"), "{e}");
    let e = load(json!({"version":1,"content":"testbed","world":{"walls":3}}));
    assert!(e.contains("unknown field `walls`"), "{e}");
}

// ---------------------------------------------------------------- perception

#[test]
fn sees_through_an_open_door_but_not_a_closed_one() {
    let mut app = app("doors");
    let (eye, target, door) = (
        id(&app, "eye"),
        id(&app, "target"),
        id(&app, "workshop_door"),
    );
    run(&mut app, 10);
    assert!(!senses(&app, eye, target), "closed door occludes");

    interact(&mut app, eye, "open", door).unwrap();
    run(&mut app, 15);
    assert!(get::<Door>(&app, door).open);
    assert!(senses(&app, eye, target), "seen through the open door");
    let memory = get::<PerceptionMemory>(&app, eye);
    assert_eq!(memory.get(target).unwrap().via, SenseKind::Vision);

    interact(&mut app, eye, "close", door).unwrap();
    run(&mut app, 15);
    assert!(!get::<Door>(&app, door).open);
    assert!(!senses(&app, eye, target), "closed again");
    assert!(
        get::<PerceptionMemory>(&app, eye).get(target).is_some(),
        "still remembered"
    );
    let lost = journal(&app).into_iter().any(|e| {
        e.unit == Some(eye) && matches!(e.kind, JournalKind::Lost { target: t, .. } if t == target)
    });
    assert!(lost, "journal Lost");
}

/// `(tick, unit, target, success)` of every Per detection roll.
fn detect_rolls(app: &App) -> Vec<(u64, SimId, i32, bool)> {
    journal(app)
        .into_iter()
        .filter_map(|e| match e.kind {
            JournalKind::Roll {
                skill,
                purpose,
                target,
                success,
                ..
            } if purpose == "perception.detect" => {
                assert_eq!(skill, "Per");
                Some((e.tick, e.unit.unwrap(), target, success))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn per_roll_detection_follows_the_perception_stat() {
    let mut app = app("perception");
    let (sharp, dull) = (id(&app, "sharp"), id(&app, "dull"));
    let (t_sharp, t_dull) = (id(&app, "target_sharp"), id(&app, "target_dull"));
    run(&mut app, 300);
    let rolls = detect_rolls(&app);
    let of = |u: SimId| -> Vec<(u64, i32, bool)> {
        rolls
            .iter()
            .filter(|r| r.1 == u)
            .map(|r| (r.0, r.2, r.3))
            .collect()
    };
    let (s, d) = (of(sharp), of(dull));
    assert!(!s.is_empty() && !d.is_empty(), "both observers roll");
    // Same distance band for both: the targets differ by Per (12 vs 8).
    let ts: Vec<i32> = s.iter().map(|r| r.1).collect();
    let td: Vec<i32> = d.iter().map(|r| r.1).collect();
    assert!(ts.iter().all(|&t| t == ts[0]) && td.iter().all(|&t| t == td[0]));
    assert_eq!(ts[0] - td[0], 4, "Per 12 vs Per 8");
    // The sharp scout spots its target (and then stops rolling for it).
    assert!(senses(&app, sharp, t_sharp));
    assert_eq!(s.iter().filter(|r| r.2).count(), 1, "{s:?}");
    assert!(
        s.len() <= d.len(),
        "the dull minion needs more tries: {s:?} vs {d:?}"
    );
    // A detection is exactly the first successful roll.
    let first = s.iter().find(|r| r.2).unwrap().0;
    let perceived = journal(&app)
        .into_iter()
        .find(|e| {
            e.unit == Some(sharp)
                && matches!(e.kind, JournalKind::Perceived { target, .. } if target == t_sharp)
        })
        .unwrap();
    assert_eq!(perceived.tick, first);
    let _ = t_dull;
}

#[test]
fn agent_observe_shows_only_what_the_unit_perceives() {
    let mut app = app("agent");
    let (eye, target, door) = (
        id(&app, "eye"),
        id(&app, "target"),
        id(&app, "workshop_door"),
    );
    run(&mut app, 5);
    let obs = call(&mut app, "agent/observe", json!({"unit": eye.0}));
    assert_eq!(obs["omniscient"], json!(false));
    assert_eq!(obs["sections"]["testbed"]["template"], json!("scout"));
    assert!(
        obs["sections"]["sheet"].is_object(),
        "sheet section (rules glue)"
    );
    let p = &obs["sections"]["perception"];
    assert!(!ids_of(&p["sensed"]).contains(&u64::from(target.0)), "{p}");
    let state = call(&mut app, "world/state", json!({}));
    let in_state = state["entities"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["sim_id"] == json!(target.0));
    assert!(in_state, "world/state (debug) shows everything");

    interact(&mut app, eye, "open", door).unwrap();
    run(&mut app, 15);
    let obs = call(&mut app, "agent/observe", json!({"unit": eye.0}));
    let sensed = ids_of(&obs["sections"]["perception"]["sensed"]);
    assert!(sensed.contains(&u64::from(target.0)), "{sensed:?}");
    let text = call(
        &mut app,
        "agent/observe",
        json!({"unit": eye.0, "format": "text"}),
    );
    assert!(text["text"].as_str().unwrap().contains("scout"), "{text}");
}

// ---------------------------------------------------------------- doors and keys

#[test]
fn the_key_unlocks_the_store_door() {
    let mut app = app("locked_store");
    let (warden, door) = (id(&app, "warden"), id(&app, "store_door"));
    let e = interact(&mut app, warden, "open", door).unwrap_err();
    assert_eq!(reason(e), "door is locked");
    interact(&mut app, warden, "unlock", door).unwrap();
    run(&mut app, 25);
    let d = get::<Door>(&app, door);
    assert!(!d.is_locked() && !d.open, "{d:?}");
    interact(&mut app, warden, "open", door).unwrap();
    run(&mut app, 15);
    assert!(get::<Door>(&app, door).open);
    assert_eq!(
        interactions(&app, warden),
        [
            ("unlock".to_owned(), "success".to_owned()),
            ("open".to_owned(), "success".to_owned())
        ]
    );
}

#[test]
fn pick_up_the_key_then_walk_into_the_store() {
    let mut app = app("locked_store");
    let (keeper, key, door) = (
        id(&app, "keeper"),
        id(&app, "store_key"),
        id(&app, "store_door"),
    );
    // Without the key the lock holds.
    assert!(interact(&mut app, keeper, "unlock", door).is_err());
    interact(&mut app, keeper, "pick_up", key).unwrap();
    run(&mut app, 15);
    assert_eq!(get::<CarriedBy>(&app, key).holder, keeper);
    // An order into the store: the path goes through the locked door, which
    // the keeper unlocks and opens on the way (nav door markers).
    order(
        &mut app,
        1,
        &[keeper],
        UnitOrder::Move { pos: (6500, 6500) },
    )
    .unwrap();
    for _ in 0..60 {
        run(&mut app, 10);
        if pos(&app, keeper) == SimPos::new(6500, 6500) {
            break;
        }
    }
    assert_eq!(pos(&app, keeper), SimPos::new(6500, 6500));
    let d = get::<Door>(&app, door);
    assert!(d.open && !d.is_locked(), "{d:?}");
    let verbs: Vec<String> = interactions(&app, keeper)
        .into_iter()
        .map(|v| v.0)
        .collect();
    assert_eq!(verbs, ["pick_up", "unlock", "open"]);
}

// ---------------------------------------------------------------- tools

#[test]
fn chopping_needs_an_equipped_axe() {
    let mut app = app("chop");
    let (jack, bare, tree) = (id(&app, "lumberjack"), id(&app, "bare"), id(&app, "tree_0"));
    let e = interact(&mut app, bare, "use_tool_on", tree).unwrap_err();
    assert_eq!(reason(e), "needs tool:axe");
    let before: u32 = ground_items(&mut app, "log").iter().map(|l| l.1).sum();
    let mut chopped = false;
    for _ in 0..5 {
        interact(&mut app, jack, "use_tool_on", tree).unwrap();
        run(&mut app, 65);
        let after: u32 = ground_items(&mut app, "log").iter().map(|l| l.1).sum();
        if after == before + 2 {
            chopped = true;
            break;
        }
    }
    assert!(chopped, "{:?}", interactions(&app, jack));
    assert_eq!(get::<ToolTarget>(&app, tree).remaining, Some(2));
    // The chop is a Woodcutting check (interaction × rules glue).
    let rolled = journal(&app).into_iter().any(|e| {
        e.unit == Some(jack)
            && matches!(&e.kind, JournalKind::Roll { skill, .. } if skill == "Woodcutting")
    });
    assert!(rolled);
}

#[test]
fn the_woodcutter_chops_trees_on_its_own() {
    let mut app = app("testbed");
    let wc = id(&app, "woodcutter");
    run(&mut app, 1500);
    let chops = interactions(&app, wc)
        .into_iter()
        .filter(|(v, o)| v == "use_tool_on" && o == "success")
        .count();
    assert!(chops >= 1, "{:?}", interactions(&app, wc));
    let logs: u32 = ground_items(&mut app, "log").iter().map(|l| l.1).sum();
    assert!(logs >= 2 + 2, "the pile plus at least one chop: {logs}");
}

// ---------------------------------------------------------------- crafting

/// `(margin, quality)` of every `CraftDone` and the number of failures of `unit`.
fn crafts(app: &App, unit: SimId) -> (Vec<(i32, i32)>, usize) {
    let mut done = Vec::new();
    let mut failed = 0;
    for e in journal(app).into_iter().filter(|e| e.unit == Some(unit)) {
        match e.kind {
            JournalKind::CraftDone {
                margin, quality, ..
            } => done.push((margin, quality)),
            JournalKind::CraftFailed { outcome, .. } if outcome != "rejected" => failed += 1,
            _ => {}
        }
    }
    (done, failed)
}

#[test]
fn plank_quality_follows_carpentry_skill() {
    let mut app = app("craft");
    let bench = id(&app, "workbench");
    let (master, apprentice) = (id(&app, "master"), id(&app, "apprentice"));
    for unit in [master, apprentice] {
        for _ in 0..6 {
            content(&mut app, &Craft::new(unit, "planks").at(bench)).unwrap();
            run(&mut app, 42);
        }
    }
    let (m_done, m_failed) = crafts(&app, master);
    let (a_done, a_failed) = crafts(&app, apprentice);
    assert_eq!(m_done.len() + m_failed, 6);
    assert_eq!(a_done.len() + a_failed, 6);
    let quality = |d: &[(i32, i32)]| d.iter().map(|x| x.1).sum::<i32>();
    assert!(
        m_done.len() > a_done.len(),
        "master {m_done:?}, apprentice {a_done:?}"
    );
    assert!(quality(&m_done) > quality(&a_done));
    assert!(
        m_done.iter().any(|d| d.1 > 0),
        "a skilled carpenter makes better planks"
    );
    // Planks are in the master's inventory, two per success.
    let planks: u32 = carried(&app, master)
        .iter()
        .filter(|c| c.0 == "plank")
        .map(|c| c.1)
        .sum();
    assert_eq!(planks, 2 * u32::try_from(m_done.len()).unwrap());
    // Every attempt is a Carpentry roll (journal `Roll`, purpose craft.planks).
    let rolls = journal(&app)
        .into_iter()
        .filter(
            |e| matches!(&e.kind, JournalKind::Roll { purpose, .. } if purpose == "craft.planks"),
        )
        .count();
    assert_eq!(rolls, 12);
}

// ---------------------------------------------------------------- vehicles

#[test]
fn mount_the_cart_by_the_interact_verb_and_drive_it() {
    let mut app = app("vehicles");
    let (driver, passenger, cart) = (id(&app, "driver"), id(&app, "passenger"), id(&app, "cart"));
    interact(&mut app, driver, "mount", cart).unwrap();
    run(&mut app, 3);
    let m = get::<MountedIn>(&app, driver);
    assert_eq!((m.vehicle, m.seat), (cart, 0), "the driver seat first");
    interact(&mut app, passenger, "mount", cart).unwrap();
    run(&mut app, 3);
    assert_eq!(get::<MountedIn>(&app, passenger).seat, 1);
    let e = content(
        &mut app,
        &TestbedDrive {
            unit: passenger.0,
            pos: (4500, -1500),
        },
    )
    .unwrap_err();
    assert!(reason(e).contains("not driving"));

    content(
        &mut app,
        &TestbedDrive {
            unit: driver.0,
            pos: (4500, -1500),
        },
    )
    .unwrap();
    run(&mut app, 120);
    assert_eq!(pos(&app, cart), SimPos::new(4500, -1500));
    assert_eq!(
        pos(&app, driver),
        SimPos::new(4500, -1500),
        "the crew rides along"
    );
    assert_eq!(
        pos(&app, passenger),
        SimPos::new(4500, -2200),
        "seat offset"
    );
    interact(&mut app, passenger, "dismount", cart).unwrap();
    run(&mut app, 3);
    assert!(app.world().get::<MountedIn>(ent(&app, passenger)).is_none());
}

#[test]
fn the_direct_wagon_paths_around_the_compound() {
    let mut app = app("vehicles");
    let wagon = id(&app, "wagon");
    let goal = SimPos::new(500, 14_500);
    let e = order(
        &mut app,
        2,
        &[wagon],
        UnitOrder::Move {
            pos: (goal.x, goal.z),
        },
    )
    .unwrap_err();
    assert!(
        format!("{e:?}").contains('2'),
        "player 2 does not own it: {e:?}"
    );
    order(
        &mut app,
        1,
        &[wagon],
        UnitOrder::Move {
            pos: (goal.x, goal.z),
        },
    )
    .unwrap();
    let mut ticks = 0;
    while pos(&app, wagon) != goal && ticks < 3000 {
        run(&mut app, 10);
        ticks += 10;
        let p = pos(&app, wagon);
        let grid = app.world().resource::<SpatialGrid>();
        assert!(
            !grid.is_blocking(grid.cell_of(p)),
            "wagon inside a wall at {p:?}"
        );
    }
    assert_eq!(pos(&app, wagon), goal, "after {ticks} ticks");
    let found = journal(&app)
        .into_iter()
        .any(|e| e.unit == Some(wagon) && matches!(e.kind, JournalKind::PathFound { .. }));
    assert!(found, "the wagon path-finds (npc_nav)");
    // The compound is in the way: the path is far longer than the line.
    assert!(ticks > 30_000 / 70, "{ticks}");
}

// ---------------------------------------------------------------- command

#[test]
fn a_group_move_order_forms_a_formation() {
    let mut app = app("formation");
    let units: Vec<SimId> = ["a", "b", "c", "d"].iter().map(|n| id(&app, n)).collect();
    let enemy = id(&app, "enemy");
    assert!(order(&mut app, 1, &[enemy], UnitOrder::Move { pos: (0, 0) }).is_err());
    let target = SimPos::new(2500, -6500);
    let cmd = IssueOrder::new(
        PlayerId(1),
        units.clone(),
        UnitOrder::Move {
            pos: (target.x, target.z),
        },
    )
    .formation(Formation {
        kind: FormationKind::Box,
        spacing_mm: 1500,
    });
    content(&mut app, &cmd).unwrap();
    run(&mut app, 500);
    let at: Vec<SimPos> = units.iter().map(|&u| pos(&app, u)).collect();
    for &u in &units {
        assert!(get::<OrderQueue>(&app, u).is_empty(), "#{} still busy", u.0);
    }
    for (i, a) in at.iter().enumerate() {
        for b in &at[i + 1..] {
            assert!(a.dist2(*b) >= 1000 * 1000, "slots apart: {at:?}");
        }
    }
    let n = i32::try_from(at.len()).unwrap();
    let cx = at.iter().map(|p| p.x).sum::<i32>() / n;
    let cz = at.iter().map(|p| p.z).sum::<i32>() / n;
    assert!(
        SimPos::new(cx, cz).dist2(target) <= 1500 * 1500,
        "centroid ({cx}, {cz})"
    );
}

// ---------------------------------------------------------------- determinism

fn hashes(seed: u64, journal: bool) -> Vec<u64> {
    let mut app = if journal {
        app_seeded("testbed", seed)
    } else {
        let mut app = testbed_content::try_new_sim_app(seed, "testbed", 0).unwrap();
        app.finish();
        app.cleanup();
        app
    };
    let mut out = Vec::new();
    for _ in 0..4 {
        run(&mut app, 300);
        out.push(state_hash(app.world_mut()));
    }
    out
}

#[test]
fn the_testbed_is_deterministic() {
    let a = hashes(7, true);
    assert_eq!(a, hashes(7, true), "same seed, same hashes");
    assert_eq!(a, hashes(7, false), "the journal never changes the sim");
    assert_ne!(a.last(), hashes(8, true).last(), "the seed matters");
}

/// `cargo test -p testbed_content --test scenarios print_testbed -- --ignored --nocapture`:
/// the map at ticks 0 and 1200 and a journal summary (kinds by count).
#[test]
#[ignore = "prints the testbed for humans"]
fn print_testbed() {
    let mut app = app("testbed");
    run(&mut app, 1);
    println!("tick 1\n{}", ascii_map(app.world(), -13, 20, 11, -17));
    run(&mut app, 1199);
    println!("tick 1200\n{}", ascii_map(app.world(), -13, 20, 11, -17));
    let mut counts = std::collections::BTreeMap::new();
    for e in journal(&app) {
        *counts.entry(e.kind.name()).or_insert(0u32) += 1;
    }
    println!("{counts:?}");
    for &u in &app.world().resource::<TestbedIds>().units.clone() {
        let xp = get::<Experience>(&app, u);
        println!(
            "#{} at {:?} xp {:?} carries {:?}",
            u.0,
            pos(&app, u),
            xp,
            carried(&app, u)
        );
    }
}

// ---------------------------------------------------------------- explorer

#[test]
fn explorer_loads_with_one_controlled_actor() {
    let app = app("explorer");
    let ids = app.world().resource::<TestbedIds>().clone();
    assert_eq!(ids.units.len(), 1);
    assert_eq!(ids.units[0], id(&app, "explorer"));
    let c = get::<ExternalController>(&app, id(&app, "explorer"));
    assert_eq!(c.id, "explorer");
    assert_eq!(c.mode, ControlMode::Direct);
}

#[test]
fn explorer_compound_ids_match_testbed() {
    let explorer = app("explorer");
    let smithy = app("smithy");
    for name in ["gate", "workshop_door", "store_door", "store_key"] {
        assert_eq!(id(&explorer, name), id(&smithy, name), "{name}");
    }
}

/// Outside the compound walls (cells x -12..=12, z -10..=10).
fn outside(p: SimPos) -> bool {
    p.x < -12_000 || p.x > 13_000 || p.z < -10_000 || p.z > 11_000
}

#[test]
fn explorer_outside_features_exist() {
    let mut app = app("explorer");
    let world = app.world_mut();
    let mut trees = 0;
    let mut rocks = 0;
    for (p, at) in world
        .query::<(&testbed_content::Prop, &SimPos)>()
        .iter(world)
    {
        if !outside(*at) {
            continue;
        }
        match p.kind {
            PropKind::Tree => trees += 1,
            PropKind::OreRock => rocks += 1,
            _ => {}
        }
    }
    assert!(trees >= 12, "{trees} trees outside");
    assert_eq!(rocks, 3);
    for name in ["ruin_chest", "ruin_scroll", "far_book"] {
        assert!(outside(pos(&app, id(&app, name))), "{name}");
    }
    let map = ascii_map(app.world(), -40, 40, 40, -20);
    println!("{map}");
}

#[test]
fn explorer_actor_cannot_see_far_book_at_start() {
    let mut app = app("explorer");
    run(&mut app, 10);
    let eye = id(&app, "explorer");
    assert!(!senses(&app, eye, id(&app, "far_book")));
    let m = get::<PerceptionMemory>(&app, eye);
    assert!(m.get(id(&app, "far_book")).is_none());
}

#[test]
fn explorer_reaches_the_ruin() {
    let mut app = app("explorer");
    let eye = id(&app, "explorer");
    // The gap is the ruin's south wall cell (0, 26). A Direct unit's tree is
    // suspended, so it carries the Move order out through `execute_orders`.
    let gap = (500, 26_500);
    order(&mut app, 1, &[eye], UnitOrder::Move { pos: gap }).unwrap();
    run(&mut app, 1);
    let r = call(
        &mut app,
        "agent/act",
        json!({"unit": eye.0, "controller": "explorer", "affordance": "execute_orders"}),
    );
    assert_eq!(r["accepted"], json!(true), "{r}");
    for _ in 0..240 {
        run(&mut app, 10);
        if pos(&app, eye) == SimPos::new(gap.0, gap.1) {
            break;
        }
    }
    assert_eq!(pos(&app, eye), SimPos::new(gap.0, gap.1));
    assert!(
        journal(&app).iter().any(|e| e.unit == Some(eye)
            && matches!(&e.kind, JournalKind::OrderDone { order } if order == "Move")),
        "the Move order completes"
    );
}
