//! Winter heat on the regional island (`living_world`).
//!
//! Regression for the 14-day audit in which hearth fuel was ordered only once a
//! hearth was already going out: no log was cut before winter, the log price rose
//! from 1 to 64 coins, cold residents walked to other settlements to buy fuel and
//! reached Warmth 0, and a settlement ended without a woodcutter.
//!
//! The first test is cheap and checks the cause: before the fix a steward wanted
//! one log, and only once the hearth was about to go out. The second runs through the
//! first six winter days and is explicit:
//! cargo test --locked -p testbed_content --test island_winter_warmth -- --ignored --nocapture
use bevy_ecs::prelude::*;
use npc_framework::prelude::*;
use std::collections::BTreeMap;
use testbed_content::village::{
    self, Role, VillageState, Villager, Vitality, colony, economy,
    frontier::{labor::ResidentProfile, settlements},
    production,
};

const DAY: u64 = 24_000;
const SAMPLE: u64 = 300;
/// The world starts two days before winter; this is half a day in.
const BEFORE_WINTER_TICKS: u64 = DAY / 2;
/// Winter runs from tick 48000 to 216000. The audit first saw Warmth 0 between
/// ticks 168000 and 180000, so the stretch ends at tick 192000.
const WINTER_STRETCH_TICKS: u64 = 8 * DAY;
/// Consecutive 300-tick samples at Warmth 0 that count as a sustained freeze.
const SUSTAINED_SAMPLES: u32 = 3;

fn residents(world: &World) -> Vec<(SimId, Entity)> {
    world
        .resource::<SimIndex>()
        .map
        .iter()
        .filter(|(_, entity)| world.get::<ResidentProfile>(**entity).is_some())
        .map(|(id, entity)| (*id, *entity))
        .collect()
}

/// Living woodcutters by the settlement whose timber yard they work at.
fn log_producers(world: &World) -> BTreeMap<SimId, u32> {
    let mut yards: BTreeMap<SimId, u32> = settlements::all(world)
        .into_iter()
        .map(|(settlement, _)| (settlement, 0))
        .collect();
    for (_, actor) in residents(world) {
        let person = world.get::<Villager>(actor).expect("resident");
        if person.role != Role::Woodcutter || !village::living(world, actor) {
            continue;
        }
        if let Some(settlement) = world
            .resource::<SimIndex>()
            .get(person.workplace)
            .and_then(|site| settlements::member(world, site))
        {
            *yards.entry(settlement).or_default() += 1;
        }
    }
    yards
}

fn assert_every_yard_is_staffed(world: &World) {
    let now = world.resource::<SimClock>().tick;
    for (settlement, workers) in log_producers(world) {
        assert!(
            workers >= 1,
            "tick {now}: settlement {} has no log producer",
            settlements::definition(world, settlement).map_or("?", |region| region.key.as_str())
        );
    }
}

#[test]
fn hearth_fuel_is_ordered_and_cut_before_winter_arrives() {
    let mut app = testbed_content::new_sim_app(7, "living_world");
    assert_eq!(village::calendar(app.world()).1, village::Season::Autumn);
    while app.world().resource::<SimClock>().tick < BEFORE_WINTER_TICKS {
        run_ticks(app.world_mut(), SAMPLE);
        assert_every_yard_is_staffed(app.world());
    }
    let world = app.world();
    assert_eq!(village::calendar(world).1, village::Season::Autumn);
    // Every home has one resident, its steward, who now plans a fuel reserve.
    let (mut stewards, mut stocked) = (0, 0);
    for (_, actor) in residents(world) {
        let wanted = colony::household_materials(world, actor)
            .into_iter()
            .find(|(kind, _)| *kind == "log")
            .map_or(0, |(_, quantity)| quantity);
        assert!(
            wanted >= 2,
            "{} keeps no winter fuel reserve two days before winter",
            world.get::<Villager>(actor).expect("resident").name
        );
        stewards += 1;
        stocked += u32::from(
            economy::count(world, actor, "log") >= 2
                || production::desired_stock(world, actor, "log") >= 2,
        );
    }
    let cut = world
        .resource::<VillageState>()
        .produced
        .get("log")
        .copied()
        .unwrap_or(0);
    println!(
        "WINTER_FUEL tick={BEFORE_WINTER_TICKS} stewards={stewards} stocked_or_funded={stocked} logs_cut={cut}"
    );
    assert!(
        stocked * 2 >= stewards,
        "only {stocked} of {stewards} stewards hold or have funded their fuel reserve"
    );
}

#[test]
#[ignore = "explicit 192000-tick winter stretch (about 25 minutes)"]
fn every_settlement_keeps_a_log_producer_and_nobody_freezes_through_winter() {
    let mut app = testbed_content::new_sim_app(7, "living_world");
    let people = residents(app.world());
    let mut streak: BTreeMap<SimId, u32> = BTreeMap::new();
    let (mut zero_samples, mut longest, mut minimum) = (0_u64, 0_u32, 100_000);
    while app.world().resource::<SimClock>().tick < WINTER_STRETCH_TICKS {
        run_ticks(app.world_mut(), SAMPLE);
        let world = app.world();
        let now = world.resource::<SimClock>().tick;
        assert_every_yard_is_staffed(world);
        for (id, actor) in &people {
            let name = &world.get::<Villager>(*actor).expect("resident").name;
            let vitality = world.get::<Vitality>(*actor).expect("vitality");
            assert!(
                vitality.died_at.is_none() && vitality.health > 0,
                "tick {now}: {name} died"
            );
            let warmth = economy::need(world, *actor, "Warmth");
            minimum = minimum.min(warmth);
            let run = streak.entry(*id).or_default();
            if warmth == 0 {
                *run += 1;
                zero_samples += 1;
                longest = longest.max(*run);
                assert!(
                    *run < SUSTAINED_SAMPLES,
                    "tick {now}: {name} has been at Warmth 0 for {} ticks",
                    u64::from(*run) * SAMPLE
                );
            } else {
                *run = 0;
            }
        }
        if now.is_multiple_of(DAY) {
            let state = world.resource::<VillageState>();
            println!(
                "WINTER_WARMTH tick={now} season={:?} min_warmth_so_far={minimum} zero_samples={zero_samples} log_price={:?} logs_cut={:?} yards={:?}",
                village::calendar(world).1,
                village::market::unit_price(world, "log"),
                state.produced.get("log"),
                log_producers(world).values().collect::<Vec<_>>(),
            );
        }
    }
    println!(
        "WINTER_WARMTH_DONE ticks={WINTER_STRETCH_TICKS} min_warmth={minimum} zero_samples={zero_samples} longest_zero_streak_samples={longest}"
    );
    village::integrity::validate_colony(app.world()).expect("colony integrity");
}
