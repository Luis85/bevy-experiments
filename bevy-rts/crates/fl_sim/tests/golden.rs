//! Golden stability: registering more names does not move an existing match's hashes
//! (spec §5.4, ADR-0004, ADR-0005). Names hash by their stable keys, never by dense ids, and
//! RNG streams are keyed by name, and absent state hashes to nothing, so a feature that adds a
//! command, a Hashed component and resource it never inserts and a stream it draws from every
//! tick leaves the hash stream unchanged.

use std::path::PathBuf;
use std::sync::Arc;

use bevy_ecs::prelude::*;
use fl_math::{Fx, FxVec2};
use fl_protocol::{CommandPayload, DebugWanderCmd, PlayerId, SimId, Tick};
use fl_sim::{
    Class, CommandCtx, CommandHandler, CommandRegistry, MatchSeed, MatchSetup, Pose, RejectReason,
    RunConfig, SimBuilder, SimClock, SimEntity, SimFeature, SimHash, SimIndex, SimRunner,
    StateHasher, TestDriver, ThinkSet, build_sim_app, load_content, rolls, setup_scenario,
};
use fl_view::{CommandDraft, TickDriver};
use serde::{Deserialize, Serialize};

fn core_mod() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/core_mod")
}

/// `"aaa.noop"`: does nothing.
#[derive(Debug, Serialize, Deserialize)]
struct NoopCmd;

impl CommandPayload for NoopCmd {
    const NAME: &'static str = "aaa.noop";
}

struct NoopHandler;

impl CommandHandler<NoopCmd> for NoopHandler {
    fn apply(
        &self,
        _payload: &NoopCmd,
        _world: &mut World,
        _ctx: &CommandCtx<'_>,
    ) -> Result<(), RejectReason> {
        Ok(())
    }
}

/// Hashed `"aaa.marker"`, never inserted.
#[derive(Component, Serialize, Deserialize)]
struct Marker;

impl SimHash for Marker {
    fn sim_hash(&self, _h: &mut StateHasher) {}
}

/// Hashed `"aaa.unused"`, never inserted.
#[derive(Resource, Serialize, Deserialize)]
struct Unused;

impl SimHash for Unused {
    fn sim_hash(&self, _h: &mut StateHasher) {}
}

/// The sum of this tick's `"aaa.stream"` draws: per-tick, so Transient `"aaa.draws"`, reset
/// every tick. It never feeds back into the sim, so the hashes cannot see it.
#[derive(Resource, Default)]
struct Draws(u64);

/// Draws once per entity per tick from `"aaa.stream"`; every name sorts before the core's.
struct ExtraFeature;

impl SimFeature for ExtraFeature {
    fn id(&self) -> &'static str {
        "aaa"
    }

    fn version(&self) -> u32 {
        1
    }

    fn build(&self, sim: &mut SimBuilder) {
        sim.app_mut().init_resource::<Draws>();
        sim.register_command::<NoopCmd>(NoopHandler)
            .register_component::<Marker>("aaa.marker", Class::hashed(1))
            .register_resource::<Unused>("aaa.unused", Class::hashed(1))
            .register_resource::<Draws>("aaa.draws", Class::transient());
        let stream = sim.register_stream("aaa.stream");
        sim.add_systems(
            ThinkSet::Orders,
            move |clock: Res<SimClock>,
                  seed: Res<MatchSeed>,
                  units: Query<&SimEntity>,
                  mut draws: ResMut<Draws>| {
                for unit in &units {
                    let draw = rolls(&seed, clock.tick, unit.id(), stream).next_u64();
                    draws.0 = draws.0.wrapping_add(draw);
                }
            },
        );
    }
}

/// The duel with `features`, both units wandering (radius 100) from tick 0: every tick's
/// hash over `ticks` ticks, and #1's final position.
fn duel_hashes(features: &[Arc<dyn SimFeature>], ticks: u32) -> (Vec<(Tick, u64)>, Pose) {
    let content = load_content(features, &[core_mod()]).expect("core_mod loads");
    let setup = MatchSetup {
        seed: 3,
        cheats_allowed: true,
    };
    let run = RunConfig {
        hash_interval: Some(1),
        ..RunConfig::default()
    };
    let mut app = build_sim_app(features, content, setup, run).expect("the sim builds");
    setup_scenario(app.world_mut(), "core_test.duel").expect("duel sets up");
    let mut runner = SimRunner::new(app);
    let mut driver = TestDriver::new();
    let wander = DebugWanderCmd {
        radius: Fx::from_int(100),
    };
    for (player, unit) in [(0, 1), (1, 2)] {
        let kind = runner.world().resource::<CommandRegistry>().encode(&wander);
        driver.issue(CommandDraft {
            player: PlayerId(player),
            units: vec![SimId(unit)],
            kind,
            queue: false,
        });
    }
    let hashes = runner
        .run(&mut driver, ticks)
        .into_iter()
        .map(|out| (out.tick, out.hash.expect("hash interval is 1")))
        .collect();
    let world = runner.world();
    let first = world.resource::<SimIndex>().get(SimId(1)).expect("#1");
    (hashes, *world.get::<Pose>(first).expect("a pose"))
}

#[test]
fn golden_stability_extra_feature() {
    let (plain, plain_pose) = duel_hashes(&[], 300);
    let (extra, extra_pose) = duel_hashes(&[Arc::new(ExtraFeature)], 300);
    assert_eq!(plain.len(), 300);
    let first_difference = plain.iter().zip(&extra).find(|(a, b)| a != b);
    assert_eq!(first_difference, None, "the extra feature moved the hashes");
    assert_eq!(plain_pose, extra_pose);
    // Not vacuous: #1 wandered off its start at (100, 100).
    let start = FxVec2::new(Fx::from_int(100), Fx::from_int(100));
    assert_ne!(plain_pose.pos, start, "#1 wandered");
}
