//! The `npcctl bench` harness: ticks per second and per-mechanic stats per
//! scenario size, and the comparison against a baseline file. Generic over
//! the content's app factory.

use std::path::Path;
use std::time::Instant;

use npc_kernel::replay::AppFactory;
use npc_kernel::run_ticks;
use npc_perf::{MechanicStats, PerfBudgets, PerfCollector, SystemStats};
use serde::{Deserialize, Serialize};

use crate::output::{Failure, read_json};

/// The whole `bench` output (also the baseline file format).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BenchOutput {
    /// `git rev-parse --short HEAD`, when available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_rev: Option<String>,
    /// `"release"` or `"debug"` (debug assertions on).
    #[serde(default)]
    pub profile: String,
    #[serde(default)]
    pub seed: u64,
    /// Ticks run (and discarded) before measuring, per village size.
    #[serde(default)]
    pub warmup: u64,
    pub results: Vec<BenchResult>,
    /// Only with `--baseline`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comparison: Option<Comparison>,
}

/// One village size.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BenchResult {
    pub villagers: u32,
    /// Measured ticks.
    pub ticks: u64,
    /// Wall time of the measured ticks (profiler on).
    pub wall_ms: f64,
    pub ticks_per_sec: f64,
    /// `wall_ms / ticks`.
    #[serde(default)]
    pub ms_per_tick: f64,
    /// Budget breaches recorded while measuring (budgets:
    /// `PerfBudgets::default_npc_for_world(app, villagers)`: the core table
    /// plus the rows the content registered; rate-limited to one per
    /// mechanic per 5 s).
    #[serde(default)]
    pub budget_breaches: usize,
    /// Every mechanic, by whole-run total time descending. One sample per tick.
    pub mechanics: Vec<MechanicStats>,
    /// The top systems by whole-run total time.
    pub systems: Vec<SystemStats>,
}

/// Result of comparing against `--baseline`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Comparison {
    pub baseline: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub baseline_git_rev: Option<String>,
    pub baseline_profile: String,
    /// Statistic compared (`p95` by default).
    pub metric: Metric,
    /// A mechanic regresses when `value > baseline * (1 + threshold_pct/100)`
    /// and `value - baseline >= min_us`, `value` being `metric` in µs.
    pub threshold_pct: f64,
    pub min_us: f64,
    /// Mechanics with fewer samples than this (in either run) are skipped:
    /// their percentiles are a handful of ticks.
    pub min_samples: u64,
    pub regressions: Vec<Regression>,
    /// Throughput per village size present in both runs (informational).
    pub throughput: Vec<Throughput>,
    /// Village sizes of this run with no baseline result.
    pub unmatched_villagers: Vec<u32>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Regression {
    pub villagers: u32,
    pub mechanic: String,
    /// The compared statistic of the baseline, µs.
    pub baseline_us: f64,
    /// The compared statistic of this run, µs.
    pub value_us: f64,
    /// `(value / baseline - 1) * 100`.
    pub change_pct: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Throughput {
    pub villagers: u32,
    pub baseline_ticks_per_sec: f64,
    pub ticks_per_sec: f64,
    pub change_pct: f64,
}

/// Which per-mechanic statistic `--baseline` compares.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum Metric {
    /// Rolling p95 per tick (spec default). Unstable for mechanics that run
    /// expensively on only a few percent of ticks (e.g. `think` with one
    /// villager re-thinking every 40 ticks).
    P95,
    /// Rolling median per tick.
    P50,
    /// Whole-run mean per sample (`total_us / samples`). Smooth on an idle
    /// machine, but one preempted tick moves it, so it is noisier than p95
    /// on a busy one.
    Mean,
}

impl Metric {
    fn of(self, m: &MechanicStats) -> f64 {
        match self {
            Metric::P95 => m.p95_us,
            Metric::P50 => m.p50_us,
            Metric::Mean if m.samples > 0 => m.total_us / m.samples as f64,
            Metric::Mean => 0.0,
        }
    }
}

pub struct BenchArgs<'a> {
    pub collector: &'a PerfCollector,
    /// Builds each measured app (`factory(seed, scenario, n)`).
    pub factory: AppFactory,
    /// Scenario that scales with `villagers` (e.g. `"village_n"`).
    pub scenario: &'a str,
    pub villagers: &'a [u32],
    pub ticks: u64,
    pub warmup: u64,
    pub seed: u64,
    pub top: usize,
}

/// Runs every village size. The collector must already be the global
/// subscriber's layer (installed before any app is built).
pub fn run(args: &BenchArgs) -> Result<BenchOutput, Failure> {
    let collector = args.collector;
    let mut results = Vec::new();
    for &n in args.villagers {
        tracing::info!(
            "bench: {n} villagers, {} warmup + {} ticks",
            args.warmup,
            args.ticks
        );
        let mut app = (args.factory)(args.seed, args.scenario, n).map_err(Failure::usage)?;
        // No budgets while warming up: the first ticks pay one-off schedule
        // initialisation and would only produce spurious breach warnings.
        collector.set_budgets(PerfBudgets::default());
        let world = app.world_mut();
        for _ in 0..args.warmup {
            run_ticks(world, 1);
            collector.end_frame();
        }
        collector.reset();
        collector.take_breaches();
        collector.set_budgets(PerfBudgets::default_npc_for_world(world, n));
        let start = Instant::now();
        for _ in 0..args.ticks {
            run_ticks(world, 1);
            collector.end_frame();
        }
        let wall = start.elapsed().as_secs_f64();
        let report = collector.report();
        let wall_ms = wall * 1000.0;
        results.push(BenchResult {
            villagers: n,
            ticks: args.ticks,
            wall_ms: round3(wall_ms),
            ticks_per_sec: round3(if wall > 0.0 {
                args.ticks as f64 / wall
            } else {
                0.0
            }),
            ms_per_tick: round3(if args.ticks > 0 {
                wall_ms / args.ticks as f64
            } else {
                0.0
            }),
            budget_breaches: collector.take_breaches().len(),
            mechanics: report.mechanics.clone(),
            systems: report.top_systems(args.top).to_vec(),
        });
        drop(app);
    }
    Ok(BenchOutput {
        git_rev: git_rev(),
        profile: profile().to_owned(),
        seed: args.seed,
        warmup: args.warmup,
        results,
        comparison: None,
    })
}

fn round3(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

pub fn profile() -> &'static str {
    if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    }
}

fn git_rev() -> Option<String> {
    let out = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    let rev = String::from_utf8(out.stdout).ok()?.trim().to_owned();
    (out.status.success() && !rev.is_empty()).then_some(rev)
}

/// Compares `current` against the baseline at `path`.
pub fn compare(
    current: &BenchOutput,
    path: &Path,
    metric: Metric,
    threshold_pct: f64,
    min_us: f64,
    min_samples: u64,
) -> Result<Comparison, Failure> {
    let base: BenchOutput = read_json(path, "baseline")?;
    if !base.profile.is_empty() && base.profile != current.profile {
        tracing::warn!(
            "baseline was measured with the {} profile, this run is {}: expect noise",
            base.profile,
            current.profile
        );
    }
    let mut regressions = Vec::new();
    let mut throughput = Vec::new();
    let mut unmatched = Vec::new();
    for r in &current.results {
        let Some(b) = base.results.iter().find(|b| b.villagers == r.villagers) else {
            unmatched.push(r.villagers);
            continue;
        };
        throughput.push(Throughput {
            villagers: r.villagers,
            baseline_ticks_per_sec: b.ticks_per_sec,
            ticks_per_sec: r.ticks_per_sec,
            change_pct: round3(pct(r.ticks_per_sec, b.ticks_per_sec)),
        });
        for m in &r.mechanics {
            let Some(bm) = b.mechanics.iter().find(|bm| bm.mechanic == m.mechanic) else {
                continue;
            };
            if m.samples < min_samples || bm.samples < min_samples {
                continue;
            }
            let (base_us, value_us) = (metric.of(bm), metric.of(m));
            let limit = base_us * (1.0 + threshold_pct / 100.0);
            if value_us > limit && value_us - base_us >= min_us {
                regressions.push(Regression {
                    villagers: r.villagers,
                    mechanic: m.mechanic.clone(),
                    baseline_us: round3(base_us),
                    value_us: round3(value_us),
                    change_pct: round3(pct(value_us, base_us)),
                });
            }
        }
    }
    Ok(Comparison {
        baseline: path.display().to_string(),
        baseline_git_rev: base.git_rev,
        baseline_profile: base.profile,
        metric,
        threshold_pct,
        min_us,
        min_samples,
        regressions,
        throughput,
        unmatched_villagers: unmatched,
    })
}

fn pct(now: f64, base: f64) -> f64 {
    if base > 0.0 {
        (now / base - 1.0) * 100.0
    } else {
        0.0
    }
}
