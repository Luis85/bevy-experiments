//! Fast multi-seed survival soak for the regional island (`living_world`).
//!
//! Three seeds run as separate tests so the harness executes them in parallel.
//! Each asserts the founding residents stay alive and housed, total currency is
//! conserved and no stock is invalid. The one-year variant is explicit.
use bevy_ecs::prelude::*;
use island_content::village::{
    self, CoinPurse, ResourcePatch, VillageState, Villager, Vitality, Wallet, colony,
    frontier::labor::ResidentProfile,
};
use npc_framework::interaction::Item;
use npc_framework::prelude::*;
use std::collections::BTreeSet;

const DAY: u64 = 24_000;
const SAMPLE: u64 = 600;
/// Horizon of the not-ignored tests: half a day. Three seeds in parallel ran
/// 48000 ticks in 320 s on a busy machine (about 150 ticks/s each in the test
/// profile), so the planned two days cannot fit a 90 s budget; 12000 ticks can.
const FAST_TICKS: u64 = DAY / 2;
const TWO_DAY_TICKS: u64 = 2 * DAY;
const YEAR_TICKS: u64 = 28 * DAY;

fn residents(world: &World) -> Vec<(SimId, Entity)> {
    world
        .resource::<SimIndex>()
        .map
        .iter()
        .filter(|(_, entity)| world.get::<ResidentProfile>(**entity).is_some())
        .map(|(id, entity)| (*id, *entity))
        .collect()
}

fn money(world: &World) -> u64 {
    world
        .resource::<SimIndex>()
        .map
        .values()
        .map(|entity| {
            u64::from(world.get::<Wallet>(*entity).map_or(0, |w| w.coins))
                + u64::from(world.get::<CoinPurse>(*entity).map_or(0, |p| p.coins))
        })
        .sum::<u64>()
        + u64::from(world.resource::<VillageState>().fund)
        // The merchant boat's purse is the other half of the closed system.
        + u64::from(village::merchant::purse(world))
}

fn check(world: &World, founders: &BTreeSet<SimId>, currency: u64, seed: u64) {
    let now = world.resource::<SimClock>().tick;
    let people = residents(world);
    let current: BTreeSet<_> = people.iter().map(|(id, _)| *id).collect();
    // Settlers may land by boat, but no founder may vanish.
    assert!(
        founders.is_subset(&current),
        "seed {seed} tick {now}: a founding identity vanished"
    );
    assert_eq!(
        money(world),
        currency,
        "seed {seed} tick {now}: currency drift"
    );
    for (id, actor) in people {
        let name = &world.get::<Villager>(actor).expect("resident").name;
        let vitality = world.get::<Vitality>(actor).expect("vitality");
        assert!(
            vitality.died_at.is_none() && vitality.health > 0,
            "seed {seed} tick {now}: {name} ({}) died",
            id.0
        );
        assert!(
            colony::home_of(world, id).is_some(),
            "seed {seed} tick {now}: {name} is unhoused"
        );
    }
    for (id, entity) in &world.resource::<SimIndex>().map {
        if let Some(patch) = world.get::<ResourcePatch>(*entity) {
            assert!(
                patch.remaining <= patch.capacity,
                "seed {seed} tick {now}: patch {} exceeds capacity",
                id.0
            );
        }
        if let Some(item) = world.get::<Item>(*entity) {
            assert!(
                item.stack >= 1,
                "seed {seed} tick {now}: item {} ({}) has an empty stack",
                id.0,
                item.kind
            );
        }
    }
}

fn soak(seed: u64, end: u64) {
    let mut app: bevy_app::App = island_content::new_sim_app(seed, "living_world");
    let founders: BTreeSet<_> = residents(app.world())
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    assert_eq!(founders.len(), 42);
    let currency = money(app.world());
    assert_eq!(
        currency,
        42 * u64::from(village::NPC_SEED_COINS) + u64::from(village::merchant::purse(app.world()))
    );
    check(app.world(), &founders, currency, seed);
    while app.world().resource::<SimClock>().tick < end {
        let now = app.world().resource::<SimClock>().tick;
        run_ticks(app.world_mut(), SAMPLE.min(end - now));
        check(app.world(), &founders, currency, seed);
    }
    village::integrity::validate_colony(app.world()).expect("colony integrity");
    let state = app.world().resource::<VillageState>();
    let civic = app.world().resource::<village::civic::CivicState>();
    println!(
        "ISLAND_SOAK seed={seed} ticks={end} transactions={} fund={} built={} cancelled={} \
         civic_spent={} produced={:?}",
        state.transactions,
        state.fund,
        civic.completed,
        civic.cancelled,
        civic.spent,
        state.produced
    );
    assert!(
        state.transactions > 0,
        "seed {seed}: no physical trade happened"
    );
}

#[test]
fn island_soak_seed_7_keeps_everyone_alive_housed_and_solvent() {
    soak(7, FAST_TICKS);
}

#[test]
fn island_soak_seed_11_keeps_everyone_alive_housed_and_solvent() {
    soak(11, FAST_TICKS);
}

#[test]
fn island_soak_seed_23_keeps_everyone_alive_housed_and_solvent() {
    soak(23, FAST_TICKS);
}

/// Ticks of the seed-divergence check: a tenth of a day is enough for different
/// founders to buy, eat and work differently.
const DIVERGENCE_TICKS: u64 = 2_400;

struct SeedRun {
    seed: u64,
    founders: Vec<ResidentProfile>,
    stores: Vec<u32>,
    hash: u64,
    trajectory: Vec<(SimPos, u32)>,
}

fn seed_run(seed: u64) -> SeedRun {
    let mut app: bevy_app::App = island_content::new_sim_app(seed, "living_world");
    let people = residents(app.world());
    let founders = people
        .iter()
        .map(|(_, actor)| app.world().get::<ResidentProfile>(*actor).unwrap().clone())
        .collect();
    let stores = people
        .iter()
        .map(|(_, actor)| {
            ["bread", "log", village::catalog::WATER]
                .into_iter()
                .map(|kind| village::economy::count(app.world(), *actor, kind))
                .sum()
        })
        .collect();
    let identities: BTreeSet<_> = people.iter().map(|(id, _)| *id).collect();
    let currency = money(app.world());
    run_ticks(app.world_mut(), DIVERGENCE_TICKS);
    check(app.world(), &identities, currency, seed);
    let trajectory = people
        .iter()
        .map(|(_, actor)| {
            (
                *app.world().get::<SimPos>(*actor).unwrap(),
                app.world().get::<Wallet>(*actor).unwrap().coins,
            )
        })
        .collect();
    SeedRun {
        seed,
        founders,
        stores,
        hash: state_hash(app.world_mut()),
        trajectory,
    }
}

/// The seed must matter: founders differ in taste, aptitudes and starting
/// stores, so no two of these seeds may produce the same world. Every seed
/// still satisfies the survival invariants at the end of the stretch.
#[test]
fn island_seeds_found_different_residents_and_diverge() {
    let runs: Vec<SeedRun> = [7, 11, 23]
        .into_iter()
        .map(|seed| std::thread::spawn(move || seed_run(seed)))
        .collect::<Vec<_>>()
        .into_iter()
        .map(|run| run.join().expect("seed run"))
        .collect();
    for (index, a) in runs.iter().enumerate() {
        for b in &runs[index + 1..] {
            println!(
                "ISLAND_SEEDS {} vs {}: hash {:016x} vs {:016x}",
                a.seed, b.seed, a.hash, b.hash
            );
            assert_ne!(
                a.founders, b.founders,
                "seeds {} and {} founded identical residents",
                a.seed, b.seed
            );
            assert_ne!(
                a.stores, b.stores,
                "seeds {} and {} gave identical starting stores",
                a.seed, b.seed
            );
            assert_ne!(
                a.hash, b.hash,
                "seeds {} and {} reached the same state hash",
                a.seed, b.seed
            );
            assert_ne!(
                a.trajectory, b.trajectory,
                "seeds {} and {} left every resident in the same place with the same coins",
                a.seed, b.seed
            );
        }
    }
}

/// Run explicitly:
/// cargo test --locked -p island_content --test island_soak_fast -- --ignored --nocapture
#[test]
#[ignore = "explicit two-day (48000-tick) island survival soak on three seeds"]
fn island_soak_two_days_three_seeds() {
    let runs: Vec<_> = [7, 11, 23]
        .into_iter()
        .map(|seed| std::thread::spawn(move || soak(seed, TWO_DAY_TICKS)))
        .collect();
    for run in runs {
        run.join().expect("two-day soak");
    }
}

/// Six days reach well into the first winter, with the civic planner building
/// from the third day on.
#[test]
#[ignore = "explicit six-day (144000-tick) island survival soak on three seeds"]
fn island_soak_six_days_three_seeds() {
    let runs: Vec<_> = [7, 11, 23]
        .into_iter()
        .map(|seed| std::thread::spawn(move || soak(seed, 6 * DAY)))
        .collect();
    for run in runs {
        run.join().expect("six-day soak");
    }
}

#[test]
#[ignore = "explicit one-year (28-day, 672000-tick) island survival soak"]
fn island_soak_one_year_seed_7() {
    soak(7, YEAR_TICKS);
}
