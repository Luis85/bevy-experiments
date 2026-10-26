//! Terrain (spec §7, ADR-0014): the Hashed terrain grid, the Derived navigation grid (per-class
//! blocking, clearance, costs), the speed formula, blocked cells in movement and spawns, and
//! `debug.set_terrain`. Every scenario hashes every tick.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use fl_content::{ContentDb, MapDef, MoveClass};
use fl_math::{Fx, FxVec2};
use fl_protocol::{
    Controller, DebugSetTerrainCmd, DebugSpawnCmd, DebugTeleportCmd, MoveCmd, PlayerId, SimId,
};
use fl_sim::nav::{COST_ROAD, MoveProfile, NavGrid, PathRules, content_profiles};
use fl_sim::{
    CellRect, Mobility, MovementRules, PathFollow, PathState, SetupError, SimCounters, SimIndex,
    SpatialGrid, SpawnError, TerrainGrid, last_seqs, setup_scenario_def,
};
use fl_test_utils::{
    Scenario, ScenarioBuilder, ascii_terrain_content, ascii_terrain_content_with, assert_branch,
    base_content, write_ascii_mod,
};
use fl_view::SimEvent;
use proptest::prelude::*;

/// The terrain and a freshly built navigation grid of the ASCII map `rows` (4 m cells).
fn nav_of(rows: &[&str]) -> (Arc<ContentDb>, TerrainGrid, NavGrid) {
    let content = ascii_terrain_content(rows, 4);
    let terrain = terrain_of(&content, "t.ascii");
    let nav = build_nav(&content, &terrain);
    (content, terrain, nav)
}

/// The terrain of map `id`'s layers.
fn terrain_of(content: &ContentDb, id: &str) -> TerrainGrid {
    let map = content.kind::<MapDef>().get(id).expect("the map exists");
    let terrain = map.terrain.as_ref().expect("the map has layers");
    TerrainGrid::from_layers(map.layers.as_ref().expect("decoded"), terrain.height_m)
}

/// A from-scratch navigation grid of `terrain` with 4 m cells.
fn build_nav(content: &ContentDb, terrain: &TerrainGrid) -> NavGrid {
    let max = content.rules::<PathRules>().max_clearance;
    let cell = Fx::from_int(4);
    NavGrid::build(
        terrain,
        content,
        content.rules::<MovementRules>(),
        &content_profiles(content, cell, max),
        max,
        cell,
    )
}

fn profile(class: MoveClass, size: u8) -> MoveProfile {
    MoveProfile { class, size }
}

/// Every profile of size 1..=3.
fn all_profiles() -> Vec<MoveProfile> {
    MoveClass::ALL
        .into_iter()
        .flat_map(|class| (1..=3).map(move |size| profile(class, size)))
        .collect()
}

fn at(x: i32, y: i32) -> FxVec2 {
    FxVec2::new(Fx::from_int(x), Fx::from_int(y))
}

fn move_to(x: i32, y: i32) -> MoveCmd {
    MoveCmd { target: at(x, y) }
}

/// A value as RON (and JSON) parse it: rounded to the nearest raw `Fx`.
fn parsed(text: &str) -> Fx {
    serde_json::from_str(text).expect("a number")
}

/// The codes of the commands the last tick rejected.
fn rejections(s: &Scenario) -> Vec<String> {
    s.last_output()
        .expect("a tick ran")
        .events
        .iter()
        .filter_map(|event| match event {
            SimEvent::CommandRejected { reason, .. } => Some(reason.clone()),
            _ => None,
        })
        .collect()
}

/// The ASCII map `rows` (4 m cells) with players `t.red` and `t.blue`, cheats on.
fn ascii(rows: &[&str]) -> ScenarioBuilder {
    ScenarioBuilder::new()
        .ascii_map(rows, 4)
        .player("t.red", 0)
        .player("t.blue", 1)
}

#[test]
fn speed_formula_matches_spec() {
    // Row 0 (north) is y = 0 here: one row. Cells: road, grass, forest, 'F' (pass 0.5), water.
    let (content, _, nav) = nav_of(&["r.fF~"]);
    let min = content.rules::<MovementRules>().min_speed_frac;
    assert_eq!(
        min.raw(),
        214_748_365,
        "RON rounds 0.05 to the nearest raw unit"
    );
    assert_eq!(nav.speed_factor(parsed("0.5"), 0, 0), Fx::ONE);
    assert_eq!(nav.speed_factor(parsed("0.5"), 1, 0), Fx::from_ratio(9, 10));
    assert_eq!(
        nav.speed_factor(parsed("0.2"), 2, 0),
        Fx::from_ratio(19, 25)
    );
    assert_eq!(nav.speed_factor(parsed("1.0"), 3, 0), Fx::ONE);
    assert_eq!(
        nav.speed_factor(Fx::ZERO, 4, 0),
        min,
        "clamped to min_speed_frac"
    );
    assert_eq!(
        nav.speed_factor(parsed("-0.18"), 1, 0),
        Fx::from_ratio(191, 250)
    );
}

#[test]
fn nav_cost_is_integer_ceiling() {
    let (_, _, nav) = nav_of(&["rF"]);
    for class in MoveClass::ALL {
        assert_eq!(nav.cost(class, 0, 0), COST_ROAD, "{class:?} on a road");
    }
    // Pass 0.5: 0.5 + 0.5 × 0.5 = 0.75 → ceil(133.3); 0.5 + 0.5 × 0.2 = 0.6 → ceil(166.7).
    assert_eq!(nav.cost(MoveClass::Tracked, 1, 0), 134);
    assert_eq!(nav.cost(MoveClass::Wheeled, 1, 0), 167);
}

/// The ASCII mod with base rules and its files edited by `edit`, loaded: the load error text.
fn load_error(name: &str, edit: impl FnOnce(&Path)) -> String {
    let dir = std::env::temp_dir().join(format!("fl_sim_terrain_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a temporary mod directory");
    write_ascii_mod(&dir, &["...."], 4, None);
    edit(&dir);
    // Rules and unit validation do not depend on the feature set.
    let result = fl_sim::load_content(&[], std::slice::from_ref(&dir));
    let _ = std::fs::remove_dir_all(&dir);
    match result {
        Ok(_) => panic!("{name}: the mod loads"),
        Err(err) => err.to_string(),
    }
}

/// `rules.ron` of the base content.
fn base_rules() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/base/rules.ron");
    std::fs::read_to_string(path).expect("the base rules")
}

/// `rules.ron` of the base content with `from` replaced by `to`.
fn rules_with(from: &str, to: &str) -> String {
    let rules = base_rules();
    assert!(rules.contains(from), "the base rules hold {from:?}");
    rules.replacen(from, to, 1)
}

fn assert_rules_error(name: &str, from: &str, to: &str, wants: &[&str]) {
    let rules = rules_with(from, to);
    let err = load_error(name, |dir| {
        std::fs::write(dir.join("rules.ron"), rules).expect("rules written");
    });
    for want in wants {
        assert!(err.contains(want), "{name}: {want:?} not in {err}");
    }
}

#[test]
fn movement_rules_ranges_are_validated() {
    let frac = "min_speed_frac: 0.05";
    assert_rules_error(
        "frac_zero",
        frac,
        "min_speed_frac: 0.0",
        &["Movement/min_speed_frac", "(0, 1]"],
    );
    assert_rules_error(
        "frac_big",
        frac,
        "min_speed_frac: 1.5",
        &["Movement/min_speed_frac", "(0, 1]"],
    );
    // 100 / 65535 is 0.0015259...: just below it, costs would overflow a u16.
    assert_rules_error(
        "frac_tiny",
        frac,
        "min_speed_frac: 0.0015",
        &["Movement/min_speed_frac", "u16"],
    );
    assert_rules_error(
        "slope_missing",
        "Foot: 100.0, ",
        "",
        &["Movement/max_slope_m", "Foot"],
    );
    assert_rules_error(
        "factor_missing",
        ", Tracked: 0.5}",
        "}",
        &["Movement/path_terrain_factor", "Tracked"],
    );
    assert_rules_error(
        "factor_range",
        "Wheeled: 0.2",
        "Wheeled: 1.5",
        &["Movement/path_terrain_factor.Wheeled", "[-1, 1]"],
    );
    assert_rules_error(
        "slope_negative",
        "Wheeled: 1.5",
        "Wheeled: -1.0",
        &["Movement/max_slope_m.Wheeled", "at least 0"],
    );
    // Path following reads both (Task 8): degrees outside (0, 180] wrap as an angle, and a
    // non-positive arrival radius never arrives short of a corner.
    let turn = "turn_in_place_deg: 30.0";
    for (name, to) in [
        ("turn_zero", "0.0"),
        ("turn_wide", "180.5"),
        ("turn_neg", "-10.0"),
    ] {
        assert_rules_error(
            name,
            turn,
            &format!("turn_in_place_deg: {to}"),
            &["Movement/turn_in_place_deg", "(0, 180]"],
        );
    }
    let _ = ascii_terrain_content_with(
        &["."],
        4,
        Some(&rules_with(turn, "turn_in_place_deg: 180.0")),
    );
    for (name, to) in [("arrive_zero", "0.0"), ("arrive_neg", "-0.5")] {
        assert_rules_error(
            name,
            "arrive_m: 0.5",
            &format!("arrive_m: {to}"),
            &["Movement/arrive_m", "above 0"],
        );
    }

    // A unit's terrain factor may lie in [-1, 1]; -1.5 is out.
    let err = load_error("unit_factor", |dir| {
        let path = dir.join("units/truck.unit.ron");
        let unit = std::fs::read_to_string(&path).expect("the truck");
        let unit = unit.replace("terrain_factor: 0.2", "terrain_factor: -1.5");
        std::fs::write(path, unit).expect("truck written");
    });
    assert!(
        err.contains("terrain_factor") && err.contains("[-1, 1]"),
        "{err}"
    );
    // The base rules load as given.
    let _ = ascii_terrain_content_with(&["."], 4, Some(&base_rules()));
}

#[test]
fn path_rules_ranges_are_validated() {
    assert_rules_error(
        "cluster",
        "cluster_cells: 16",
        "cluster_cells: 0",
        &["Path/cluster_cells", "at least 1"],
    );
    assert_rules_error(
        "clearance",
        "max_clearance: 3",
        "max_clearance: 0",
        &["Path/max_clearance", "at least 1"],
    );
    // R124: the hard ceiling of the flow cache is no lower than its cap.
    assert_rules_error(
        "hard_max",
        "flow_cache_hard_max: 128",
        "flow_cache_hard_max: 31",
        &["Path/flow_cache_hard_max", "at least flow_cache_max (32)"],
    );
    // Task 11 review Minor 8: an idle unit's push cap is `max_push_frac × speed ×
    // idle_every`, so the stagger is bounded.
    for (name, bad) in [("idle", "idle_every: 0"), ("idle_big", "idle_every: 65")] {
        assert_rules_error(
            name,
            "idle_every: 5",
            bad,
            &["Steering/idle_every", "1..=64"],
        );
    }
    let widest = base_rules().replace("idle_every: 5", "idle_every: 64");
    let content = ascii_terrain_content_with(&["...."], 4, Some(&widest));
    assert_eq!(content.rules::<fl_sim::SteeringRules>().idle_every, 64);
}

/// Final review M2: a negative gain attracts, a non-positive push cap reverses the push, and an
/// extreme margin overflows `r × r` (R104 class through content).
#[test]
fn steering_rules_ranges_are_validated() {
    let steering =
        "separation_gain: 0.5, separation_margin_m: 0.5, idle_every: 5, max_push_frac: 0.5";
    let with = |field: &str, value: &str| {
        let from = format!("{field}: 0.5");
        assert!(steering.contains(&from), "{field} is 0.5 in the base rules");
        steering.replacen(&from, &format!("{field}: {value}"), 1)
    };
    for (field, bad, range, good) in [
        (
            "separation_gain",
            &["-0.1", "4.5"][..],
            "[0, 4]",
            &["0.0", "4.0"][..],
        ),
        (
            "separation_margin_m",
            &["-0.5", "50.5"][..],
            "[0, 50]",
            &["0.0", "50.0"][..],
        ),
        (
            "max_push_frac",
            &["0.0", "-0.5", "1.5"][..],
            "(0, 1]",
            &["1.0"][..],
        ),
    ] {
        for value in bad {
            assert_rules_error(
                &format!("{field}_{}", value.replace(['-', '.'], "_")),
                steering,
                &with(field, value),
                &[&format!("Steering/{field}"), range],
            );
        }
        for value in good {
            let rules = rules_with(steering, &with(field, value));
            let _ = ascii_terrain_content_with(&["...."], 4, Some(&rules));
        }
    }
}

/// Final review M2: follow and formation distances are capped at the largest map side (R89),
/// so products of them stay inside `Fx`.
#[test]
fn group_distances_are_capped_at_the_map_side() {
    for (from, field) in [
        ("formation_max_spread_m: 40.0", "formation_max_spread_m"),
        ("follow_distance_m: 8.0", "follow_distance_m"),
    ] {
        assert_rules_error(
            field,
            from,
            &format!("{field}: 4096.5"),
            &[&format!("Movement/{field}"), "4096"],
        );
        let rules = rules_with(from, &format!("{field}: 4096.0"));
        let _ = ascii_terrain_content_with(&["...."], 4, Some(&rules));
    }
}

#[test]
fn profile_anchor_is_centred() {
    // 10 × 10 grass with water at cell (6, 6) (row 9 − 6 = 3).
    let mut rows = vec![".........."; 10];
    rows[3] = "......~...";
    let (_, _, nav) = nav_of(&rows);
    let tracked = |size| profile(MoveClass::Tracked, size);
    // Size 3 at centre (5, 5): the anchor (4, 4) square covers (4..=6, 4..=6), water included.
    assert!(nav.clearance(MoveClass::Tracked, 4, 4) < 3);
    assert!(!nav.passable(tracked(3), 5, 5));
    // At centre (4, 4) the anchor is (3, 3), clear of the water.
    assert!(nav.clearance(MoveClass::Tracked, 3, 3) >= 3);
    assert!(nav.passable(tracked(3), 4, 4));
    // Sizes 1 and 2 anchor at the centre cell.
    assert!(
        !nav.passable(tracked(2), 5, 5),
        "(5..=6, 5..=6) holds the water"
    );
    assert!(nav.passable(tracked(2), 4, 4));
    assert!(nav.passable(tracked(1), 5, 5));
    assert!(!nav.passable(tracked(1), 6, 6));
    // Off the map, and squares running off it.
    assert!(!nav.passable(tracked(1), -1, 0));
    assert!(!nav.passable(tracked(1), 10, 0));
    assert!(!nav.passable(tracked(2), 9, 9));
}

/// How far `unit` (`t.truck`, from (6, 6) heading east) drives in 40 ticks along `row`.
fn truck_distance(row: &str) -> Fx {
    let rows = [row, row, row];
    let mut s = ascii(&rows)
        .unit("t.truck", 0, 6, 6)
        .command(0, 0, &[1], move_to(110, 6))
        .build();
    s.run_ticks(40);
    s.pose(1).expect("the truck").pos.x - Fx::from_int(6)
}

#[test]
fn truck_slows_down_off_road() {
    let road = truck_distance(&"r".repeat(30));
    let grass = truck_distance(&".".repeat(30));
    // 72 km/h is 1 m per tick on the road.
    assert_eq!(road, Fx::from_int(40));
    let ratio = grass / road;
    // 0.8 + 0.2 × 0.2 = 0.84 (spec §12.4).
    assert!(
        ratio >= Fx::from_ratio(83, 100) && ratio <= Fx::from_ratio(85, 100),
        "ratio {ratio}"
    );
}

#[test]
fn water_blocks_every_class() {
    let (_, _, nav) = nav_of(&[".~."]);
    for class in MoveClass::ALL {
        assert!(nav.blocked(class, 1, 0), "{class:?}");
        assert!(!nav.blocked(class, 0, 0), "{class:?}");
    }
    for p in all_profiles() {
        assert!(!nav.passable(p, 1, 0), "{p:?}");
    }
}

#[test]
fn deep_forest_blocks_vehicles_not_foot() {
    let (_, _, nav) = nav_of(&[".F."]);
    assert!(!nav.blocked(MoveClass::Foot, 1, 0));
    for class in [MoveClass::Wheeled, MoveClass::HalfTrack, MoveClass::Tracked] {
        assert!(nav.blocked(class, 1, 0), "{class:?}");
    }
    assert!(nav.passable(profile(MoveClass::Foot, 1), 1, 0));
}

#[test]
fn bridge_cells_pass_over_water() {
    let (_, _, nav) = nav_of(&["~=~", "~=~", "~=~"]);
    for class in MoveClass::ALL {
        for y in 0..3 {
            assert!(!nav.blocked(class, 1, y), "{class:?} on the bridge");
            assert!(nav.blocked(class, 0, y), "{class:?} in the water");
        }
        assert!(nav.passable(profile(class, 1), 1, 1));
    }
}

#[test]
fn steep_slope_blocks_wheeled_not_foot() {
    let (content, _, nav) = nav_of(&["..9.."]);
    let cell = Fx::from_int(4);
    let max = content.rules::<PathRules>().max_clearance;
    let truck = content_profile(&content, "t.truck", cell, max);
    let rifleman = content_profile(&content, "t.rifleman", cell, max);
    // The 9 m step blocks both sides of it for the truck (max 1.5 m), not for infantry.
    for x in 1..=3 {
        assert!(!nav.passable(truck, x, 0), "truck at x {x}");
        assert!(nav.passable(rifleman, x, 0), "rifleman at x {x}");
    }
    assert!(nav.passable(truck, 0, 0));
    assert!(nav.passable(truck, 4, 0));
}

/// The profile a unit of `id` spawns with.
fn content_profile(content: &ContentDb, id: &str, cell: Fx, max: u8) -> MoveProfile {
    let unit = content
        .unit(content.unit_id(id).expect("a unit"))
        .expect("a def");
    let mobility = unit
        .get::<fl_sim::MobilityDef>(content.registry())
        .expect("it moves");
    let width = unit
        .get::<fl_sim::HullDef>(content.registry())
        .expect("a hull")
        .footprint
        .x;
    let size = u8::try_from(((width.raw() + cell.raw() - 1) / cell.raw()).clamp(1, i64::from(max)))
        .expect("a size");
    MoveProfile {
        class: mobility.class,
        size,
    }
}

#[test]
fn clearance_blocks_wide_units_in_narrow_gaps() {
    // A water wall at x = 3 with a one-cell gap at y = 2.
    let rows = ["...~...", "...~...", ".......", "...~...", "...~..."];
    let (content, _, nav) = nav_of(&rows);
    let cell = Fx::from_int(4);
    let max = content.rules::<PathRules>().max_clearance;
    let tank = content_profile(&content, "t.tank", cell, max);
    let wide = content_profile(&content, "t.wide_tank", cell, max);
    assert_eq!(
        (tank.size, wide.size),
        (1, 2),
        "3.0 m and 7.0 m on 4 m cells"
    );
    assert!(nav.passable(tank, 3, 2), "the tank fits the gap");
    for y in 0..5 {
        assert!(
            !nav.passable(wide, 3, y),
            "the wide tank cannot stand at (3, {y})"
        );
    }
    assert!(nav.passable(wide, 5, 2));
}

#[test]
fn units_stop_at_blocked_cells() {
    let rows = ["....~~~~", "....~~~~", "....~~~~"];
    // A move into the water: the goal snaps to the nearest shore cell (x = 3, centre 14 m),
    // and the tank drives there without entering the water.
    let mut s = ascii(&rows)
        .unit("t.tank", 0, 2, 6)
        .command(0, 0, &[1], move_to(28, 6))
        .build();
    for _ in 0..80 {
        s.run_ticks(1);
        let pos = s.pose(1).expect("the tank").pos;
        assert!(
            pos.x < Fx::from_int(16),
            "the tank entered the water at {pos:?}"
        );
    }
    let pos = s.pose(1).expect("the tank").pos;
    assert!(
        pos.x > Fx::from_int(13),
        "the tank drove up to the shore: {pos:?}"
    );

    // A path straight into the water (no plan makes one; forced here): the step into the
    // first water cell is skipped and counted, and the path goes back to Idle.
    let mut s = ascii(&rows).unit("t.tank", 0, 2, 6).build();
    {
        let world = s.runner.world_mut();
        let tank = world.resource::<SimIndex>().get(SimId(1)).expect("alive");
        let mut follow = world.get_mut::<PathFollow>(tank).expect("a mobile unit");
        follow.state = PathState::Following;
        follow.goal = at(28, 6);
        follow.corners = vec![at(28, 6)];
    }
    let mut blocked = 0;
    for _ in 0..80 {
        s.run_ticks(1);
        let pos = s.pose(1).expect("the tank").pos;
        assert!(
            pos.x < Fx::from_int(16),
            "the tank entered the water at {pos:?}"
        );
        blocked += s.world().resource::<SimCounters>().get("move.blocked");
    }
    let pos = s.pose(1).expect("the tank").pos;
    assert!(
        pos.x > Fx::from_int(15),
        "the tank drove up to the shore: {pos:?}"
    );
    assert_eq!(blocked, 1, "move.blocked was counted once");
    let world = s.world();
    let tank = world.resource::<SimIndex>().get(SimId(1)).expect("alive");
    let follow = world.get::<PathFollow>(tank).expect("a mobile unit");
    assert_eq!(follow.state, PathState::Idle);
}

#[test]
fn spawn_on_blocked_cell_is_rejected() {
    let mut s = ascii(&["..~~", "..~~"]).build();
    let units_before = s.world().resource::<SimIndex>().len();
    s.issue(
        0,
        &[],
        DebugSpawnCmd {
            unit: "t.tank".to_owned(),
            owner: PlayerId(0),
            pos: at(12, 2),
            heading: fl_math::Angle::ZERO,
        },
        false,
    );
    s.run_ticks(1);
    assert_eq!(rejections(&s), ["core.invalid_target"]);
    assert_eq!(s.world().resource::<SimIndex>().len(), units_before);

    // A scenario unit on water fails setup, and nothing changes.
    let def = fl_content::ScenarioDef {
        id: "t.wet".to_owned(),
        map: "t.ascii".to_owned(),
        seed: 1,
        players: vec![fl_content::ScenarioPlayer {
            faction: "t.red".to_owned(),
            team: 0,
            controller: Controller::Human,
        }],
        units: vec![
            fl_content::ScenarioUnit {
                unit: "t.tank".to_owned(),
                player: 0,
                pos: at(2, 2),
                heading_deg: Fx::ZERO,
            },
            fl_content::ScenarioUnit {
                unit: "t.tank".to_owned(),
                player: 0,
                pos: at(10, 2),
                heading_deg: Fx::ZERO,
            },
        ],
    };
    let units = s.world().resource::<SimIndex>().len();
    match setup_scenario_def(s.runner.world_mut(), &def) {
        Err(SetupError::Unit {
            index: 1,
            error: SpawnError::BlockedCell,
            ..
        }) => {}
        other => panic!("expected the blocked unit 1, got {other:?}"),
    }
    assert_eq!(
        s.world().resource::<SimIndex>().len(),
        units,
        "all or nothing"
    );
}

#[test]
fn teleport_into_water_is_rejected() {
    let mut s = ascii(&["..~~", "..~~"]).unit("t.tank", 0, 2, 2).build();
    s.run_ticks(1);
    let before = s.pose(1).expect("the tank");
    let terrain = s.world().resource::<TerrainGrid>().clone();
    s.issue(0, &[1], DebugTeleportCmd { pos: at(12, 2) }, false);
    s.run_ticks(1);
    assert_eq!(rejections(&s), ["core.invalid_target"]);
    assert_eq!(s.pose(1), Some(before), "the tank stays");
    assert_eq!(*s.world().resource::<TerrainGrid>(), terrain);
    assert_eq!(
        last_seqs(s.world()),
        [(PlayerId(0), 0)].into(),
        "LastSeq advanced"
    );

    // Dry land is fine.
    s.issue(0, &[1], DebugTeleportCmd { pos: at(6, 6) }, false);
    s.run_ticks(1);
    assert!(rejections(&s).is_empty());
    assert_eq!(s.pose(1).expect("the tank").pos, at(6, 6));
}

/// A river at y = 2 with a bridge at x = 2 (5 × 5 cells).
const RIVER: [&str; 5] = [".....", ".....", "~~=~~", ".....", "....."];

fn set_terrain(min: (u32, u32), max: (u32, u32), terrain: &str) -> DebugSetTerrainCmd {
    DebugSetTerrainCmd {
        min_cell: min,
        max_cell: max,
        terrain: terrain.to_owned(),
    }
}

#[test]
fn set_terrain_blocks_a_bridge() {
    let mut s = ascii(&RIVER).build();
    for class in MoveClass::ALL {
        assert!(
            s.world()
                .resource::<NavGrid>()
                .passable(profile(class, 1), 2, 2)
        );
    }
    s.issue(0, &[], set_terrain((2, 2), (2, 2), "t.water"), false);
    s.run_ticks(1);
    assert!(rejections(&s).is_empty());
    let nav = s.world().resource::<NavGrid>();
    for p in all_profiles() {
        assert!(!nav.passable(p, 2, 2), "{p:?} on the drowned bridge");
    }
    assert_eq!(nav.version(), 1);
    assert_eq!(
        s.world().resource::<TerrainGrid>().terrain_at(2, 2),
        Some("t.water")
    );
    let changed: Vec<_> = s
        .journal()
        .iter()
        .filter(|entry| entry.kind == "core.terrain_changed")
        .collect();
    assert_eq!(changed.len(), 1, "{changed:?}");
    assert_eq!(changed[0].data["terrain"], "t.water");
    assert!(s.world().resource::<SimCounters>().get("nav.cells_updated") > 0);
}

#[test]
fn set_terrain_rejects_bad_input() {
    let mut s = ascii(&RIVER).unit("t.tank", 0, 2, 2).build();
    s.run_ticks(1);
    let terrain = s.world().resource::<TerrainGrid>().clone();
    let nav = s.world().resource::<NavGrid>().clone();
    let cases = [
        (
            set_terrain((0, 0), (1, 1), "t.lava"),
            "core.unknown_content",
        ),
        (
            set_terrain((3, 0), (1, 1), "t.water"),
            "core.invalid_target",
        ),
        (
            set_terrain((0, 3), (1, 1), "t.water"),
            "core.invalid_target",
        ),
        (
            set_terrain((0, 0), (5, 1), "t.water"),
            "core.invalid_target",
        ),
        (
            set_terrain((0, 0), (1, 5), "t.water"),
            "core.invalid_target",
        ),
    ];
    for (seq, (cmd, code)) in (0u32..).zip(cases) {
        s.issue(0, &[], cmd, false);
        s.run_ticks(1);
        assert_eq!(rejections(&s), [code]);
        assert_eq!(*s.world().resource::<TerrainGrid>(), terrain);
        assert_eq!(*s.world().resource::<NavGrid>(), nav);
        assert_eq!(s.world().resource::<NavGrid>().version(), 0);
        assert_eq!(last_seqs(s.world()), [(PlayerId(0), seq)].into());
    }
}

#[test]
fn set_terrain_needs_cheats() {
    let mut s = ascii(&RIVER).cheats(false).build();
    s.issue(0, &[], set_terrain((2, 2), (2, 2), "t.water"), false);
    s.run_ticks(1);
    assert_eq!(rejections(&s), ["core.debug_disabled"]);
    assert_eq!(
        s.world().resource::<TerrainGrid>().terrain_at(2, 2),
        Some("t.bridge")
    );
}

/// Every terrain id of the ASCII mod, sorted (the palette its maps use).
const IDS: [&str; 7] = [
    "t.bridge",
    "t.deep_forest",
    "t.forest",
    "t.grass",
    "t.road",
    "t.swamp",
    "t.water",
];

fn rect_strategy() -> impl Strategy<Value = (CellRect, usize)> {
    (0u32..32, 0u32..32, 0u32..32, 0u32..32, 0usize..IDS.len()).prop_map(|(x0, x1, y0, y1, id)| {
        let rect = CellRect {
            min: (x0.min(x1), y0.min(y1)),
            max: (x0.max(x1), y0.max(y1)),
        };
        (rect, id)
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn incremental_nav_equals_full_rebuild(
        types in proptest::collection::vec(0u8..7, 32 * 32),
        heights in proptest::collection::vec(0u16..10, 32 * 32),
        edits in proptest::collection::vec(rect_strategy(), 1..8),
    ) {
        let content = ascii_terrain_content(&["."], 4);
        let mut terrain = TerrainGrid {
            cells_x: 32,
            cells_y: 32,
            palette: IDS.iter().map(|&id| id.to_owned()).collect(),
            types,
            heights,
            height_m: (Fx::ZERO, Fx::from_int(65535)),
        };
        let rules = content.rules::<MovementRules>();
        let mut nav = build_nav(&content, &terrain);
        for (rect, id) in edits {
            terrain.set_rect(rect, IDS[id]);
            let updated = nav.update_rect(&terrain, &content, rules, rect);
            prop_assert!(updated.min.0 <= rect.min.0 && updated.max.0 >= rect.max.0);
            prop_assert!(updated.min.1 <= rect.min.1 && updated.max.1 >= rect.max.1);
            prop_assert_eq!(&nav, &build_nav(&content, &terrain));
        }
    }
}

/// The river map with a tank and a rifleman, the bridge drowned at tick 5.
fn drowned_bridge() -> ScenarioBuilder {
    ascii(&RIVER)
        .unit("t.tank", 0, 2, 2)
        .unit("t.rifleman", 0, 18, 18)
        .command(1, 0, &[1], move_to(18, 2))
        .command(5, 0, &[], set_terrain((2, 2), (2, 2), "t.water"))
        .command(8, 0, &[2], move_to(2, 18))
}

#[test]
fn save_right_after_set_terrain() {
    let mut s = drowned_bridge().build();
    s.run_ticks(6);
    let bytes = s.save();
    let loaded = s.reload(&bytes);
    assert_eq!(
        loaded.world().resource::<TerrainGrid>(),
        s.world().resource::<TerrainGrid>()
    );
    assert_eq!(s.world().resource::<NavGrid>().version(), 1);
    assert_eq!(loaded.world().resource::<NavGrid>().version(), 0, "rebuilt");
    assert_eq!(
        loaded.world().resource::<NavGrid>(),
        s.world().resource::<NavGrid>()
    );
    assert_branch(drowned_bridge, 6, 56);
}

#[test]
fn core_registers_one_rebuild() {
    let mut s = drowned_bridge().build();
    s.run_ticks(10);
    let bytes = s.save();
    let loaded = s.reload(&bytes);
    let (live, back) = (s.world(), loaded.world());
    assert_eq!(
        back.resource::<SpatialGrid>(),
        live.resource::<SpatialGrid>()
    );
    assert_eq!(back.resource::<NavGrid>(), live.resource::<NavGrid>());
    // The navigation grid was built from scratch once: no update since.
    assert_eq!(back.resource::<NavGrid>().version(), 0);
    assert_eq!(live.resource::<NavGrid>().version(), 1);
}

#[test]
fn testbed_ridge_blocks_wheeled_hill_stays_passable() {
    // Task 4's testbed: a 6 m ridge at x 140..=143, y 140..=170, and a 12 m hill of radius 25
    // cells around (60, 200) (crates/fl_cli/src/map_gen.rs).
    let content = base_content();
    let terrain = terrain_of(&content, "base.testbed_1km");
    let nav = build_nav(&content, &terrain);
    let rules = content.rules::<MovementRules>();
    let wheeled = rules.max_slope_m[&MoveClass::Wheeled];
    let step = terrain.height_at(140, 150) - terrain.height_at(139, 150);
    assert!(
        step > wheeled,
        "the ridge step {step} m exceeds {wheeled} m"
    );
    let truck = profile(MoveClass::Wheeled, 1);
    for (x, y) in [(139, 150), (140, 150), (143, 150), (144, 150)] {
        assert!(
            !nav.passable(truck, x, y),
            "the ridge edge ({x}, {y}) blocks wheels"
        );
        assert!(
            nav.passable(profile(MoveClass::Foot, 1), x, y),
            "infantry climbs ({x}, {y})"
        );
    }
    // Its steepest step is about 6 m × π / 25 ≈ 0.75 m per cell.
    for y in 175..=225 {
        for x in 35..=85 {
            assert!(
                !nav.blocked(MoveClass::Wheeled, x, y),
                "the hill blocks wheels at ({x}, {y})"
            );
        }
    }
    assert!(nav.passable(truck, 60, 200), "the hilltop");
    assert!(nav.passable(truck, 60, 185), "the hillside");
}

#[test]
fn units_without_terrain_keep_m0_movement() {
    // Mobility rides on Pose and MoveOrder only: a builder flat map is open ground.
    let mut s = ScenarioBuilder::new()
        .map_flat(400, 400)
        .player("base.de", 0)
        .unit("base.de.pz3", 0, 100, 100)
        .command(0, 0, &[1], move_to(300, 100))
        .build();
    let speed = {
        let world = s.world();
        let entity = world.resource::<SimIndex>().get(SimId(1)).expect("alive");
        world.get::<Mobility>(entity).expect("it moves").speed
    };
    s.run_ticks(10);
    let pos = s.pose(1).expect("the tank").pos;
    assert_eq!(pos.x, Fx::from_int(100) + speed.mul_int(10));
}

/// A base-content flat map of 10 × 10 open cells (4 m) with player `base.de`.
fn flat() -> Scenario {
    ScenarioBuilder::new()
        .map_flat(40, 40)
        .player("base.de", 0)
        .build()
}

#[test]
fn set_terrain_grows_the_palette() {
    let mut s = flat();
    assert!(s.world().resource::<TerrainGrid>().palette.is_empty());
    s.issue(0, &[], set_terrain((2, 2), (3, 3), "base.water"), false);
    s.run_ticks(1);
    assert!(rejections(&s).is_empty());
    let terrain = s.world().resource::<TerrainGrid>().clone();
    assert_eq!(terrain.palette, ["base.water"]);
    let nav = s.world().resource::<NavGrid>();
    assert!(nav.blocked(MoveClass::Foot, 2, 2));
    assert!(!nav.blocked(MoveClass::Foot, 4, 4));
    // Only the touched rect, grown by 1 (slope) and by max_clearance − 1 = 2 toward −x/−y, is
    // recomputed: (0, 0)..=(4, 4), 25 of 100 cells.
    assert_eq!(
        s.world().resource::<SimCounters>().get("nav.cells_updated"),
        25
    );
    let content = base_content();
    assert_eq!(
        *nav,
        build_nav(&content, &terrain),
        "partial == full rebuild"
    );

    // A second new kind appends; the first keeps its index.
    s.issue(
        0,
        &[],
        set_terrain((7, 0), (9, 1), "base.deep_forest"),
        false,
    );
    s.run_ticks(1);
    let terrain = s.world().resource::<TerrainGrid>().clone();
    assert_eq!(terrain.palette, ["base.water", "base.deep_forest"]);
    assert_eq!(terrain.type_at(2, 2), 0);
    assert_eq!(terrain.type_at(8, 1), 1);
    let nav = s.world().resource::<NavGrid>();
    assert!(nav.blocked(MoveClass::Tracked, 8, 1) && !nav.blocked(MoveClass::Foot, 8, 1));
    assert_eq!(
        *nav,
        build_nav(&content, &terrain),
        "partial == full rebuild"
    );
    assert_eq!(nav.version(), 2);
}

#[test]
fn set_terrain_without_a_map_is_rejected() {
    let mut s = ScenarioBuilder::new().player("base.de", 0).build();
    s.issue(0, &[], set_terrain((0, 0), (0, 0), "base.water"), false);
    s.run_ticks(1);
    assert_eq!(rejections(&s), ["core.invalid_target"]);
    assert!(s.world().get_resource::<TerrainGrid>().is_none());
    assert_eq!(last_seqs(s.world()), [(PlayerId(0), 0)].into());
}

#[test]
fn set_terrain_with_a_full_palette_is_rejected() {
    let mut s = flat();
    // 255 names, none of them "base.water": no index is left for a new name.
    let full: Vec<String> = (0..255).map(|i| format!("x.kind{i:03}")).collect();
    s.runner.world_mut().resource_mut::<TerrainGrid>().palette = full.clone();
    s.issue(0, &[], set_terrain((0, 0), (0, 0), "base.water"), false);
    s.run_ticks(1);
    assert_eq!(rejections(&s), ["core.invalid_target"]);
    assert_eq!(s.world().resource::<TerrainGrid>().palette, full);
    assert_eq!(
        s.world().resource::<TerrainGrid>().type_at(0, 0),
        TerrainGrid::OPEN
    );
}

/// A save of the river map whose `core.terrain` is replaced by `edit` of the saved grid.
fn save_with_terrain(edit: impl FnOnce(&mut TerrainGrid)) -> (Scenario, Vec<u8>) {
    let mut s = ascii(&RIVER).build();
    s.run_ticks(1);
    let mut save: fl_sim::SaveFile = postcard::from_bytes(&s.save()).expect("a SaveFile");
    let entry = save
        .sim
        .resources
        .iter_mut()
        .find(|(name, _, _)| name == "core.terrain")
        .expect("the save holds the terrain");
    let mut terrain: TerrainGrid = postcard::from_bytes(&entry.2).expect("a TerrainGrid");
    edit(&mut terrain);
    entry.2 = postcard::to_allocvec(&terrain).expect("encodes");
    let bytes = postcard::to_allocvec(&save).expect("encodes");
    (s, bytes)
}

/// The load error of `bytes` with `s`'s features and content.
fn load_error_of(s: &Scenario, bytes: &[u8]) -> fl_sim::LoadError {
    let content = Arc::clone(&s.world().resource::<fl_sim::Content>().0);
    match fl_sim::load_world(s.features(), content, bytes, fl_sim::RunConfig::default()) {
        Ok(_) => panic!("the save loads"),
        Err(err) => err,
    }
}

/// An edit of a saved terrain grid.
type TerrainEdit = fn(&mut TerrainGrid);

#[test]
fn malformed_terrain_in_a_save_is_a_load_error() {
    // The unedited save loads.
    let (s, bytes) = save_with_terrain(|_| {});
    let _ = s.reload(&bytes);

    let cases: [(&str, TerrainEdit, &str); 6] = [
        ("cells", |t| t.cells_x += 1, "cells"),
        (
            "types",
            |t| {
                t.types.pop();
            },
            "types",
        ),
        ("heights", |t| t.heights.push(0), "heights"),
        (
            "palette name",
            |t| t.palette[0] = "t.lava".to_owned(),
            "t.lava",
        ),
        ("type index", |t| t.types[0] = 9, "palette"),
        (
            "palette size",
            |t| {
                t.palette = (0..256).map(|i| format!("t.k{i}")).collect();
            },
            "255",
        ),
    ];
    for (what, edit, wants) in cases {
        let (s, bytes) = save_with_terrain(edit);
        match load_error_of(&s, &bytes) {
            fl_sim::LoadError::Codec { name, message } => {
                assert_eq!(name, "core.terrain", "{what}");
                assert!(
                    message.contains(wants),
                    "{what}: {wants:?} not in {message:?}"
                );
            }
            other => panic!("{what}: expected a Codec error, got {other}"),
        }
    }
}

#[test]
fn terrain_without_a_map_in_a_save_is_a_load_error() {
    let mut s = ascii(&RIVER).build();
    s.run_ticks(1);
    let mut save: fl_sim::SaveFile = postcard::from_bytes(&s.save()).expect("a SaveFile");
    save.sim.resources.retain(|(name, _, _)| name != "core.map");
    let bytes = postcard::to_allocvec(&save).expect("encodes");
    match load_error_of(&s, &bytes) {
        fl_sim::LoadError::Codec { name, message } => {
            assert_eq!(name, "core.terrain");
            assert!(message.contains("map"), "{message}");
        }
        other => panic!("expected a Codec error, got {other}"),
    }
}
