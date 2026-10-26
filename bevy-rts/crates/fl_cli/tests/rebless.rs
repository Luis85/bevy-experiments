//! `fl_cli rebless --reason MCR-nnn` (ADR-0021, R146): it refuses an MCR id the changelog does
//! not have and a replay whose semantic check fails, writing nothing; otherwise it writes
//! `<name>.replay.new` for every replay whose bytes change and the re-bless report.

use std::path::{Path, PathBuf};
use std::process::Command;

use fl_math::{Fx, FxVec2};
use fl_protocol::{MoveCmd, SimId, Tick};
use fl_sim::{Replay, SemanticCheck, rebless_replay};
use fl_test_utils::{ScenarioBuilder, base_content};
use serde_json::Value;

/// The exit code, the single JSON document on stdout, and stderr.
struct Run {
    code: Option<i32>,
    json: Value,
    stderr: String,
}

fn fl_cli(args: &[&str]) -> Run {
    let output = Command::new(env!("CARGO_BIN_EXE_fl_cli"))
        .args(args)
        .output()
        .expect("run fl_cli");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let json = serde_json::from_str(&stdout).unwrap_or_else(|err| {
        panic!("stdout of {args:?} is not one JSON document ({err}):\n{stdout}\nstderr:\n{stderr}")
    });
    Run {
        code: output.status.code(),
        json,
        stderr,
    }
}

fn s(path: &Path) -> &str {
    path.to_str().expect("UTF-8 path")
}

/// 40 ticks of the base testbed with every unit wandering and one move, recorded on this
/// build.
fn recorded_testbed() -> Replay {
    let mut s = ScenarioBuilder::new()
        .scenario("base.testbed")
        .wander_all(100)
        .command(
            10,
            0,
            &[1, 2],
            MoveCmd {
                target: FxVec2::new(Fx::from_int(600), Fx::from_int(500)),
            },
        )
        .build();
    s.run_ticks(40);
    s.replay()
}

/// The format-3 bytes of `replay` (which must have a scenario or save start): format 4 appends
/// `semantic` and `journal_summary` to format 3's fields, so with both empty its encoding is
/// format 3's with the version byte changed and two empty-length bytes after.
fn format_3_bytes(replay: &Replay) -> Vec<u8> {
    let mut replay = replay.clone();
    replay.semantic.clear();
    replay.journal_summary.clear();
    let mut bytes = replay.to_bytes();
    assert_eq!(bytes[0], 4);
    assert_eq!(bytes[bytes.len() - 2..], [0, 0]);
    bytes[0] = 3;
    bytes.truncate(bytes.len() - 2);
    let read = Replay::from_bytes(&bytes).expect("format 3 reads");
    assert_eq!(read, replay, "the bytes are the format-3 encoding");
    bytes
}

/// A scratch directory `replays/` and a changelog with the headings `mcrs`.
struct Scratch {
    _dir: tempfile::TempDir,
    replays: PathBuf,
    changelog: PathBuf,
    report: PathBuf,
}

impl Scratch {
    fn new(mcrs: &[&str]) -> Scratch {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let replays = dir.path().join("replays");
        std::fs::create_dir(&replays).expect("the replays directory");
        let changelog = dir.path().join("changelog-mechanics.md");
        let mut text = "# Mechanic change requests\n\n## Entries\n\n".to_owned();
        for mcr in mcrs {
            text.push_str(&format!(
                "## {mcr} — a change (C8)\n\n- Why: testing.\n- Predicted effect: hash streams \
                 identical\n  (first divergent tick: none).\n\n"
            ));
        }
        std::fs::write(&changelog, text).expect("the changelog");
        let report = dir.path().join("out").join("rebless-report.md");
        Scratch {
            _dir: dir,
            replays,
            changelog,
            report,
        }
    }

    fn put(&self, name: &str, bytes: &[u8]) {
        std::fs::write(self.replays.join(name), bytes).expect("a replay");
    }

    fn rebless(&self, reason: &str) -> Run {
        fl_cli(&[
            "--json",
            "rebless",
            "--reason",
            reason,
            "--dir",
            s(&self.replays),
            "--changelog",
            s(&self.changelog),
            "--report",
            s(&self.report),
        ])
    }

    /// The names in `replays/` ending in `.new`.
    fn new_files(&self) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(&self.replays)
            .expect("the replays directory")
            .map(|entry| {
                entry
                    .expect("an entry")
                    .file_name()
                    .into_string()
                    .expect("UTF-8")
            })
            .filter(|name| name.ends_with(".new"))
            .collect();
        names.sort();
        names
    }
}

#[test]
fn rebless_refuses_unknown_mcr() {
    let scratch = Scratch::new(&["MCR-000"]);
    scratch.put("a.replay", &recorded_testbed().to_bytes());
    let run = scratch.rebless("MCR-999");
    assert_eq!(run.code, Some(1), "{:#}\n{}", run.json, run.stderr);
    assert!(run.stderr.contains("MCR-999"), "{}", run.stderr);
    assert!(
        run.json["error"]
            .as_str()
            .is_some_and(|e| e.contains("MCR-999"))
    );
    assert!(scratch.new_files().is_empty());
    assert!(!scratch.report.exists(), "no report");

    // A heading that only starts with the id is not it.
    let scratch = Scratch::new(&["MCR-0001"]);
    scratch.put("a.replay", &recorded_testbed().to_bytes());
    assert_eq!(scratch.rebless("MCR-000").code, Some(1));
    assert!(scratch.new_files().is_empty());
}

#[test]
fn rebless_refuses_failing_semantic_check() {
    let scratch = Scratch::new(&["MCR-000"]);
    scratch.put("good.replay", &format_3_bytes(&recorded_testbed()));
    let ghost = recorded_testbed().with_semantics(vec![SemanticCheck::UnitExists {
        id: "ghost_lives".to_owned(),
        tick: Tick(20),
        unit: SimId(99),
        exists: true,
    }]);
    scratch.put("ghost.replay", &ghost.to_bytes());
    let run = scratch.rebless("MCR-000");
    assert_eq!(run.code, Some(1), "{:#}\n{}", run.json, run.stderr);
    assert!(run.stderr.contains("ghost_lives"), "{}", run.stderr);
    assert!(run.stderr.contains("ghost.replay"), "{}", run.stderr);
    assert!(scratch.new_files().is_empty(), "nothing written");
    assert!(!scratch.report.exists(), "no report");
}

#[test]
fn rebless_writes_new_and_report() {
    let scratch = Scratch::new(&["MCR-000"]);
    let recorded = recorded_testbed();
    scratch.put("old.replay", &format_3_bytes(&recorded));
    // A replay this build re-blessed already: its bytes do not change.
    let (current, _) = rebless_replay(&recorded, &fl_catalog::features(), base_content())
        .expect("the replay re-runs");
    scratch.put("current.replay", &current.to_bytes());

    let run = scratch.rebless("MCR-000");
    assert_eq!(run.code, Some(0), "{:#}\n{}", run.json, run.stderr);
    assert_eq!(scratch.new_files(), ["old.replay.new"]);
    let new = std::fs::read(scratch.replays.join("old.replay.new")).expect("the new replay");
    assert_eq!(new[0], 4, "written as format 4");
    let new = Replay::from_bytes(&new).expect("the new replay decodes");
    assert_eq!(new.hashes, recorded.hashes);
    assert_eq!(new.inputs, recorded.inputs);
    assert_eq!(new.start, recorded.start);
    assert!(!new.journal_summary.is_empty());

    let json = &run.json;
    assert_eq!(json["reason"], "MCR-000");
    let replays = json["replays"].as_array().expect("replays");
    let names: Vec<&str> = replays
        .iter()
        .map(|r| r["name"].as_str().unwrap_or(""))
        .collect();
    assert_eq!(names, ["current.replay", "old.replay"]);
    assert_eq!(replays[0]["changed"], false);
    assert_eq!(replays[0]["changes"], Value::Array(Vec::new()));
    assert_eq!(replays[1]["changed"], true);
    // What changed is the format and the summary, never the hash stream.
    assert_eq!(
        replays[1]["changes"],
        serde_json::json!(["format 3 → 4", "journal summary"])
    );
    assert_eq!(
        json["predicted_effect"],
        "hash streams identical (first divergent tick: none)."
    );
    for replay in replays {
        assert!(replay["first_divergence"].is_null(), "{replay:#}");
    }
    assert_eq!(replays[0]["journal_diff"], Value::Array(Vec::new()));
    let diff = replays[1]["journal_diff"]
        .as_array()
        .expect("a journal diff");
    assert!(
        diff.iter()
            .all(|d| d["before"] == 0 && d["after"].as_u64() > Some(0)),
        "format 3 had no summary: {diff:?}"
    );
    assert!(!diff.is_empty());

    let report = std::fs::read_to_string(&scratch.report).expect("the report");
    assert!(report.contains("MCR-000"), "{report}");
    assert!(
        report.contains(
            "Predicted effect (MCR-000): hash streams identical (first divergent tick: none)."
        ),
        "{report}"
    );
    assert!(
        report.contains("- changed: yes (format 3 → 4; journal summary), wrote old.replay.new"),
        "{report}"
    );
    assert!(report.contains("## old.replay"), "{report}");
    assert!(report.contains("first divergent tick: none"), "{report}");
    assert!(
        report.contains("entities: 10 at the start, 10 at the end"),
        "{report}"
    );
}
