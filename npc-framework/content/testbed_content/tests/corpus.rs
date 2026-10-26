//! Wave G2b: the Actor Testbed part of the committed replay corpus
//! (`npc-framework/tests/replays/testbed_*.json`) must verify: every
//! checkpoint hash of every testbed replay and agent session record is
//! reproduced by a fresh run (the village's files are verified by
//! `content/village_content/tests/corpus.rs`).
//!
//! - Replays name their scenario `testbed` or `testbed:<name>` and are
//!   rebuilt by `testbed_content::replay::try_new_replay_app` (recorded with
//!   `npcctl replay record --scenario testbed[:<name>] [--commands ..]`).
//! - Session records (`{"scenario": {..}, "replay": {..}}`, as `npcctl serve
//!   --record` writes them) are re-run from their scenario document with the
//!   testbed scenario loader, without the agent.
//!
//! The files are oracles: never edit or re-record them to make this test
//! pass. Re-recording needs explicit approval. A change to the testbed's
//! registered content (a new name) changes its content hash and so fails
//! every testbed file: say so and ask for a re-record.

use std::path::PathBuf;

use npc_framework::agent_io::ScenarioLoaders;
use npc_framework::agent_io::scenario::{SessionRecord, verify_session};
use serde_json::Value;
use testbed_content::replay::{Replay, is_testbed, verify};

/// Testbed files the corpus must always contain (more may be added).
const REQUIRED: [&str; 4] = [
    "testbed_default_2days",
    "testbed_orders_craft",
    "testbed_vehicles",
    "testbed_agent_session",
];

fn corpus_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/replays")
}

fn loaders() -> ScenarioLoaders {
    let mut l = ScenarioLoaders::default();
    testbed_content::register_scenario_loader(&mut l);
    l
}

/// What a corpus file is, for this test.
enum Entry {
    Replay(Box<Replay>),
    Session(Box<SessionRecord>),
    /// Another content's file.
    Other,
}

fn classify(v: Value) -> Result<Entry, String> {
    if v.get("replay").is_some_and(Value::is_object) {
        if v["scenario"]["content"].as_str() != Some("testbed") {
            return Ok(Entry::Other);
        }
        return serde_json::from_value(v)
            .map(|r| Entry::Session(Box::new(r)))
            .map_err(|e| format!("not a session record: {e}"));
    }
    if !v["scenario"].as_str().is_some_and(is_testbed) {
        return Ok(Entry::Other);
    }
    serde_json::from_value(v)
        .map(|r| Entry::Replay(Box::new(r)))
        .map_err(|e| format!("not a replay: {e}"))
}

#[test]
fn testbed_replay_corpus_verifies() {
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
            "corpus file {required}.json is missing from {}",
            dir.display()
        );
    }
    let loaders = loaders();
    let mut failures = Vec::new();
    let mut checked_files = 0;
    for path in &files {
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        let text = std::fs::read_to_string(path).expect("readable corpus file");
        let entry = match serde_json::from_str(&text).map_err(|e| e.to_string()) {
            Ok(v) => classify(v),
            Err(e) => Err(format!("not JSON: {e}")),
        };
        let (report, checkpoints, ticks, commands) = match entry {
            Ok(Entry::Other) => continue,
            Ok(Entry::Replay(r)) => (verify(&r), r.checkpoints.len(), r.ticks, r.commands.len()),
            Ok(Entry::Session(s)) => (
                verify_session(&loaders, &s),
                s.replay.checkpoints.len(),
                s.replay.ticks,
                s.replay.commands.len(),
            ),
            Err(e) => {
                failures.push(format!("{name}: {e}"));
                continue;
            }
        };
        checked_files += 1;
        eprintln!(
            "{name}: ok={} checked={}/{checkpoints} ticks={ticks} commands={commands}",
            report.ok, report.checked
        );
        if checkpoints == 0 {
            failures.push(format!("{name}: no checkpoints"));
        } else if !report.ok || report.checked != checkpoints {
            failures.push(format!("{name}: {report:?}"));
        }
    }
    assert!(checked_files >= REQUIRED.len(), "{checked_files} files");
    assert!(
        failures.is_empty(),
        "testbed corpus failures:\n{}",
        failures.join("\n")
    );
}

#[test]
fn the_agent_session_record_holds_agent_actions() {
    let path = corpus_dir().join("testbed_agent_session.json");
    let text = std::fs::read_to_string(&path).expect("the session record");
    let record: SessionRecord = serde_json::from_str(&text).expect("a session record");
    assert_eq!(record.scenario.content, "testbed");
    let kinds: Vec<String> = record
        .replay
        .commands
        .iter()
        .map(|c| c.2.kind().to_owned())
        .collect();
    assert!(kinds.iter().any(|k| k == "AgentAct"), "{kinds:?}");
    assert!(kinds.iter().any(|k| k == "AttachController"), "{kinds:?}");
}
