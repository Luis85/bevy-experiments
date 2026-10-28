//! Legacy mechanic accounting plus bounded, execution-time trace ancestry.
use crate::collector::{Inner, Kind, Slot};
use crate::tree::TraceSlot;
use crate::{PerfDomain, TraceId, TraceKind};
use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex};
use tracing::{
    Subscriber,
    field::{Field, Visit},
    span::{Attributes, Id},
};
use tracing_subscriber::{
    layer::{Context, Layer},
    registry::LookupSpan,
};

pub const SYSTEM_SPAN: &str = "system";
pub const MECHANIC_SPAN: &str = "mechanic";
/// Additional measured scope: `info_span!("perf_scope", name="work", domain="simulation")`.
pub const SCOPE_SPAN: &str = "perf_scope";

#[derive(Clone, Debug)]
pub struct PerfLayer {
    inner: Arc<Inner>,
}
impl PerfLayer {
    pub(crate) fn new(inner: Arc<Inner>) -> Self {
        Self { inner }
    }
}

#[derive(Default)]
struct Paths {
    generation: u64,
    // Fresh mechanic spans normally have one parent; avoid a hash allocation per call.
    first: Option<(Option<TraceId>, Arc<TraceSlot>)>,
    nodes: HashMap<Option<TraceId>, Arc<TraceSlot>>,
}
struct SpanInfo {
    kind: TraceKind,
    name: Arc<str>,
    legacy: Option<Arc<Slot>>,
    tagged: Option<Arc<Slot>>,
    domain: Option<PerfDomain>,
    mechanic: Option<Arc<str>>,
    paths: Mutex<Paths>,
}
struct Active {
    collector: u64,
    id: u64,
    generation: u64,
    started: u64,
    children: u64,
    legacy: Option<Arc<Slot>>,
    trace: Option<Arc<TraceSlot>>,
}
thread_local! { static ACTIVE:RefCell<Vec<Active>>=const { RefCell::new(Vec::new()) }; }

struct NameVisitor<'a> {
    inner: &'a Inner,
    kind: Option<Kind>,
    slot: Option<Arc<Slot>>,
    name: Option<Arc<str>>,
    domain: Option<PerfDomain>,
    mechanic: Option<Arc<str>>,
}
impl Visit for NameVisitor<'_> {
    fn record_str(&mut self, field: &Field, value: &str) {
        match field.name() {
            "name" => {
                self.slot = self.kind.map(|kind| self.inner.slot(kind, value));
                self.name = Some(
                    self.slot
                        .as_ref()
                        .map_or_else(|| Arc::from(value), |s| s.name.clone()),
                );
            }
            "domain" => self.domain = PerfDomain::parse(value),
            "mechanic" => self.mechanic = Some(value.into()),
            _ => {}
        }
    }
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        if matches!(field.name(), "name" | "domain" | "mechanic") {
            let formatted = format!("{value:?}");
            let text = formatted
                .strip_prefix('"')
                .and_then(|s| s.strip_suffix('"'))
                .unwrap_or(&formatted);
            self.record_str(field, text);
        }
    }
}
impl<S> Layer<S> for PerfLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        let kind = match attrs.metadata().name() {
            SYSTEM_SPAN => TraceKind::System,
            "system_commands" => TraceKind::Deferred,
            "check_conditions" => TraceKind::Conditions,
            MECHANIC_SPAN => TraceKind::Mechanic,
            SCOPE_SPAN => TraceKind::Scope,
            "schedule" => TraceKind::Tick,
            _ => return,
        };
        let mut visitor = NameVisitor {
            inner: &self.inner,
            kind: match kind {
                TraceKind::System | TraceKind::Deferred | TraceKind::Conditions => {
                    Some(Kind::System)
                }
                TraceKind::Mechanic => Some(Kind::Mechanic),
                _ => None,
            },
            slot: None,
            name: None,
            domain: None,
            mechanic: None,
        };
        attrs.record(&mut visitor);
        let (Some(mut name), Some(span)) = (visitor.name, ctx.span(id)) else {
            return;
        };
        if kind == TraceKind::Tick {
            if !name.contains("NpcSimTick") {
                return;
            }
            name = Arc::from("NpcSimTick");
        }
        let tagged = visitor.slot.clone();
        let mut legacy = if matches!(kind, TraceKind::System | TraceKind::Mechanic) {
            visitor.slot
        } else {
            None
        };
        if kind == TraceKind::Mechanic {
            let ancestor = span.scope().skip(1).find_map(|s| {
                s.extensions()
                    .get::<SpanInfo>()
                    .and_then(|info| info.legacy.as_ref().map(|slot| slot.mechanic()))
            });
            if ancestor.is_some_and(|m| m.as_ref() == name.as_ref()) {
                legacy = None;
            }
        }
        span.extensions_mut().insert(SpanInfo {
            kind,
            name,
            legacy,
            tagged,
            domain: visitor.domain,
            mechanic: visitor.mechanic,
            paths: Mutex::new(Paths::default()),
        });
    }
    fn on_enter(&self, id: &Id, ctx: Context<'_, S>) {
        let Some(span) = ctx.span(id) else {
            return;
        };
        let ext = span.extensions();
        let Some(info) = ext.get::<SpanInfo>() else {
            return;
        };
        // Keep path creation in the same epoch as parent lookup. A concurrent reset
        // must not insert an old parent ID into the freshly cleared tree.
        let _epoch = self
            .inner
            .recording
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let generation = self.inner.traces.generation();
        ACTIVE.with(|active| {
            let mut active = active.borrow_mut();
            let ancestor = active
                .iter()
                .rev()
                .find(|a| a.collector == self.inner.id && a.generation == generation);
            let omitted = ancestor.is_some_and(|a| a.trace.is_none());
            let parent = ancestor.and_then(|a| a.trace.as_ref());
            let parent_id = parent.map(|p| p.id);
            let mut paths = info
                .paths
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if paths.generation != generation {
                paths.first = None;
                paths.nodes.clear();
                paths.generation = generation;
            }
            let cached = paths
                .first
                .as_ref()
                .filter(|(parent, _)| *parent == parent_id)
                .map(|(_, node)| node.clone())
                .or_else(|| paths.nodes.get(&parent_id).cloned());
            let trace = if omitted {
                self.inner.traces.note_drop();
                None
            } else {
                cached.or_else(|| {
                    let mechanic = match info.kind {
                        TraceKind::System | TraceKind::Deferred | TraceKind::Conditions => info
                            .tagged
                            .as_ref()
                            .map(|s| s.mechanic())
                            .unwrap_or_else(|| Arc::from("untagged")),
                        TraceKind::Mechanic => info.name.clone(),
                        TraceKind::Tick => Arc::from("driver.tick"),
                        TraceKind::Scope => info
                            .mechanic
                            .clone()
                            .or_else(|| parent.map(|p| p.mechanic.clone()))
                            .unwrap_or_else(|| info.name.clone()),
                    };
                    let node = self.inner.traces.slot(
                        info.kind,
                        info.name.clone(),
                        mechanic,
                        info.domain,
                        parent,
                    );
                    if let Some(node) = &node {
                        if paths.first.is_none() {
                            paths.first = Some((parent_id, node.clone()));
                        } else {
                            paths.nodes.insert(parent_id, node.clone());
                        }
                    }
                    node
                })
            };
            active.push(Active {
                collector: self.inner.id,
                id: id.into_u64(),
                generation,
                started: self.inner.now_ns(),
                children: 0,
                legacy: info.legacy.clone(),
                trace,
            });
        });
    }
    fn on_exit(&self, id: &Id, _ctx: Context<'_, S>) {
        let end = self.inner.now_ns();
        let entry = ACTIVE.with(|active| {
            let mut active = active.borrow_mut();
            let index = active
                .iter()
                .rposition(|a| a.collector == self.inner.id && a.id == id.into_u64())?;
            let entry = active.remove(index);
            let inclusive = end.saturating_sub(entry.started);
            if entry.trace.is_some()
                && let Some(parent) = active
                    .iter_mut()
                    .rev()
                    .find(|a| a.collector == self.inner.id && a.generation == entry.generation)
            {
                parent.children = parent.children.saturating_add(inclusive);
            }
            Some((entry, inclusive))
        });
        let Some((entry, inclusive)) = entry else {
            return;
        };
        let _epoch = self
            .inner
            .recording
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if entry.generation != self.inner.traces.generation() {
            return;
        }
        if let Some(slot) = entry.legacy {
            slot.record(inclusive);
        }
        if let Some(trace) = entry.trace {
            trace.record(inclusive, inclusive.saturating_sub(entry.children));
            if trace.kind == TraceKind::Tick {
                self.inner.traces.tick(inclusive);
            }
        }
    }
}
