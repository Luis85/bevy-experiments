//! Bounded execution-path statistics. Legacy budget accounting is deliberately separate.
use crate::stats::{Ring, us};
use crate::{DomainTags, PerfDomain, PerfReport};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};

pub type TraceId = u32;
pub const DEFAULT_TRACE_LIMIT: usize = 4096;
pub const MAX_TRACE_DEPTH: u16 = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TraceKind {
    System,
    Deferred,
    Conditions,
    Mechanic,
    Scope,
    Tick,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PerfInterval {
    RenderFrame,
    SimTick,
    #[default]
    Collection,
}
impl PerfInterval {
    pub fn label(self) -> &'static str {
        match self {
            Self::RenderFrame => "render frame",
            Self::SimTick => "simulation tick",
            Self::Collection => "collection interval",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DurationStats {
    pub samples: usize,
    pub mean_us: f64,
    pub p50_us: f64,
    pub p95_us: f64,
    pub max_us: f64,
    pub last_us: f64,
}
impl DurationStats {
    fn of(ring: &Ring, scratch: &mut Vec<u64>) -> Self {
        let s = ring.summary_with(scratch);
        Self {
            samples: ring.len(),
            mean_us: ring.mean() / 1000.0,
            p50_us: us(s.p50),
            p95_us: us(s.p95),
            max_us: us(s.max),
            last_us: us(ring.last()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DomainStats {
    pub domain: PerfDomain,
    /// Sum of measured self time per closed interval; frame wrappers excluded.
    pub work: DurationStats,
    pub total_us: f64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GroupStats {
    pub domain: PerfDomain,
    /// Dot-separated mechanic prefix. Parent series sum raw interval costs, never child percentiles.
    pub mechanic: String,
    pub work: DurationStats,
    pub total_us: f64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TraceNode {
    pub id: TraceId,
    pub parent: Option<TraceId>,
    pub kind: TraceKind,
    pub name: String,
    pub mechanic: String,
    pub domain: PerfDomain,
    /// Inclusive frame wrappers are available for context, not domain attribution.
    pub frame_wrapper: bool,
    pub calls: u64,
    pub inclusive_total_us: f64,
    pub self_total_us: f64,
    /// Rolling closed-interval values, including intervals with no calls.
    pub self_mean_us: f64,
    pub calls_per_interval: f64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TraceDetail {
    pub id: TraceId,
    pub inclusive_per_call: DurationStats,
    pub self_per_call: DurationStats,
    pub self_per_interval: DurationStats,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BudgetStatus {
    pub mechanic: String,
    pub budget_us: u64,
    pub observed_p95_us: Option<f64>,
    pub samples: u64,
    pub ready: bool,
    pub over_budget: bool,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RuntimeStats {
    pub frame_fps: f64,
    pub frame_ms: f64,
    pub sim_tps: Option<f64>,
    pub sim_target_tps: Option<f64>,
    pub sim_hz: Option<u32>,
    pub multiplier: Option<u32>,
    pub paused: bool,
    pub frame_target_us: u64,
    pub driver_cap: Option<u32>,
    pub pacing: Option<String>,
    pub host_time_scale: Option<f64>,
}
/// A diagnostic frame target; it never caps rendering or changes simulation pacing.
#[derive(bevy_ecs::prelude::Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct PerfTargets {
    pub frame_budget_us: u64,
}
impl Default for PerfTargets {
    fn default() -> Self {
        Self {
            frame_budget_us: 16_667,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PerfTreeReport {
    pub frames: u64,
    pub generation: u64,
    pub window: usize,
    pub min_samples: u64,
    pub interval: PerfInterval,
    pub domains: Vec<DomainStats>,
    pub groups: Vec<GroupStats>,
    pub nodes: Vec<TraceNode>,
    pub detail: Option<TraceDetail>,
    /// Populated only by an explicit complete capture (Freeze/export).
    pub details: Vec<TraceDetail>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub history: Option<crate::HistoryCapture>,
    pub tick: Option<DurationStats>,
    pub budgets: Vec<BudgetStatus>,
    pub runtime: Option<RuntimeStats>,
    pub node_limit: usize,
    pub dropped_entries: u64,
}

#[derive(Debug)]
pub(crate) struct TraceSlot {
    pub id: TraceId,
    pub parent: Option<TraceId>,
    pub depth: u16,
    pub kind: TraceKind,
    pub name: Arc<str>,
    pub mechanic: Arc<str>,
    pub domain: PerfDomain,
    pub frame_wrapper: bool,
    groups: Vec<Arc<str>>,
    data: Mutex<NodeData>,
}
#[derive(Debug)]
struct NodeData {
    calls: u64,
    inclusive_ns: u64,
    self_ns: u64,
    calls_window: Ring,
    self_window: Ring,
    inclusive_window: Ring,
    frame_window: Ring,
    frame_calls: u64,
    frame_self: u64,
}
impl TraceSlot {
    pub(crate) fn record(&self, inclusive: u64, own: u64) {
        let mut d = lock(&self.data);
        d.calls = d.calls.saturating_add(1);
        d.inclusive_ns = d.inclusive_ns.saturating_add(inclusive);
        d.self_ns = d.self_ns.saturating_add(own);
        d.inclusive_window.push(inclusive);
        d.self_window.push(own);
        d.frame_self = d.frame_self.saturating_add(own);
        d.frame_calls = d.frame_calls.saturating_add(1);
    }
}
#[derive(Debug)]
struct Work {
    ring: Ring,
    total: u64,
}
impl Work {
    fn new(window: usize, previous: u64) -> Self {
        Self {
            ring: zero_ring(window, previous),
            total: 0,
        }
    }
    fn push(&mut self, value: u64) {
        self.ring.push(value);
        self.total = self.total.saturating_add(value);
    }
}
type Key = (Option<TraceId>, TraceKind, Arc<str>);
#[derive(Debug)]
struct Store {
    tags: DomainTags,
    limit: usize,
    map: HashMap<Key, Arc<TraceSlot>>,
    nodes: Vec<Arc<TraceSlot>>,
}
#[derive(Debug)]
struct Frames {
    frames: u64,
    interval: PerfInterval,
    domains: BTreeMap<PerfDomain, Work>,
    groups: BTreeMap<(PerfDomain, Arc<str>), Work>,
    runtime: Option<RuntimeStats>,
}
#[derive(Debug)]
pub(crate) struct TraceCollector {
    window: usize,
    generation: AtomicU64,
    frame_count: AtomicU64,
    dropped: AtomicU64,
    store: RwLock<Store>,
    frames: Mutex<Frames>,
    tick: Mutex<Ring>,
}
impl TraceCollector {
    pub(crate) fn new(window: usize) -> Self {
        Self {
            window,
            generation: AtomicU64::new(1),
            frame_count: AtomicU64::new(0),
            dropped: AtomicU64::new(0),
            store: RwLock::new(Store {
                tags: DomainTags::default(),
                limit: DEFAULT_TRACE_LIMIT,
                map: HashMap::new(),
                nodes: Vec::new(),
            }),
            frames: Mutex::new(Frames {
                frames: 0,
                interval: PerfInterval::Collection,
                domains: BTreeMap::new(),
                groups: BTreeMap::new(),
                runtime: None,
            }),
            tick: Mutex::new(Ring::new(window)),
        }
    }
    pub(crate) fn generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }
    pub(crate) fn note_drop(&self) {
        self.dropped.fetch_add(1, Ordering::Relaxed);
    }
    pub(crate) fn slot(
        &self,
        kind: TraceKind,
        name: Arc<str>,
        mechanic: Arc<str>,
        explicit: Option<PerfDomain>,
        parent: Option<&Arc<TraceSlot>>,
    ) -> Option<Arc<TraceSlot>> {
        let key = (parent.map(|p| p.id), kind, name.clone());
        {
            let store = read(&self.store);
            if let Some(slot) = store.map.get(&key) {
                return Some(slot.clone());
            }
            if store.nodes.len() >= store.limit
                || parent.is_some_and(|p| p.depth >= MAX_TRACE_DEPTH)
            {
                self.dropped.fetch_add(1, Ordering::Relaxed);
                return None;
            }
        }
        let mut store = write(&self.store);
        if let Some(slot) = store.map.get(&key) {
            return Some(slot.clone());
        }
        if store.nodes.len() >= store.limit {
            self.dropped.fetch_add(1, Ordering::Relaxed);
            return None;
        }
        let domain = explicit
            .or_else(|| {
                store.tags.classify(
                    matches!(
                        kind,
                        TraceKind::System | TraceKind::Deferred | TraceKind::Conditions
                    )
                    .then_some(&*name),
                    &mechanic,
                )
            })
            .or_else(|| {
                (!matches!(
                    kind,
                    TraceKind::System | TraceKind::Deferred | TraceKind::Conditions
                ))
                .then(|| parent.map(|p| p.domain))
                .flatten()
            })
            .unwrap_or_default();
        let before = self.frame_count.load(Ordering::Relaxed);
        let mut groups = Vec::new();
        let mut prefix = String::new();
        for piece in mechanic.split('.') {
            if !prefix.is_empty() {
                prefix.push('.');
            }
            prefix.push_str(piece);
            groups.push(Arc::from(prefix.as_str()));
        }
        let slot = Arc::new(TraceSlot {
            id: store.nodes.len() as u32,
            parent: key.0,
            depth: parent.map_or(0, |p| p.depth + 1),
            kind,
            name,
            mechanic,
            domain,
            frame_wrapper: kind == TraceKind::System
                && key
                    .2
                    .trim_start_matches(['<', '&', ' '])
                    .starts_with("bevy_app::main_schedule"),
            groups,
            data: Mutex::new(NodeData {
                calls: 0,
                inclusive_ns: 0,
                self_ns: 0,
                calls_window: zero_ring(self.window, before),
                self_window: Ring::new(self.window),
                inclusive_window: Ring::new(self.window),
                frame_window: zero_ring(self.window, before),
                frame_calls: 0,
                frame_self: 0,
            }),
        });
        store.map.insert(key, slot.clone());
        store.nodes.push(slot.clone());
        Some(slot)
    }
    pub(crate) fn tick(&self, duration: u64) {
        lock(&self.tick).push(duration);
    }
    pub(crate) fn end_frame(&self) {
        let mut frames = lock(&self.frames);
        let mut domains: BTreeMap<PerfDomain, u64> = BTreeMap::new();
        let mut groups: BTreeMap<(PerfDomain, Arc<str>), u64> = BTreeMap::new();
        for slot in &read(&self.store).nodes {
            let mut d = lock(&slot.data);
            let (value, calls) = (d.frame_self, d.frame_calls);
            d.frame_window.push(value);
            d.calls_window.push(calls);
            d.frame_self = 0;
            d.frame_calls = 0;
            if slot.frame_wrapper || calls == 0 {
                continue;
            }
            *domains.entry(slot.domain).or_default() += value;
            for prefix in &slot.groups {
                *groups.entry((slot.domain, prefix.clone())).or_default() += value;
            }
        }
        let before = frames.frames;
        for domain in domains.keys() {
            frames
                .domains
                .entry(*domain)
                .or_insert_with(|| Work::new(self.window, before));
        }
        for (domain, work) in &mut frames.domains {
            work.push(domains.get(domain).copied().unwrap_or(0));
        }
        for key in groups.keys() {
            frames
                .groups
                .entry(key.clone())
                .or_insert_with(|| Work::new(self.window, before));
        }
        for (key, work) in &mut frames.groups {
            work.push(groups.get(key).copied().unwrap_or(0));
        }
        frames.frames = frames.frames.saturating_add(1);
        self.frame_count.store(frames.frames, Ordering::Relaxed);
    }
    pub(crate) fn report(
        &self,
        selected: Option<TraceId>,
        complete: bool,
        flat: &PerfReport,
        budgets: &crate::PerfBudgets,
        min: u64,
    ) -> PerfTreeReport {
        let frames = lock(&self.frames);
        let store = read(&self.store);
        let mut scratch = Vec::with_capacity(self.window);
        let domains = frames
            .domains
            .iter()
            .map(|(&domain, w)| DomainStats {
                domain,
                work: DurationStats::of(&w.ring, &mut scratch),
                total_us: us(w.total),
            })
            .collect();
        let groups = frames
            .groups
            .iter()
            .map(|((domain, mechanic), w)| GroupStats {
                domain: *domain,
                mechanic: mechanic.to_string(),
                work: DurationStats::of(&w.ring, &mut scratch),
                total_us: us(w.total),
            })
            .collect();
        let mut nodes = Vec::new();
        let mut detail = None;
        let mut details = Vec::new();
        for slot in &store.nodes {
            let d = lock(&slot.data);
            if d.calls == 0 {
                continue;
            }
            nodes.push(TraceNode {
                id: slot.id,
                parent: slot.parent,
                kind: slot.kind,
                name: slot.name.to_string(),
                mechanic: slot.mechanic.to_string(),
                domain: slot.domain,
                frame_wrapper: slot.frame_wrapper,
                calls: d.calls,
                inclusive_total_us: us(d.inclusive_ns),
                self_total_us: us(d.self_ns),
                self_mean_us: d.frame_window.mean() / 1000.0,
                calls_per_interval: d.calls_window.mean(),
            });
            if complete || selected == Some(slot.id) {
                let value = TraceDetail {
                    id: slot.id,
                    inclusive_per_call: DurationStats::of(&d.inclusive_window, &mut scratch),
                    self_per_call: DurationStats::of(&d.self_window, &mut scratch),
                    self_per_interval: DurationStats::of(&d.frame_window, &mut scratch),
                };
                if selected == Some(slot.id) {
                    detail = Some(value.clone());
                }
                if complete {
                    details.push(value);
                }
            }
        }
        let tick = lock(&self.tick);
        let budgets = budgets
            .iter()
            .map(|(mechanic, budget_us)| {
                let observed = flat.mechanic(mechanic);
                BudgetStatus {
                    mechanic: mechanic.into(),
                    budget_us,
                    observed_p95_us: observed.map(|m| m.p95_us),
                    samples: observed.map_or(0, |m| m.samples),
                    ready: observed.is_some_and(|m| m.samples >= min),
                    over_budget: observed.is_some_and(|m| m.over_budget),
                }
            })
            .collect();
        PerfTreeReport {
            frames: frames.frames,
            generation: self.generation(),
            window: self.window,
            min_samples: min,
            interval: frames.interval,
            domains,
            groups,
            nodes,
            detail,
            details,
            history: None,
            tick: (tick.len() > 0).then(|| DurationStats::of(&tick, &mut scratch)),
            budgets,
            runtime: frames.runtime.clone(),
            node_limit: store.limit,
            dropped_entries: self.dropped.load(Ordering::Relaxed),
        }
    }
    pub(crate) fn reset(&self) {
        self.reset_with(|_| {});
    }
    fn reset_with(&self, change: impl FnOnce(&mut Store)) {
        let mut frames = lock(&self.frames);
        let mut store = write(&self.store);
        change(&mut store);
        self.generation.fetch_add(1, Ordering::AcqRel);
        store.nodes.clear();
        store.map.clear();
        frames.frames = 0;
        frames.domains.clear();
        frames.groups.clear();
        self.frame_count.store(0, Ordering::Relaxed);
        self.dropped.store(0, Ordering::Relaxed);
        lock(&self.tick).clear();
    }
    pub(crate) fn set_domains(&self, tags: DomainTags) {
        self.reset_with(|store| store.tags = tags);
    }
    pub(crate) fn set_limit(&self, limit: usize) {
        self.reset_with(|store| store.limit = limit.clamp(1, 65_536));
    }
    pub(crate) fn set_interval(&self, interval: PerfInterval) {
        lock(&self.frames).interval = interval;
    }
    pub(crate) fn set_runtime(&self, runtime: RuntimeStats) {
        lock(&self.frames).runtime = Some(runtime);
    }
}
fn zero_ring(window: usize, previous: u64) -> Ring {
    let mut r = Ring::new(window);
    for _ in 0..previous.min(window as u64) {
        r.push(0);
    }
    r
}
fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}
fn read<T>(m: &RwLock<T>) -> std::sync::RwLockReadGuard<'_, T> {
    m.read().unwrap_or_else(std::sync::PoisonError::into_inner)
}
fn write<T>(m: &RwLock<T>) -> std::sync::RwLockWriteGuard<'_, T> {
    m.write().unwrap_or_else(std::sync::PoisonError::into_inner)
}
