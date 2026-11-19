//! Day-by-day trace of one shipped year (ignored; run in release), for
//! comparing two builds: per day the census (residents, couples, births),
//! civic buildings completed, bread and eggs made, bread imported, trades
//! by role and the meetings of the day. Prints one `TRACE` line a day.
//! `TRACE_ECONOMY_PATCH`: a JSON object whose `live` keys replace those of
//! `economy.live`, for a perturbation run of the same build;
//! `TRACE_SCHEDULE_PATCH` likewise sets keys of `behavior.schedule`.
//!
//! ```sh
//! TRACE_SEED=7 CARGO_INCREMENTAL=0 cargo test --locked --release -p island_content \
//!   --test island_year_trace -- --ignored --nocapture
//! ```
mod life_support;

use island_content::village::{
    self, VillageState, Villager, civic, colony::SocialLife, merchant, view,
};
use life_support::*;
use npc_framework::prelude::*;
use std::collections::BTreeMap;

const DAY: u64 = 24_000;

#[test]
#[ignore = "one-year day-by-day trace on TRACE_SEED (default 7); run in release"]
fn year_trace() {
    let seed = std::env::var("TRACE_SEED")
        .ok()
        .and_then(|text| text.parse().ok())
        .unwrap_or(7);
    let days = std::env::var("TRACE_DAYS")
        .ok()
        .and_then(|text| text.parse().ok())
        .unwrap_or(28);
    let mut app = world_for(seed, |world| {
        if let Ok(text) = std::env::var("TRACE_ECONOMY_PATCH") {
            let patch: serde_json::Value = serde_json::from_str(&text).expect("patch JSON");
            println!("TRACE_PATCH {patch}");
            for (key, value) in patch["live"].as_object().expect("live keys") {
                economy(world)["live"][key] = value.clone();
            }
        }
        // `TRACE_SCHEDULE_PATCH`: keys of `behavior.schedule` (the rest of
        // the behaviour block keeps its defaults).
        if let Ok(text) = std::env::var("TRACE_SCHEDULE_PATCH") {
            let patch: serde_json::Value = serde_json::from_str(&text).expect("patch JSON");
            println!("TRACE_SCHEDULE_PATCH {patch}");
            world["living_world"]["behavior"]["schedule"] = patch;
        }
    });
    // Hen keepers' time by running leaf label, sampled every 100 ticks.
    let mut keeper_time: BTreeMap<String, u32> = BTreeMap::new();
    for day in 1..=days {
        for _ in 0..DAY / 100 {
            run_ticks(app.world_mut(), 100);
            let world = app.world();
            for (_, actor) in all_residents(world) {
                if world
                    .get::<Villager>(actor)
                    .is_some_and(|person| format!("{:?}", person.role).contains("herder"))
                    && village::living(world, actor)
                {
                    let label = if world.get::<village::colony::Chatting>(actor).is_some() {
                        "chatting".to_owned()
                    } else {
                        village::resident::label(world, actor)
                            .unwrap_or("none")
                            .to_owned()
                    };
                    *keeper_time.entry(label).or_default() += 1;
                }
            }
        }
        let world = app.world();
        let census = view::census(world);
        let civic = world.resource::<civic::CivicState>();
        let state = world.resource::<VillageState>();
        let boat = world.resource::<merchant::Merchant>();
        let made = |kind: &str| state.produced.get(kind).copied().unwrap_or(0);
        let mut roles: BTreeMap<String, u32> = BTreeMap::new();
        let mut fond = 0_u32;
        for (_, actor) in all_residents(world) {
            if !village::living(world, actor) {
                continue;
            }
            if let Some(person) = world.get::<Villager>(actor) {
                *roles.entry(format!("{:?}", person.role)).or_default() += 1;
            }
            fond += world.get::<SocialLife>(actor).map_or(0, |social| {
                social
                    .affinity
                    .values()
                    .filter(|score| **score >= 12_000)
                    .count() as u32
            });
        }
        println!(
            "TRACE seed={seed} day={day} residents={} couples={} births={} built={} bread_made={} \
             bread_imported={} eggs_made={} fond_pairs={} social_events={} pens={:?} roles={:?}",
            census.residents,
            census.couples,
            census.births,
            civic.completed,
            made("bread"),
            boat.imported.get("bread").copied().unwrap_or(0),
            made("village_egg"),
            fond,
            world
                .resource::<village::colony::ColonyState>()
                .social_events,
            village::livestock::views(world)
                .iter()
                .map(|pen| (pen.head, pen.born, pen.died, pen.hungry_days))
                .collect::<Vec<_>>(),
            roles,
        );
    }
    println!("TRACE_KEEPERS seed={seed} {keeper_time:?}");
}
