//! `cargo xtask verify --island`: its command line and the shape of its
//! report.
//!
//! These tests run the real `xtask` binary with `CARGO` pointing at a program
//! that does not exist. Every cargo step then fails at once, so nothing is
//! built, linted or tested here: only the argument handling, the step list
//! and the steps that need no cargo (`replays` with no island replay, `deny`)
//! are checked. What the island tier actually verifies is covered by running
//! it (`cargo xtask verify --island`); the pure helpers have unit tests in
//! `xtask/src/main.rs`.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn xtask(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(args)
        .env("CARGO", "xtask-island-tier-test-no-such-cargo")
        .env_remove("XTASK_ASSUME_MISSING")
        .stdin(Stdio::null())
        .output()
        .expect("the xtask binary runs")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask lives in the workspace root")
        .to_path_buf()
}

/// Island replays the tier would hand to `npcctl` (corpus and staged).
fn island_replays() -> usize {
    ["replays", "replays.next"]
        .iter()
        .filter_map(|dir| std::fs::read_dir(workspace_root().join("tests").join(dir)).ok())
        .flatten()
        .filter_map(Result::ok)
        .filter(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            name.starts_with("island_") && name.ends_with(".json")
        })
        .count()
}

/// The compact report's object for one step, up to its summary.
fn step<'a>(report: &'a str, name: &str) -> &'a str {
    let start = report
        .find(&format!("{{\"name\":\"{name}\","))
        .unwrap_or_else(|| panic!("no step `{name}` in {report}"));
    let rest = &report[start..];
    let end = rest.find(",\"log_tail\":").expect("a log_tail field");
    &rest[..end]
}

#[test]
fn island_and_quick_cannot_be_combined() {
    for args in [
        ["verify", "--island", "--quick"],
        ["verify", "--quick", "--island"],
    ] {
        let out = xtask(&args);
        assert_eq!(out.status.code(), Some(2), "{args:?} is a usage error");
        assert!(out.stdout.is_empty(), "{args:?} prints no verdict");
        let err = text(&out.stderr);
        assert!(
            err.contains("--quick") && err.contains("--island") && err.contains("usage:"),
            "{args:?}: {err}"
        );
    }
}

#[test]
fn help_names_the_island_tier() {
    let out = xtask(&["verify", "--help"]);
    assert_eq!(out.status.code(), Some(0));
    let err = text(&out.stderr);
    assert!(err.contains("[--quick | --island]"), "{err}");
    assert!(err.contains("--island  "), "{err}");
}

#[test]
fn island_report_keeps_the_seven_steps_and_scopes_them() {
    let out = xtask(&["verify", "--island"]);
    // No cargo: fmt, clippy, test and arch_fitness fail, so the verdict is red.
    assert_eq!(out.status.code(), Some(1));
    let report = text(&out.stdout);
    let report = report.trim();
    assert_eq!(report.lines().count(), 1, "one JSON object: {report}");
    assert!(report.starts_with("{\"ok\":false,\"steps\":["), "{report}");

    // The same seven steps, in the same order, as every other tier.
    let names = [
        "fmt",
        "clippy",
        "test",
        "arch_fitness",
        "replays",
        "goldens",
        "deny",
    ];
    let mut at = 0;
    for name in names {
        let found = report[at..]
            .find(&format!("{{\"name\":\"{name}\","))
            .unwrap_or_else(|| panic!("step `{name}` missing or out of order: {report}"));
        at += found + 1;
    }
    assert_eq!(report.matches("{\"name\":\"").count(), names.len());

    // clippy and test name the island crates, not the workspace.
    let packages = "-p island_content -p island_view -p island_game";
    for name in ["clippy", "test"] {
        let s = step(report, name);
        assert!(s.contains(packages), "{name} is scoped to the island: {s}");
        assert!(!s.contains("--workspace"), "{name}: {s}");
    }
    assert!(step(report, "clippy").contains("--all-targets"));
    assert!(step(report, "clippy").contains("-- -D warnings"));

    // deny is skipped by name; replays without island replays is a note.
    assert!(step(report, "deny").contains("\"ok\":true"));
    assert!(step(report, "deny").contains("\"summary\":\"skipped: --island\""));
    if island_replays() == 0 {
        let s = step(report, "replays");
        assert!(s.contains("\"ok\":true"), "{s}");
        assert!(s.contains("\"summary\":\"0 island replays (note: "), "{s}");
    }
}
