//! The determinism and save fitness checks of `fl_test_utils` over the base content: twin
//! runs, schedule-shuffle runs, the byte-identical save round trip and the branch test
//! (spec §12.3, ADR-0005, ADR-0006). The `smoke_` tests run in every `gate task`.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use bevy_ecs::schedule::IntoScheduleConfigs;

use fl_math::{Angle, Fx, FxVec2};
use fl_protocol::{MoveCmd, PlayerId, SimId, Tick};
use fl_sim::{
    MapInfo, MatchHeaderRes, MatchSeed, MatchSetup, Owner, Players, RunConfig, ScaleScenario,
    SimBuilder, SimFeature, SimIndex, SimTick, build_sim_app, system_order,
};
use fl_test_utils::{
    ScenarioBuilder, assert_branch, assert_deterministic, assert_save_round_trip,
    assert_shuffle_invariant, base_content, shuffle_seeds,
};

fn v(x: i32, y: i32) -> FxVec2 {
    FxVec2::new(Fx::from_int(x), Fx::from_int(y))
}

fn move_to(x: i32, y: i32) -> MoveCmd {
    MoveCmd { target: v(x, y) }
}

/// The base testbed: #1–#5 player 0 at y = 350, #6–#10 player 1 at y = 650.
fn testbed() -> ScenarioBuilder {
    ScenarioBuilder::new().scenario("base.testbed")
}

fn testbed_wander() -> ScenarioBuilder {
    testbed().wander_all(100)
}

#[test]
fn smoke_testbed_twin_run() {
    assert_deterministic(testbed_wander, 600);

    // Not vacuous: the units wandered off their start.
    let mut s = testbed_wander().build();
    let start = s.pose(1).expect("#1 exists");
    s.run_ticks(600);
    assert_eq!(s.tick(), Tick(600));
    assert_eq!(s.hashes().len(), 600, "one hash per tick");
    assert_ne!(s.pose(1).expect("#1 alive").pos, start.pos, "#1 wandered");
}

// The shuffle probes: systems that access nothing, so nothing orders them.
fn probe_0() {}
fn probe_1() {}
fn probe_2() {}
fn probe_3() {}
fn probe_4() {}
fn probe_5() {}
fn probe_6() {}
fn probe_7() {}

/// Eight systems that access nothing, added to `SimTick` outside every hook, so each is a root
/// of the schedule's dependency graph (no system before it), with no order among them.
///
/// Bevy's `shuffle_seed` shuffles only the order of the graph's *nodes*; each node keeps its
/// edge order, and the executed order is the reverse of Tarjan's DFS finish order. A system
/// reached through a predecessor is therefore visited in that predecessor's (unshuffled) edge
/// order, so the order of unordered systems inside a hook changes only for some seeds. The
/// order of roots is the reverse of their shuffled node order: a seed leaves eight roots in
/// their unshuffled order with probability 1/40320. The M0 game has no unordered systems of
/// its own (its five Housekeeping clears, which were roots, became one `reset_transients`).
struct ShuffleProbe;

impl SimFeature for ShuffleProbe {
    fn id(&self) -> &'static str {
        "shuffle_probe"
    }

    fn version(&self) -> u32 {
        1
    }

    fn build(&self, sim: &mut SimBuilder) {
        sim.app_mut().add_systems(
            SimTick,
            // They access nothing, so running anywhere (even beside an exclusive system) is
            // harmless; ambiguity detection would otherwise pair them with every exclusive one.
            (
                probe_0, probe_1, probe_2, probe_3, probe_4, probe_5, probe_6, probe_7,
            )
                .ambiguous_with_all(),
        );
    }
}

/// The executed system order of the game's `SimTick`, with the [`ShuffleProbe`], under
/// `shuffle_seed`.
fn order(shuffle_seed: Option<u64>) -> String {
    let run = RunConfig {
        shuffle_seed,
        ..RunConfig::default()
    };
    let mut features = fl_catalog::features();
    features.push(Arc::new(ShuffleProbe));
    let mut app = build_sim_app(&features, base_content(), MatchSetup::default(), run)
        .expect("the sim builds");
    system_order(&mut app)
}

/// The probes' lines of a system order, in executed order.
fn probe_order(order: &str) -> Vec<&str> {
    order
        .lines()
        .filter(|line| line.contains("determinism::probe_"))
        .collect()
}

#[test]
fn smoke_shuffle_invariant() {
    let seeds = shuffle_seeds();
    assert!(!seeds.is_empty(), "at least one shuffle seed");
    // Not vacuous: every seed runs the unordered probes in another order.
    let plain = order(None);
    assert_eq!(probe_order(&plain).len(), 8, "{plain}");
    for &seed in &seeds {
        let shuffled = order(Some(seed));
        assert_ne!(
            probe_order(&shuffled),
            probe_order(&plain),
            "shuffle seed 0x{seed:x} leaves the unordered probes in order"
        );
    }
    assert_shuffle_invariant(|| testbed_wander().feature(Arc::new(ShuffleProbe)), 200);
}

#[test]
fn smoke_save_round_trip() {
    let mut s = testbed_wander().build();
    // Several split points: before the first tick, right after the wander orders, mid-run.
    for split in [0, 1, 50] {
        s.run_ticks(split - s.tick().0);
        assert_save_round_trip(&mut s);
    }

    // `Scenario::load` resumes at the saved tick with the same next hash.
    let bytes = s.save();
    let mut loaded = fl_test_utils::Scenario::load(fl_catalog::features(), base_content(), &bytes);
    assert_eq!(loaded.tick(), Tick(50));
    s.run_ticks(1);
    loaded.run_ticks(1);
    assert_eq!(loaded.hashes(), s.hashes()[50..]);
}

#[test]
fn smoke_branch() {
    assert_branch(
        || {
            testbed_wander()
                .command(10, 0, &[1, 2], move_to(500, 300))
                .command(60, 1, &[6, 7], move_to(400, 400))
        },
        40,
        120,
    );
}

#[test]
fn scale_twin_run_500_units() {
    assert_deterministic(
        || {
            ScenarioBuilder::new().scale(ScaleScenario {
                units: 500,
                ..ScaleScenario::default()
            })
        },
        200,
    );
}

/// A scale run with the full entity mix (followers, projectiles in flight, sensing) saved
/// mid-flight and loaded runs on with the unbroken run's hashes: the `ScaleMix` that drives
/// the firing is saved with it.
#[test]
fn scale_mix_branch() {
    assert_branch(
        || ScenarioBuilder::new().scale(ScaleScenario::with_units(300)),
        33,
        80,
    );
}

#[test]
fn commands_mid_run_twin() {
    let make = || {
        testbed()
            .command(10, 0, &[1, 2, 3], move_to(500, 500))
            .command(50, 1, &[6, 7], move_to(300, 400))
            .command(90, 0, &[1], move_to(600, 600))
    };
    assert_deterministic(make, 150);

    let mut s = make().build();
    let start = s.pose(1).expect("#1").pos;
    let idle = s.pose(4).expect("#4").pos;
    s.run_ticks(150);
    assert_ne!(s.pose(1).expect("#1").pos, start, "#1 drove off");
    assert_eq!(s.pose(4).expect("#4").pos, idle, "#4 had no orders");
}

#[test]
fn different_seeds_diverge() {
    let run = |seed: u64| {
        let mut s = ScenarioBuilder::new()
            .scale(ScaleScenario {
                units: 500,
                ..ScaleScenario::default()
            })
            .seed(seed)
            .build();
        // The header's seed is the seed the sim draws from.
        let world = s.world();
        assert_eq!(world.resource::<MatchHeaderRes>().0.seed, seed);
        assert_eq!(*world.resource::<MatchSeed>(), MatchSeed(seed));
        s.run_ticks(50);
        s.hashes().last().copied().expect("hashed")
    };
    let (a, b) = (run(1), run(2));
    assert_eq!(a.0, b.0);
    assert_ne!(a.1, b.1, "seeds 1 and 2 give the same final hash");
}

#[test]
fn builder_units_get_sequential_ids() {
    let s = testbed()
        .unit("base.de.pz3", 0, 100, 100)
        .unit("base.su.t34", 1, 200, 200)
        .build();
    assert_eq!(s.world().resource::<SimIndex>().len(), 12);
    let p11 = s.pose(11).expect("#11 follows the 10 scenario units");
    assert_eq!(p11.pos, v(100, 100));
    assert_eq!(p11.heading, Angle::ZERO);
    assert_eq!(s.pose(12).expect("#12").pos, v(200, 200));
    let entity = s
        .world()
        .resource::<SimIndex>()
        .get(SimId(12))
        .expect("#12");
    assert_eq!(s.world().get::<Owner>(entity), Some(&Owner(PlayerId(1))));

    // Without a scenario: the builder's map, players and units only.
    let s = ScenarioBuilder::new()
        .map_flat(800, 600)
        .player("base.de", 0)
        .player("base.su", 1)
        .unit("base.de.tiger", 1, 10, 20)
        .unit("base.su.kv1", 0, 30, 40)
        .build();
    let world = s.world();
    assert_eq!(world.resource::<MapInfo>().size, v(800, 600));
    assert_eq!(world.resource::<Players>().list.len(), 2);
    assert_eq!(world.resource::<SimIndex>().len(), 2);
    assert_eq!(s.pose(1).expect("#1").pos, v(10, 20));
    assert_eq!(s.pose(2).expect("#2").pos, v(30, 40));
}

/// How many scenarios [`divergence_is_reported`] built.
static BUILDS: AtomicU32 = AtomicU32::new(0);

#[test]
#[should_panic(expected = "diverged at tick")]
fn divergence_is_reported() {
    assert_deterministic(
        || {
            // The second run puts the unit one metre further east.
            let x = 100 + i32::try_from(BUILDS.fetch_add(1, Ordering::SeqCst)).expect("few");
            ScenarioBuilder::new()
                .map_flat(1000, 1000)
                .player("base.de", 0)
                .unit("base.de.pz3", 0, x, 100)
        },
        10,
    );
}
