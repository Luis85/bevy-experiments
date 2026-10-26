//! The shared, thread-safe collector behind [`PerfCollector`].
//!
//! Lock order (always taken in this order, never the reverse):
//! `Inner::state` → `Inner::slots` → `Slot::data`. The span hot path
//! (`on_exit`) only takes one `Slot::data` lock.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError, RwLock};
use std::time::{Duration, Instant};

use crate::budget::{BudgetBreach, PerfBudgets};
use crate::layer::PerfLayer;
use crate::report::PerfReport;
use crate::stats::{MechanicStats, Ring, SystemStats, us};
use crate::tags::MechanicTags;

/// Default rolling-window length (samples per system and per mechanic).
pub const DEFAULT_WINDOW: usize = 600;

/// Default minimum number of samples (frames in which it ran) a mechanic
/// needs since the last reset before its budget is checked: a p95 over a
/// handful of samples is just their max, and the first frames after a reset
/// or start-up pay one-off costs (schedule initialisation, first archetypes).
pub const DEFAULT_MIN_SAMPLES: u64 = 60;

/// Default minimum wall time between two breaches (and log lines) of one
/// mechanic ([`PerfCollector::set_breach_interval`]).
pub const BREACH_INTERVAL: Duration = Duration::from_secs(5);

/// How [`PerfCollector::end_frame`] logs a budget breach (target
/// `npc_perf`). Breaches are recorded ([`PerfCollector::take_breaches`],
/// the report's `over_budget`, the F4 panel, `npcctl bench`) whatever the
/// mode: it only picks the log level.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BreachLog {
    /// `warn!` on every breach (the default; at most one per mechanic per
    /// breach interval).
    #[default]
    Warn,
    /// `warn!` on a mechanic's first breach since the last reset, `debug!`
    /// afterwards: a windowed game says once that a mechanic is slow
    /// instead of every few seconds (`NpcPerfPlugin`'s default).
    WarnOnce,
    /// `debug!` only.
    Debug,
}

/// Cap on stored, not-yet-taken breaches.
const MAX_PENDING_BREACHES: usize = 1024;

/// What a timed span is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    /// A Bevy `"system"` span; its mechanic comes from the tags.
    System,
    /// A `"mechanic"` span; its name is the mechanic.
    Mechanic,
}

/// One timed name (a system, or a mechanic span name).
#[derive(Debug)]
pub(crate) struct Slot {
    kind: Kind,
    name: String,
    data: Mutex<SlotData>,
}

#[derive(Debug)]
struct SlotData {
    mechanic: Arc<str>,
    calls: u64,
    total_ns: u64,
    window: Ring,
    frame_ns: u64,
    frame_calls: u64,
}

impl Slot {
    /// The mechanic this slot's time counts towards.
    pub(crate) fn mechanic(&self) -> Arc<str> {
        lock(&self.data).mechanic.clone()
    }

    /// Records one span duration.
    pub(crate) fn record(&self, ns: u64) {
        let mut d = lock(&self.data);
        d.calls += 1;
        d.total_ns += ns;
        d.window.push(ns);
        d.frame_ns += ns;
        d.frame_calls += 1;
    }
}

#[derive(Debug, Default)]
struct Slots {
    systems: HashMap<String, Arc<Slot>>,
    mechanics: HashMap<String, Arc<Slot>>,
    list: Vec<Arc<Slot>>,
}

impl Slots {
    fn get(&self, kind: Kind, name: &str) -> Option<&Arc<Slot>> {
        match kind {
            Kind::System => self.systems.get(name),
            Kind::Mechanic => self.mechanics.get(name),
        }
    }
}

#[derive(Debug)]
struct MechAcc {
    window: Ring,
    samples: u64,
    calls: u64,
    total_ns: u64,
}

#[derive(Debug, Default)]
struct State {
    tags: MechanicTags,
    budgets: PerfBudgets,
    min_samples: u64,
    mechanics: BTreeMap<Arc<str>, MechAcc>,
    frames: u64,
    breaches: Vec<BudgetBreach>,
    last_breach: HashMap<String, Instant>,
    breach_interval: Duration,
    breach_log: BreachLog,
    /// Mechanics already warned about ([`BreachLog::WarnOnce`]).
    warned: BTreeSet<String>,
    scratch: Vec<u64>,
}

#[derive(Debug)]
pub(crate) struct Inner {
    window: usize,
    epoch: Instant,
    state: Mutex<State>,
    slots: RwLock<Slots>,
}

impl Inner {
    /// Monotonic nanoseconds since the collector was created.
    #[inline]
    pub(crate) fn now_ns(&self) -> u64 {
        self.epoch.elapsed().as_nanos() as u64
    }

    /// The slot for `(kind, name)`, created on first use.
    pub(crate) fn slot(&self, kind: Kind, name: &str) -> Arc<Slot> {
        if let Some(slot) = read(&self.slots).get(kind, name) {
            return slot.clone();
        }
        let state = lock(&self.state);
        let mut slots = write(&self.slots);
        if let Some(slot) = slots.get(kind, name) {
            return slot.clone();
        }
        let mechanic: Arc<str> = match kind {
            Kind::System => state.tags.mechanic_for(name).into(),
            Kind::Mechanic => name.into(),
        };
        let slot = Arc::new(Slot {
            kind,
            name: name.to_owned(),
            data: Mutex::new(SlotData {
                mechanic,
                calls: 0,
                total_ns: 0,
                window: Ring::new(self.window),
                frame_ns: 0,
                frame_calls: 0,
            }),
        });
        match kind {
            Kind::System => slots.systems.insert(name.to_owned(), slot.clone()),
            Kind::Mechanic => slots.mechanics.insert(name.to_owned(), slot.clone()),
        };
        slots.list.push(slot.clone());
        slot
    }
}

/// The profiler: a cheap-to-clone handle to shared, thread-safe stats.
///
/// Feed it through [`PerfCollector::layer`] (a `tracing` layer) and call
/// [`PerfCollector::end_frame`] once per frame (or per sim tick headless).
#[derive(Clone, Debug)]
pub struct PerfCollector(pub(crate) Arc<Inner>);

impl Default for PerfCollector {
    fn default() -> Self {
        Self::new(DEFAULT_WINDOW)
    }
}

impl PerfCollector {
    /// A collector keeping the last `window` samples (min 1) per system and
    /// per mechanic, tagged with [`MechanicTags::default_npc`], with no
    /// budgets and [`DEFAULT_MIN_SAMPLES`].
    pub fn new(window: usize) -> Self {
        let state = State {
            tags: MechanicTags::default_npc(),
            min_samples: DEFAULT_MIN_SAMPLES,
            breach_interval: BREACH_INTERVAL,
            ..State::default()
        };
        Self(Arc::new(Inner {
            window: window.max(1),
            epoch: Instant::now(),
            state: Mutex::new(state),
            slots: RwLock::new(Slots::default()),
        }))
    }

    /// The process-wide collector (window [`DEFAULT_WINDOW`]), created on
    /// first use. Meant for `LogPlugin::custom_layer`, which is a plain `fn`
    /// and cannot capture a collector.
    pub fn global() -> &'static PerfCollector {
        static GLOBAL: OnceLock<PerfCollector> = OnceLock::new();
        GLOBAL.get_or_init(PerfCollector::default)
    }

    /// The rolling-window length.
    pub fn window(&self) -> usize {
        self.0.window
    }

    /// A `tracing` layer feeding this collector.
    pub fn layer(&self) -> PerfLayer {
        PerfLayer::new(self.0.clone())
    }

    /// [`PerfCollector::layer`] boxed as `Box<dyn Layer<Registry> + Send +
    /// Sync>`, which is Bevy's `bevy::log::BoxedLayer`.
    pub fn boxed_layer(
        &self,
    ) -> Box<dyn tracing_subscriber::Layer<tracing_subscriber::Registry> + Send + Sync> {
        Box::new(self.layer())
    }

    /// Closes the current frame: each mechanic that ran since the last call
    /// gets one sample (the sum of its time), then budgets are checked for
    /// mechanics with at least [`PerfCollector::min_samples`] samples since
    /// the last reset. A breach is recorded, and logged on target `npc_perf`
    /// (level per [`PerfCollector::set_breach_log`]), at most once per
    /// mechanic per breach interval (default [`BREACH_INTERVAL`]).
    pub fn end_frame(&self) {
        let mut warnings = Vec::new();
        {
            let mut guard = lock(&self.0.state);
            let state = &mut *guard;
            state.frames += 1;
            let mut sums: BTreeMap<Arc<str>, (u64, u64)> = BTreeMap::new();
            for slot in &read(&self.0.slots).list {
                let mut d = lock(&slot.data);
                if d.frame_calls > 0 {
                    let entry = sums.entry(d.mechanic.clone()).or_default();
                    entry.0 += d.frame_ns;
                    entry.1 += d.frame_calls;
                    d.frame_ns = 0;
                    d.frame_calls = 0;
                }
            }
            let now = Instant::now();
            for (mechanic, (ns, calls)) in sums {
                let window = self.0.window;
                let acc = state
                    .mechanics
                    .entry(mechanic.clone())
                    .or_insert_with(|| MechAcc {
                        window: Ring::new(window),
                        samples: 0,
                        calls: 0,
                        total_ns: 0,
                    });
                acc.window.push(ns);
                acc.samples += 1;
                acc.calls += calls;
                acc.total_ns += ns;

                let Some(budget_us) = state.budgets.get(&mechanic) else {
                    continue;
                };
                if acc.samples < state.min_samples {
                    continue;
                }
                let recent = state
                    .last_breach
                    .get(&*mechanic)
                    .is_some_and(|&t| now.duration_since(t) < state.breach_interval);
                if recent {
                    continue;
                }
                let p95_ns = acc.window.p95(&mut state.scratch);
                if p95_ns > budget_us.saturating_mul(1000) {
                    state.last_breach.insert(mechanic.to_string(), now);
                    let breach = BudgetBreach {
                        mechanic: mechanic.to_string(),
                        p95_us: us(p95_ns),
                        budget_us,
                        frame: state.frames,
                    };
                    if state.breaches.len() < MAX_PENDING_BREACHES {
                        state.breaches.push(breach.clone());
                    }
                    let warn = match state.breach_log {
                        BreachLog::Warn => true,
                        BreachLog::WarnOnce => state.warned.insert(breach.mechanic.clone()),
                        BreachLog::Debug => false,
                    };
                    warnings.push((breach, warn));
                }
            }
        }
        for (b, warn) in warnings {
            if warn {
                tracing::warn!(
                    target: "npc_perf",
                    mechanic = %b.mechanic,
                    p95_us = b.p95_us,
                    budget_us = b.budget_us,
                    frame = b.frame,
                    "mechanic {} over budget: p95 {:.1} us > {} us",
                    b.mechanic,
                    b.p95_us,
                    b.budget_us
                );
            } else {
                tracing::debug!(
                    target: "npc_perf",
                    mechanic = %b.mechanic,
                    p95_us = b.p95_us,
                    budget_us = b.budget_us,
                    frame = b.frame,
                    "mechanic {} over budget: p95 {:.1} us > {} us",
                    b.mechanic,
                    b.p95_us,
                    b.budget_us
                );
            }
        }
    }

    /// A snapshot of all stats: [`PerfCollector::report_top`] with no limit.
    /// O(window) per system and mechanic (selection, no per-row sort).
    pub fn report(&self) -> PerfReport {
        self.report_top(usize::MAX)
    }

    /// The report truncated to the `top` systems and `top` mechanics by
    /// whole-run total (the same rows as `report()` truncated; BRP
    /// `npc/perf {top}`). See [`PerfCollector::report_limited`].
    pub fn report_top(&self, top: usize) -> PerfReport {
        self.report_limited(top, top)
    }

    /// The report with at most `systems` systems and `mechanics` mechanics
    /// (the highest whole-run totals). Percentiles are computed only for
    /// the rows kept, so its cost does not grow with the number of systems:
    /// what a live panel should call (the F4 panel keeps every mechanic, to
    /// colour systems of over-budget mechanics).
    pub fn report_limited(&self, systems: usize, mechanics: usize) -> PerfReport {
        let state = lock(&self.0.state);
        let mut scratch = Vec::with_capacity(self.0.window);
        // Rank by total first (cheap), then summarise only the kept rows.
        let mut rows: Vec<(u64, Arc<Slot>)> = read(&self.0.slots)
            .list
            .iter()
            .filter(|slot| slot.kind == Kind::System)
            .filter_map(|slot| {
                let d = lock(&slot.data);
                (d.calls > 0).then(|| (d.total_ns, slot.clone()))
            })
            .collect();
        rows.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.name.cmp(&b.1.name)));
        rows.truncate(systems);
        let systems = rows
            .iter()
            .map(|(_, slot)| {
                let d = lock(&slot.data);
                let s = d.window.summary_with(&mut scratch);
                SystemStats {
                    name: slot.name.clone(),
                    mechanic: d.mechanic.to_string(),
                    calls: d.calls,
                    total_us: us(d.total_ns),
                    p50_us: us(s.p50),
                    p95_us: us(s.p95),
                    max_us: us(s.max),
                }
            })
            .collect();
        let mut accs: Vec<(&Arc<str>, &MechAcc)> = state.mechanics.iter().collect();
        accs.sort_by(|a, b| b.1.total_ns.cmp(&a.1.total_ns).then_with(|| a.0.cmp(b.0)));
        accs.truncate(mechanics);
        let min_samples = state.min_samples;
        let mechanics = accs
            .into_iter()
            .map(|(mechanic, acc)| {
                let s = acc.window.summary_with(&mut scratch);
                let budget_us = state.budgets.get(mechanic);
                MechanicStats {
                    mechanic: mechanic.to_string(),
                    samples: acc.samples,
                    calls: acc.calls,
                    p50_us: us(s.p50),
                    p95_us: us(s.p95),
                    max_us: us(s.max),
                    total_us: us(acc.total_ns),
                    budget_us,
                    over_budget: acc.samples >= min_samples
                        && budget_us.is_some_and(|b| s.p95 > b.saturating_mul(1000)),
                }
            })
            .collect();
        PerfReport {
            systems,
            mechanics,
            frames: state.frames,
        }
    }

    /// Clears every stat, the frame counter, pending breaches and which
    /// mechanics were warned about ([`BreachLog::WarnOnce`]). Spans that
    /// already exist keep being timed. Tags and budgets are kept.
    pub fn reset(&self) {
        let mut state = lock(&self.0.state);
        state.mechanics.clear();
        state.frames = 0;
        state.breaches.clear();
        state.last_breach.clear();
        state.warned.clear();
        for slot in &read(&self.0.slots).list {
            let mut d = lock(&slot.data);
            d.calls = 0;
            d.total_ns = 0;
            d.window.clear();
            d.frame_ns = 0;
            d.frame_calls = 0;
        }
    }

    /// Replaces the tags and re-maps every known system. Mechanic history
    /// already recorded stays under the old mechanic names.
    pub fn set_tags(&self, tags: MechanicTags) {
        let mut state = lock(&self.0.state);
        state.tags = tags;
        for slot in &read(&self.0.slots).list {
            if slot.kind == Kind::System {
                lock(&slot.data).mechanic = state.tags.mechanic_for(&slot.name).into();
            }
        }
    }

    /// The current tags.
    pub fn tags(&self) -> MechanicTags {
        lock(&self.0.state).tags.clone()
    }

    /// Replaces the budgets (checked from the next `end_frame`).
    pub fn set_budgets(&self, budgets: PerfBudgets) {
        lock(&self.0.state).budgets = budgets;
    }

    /// Sets how many samples a mechanic needs (since the last reset) before
    /// its budget is checked and the report marks it `over_budget`
    /// (default [`DEFAULT_MIN_SAMPLES`]; `0` or `1` checks from the first).
    pub fn set_min_samples(&self, samples: u64) {
        lock(&self.0.state).min_samples = samples;
    }

    /// See [`PerfCollector::set_min_samples`].
    pub fn min_samples(&self) -> u64 {
        lock(&self.0.state).min_samples
    }

    /// The current budgets.
    pub fn budgets(&self) -> PerfBudgets {
        lock(&self.0.state).budgets.clone()
    }

    /// Sets how breaches are logged (default [`BreachLog::Warn`]); they are
    /// recorded either way.
    pub fn set_breach_log(&self, mode: BreachLog) {
        lock(&self.0.state).breach_log = mode;
    }

    /// See [`PerfCollector::set_breach_log`].
    pub fn breach_log(&self) -> BreachLog {
        lock(&self.0.state).breach_log
    }

    /// Sets the minimum wall time between two breaches of one mechanic
    /// (default [`BREACH_INTERVAL`]).
    pub fn set_breach_interval(&self, interval: Duration) {
        lock(&self.0.state).breach_interval = interval;
    }

    /// Drains the breaches recorded since the last call.
    pub fn take_breaches(&self) -> Vec<BudgetBreach> {
        std::mem::take(&mut lock(&self.0.state).breaches)
    }

    /// Records one run of system `name` lasting `duration`, as if its span
    /// had been entered and exited.
    pub fn record_system(&self, name: &str, duration: Duration) {
        self.0
            .slot(Kind::System, name)
            .record(duration.as_nanos() as u64);
    }

    /// Records one `"mechanic"` span named `mechanic` lasting `duration`.
    pub fn record_mechanic(&self, mechanic: &str, duration: Duration) {
        self.0
            .slot(Kind::Mechanic, mechanic)
            .record(duration.as_nanos() as u64);
    }

    /// Test hook: [`PerfCollector::record_system`] with whole microseconds.
    pub fn record_for_test(&self, name: &str, us: u64) {
        self.record_system(name, Duration::from_micros(us));
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

fn read<T>(l: &RwLock<T>) -> std::sync::RwLockReadGuard<'_, T> {
    l.read().unwrap_or_else(PoisonError::into_inner)
}

fn write<T>(l: &RwLock<T>) -> std::sync::RwLockWriteGuard<'_, T> {
    l.write().unwrap_or_else(PoisonError::into_inner)
}
