//! The view format (spec §5.3, §12.7, ADR-0002, ADR-0011): the serde-reflection formats of
//! the snapshot, its entity records, the events and the tick input, pinned by an insta snapshot
//! whose first line is [`VIEW_FORMAT_VERSION`].
//!
//! [`JournalEntry`](fl_view::JournalEntry) and [`SimReply`](fl_view::SimReply) are not traced:
//! they carry `serde_json::Value`, which is self-describing (`deserialize_any`), so it has no
//! static format. [`TickOutput`](fl_view::TickOutput) and the runner messages are never
//! serialized.
//!
//! The guard reads the committed snapshot: a type whose format changed while
//! `VIEW_FORMAT_VERSION` stayed the same fails before the snapshot diff, which then needs a
//! bless.

use std::fmt::Write as _;

use fl_view::{EntityRecord, SimEvent, SimSnapshot, TickInput, VIEW_FORMAT_VERSION};
use serde::de::DeserializeOwned;
use serde_reflection::{Tracer, TracerConfig};

#[path = "../../fl_protocol/tests/support/format_guard.rs"]
mod format_guard;

use format_guard::{committed, guard};

fn trace<T: DeserializeOwned>(tracer: &mut Tracer) {
    tracer
        .trace_simple_type::<T>()
        .unwrap_or_else(|err| panic!("{} traces: {err}", std::any::type_name::<T>()));
}

/// `VIEW_FORMAT_VERSION = n`, then one `<Type>: <format>` line per named type, in name order.
fn view_formats_text() -> String {
    let mut tracer = Tracer::new(TracerConfig::default());
    trace::<SimEvent>(&mut tracer);
    trace::<EntityRecord>(&mut tracer);
    trace::<SimSnapshot>(&mut tracer);
    trace::<TickInput>(&mut tracer);
    let registry = tracer.registry().expect("every traced type is complete");
    let mut out = format!("VIEW_FORMAT_VERSION = {VIEW_FORMAT_VERSION}\n");
    for (name, format) in &registry {
        writeln!(out, "{name}: {format:?}").expect("writing to a String");
    }
    out
}

#[test]
fn view_formats() {
    let text = view_formats_text();
    if let Some(committed) = committed("view_formats")
        && let Err(err) = guard(&committed, &text)
    {
        panic!("{err}");
    }
    insta::assert_snapshot!("view_formats", text);
}
