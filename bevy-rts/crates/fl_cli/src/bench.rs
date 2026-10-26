//! `bench`: tier-3 wall-clock benches of the sim at scale (spec §12.6, §15; ADR-0010).
//!
//! Informational, not a gate (the gates are work counters). Each size runs `repeat` times,
//! interleaved across sizes, and the run with the median mean tick time is reported. The
//! per-phase, per-feature and per-system breakdown comes from `fl_observe`'s profiler, which is
//! process-global, so runs are serialized.
//!
//! The scenario is the [`ScaleScenario`] entity mix on the base mod (followers and projectiles
//! besides the units): on the 2 km terrain map with pathing ([`ScaleScenario::on_terrain`]) by
//! default, or with `flat` on M0's flat, open map ([`ScaleScenario::with_units`], the workload
//! `docs/perf/M0.json` measured). [`populate`] is the one place that builds it. Rows report
//! `units`, the total sim `entities` and the `map` (`"base.scale_2km"` or `"open"`).

use std::fmt::Write as _;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use bevy_app::App;
use fl_content::ContentDb;
use fl_observe::{DebugConfig, PerfSnapshot, Stat, WINDOW, init_headless};
use fl_protocol::Tick;
use fl_sim::{
    MatchSetup, RunConfig, ScaleScenario, SimCounters, SimIndex, build_sim_app, load_content,
    step_tick,
};
use fl_view::TickInput;
use serde::{Deserialize, Serialize};

use crate::report::{CliError, Report};

/// A row's `map` on the flat, open map.
const OPEN_MAP: &str = "open";

/// What to run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BenchConfig {
    /// Unit counts, one bench per entry.
    pub units: Vec<u32>,
    /// Measured ticks per run.
    pub ticks: u32,
    /// Ticks run first and not measured.
    pub warmup: u32,
    /// The match seed (and the scale scenario's).
    pub seed: u64,
    /// Runs per size; the median by `mean_ms` is reported (the upper middle one for an even
    /// count). 0 counts as 1.
    pub repeat: u32,
    /// Run on M0's flat, open map ([`ScaleScenario::with_units`]) instead of the 2 km terrain
    /// map ([`ScaleScenario::on_terrain`]).
    pub flat: bool,
}

impl BenchConfig {
    /// The runs per size actually made: `repeat`, at least 1.
    #[must_use]
    pub fn runs(&self) -> u32 {
        self.repeat.max(1)
    }

    /// The ticks the profiler's statistics cover: its rolling window of [`WINDOW`] ticks, or
    /// fewer if fewer ticks are measured. The per-phase, per-feature and per-system means are
    /// over these ticks; the headline mean, p95 and max are over every measured tick.
    #[must_use]
    pub fn profiler_window_ticks(&self) -> u32 {
        self.ticks.min(u32::try_from(WINDOW).unwrap_or(u32::MAX))
    }
}

impl Default for BenchConfig {
    fn default() -> Self {
        BenchConfig {
            units: vec![500, 3000, 5000],
            ticks: 400,
            warmup: 40,
            seed: 1,
            repeat: 5,
            flat: false,
        }
    }
}

/// The result for one size.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BenchRow {
    pub units: u32,
    /// The map the units ran on: `"base.scale_2km"`, or [`OPEN_MAP`] for the flat, open map
    /// (also what a row without one, written before maps had terrain, ran on).
    #[serde(default = "open_map")]
    pub map: String,
    /// Live sim entities at the end of the run.
    pub entities: usize,
    /// Wall-clock milliseconds per tick over the measured ticks.
    pub mean_ms: f64,
    pub p95_ms: f64,
    pub max_ms: f64,
    /// Mean milliseconds per phase, in phase order, over the profiler window
    /// ([`BenchConfig::profiler_window_ticks`]: the last 240 measured ticks at most).
    pub per_phase_mean_ms: Vec<(String, f64)>,
    /// Mean of each work counter per tick over the measured ticks, sorted by name.
    pub counters_per_tick: Vec<(String, f64)>,
}

fn open_map() -> String {
    OPEN_MAP.to_owned()
}

/// The profiler is process-global: one bench (or profiled test) at a time.
static EXCLUSIVE: Mutex<()> = Mutex::new(());

/// Hold this while using the global profiler, so that other users in the process do not mix
/// their ticks into the statistics. [`run_bench_full`] takes it itself.
pub fn exclusive() -> MutexGuard<'static, ()> {
    EXCLUSIVE.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The median of `xs` (the mean of the middle two for an even count); 0 if empty.
#[must_use]
pub fn median(xs: &[f64]) -> f64 {
    let mut sorted = xs.to_vec();
    sorted.sort_by(f64::total_cmp);
    match sorted.len() {
        0 => 0.0,
        n if n % 2 == 1 => sorted[n / 2],
        n => f64::midpoint(sorted[n / 2 - 1], sorted[n / 2]),
    }
}

/// Nearest-rank percentile of an ascending slice.
#[expect(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "bench statistics"
)]
fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let rank = (p * sorted.len() as f64).ceil() as usize;
    sorted[rank.clamp(1, sorted.len()) - 1]
}

/// Milliseconds `f` takes on the wall clock.
#[expect(clippy::disallowed_types, reason = "bench wall clock")]
fn time_ms(f: impl FnOnce()) -> f64 {
    use std::time::Instant;
    let started = Instant::now();
    f();
    started.elapsed().as_secs_f64() * 1000.0
}

/// The scenario of the bench: the entity mix (spec §5.7) scaled to `units` units, on the
/// 2 km terrain map or, `flat`, on the flat, open map. Returns the row's `map`.
fn populate(app: &mut App, units: u32, seed: u64, flat: bool) -> Result<String, CliError> {
    let mix = if flat {
        ScaleScenario::with_units(units)
    } else {
        ScaleScenario::on_terrain(units)
    };
    let scale = ScaleScenario { seed, ..mix };
    scale
        .populate(app.world_mut())
        .map_err(|e| CliError::Setup(e.to_string()))?;
    Ok(scale.map.unwrap_or_else(open_map))
}

#[expect(clippy::cast_precision_loss, reason = "counts are far below 2^52")]
fn run_once(
    cfg: &BenchConfig,
    content: &Arc<ContentDb>,
    units: u32,
) -> Result<(BenchRow, PerfSnapshot), CliError> {
    let profiler = init_headless(
        &DebugConfig {
            log_filter: "error".to_owned(),
            json_log: false,
            journal_file: false,
            profiler: true,
            ..DebugConfig::default()
        },
        None,
    );
    let run = RunConfig {
        phase_markers: true,
        ..RunConfig::default()
    };
    let setup = MatchSetup {
        seed: cfg.seed,
        cheats_allowed: false,
    };
    let mut app = build_sim_app(&fl_catalog::features(), Arc::clone(content), setup, run)
        .map_err(|e| CliError::Build(e.to_string()))?;
    let map = populate(&mut app, units, cfg.seed, cfg.flat)?;

    let mut tick = 0;
    let mut step = |app: &mut App| {
        let input = TickInput {
            tick: Tick(tick),
            commands: Vec::new(),
        };
        tick += 1;
        step_tick(app.world_mut(), input);
    };
    for _ in 0..cfg.warmup {
        step(&mut app);
    }
    profiler.reset();
    let mut times = Vec::with_capacity(cfg.ticks as usize);
    let mut counters: Vec<(String, f64)> = Vec::new();
    for _ in 0..cfg.ticks {
        times.push(time_ms(|| step(&mut app)));
        for (name, n) in app.world().resource::<SimCounters>().snapshot() {
            match counters.iter_mut().find(|(k, _)| k == name) {
                Some((_, sum)) => *sum += n as f64,
                None => counters.push((name.to_owned(), n as f64)),
            }
        }
    }
    let perf = profiler.snapshot();
    counters.sort_by(|a, b| a.0.cmp(&b.0));
    let denom = f64::from(cfg.ticks.max(1));
    for (_, sum) in &mut counters {
        *sum /= denom;
    }
    let mean_ms = times.iter().sum::<f64>() / denom;
    times.sort_by(f64::total_cmp);
    let row = BenchRow {
        units,
        map,
        entities: app.world().resource::<SimIndex>().len(),
        mean_ms,
        p95_ms: percentile(&times, 0.95),
        max_ms: times.last().copied().unwrap_or(0.0),
        per_phase_mean_ms: perf
            .phases
            .iter()
            .map(|(name, stat)| (name.clone(), stat.mean_ms))
            .collect(),
        counters_per_tick: counters,
    };
    Ok((row, perf))
}

/// Run every size [`BenchConfig::runs`] times, interleaved (all sizes, then again), and keep
/// for each size the run with the median `mean_ms` (the upper middle one for an even count),
/// with its profiler snapshot.
///
/// # Errors
/// If the base content or a scenario cannot be built.
pub fn run_bench_full(cfg: &BenchConfig) -> Result<Vec<(BenchRow, PerfSnapshot)>, CliError> {
    let _guard = exclusive();
    let content = load_content(&fl_catalog::features(), &[fl_catalog::base_mod_dir()])
        .map_err(|e| CliError::Build(e.to_string()))?;
    let mut runs: Vec<Vec<(BenchRow, PerfSnapshot)>> =
        cfg.units.iter().map(|_| Vec::new()).collect();
    for _ in 0..cfg.runs() {
        for (slot, &units) in runs.iter_mut().zip(&cfg.units) {
            slot.push(run_once(cfg, &content, units)?);
        }
    }
    Ok(runs
        .into_iter()
        .map(|mut slot| {
            slot.sort_by(|a, b| a.0.mean_ms.total_cmp(&b.0.mean_ms));
            let mid = slot.len() / 2;
            slot.swap_remove(mid)
        })
        .collect())
}

/// [`run_bench_full`] without the profiler snapshots.
///
/// # Panics
/// If the base content or a scenario cannot be built.
#[must_use]
pub fn run_bench(cfg: &BenchConfig) -> Vec<BenchRow> {
    run_bench_full(cfg)
        .unwrap_or_else(|err| panic!("bench failed: {err}"))
        .into_iter()
        .map(|(row, _)| row)
        .collect()
}

/// A profiler statistic: its window statistics, `samples` since the measuring started and
/// `window_samples`, the samples the statistics cover (at most [`WINDOW`]).
fn stat_json(stat: &Stat) -> serde_json::Value {
    let window = stat.samples.min(u64::try_from(WINDOW).unwrap_or(u64::MAX));
    serde_json::json!({
        "mean_ms": stat.mean_ms, "p50_ms": stat.p50_ms, "p95_ms": stat.p95_ms,
        "max_ms": stat.max_ms, "samples": stat.samples, "window_samples": window,
    })
}

fn snapshot_json(perf: &PerfSnapshot) -> serde_json::Value {
    let by_name = |series: &[(String, Stat)]| -> serde_json::Map<String, serde_json::Value> {
        series
            .iter()
            .map(|(n, s)| (n.clone(), stat_json(s)))
            .collect()
    };
    let systems: serde_json::Map<_, _> = perf
        .systems
        .iter()
        .map(|s| {
            let mut v = stat_json(&s.stat);
            v["feature"] = s.feature.clone().into();
            (s.name.clone(), v)
        })
        .collect();
    serde_json::json!({
        "ticks": perf.ticks,
        "tick": stat_json(&perf.tick),
        "phases": by_name(&perf.phases),
        "features": by_name(&perf.features),
        "systems": systems,
    })
}

/// The `--profile --json` document: `meta`, `rows` (the [`BenchRow`]s, which `--baseline`
/// reads back), `sizes` (per row, the profiler's tick, phases, features and systems), and the
/// `phases`, `features` and `systems` of the last size repeated at the top level.
///
/// `perf[i]` belongs to `rows[i]`.
#[must_use]
pub fn profile_json(
    rows: &[BenchRow],
    perf: &[PerfSnapshot],
    meta: &serde_json::Value,
) -> serde_json::Value {
    let sizes: Vec<_> = rows
        .iter()
        .zip(perf)
        .map(|(row, p)| {
            let mut v = snapshot_json(p);
            v["units"] = row.units.into();
            v
        })
        .collect();
    let mut doc = serde_json::json!({
        "meta": meta,
        "rows": rows,
        "sizes": sizes,
    });
    if let Some(last) = sizes.last() {
        for key in ["phases", "features", "systems"] {
            doc[key] = last[key].clone();
        }
    }
    doc
}

/// One row of a breakdown table: `name` and its value in each of `rows`.
fn breakdown(
    out: &mut String,
    title: &str,
    rows: &[BenchRow],
    list: fn(&BenchRow) -> &[(String, f64)],
) {
    let mut names: Vec<&str> = Vec::new();
    for r in rows {
        for (name, _) in list(r) {
            if !names.contains(&name.as_str()) {
                names.push(name);
            }
        }
    }
    if names.is_empty() {
        return;
    }
    let _ = write!(out, "\n## {title}\n\n| name |");
    for r in rows {
        let _ = write!(out, " {} |", r.units);
    }
    out.push_str("\n|---|");
    for _ in rows {
        out.push_str("---:|");
    }
    out.push('\n');
    for name in names {
        let _ = write!(out, "| {name} |");
        for r in rows {
            match list(r).iter().find(|(n, _)| n == name) {
                Some((_, v)) => {
                    let _ = write!(out, " {v:.3} |");
                }
                None => out.push_str(" - |"),
            }
        }
        out.push('\n');
    }
}

/// The markdown report: the headline table, then per-phase milliseconds (over the last
/// `profiler_window_ticks` ticks, [`BenchConfig::profiler_window_ticks`]) and per-tick counters.
#[must_use]
pub fn render_markdown(rows: &[BenchRow], meta: &str, profiler_window_ticks: u32) -> String {
    let mut out = format!("# Sim bench\n\n{meta}\n\n");
    out.push_str(
        "| units | map | entities | mean ms | p95 ms | max ms |\n|---:|---|---:|---:|---:|---:|\n",
    );
    for r in rows {
        let _ = writeln!(
            out,
            "| {} | {} | {} | {:.3} | {:.3} | {:.3} |",
            r.units, r.map, r.entities, r.mean_ms, r.p95_ms, r.max_ms
        );
    }
    let phases = format!("Mean ms per phase (profiler window: last {profiler_window_ticks} ticks)");
    breakdown(&mut out, &phases, rows, |r| &r.per_phase_mean_ms);
    breakdown(&mut out, "Work counters per tick", rows, |r| {
        &r.counters_per_tick
    });
    out
}

/// Why [`compare`] failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CompareError {
    /// The run and the baseline share no size: nothing was compared.
    #[error(
        "bench baseline: no size in common (run {run:?}, baseline {baseline:?}): nothing compared"
    )]
    NoCommonSize { run: Vec<u32>, baseline: Vec<u32> },
    /// These sizes regressed, one message each.
    #[error("bench regression:\n{}", .0.join("\n"))]
    Regression(Vec<String>),
}

/// Fail if a size's `mean_ms` exceeds its baseline's by more than `max_regression` (0.20 =
/// 20 %). Sizes missing from either side are skipped, but at least one must be in both.
///
/// # Errors
/// [`CompareError::Regression`] naming each regressed size, or
/// [`CompareError::NoCommonSize`].
pub fn compare(
    rows: &[BenchRow],
    baseline: &[BenchRow],
    max_regression: f64,
) -> Result<(), CompareError> {
    let mut failures = Vec::new();
    let mut compared = 0;
    for base in baseline {
        let Some(now) = rows.iter().find(|r| r.units == base.units) else {
            continue;
        };
        compared += 1;
        if now.mean_ms > base.mean_ms * (1.0 + max_regression) {
            failures.push(format!(
                "{} units: {:.3} ms vs baseline {:.3} ms (+{:.1}%, limit {:.0}%)",
                base.units,
                now.mean_ms,
                base.mean_ms,
                (now.mean_ms / base.mean_ms - 1.0) * 100.0,
                max_regression * 100.0
            ));
        }
    }
    if compared == 0 {
        let sizes = |rs: &[BenchRow]| rs.iter().map(|r| r.units).collect();
        Err(CompareError::NoCommonSize {
            run: sizes(rows),
            baseline: sizes(baseline),
        })
    } else if failures.is_empty() {
        Ok(())
    } else {
        Err(CompareError::Regression(failures))
    }
}

/// The rows of a document written by [`profile_json`].
///
/// # Errors
/// If `doc` has no valid `rows`.
pub fn rows_from_json(doc: &serde_json::Value) -> Result<Vec<BenchRow>, CliError> {
    serde_json::from_value(doc["rows"].clone())
        .map_err(|e| CliError::Io(format!("baseline has no valid rows: {e}")))
}

/// Where `bench` writes and what it compares against.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BenchOutputs {
    /// Also write the markdown table here.
    pub markdown: Option<PathBuf>,
    /// Write the JSON document here (e.g. `docs/perf/<milestone>.json`).
    pub json: Option<PathBuf>,
    /// Include the profiler's breakdown in the JSON.
    pub profile: bool,
    /// A JSON file written earlier; a regression against it fails the bench.
    pub baseline: Option<PathBuf>,
    /// The allowed slowdown of the mean tick time against the baseline (0.20 = 20 %).
    pub max_regression: f64,
    /// Compare against a baseline that ran on another map (with a warning) instead of
    /// refusing.
    pub force: bool,
}

/// The rows of baseline `path`, and its build type if it names one.
fn read_baseline(path: &PathBuf) -> Result<(Vec<BenchRow>, Option<String>), CliError> {
    let text = std::fs::read_to_string(path)
        .map_err(|err| CliError::Io(format!("cannot read {}: {err}", path.display())))?;
    let doc: serde_json::Value = serde_json::from_str(&text)
        .map_err(|err| CliError::Io(format!("{} is not JSON: {err}", path.display())))?;
    let build = doc["meta"]["build"].as_str().map(str::to_owned);
    Ok((rows_from_json(&doc)?, build))
}

/// `bench`: [`run_bench_full`], as the markdown report (text) and the [`profile_json`]
/// document (JSON; without `profile` only its `meta` and `rows`), written where `outputs`
/// says. With a baseline, a regression makes the exit code 1 and adds
/// `"baseline": {"ok": false, "message"}` to the JSON (`{"ok": true}` otherwise); a baseline
/// of another build type is warned about on stderr. A baseline that ran on another map
/// (`docs/perf/M0.json` is the open map: compare it with `--flat`) is refused before anything
/// runs, unless `force`, which compares with a warning.
///
/// # Errors
/// If the bench cannot run, a file cannot be read or written, or the baseline has no valid
/// rows, ran on another map (without `force`) or has no size in common with this run.
pub fn bench_report(cfg: &BenchConfig, outputs: &BenchOutputs) -> Result<Report, CliError> {
    let cfg = BenchConfig {
        repeat: cfg.runs(),
        ..cfg.clone()
    };
    let map = if cfg.flat {
        OPEN_MAP
    } else {
        fl_sim::SCALE_MAP
    };
    let baseline = outputs.baseline.as_ref().map(read_baseline).transpose()?;
    if let Some((base, _)) = &baseline
        && let Some(other) = base.iter().map(|row| row.map.as_str()).find(|&m| m != map)
    {
        let message = format!(
            "the baseline ran on map {other}, this run on map {map}: the comparison is not \
             like for like (--flat runs the open map)"
        );
        if !outputs.force {
            return Err(CliError::Check(format!(
                "bench baseline: {message}; pass --force to compare anyway"
            )));
        }
        eprintln!("warning: {message}");
    }
    let results = run_bench_full(&cfg)?;
    let (rows, perf): (Vec<_>, Vec<_>) = results.into_iter().unzip();
    let build = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    let window = cfg.profiler_window_ticks();
    let BenchConfig {
        ticks,
        warmup,
        seed,
        repeat,
        ..
    } = cfg;
    let meta_text = format!(
        "map {map}, ticks {ticks} (+{warmup} warmup), seed {seed}, median of {repeat}, {build} build; \
         profiler window: last {window} ticks; \
         wall times include profiler and phase-marker overhead"
    );
    let markdown = render_markdown(&rows, &meta_text, window);
    let write = |path: &PathBuf, text: &str| {
        std::fs::write(path, text)
            .map_err(|err| CliError::Io(format!("cannot write {}: {err}", path.display())))
    };
    if let Some(path) = &outputs.markdown {
        write(path, &markdown)?;
    }
    let meta = serde_json::json!({
        "units": cfg.units, "map": map, "ticks": ticks, "warmup": warmup, "seed": seed,
        "repeat": repeat, "build": build, "profiler_window_ticks": window,
        "note": "wall times include profiler and phase-marker overhead; \
                 profiler statistics cover the last profiler_window_ticks ticks",
    });
    let mut doc = profile_json(&rows, &perf, &meta);
    if !outputs.profile
        && let Some(map) = doc.as_object_mut()
    {
        map.retain(|key, _| matches!(key.as_str(), "meta" | "rows"));
    }
    if let Some(path) = &outputs.json {
        let text = serde_json::to_string_pretty(&doc).map_err(|e| CliError::Io(e.to_string()))?;
        write(path, &(text + "\n"))?;
    }
    let mut report = Report::ok(doc, markdown);
    if let Some((base, base_build)) = baseline {
        if let Some(base_build) = base_build
            && base_build != build
        {
            eprintln!(
                "warning: the baseline is a {base_build} build, this run is a {build} build: \
                 the comparison is not like for like"
            );
        }
        match compare(&rows, &base, outputs.max_regression) {
            Ok(()) => report.json["baseline"] = serde_json::json!({"ok": true}),
            Err(err @ CompareError::NoCommonSize { .. }) => {
                return Err(CliError::Check(err.to_string()));
            }
            Err(err @ CompareError::Regression(_)) => {
                let message = err.to_string();
                eprintln!("{message}");
                report.text.push_str(&format!("\n{message}\n"));
                report.json["baseline"] = serde_json::json!({"ok": false, "message": message});
                report.code = 1;
            }
        }
    }
    Ok(report)
}
