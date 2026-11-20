//! S4 tick cost: the faster supplier and route lookups change no result.
//!
//! The lookups behind a leaf start (the supplier scan, its stock counts and
//! route estimates) reuse query states (`village::lookup`), count stock
//! without copying items and search routes over a dense scratch array. The
//! test switch `lookup::BruteForce` puts every one back on its original path,
//! so both can be compared on the shipped world:
//! - the answers, sampled through a run;
//! - the state hash of a whole day, every tick;
//! - a save between two market rounds, continued as the title screen does.
use bevy_app::App;
use bevy_ecs::prelude::*;
use island_content::village::{self, Villager, catalog, economy, lookup, market, view};
use npc_framework::kernel::hash::state_hash;
use npc_framework::prelude::*;
use npc_framework::snapshot;

const SEED: u64 = 7;

fn island(seed: u64) -> App {
    island_content::replay::try_new_replay_app(seed, "island:living_world", 1).expect("island")
}

fn residents(world: &mut World) -> Vec<(SimId, Entity)> {
    let mut out: Vec<(SimId, Entity)> = world
        .query::<(Entity, &SimId, &Villager)>()
        .iter(world)
        .filter(|(_, _, person)| person.role != village::Role::Visitor)
        .map(|(e, id, _)| (*id, e))
        .collect();
    out.retain(|(_, e)| village::living(world, *e));
    out.sort_unstable();
    out
}

/// What the S4 paths answer, for these residents and every traded good.
fn answers(
    world: &World,
    people: &[(SimId, Entity)],
) -> Vec<(SimId, &'static str, Option<SimId>, u32)> {
    let mut out = Vec::new();
    for (id, e) in people {
        for kind in catalog::trade_kinds(world) {
            out.push((
                *id,
                *kind,
                view::supplier(world, *e, kind),
                economy::count(world, *e, kind),
            ));
        }
    }
    out
}

fn with_brute_force<R>(world: &mut World, read: impl FnOnce(&World) -> R) -> R {
    world.insert_resource(lookup::BruteForce);
    let answer = read(world);
    world.remove_resource::<lookup::BruteForce>();
    answer
}

#[test]
fn offer_index_equals_brute_force() {
    let mut app = island(SEED);
    let world = app.world_mut();
    let mut compared = 0;
    let mut sellers_found = 0;
    for sample in 0..120_usize {
        run_ticks(world, 50);
        let people = residents(world);
        assert!(!people.is_empty());
        // Four residents a sample, in turn, so every resident is asked about
        // every good several times in the run.
        let chosen: Vec<_> = (0..4)
            .map(|k| people[(sample * 4 + k) % people.len()])
            .collect();
        let fast = answers(world, &chosen);
        let slow = with_brute_force(world, |world| answers(world, &chosen));
        assert_eq!(fast, slow, "tick {}", world.resource::<SimClock>().tick);
        compared += fast.len();
        sellers_found += fast.iter().filter(|row| row.2.is_some()).count();
        let ids = lookup::ids_with::<village::Vitality>(world);
        let mut fresh: Vec<(SimId, Entity)> = world
            .query::<(Entity, &SimId, &village::Vitality)>()
            .iter(world)
            .map(|(e, id, _)| (*id, e))
            .collect();
        fresh.sort_unstable();
        assert_eq!(ids, fresh, "the reused query answers as a fresh one");
    }
    assert!(compared > 5_000, "{compared} answers compared");
    assert!(
        sellers_found > 500,
        "only {sellers_found} answers named a seller"
    );
}

#[test]
fn hashes_unchanged_by_offer_index() {
    let mut fast = island(SEED);
    let mut slow = island(SEED);
    slow.world_mut().insert_resource(lookup::BruteForce);
    let day = fast.world().resource::<SimConfig>().day_length;
    for _ in 0..day {
        run_ticks(fast.world_mut(), 1);
        run_ticks(slow.world_mut(), 1);
        let tick = fast.world().resource::<SimClock>().tick;
        assert_eq!(
            state_hash(fast.world_mut()),
            state_hash(slow.world_mut()),
            "tick {tick}"
        );
    }
}

#[test]
fn save_load_mid_round_keeps_hashes() {
    let mut app = island(SEED);
    let world = app.world_mut();
    let round = market::round_ticks(world);
    // Past the first rounds and plan refreshes, then to the middle of a round.
    run_ticks(world, 6 * round);
    let until_mid = loop {
        let now = world.resource::<SimClock>().tick;
        let last = world
            .resource::<market::MarketState>()
            .last_round
            .expect("market rounds ran");
        if now == last + round / 2 {
            break now;
        }
        run_ticks(world, 1);
    };
    assert!(until_mid > 0);
    let saved = snapshot::save(app.world_mut()).expect("saved");
    let mut restored = island(SEED);
    let report =
        snapshot::load(restored.world_mut(), &saved, snapshot::LoadMode::Strict).expect("restores");
    assert!(
        report.defaulted.is_empty() && report.skipped.is_empty(),
        "{report:?}"
    );
    assert_eq!(
        state_hash(restored.world_mut()),
        state_hash(app.world_mut())
    );
    // Across four round boundaries and plan refreshes, every tick.
    for _ in 0..4 * round {
        run_ticks(app.world_mut(), 1);
        run_ticks(restored.world_mut(), 1);
        let tick = app.world().resource::<SimClock>().tick;
        assert_eq!(
            state_hash(restored.world_mut()),
            state_hash(app.world_mut()),
            "tick {tick}"
        );
    }
}
