//! The `walls` observation section (`testbed_content::walls`): an agent
//! playing the explorer senses the compound's walls and its gate from the
//! start position (`docs/_notes/explorer-e2e.md`, finding P3), gaps without
//! a door (the ruin's), static-layer walls, and observing changes no state.

use bevy_app::App;
use npc_framework::agent_io;
use npc_framework::prelude::*;
use serde_json::{Value, json};
use testbed_content::{TestbedIds, try_new_sim_app_with};

fn app(name: &str) -> App {
    let mut app = try_new_sim_app_with(1, name, |_| {}).unwrap_or_else(|e| panic!("{name}: {e}"));
    app.finish();
    app.cleanup();
    app
}

fn id(app: &App, name: &str) -> SimId {
    app.world().resource::<TestbedIds>().id(name)
}

fn observe(app: &mut App, unit: SimId, format: &str) -> Value {
    agent_io::api::call(
        app.world_mut(),
        "agent/observe",
        Some(json!({"unit": unit.0, "format": format})),
    )
    .unwrap_or_else(|e| panic!("observe: {e:?}"))
}

fn explorer(app: &App) -> SimId {
    app.world().resource::<TestbedIds>().units[0]
}

fn opening_named<'a>(walls: &'a Value, name: &str) -> Option<&'a Value> {
    walls["openings"]
        .as_array()?
        .iter()
        .find(|o| o["name"] == json!(name))
}

#[test]
fn the_explorer_senses_the_gate_from_its_start() {
    let mut app = app("explorer");
    let unit = explorer(&app);
    let gate = id(&app, "gate");
    let walls = observe(&mut app, unit, "json")["sections"]["walls"].clone();

    assert_eq!(walls["range_mm"], json!(15_000));
    // The gate: behind the explorer (it faces north from (0, -6.5 m)), in
    // the gap of the south wall; senses here need no line of sight.
    let o = opening_named(&walls, "gate").unwrap_or_else(|| panic!("no gate in {walls:#}"));
    assert_eq!(o["kind"], json!("door"));
    assert_eq!(o["door"], json!(gate.0));
    assert_eq!(o["state"], json!("closed"));
    assert_eq!(o["at"], json!([500, -9500]));
    assert_eq!(o["width_mm"], json!(1000));
    assert_eq!(o["in_wall"], json!("east-west"));
    assert_eq!(o["dir"], json!("south"));
    assert_eq!(o["dist_mm"], json!(3041));
    // The gate is the nearest opening; the inner doors are listed too.
    assert_eq!(walls["openings"][0]["name"], json!("gate"));
    assert!(opening_named(&walls, "workshop_door").is_some());
    let store = opening_named(&walls, "store_door").unwrap();
    assert_eq!(store["state"], json!("locked"));

    // The nearest wall: the south wall's west run, its face 2.5 m south
    // (distances are to the nearest point of the wall's cells).
    let first = &walls["walls"][0];
    assert_eq!(first["runs"], json!("east-west"));
    assert_eq!(first["from"], json!([-11500, -9500]));
    assert_eq!(first["to"], json!([-500, -9500]));
    assert_eq!(first["length_mm"], json!(12_000));
    assert_eq!(first["nearest"], json!([-1, -9001]));
    assert_eq!(first["dist_mm"], json!(2501));
    assert_eq!(first["dir"], json!("south"));
    // Every listed thing is within range, ascending.
    for list in ["walls", "openings"] {
        let d: Vec<u64> = walls[list]
            .as_array()
            .unwrap()
            .iter()
            .map(|w| w["dist_mm"].as_u64().unwrap())
            .collect();
        assert!(d.windows(2).all(|p| p[0] <= p[1]) && d.iter().all(|&d| d <= 15_000));
    }
    // The ruin (z 26..30 m) is out of range.
    assert!(
        walls["openings"]
            .as_array()
            .unwrap()
            .iter()
            .all(|o| o["kind"] == json!("door")),
        "{walls:#}"
    );

    // And in the text rendering.
    let text = observe(&mut app, unit, "text")["text"]
        .as_str()
        .unwrap()
        .to_owned();
    let gate_text = format!(
        "Openings within 15.00 m: the gate (closed door #{}) at (0.50 m, -9.50 m) in an east-west wall, 3.04 m south",
        gate.0
    );
    assert!(text.contains(&gate_text), "{text}");
    assert!(
        text.contains("Walls within 15.00 m: an east-west wall from "),
        "{text}"
    );
}

#[test]
fn a_gap_without_a_door_is_an_opening() {
    // An actor south of the ruin's gap (cell (0, 26)).
    let doc = json!({
        "version": 1, "content": "testbed", "name": "ruin_gap", "seed": 1,
        "world": {"layout": "frontier", "furnish": true, "store_locked": true},
        "actors": [{"kind": "unit", "template": "scout", "pos": [500, 22500], "facing": [0, 1]}]
    });
    let doc = agent_io::ScenarioDoc::from_value(doc).unwrap();
    let mut app = testbed_content::scenario::load_with(&doc, 1, |_| {})
        .unwrap()
        .app;
    app.finish();
    app.cleanup();
    let unit = explorer(&app);
    let walls = observe(&mut app, unit, "json")["sections"]["walls"].clone();
    let gap = walls["openings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["kind"] == json!("gap"))
        .unwrap_or_else(|| panic!("no gap in {walls:#}"));
    assert_eq!(gap["at"], json!([500, 26500]));
    assert_eq!(gap["door"], Value::Null);
    assert_eq!(gap["in_wall"], json!("east-west"));
    assert_eq!(gap["dir"], json!("north"));
    let text = observe(&mut app, unit, "text")["text"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(
        text.contains("a 1 m gap at (0.50 m, 26.50 m) in an east-west wall, 4.00 m north"),
        "{text}"
    );
}

#[test]
fn static_layer_walls_are_sensed() {
    let mut app = app("explorer");
    let unit = explorer(&app);
    // A static north-south run of 3 cells west of the explorer, at x = -5.
    const WALL: CellInfo = CellInfo {
        flags: CellFlags {
            blocking: true,
            occluding: true,
            tags: 0,
        },
        extra_cost: 0,
    };
    {
        let mut grid = app.world_mut().resource_mut::<SpatialGrid>();
        for z in -8..=-6 {
            grid.set_static(CellCoord::new(-5, z), WALL);
        }
    }
    let walls = observe(&mut app, unit, "json")["sections"]["walls"].clone();
    let w = walls["walls"]
        .as_array()
        .unwrap()
        .iter()
        .find(|w| w["id"].is_null())
        .unwrap_or_else(|| panic!("no static run in {walls:#}"));
    assert_eq!(w["from"], json!([-4500, -7500]));
    assert_eq!(w["to"], json!([-4500, -5500]));
    assert_eq!(w["runs"], json!("north-south"));
    assert_eq!(w["length_mm"], json!(3000));
    assert_eq!(w["dir"], json!("west"));
}

#[test]
fn observing_walls_is_read_only_and_deterministic() {
    let mut a = app("explorer");
    let mut b = app("explorer");
    run_ticks(a.world_mut(), 40);
    run_ticks(b.world_mut(), 40);
    let unit = explorer(&a);
    let before = state_hash(a.world_mut());
    let first = observe(&mut a, unit, "text");
    let again = observe(&mut a, unit, "text");
    assert_eq!(state_hash(a.world_mut()), before);
    assert_eq!(first["text"], again["text"]);
    assert_eq!(
        observe(&mut a, unit, "json")["sections"]["walls"],
        observe(&mut b, unit, "json")["sections"]["walls"]
    );
    // Observed and unobserved runs stay identical.
    run_ticks(a.world_mut(), 40);
    run_ticks(b.world_mut(), 40);
    assert_eq!(state_hash(a.world_mut()), state_hash(b.world_mut()));
}
