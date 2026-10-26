//! The M1 testbed save fixture (M2 prelude Task 2, R83): the movement testbed saved at tick 310
//! by tag `m1` (before the order queue changes `core.move_order`), mid-path with a Shift-queued
//! rotate and move, a follower (#5) and player 1's flow group. Later builds load it and must
//! move every unit exactly as M1 did.

#[path = "support/testbed_paths.rs"]
#[allow(
    dead_code,
    reason = "shared with golden.rs and movement_determinism.rs, which use other items"
)]
mod testbed_paths;

use std::path::PathBuf;

use fl_protocol::Tick;
use fl_sim::{MoveOrder, Order, PathFollow, PathState, RunConfig, SimClock, SimIndex, load_world};
use fl_test_utils::{Scenario, base_content, pose_trace, read_pose_trace};
use testbed_paths::TICKS;

const SAVE_TICK: u32 = 310;

fn replays_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/replays")
}

/// The M1 testbed save (protected, blessed).
fn m1_save() -> Vec<u8> {
    let path = replays_dir().join("m1_testbed.save");
    std::fs::read(&path).unwrap_or_else(|err| panic!("reading {}: {err}", path.display()))
}

#[test]
fn m1_testbed_save_loads() {
    let run = RunConfig {
        hash_interval: Some(1),
        ..RunConfig::default()
    };
    let loaded = load_world(&fl_catalog::features(), base_content(), &m1_save(), run)
        .unwrap_or_else(|err| panic!("the M1 save does not load: {err}"));
    let world = loaded.app.world();
    assert_eq!(world.resource::<SimClock>().tick, Tick(SAVE_TICK));
    assert_eq!(world.resource::<SimIndex>().len(), 13);

    // Mid-path content: units driving a path, player 1's flow group, and queued rotate or
    // follow orders (the save holds no pending path requests: the queue has drained).
    let (mut following, mut flow, mut queued) = (0, 0, 0);
    for (_, entity) in world.resource::<SimIndex>().iter() {
        match world.get::<PathFollow>(entity).map(|path| &path.state) {
            Some(PathState::Following) => following += 1,
            Some(PathState::Flow { .. }) => flow += 1,
            _ => {}
        }
        if let Some(orders) = world.get::<MoveOrder>(entity) {
            queued += orders
                .orders
                .iter()
                .filter(|order| matches!(order, Order::Rotate { .. } | Order::Follow { .. }))
                .count();
        }
    }
    assert!(following > 0, "a unit is mid-path");
    assert!(flow > 0, "a unit follows a flow field");
    assert!(queued > 0, "a rotate or follow order is queued");
}

#[test]
fn m1_save_continues_with_m1_pose_trace() {
    let recorded = read_pose_trace(&replays_dir().join("testbed_paths.poses"))
        .unwrap_or_else(|err| panic!("the recorded pose trace: {err}"));
    let mut s = Scenario::load(fl_catalog::features(), base_content(), &m1_save());
    let rest = TICKS - SAVE_TICK;
    let trace = pose_trace(&mut s, rest);
    assert_eq!(trace.len(), rest as usize);
    let expected: Vec<_> = recorded
        .iter()
        .filter(|(tick, _)| (SAVE_TICK..TICKS).contains(&tick.0))
        .copied()
        .collect();
    assert_eq!(
        expected.len(),
        rest as usize,
        "the recorded trace covers 310..=899"
    );
    assert_eq!(
        trace, expected,
        "the M1 save moves its units exactly as M1 did"
    );
}
