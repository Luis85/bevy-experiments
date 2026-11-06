#![allow(clippy::print_stdout, clippy::print_stderr)] // CLI example / test diagnostics
//! Save-clone benchmark (W1-50, G1.8). Times [`Game::save_snapshot`], the
//! main-thread copy the runtime hands to its storage worker on every save,
//! over `save_clone_fixture(1000 | 2500 | 5000)`: the synthetic stress game
//! (three unsettled expeditions, 150 run records, 1,500 corpses) with its
//! stash sized to that many items. It also breaks the copy down per
//! component and times the storage worker's `serde_json::to_vec`, off the
//! main thread, for scale. Fixture creation is excluded.
//!
//! Gate (docs/PERF_BUDGET.json `save_clone`): p99 at the 1,000-item fixture
//! within `p99_ms_max` in the dev and playtest profiles; release is
//! informational. `--out <path>` writes the JSON report, `--profile-name`
//! labels it (default `dev` with debug assertions, else `release`) and
//! `--iterations` overrides the 1,000 timed iterations.
use horde_core::sim::{Game, save_clone_fixture};
use serde::Serialize;
use std::hint::black_box;
use std::time::Instant;

/// The stash sizes measured; the gate reads the first.
pub const ITEM_COUNTS: [usize; 3] = [1000, 2500, 5000];
/// Untimed iterations before each timed series.
pub const WARMUP: usize = 50;
/// Timed iterations per series.
pub const ITERATIONS: usize = 1000;
const BUDGET: &str = include_str!("../../../docs/PERF_BUDGET.json");

/// Wall-time distribution of one timed series, in milliseconds.
#[derive(Clone, Debug, Serialize)]
pub struct Stats {
    pub samples: usize,
    pub mean_ms: f64,
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
    pub max_ms: f64,
}
/// The snapshot's main parts, each cloned on its own.
#[derive(Clone, Debug, Serialize)]
pub struct Components {
    pub campaign_items: Stats,
    pub run_history: Stats,
    pub attempt: Stats,
    pub battles: Stats,
}
/// One fixture's measurement.
#[derive(Clone, Debug, Serialize)]
pub struct SaveCloneReport {
    pub items: usize,
    pub workers: usize,
    pub run_records: usize,
    pub warmup: usize,
    pub iterations: usize,
    /// `Game::save_snapshot`, the gated main-thread cost.
    pub save_snapshot: Stats,
    pub components: Components,
    /// `serde_json::to_vec` of the snapshot (storage worker, informational).
    pub serialize: Stats,
    pub bytes: usize,
}
#[derive(Debug, Serialize)]
pub struct Gate {
    pub items: usize,
    pub workers: usize,
    pub p99_ms_max: f64,
    pub profiles: Vec<String>,
    pub gated: bool,
    pub p99_ms: f64,
    pub pass: bool,
}
#[derive(Debug, Serialize)]
pub struct Output {
    pub version: u32,
    pub profile: String,
    pub debug_assertions: bool,
    pub cpu: String,
    pub notes: Vec<&'static str>,
    pub gate: Gate,
    pub reports: Vec<SaveCloneReport>,
}

/// Times `work` `iterations` times after `WARMUP.min(iterations)` untimed runs.
fn series<T>(iterations: usize, mut work: impl FnMut() -> T) -> Stats {
    for _ in 0..WARMUP.min(iterations) {
        black_box(work());
    }
    let mut timings: Vec<f64> = (0..iterations)
        .map(|_| {
            let start = Instant::now();
            let value = work();
            let elapsed = start.elapsed();
            // Dropping the copy is the storage worker's cost, not the clone's.
            drop(black_box(value));
            elapsed.as_secs_f64() * 1000.0
        })
        .collect();
    timings.sort_by(f64::total_cmp);
    let n = timings.len();
    let at = |q: usize| timings[(n * q / 100).min(n - 1)];
    Stats {
        samples: n,
        mean_ms: timings.iter().sum::<f64>() / n as f64,
        p50_ms: at(50),
        p95_ms: at(95),
        p99_ms: at(99),
        max_ms: timings[n - 1],
    }
}

/// Measures `fixture`'s save snapshot, its components and its serialization.
pub fn measure(fixture: &Game, iterations: usize) -> SaveCloneReport {
    let iterations = iterations.max(1);
    let campaign = fixture.campaign();
    let snapshot = fixture.save_snapshot();
    SaveCloneReport {
        items: campaign.items.len(),
        workers: fixture.battles().iter().filter(|b| !b.settled).count(),
        run_records: campaign.run_history.len(),
        warmup: WARMUP.min(iterations),
        iterations,
        save_snapshot: series(iterations, || fixture.save_snapshot()),
        components: Components {
            campaign_items: series(iterations, || campaign.items.clone()),
            run_history: series(iterations, || campaign.run_history.clone()),
            attempt: series(iterations, || fixture.attempt().cloned()),
            battles: series(iterations, || fixture.battles().to_vec()),
        },
        serialize: series(iterations, || {
            serde_json::to_vec(&snapshot).expect("serializes")
        }),
        bytes: serde_json::to_vec(&snapshot).expect("serializes").len(),
    }
}

/// The report for `reports` under the budget's `save_clone` gate.
pub fn output(profile: &str, reports: Vec<SaveCloneReport>) -> Result<Output, String> {
    let budget: serde_json::Value = serde_json::from_str(BUDGET).map_err(|e| e.to_string())?;
    let gate = &budget["save_clone"];
    let number = |key: &str| gate[key].as_u64().ok_or(format!("save_clone.{key}"));
    let items = usize::try_from(number("items_min")?).map_err(|e| e.to_string())?;
    let workers = usize::try_from(number("workers")?).map_err(|e| e.to_string())?;
    let p99_ms_max = gate["p99_ms_max"].as_f64().ok_or("save_clone.p99_ms_max")?;
    let profiles: Vec<String> = gate["profiles"]
        .as_array()
        .ok_or("save_clone.profiles")?
        .iter()
        .filter_map(|p| p.as_str().map(str::to_owned))
        .collect();
    let gated_report = reports
        .iter()
        .find(|r| r.items >= items && r.workers >= workers)
        .ok_or(format!(
            "No report with {items}+ items and {workers} workers"
        ))?;
    let p99_ms = gated_report.save_snapshot.p99_ms;
    let gated = profiles.iter().any(|p| p == profile);
    Ok(Output {
        version: 1,
        profile: profile.to_owned(),
        debug_assertions: cfg!(debug_assertions),
        cpu: std::env::var("LASTLIGHT_BENCH_CPU")
            .or_else(|_| std::env::var("PROCESSOR_IDENTIFIER"))
            .unwrap_or_else(|_| "not reported".into()),
        notes: vec![
            "CPU-only: Game::save_snapshot on the main thread, fixture creation excluded.",
            "Synthetic save_clone_fixture: the review-only stress game, never a gameplay claim.",
            "serialize is the storage worker's serde_json::to_vec, off the main thread.",
        ],
        gate: Gate {
            items,
            workers,
            p99_ms_max,
            profiles,
            gated,
            p99_ms,
            pass: p99_ms <= p99_ms_max,
        },
        reports,
    })
}

fn argument(args: &[String], name: &str) -> Option<String> {
    args.windows(2).find(|w| w[0] == name).map(|w| w[1].clone())
}
fn run() -> Result<bool, String> {
    let args: Vec<String> = std::env::args().collect();
    let iterations = argument(&args, "--iterations")
        .map(|n| n.parse::<usize>())
        .transpose()
        .map_err(|e| e.to_string())?
        .unwrap_or(ITERATIONS)
        .clamp(1, 1_000_000);
    let profile = argument(&args, "--profile-name").unwrap_or_else(|| {
        if cfg!(debug_assertions) {
            "dev"
        } else {
            "release"
        }
        .into()
    });
    let mut reports = Vec::new();
    for items in ITEM_COUNTS {
        let fixture = save_clone_fixture(items)?;
        let report = measure(&fixture, iterations);
        eprintln!(
            "{items} items: save_snapshot p99 {:.4} ms (mean {:.4}), to_vec p99 {:.3} ms, {} bytes",
            report.save_snapshot.p99_ms,
            report.save_snapshot.mean_ms,
            report.serialize.p99_ms,
            report.bytes
        );
        reports.push(report);
    }
    let report = output(&profile, reports)?;
    let json = serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?;
    if let Some(path) = argument(&args, "--out") {
        let path = std::path::Path::new(&path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(path, &json).map_err(|e| e.to_string())?;
    }
    println!("{json}");
    Ok(!report.gate.gated || report.gate.pass)
}
fn main() {
    match run() {
        Ok(true) => {}
        Ok(false) => {
            eprintln!("Save clone over budget");
            std::process::exit(1);
        }
        Err(error) => {
            eprintln!("Save-clone benchmark failed: {error}");
            std::process::exit(3);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_clone_fixture_matches_spec() {
        let g = save_clone_fixture(ITEM_COUNTS[0]).expect("save-clone fixture");
        assert!(g.campaign().items.len() >= 1000);
        let workers: Vec<_> = g.battles().iter().filter(|b| !b.settled).collect();
        assert_eq!(workers.len(), 3, "three unsettled expeditions");
        assert!(workers.iter().all(|b| b.game().attempt().is_some()));
        assert_eq!(g.campaign().run_history.len(), 150);
        assert!(g.attempt().is_some(), "the main field is deployed");
    }

    #[test]
    fn save_clone_measure_reports_json() {
        let g = save_clone_fixture(ITEM_COUNTS[0]).expect("save-clone fixture");
        let report = measure(&g, 3);
        assert_eq!(
            (report.items, report.workers, report.run_records),
            (1000, 3, 150)
        );
        assert_eq!(report.bytes, serde_json::to_vec(&g).unwrap().len());
        let dev = output("dev", vec![report]).expect("gate reads the budget");
        assert!(dev.gate.gated);
        let value: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&dev).unwrap()).unwrap();
        assert_eq!(value["debug_assertions"], cfg!(debug_assertions));
        assert_eq!(value["gate"]["p99_ms_max"], 1.0);
        let r = &value["reports"][0];
        for series in [
            &r["save_snapshot"],
            &r["serialize"],
            &r["components"]["campaign_items"],
            &r["components"]["run_history"],
            &r["components"]["attempt"],
            &r["components"]["battles"],
        ] {
            assert_eq!(series["samples"], 3);
            for key in ["mean_ms", "p50_ms", "p95_ms", "p99_ms", "max_ms"] {
                assert!(series[key].as_f64().is_some_and(|ms| ms >= 0.0), "{key}");
            }
            assert!(series["p50_ms"].as_f64() <= series["max_ms"].as_f64());
        }
        assert!(output("release", Vec::new()).is_err(), "no gated fixture");
        let release = output("release", vec![measure(&g, 1)]).unwrap();
        assert!(!release.gate.gated, "release is informational");
    }
}
