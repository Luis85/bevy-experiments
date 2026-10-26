//! Task P2: the committed replay corpus (`npc-framework/tests/replays/*.json`,
//! recorded with `npcctl replay record`) must verify: every checkpoint hash
//! of every replay is reproduced by a fresh run.
//!
//! The replays are oracles: never edit or re-record them to make this test
//! pass. Re-recording needs explicit approval (and, when the hash encoding
//! changes, the equivalence proof described in the baseline-hardening plan).
//!
//! The corpus also holds other contents' files: Actor Testbed replays
//! (`"scenario": "testbed"` / `"testbed:<name>"`) and agent session records
//! (`{"scenario": {..}, "replay": {..}}`). This crate cannot build them;
//! `content/testbed_content/tests/corpus.rs` verifies them (and
//! `cargo xtask verify` checks every file with `npcctl replay verify`). Any
//! other file still fails here.

use std::path::PathBuf;

use village_content::replay::{Replay, verify};

/// Replays the corpus must always contain (more may be added).
const REQUIRED: [&str; 4] = [
    "village_seed1_3days",
    "village_orders",
    "village_spawn_despawn",
    "village_n_25_1day",
];

fn corpus_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/replays")
}

/// A testbed replay or an agent session record of another content.
fn is_other_content(v: &serde_json::Value) -> bool {
    let session = v.get("replay").is_some_and(serde_json::Value::is_object)
        && v["scenario"]["content"]
            .as_str()
            .is_some_and(|c| c != "village");
    let testbed = v["scenario"]
        .as_str()
        .is_some_and(|s| s == "testbed" || s.starts_with("testbed:"));
    session || testbed
}

#[test]
fn replay_corpus_verifies() {
    let dir = corpus_dir();
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()))
        .map(|entry| entry.expect("dir entry").path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    files.sort();
    for required in REQUIRED {
        assert!(
            files
                .iter()
                .any(|p| p.file_stem().is_some_and(|s| s == required)),
            "corpus replay {required}.json is missing from {}",
            dir.display()
        );
    }

    let mut failures = Vec::new();
    for path in &files {
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        let text = std::fs::read_to_string(path).expect("readable replay");
        let value: serde_json::Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(e) => {
                failures.push(format!("{name}: not JSON: {e}"));
                continue;
            }
        };
        if is_other_content(&value) {
            eprintln!("{name}: another content's file (verified by its own corpus test)");
            continue;
        }
        let replay: Replay = match serde_json::from_value(value) {
            Ok(r) => r,
            Err(e) => {
                failures.push(format!("{name}: not a replay: {e}"));
                continue;
            }
        };
        if replay.checkpoints.is_empty() {
            failures.push(format!("{name}: no checkpoints"));
            continue;
        }
        let report = verify(&replay);
        eprintln!(
            "{name}: ok={} checked={}/{} ticks={} commands={}",
            report.ok,
            report.checked,
            replay.checkpoints.len(),
            replay.ticks,
            replay.commands.len()
        );
        if !report.ok || report.checked != replay.checkpoints.len() {
            failures.push(format!("{name}: {report:?}"));
        }
    }
    assert!(
        failures.is_empty(),
        "replay corpus failures:\n{}",
        failures.join("\n")
    );
}
