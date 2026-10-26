//! The work-counter perf gate (spec §5.7, §12.6; ADR-0010): a [`CounterSim`] run headless,
//! its per-tick [`SimCounters`] (and, through a counting allocator, its allocations) collected
//! into a [`CounterRun`] and checked against the per-tick budgets in `docs/perf/budgets.ron`.
//!
//! The scenarios ([`scenarios`]): the scale scenario's entity mix on the 2 km terrain map
//! ([`CounterSim::scale`], wandering through single path requests) and on M0's flat, open map
//! ([`CounterSim::flat`], so non-path regressions stay gated at M0-level work, R102), and three
//! pathing workloads the mix does not exercise: repeated group moves on flow fields
//! ([`CounterSim::group_move`]), more live flow fields than the cache keeps
//! ([`CounterSim::flow_thrash`]) and a crowd through a choke point
//! ([`CounterSim::choke_crowd`]).
//!
//! The counters are exact, so the check is too; wall clock is never gated here. Every run
//! builds its own sim, so it starts with empty navigation caches (R86: the `flowfield.*`
//! counters depend on what the cache held).

mod scenarios;

use std::alloc::System;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use fl_protocol::Tick;
use fl_sim::{ScaleScenario, SimCounters, SimIndex};
use serde::{Deserialize, Serialize};
use stats_alloc::StatsAlloc;

pub use scenarios::CounterSim;

/// Per-tick budgets: scenario name (`"scale_500"`) → counter name → the most work one tick
/// may do. `docs/perf/budgets.ron`, protected: the lead blesses it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BudgetFile {
    pub scenarios: BTreeMap<String, BTreeMap<String, u64>>,
}

/// The counters of a measured run, by counter name.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CounterRun {
    /// The largest count of one tick.
    pub per_tick_max: BTreeMap<String, u64>,
    /// The sum over every measured tick.
    pub totals: BTreeMap<String, u64>,
    /// Sim entities ([`SimIndex`] length) after the last tick.
    pub entities: usize,
}

impl CounterRun {
    fn add(&mut self, name: &str, n: u64) {
        let max = self.per_tick_max.entry(name.to_owned()).or_default();
        *max = (*max).max(n);
        let total = self.totals.entry(name.to_owned()).or_default();
        *total = total.saturating_add(n);
    }
}

/// `docs/perf/budgets.ron` of this workspace.
pub fn budgets_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/perf/budgets.ron")
}

/// Read a [`BudgetFile`].
///
/// # Panics
/// If `path` cannot be read or does not parse.
pub fn load_budgets(path: &Path) -> BudgetFile {
    let text = std::fs::read_to_string(path).unwrap_or_else(|err| {
        panic!(
            "reading the perf budgets {}: {err} (record them with FL_RECORD_BUDGETS=1; the \
             lead blesses the .new file)",
            path.display()
        )
    });
    ron::from_str(&text)
        .unwrap_or_else(|err| panic!("the perf budgets {} do not parse: {err}", path.display()))
}

/// Run [`CounterSim::scale`]`(units)`; see [`run_counters`].
///
/// # Panics
/// If the sim does not build or the scenario does not set up.
pub fn run_counter_scenario(
    units: u32,
    warmup: u32,
    ticks: u32,
    alloc: Option<&'static StatsAlloc<System>>,
) -> CounterRun {
    run_counters(CounterSim::scale(units), warmup, ticks, alloc)
}

/// Run `sim`: `warmup` ticks unmeasured, then `ticks` ticks whose [`SimCounters`] are
/// collected after each tick. With `alloc` (the test binary's global allocator), each
/// measured tick also counts `"alloc.count"` (allocations plus reallocations during
/// `step_tick`) and `"alloc.bytes"` (bytes requested by allocations, plus the net growth of
/// reallocations: the bytes they requested minus the bytes they freed, when positive). The
/// allocator counts every thread of the process, so nothing else may allocate while a
/// measured tick runs.
///
/// Pick `warmup` long enough for buffer capacities to settle: the projectile population
/// peaks one flight time after the start, when the first despawns arrive, and the tables
/// grow then.
pub fn run_counters(
    mut sim: CounterSim,
    warmup: u32,
    ticks: u32,
    alloc: Option<&'static StatsAlloc<System>>,
) -> CounterRun {
    for _ in 0..warmup {
        sim.step(None);
    }
    let mut run = CounterRun::default();
    for _ in 0..ticks {
        let (output, change) = sim.step(alloc);
        drop(output);
        if let Some(change) = change {
            let count = change.allocations.saturating_add(change.reallocations);
            let bytes = change
                .bytes_allocated
                .saturating_add(change.bytes_reallocated.max(0).unsigned_abs());
            run.add("alloc.count", count as u64);
            run.add("alloc.bytes", bytes as u64);
        }
        for (name, n) in sim.world().resource::<SimCounters>().snapshot() {
            run.add(name, n);
        }
    }
    run.entities = sim.world().resource::<SimIndex>().len();
    run
}

/// [`CounterSim::scale`]`(units)` from tick 0: the `"path.queue_len"` left after each of
/// `ticks` ticks (0 when nothing waits).
///
/// # Panics
/// If the sim does not build or the scenario does not set up.
pub fn scale_queue_lengths(units: u32, ticks: u32) -> Vec<u64> {
    let mut sim = CounterSim::scale(units);
    (0..ticks)
        .map(|_| {
            sim.step(None);
            counter(sim.world(), "path.queue_len")
        })
        .collect()
}

/// Counter `name` of the tick that just ran; 0 if it was not counted.
fn counter(world: &bevy_ecs::world::World, name: &str) -> u64 {
    world
        .resource::<SimCounters>()
        .snapshot()
        .into_iter()
        .find(|&(n, _)| n == name)
        .map_or(0, |(_, n)| n)
}

/// One tick's path budget: `"path.budget_spent"` and the `charge` of every
/// `"core.path_result"` journaled, in order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TickCharges {
    pub tick: Tick,
    pub spent: u64,
    pub charges: Vec<u64>,
}

/// [`CounterSim::scale`]`(units)` with the journal on: `warmup` ticks, then for each of
/// `ticks` ticks its [`TickCharges`].
///
/// # Panics
/// If the sim does not build, the scenario does not set up, or a path result has no charge.
pub fn scale_path_charges(units: u32, warmup: u32, ticks: u32) -> Vec<TickCharges> {
    let mut sim = CounterSim::from_scale(&ScaleScenario::on_terrain(units), true);
    for _ in 0..warmup {
        sim.step(None);
    }
    (0..ticks)
        .map(|_| {
            let (output, _) = sim.step(None);
            let charges = output
                .journal
                .iter()
                .filter(|e| e.kind == "core.path_result")
                .map(|e| {
                    e.data["charge"]
                        .as_u64()
                        .unwrap_or_else(|| panic!("a path result without a charge: {e:?}"))
                })
                .collect();
            let spent = counter(sim.world(), "path.budget_spent");
            TickCharges {
                tick: output.tick,
                spent,
                charges,
            }
        })
        .collect()
}

/// Check `run` against the budgets of `scenario`: every counter's per-tick maximum must be
/// within its budget, every counter the run saw must have one, and every budgeted counter must
/// have been measured (a vanished `alloc.*` counter must not stop gating silently).
///
/// # Errors
/// One message per failed counter, in counter name order:
/// `"scale_500: spawn.entities max 31 > budget 25"`,
/// `"scale_500: foo.bar max 3 has no budget"`,
/// `"scale_500: alloc.count has budget 74 but was not measured"`; or `"scale_500: no budgets"` if the file has
/// no entry for `scenario`.
pub fn check_budgets(
    scenario: &str,
    run: &CounterRun,
    budgets: &BudgetFile,
) -> Result<(), Vec<String>> {
    let Some(limits) = budgets.scenarios.get(scenario) else {
        return Err(vec![format!("{scenario}: no budgets")]);
    };
    let names: std::collections::BTreeSet<&String> =
        run.per_tick_max.keys().chain(limits.keys()).collect();
    let errors: Vec<String> = names
        .into_iter()
        .filter_map(
            |name| match (run.per_tick_max.get(name), limits.get(name)) {
                (Some(&max), Some(&budget)) if max > budget => {
                    Some(format!("{scenario}: {name} max {max} > budget {budget}"))
                }
                (Some(_), Some(_)) => None,
                (Some(&max), None) => Some(format!("{scenario}: {name} max {max} has no budget")),
                (None, Some(&budget)) => Some(format!(
                    "{scenario}: {name} has budget {budget} but was not measured"
                )),
                (None, None) => unreachable!("the name comes from one of the two maps"),
            },
        )
        .collect();
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// The budget recorded for a per-tick maximum of `max`: `ceil(max × 1.25)`, but at least
/// `max + 2`, so a small count (a blocked unit or two, a few requests) keeps some headroom
/// (the `+2` decides below 8).
pub fn budget_for(max: u64) -> u64 {
    max.saturating_mul(5).div_ceil(4).max(max.saturating_add(2))
}

/// What the budget file says about a scenario, as comment lines above the budgets.
const SCENARIO_NOTES: &[(&str, &str)] = &[(
    "flat_3000",
    "M0's flat, open workload: non-path work and allocations at M0 levels (R102, R125).",
)];

/// Record `run` as the budgets of `scenario`: [`budget_for`] of every counter's per-tick
/// maximum, written to `<path>.new` with every other scenario of `<path>.new` (or else of
/// `path`) kept, for the lead to bless. Concurrent recorders (nextest runs each test in its
/// own process) take turns through a `<path>.new.lock` file.
///
/// Recording merges into an existing `<path>.new`: a scenario recorded earlier, and not
/// again, stays in it. Delete `<path>.new` before a recording session for a clean file.
///
/// # Panics
/// If an existing budget file does not parse, the lock is held for over a minute, or the
/// file cannot be written.
pub fn record_budgets(scenario: &str, run: &CounterRun, path: &Path) {
    let new_path = PathBuf::from(format!("{}.new", path.display()));
    let lock = PathBuf::from(format!("{}.lock", new_path.display()));
    let _guard = LockFile::acquire(&lock);

    let mut budgets = [&new_path, path]
        .into_iter()
        .find(|p| p.exists())
        .map(load_budgets)
        .unwrap_or_default();
    let limits = run
        .per_tick_max
        .iter()
        .map(|(name, &max)| (name.clone(), budget_for(max)))
        .collect();
    budgets.scenarios.insert(scenario.to_owned(), limits);

    // LF on every platform, as the committed file is (ron's default is CRLF on Windows).
    let body = ron::ser::to_string_pretty(
        &budgets,
        ron::ser::PrettyConfig::default().new_line(
            "
",
        ),
    )
    .expect("budgets serialize");
    let notes: String = SCENARIO_NOTES
        .iter()
        .filter(|(name, _)| budgets.scenarios.contains_key(*name))
        .map(|(name, note)| format!("// {name}: {note}\n"))
        .collect();
    let text = format!(
        "// Per-tick work-counter budgets (spec §12.6, ADR-0010): max(ceil(max × 1.25), max + 2)\n\
         // of a recorded run. Protected: recorded with FL_RECORD_BUDGETS=1 into budgets.ron.new;\n\
         // the lead blesses it with `cargo xtask bless`.\n{notes}{body}\n"
    );
    if let Some(dir) = new_path.parent() {
        std::fs::create_dir_all(dir)
            .unwrap_or_else(|err| panic!("creating {}: {err}", dir.display()));
    }
    std::fs::write(&new_path, text)
        .unwrap_or_else(|err| panic!("writing {}: {err}", new_path.display()));
}

/// An exclusive lock held while the file exists; removed on drop.
struct LockFile(PathBuf);

impl LockFile {
    fn acquire(path: &Path) -> LockFile {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)
                .unwrap_or_else(|err| panic!("creating {}: {err}", dir.display()));
        }
        for _ in 0..1200 {
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
            {
                Ok(_) => return LockFile(path.to_owned()),
                Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(err) => panic!("creating the lock {}: {err}", path.display()),
            }
        }
        panic!(
            "the lock {} was held for over a minute; delete it if no recorder runs",
            path.display()
        );
    }
}

impl Drop for LockFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_with(counters: &[(&str, u64)]) -> CounterRun {
        let mut run = CounterRun::default();
        for &(name, n) in counters {
            run.add(name, n);
        }
        run
    }

    #[test]
    fn record_then_check_round_trips() {
        let dir = std::env::temp_dir().join(format!("fl_perf_budgets_{}", std::process::id()));
        let path = dir.join("budgets.ron");
        let run = run_with(&[("a.n", 4), ("a.n", 8), ("b.n", 0)]);
        record_budgets("s", &run, &path);
        record_budgets("t", &run_with(&[("c.n", 1)]), &path);
        let budgets = load_budgets(&PathBuf::from(format!("{}.new", path.display())));
        let _ = std::fs::remove_dir_all(&dir);

        assert_eq!(budgets.scenarios["s"]["a.n"], 10, "ceil(8 × 1.25)");
        assert_eq!(budgets.scenarios["s"]["b.n"], 2, "0 + 2");
        assert_eq!(budgets.scenarios["t"]["c.n"], 3, "1 + 2 > ceil(1 × 1.25)");
        assert_eq!(check_budgets("s", &run, &budgets), Ok(()));
        assert_eq!(
            check_budgets("t", &run, &budgets),
            Err(vec![
                "t: a.n max 8 has no budget".to_owned(),
                "t: b.n max 0 has no budget".to_owned(),
                "t: c.n has budget 3 but was not measured".to_owned(),
            ])
        );
        assert_eq!(
            check_budgets("u", &run, &budgets),
            Err(vec!["u: no budgets".to_owned()])
        );
    }

    #[test]
    fn budgeted_counter_absent_from_the_run_fails() {
        let budgets = BudgetFile {
            scenarios: BTreeMap::from([(
                "s".to_owned(),
                BTreeMap::from([("a.n".to_owned(), 10), ("alloc.count".to_owned(), 74)]),
            )]),
        };
        let run = run_with(&[("a.n", 4)]);
        assert_eq!(
            check_budgets("s", &run, &budgets),
            Err(vec![
                "s: alloc.count has budget 74 but was not measured".to_owned()
            ])
        );
        assert_eq!(
            check_budgets("s", &run_with(&[("a.n", 4), ("alloc.count", 3)]), &budgets),
            Ok(())
        );
    }

    #[test]
    fn small_maxima_keep_two_of_headroom() {
        assert_eq!(budget_for(0), 2);
        assert_eq!(budget_for(1), 3);
        assert_eq!(budget_for(7), 9, "ceil(8.75) = 7 + 2");
        assert_eq!(budget_for(8), 10);
        assert_eq!(budget_for(100), 125);
    }
}
