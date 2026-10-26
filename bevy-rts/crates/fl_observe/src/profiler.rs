//! The profiler: a tracing layer that times systems, phases and ticks (spec §15 D).
//!
//! - Every bevy_ecs `system` span (field `name`) is timed from enter to exit, on any thread.
//! - A system that runs inside a `sim_tick` span entered on the same thread belongs to that
//!   tick; any other system run (client systems on Bevy's worker threads) is pooled and
//!   folded when a `frame` span closes.
//! - Phases are timed between `fl_sim::phase` marker events (`phase = "<Phase>"`,
//!   `edge = "start" | "end"`) inside a `sim_tick` span.
//! - When a `sim_tick` span closes, its busy time, phases, systems and features fold into
//!   rolling statistics over the last [`WINDOW`] samples, and the tick is checked against the
//!   [`Budgets`]; an overrun logs a warning on target `fl_observe::budget`.
//!
//! Observe-only: the sim never reads any of this.

use std::cell::RefCell;
use std::collections::{BTreeMap, VecDeque};
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::Duration;

use serde::Serialize;
use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id};
use tracing::{Event, Subscriber};
use tracing_subscriber::layer::{Context, Layer};
use tracing_subscriber::registry::LookupSpan;

use crate::config::Budgets;

/// Samples kept per series: 240 ticks, 12 s of sim time at 20 Hz.
pub const WINDOW: usize = 240;

/// The marker event target of the sim's phases.
pub const PHASE_TARGET: &str = "fl_sim::phase";

/// The warning target of budget overruns.
pub const BUDGET_TARGET: &str = "fl_observe::budget";

/// A wall-clock reading. Observe-only: nothing in the sim ever sees one.
#[expect(
    clippy::disallowed_types,
    reason = "observe-only wall clock (spec §15)"
)]
#[derive(Copy, Debug)]
struct Stamp(std::time::Instant);

// By hand: a derived `Clone` names the field type again, outside the `#[expect]`.
impl Clone for Stamp {
    fn clone(&self) -> Self {
        *self
    }
}

impl Stamp {
    #[expect(
        clippy::disallowed_types,
        reason = "observe-only wall clock (spec §15)"
    )]
    fn now() -> Self {
        Self(std::time::Instant::now())
    }

    fn since(self, earlier: Self) -> Duration {
        self.0.saturating_duration_since(earlier.0)
    }
}

/// Rolling statistics of one series, in milliseconds.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Stat {
    /// Mean of the window.
    pub mean_ms: f64,
    /// Nearest-rank median of the window.
    pub p50_ms: f64,
    /// Nearest-rank 95th percentile of the window.
    pub p95_ms: f64,
    /// Maximum of the window.
    pub max_ms: f64,
    /// Samples since the start (or the last [`Profiler::reset`]), not only in the window.
    pub samples: u64,
}

/// One system's timings: one sample per tick (or client frame) it ran in.
#[derive(Clone, Debug, Serialize)]
pub struct SystemStat {
    /// The system name, such as `"fl_sim::movement::move_system"`.
    pub name: String,
    /// [`feature_of`] the name.
    pub feature: String,
    pub stat: Stat,
    /// The window, oldest first.
    pub history_ms: Vec<f32>,
}

/// Everything the profiler measured so far.
#[derive(Clone, Debug, Serialize)]
pub struct PerfSnapshot {
    /// Sim ticks folded.
    pub ticks: u64,
    /// Busy time of the `sim_tick` span.
    pub tick: Stat,
    /// Per phase, in first-seen order.
    pub phases: Vec<(String, Stat)>,
    /// Per feature (the summed time of its systems per tick or frame), by name.
    pub features: Vec<(String, Stat)>,
    /// Per system, by name.
    pub systems: Vec<SystemStat>,
    /// Tick times of the last [`WINDOW`] ticks, oldest first.
    pub history_ms: Vec<f32>,
}

/// The feature a system belongs to, from its module path:
/// `fl_feat_<x>::…` → `"<x>"`, `fl_feat_<x>_client::…` → `"<x> (client)"`,
/// `fl_sim::<module>::…` → `"core.<module>"`, `fl_client::<module>::…` → `"client.<module>"`,
/// `bevy::…` and `bevy_*::…` → `"bevy"`, anything else → its crate name.
pub fn feature_of(system_name: &str) -> String {
    let path = system_name.trim_start_matches('<');
    let path = path.split(['<', ' ']).next().unwrap_or(path);
    let segments: Vec<&str> = path.split("::").collect();
    let krate = segments[0];
    // A module only when there is something after it (the system's own name).
    let module = (segments.len() >= 3).then(|| segments[1]);
    let qualified =
        |prefix: &str| module.map_or_else(|| prefix.to_owned(), |m| format!("{prefix}.{m}"));
    if krate == "fl_sim" {
        qualified("core")
    } else if krate == "fl_client" {
        qualified("client")
    } else if let Some(feature) = krate.strip_prefix("fl_feat_") {
        match feature.strip_suffix("_client") {
            Some(client) => format!("{client} (client)"),
            None => feature.to_owned(),
        }
    } else if krate == "bevy" || krate.starts_with("bevy_") {
        "bevy".to_owned()
    } else {
        krate.to_owned()
    }
}

/// One rolling series.
#[derive(Debug, Default)]
struct Series {
    window: VecDeque<f64>,
    samples: u64,
}

impl Series {
    fn push(&mut self, ms: f64) {
        if self.window.len() == WINDOW {
            self.window.pop_front();
        }
        self.window.push_back(ms);
        self.samples += 1;
    }

    fn stat(&self) -> Stat {
        let mut sorted: Vec<f64> = self.window.iter().copied().collect();
        sorted.sort_by(f64::total_cmp);
        stat_of_sorted(&sorted, self.samples)
    }

    fn history(&self) -> Vec<f32> {
        self.window.iter().map(|&ms| ms as f32).collect()
    }
}

fn stat_of_sorted(sorted: &[f64], samples: u64) -> Stat {
    if sorted.is_empty() {
        return Stat {
            samples,
            ..Stat::default()
        };
    }
    let mean_ms = sorted.iter().sum::<f64>() / sorted.len() as f64;
    Stat {
        mean_ms,
        p50_ms: nearest_rank(sorted, 50),
        p95_ms: nearest_rank(sorted, 95),
        max_ms: sorted[sorted.len() - 1],
        samples,
    }
}

/// The nearest-rank `percent`-th percentile of an ascending, non-empty slice: the value at
/// rank `ceil(percent / 100 × n)`.
fn nearest_rank(sorted: &[f64], percent: usize) -> f64 {
    let rank = (percent * sorted.len())
        .div_ceil(100)
        .clamp(1, sorted.len());
    sorted[rank - 1]
}

fn millis(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

/// What one sim tick (or the client's frame pool) accumulates until it folds.
#[derive(Debug, Default)]
struct Pending {
    systems: BTreeMap<String, Duration>,
    phases: Vec<(String, Duration)>,
    open_phases: Vec<(String, Stamp)>,
}

impl Pending {
    fn add_system(&mut self, name: &str, elapsed: Duration) {
        match self.systems.get_mut(name) {
            Some(total) => *total += elapsed,
            None => {
                self.systems.insert(name.to_owned(), elapsed);
            }
        }
    }

    fn phase_edge(&mut self, phase: String, start: bool, now: Stamp) {
        if start {
            self.open_phases.push((phase, now));
            return;
        }
        let Some(at) = self.open_phases.iter().rposition(|(p, _)| *p == phase) else {
            return;
        };
        let (_, started) = self.open_phases.remove(at);
        let elapsed = now.since(started);
        match self.phases.iter_mut().find(|(p, _)| *p == phase) {
            Some((_, total)) => *total += elapsed,
            None => self.phases.push((phase, elapsed)),
        }
    }
}

#[derive(Debug)]
struct SystemSeries {
    feature: String,
    series: Series,
}

/// A budget overrun, logged after the profiler's lock is released.
struct Overrun {
    what: String,
    ms: f64,
    budget_ms: f64,
}

#[derive(Debug, Default)]
struct Inner {
    budgets: Budgets,
    ticks: u64,
    tick: Series,
    phases: Vec<(String, Series)>,
    features: BTreeMap<String, Series>,
    systems: BTreeMap<String, SystemSeries>,
    /// Open sim ticks, by `sim_tick` span id.
    sim_pending: BTreeMap<u64, Pending>,
    /// System runs outside any sim tick, until the next `frame` span closes.
    client_pending: Pending,
    /// Ticks plus frames folded; paces the budget warnings.
    folds: u64,
    /// The fold of each budget's last warning.
    last_warned: BTreeMap<String, u64>,
}

impl Inner {
    /// Folds one tick's or frame's systems; returns the per-feature sums.
    fn fold_systems(&mut self, systems: BTreeMap<String, Duration>) -> BTreeMap<String, f64> {
        let mut features: BTreeMap<String, f64> = BTreeMap::new();
        for (name, elapsed) in systems {
            let ms = millis(elapsed);
            let system = self
                .systems
                .entry(name)
                .or_insert_with_key(|name| SystemSeries {
                    feature: feature_of(name),
                    series: Series::default(),
                });
            system.series.push(ms);
            *features.entry(system.feature.clone()).or_default() += ms;
        }
        for (feature, &ms) in &features {
            self.features.entry(feature.clone()).or_default().push(ms);
        }
        features
    }

    /// An overrun of `budget_ms`, unless this budget already warned in the last [`WINDOW`]
    /// folds.
    fn check(&mut self, what: String, ms: f64, budget_ms: Option<f64>) -> Option<Overrun> {
        let budget_ms = budget_ms.filter(|&budget| ms > budget)?;
        let folds = self.folds;
        let quiet = self
            .last_warned
            .get(&what)
            .is_some_and(|&last| folds - last < WINDOW as u64);
        if quiet {
            return None;
        }
        self.last_warned.insert(what.clone(), folds);
        Some(Overrun {
            what,
            ms,
            budget_ms,
        })
    }

    fn check_features(&mut self, features: &BTreeMap<String, f64>, out: &mut Vec<Overrun>) {
        for (feature, &ms) in features {
            let budget = self.budgets.per_feature_ms.get(feature).copied();
            out.extend(self.check(format!("feature {feature}"), ms, budget));
        }
    }

    fn fold_tick(&mut self, key: u64, busy: Duration) -> Vec<Overrun> {
        let pending = self.sim_pending.remove(&key).unwrap_or_default();
        self.ticks += 1;
        self.folds += 1;
        let mut overruns = Vec::new();

        let tick_ms = millis(busy);
        self.tick.push(tick_ms);
        let budget = Some(self.budgets.tick_ms);
        overruns.extend(self.check("tick".to_owned(), tick_ms, budget));

        for (phase, elapsed) in pending.phases {
            let ms = millis(elapsed);
            let budget = self.budgets.per_phase_ms.get(&phase).copied();
            overruns.extend(self.check(format!("phase {phase}"), ms, budget));
            match self.phases.iter_mut().find(|(p, _)| *p == phase) {
                Some((_, series)) => series.push(ms),
                None => {
                    let mut series = Series::default();
                    series.push(ms);
                    self.phases.push((phase, series));
                }
            }
        }

        let features = self.fold_systems(pending.systems);
        self.check_features(&features, &mut overruns);
        overruns
    }

    fn fold_frame(&mut self) -> Vec<Overrun> {
        let pending = std::mem::take(&mut self.client_pending);
        self.folds += 1;
        let features = self.fold_systems(pending.systems);
        let mut overruns = Vec::new();
        self.check_features(&features, &mut overruns);
        overruns
    }
}

fn warn_overruns(overruns: Vec<Overrun>) {
    for o in overruns {
        tracing::warn!(
            target: BUDGET_TARGET,
            budget = %o.what,
            ms = o.ms,
            budget_ms = o.budget_ms,
            "{} over budget: {:.3} ms > {:.3} ms",
            o.what,
            o.ms,
            o.budget_ms
        );
    }
}

/// Collects timings from a [`ProfilerLayer`]. One lives for the whole process
/// ([`global_profiler`]); tests make their own with [`Profiler::new`].
#[derive(Debug)]
pub struct Profiler {
    /// Tells this profiler's open ticks apart in the thread-local stack.
    id: u64,
    inner: Mutex<Inner>,
}

impl Default for Profiler {
    fn default() -> Self {
        Self::new()
    }
}

impl Profiler {
    /// An empty profiler with the default [`Budgets`].
    pub fn new() -> Profiler {
        static NEXT_ID: AtomicU64 = AtomicU64::new(0);
        Profiler {
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            inner: Mutex::new(Inner::default()),
        }
    }

    /// The tracing layer that feeds this profiler.
    pub fn layer(&'static self) -> ProfilerLayer {
        ProfilerLayer { profiler: self }
    }

    /// Replaces the budgets; every budget may warn again at once.
    pub fn set_budgets(&self, b: Budgets) {
        let mut inner = self.lock();
        inner.budgets = b;
        inner.last_warned.clear();
    }

    /// The statistics so far.
    pub fn snapshot(&self) -> PerfSnapshot {
        let inner = self.lock();
        let named = |(name, series): (&String, &Series)| (name.clone(), series.stat());
        PerfSnapshot {
            ticks: inner.ticks,
            tick: inner.tick.stat(),
            phases: inner.phases.iter().map(|(n, s)| named((n, s))).collect(),
            features: inner.features.iter().map(named).collect(),
            systems: inner
                .systems
                .iter()
                .map(|(name, system)| SystemStat {
                    name: name.clone(),
                    feature: system.feature.clone(),
                    stat: system.series.stat(),
                    history_ms: system.series.history(),
                })
                .collect(),
            history_ms: inner.tick.history(),
        }
    }

    /// Forgets every statistic; keeps the budgets and the ticks in flight.
    pub fn reset(&self) {
        let mut inner = self.lock();
        let budgets = std::mem::take(&mut inner.budgets);
        let sim_pending = std::mem::take(&mut inner.sim_pending);
        let client_pending = std::mem::take(&mut inner.client_pending);
        *inner = Inner {
            budgets,
            sim_pending,
            client_pending,
            ..Inner::default()
        };
    }

    /// The lock; a panic while it was held loses at most one sample, so poison is ignored.
    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// The process-wide profiler that [`init_headless`](crate::init_headless) installs.
pub fn global_profiler() -> &'static Profiler {
    static GLOBAL: OnceLock<Profiler> = OnceLock::new();
    GLOBAL.get_or_init(Profiler::new)
}

thread_local! {
    /// The `sim_tick` spans entered on this thread, innermost last, with their profiler's id.
    static OPEN_TICKS: RefCell<Vec<(u64, Id)>> = const { RefCell::new(Vec::new()) };
}

/// What the profiler keeps in a span's extensions.
#[derive(Debug)]
enum Timed {
    System {
        name: String,
        entered: Option<Stamp>,
    },
    SimTick {
        entered: Option<Stamp>,
        busy: Duration,
    },
    Frame,
}

/// The string fields the profiler reads: a system's `name`, a marker's `phase` and `edge`.
#[derive(Default)]
struct Fields {
    name: Option<String>,
    phase: Option<String>,
    edge: Option<String>,
}

impl Fields {
    fn slot(&mut self, field: &Field) -> Option<&mut Option<String>> {
        match field.name() {
            "name" => Some(&mut self.name),
            "phase" => Some(&mut self.phase),
            "edge" => Some(&mut self.edge),
            _ => None,
        }
    }
}

impl Visit for Fields {
    fn record_str(&mut self, field: &Field, value: &str) {
        if let Some(slot) = self.slot(field) {
            *slot = Some(value.to_owned());
        }
    }

    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        if let Some(slot) = self.slot(field) {
            *slot = Some(format!("{value:?}").trim_matches('"').to_owned());
        }
    }
}

/// The tracing layer of a [`Profiler`]; see the [module docs](self).
#[derive(Clone, Copy, Debug)]
pub struct ProfilerLayer {
    profiler: &'static Profiler,
}

impl ProfilerLayer {
    /// The innermost `sim_tick` span of this profiler entered on this thread.
    fn open_tick(&self) -> Option<u64> {
        OPEN_TICKS.with_borrow(|open| {
            open.iter()
                .rev()
                .find(|(owner, _)| *owner == self.profiler.id)
                .map(|(_, id)| id.into_u64())
        })
    }
}

impl<S> Layer<S> for ProfilerLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        let timed = match attrs.metadata().name() {
            "system" => {
                let mut fields = Fields::default();
                attrs.record(&mut fields);
                let Some(name) = fields.name else { return };
                Timed::System {
                    name,
                    entered: None,
                }
            }
            "sim_tick" => Timed::SimTick {
                entered: None,
                busy: Duration::ZERO,
            },
            "frame" => Timed::Frame,
            _ => return,
        };
        if let Some(span) = ctx.span(id) {
            span.extensions_mut().insert(timed);
        }
    }

    fn on_enter(&self, id: &Id, ctx: Context<'_, S>) {
        let Some(span) = ctx.span(id) else { return };
        let mut extensions = span.extensions_mut();
        match extensions.get_mut::<Timed>() {
            Some(Timed::System { entered, .. }) => *entered = Some(Stamp::now()),
            Some(Timed::SimTick { entered, .. }) => {
                OPEN_TICKS.with_borrow_mut(|open| open.push((self.profiler.id, id.clone())));
                *entered = Some(Stamp::now());
            }
            Some(Timed::Frame) | None => {}
        }
    }

    fn on_exit(&self, id: &Id, ctx: Context<'_, S>) {
        let now = Stamp::now();
        let Some(span) = ctx.span(id) else { return };
        let mut extensions = span.extensions_mut();
        match extensions.get_mut::<Timed>() {
            Some(Timed::System { name, entered }) => {
                let Some(start) = entered.take() else { return };
                let elapsed = now.since(start);
                let tick = self.open_tick();
                let mut inner = self.profiler.lock();
                let pending = match tick {
                    Some(tick) => inner.sim_pending.entry(tick).or_default(),
                    None => &mut inner.client_pending,
                };
                pending.add_system(name, elapsed);
            }
            Some(Timed::SimTick { entered, busy }) => {
                if let Some(start) = entered.take() {
                    *busy += now.since(start);
                }
                OPEN_TICKS.with_borrow_mut(|open| {
                    let me = (self.profiler.id, id.clone());
                    if let Some(at) = open.iter().rposition(|entry| *entry == me) {
                        open.remove(at);
                    }
                });
            }
            Some(Timed::Frame) | None => {}
        }
    }

    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        if event.metadata().target() != PHASE_TARGET {
            return;
        }
        let now = Stamp::now();
        let mut fields = Fields::default();
        event.record(&mut fields);
        let (Some(phase), Some(edge)) = (fields.phase, fields.edge) else {
            return;
        };
        let start = match edge.as_str() {
            "start" => true,
            "end" => false,
            _ => return,
        };
        let Some(tick) = self.open_tick() else { return };
        let mut inner = self.profiler.lock();
        inner
            .sim_pending
            .entry(tick)
            .or_default()
            .phase_edge(phase, start, now);
    }

    fn on_close(&self, id: Id, ctx: Context<'_, S>) {
        let Some(span) = ctx.span(&id) else { return };
        let timed = span.extensions_mut().remove::<Timed>();
        let overruns = match timed {
            Some(Timed::SimTick { busy, .. }) => {
                self.profiler.lock().fold_tick(id.into_u64(), busy)
            }
            Some(Timed::Frame) => self.profiler.lock().fold_frame(),
            Some(Timed::System { .. }) | None => return,
        };
        // After the lock is released: the warning goes through the whole subscriber.
        warn_overruns(overruns);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentiles_nearest_rank() {
        let mut series = Series::default();
        // Out of order on purpose: the stat sorts.
        for ms in (1..=100).rev() {
            series.push(f64::from(ms));
        }
        let stat = series.stat();
        assert!((stat.p50_ms - 50.0).abs() < f64::EPSILON, "{stat:?}");
        assert!((stat.p95_ms - 95.0).abs() < f64::EPSILON, "{stat:?}");
        assert!((stat.max_ms - 100.0).abs() < f64::EPSILON);
        assert!((stat.mean_ms - 50.5).abs() < 1e-9);
        assert_eq!(stat.samples, 100);

        assert!((nearest_rank(&[7.0], 50) - 7.0).abs() < f64::EPSILON);
        assert!((nearest_rank(&[1.0, 2.0], 50) - 1.0).abs() < f64::EPSILON);
        assert!((nearest_rank(&[1.0, 2.0], 95) - 2.0).abs() < f64::EPSILON);
        assert_eq!(Series::default().stat(), Stat::default());
    }

    #[test]
    fn window_keeps_the_last_240() {
        let mut series = Series::default();
        for ms in 0..300 {
            series.push(f64::from(ms));
        }
        let stat = series.stat();
        assert_eq!(stat.samples, 300);
        assert!((stat.max_ms - 299.0).abs() < f64::EPSILON);
        let history = series.history();
        assert_eq!(history.len(), WINDOW);
        assert!((history[0] - 60.0).abs() < f32::EPSILON);
    }

    #[test]
    fn feature_of_edge_cases() {
        assert_eq!(feature_of("fl_sim::f"), "core");
        assert_eq!(feature_of("fl_client::f"), "client");
        assert_eq!(feature_of("bevy::x"), "bevy");
        assert_eq!(
            feature_of("fl_sim::movement::f<fl_feat_x::T>"),
            "core.movement"
        );
        assert_eq!(feature_of("apply_deferred"), "apply_deferred");
    }
}
