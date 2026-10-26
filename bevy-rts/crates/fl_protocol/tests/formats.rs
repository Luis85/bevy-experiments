//! The wire format (spec §12.7, ADR-0007, ADR-0011): the serde-reflection formats of the
//! command envelope, the core and `debug.*` payloads, the [`MatchHeader`] and its
//! [`StringTables`], pinned by an insta snapshot whose first line is [`FORMAT_VERSION`].
//!
//! The guard reads the committed snapshot: a type whose format changed while
//! `FORMAT_VERSION` stayed the same fails before the snapshot diff, which then needs a bless.

use std::fmt::Write as _;

use fl_protocol::{
    CommandKind, DebugInvulnerableCmd, DebugSetHpCmd, DebugSetOwnerCmd, DebugSetTerrainCmd,
    DebugSpawnCmd, DebugTeleportCmd, DebugWanderCmd, FORMAT_VERSION, FollowCmd, MatchHeader,
    MoveCmd, PlayerCommand, RotateCmd, StopCmd, StringTables,
};
use serde::de::DeserializeOwned;
use serde_reflection::{Tracer, TracerConfig};

#[path = "support/format_guard.rs"]
mod format_guard;

use format_guard::{committed, guard};

fn trace<T: DeserializeOwned>(tracer: &mut Tracer) {
    tracer
        .trace_simple_type::<T>()
        .unwrap_or_else(|err| panic!("{} traces: {err}", std::any::type_name::<T>()));
}

/// `FORMAT_VERSION = n`, then one `<Type>: <format>` line per named type, in name order.
fn protocol_formats_text() -> String {
    let mut tracer = Tracer::new(TracerConfig::default());
    trace::<PlayerCommand>(&mut tracer);
    trace::<CommandKind>(&mut tracer);
    trace::<MatchHeader>(&mut tracer);
    trace::<StringTables>(&mut tracer);
    trace::<MoveCmd>(&mut tracer);
    trace::<StopCmd>(&mut tracer);
    trace::<RotateCmd>(&mut tracer);
    trace::<FollowCmd>(&mut tracer);
    trace::<DebugSpawnCmd>(&mut tracer);
    trace::<DebugWanderCmd>(&mut tracer);
    trace::<DebugSetOwnerCmd>(&mut tracer);
    trace::<DebugInvulnerableCmd>(&mut tracer);
    trace::<DebugSetHpCmd>(&mut tracer);
    trace::<DebugTeleportCmd>(&mut tracer);
    trace::<DebugSetTerrainCmd>(&mut tracer);
    let registry = tracer.registry().expect("every traced type is complete");
    let mut out = format!("FORMAT_VERSION = {FORMAT_VERSION}\n");
    for (name, format) in &registry {
        writeln!(out, "{name}: {format:?}").expect("writing to a String");
    }
    out
}

#[test]
fn protocol_formats() {
    let text = protocol_formats_text();
    if let Some(committed) = committed("protocol_formats")
        && let Err(err) = guard(&committed, &text)
    {
        panic!("{err}");
    }
    insta::assert_snapshot!("protocol_formats", text);
}

#[test]
fn format_guard_flags_unbumped_change() {
    let committed = "FORMAT_VERSION = 1\nMoveCmd: A";
    let err = guard(committed, "FORMAT_VERSION = 1\nMoveCmd: B").expect_err("unbumped change");
    assert!(
        err.contains("wire format of MoveCmd changed without a version bump"),
        "{err}"
    );
    guard(committed, "FORMAT_VERSION = 2\nMoveCmd: B").expect("a bumped change passes");
    guard(committed, "FORMAT_VERSION = 1\nMoveCmd: A\nStopCmd: C").expect("an added type passes");
}
