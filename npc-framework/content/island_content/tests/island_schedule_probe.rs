//! Day-plan probe of the shipped island (ignored; run in release): how the
//! residents' days go over PROBE_DAYS (default 14) on PROBE_SEEDS (default
//! 7,11,23). Prints one `SCHEDULE_PROBE` line per seed: the share of resident
//! samples spent at physical work (the 14-day audit's own labels), meetings
//! per resident-day, the resident-days that ended without the day's sit-down
//! meal, and inn visits (distinct resident-days with a sample inside an inn).
//! Asserts nothing.
//!
//! ```sh
//! CARGO_INCREMENTAL=0 cargo test --locked --release -p island_content \
//!   --test island_schedule_probe -- --ignored --nocapture
//! ```
use bevy_ecs::prelude::*;
use island_content::village::{
    self, Villager,
    colony::{Chatting, SocialLife},
    indoors,
    resident::DayRhythm,
};
use npc_framework::prelude::*;
use std::collections::BTreeSet;

/// Shorter than the shortest meeting, so no meeting is missed.
const SAMPLE: u64 = 100;

fn env_list(name: &str, default: &[u64]) -> Vec<u64> {
    std::env::var(name)
        .ok()
        .map(|text| {
            text.split(',')
                .filter_map(|part| part.trim().parse().ok())
                .collect()
        })
        .unwrap_or_else(|| default.to_vec())
}

fn residents(world: &World) -> Vec<(SimId, Entity)> {
    let mut out: Vec<(SimId, Entity)> = world
        .resource::<SimIndex>()
        .map
        .iter()
        .filter(|(_, entity)| {
            world
                .get::<Villager>(**entity)
                .is_some_and(|person| person.role != village::Role::Visitor)
                && world.get::<SocialLife>(**entity).is_some()
                && village::living(world, **entity)
        })
        .map(|(id, entity)| (*id, *entity))
        .collect();
    out.sort_unstable_by_key(|(id, _)| *id);
    out
}

fn probe(seed: u64, days: u64) -> String {
    let mut app = island_content::new_sim_app(seed, "living_world");
    let day = app.world().resource::<SimConfig>().day_length;
    let end = days * day;
    let mut samples = 0_u64;
    let mut working = 0_u64;
    let mut meetings = BTreeSet::new();
    let mut inn_days = BTreeSet::new();
    let mut resident_days = 0_u64;
    let mut meals_missed = 0_u64;
    while app.world().resource::<SimClock>().tick < end {
        run_ticks(app.world_mut(), SAMPLE);
        let world = app.world();
        let now = world.resource::<SimClock>().tick;
        let today = (now - 1) / day;
        let people = residents(world);
        for (id, actor) in &people {
            samples += 1;
            working += u64::from(matches!(
                village::activity(world, *actor).0,
                "gathering" | "crafting" | "hunting" | "farming" | "building" | "restoring"
            ));
            if let Some(chat) = world.get::<Chatting>(*actor) {
                meetings.insert((*id, chat.with, chat.until));
            }
            if indoors::inside_entity(world, *actor)
                .is_some_and(|building| indoors::is_inn(world, building))
            {
                inn_days.insert((*id, today));
            }
        }
        // The last sample of a day: who had no sit-down meal today.
        if now.is_multiple_of(day) {
            for (_, actor) in &people {
                resident_days += 1;
                meals_missed += u64::from(
                    world
                        .get::<DayRhythm>(*actor)
                        .is_none_or(|rhythm| rhythm.last_meal != Some(today)),
                );
            }
        }
    }
    let resident_day_samples = samples * SAMPLE / day;
    format!(
        "SCHEDULE_PROBE seed={seed} days={days} residents_end={} work_permille={} \
         work_tenths_of_hours_per_day={} meetings_per_resident_day_x1000={} \
         meals_missed={meals_missed}/{resident_days} ({}‰) inn_visit_days={} \
         inn_visits_per_100_resident_days={} hash={:016x}",
        residents(app.world()).len(),
        working * 1000 / samples.max(1),
        working * 240 / samples.max(1),
        meetings.len() as u64 * 1000 / resident_day_samples.max(1),
        meals_missed * 1000 / resident_days.max(1),
        inn_days.len(),
        inn_days.len() as u64 * 100 / resident_day_samples.max(1),
        npc_framework::kernel::hash::state_hash(app.world_mut()),
    )
}

#[test]
#[ignore = "day-plan probe over PROBE_DAYS (default 14) on PROBE_SEEDS (default 7,11,23); run in release"]
fn schedule_probe_how_the_days_go() {
    let days = std::env::var("PROBE_DAYS")
        .ok()
        .and_then(|text| text.parse().ok())
        .unwrap_or(14);
    let runs: Vec<_> = env_list("PROBE_SEEDS", &[7, 11, 23])
        .into_iter()
        .map(|seed| std::thread::spawn(move || probe(seed, days)))
        .collect();
    for run in runs {
        println!("{}", run.join().expect("probe"));
    }
}
