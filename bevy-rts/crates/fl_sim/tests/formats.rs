//! The save and replay formats (spec §5.8, §12.7, ADR-0006, ADR-0007, ADR-0011): the
//! serde-reflection formats of the [`SaveFile`] layout and the [`Replay`], pinned by an insta
//! snapshot. `SAVE_FORMAT_VERSION = n`, `REPLAY_FORMAT_VERSION = n` and
//! `SCALE_SCENARIO_VERSION = n` (the replay's versioned scale params) head their types. The
//! codec of every `Hashed` component and resource of every catalog feature is pinned by
//! `fl_test_utils/tests/formats.rs`.
//!
//! The guard reads the committed snapshot: a type whose format changed while its version stayed
//! the same fails before the snapshot diff, which then needs a bless.

use std::fmt::Write as _;

use fl_sim::{
    REPLAY_FORMAT_VERSION, Replay, ReplayEvent, ReplayStart, SAVE_FORMAT_VERSION, SaveFile,
    ScaleScenario, SemanticCheck,
};
use serde::de::DeserializeOwned;
use serde_reflection::{Registry, Tracer, TracerConfig};

#[path = "../../fl_protocol/tests/support/format_guard.rs"]
mod format_guard;

use format_guard::{committed, guard};

fn trace<T: DeserializeOwned>(tracer: &mut Tracer) {
    tracer
        .trace_simple_type::<T>()
        .unwrap_or_else(|err| panic!("{} traces: {err}", std::any::type_name::<T>()));
}

/// `header`, then one `<Type>: <format>` line per named type of `registry`, in name order.
fn section(out: &mut String, header: &str, registry: &Registry) {
    writeln!(out, "{header}").expect("writing to a String");
    for (name, format) in registry {
        writeln!(out, "{name}: {format:?}").expect("writing to a String");
    }
}

/// The save layout and the replay.
fn save_formats_text() -> String {
    let mut save = Tracer::new(TracerConfig::default());
    trace::<SaveFile>(&mut save);
    // Enums nested in `Replay` are traced first: a nested enum gets one variant per trace.
    let mut replay = Tracer::new(TracerConfig::default());
    trace::<ReplayStart>(&mut replay);
    trace::<ReplayEvent>(&mut replay);
    trace::<SemanticCheck>(&mut replay);
    trace::<Replay>(&mut replay);
    // A replay's scale start holds `ScaleScenario::encode`'s bytes, versioned on their own
    // (ADR-0021, M10): a field change must bump `ScaleScenario::VERSION`.
    let mut scale = Tracer::new(TracerConfig::default());
    trace::<ScaleScenario>(&mut scale);

    let mut out = String::new();
    section(
        &mut out,
        &format!("SAVE_FORMAT_VERSION = {SAVE_FORMAT_VERSION}"),
        &save.registry().expect("the save layout is complete"),
    );
    section(
        &mut out,
        &format!("REPLAY_FORMAT_VERSION = {REPLAY_FORMAT_VERSION}"),
        &replay.registry().expect("the replay is complete"),
    );
    section(
        &mut out,
        &format!("SCALE_SCENARIO_VERSION = {}", ScaleScenario::VERSION),
        &scale.registry().expect("the scale scenario is complete"),
    );
    out
}

/// A `ScaleScenario` field change without a `ScaleScenario::VERSION` bump fails the guard; with
/// the bump it passes (M10: the scale params are versioned apart from the replay format).
#[test]
fn scale_scenario_change_needs_a_version_bump() {
    let text = save_formats_text();
    let version = format!("SCALE_SCENARIO_VERSION = {}", ScaleScenario::VERSION);
    let (_, scale_section) = text.split_once(&version).expect("the scale section");
    assert!(scale_section.contains("\nScaleScenario: "), "{text}");
    let field = r#"Named { name: "map", value: Option(Str) }"#;
    assert!(text.contains(field), "{text}");
    let changed = text.replace(
        field,
        &format!(r#"{field}, Named {{ name: "lanes", value: U32 }}"#),
    );
    let err = guard(&text, &changed).expect_err("a new field without a version bump");
    assert_eq!(
        err,
        "wire format of ScaleScenario changed without a version bump"
    );
    let bumped = changed.replace(
        &version,
        &format!("SCALE_SCENARIO_VERSION = {}", ScaleScenario::VERSION + 1),
    );
    guard(&text, &bumped).expect("a bumped ScaleScenario::VERSION passes");
}

#[test]
fn save_codec_formats() {
    let text = save_formats_text();
    assert!(text.contains("\nSaveFile: "), "{text}");
    assert!(text.contains("\nReplay: "), "{text}");
    if let Some(committed) = committed("save_codec_formats")
        && let Err(err) = guard(&committed, &text)
    {
        panic!("{err}");
    }
    insta::assert_snapshot!("save_codec_formats", text);
}

#[test]
fn format_guard_flags_unbumped_codec_change() {
    let committed = "SAVE_FORMAT_VERSION = 1\nSaveFile: A\ncore.pose@1:\n  Pose: B\n";
    let err = guard(
        committed,
        "SAVE_FORMAT_VERSION = 1\nSaveFile: A\ncore.pose@1:\n  Pose: C\n",
    )
    .expect_err("unbumped codec change");
    assert_eq!(err, "core.pose: format changed without a version bump");
    guard(
        committed,
        "SAVE_FORMAT_VERSION = 1\nSaveFile: A\ncore.pose@2 reads 1:\n  Pose: C\n",
    )
    .expect("a bumped codec with a migration from the committed version passes");
    let err = guard(
        committed,
        "SAVE_FORMAT_VERSION = 1\nSaveFile: D\ncore.pose@1:\n  Pose: B\n",
    )
    .expect_err("unbumped layout change");
    assert_eq!(
        err,
        "wire format of SaveFile changed without a version bump"
    );
}

#[test]
fn format_guard_flags_bump_without_migration() {
    let committed = "core.pose@1:\n  Pose: B\ncore.players@1 reads older:\n  Players: P\n";
    let err = guard(committed, "core.pose@2:\n  Pose: C\n").expect_err("no migration from 1");
    assert_eq!(
        err,
        "core.pose: version rose from 1 to 2 without a migration from 1"
    );
    let err = guard(committed, "core.pose@3 reads 2:\n  Pose: C\n")
        .expect_err("the migration reads 2, the committed version is 1");
    assert_eq!(
        err,
        "core.pose: version rose from 1 to 3 without a migration from 1"
    );
    guard(committed, "core.pose@3 reads 1, 2:\n  Pose: C\n").expect("reads 1");
    // A custom codec decodes every older version.
    guard(committed, "core.players@2 reads older:\n  Players: Q\n").expect("reads older");
}
