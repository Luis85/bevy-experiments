//! The work-counter perf gate (spec §5.7, §12.6; ADR-0010): the scale scenario's entity mix
//! on the 2 km terrain map at 500, 3,000 and 5,000 units and on M0's flat, open map at 3,000
//! (non-path work and allocations gated at M0 levels, R102), and the pathing workloads the mix
//! does not exercise (group moves on flow fields, more live flow fields than the cache keeps, a
//! crowd through a choke point), every per-tick counter held to `docs/perf/budgets.ron`. Every
//! run starts with empty navigation caches (R86).
//!
//! A counting allocator wraps this binary's allocator, so allocations are budgeted too; they
//! are never snapshotted. The totals of the other counters are insta snapshots.
//!
//! The allocator counts the whole process, so the measurements assume nothing else allocates
//! meanwhile: every test of this binary holds [`SERIAL`] while it runs (nextest also runs each
//! test in a process of its own; `cargo test` runs them as threads of one process).
//!
//! `FL_RECORD_BUDGETS=1` records the budgets into `docs/perf/budgets.ron.new` instead of
//! checking them; only the lead promotes it
//! (`cargo xtask bless docs/perf/budgets.ron --reason "…"`).

use std::alloc::System;
use std::collections::BTreeMap;
use std::sync::{Mutex, MutexGuard};

use fl_sim::nav::PathRules;
use fl_test_utils::{
    BudgetFile, CounterRun, CounterSim, base_content, budgets_path, check_budgets, load_budgets,
    record_budgets, run_counters, scale_path_charges, scale_queue_lengths,
};
use serde::Serialize;
use stats_alloc::{INSTRUMENTED_SYSTEM, StatsAlloc};

#[global_allocator]
static GLOBAL: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

/// Ticks run before measuring: two flight times (2 × 20 ticks). The projectile population
/// peaks after one, when the first despawns arrive, and the tables grow then; by the second
/// the buffer capacities have settled, so a one-off growth is not budgeted as per-tick work.
const WARMUP: u32 = 40;

/// Ticks measured.
const TICKS: u32 = 200;

/// Held by every test of this binary for its whole run, so no other test allocates while a
/// tick is measured, whichever runner runs the tests.
static SERIAL: Mutex<()> = Mutex::new(());

fn serial() -> MutexGuard<'static, ()> {
    SERIAL
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// What a scenario's snapshot records: the sim entities at the end and every counter's total
/// except the allocations, which are budgeted, never snapshotted.
#[derive(Serialize)]
struct Totals<'a> {
    entities: usize,
    totals: BTreeMap<&'a str, u64>,
}

/// Run `sim`; record or check the budgets of `scenario`; snapshot the entity count and the
/// totals, allocations left out.
///
/// The alloc counters are exact only if nothing else in the process allocates during a
/// measured tick: the caller holds [`SERIAL`], and nextest runs each test in its own process.
fn gate(scenario: &str, sim: impl FnOnce() -> CounterSim) {
    let _serial = serial();
    let run = run_counters(sim(), WARMUP, TICKS, Some(GLOBAL));
    let path = budgets_path();
    if std::env::var_os("FL_RECORD_BUDGETS").is_some_and(|v| v == "1") {
        record_budgets(scenario, &run, &path);
    } else if let Err(errors) = check_budgets(scenario, &run, &load_budgets(&path)) {
        panic!(
            "{scenario} exceeds its per-tick work budgets:\n  {}",
            errors.join("\n  ")
        );
    }
    let totals = Totals {
        entities: run.entities,
        totals: run
            .totals
            .iter()
            .filter(|(name, _)| !name.starts_with("alloc."))
            .map(|(name, &n)| (name.as_str(), n))
            .collect(),
    };
    insta::assert_ron_snapshot!(scenario, totals);
}

#[test]
fn smoke_counters_scale_500() {
    gate("scale_500", || CounterSim::scale(500));
}

#[test]
fn counters_scale_3000() {
    gate("scale_3000", || CounterSim::scale(3000));
}

#[test]
fn milestone_counters_scale_5000() {
    gate("scale_5000", || CounterSim::scale(5000));
}

/// M0's flat workload (`ScaleScenario::with_units(3000)`, no terrain): non-path work and
/// allocations stay gated at M0 levels while the terrain scenarios carry the pathing (R102,
/// R125).
#[test]
fn counters_flat_3000() {
    gate("flat_3000", || CounterSim::flat(3000));
}

/// Group moves on flow fields (Task 10): `flowfield.abstract_nodes` and `path.budget_spent`
/// among the budgeted counters.
#[test]
fn counters_group_move() {
    gate("group_move", CounterSim::group_move);
}

/// 36 live flow fields, more than `Path.flow_cache_max` (32): with eviction at the tick
/// boundary (R124) the fields are built once, in the request tick, so `flowfield.*` measures
/// that tick's builds and first-time tiles, never a rebuild.
#[test]
fn counters_flow_thrash() {
    gate("flow_thrash", CounterSim::flow_thrash);
}

/// A crowd through a choke point (R117 as amended by R127): `steer.pairs` and the path and
/// flow counters budgeted. `move.blocked` never occurs here (units slide along the walls), so
/// it is gated where it does, in `scale_3000` and `scale_5000`; zero-valued counters are not
/// emitted. A head-on jam variant is C7 crowd tuning.
#[test]
fn counters_choke_crowd() {
    gate("choke_crowd", CounterSim::choke_crowd);
}

/// The path budget holds at scale (ADR-0015): each tick, the work charged before the last
/// request started stays below `Path.budget_per_tick`, so `path.budget_spent` exceeds the
/// budget by less than the largest single charge of the tick; and the counter is the sum of
/// the journaled charges.
#[test]
fn path_budget_holds_at_scale() {
    let _serial = serial();
    let budget = u64::from(base_content().rules::<PathRules>().budget_per_tick);
    let ticks = scale_path_charges(3000, WARMUP, 100);
    let mut charged = 0;
    for tick in &ticks {
        let largest = tick.charges.iter().copied().max().unwrap_or(0);
        assert!(
            tick.spent.saturating_sub(largest) < budget,
            "tick {:?}: spent {} with largest charge {largest}, budget {budget}",
            tick.tick,
            tick.spent
        );
        assert_eq!(
            tick.spent,
            tick.charges.iter().sum::<u64>(),
            "tick {:?}: path.budget_spent is the sum of the charges {:?}",
            tick.tick,
            tick.charges
        );
        charged += usize::from(!tick.charges.is_empty());
    }
    assert!(charged > 50, "paths were planned on {charged} of 100 ticks");
}

#[test]
fn budget_violation_names_counter() {
    let _serial = serial();
    let run = CounterRun {
        per_tick_max: BTreeMap::from([("spawn.entities".to_owned(), 31)]),
        totals: BTreeMap::from([("spawn.entities".to_owned(), 31)]),
        entities: 0,
    };
    let budgets = BudgetFile {
        scenarios: BTreeMap::from([(
            "scale_500".to_owned(),
            BTreeMap::from([("spawn.entities".to_owned(), 25)]),
        )]),
    };
    assert_eq!(
        check_budgets("scale_500", &run, &budgets),
        Err(vec![
            "scale_500: spawn.entities max 31 > budget 25".to_owned()
        ])
    );

    let within = BudgetFile {
        scenarios: BTreeMap::from([(
            "scale_500".to_owned(),
            BTreeMap::from([("spawn.entities".to_owned(), 31)]),
        )]),
    };
    assert_eq!(check_budgets("scale_500", &run, &within), Ok(()));
}

/// The 5,000-unit path-queue burst drains (R126). Its source: at tick 0 every unit is idle, so
/// wandering gives all 5,000 a move at once and the queue starts near 5,000 requests; the
/// budget (`Path.budget_per_tick`) processes about 100 a tick, so it is empty by tick 52 and
/// stays empty: later wander orders come one unit at a time as each arrives.
#[test]
fn queue_burst_drains_at_5000() {
    let _serial = serial();
    let queue = scale_queue_lengths(5000, 160);
    assert!(
        queue[0] > 4000,
        "the tick-0 wander burst: {} queued",
        queue[0]
    );
    let late: Vec<(usize, u64)> = queue
        .iter()
        .copied()
        .enumerate()
        .skip(60)
        .filter(|&(_, n)| n > 0)
        .collect();
    assert!(
        late.is_empty(),
        "requests still queued after tick 60: {late:?}"
    );
}
