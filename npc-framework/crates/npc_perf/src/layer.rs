//! The `tracing` layer that times `"system"` and `"mechanic"` spans.

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use tracing::Subscriber;
use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id};
use tracing_subscriber::layer::{Context, Layer};
use tracing_subscriber::registry::LookupSpan;

use crate::collector::{Inner, Kind, Slot};

/// Span name of Bevy's per-system spans (`bevy_ecs/trace`).
pub const SYSTEM_SPAN: &str = "system";
/// Span name of custom mechanic spans: `info_span!("mechanic", name = "think.agent")`.
pub const MECHANIC_SPAN: &str = "mechanic";

/// A `tracing_subscriber` layer timing every span named [`SYSTEM_SPAN`] or
/// [`MECHANIC_SPAN`] that has a `name` field, from `on_enter` to `on_exit`.
///
/// - The `name` field is read once in `on_new_span` and the span's stats slot
///   is stored in the span's extensions, so entering and exiting a span (once
///   per system run) costs two clock reads, an extensions read lock and one
///   uncontended mutex, with no allocation or name lookup.
/// - `Send + Sync`: spans may be entered on any worker thread; each span's
///   start time is kept on the span itself (keyed by span id), so concurrent
///   *different* spans never interfere. Re-entering one span while it is
///   already entered (on any thread) is not supported: the inner entry's start
///   overwrites the outer one.
/// - A `"mechanic"` span inside a timed span of the **same** mechanic (the
///   `vehicles` span a `npc_vehicles` system opens, tagged `vehicles`
///   itself) is not timed: its time is already in the enclosing span, and
///   summing both would count the mechanic twice per frame.
/// - It never filters or alters other layers' output.
#[derive(Clone, Debug)]
pub struct PerfLayer {
    inner: Arc<Inner>,
}

impl PerfLayer {
    pub(crate) fn new(inner: Arc<Inner>) -> Self {
        Self { inner }
    }
}

/// Stored in a timed span's extensions.
struct Timed {
    slot: Arc<Slot>,
    /// `Inner::now_ns` at the last `on_enter`.
    start_ns: AtomicU64,
}

/// Resolves the `name` field straight to a stats slot, so the common
/// `&str` case allocates nothing once the slot exists.
struct NameVisitor<'a> {
    inner: &'a Inner,
    kind: Kind,
    slot: Option<Arc<Slot>>,
}

impl Visit for NameVisitor<'_> {
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "name" {
            self.slot = Some(self.inner.slot(self.kind, value));
        }
    }

    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        if field.name() == "name" && self.slot.is_none() {
            let s = format!("{value:?}");
            let name = s
                .strip_prefix('"')
                .and_then(|s| s.strip_suffix('"'))
                .unwrap_or(&s);
            self.slot = Some(self.inner.slot(self.kind, name));
        }
    }
}

impl<S> Layer<S> for PerfLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        let kind = match attrs.metadata().name() {
            SYSTEM_SPAN => Kind::System,
            MECHANIC_SPAN => Kind::Mechanic,
            _ => return,
        };
        let mut visitor = NameVisitor {
            inner: &self.inner,
            kind,
            slot: None,
        };
        attrs.record(&mut visitor);
        let (Some(slot), Some(span)) = (visitor.slot, ctx.span(id)) else {
            return;
        };
        if kind == Kind::Mechanic {
            // The nearest timed ancestor already counts this mechanic.
            let enclosing = span
                .scope()
                .skip(1)
                .find_map(|s| s.extensions().get::<Timed>().map(|t| t.slot.mechanic()));
            if enclosing.is_some_and(|m| m == slot.mechanic()) {
                return;
            }
        }
        span.extensions_mut().insert(Timed {
            slot,
            start_ns: AtomicU64::new(0),
        });
    }

    fn on_enter(&self, id: &Id, ctx: Context<'_, S>) {
        if let Some(span) = ctx.span(id)
            && let Some(timed) = span.extensions().get::<Timed>()
        {
            timed.start_ns.store(self.inner.now_ns(), Ordering::Relaxed);
        }
    }

    fn on_exit(&self, id: &Id, ctx: Context<'_, S>) {
        // Read the clock before the span lookup so the lookup is not timed.
        let end = self.inner.now_ns();
        if let Some(span) = ctx.span(id)
            && let Some(timed) = span.extensions().get::<Timed>()
        {
            let start = timed.start_ns.load(Ordering::Relaxed);
            timed.slot.record(end.saturating_sub(start));
        }
    }
}
