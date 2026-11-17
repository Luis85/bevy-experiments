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
//! Whole-day hunt for slow ticks (S6): `PROFILE_SAVE=PATH` profiles a saved
//! island (a capture fixture or a player save) instead of a warmed-up seed,
//! `PROFILE_SKIP=N` runs N ticks before measuring, `PROFILE_TICKS` sets the
//! window (24,000 is one day), `PROFILE_WORST=N` lists the N slowest ticks
//! with their time of day and each probe's share, and `PROFILE_SAVE_COST=1`
//! times one island save (snapshot, JSON, file) at the end.
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

/// One slow tick: its wall time (ns), its tick and each probe's sum (ns).
type SlowTick = (u64, u64, Vec<(&'static str, u64)>);

/// The island of a save, opened as the host's Continue does.
fn open_save(path: &str) -> bevy_app::App {
    use npc_framework::snapshot;
    let saved = snapshot::read(std::path::Path::new(path)).expect("a readable save");
    let doc: npc_framework::agent_io::scenario::ScenarioDoc =
        serde_json::from_value(saved.header["scenario"].clone()).expect("the save's scenario");
    let seed = saved.header["seed"].as_u64().expect("the save's seed");
    let mut loaded = island_content::scenario::load_with(&doc, seed, |_| {}).expect("loads");
    npc_framework::agent_io::scenario::attach_controllers(&doc, &mut loaded).expect("controllers");
    let mut app = loaded.app;
    snapshot::load(app.world_mut(), &saved, snapshot::LoadMode::Strict).expect("restores");
    app
}

#[allow(clippy::disallowed_methods)] // determinism: profiling only; never feeds the sim.
fn save_cost(world: &mut World) -> Value {
    let start = Instant::now();
    let snap = village::island_save::save_island(world).expect("snapshot");
    let snapshot_us = start.elapsed().as_micros();
    let start = Instant::now();
    let bytes = serde_json::to_vec(&snap).expect("json");
    let json_us = start.elapsed().as_micros();
    let start = Instant::now();
    let begun = village::island_save::begin_save(world).expect("begin");
    let begin_us = start.elapsed().as_micros();
    let start = Instant::now();
    let split = begun.finish().expect("finish");
    let finish_us = start.elapsed().as_micros();
    let same = serde_json::to_vec(&split).expect("json") == bytes;
    let start = Instant::now();
    let _ = npc_framework::kernel::hash::state_hash(world);
    let state_hash_us = start.elapsed().as_micros();
    let path = std::env::temp_dir().join("island_tick_profile_save.json");
    let start = Instant::now();
    npc_framework::snapshot::write_atomic(&path, &snap).expect("written");
    let write_us = start.elapsed().as_micros();
    let _ = std::fs::remove_file(&path);
    json!({
        "bytes": bytes.len(),
        "snapshot_us": snapshot_us,
        "json_us": json_us,
        "write_atomic_us (json + sync + rename)": write_us,
        "begin_save_us (on the frame)": begin_us,
        "finish_us (off the frame)": finish_us,
        "state_hash_us": state_hash_us,
        "split_save_equals_save_island": same,
    })
}

#[test]
#[ignore = "profile run (S4): minutes per warm-up day; run with --release"]
fn island_tick_profile() {
    let seed = env("PROFILE_SEED", 11);
    let days = env("PROFILE_DAYS", 7);
    let ticks = env("PROFILE_TICKS", 2_000);
    let worst = usize::try_from(env("PROFILE_WORST", 20)).unwrap_or(20);
    let save = std::env::var("PROFILE_SAVE").ok();
    let mut app = match &save {
        Some(path) => open_save(path),
        None => island_content::replay::try_new_replay_app(seed, "island:living_world", 1)
            .expect("island app"),
    };
    let day = app.world().resource::<SimConfig>().day_length;
    let world = app.world_mut();
    if save.is_none() {
        run_ticks(world, days * day);
    }
    run_ticks(world, env("PROFILE_SKIP", 0));
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
    // (ns, tick, probes of that tick), the slowest `worst` kept.
    let mut slow: Vec<SlowTick> = Vec::new();
    for _ in 0..ticks {
        let at = world.resource::<SimClock>().tick;
        let ns = u64::try_from(timed_tick(world).as_nanos()).unwrap_or(u64::MAX);
        tick_ns.push(ns);
        let parts = probe::end_tick_rows();
        if slow.len() < worst || slow.last().is_some_and(|(least, ..)| ns > *least) {
            slow.push((ns, at, parts));
            slow.sort_by_key(|(ns, at, _)| (std::cmp::Reverse(*ns), *at));
            slow.truncate(worst);
        }
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
    let slow: Vec<Value> = slow
        .into_iter()
        .map(|(ns, at, mut parts)| {
            let of_day = at % day;
            parts.sort_by_key(|(name, ns)| (std::cmp::Reverse(*ns), *name));
            json!({
                "tick": at,
                "time_of_day": format!("{:02}:{:02}", of_day * 24 / day, of_day * 24 * 60 / day % 60),
                "tick_of_day": of_day,
                "ms": format!("{:.1}", Duration::from_nanos(ns).as_secs_f64() * 1000.0),
                "probes_ms": parts.iter().take(12).map(|(name, ns)| format!("{name} {:.1}", Duration::from_nanos(*ns).as_secs_f64() * 1000.0)).collect::<Vec<_>>(),
            })
        })
        .collect();
    let save_cost = (env("PROFILE_SAVE_COST", 0) != 0).then(|| save_cost(world));
    let out = json!({
        "save": save,
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
        "slowest_ticks": slow,
        "save_cost": save_cost,
        "rows": rows,
    });
    let text = serde_json::to_string_pretty(&out).expect("json");
    if let Ok(path) = std::env::var("PROFILE_OUT") {
        std::fs::write(&path, &text).expect("write the profile");
    }
    println!("{text}");
}
