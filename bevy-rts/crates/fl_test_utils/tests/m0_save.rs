//! M0 saves load under M1 (R78, ADR-0006 M1 refinement), and terrain end to end on the game's
//! features: the open map of M0 content, the navigation debug queries, and determinism on an
//! ASCII river map.
//!
//! `m0_testbed.save` is the base testbed saved at tick 40 by tag `m0` (R81): core v1, no
//! terrain.

use std::path::PathBuf;

use fl_content::MapDef;
use fl_math::{Fx, FxVec2};
use fl_protocol::{MoveCmd, SimId, Tick};
use fl_sim::nav::{NavGrid, PathRules, content_profiles};
use fl_sim::{
    Mobility, MoveOrder, MovementRules, Order, PathFollow, PathQueue, RunConfig, SaveFile,
    SimClock, SimIndex, TerrainGrid, compute_state_hash, load_world, save_world,
};
use fl_test_utils::{
    Scenario, ScenarioBuilder, assert_deterministic, assert_shuffle_invariant, base_content,
};
use fl_view::{SimQuery, SimReply};
use serde_json::{Value, json};

/// The M0 testbed save (protected, blessed).
fn m0_save() -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/replays/m0_testbed.save");
    std::fs::read(&path).unwrap_or_else(|err| panic!("reading {}: {err}", path.display()))
}

fn load_m0() -> Scenario {
    Scenario::load(fl_catalog::features(), base_content(), &m0_save())
}

#[test]
fn m0_testbed_save_loads_under_m1() {
    let content = base_content();
    let run = RunConfig {
        hash_interval: Some(1),
        ..RunConfig::default()
    };
    let loaded = load_world(&fl_catalog::features(), content.clone(), &m0_save(), run)
        .unwrap_or_else(|err| panic!("the M0 save does not load: {err}"));
    let world = loaded.app.world();
    assert_eq!(world.resource::<SimClock>().tick, Tick(40));

    // The fill gave the testbed map's terrain, and the rebuild its navigation grid.
    let map = content
        .kind::<MapDef>()
        .get("base.testbed_1km")
        .expect("the testbed map");
    let expected = TerrainGrid::from_layers(
        map.layers.as_ref().expect("decoded"),
        map.terrain.as_ref().expect("layers").height_m,
    );
    assert_eq!(*world.resource::<TerrainGrid>(), expected);
    let max = content.rules::<PathRules>().max_clearance;
    let fresh = NavGrid::build(
        &expected,
        &content,
        content.rules::<MovementRules>(),
        &content_profiles(&content, map.cell_m, max),
        max,
        map.cell_m,
    );
    assert_eq!(*world.resource::<NavGrid>(), fresh);

    // The R78 fills: an empty path queue, and an idle path on every unit that moves.
    assert_eq!(*world.resource::<PathQueue>(), PathQueue::default());
    let mut mobile = 0;
    for (id, entity) in world.resource::<SimIndex>().iter() {
        if world.get::<Mobility>(entity).is_some() {
            mobile += 1;
            assert_eq!(
                world.get::<PathFollow>(entity),
                Some(&PathFollow::default()),
                "{id}"
            );
        }
    }
    assert!(mobile > 0, "the testbed has units that move");

    // MoveOrder v1 migrated: each saved waypoint is a move order (the fixture's units stand
    // still at tick 40, so their v1 orders are empty; `pathing.rs` migrates waypoints).
    let save: SaveFile = postcard::from_bytes(&m0_save()).expect("a SaveFile");
    let mut migrated = 0;
    for (id, components) in &save.sim.entities {
        let Some((_, version, bytes)) = components
            .iter()
            .find(|(name, _, _)| name == "core.move_order")
        else {
            continue;
        };
        assert_eq!(*version, 1, "M0 saved MoveOrder v1");
        let waypoints: Vec<FxVec2> = postcard::from_bytes(bytes).expect("v1 bytes");
        let entity = world.resource::<SimIndex>().get(*id).expect("loaded");
        let orders = &world.get::<MoveOrder>(entity).expect("migrated").orders;
        let expected: Vec<Order> = waypoints
            .iter()
            .map(|&target| Order::Move { target })
            .collect();
        assert_eq!(orders, &expected, "{id}");
        migrated += 1;
    }
    assert_eq!(
        migrated, mobile,
        "every M0 unit that moves had a v1 MoveOrder"
    );

    let mut a = load_m0();
    let mut b = load_m0();
    a.run_ticks(200);
    b.run_ticks(200);
    assert_eq!(a.tick(), Tick(240));
    assert_eq!(a.hashes().len(), 200);
    assert_eq!(a.hashes(), b.hashes(), "two loads run the same");

    // Moving units go through the path queue: both players order their units on.
    let mut c = load_m0();
    let units: Vec<u32> = (1..=10).collect();
    let target = FxVec2::new(Fx::from_int(500), Fx::from_int(500));
    for player in [0, 1] {
        c.issue(player, &units, MoveCmd { target }, false);
    }
    let before: Vec<_> = units.iter().map(|&id| c.pose(id)).collect();
    c.run_ticks(50);
    assert!(
        c.journal().iter().any(|e| e.kind == "core.path_request"),
        "a path request within 50 ticks"
    );
    // ... and the requests are planned and driven: some unit found a path and moved.
    assert!(
        c.journal()
            .iter()
            .any(|e| e.kind == "core.path_result" && e.data["status"] == "found"),
        "a path was found"
    );
    let after: Vec<_> = units.iter().map(|&id| c.pose(id)).collect();
    let moved = before
        .iter()
        .zip(&after)
        .filter(|(b, a)| matches!((b, a), (Some(b), Some(a)) if b.pos != a.pos))
        .count();
    assert!(moved > 0, "the loaded units drive along their paths");
}

#[test]
fn m0_save_round_trips_after_load() {
    let mut first = load_m0();
    let bytes = first.save();
    let save: fl_sim::SaveFile = postcard::from_bytes(&bytes).expect("a SaveFile");
    let features = save.header.features;
    assert!(
        features.contains(&("core".to_owned(), 2)),
        "the new save is core v2: {features:?}"
    );
    let second = first.reload(&bytes);
    assert_eq!(
        compute_state_hash(second.world()),
        compute_state_hash(first.world())
    );
    assert_eq!(
        save_world(second.world(), &[]).expect("saves"),
        bytes,
        "the fills ran once: the resave is the same bytes"
    );
}

#[test]
fn open_map_moves_at_road_speed() {
    let core_mod = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../fl_sim/tests/fixtures/core_mod")
        .canonicalize()
        .expect("the core_mod fixture");
    let features = fl_catalog::features();
    let content = fl_sim::load_content(&features, &[core_mod]).expect("core_mod loads");
    let target = FxVec2::new(Fx::from_int(800), Fx::from_int(100));
    let mut s = ScenarioBuilder::with_content(content, features)
        .scenario("core_test.duel")
        .command(0, 0, &[1], MoveCmd { target })
        .build();
    let speed = {
        let world = s.world();
        let tank = world.resource::<SimIndex>().get(SimId(1)).expect("alive");
        world.get::<Mobility>(tank).expect("it moves").speed
    };
    let start = s.pose(1).expect("the tank").pos.x;
    for n in 1..=20 {
        s.run_ticks(1);
        assert_eq!(s.pose(1).expect("the tank").pos.x, start + speed.mul_int(n));
    }
}

fn query(s: &mut Scenario, name: &str, args: Value) -> SimReply {
    s.runner.query(SimQuery::Debug {
        name: name.to_owned(),
        args,
    })
}

fn answer(s: &mut Scenario, name: &str, args: Value) -> Value {
    match query(s, name, args.clone()) {
        SimReply::Debug(value) => value,
        other => panic!("{name} {args}: {other:?}"),
    }
}

#[test]
fn terrain_debug_queries() {
    let mut s = ScenarioBuilder::new()
        .ascii_map(&["~~.."], 4)
        .player("t.red", 0)
        .unit("t.tank", 0, 10, 2)
        .build();
    let tank = answer(
        &mut s,
        "core.passability",
        json!({ "class": "Tracked", "size": 1 }),
    );
    assert_eq!(tank["blocked"], json!([[0, 2]]), "{tank}");
    assert_eq!(tank["cells"], json!([4, 1]));
    assert_eq!(tank["version"], json!(0));
    let by_unit = answer(&mut s, "core.passability", json!({ "unit": 1 }));
    assert_eq!(by_unit, tank, "unit #1 is a size-1 tank");

    let water = answer(&mut s, "core.terrain_at", json!({ "cell": [1, 0] }));
    assert_eq!(water["terrain"], "t.water");
    assert_eq!(water["blocked"]["Foot"], true);
    assert_eq!(water["clearance"]["Tracked"], 0);
    assert_eq!(water["height_raw"], json!(0));
    let grass = answer(&mut s, "core.terrain_at", json!({ "cell": [3, 0] }));
    assert_eq!(grass["terrain"], "t.grass");
    assert_eq!(grass["blocked"]["Tracked"], false);
    assert_eq!(
        grass["cost"]["Foot"], 100,
        "path factor 1.0: 0.8 + 0.2 × 1.0 = 1"
    );
    assert_eq!(
        grass["cost"]["Tracked"], 112,
        "0.8 + 0.2 × 0.5 = 0.9 → ceil(111.1)"
    );

    let names = answer(&mut s, "core.debug_queries", Value::Null);
    for name in ["core.passability", "core.terrain_at"] {
        assert!(names["queries"].as_array().unwrap().contains(&json!(name)));
    }

    // Bad args are errors, never panics.
    for (name, args) in [
        ("core.passability", json!({})),
        ("core.passability", json!({ "class": "Hover" })),
        ("core.passability", json!({ "class": "Foot", "size": 0 })),
        ("core.passability", json!({ "class": "Foot", "size": 99 })),
        ("core.passability", json!({ "unit": 99 })),
        ("core.passability", json!({ "unit": "one" })),
        ("core.terrain_at", json!({ "cell": [4, 0] })),
        ("core.terrain_at", json!({ "cell": [-1, 0] })),
        ("core.terrain_at", json!({ "cell": "a1" })),
        ("core.terrain_at", Value::Null),
    ] {
        assert!(
            matches!(query(&mut s, name, args.clone()), SimReply::Error(_)),
            "{name} {args}"
        );
    }
    s.run_ticks(1);
}

/// A 12 × 8 map with a river (y = 3..=4) and two bridges; tanks, trucks and infantry ordered
/// across.
fn river_crossing() -> ScenarioBuilder {
    let rows = [
        "....rrrr....",
        "....r..f....",
        "..F.r.....9.",
        "~~~~=~~~~=~~",
        "~~~~=~~~~=~~",
        ".fff........",
        "....rrrrrrr.",
        "......s.....",
    ];
    ScenarioBuilder::new()
        .ascii_map(&rows, 4)
        .player("t.red", 0)
        .player("t.blue", 1)
        .unit("t.tank", 0, 6, 2)
        .unit("t.truck", 0, 18, 26)
        .unit("t.rifleman", 0, 30, 6)
        .unit("t.tank", 1, 40, 30)
        .unit("t.rifleman", 1, 10, 30)
        .command(
            0,
            0,
            &[1, 2, 3],
            MoveCmd {
                target: FxVec2::new(Fx::from_int(18), Fx::from_int(30)),
            },
        )
        .command(
            3,
            1,
            &[4],
            MoveCmd {
                target: FxVec2::new(Fx::from_int(38), Fx::from_int(2)),
            },
        )
        .command(
            10,
            1,
            &[5],
            MoveCmd {
                target: FxVec2::new(Fx::from_int(18), Fx::from_int(2)),
            },
        )
        .command(
            40,
            0,
            &[1, 2],
            MoveCmd {
                target: FxVec2::new(Fx::from_int(46), Fx::from_int(30)),
            },
        )
}

#[test]
fn smoke_terrain_twin_and_shuffle() {
    assert_deterministic(river_crossing, 200);
    assert_shuffle_invariant(river_crossing, 200);
}
