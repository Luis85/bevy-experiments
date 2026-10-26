//! The staged re-baseline of the testbed part of the replay corpus (the
//! testbed counterpart of `village_content/tests/corpus_equivalence.rs`).
//!
//! While `tests/replays.next/` exists, every staged testbed replay or agent
//! session record must be the same run as its corpus original: same seed,
//! scenario (and scenario document), villager count, length, commands
//! (byte-identical JSON) and checkpoint ticks. Only the content hash and the
//! hash *values* may differ, some checkpoint hash must differ (else there is
//! nothing to re-baseline), and the staged file must verify with this build.
//! Behaviour equality (journals before vs after) is shown by the evidence
//! note of the re-baseline (e.g. `docs/_notes/fx1-rebaseline-evidence.md`).
//!
//! A documented id remap next to a staged file
//! (`tests/replays.next/<name>.idmap.json`, see `remap_commands`) is applied
//! to the original's commands first (`docs/rebaselining.md`).
//!
//! Dormant by design: with no `tests/replays.next/` it passes at once.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use npc_framework::agent_io::ScenarioLoaders;
use npc_framework::agent_io::scenario::{SessionRecord, verify_session};
use serde_json::Value;
use testbed_content::replay::{Replay, is_testbed, verify};

fn dir(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests")
        .join(name)
}

fn checkpoint_ticks(replay: &Value) -> Vec<u64> {
    replay["checkpoints"]
        .as_array()
        .expect("checkpoints")
        .iter()
        .map(|c| c[0].as_u64().expect("tick"))
        .collect()
}

/// Suffix of an id remap staged next to a replay (`<name>.idmap.json`).
const IDMAP_SUFFIX: &str = ".idmap.json";

/// Applies the documented id remap of a staged replay to the original's
/// `commands` (`docs/rebaselining.md`, "SimIds moved"). The file
/// `tests/replays.next/<name>.idmap.json` is
/// `{"reason": "..", "fields": ["unit", ..], "ids": {"6": 7, ..}}`: every
/// integer (or integer in an array) under a key listed in `fields`, at any
/// depth of a command, that is a key of `ids` is replaced by its value.
/// Without the file the commands must be byte-identical JSON.
fn remap_commands(name: &str, staged: &Path, commands: &Value) -> Value {
    let stem = name.strip_suffix(".json").unwrap_or(name);
    let file = staged.with_file_name(format!("{stem}{IDMAP_SUFFIX}"));
    let Ok(text) = std::fs::read_to_string(&file) else {
        return commands.clone();
    };
    let map: Value = serde_json::from_str(&text).expect("idmap is JSON");
    assert!(
        map["reason"].as_str().is_some_and(|r| !r.trim().is_empty()),
        "{}: an id remap must say why (`reason`)",
        file.display()
    );
    let fields: Vec<&str> = map["fields"]
        .as_array()
        .expect("idmap `fields`")
        .iter()
        .map(|f| f.as_str().expect("field name"))
        .collect();
    let ids: BTreeMap<u64, u64> = map["ids"]
        .as_object()
        .expect("idmap `ids`")
        .iter()
        .map(|(k, v)| {
            (
                k.parse().expect("idmap `ids` keys are ids"),
                v.as_u64().expect("idmap `ids` values are ids"),
            )
        })
        .collect();
    assert!(
        !fields.is_empty() && !ids.is_empty(),
        "{}: empty",
        file.display()
    );
    let mut out = commands.clone();
    remap(&mut out, &fields, &ids, false);
    out
}

fn remap(v: &mut Value, fields: &[&str], ids: &BTreeMap<u64, u64>, is_id: bool) {
    match v {
        Value::Number(n) if is_id => {
            if let Some(&new) = n.as_u64().and_then(|old| ids.get(&old)) {
                *v = Value::from(new);
            }
        }
        Value::Array(items) => {
            for item in items {
                remap(item, fields, ids, is_id);
            }
        }
        Value::Object(map) => {
            for (key, item) in map.iter_mut() {
                remap(item, fields, ids, fields.contains(&key.as_str()));
            }
        }
        _ => {}
    }
}

/// Compares one staged replay object with its original (see the module docs).
fn same_run(name: &str, staged: &Path, old: &Value, new: &Value) {
    for key in ["seed", "scenario", "villagers", "ticks"] {
        assert_eq!(old[key], new[key], "{name}: `{key}` differs");
    }
    assert_eq!(
        remap_commands(name, staged, &old["commands"]),
        new["commands"],
        "{name}: `commands` differ (after the id remap, if any)"
    );
    assert_eq!(
        checkpoint_ticks(old),
        checkpoint_ticks(new),
        "{name}: checkpoint ticks differ"
    );
    assert_ne!(
        old["checkpoints"], new["checkpoints"],
        "{name}: staged without any checkpoint change"
    );
}

#[test]
fn staged_testbed_corpus_is_the_same_runs() {
    let next = dir("replays.next");
    let Ok(entries) = std::fs::read_dir(&next) else {
        eprintln!("no {} (nothing staged)", next.display());
        return;
    };
    let mut names: Vec<PathBuf> = entries
        .map(|e| e.expect("dir entry").path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .filter(|p| !p.to_string_lossy().ends_with(IDMAP_SUFFIX))
        .collect();
    names.sort();
    let mut loaders = ScenarioLoaders::default();
    testbed_content::register_scenario_loader(&mut loaders);
    for staged in names {
        let file = staged.file_name().expect("file name").to_owned();
        let name = file.to_string_lossy().into_owned();
        let read = |p: &PathBuf| -> Value {
            serde_json::from_str(&std::fs::read_to_string(p).expect("readable")).expect("json")
        };
        let new = read(&staged);
        if new.get("replay").is_some_and(Value::is_object) {
            if new["scenario"]["content"].as_str() != Some("testbed") {
                continue;
            }
            let old = read(&dir("replays").join(&file));
            assert_eq!(
                old["scenario"], new["scenario"],
                "{name}: scenario document differs"
            );
            assert_eq!(old["seed"], new["seed"], "{name}: seed differs");
            same_run(&name, &staged, &old["replay"], &new["replay"]);
            let record: SessionRecord = serde_json::from_value(new).expect("a session record");
            let report = verify_session(&loaders, &record);
            assert!(
                report.ok && report.checked == record.replay.checkpoints.len(),
                "{name}: staged session does not verify: {report:?}"
            );
            continue;
        }
        if !new["scenario"].as_str().is_some_and(is_testbed) {
            continue;
        }
        let old = read(&dir("replays").join(&file));
        same_run(&name, &staged, &old, &new);
        let replay: Replay = serde_json::from_value(new).expect("a replay");
        let report = verify(&replay);
        assert!(
            report.ok && report.checked == replay.checkpoints.len(),
            "{name}: staged replay does not verify: {report:?}"
        );
    }
}
