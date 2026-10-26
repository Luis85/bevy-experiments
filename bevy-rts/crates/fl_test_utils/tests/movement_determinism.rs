//! The M1 determinism suite (spec §12.3, ADR-0005, ADR-0006, ADR-0012, ADR-0015): twin runs,
//! schedule-shuffle runs, the save round trip and the branch test on scenarios that drive the
//! whole M1 movement stack — grid A* and HPA* paths through the budgeted path request queue,
//! flow fields for group moves, steering, `core.move`/`core.rotate`/`core.follow` with
//! Shift-queue and formations, `debug.set_terrain`, a value reload, and an M0 save run under M1.
//!
//! The per-feature branch tests in `crates/fl_sim/tests/*.rs` cover each mechanic on its own
//! small ASCII map; this suite runs them together on the testbed's real terrain
//! ([`testbed_paths`], also the `testbed_paths.replay` golden), on the 2 km scale map, across a
//! value reload and from the M0 save.
//!
//! **What the shuffle runs permute.** Bevy's `shuffle_seed` shuffles the order in which the
//! schedule graph's nodes are inserted before the topological sort, so it can move only
//! systems that no ordering constraint fixes. Every system of the M1 game's `SimTick` is
//! ordered (hooks are chained, and core systems inside a hook are chained, R46), so under each
//! seed the game runs the very same system order: [`m1_shuffle_moves_no_game_system`] pins
//! that. The shuffle runs here therefore prove that the shuffle setting changes no outcome, not
//! that unordered systems commute; what keeps unordered systems safe is `SimTick`'s ambiguity
//! detection at `LogLevel::Error` (an unordered pair with conflicting access fails the build).
//! The mechanism itself is checked by `determinism.rs::smoke_shuffle_invariant`, whose eight
//! unordered probe systems do move.
//!
//! **Proptests run on a fixed seed** (R140). [`m1_random_orders_branch`] draws its cases from
//! proptest's deterministic ChaCha RNG (`TestRng::deterministic_rng`) with failure persistence
//! off, so every gate run draws the same cases and a random seed can never fail a gate row. Its
//! case count is `PROPTEST_CASES` when set (the nightly gate's 4096), else a per-test default
//! ([`cases`]); a larger count runs a superset of the default's cases.

#[path = "support/testbed_paths.rs"]
#[allow(dead_code, reason = "shared with golden.rs, which uses other items")]
mod testbed_paths;

use std::path::Path;
use std::sync::Arc;

use bevy_ecs::entity::Entity;
use fl_content::ContentDb;
use fl_protocol::{
    DebugSetTerrainCmd, FollowCmd, MoveCmd, PlayerId, RotateCmd, SimId, StopCmd, Tick,
};
use fl_sim::{
    MatchSetup, MoveOrder, Order, Owner, PathFollow, PathQueue, PathState, RunConfig,
    ScaleScenario, SimIndex, SimRunner, TerrainGrid, TestDriver, apply_content_values,
    build_sim_app, compute_state_hash, load_world, system_order,
};
use fl_test_utils::{
    Scenario, ScenarioBuilder, assert_branch, assert_deterministic, assert_save_round_trip,
    assert_shuffle_invariant, base_content, shuffle_seeds,
};
use proptest::prelude::*;
use proptest::test_runner::{Config, RngAlgorithm, TestRng, TestRunner};
use testbed_paths::{BRIDGE_A, FLOW_GROUP, FOLLOW_TICK, PARTY, PLAYER_1, testbed_paths, v};

// --- helpers ---------------------------------------------------------------------------------

fn entity(s: &Scenario, id: u32) -> Entity {
    s.world()
        .resource::<SimIndex>()
        .get(SimId(id))
        .unwrap_or_else(|| panic!("#{id} is alive"))
}

fn orders(s: &Scenario, id: u32) -> Vec<Order> {
    s.world()
        .get::<MoveOrder>(entity(s, id))
        .expect("a mobile unit has a MoveOrder")
        .orders
        .clone()
}

fn path_state(s: &Scenario, id: u32) -> PathState {
    s.world()
        .get::<PathFollow>(entity(s, id))
        .expect("a mobile unit has a PathFollow")
        .state
}

/// The ids of `ids` following a flow field.
fn on_flow(s: &Scenario, ids: &[u32]) -> Vec<u32> {
    ids.iter()
        .copied()
        .filter(|&id| matches!(path_state(s, id), PathState::Flow { .. }))
        .collect()
}

/// The queued path requests, as `(unit, issued tick)`.
fn queued(s: &Scenario) -> Vec<(SimId, Tick)> {
    s.world()
        .resource::<PathQueue>()
        .pending
        .iter()
        .map(|r| (r.unit, r.issued))
        .collect()
}

/// Save `s` and carry on in a sim loaded from that save with `content` (for a save taken
/// after a value reload, the reloaded content): the loaded runner and driver replace `s`'s,
/// so `s` keeps its builder commands still to come and its hashes so far.
fn continue_from_save(s: &mut Scenario, content: Arc<ContentDb>) {
    let bytes = s.save();
    let run = RunConfig {
        hash_interval: Some(1),
        ..RunConfig::default()
    };
    let loaded = load_world(s.features(), content, &bytes, run)
        .unwrap_or_else(|err| panic!("the save of tick {} does not load: {err}", s.tick().0));
    let runner = SimRunner::new(loaded.app);
    let mut driver = TestDriver::new().starting_at(runner.next_tick());
    driver.restore(loaded.pending, &loaded.last_seq);
    s.runner = runner;
    s.driver = driver;
}

/// The hashes of ticks `from..` of `s`.
fn hashes_from(s: &Scenario, from: u32) -> Vec<(Tick, u64)> {
    s.hashes()
        .into_iter()
        .filter(|(tick, _)| tick.0 >= from)
        .collect()
}

/// `a` and `b` are at the same tick, gave the same hashes from tick `from` on, and save the
/// same bytes now.
fn assert_same_run(a: &mut Scenario, b: &mut Scenario, from: u32, what: &str) {
    assert_eq!(a.tick(), b.tick(), "{what}: both runs end at the same tick");
    let (ha, hb) = (hashes_from(a, from), hashes_from(b, from));
    assert!(!ha.is_empty(), "{what}: no hashes to compare");
    if let Some((x, y)) = ha.iter().zip(&hb).find(|(x, y)| x != y) {
        panic!(
            "{what}: diverged at tick {}: a=0x{:016x} b=0x{:016x}",
            x.0.0, x.1, y.1
        );
    }
    assert_eq!(ha.len(), hb.len(), "{what}: hash count");
    assert!(
        a.save() == b.save(),
        "{what}: the saves at tick {} differ",
        a.tick().0
    );
}

// --- the scenarios -------------------------------------------------------------------------

/// Cell side of [`ascii_river`], in metres.
const CELL_M: i32 = 4;

/// A 48 × 48 map with a river at rows 22..=25 crossed by two 2-cell bridges (A at x 6..=7, B
/// at x 40..=41), a forest block and a 3 m hill.
fn ascii_river() -> Vec<String> {
    (0..48u32)
        .map(|row| {
            (0..48u32)
                .map(|x| match (row, x) {
                    (22..=25, 6 | 7 | 40 | 41) => '=',
                    (22..=25, _) => '~',
                    (30..=34, 14..=20) => 'f',
                    (8..=10, 28..=30) => '3',
                    _ => '.',
                })
                .collect()
        })
        .collect()
}

/// The ASCII flow scenario on [`ascii_river`]: eight tanks (#1–#8, a flow group), a truck
/// (#9) and a rifleman (#10) of player 0 south of the river ordered north at tick 0; a tank of
/// player 1 (#11) ordered south at tick 10; bridge A drowned at tick 30.
fn ascii_flow() -> ScenarioBuilder {
    let rows = ascii_river();
    let rows: Vec<&str> = rows.iter().map(String::as_str).collect();
    let mut b = ScenarioBuilder::new()
        .ascii_map(&rows, CELL_M)
        .player("t.red", 0)
        .player("t.blue", 1);
    for i in 0..8 {
        b = b.unit("t.tank", 0, 20 + 8 * (i % 4), 20 + 8 * (i / 4));
    }
    let party: Vec<u32> = (1..=10).collect();
    b.unit("t.truck", 0, 60, 24)
        .unit("t.rifleman", 0, 68, 24)
        .unit("t.tank", 1, 100, 170)
        .command(0, 0, &party, MoveCmd { target: v(60, 160) })
        .command(10, 1, &[11], MoveCmd { target: v(40, 20) })
        .command(
            30,
            0,
            &[],
            DebugSetTerrainCmd {
                min_cell: (6, 22),
                max_cell: (7, 25),
                terrain: "t.water".to_owned(),
            },
        )
}

/// The scale scenario on the 2 km terrain map, 500 units wandering: its tick-0 burst of path
/// requests drains over several ticks under the per-tick budget.
fn scale_on_terrain() -> ScenarioBuilder {
    ScenarioBuilder::new().scale(ScaleScenario::on_terrain(500))
}

/// [`testbed_paths`] with an A→B→A follow cycle on the far bank's movers: at tick 320 player
/// 1's truck #9 follows its rifleman #10 and #10 follows #9.
fn testbed_paths_follow_cycle() -> ScenarioBuilder {
    testbed_paths()
        .command(320, 1, &[9], FollowCmd { leader: SimId(10) })
        .command(320, 1, &[10], FollowCmd { leader: SimId(9) })
}

// --- twin and shuffle runs -------------------------------------------------------------------

#[test]
fn smoke_m1_testbed_paths_twin() {
    assert_deterministic(testbed_paths, 300);

    // Not vacuous: the match drives the M1 movement stack within those 300 ticks.
    let mut s = testbed_paths().build();
    s.run_ticks(2);
    let index = s.world().resource::<SimIndex>();
    assert_eq!(
        index.len(),
        13,
        "ten scenario units and three spawned T-34s"
    );
    for id in PLAYER_1 {
        let e = index.get(SimId(id)).expect("alive");
        assert_eq!(
            s.world().get::<Owner>(e),
            Some(&Owner(PlayerId(1))),
            "#{id}"
        );
    }
    s.run_ticks(4);
    assert_eq!(
        on_flow(&s, &FLOW_GROUP),
        FLOW_GROUP,
        "the Tracked six follow one flow field"
    );
    for id in PARTY {
        assert_eq!(
            path_state(&s, id),
            PathState::Following,
            "#{id} on a single path"
        );
    }
    s.run_ticks(FOLLOW_TICK + 1 - s.tick().0);
    assert!(
        matches!(
            orders(&s, 5)[..],
            [Order::Follow {
                leader: SimId(1),
                ..
            }]
        ),
        "#5 follows #1"
    );
    for id in PARTY {
        assert!(
            matches!(
                orders(&s, id)[..],
                [Order::Move { .. }, Order::Rotate { .. }, Order::Move { .. }]
            ),
            "#{id} has its queued rotate and move: {:?}",
            orders(&s, id)
        );
    }
}

#[test]
fn m1_scenarios_shuffle_invariant() {
    assert_shuffle_invariant(testbed_paths, 600);
    assert_shuffle_invariant(ascii_flow, 400);
    assert_shuffle_invariant(scale_on_terrain, 200);
}

/// The shuffle seeds leave the M1 game's `SimTick` in its unshuffled order: every one of its
/// systems is ordered (module docs). A change that adds an unordered system fails here: that
/// is fine if ambiguity detection passes, and the module docs must then say which systems the
/// shuffle runs move.
#[test]
fn m1_shuffle_moves_no_game_system() {
    let order = |shuffle_seed: Option<u64>| {
        let run = RunConfig {
            shuffle_seed,
            ..RunConfig::default()
        };
        let mut app = build_sim_app(
            &fl_catalog::features(),
            base_content(),
            MatchSetup::default(),
            run,
        )
        .expect("the sim builds");
        system_order(&mut app)
    };
    let plain = order(None);
    assert!(plain.lines().count() > 20, "{plain}");
    for seed in shuffle_seeds() {
        assert_eq!(order(Some(seed)), plain, "shuffle seed 0x{seed:x}");
    }
}

// --- save round trips and branches ---------------------------------------------------------

#[test]
fn m1_save_round_trip_mid_pathing() {
    let mut s = testbed_paths().build();

    // Tick 6: the truck's and the rifleman's requests of tick 5 still wait on the budget.
    s.run_ticks(6);
    assert_eq!(
        queued(&s),
        [(SimId(9), Tick(5)), (SimId(10), Tick(5))],
        "requests queued at tick 6"
    );
    assert_save_round_trip(&mut s);

    // Tick 130: bridge A drowned ten ticks ago, and the group's field was rebuilt over it.
    s.run_ticks(130 - s.tick().0);
    let terrain = s.world().resource::<TerrainGrid>();
    let ((x0, y0), (x1, y1)) = BRIDGE_A;
    for (x, y) in [(x0, y0), (x1, y1)] {
        assert_eq!(
            terrain.terrain_at(x, y),
            Some("base.water"),
            "bridge A cell ({x}, {y})"
        );
    }
    assert_eq!(on_flow(&s, &FLOW_GROUP), FLOW_GROUP);
    assert_save_round_trip(&mut s);

    // Tick 320: #5 follows #1.
    s.run_ticks(320 - s.tick().0);
    assert!(matches!(
        orders(&s, 5)[..],
        [Order::Follow {
            leader: SimId(1),
            ..
        }]
    ));
    assert_save_round_trip(&mut s);
}

#[test]
fn m1_branch_mid_pathing() {
    // Tick 6: requests queued behind the budget (m1_save_round_trip_mid_pathing checks that);
    // tick 130: the flow group mid-route right after the bridge it was heading for drowned.
    assert_branch(testbed_paths, 6, 400);
    assert_branch(testbed_paths, 130, 500);
}

/// A branch with the flow group re-routed along the north bank toward bridge B (tick 600),
/// #5 following #1, and #1–#4 working through their queued orders.
#[test]
fn m1_branch_mid_flow_reroute() {
    let mut s = testbed_paths().build();
    s.run_ticks(600);
    assert_eq!(on_flow(&s, &FLOW_GROUP), FLOW_GROUP);
    assert!(matches!(orders(&s, 5)[..], [Order::Follow { .. }]));
    drop(s);
    assert_branch(testbed_paths, 600, 900);
}

/// A branch mid-follow on real terrain: #5 following #1 across the testbed, and #9 and #10
/// following each other (A→B→A) on the far bank.
#[test]
fn m1_branch_mid_follow_cycle() {
    let mut s = testbed_paths_follow_cycle().build();
    s.run_ticks(330);
    assert!(matches!(
        orders(&s, 9)[..],
        [Order::Follow {
            leader: SimId(10),
            ..
        }]
    ));
    assert!(matches!(
        orders(&s, 10)[..],
        [Order::Follow {
            leader: SimId(9),
            ..
        }]
    ));
    assert!(matches!(
        orders(&s, 5)[..],
        [Order::Follow {
            leader: SimId(1),
            ..
        }]
    ));
    drop(s);
    assert_branch(testbed_paths_follow_cycle, 330, 600);
}

/// A branch with hundreds of path requests waiting on the per-tick budget: the scale scenario's
/// tick-0 burst, two ticks in.
#[test]
fn m1_branch_mid_path_queue() {
    let mut s = scale_on_terrain().build();
    s.run_ticks(2);
    let waiting = queued(&s);
    assert!(
        waiting.len() > 100,
        "{} requests queued at tick 2",
        waiting.len()
    );
    assert!(
        waiting.iter().any(|&(_, issued)| issued == Tick(0)),
        "requests of tick 0 still wait at tick 2"
    );
    drop(s);
    assert_branch(scale_on_terrain, 2, 150);
}

#[test]
fn m1_ascii_flow_branch() {
    // Tick 20: the flow group mid-route toward bridge A; tick 31: right after it drowned.
    let mut s = ascii_flow().build();
    s.run_ticks(20);
    let tanks: Vec<u32> = (1..=8).collect();
    assert_eq!(on_flow(&s, &tanks), tanks);
    drop(s);
    assert_branch(ascii_flow, 20, 250);
    assert_branch(ascii_flow, 31, 250);
}

// --- a value reload ------------------------------------------------------------------------

/// Copy directory `from` into `to`, recursively.
fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("a directory");
    for entry in std::fs::read_dir(from).expect("a readable directory") {
        let entry = entry.expect("a directory entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("a file type").is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).expect("a copied file");
        }
    }
}

/// The base content with the T-34 at 72 km/h on roads instead of 54: a value-only edit (the
/// same ids), as a hot reload of `units/su/t34.unit.ron` loads it.
fn faster_t34() -> Arc<ContentDb> {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let base = dir.path().join("base");
    copy_dir(&fl_catalog::base_mod_dir(), &base);
    let file = base.join("units/su/t34.unit.ron");
    let text = std::fs::read_to_string(&file).expect("the T-34's file");
    let from = "road_speed_kmh: 54.0";
    assert!(text.contains(from), "{text}");
    std::fs::write(&file, text.replace(from, "road_speed_kmh: 72.0")).expect("written");
    fl_sim::load_content(&fl_catalog::features(), &[base])
        .unwrap_or_else(|err| panic!("the edited base mod does not load: {err}"))
}

/// The tick of the value reload: the flow group, four of whose six are T-34s, is mid-route
/// after bridge A drowned.
const RELOAD_TICK: u32 = 150;

/// [`testbed_paths`] run to [`RELOAD_TICK`], then reloaded with `content` between ticks.
fn reloaded_testbed(content: &Arc<ContentDb>) -> Scenario {
    let mut s = testbed_paths().build();
    s.run_ticks(RELOAD_TICK);
    let report = apply_content_values(s.world_mut(), Arc::clone(content)).expect("a value reload");
    assert_eq!(report.tick, Tick(RELOAD_TICK));
    s
}

/// A value reload is a replay event, and a replay holding one is `NonReproducible` by design
/// (ADR-0012), so the reload is proven on the save side:
/// - a save taken before the reload, loaded with the old content and reloaded at the same tick
///   on the loaded sim (whose derived caches a load left cold, not warm), runs on with the
///   unbroken run's hashes;
/// - a save taken right after the reload, or later, loaded with the reloaded content, does too;
///
/// and each saves the unbroken run's bytes at the end. And the reload did change the match.
#[test]
fn m1_branch_across_value_reload() {
    let content = faster_t34();
    let end = 450;
    let mut unbroken = reloaded_testbed(&content);
    unbroken.run_ticks(end - RELOAD_TICK);

    // Saved before the reload: the loaded sim, on the old content, is reloaded itself.
    let split = 140;
    let mut b = testbed_paths().build();
    b.run_ticks(split);
    continue_from_save(&mut b, base_content());
    b.run_ticks(RELOAD_TICK - split);
    let report = apply_content_values(b.world_mut(), Arc::clone(&content)).expect("a value reload");
    assert_eq!(report.tick, Tick(RELOAD_TICK));
    b.run_ticks(end - RELOAD_TICK);
    assert_same_run(
        &mut unbroken,
        &mut b,
        split,
        &format!("saved at tick {split}, reloaded at {RELOAD_TICK} on the loaded sim"),
    );

    for split in [RELOAD_TICK, RELOAD_TICK + 1, 320] {
        let mut b = reloaded_testbed(&content);
        b.run_ticks(split - RELOAD_TICK);
        continue_from_save(&mut b, Arc::clone(&content));
        b.run_ticks(end - split);
        assert_same_run(
            &mut unbroken,
            &mut b,
            split,
            &format!("saved at tick {split} after the reload at {RELOAD_TICK}"),
        );
    }

    // Not vacuous: without the reload the T-34s drive slower, and the hashes part after it.
    let mut plain = testbed_paths().build();
    plain.run_ticks(end);
    let before = RELOAD_TICK as usize;
    assert_eq!(plain.hashes()[..before], unbroken.hashes()[..before]);
    assert_ne!(
        plain.hashes().last(),
        unbroken.hashes().last(),
        "the reload changed the match"
    );
}

// --- the M0 save under M1 (R78, R81, R83) ----------------------------------------------------

fn m0_save() -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/replays/m0_testbed.save");
    std::fs::read(&path).unwrap_or_else(|err| panic!("reading {}: {err}", path.display()))
}

/// The M0 testbed save (tick 40) loaded under M1, both players ordering all their units across
/// the river at once: the M0 units path through M1's queue, flow fields and steering.
fn loaded_m0_with_orders() -> Scenario {
    let mut s = Scenario::load(fl_catalog::features(), base_content(), &m0_save());
    assert_eq!(s.tick(), Tick(40));
    s.issue(
        0,
        &[1, 2, 3, 4, 5],
        MoveCmd {
            target: v(720, 760),
        },
        false,
    );
    s.issue(
        1,
        &[6, 7, 8, 9, 10],
        MoveCmd {
            target: v(300, 250),
        },
        false,
    );
    s
}

/// The M0 save keeps working as M1 match state: loaded, ordered and branched mid-pathing (with
/// requests queued, and later mid-route), it runs on with the unbroken run's hashes.
#[test]
fn m1_m0_save_branches_mid_pathing() {
    let end = 400;
    let mut unbroken = loaded_m0_with_orders();
    unbroken.run_ticks(end - 40);

    for split in [41, 200] {
        let mut b = loaded_m0_with_orders();
        b.run_ticks(split - 40);
        if split == 41 {
            assert!(
                !queued(&b).is_empty(),
                "requests wait on the budget at tick 41"
            );
        }
        continue_from_save(&mut b, base_content());
        b.run_ticks(end - split);
        assert_same_run(
            &mut unbroken,
            &mut b,
            split,
            &format!("the M0 save, branched at tick {split}"),
        );
    }

    // Not vacuous: the M0 units drove off.
    let fresh = Scenario::load(fl_catalog::features(), base_content(), &m0_save());
    let moved = (1..=10)
        .filter(|&id| fresh.pose(id).map(|p| p.pos) != unbroken.pose(id).map(|p| p.pos))
        .count();
    assert!(moved >= 8, "{moved} of 10 M0 units moved");
    assert_ne!(
        compute_state_hash(fresh.world()),
        compute_state_hash(unbroken.world())
    );
}

// --- random orders (a fixed-seed proptest) -------------------------------------------------

/// One random order of [`m1_random_orders_branch`].
#[derive(Debug, Clone)]
enum RandomOrder {
    /// `units` (a bit mask over #1–#6) move to `(x, y)` metres, Shift-queued or not.
    Move {
        units: u8,
        x: i32,
        y: i32,
        queue: bool,
    },
    /// `units` turn toward `(x, y)` metres.
    Rotate {
        units: u8,
        x: i32,
        y: i32,
        queue: bool,
    },
    /// #`follower` follows #`leader` (both 1–6; a unit following itself is rejected).
    Follow {
        follower: u32,
        leader: u32,
        queue: bool,
    },
    Stop {
        units: u8,
    },
    /// Bridge A (`second` false) or B of [`ascii_river`] drowns.
    Drown {
        second: bool,
    },
}

fn random_order() -> impl Strategy<Value = RandomOrder> {
    let point = (0i32..192, 0i32..192);
    prop_oneof![
        4 => (1u8..64, point.clone(), any::<bool>())
            .prop_map(|(units, (x, y), queue)| RandomOrder::Move { units, x, y, queue }),
        1 => (1u8..64, point, any::<bool>())
            .prop_map(|(units, (x, y), queue)| RandomOrder::Rotate { units, x, y, queue }),
        2 => (1u32..=6, 1u32..=6, any::<bool>())
            .prop_map(|(follower, leader, queue)| RandomOrder::Follow { follower, leader, queue }),
        1 => (1u8..64).prop_map(|units| RandomOrder::Stop { units }),
        1 => any::<bool>().prop_map(|second| RandomOrder::Drown { second }),
    ]
}

/// The ids of #1–#6 set in `mask`.
fn ids_of(mask: u8) -> Vec<u32> {
    (1..=6u32).filter(|i| mask & (1 << (i - 1)) != 0).collect()
}

/// [`ascii_river`] with player 0's four tanks, a truck and a rifleman south of the river, and
/// `orders` issued at their ticks.
fn random_case(orders: &[(u32, RandomOrder)]) -> ScenarioBuilder {
    let rows = ascii_river();
    let rows: Vec<&str> = rows.iter().map(String::as_str).collect();
    let mut b = ScenarioBuilder::new()
        .ascii_map(&rows, CELL_M)
        .player("t.red", 0)
        .unit("t.tank", 0, 20, 20)
        .unit("t.tank", 0, 30, 24)
        .unit("t.tank", 0, 40, 20)
        .unit("t.tank", 0, 120, 30)
        .unit("t.truck", 0, 80, 20)
        .unit("t.rifleman", 0, 90, 30);
    for (tick, order) in orders {
        let t = *tick;
        b = match order.clone() {
            RandomOrder::Move { units, x, y, queue } => {
                let payload = MoveCmd { target: v(x, y) };
                if queue {
                    b.queued_command(t, 0, &ids_of(units), payload)
                } else {
                    b.command(t, 0, &ids_of(units), payload)
                }
            }
            RandomOrder::Rotate { units, x, y, queue } => {
                let payload = RotateCmd { toward: v(x, y) };
                if queue {
                    b.queued_command(t, 0, &ids_of(units), payload)
                } else {
                    b.command(t, 0, &ids_of(units), payload)
                }
            }
            RandomOrder::Follow {
                follower,
                leader,
                queue,
            } => {
                let payload = FollowCmd {
                    leader: SimId(leader),
                };
                if queue {
                    b.queued_command(t, 0, &[follower], payload)
                } else {
                    b.command(t, 0, &[follower], payload)
                }
            }
            RandomOrder::Stop { units } => b.command(t, 0, &ids_of(units), StopCmd),
            RandomOrder::Drown { second } => {
                let x = if second { 40 } else { 6 };
                b.command(
                    t,
                    0,
                    &[],
                    DebugSetTerrainCmd {
                        min_cell: (x, 22),
                        max_cell: (x + 1, 25),
                        terrain: "t.water".to_owned(),
                    },
                )
            }
        };
    }
    b
}

/// Ticks a random case runs.
const RANDOM_END: u32 = 200;

/// `default`, or `PROPTEST_CASES` when it is set (the nightly gate sets 4096), capped at `max`:
/// the case count of this suite's proptests (R140; the semantics of `hpa.rs`'s `cases()`). A case
/// runs a whole match (~0.14 s), so the cap keeps each test under the 180 s test timeout (R142).
/// The seed stays fixed whatever the count, so a larger count runs a superset of the default's.
fn cases(default: u32, max: u32) -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
        .min(max)
}

/// Run the first `cases` cases of the fixed seed: random orders (moves, rotates, follows and
/// stops, Shift-queued or not, and bridges drowned) at random ticks, branched at a random
/// tick: the loaded sim runs on with the unbroken run's hashes.
fn random_orders_branch(cases: u32) {
    let mut runner = TestRunner::new_with_rng(
        Config {
            cases,
            failure_persistence: None,
            ..Config::default()
        },
        TestRng::deterministic_rng(RngAlgorithm::ChaCha),
    );
    let strategy = (
        proptest::collection::vec((0u32..120, random_order()), 1..10),
        1u32..RANDOM_END,
    );
    runner
        .run(&strategy, |(orders, split)| {
            assert_branch(|| random_case(&orders), split, RANDOM_END);
            Ok(())
        })
        .unwrap_or_else(|err| panic!("{err}"));
}

#[test]
fn m1_random_orders_branch() {
    random_orders_branch(cases(12, 64));
}

/// [`m1_random_orders_branch`] at 256 cases of the same seed (or `PROPTEST_CASES`, at most
/// 512), for milestone runs.
#[test]
fn milestone_m1_random_orders_branch_long() {
    random_orders_branch(cases(256, 512));
}
