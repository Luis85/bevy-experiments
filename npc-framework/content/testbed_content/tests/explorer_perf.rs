//! The `perception` mechanic's cost in a host-built explorer world, with
//! and without fog of war (plan Task 9 review), and with a rover (LIDAR +
//! camera) or a sentinel (radar + camera) as the actor (plan Task 12). Ignored by default; run in
//! release:
//! `cargo test --release -p testbed_content --test explorer_perf -- --ignored --nocapture`.
//!
//! Like `npcctl bench`: the perf layer is the global subscriber before any
//! app exists, 300 warm-up ticks, then 3000 measured ticks with the core
//! budgets; median of the perception p95 / p50 over interleaved rounds.

// determinism: wall-clock profiling of the test harness only (µs as `f64`,
// the collector's statistics); never sim state.
#![allow(
    clippy::disallowed_methods,
    clippy::disallowed_types,
    clippy::float_arithmetic
)]

use std::path::PathBuf;

use bevy_app::App;
use npc_framework::perception::{ExploredCells, RevealsFog, SeenStatics, VisibleCells};
use npc_framework::perf::{MechanicTags, PerfBudgets, PerfCollector, install_global};
use npc_framework::prelude::{SimIndex, SimPos, run_ticks};
use testbed_content::TestbedIds;
use testbed_content::explorer::host::{HostArgs, build_world};

const WARMUP: u64 = 300;
const TICKS: u64 = 3000;
const ROUNDS: usize = 5;

fn home(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("npc-explorer-perf-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// A New explorer world (as the windowed and detached hosts build it), its
/// actor (built from `preset`, the human by default) revealing fog or not.
fn explorer(fog: bool, preset: Option<&str>, tag: &str) -> (App, PathBuf) {
    let home = home(tag);
    let args = HostArgs {
        home: home.clone(),
        agent: "Wren".into(),
        world: Some("W".into()),
        resume: false,
        on_mismatch: None,
        preset: preset.map(str::to_owned),
        actors_dir: Some(home.join("actors")),
    };
    let (mut app, _, _) = build_world(&args, 0x5eed, "2026-10-25T12:00:00Z", |_| {}).unwrap();
    if !fog {
        let world = app.world_mut();
        let id = world.resource::<TestbedIds>().id("Wren");
        let e = world.resource::<SimIndex>().get(id).unwrap();
        world
            .entity_mut(e)
            .remove::<(RevealsFog, ExploredCells, VisibleCells, SeenStatics)>();
    }
    (app, home)
}

/// Walks the actor (a teleport per tick, 70 mm: walking speed) back and
/// forth across the yard when `walk`, else it stands (an agent between
/// turns).
fn step(app: &mut App, walk: bool, t: u64) {
    if walk {
        let world = app.world_mut();
        let id = world.resource::<TestbedIds>().id("Wren");
        let e = world.resource::<SimIndex>().get(id).unwrap();
        let phase = i32::try_from(t % 300).unwrap();
        let x = if phase < 150 { phase } else { 300 - phase } * 70 - 5000;
        *world.get_mut::<SimPos>(e).unwrap() = SimPos::new(x, -6500);
    }
    run_ticks(app.world_mut(), 1);
}

/// `(p50, p95, budget)` in µs of each of [`MECHANICS`] (zeros when it did
/// not run).
fn measure(
    collector: &PerfCollector,
    (fog, walk, preset): (bool, bool, Option<&str>),
    tag: &str,
) -> Vec<(f64, f64, u64)> {
    let (mut app, home) = explorer(fog, preset, tag);
    collector.set_budgets(PerfBudgets::default());
    for t in 0..WARMUP {
        step(&mut app, walk, t);
        collector.end_frame();
    }
    collector.reset();
    collector.take_breaches();
    collector.set_budgets(PerfBudgets::default_npc_for_world(app.world_mut(), 1));
    for t in WARMUP..WARMUP + TICKS {
        step(&mut app, walk, t);
        collector.end_frame();
    }
    let report = collector.report();
    let out = MECHANICS
        .iter()
        .map(|name| {
            report
                .mechanics
                .iter()
                .find(|m| m.mechanic == *name)
                .map_or((0.0, 0.0, 0), |m| {
                    (m.p50_us, m.p95_us, m.budget_us.unwrap_or(0))
                })
        })
        .collect();
    drop(app);
    let _ = std::fs::remove_dir_all(&home);
    out
}

/// The mechanics reported: perception and the fog of war.
const MECHANICS: [&str; 2] = ["perception", "fog"];

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

#[test]
#[ignore = "perf numbers; run in release with --nocapture"]
fn explorer_perception_with_and_without_fog() {
    let collector = PerfCollector::new(TICKS as usize);
    let mut tags = MechanicTags::default_npc();
    testbed_content::perf_tags(&mut tags);
    collector.set_tags(tags);
    assert!(
        install_global(&collector),
        "a global subscriber was already set"
    );
    let cases = [
        ("idle, no fog", false, false, None),
        ("idle, fog", true, false, None),
        ("walking, no fog", false, true, None),
        ("walking, fog", true, true, None),
        ("rover idle, fog", true, false, Some("rover")),
        ("rover walking, fog", true, true, Some("rover")),
        ("sentinel walking, fog", true, true, Some("sentinel")),
    ];
    // [case][mechanic] -> (p50s, p95s, budget)
    let mut samples = vec![vec![(Vec::new(), Vec::new(), 0u64); MECHANICS.len()]; cases.len()];
    for round in 0..ROUNDS {
        for (i, (name, fog, walk, preset)) in cases.iter().enumerate() {
            let got = measure(&collector, (*fog, *walk, *preset), &format!("{i}-{round}"));
            for (m, (p50, p95, budget)) in got.into_iter().enumerate() {
                println!(
                    "round {round}, {name}, {}: p50 {p50:.1} µs, p95 {p95:.1} µs",
                    MECHANICS[m]
                );
                samples[i][m].0.push(p50);
                samples[i][m].1.push(p95);
                samples[i][m].2 = budget;
            }
        }
    }
    for ((name, ..), per) in cases.iter().zip(samples) {
        for (m, (p50, p95, budget)) in per.into_iter().enumerate() {
            println!(
                "{}, {name}: median p50 {:.1} µs, median p95 {:.1} µs (budget {budget})",
                MECHANICS[m],
                median(p50),
                median(p95)
            );
        }
    }
}
