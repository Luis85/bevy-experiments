//! Wall-time probes for the island's own hot paths (S4 measurement): the
//! supplier, seller and route lookups, the resident leaf starts by action,
//! and the parts of the island tick.
//!
//! A probe reports to two places, each only when switched on:
//! - the `npc_perf` profiler as a mechanic row, when the world has a
//!   [`PerfHandle`] (the game with its debug tools);
//! - this module's own table, after [`enable`] (the headless profile test
//!   `tests/island_tick_profile.rs`: `npcctl bench` has a private collector
//!   that only `tracing` spans reach, and this crate has no `tracing`).
//!
//! Profiling only: with both off a probe reads no clock. A probe writes only
//! these non-hashed stats; no sim state, decision or journal entry depends
//! on them, so every hash is the same with the probes on or off.
use bevy_ecs::world::World;
use npc_framework::perf::{PerfCollector, PerfHandle};
use std::cell::Cell;
use std::collections::BTreeMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

thread_local! {
    /// Set while a resident leaf start runs (see [`start`]), so a lookup
    /// reports under its `.start` row there and its `.other` row elsewhere
    /// (conditions, views, the island tick).
    static IN_START: Cell<bool> = const { Cell::new(false) };
}

static ENABLED: AtomicBool = AtomicBool::new(false);
static TABLE: Mutex<BTreeMap<&'static str, Row>> = Mutex::new(BTreeMap::new());

/// One probe's totals (nanoseconds) and its per-tick sums.
#[derive(Clone, Debug, Default)]
pub struct Row {
    pub calls: u64,
    pub total_ns: u64,
    /// Time in the current tick, closed by [`end_tick`].
    pub tick_ns: u64,
    /// One sum per tick in which the probe ran.
    pub ticks: Vec<u64>,
}

/// Switches this module's own table on (and clears it).
pub fn enable() {
    reset();
    ENABLED.store(true, Ordering::Relaxed);
}

/// Clears this module's own table.
pub fn reset() {
    if let Ok(mut table) = TABLE.lock() {
        table.clear();
    }
}

/// Closes one tick of the table: each row that ran gets one sample.
pub fn end_tick() {
    if let Ok(mut table) = TABLE.lock() {
        for row in table.values_mut() {
            if row.tick_ns > 0 {
                let sum = std::mem::take(&mut row.tick_ns);
                row.ticks.push(sum);
            }
        }
    }
}

/// A copy of the table, by probe name.
pub fn table() -> BTreeMap<&'static str, Row> {
    TABLE.lock().map(|table| table.clone()).unwrap_or_default()
}

/// A running measurement; records when dropped.
#[must_use]
pub struct Probe {
    running: Option<(Option<PerfCollector>, &'static str, Instant)>,
    restore: Option<bool>,
}

impl Drop for Probe {
    fn drop(&mut self) {
        if let Some((collector, mechanic, started)) = self.running.take() {
            let elapsed = started.elapsed();
            if let Some(collector) = collector {
                collector.record_mechanic(mechanic, elapsed);
            }
            if ENABLED.load(Ordering::Relaxed) {
                record(mechanic, elapsed);
            }
        }
        if let Some(previous) = self.restore {
            IN_START.with(|flag| flag.set(previous));
        }
    }
}

fn record(mechanic: &'static str, elapsed: Duration) {
    let ns = u64::try_from(elapsed.as_nanos()).unwrap_or(u64::MAX);
    if let Ok(mut table) = TABLE.lock() {
        let row = table.entry(mechanic).or_default();
        row.calls += 1;
        row.total_ns = row.total_ns.saturating_add(ns);
        row.tick_ns = row.tick_ns.saturating_add(ns);
    }
}

#[allow(clippy::disallowed_methods)] // determinism: profiler timing only; never feeds sim state.
fn begin(
    world: &World,
    mechanic: &'static str,
) -> Option<(Option<PerfCollector>, &'static str, Instant)> {
    let collector = world
        .get_resource::<PerfHandle>()
        .map(|handle| handle.0.clone());
    (collector.is_some() || ENABLED.load(Ordering::Relaxed))
        .then(|| (collector, mechanic, Instant::now()))
}

/// Measures the scope under `mechanic`.
pub fn probe(world: &World, mechanic: &'static str) -> Probe {
    Probe {
        running: begin(world, mechanic),
        restore: None,
    }
}

/// Measures a lookup under `in_start` while a leaf start runs, else under
/// `other`.
pub fn lookup(world: &World, in_start: &'static str, other: &'static str) -> Probe {
    let starting = IN_START.with(Cell::get);
    probe(world, if starting { in_start } else { other })
}

/// Measures one leaf start under `mechanic` and marks the scope as a start.
pub fn start(world: &World, mechanic: &'static str) -> Probe {
    let previous = IN_START.with(|flag| flag.replace(true));
    Probe {
        running: begin(world, mechanic),
        restore: Some(previous),
    }
}
