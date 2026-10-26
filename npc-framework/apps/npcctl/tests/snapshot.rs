//! `npcctl snapshot save | check` through the real binary.

use std::path::PathBuf;
use std::process::{Command, Output};

use serde_json::Value;

fn npcctl(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_npcctl"))
        .args(args)
        .output()
        .expect("npcctl runs")
}

fn json_out(args: &[&str], want: i32) -> Value {
    let out = npcctl(args);
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    assert_eq!(
        out.status.code(),
        Some(want),
        "{args:?}\nstdout: {stdout}\nstderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("{args:?}: not JSON ({e}): {stdout}"))
}

fn temp_file(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("npcctl_snapshot_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

#[test]
fn snapshot_save_then_check_ok() {
    let file = temp_file("ok.json");
    let f = file.to_str().unwrap();
    let saved = json_out(
        &[
            "snapshot",
            "save",
            "--scenario",
            "testbed",
            "--seed",
            "3",
            "--ticks",
            "40",
            "--out",
            f,
        ],
        0,
    );
    assert_eq!(saved["ok"], true, "{saved}");
    assert!(file.exists());
    assert!(!file.with_extension("json.tmp").exists());
    let checked = json_out(&["snapshot", "check", "--scenario", "testbed", f], 0);
    assert_eq!(checked["ok"], true, "{checked}");
    // Saving again over the existing file works.
    json_out(
        &[
            "snapshot",
            "save",
            "--scenario",
            "testbed",
            "--ticks",
            "10",
            "--out",
            f,
        ],
        0,
    );
    let _ = std::fs::remove_file(&file);
}

#[test]
fn snapshot_check_other_scenario_fails() {
    let file = temp_file("other.json");
    let f = file.to_str().unwrap();
    json_out(
        &[
            "snapshot",
            "save",
            "--scenario",
            "testbed",
            "--ticks",
            "5",
            "--out",
            f,
        ],
        0,
    );
    let checked = json_out(&["snapshot", "check", "--scenario", "village", f], 1);
    assert_eq!(checked["ok"], false);
    let error = checked["error"].as_str().expect("an error string");
    assert!(error.contains("content hash"), "{error}");
    let _ = std::fs::remove_file(&file);
}

#[test]
fn snapshot_check_unreadable_file_is_a_usage_error() {
    let file = temp_file("missing.json");
    let v = json_out(
        &[
            "snapshot",
            "check",
            "--scenario",
            "testbed",
            file.to_str().unwrap(),
        ],
        2,
    );
    assert!(
        v["error"].as_str().unwrap().contains("unreadable save"),
        "{v}"
    );
}
