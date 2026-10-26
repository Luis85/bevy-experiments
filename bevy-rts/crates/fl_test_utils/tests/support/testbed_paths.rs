//! The movement testbed of M1 Task 14: the base testbed with river crossings, a flow group, a
//! drowned bridge, Shift-queued orders and a follow. `golden.rs` records it as
//! `tests/replays/testbed_paths.replay`, and `movement_determinism.rs` runs it twin, shuffled,
//! saved and branched; both include this file with
//! `#[path = "support/testbed_paths.rs"] mod testbed_paths;`, so they run the very same match.

use fl_math::{Angle, Fx, FxVec2};
use fl_protocol::{
    DebugSetTerrainCmd, DebugSpawnCmd, FollowCmd, MoveCmd, PlayerId, RotateCmd, SimId,
};
use fl_test_utils::ScenarioBuilder;

/// Ticks the golden replay runs.
pub const TICKS: u32 = 900;

/// Player 0's crossing party: #1 pz2, #2 pz3, #3 tiger and #4 opel blitz. Four units are below
/// `Path.flow_min_group` (6), so each gets a single request.
pub const PARTY: [u32; 4] = [1, 2, 3, 4];

/// Player 1's movers: #6 t70, #7 t34, #8 kv1, #9 zis5, #10 rifleman and the T-34s #11–#13
/// spawned at tick 1. The six Tracked units form one flow group; the truck and the rifleman get
/// single requests.
pub const PLAYER_1: [u32; 8] = [6, 7, 8, 9, 10, 11, 12, 13];

/// Player 1's flow group: its Tracked movers.
pub const FLOW_GROUP: [u32; 6] = [6, 7, 8, 11, 12, 13];

/// Where player 1 spawns its T-34s (#11–#13) at tick 1: grass 30 m north of its line.
pub const SPAWNED_T34: [(i32, i32); 3] = [(380, 680), (410, 680), (440, 680)];

/// Bridge A, the corner cells of x 70..=72 over the river rows: drowned at tick 120.
pub const BRIDGE_A: ((u32, u32), (u32, u32)) = ((70, 122), (72, 127));

/// The river's cell rows (water across the map, but for the bridges).
pub const RIVER_ROWS: std::ops::RangeInclusive<u32> = 122..=127;

/// The tick bridge A is drowned.
pub const DROWN_TICK: u32 = 120;

/// The tick #5 starts following #1.
pub const FOLLOW_TICK: u32 = 300;

pub fn v(x: i32, y: i32) -> FxVec2 {
    FxVec2::new(Fx::from_int(x), Fx::from_int(y))
}

/// The base testbed, cheats on, hashed every tick:
/// - tick 1: player 1 spawns three T-34s (#11–#13) at [`SPAWNED_T34`] (`debug.spawn`);
/// - tick 5: player 0 moves [`PARTY`] to (720, 760), across the river; player 1 moves
///   [`PLAYER_1`] to (300, 250), across it the other way;
/// - tick 120: `debug.set_terrain` turns bridge A ([`BRIDGE_A`]) into water;
/// - tick 200: player 0 Shift-queues a rotate of [`PARTY`] toward (720, 900), then a move to
///   (650, 800);
/// - tick 300: #5 (player 0's rifleman) follows #1.
pub fn testbed_paths() -> ScenarioBuilder {
    let mut b = ScenarioBuilder::new().scenario("base.testbed");
    for (x, y) in SPAWNED_T34 {
        b = b.command(
            1,
            1,
            &[],
            DebugSpawnCmd {
                unit: "base.su.t34".to_owned(),
                owner: PlayerId(1),
                pos: v(x, y),
                heading: Angle::from_degrees(Fx::from_int(270)),
            },
        );
    }
    let (min_cell, max_cell) = BRIDGE_A;
    b.command(
        5,
        0,
        &PARTY,
        MoveCmd {
            target: v(720, 760),
        },
    )
    .command(
        5,
        1,
        &PLAYER_1,
        MoveCmd {
            target: v(300, 250),
        },
    )
    .command(
        DROWN_TICK,
        0,
        &[],
        DebugSetTerrainCmd {
            min_cell,
            max_cell,
            terrain: "base.water".to_owned(),
        },
    )
    .queued_command(
        200,
        0,
        &PARTY,
        RotateCmd {
            toward: v(720, 900),
        },
    )
    .queued_command(
        200,
        0,
        &PARTY,
        MoveCmd {
            target: v(650, 800),
        },
    )
    .command(FOLLOW_TICK, 0, &[5], FollowCmd { leader: SimId(1) })
}
