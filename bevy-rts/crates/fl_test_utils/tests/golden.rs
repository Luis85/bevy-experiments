//! Golden replays (spec §12.3, ADR-0007, ADR-0021): every `tests/replays/*.replay` plays back
//! on the current build with the hashes it was recorded with, and its semantic checks hold.
//!
//! `FL_RECORD_GOLDEN=1` records the testbed wander match and the movement testbed into
//! `tests/replays/testbed_wander.replay.new` and `tests/replays/testbed_paths.replay.new`, and
//! `FL_RECORD_POSES=1` records the movement testbed's pose trace into
//! `tests/replays/testbed_paths.poses.new`; only
//! the lead promotes them (`cargo xtask bless crates/fl_test_utils/tests/replays --reason "…"`).
//! A golden whose hashes change is re-blessed only through `fl_cli rebless --reason MCR-nnn`
//! (R146), which keeps the semantic checks the record functions attach.

#[path = "support/testbed_paths.rs"]
#[allow(
    dead_code,
    reason = "shared with movement_determinism.rs, which uses other items"
)]
mod testbed_paths;

use std::collections::BTreeSet;
use std::path::PathBuf;

use fl_math::{Fx, FxVec2};
use fl_protocol::{MoveCmd, SimId, Tick};
use fl_sim::{
    MoveOrder, Order, Replay, ReplayStart, SemanticCheck, SimIndex, TerrainGrid, VerifyOutcome,
    verify_replay,
};
use fl_test_utils::{
    Scenario, ScenarioBuilder, base_content, pose_trace, read_pose_trace, unit_cells,
    write_pose_trace,
};
use testbed_paths::{PARTY, RIVER_ROWS, TICKS, testbed_paths};

/// The golden replays `golden_replays_verify` requires.
const GOLDEN: [&str; 2] = ["testbed_paths.replay", "testbed_wander.replay"];

fn replays_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/replays")
}

fn move_to(x: i32, y: i32) -> MoveCmd {
    MoveCmd {
        target: FxVec2::new(Fx::from_int(x), Fx::from_int(y)),
    }
}

/// The base testbed with every unit wandering (radius 100 m), player 0 moving #1 and #2 at
/// tick 10 and player 1 moving #6 and #7 at tick 60, for 400 ticks, hashed every tick.
fn record_testbed_wander() -> Replay {
    let mut s = ScenarioBuilder::new()
        .scenario("base.testbed")
        .wander_all(100)
        .command(10, 0, &[1, 2], move_to(600, 500))
        .command(60, 1, &[6, 7], move_to(300, 450))
        .build();
    s.run_ticks(400);
    s.replay().with_semantics(wander_semantics())
}

/// The semantic checks of the testbed wander golden, from its setup: none of the ten units
/// dies, and none wanders onto water.
fn wander_semantics() -> Vec<SemanticCheck> {
    vec![
        SemanticCheck::EntityCount {
            id: "ten_units".to_owned(),
            tick: Tick(399),
            min: 10,
            max: 10,
        },
        SemanticCheck::NeverOnTerrain {
            id: "wander_off_water".to_owned(),
            units: (1..=10).map(SimId).collect(),
            terrain: "base.water".to_owned(),
        },
    ]
}

/// The movement testbed ([`testbed_paths`]) for [`TICKS`] ticks, hashed every tick.
fn record_testbed_paths() -> Replay {
    let mut s = testbed_paths().build();
    s.run_ticks(TICKS);
    s.replay().with_semantics(paths_semantics())
}

/// The semantic checks of the movement testbed golden, from its setup: none of the thirteen
/// units dies, and none enters water (M1 Review Focus 1).
fn paths_semantics() -> Vec<SemanticCheck> {
    vec![
        SemanticCheck::EntityCount {
            id: "thirteen_units".to_owned(),
            tick: Tick(TICKS - 1),
            min: 13,
            max: 13,
        },
        SemanticCheck::NeverOnTerrain {
            id: "no_unit_on_water".to_owned(),
            units: (1..=13).map(SimId).collect(),
            terrain: "base.water".to_owned(),
        },
    ]
}

/// Play `replay` back with the game's features and the base content; panics unless every
/// hash matches.
fn assert_verifies(replay: &Replay, what: &str) {
    match verify_replay(replay, &fl_catalog::features(), base_content()) {
        Ok(VerifyOutcome::Verified { ticks, .. }) => assert_eq!(ticks, replay.ticks, "{what}"),
        other => panic!("{what} does not verify: {other:?}"),
    }
}

#[test]
fn golden_replays_verify() {
    let dir = replays_dir();
    if std::env::var_os("FL_RECORD_GOLDEN").is_some_and(|v| v == "1") {
        std::fs::create_dir_all(&dir).expect("the replays directory");
        for (name, replay) in [
            ("testbed_wander.replay.new", record_testbed_wander()),
            ("testbed_paths.replay.new", record_testbed_paths()),
        ] {
            let path = dir.join(name);
            std::fs::write(&path, replay.to_bytes())
                .unwrap_or_else(|err| panic!("writing {}: {err}", path.display()));
        }
    }

    let mut paths: Vec<PathBuf> = match std::fs::read_dir(&dir) {
        Ok(entries) => entries
            .map(|entry| entry.expect("a directory entry").path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "replay"))
            .collect(),
        Err(_) => Vec::new(),
    };
    paths.sort();
    for name in GOLDEN {
        let golden = dir.join(name);
        assert!(
            paths.contains(&golden),
            "the blessed golden replay {} is missing: {paths:?}",
            golden.display()
        );
    }
    for path in &paths {
        let bytes =
            std::fs::read(path).unwrap_or_else(|err| panic!("reading {}: {err}", path.display()));
        let replay = Replay::from_bytes(&bytes)
            .unwrap_or_else(|err| panic!("{} does not decode: {err}", path.display()));
        assert_verifies(&replay, &path.display().to_string());
    }
}

/// Both committed goldens carry the semantic checks of their record functions, and verify with
/// them (ADR-0021).
#[test]
fn golden_semantics_hold() {
    let dir = replays_dir();
    for (name, checks) in [
        ("testbed_wander.replay", wander_semantics()),
        ("testbed_paths.replay", paths_semantics()),
    ] {
        let path = dir.join(name);
        let bytes =
            std::fs::read(&path).unwrap_or_else(|err| panic!("reading {}: {err}", path.display()));
        let replay = Replay::from_bytes(&bytes)
            .unwrap_or_else(|err| panic!("{} does not decode: {err}", path.display()));
        assert_eq!(
            replay.semantic, checks,
            "{name} carries its semantic checks"
        );
        assert_verifies(&replay, name);
    }
}

#[test]
fn testbed_wander_replay_verifies() {
    let replay = record_testbed_wander();
    assert_eq!(
        replay.start,
        ReplayStart::Scenario("base.testbed".to_owned())
    );
    assert_eq!(replay.ticks, 400);
    assert_eq!(replay.hashes.len(), 400);
    let ticks: Vec<Tick> = replay.inputs.iter().map(|input| input.tick).collect();
    assert_eq!(ticks, [Tick(0), Tick(10), Tick(60)]);
    assert_verifies(&replay, "the fresh testbed wander replay");
}

#[test]
fn testbed_paths_replay_verifies() {
    let replay = record_testbed_paths();
    assert_eq!(
        replay.start,
        ReplayStart::Scenario("base.testbed".to_owned())
    );
    assert_eq!(replay.ticks, 900);
    assert_eq!(replay.hashes.len(), 900);
    let ticks: Vec<Tick> = replay.inputs.iter().map(|input| input.tick).collect();
    assert_eq!(ticks, [Tick(1), Tick(5), Tick(120), Tick(200), Tick(300)]);
    assert_verifies(&replay, "the fresh testbed paths replay");
}

/// The last move target of each of #1–#4: its formation slot of the queued move.
fn final_targets(s: &Scenario) -> Vec<FxVec2> {
    let world = s.world();
    PARTY
        .iter()
        .map(|&id| {
            let entity = world.resource::<SimIndex>().get(SimId(id)).expect("alive");
            let orders = &world.get::<MoveOrder>(entity).expect("mobile").orders;
            match orders.last() {
                Some(Order::Move { target }) => *target,
                other => panic!("#{id}'s last order is not the queued move: {other:?}"),
            }
        })
        .collect()
}

/// Ticks [`testbed_paths_semantics`] runs at most: #1–#4 finish their queued orders long after
/// the replay's 900 ticks (#1, the last, at about tick 1650).
const SEMANTIC_MAX_TICKS: u32 = 2400;

/// C8-style semantic checks on the movement testbed, run on past the replay's end until #1–#4
/// are idle: the units player 0 sent cross the river on bridge cells, no unit ever stands on a
/// water cell, and #1–#4 end within 3 m of their formation slots.
#[test]
fn testbed_paths_semantics() {
    let mut s = testbed_paths().build();
    // Every unit's terrain each tick, and the river cells each of #1–#4 stood on.
    let mut river_cells: Vec<BTreeSet<String>> = vec![BTreeSet::new(); PARTY.len()];
    let mut targets = None;
    let mut idle = false;
    while s.tick().0 < SEMANTIC_MAX_TICKS {
        s.run_ticks(1);
        if s.tick().0 == 201 {
            targets = Some(final_targets(&s));
        }
        let cells = unit_cells(s.world_mut());
        let terrain = s.world().resource::<TerrainGrid>();
        for unit in &cells {
            let here = terrain
                .terrain_at(unit.cell.0, unit.cell.1)
                .unwrap_or("open");
            assert_ne!(
                here,
                "base.water",
                "tick {}: {} on water at {:?}",
                s.tick().0 - 1,
                unit.id,
                unit.cell
            );
            if let Some(i) = PARTY.iter().position(|&id| SimId(id) == unit.id)
                && RIVER_ROWS.contains(&unit.cell.1)
            {
                river_cells[i].insert(here.to_owned());
            }
        }
        let world = s.world();
        idle = s.tick().0 > 201
            && PARTY.iter().all(|&id| {
                let entity = world.resource::<SimIndex>().get(SimId(id)).expect("alive");
                world
                    .get::<MoveOrder>(entity)
                    .expect("mobile")
                    .orders
                    .is_empty()
            });
        if idle {
            break;
        }
    }
    assert!(idle, "#1–#4 still have orders at tick {}", s.tick().0);
    let targets = targets.expect("the queued orders applied by tick 201");
    for (i, &id) in PARTY.iter().enumerate() {
        let bridge: BTreeSet<String> = ["base.bridge".to_owned()].into();
        assert_eq!(
            river_cells[i], bridge,
            "#{id} crossed the river on bridge cells only"
        );
        let pos = s.pose(id).expect("alive").pos;
        assert!(
            pos.y > Fx::from_int(4 * 128),
            "#{id} ended north of the river at {pos:?}"
        );
        let off = (pos - targets[i]).length();
        assert!(
            off <= Fx::from_int(3),
            "#{id} ended {off} m from its slot {:?}",
            targets[i]
        );
    }
}

/// The pose trace of the movement testbed ([`testbed_paths`]): a hash of every unit's pose
/// after each of its [`TICKS`] ticks, recorded on the M2 prelude branch's build (ticks
/// 310..=899 are cross-checked against the output of tag `m1` by `m1_save.rs`). A change that
/// must leave unit movement alone (the order queue, M2 prelude Task 13) keeps it matching.
/// `FL_RECORD_POSES=1` writes `tests/replays/testbed_paths.poses.new` instead.
#[test]
fn testbed_paths_pose_trace_matches() {
    let trace = pose_trace(&mut testbed_paths().build(), TICKS);
    assert_eq!(trace.len(), TICKS as usize);
    let dir = replays_dir();
    if std::env::var_os("FL_RECORD_POSES").is_some_and(|v| v == "1") {
        std::fs::create_dir_all(&dir).expect("the replays directory");
        let path = dir.join("testbed_paths.poses.new");
        write_pose_trace(&path, &trace)
            .unwrap_or_else(|err| panic!("writing {}: {err}", path.display()));
        return;
    }
    let path = dir.join("testbed_paths.poses");
    let recorded =
        read_pose_trace(&path).unwrap_or_else(|err| panic!("reading {}: {err}", path.display()));
    assert_eq!(trace, recorded, "unit poses differ from the recorded trace");
}
