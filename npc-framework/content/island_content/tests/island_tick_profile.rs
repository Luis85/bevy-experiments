//! Headless tick profile of the shipped island (S4 measurement, ignored).
//!
//! `npcctl bench` sees only `tracing` spans and this crate has none, so the
//! island's own probes (`village::probe`: supplier, seller and route lookups,
//! leaf starts by action, the parts of the island tick) are read here. The
//! world is the bench's (`try_new_replay_app` of `island:living_world`).
//!
//! ```sh
//! PROFILE_SEED=11 PROFILE_DAYS=28 PROFILE_OUT=target/npc-out/s4/profile.json \
//!   cargo test --locked --release -p island_content --test island_tick_profile -- --ignored --nocapture
//! ```
//!
//! Wall time is read only to report it; it never feeds the sim.
use bevy_ecs::prelude::*;
use island_content::village::{self, Villager, probe};
use npc_framework::prelude::*;
use serde_json::{Value, json};
use std::time::{Duration, Instant};

fn env(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn percentile(sorted: &[u64], permille: usize) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    sorted[(sorted.len() * permille / 1000).min(sorted.len() - 1)]
}

#[allow(clippy::disallowed_methods)] // determinism: profiling only; never feeds the sim.
fn timed_tick(world: &mut World) -> Duration {
    let start = Instant::now();
    run_ticks(world, 1);
    start.elapsed()
}

#[test]
#[ignore = "profile run (S4): minutes per warm-up day; run with --release"]
fn island_tick_profile() {
    let seed = env("PROFILE_SEED", 11);
    let days = env("PROFILE_DAYS", 7);
    let ticks = env("PROFILE_TICKS", 2_000);
    let mut app = island_content::replay::try_new_replay_app(seed, "island:living_world", 1)
        .expect("island app");
    let day = app.world().resource::<SimConfig>().day_length;
    let world = app.world_mut();
    run_ticks(world, days * day);
    let residents = world
        .query::<(Entity, &Villager)>()
        .iter(world)
        .filter(|(e, person)| person.role != village::Role::Visitor && village::living(world, *e))
        .count();
    probe::enable();
    let mut tick_ns = Vec::new();
    let revision = |world: &World| world.resource::<SpatialGrid>().revision();
    let mut last = revision(world);
    let mut revisions = 0_u64;
    for _ in 0..ticks {
        tick_ns.push(u64::try_from(timed_tick(world).as_nanos()).unwrap_or(u64::MAX));
        probe::end_tick();
        let now = revision(world);
        revisions += u64::from(now != last);
        last = now;
    }
    let total: u64 = tick_ns.iter().sum();
    let mut sorted = tick_ns.clone();
    sorted.sort_unstable();
    let ms = |ns: u64| ns / 1_000; // nanoseconds to µs
    let mut rows: Vec<(&str, probe::Row)> = probe::table().into_iter().collect();
    rows.sort_by_key(|(name, row)| (std::cmp::Reverse(row.total_ns), *name));
    let starts: u64 = rows
        .iter()
        .filter(|(name, _)| name.starts_with("start."))
        .map(|(_, row)| row.total_ns)
        .sum();
    let rows: Vec<Value> = rows
        .into_iter()
        .map(|(name, row)| {
            let mut per_tick = row.ticks.clone();
            per_tick.sort_unstable();
            json!({
                "probe": name,
                "calls": row.calls,
                "samples": per_tick.len(),
                "total_us": ms(row.total_ns),
                "permille_of_tick": row.total_ns * 1000 / total.max(1),
                "permille_of_starts": row.total_ns * 1000 / starts.max(1),
                "p95_us": ms(percentile(&per_tick, 950)),
                "max_us": ms(per_tick.last().copied().unwrap_or(0)),
                "per_call_ns": row.total_ns / row.calls.max(1),
            })
        })
        .collect();
    let out = json!({
        "seed": seed,
        "warmup_days": days,
        "residents": residents,
        "hash": format!("{:016x}", npc_framework::kernel::hash::state_hash(world)),
        "ticks": ticks,
        "ticks_per_sec_x1000": u64::try_from(u128::from(ticks) * 1_000_000_000_000 / u128::from(total.max(1))).unwrap_or(0),
        "mean_us_per_tick": ms(total / ticks.max(1)),
        "tick_p50_us": ms(percentile(&sorted, 500)),
        "tick_p95_us": ms(percentile(&sorted, 950)),
        "tick_p99_us": ms(percentile(&sorted, 990)),
        "tick_max_us": ms(sorted.last().copied().unwrap_or(0)),
        "leaf_starts_us": ms(starts),
        "ticks_with_grid_changes": revisions,
        "grid_cells": world.resource::<SpatialGrid>().bounds().len(),
        "rows": rows,
    });
    let text = serde_json::to_string_pretty(&out).expect("json");
    if let Ok(path) = std::env::var("PROFILE_OUT") {
        std::fs::write(&path, &text).expect("write the profile");
    }
    println!("{text}");
}
