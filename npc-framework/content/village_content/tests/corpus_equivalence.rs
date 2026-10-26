//! Plan Task P3, step 3: the staged re-baseline of the replay corpus.
//!
//! While `tests/replays.next/` exists next to `tests/replays/`, every staged
//! replay must be the same run as its corpus original: same seed, scenario,
//! villager count, length, commands (byte-identical JSON) and checkpoint
//! ticks; only the format version, the content hash and the hash *values*
//! (new registry encoding) may differ, and the staged file must verify with
//! this build. The journals of both encodings are compared byte for byte by
//! the evidence script (`docs/_notes/p3-rebaseline-evidence.md`).
//!
//! A staged replay whose scenario gained an entity may have its commands'
//! `SimId`s moved; a documented remap next to it
//! (`tests/replays.next/<name>.idmap.json`, see `remap_commands`) is applied
//! to the original's commands before they are compared. The procedure is
//! `docs/rebaselining.md`.
//!
//! Once the staged corpus is swapped in, `replays.next/` is gone and this
//! test has nothing to compare (it says so and passes).
//!
//! **Dormant by design.** It is the check of the re-baseline flow (CLAUDE.md
//! "Re-baseline", `docs/rebaselining.md`): it runs on every `cargo
//! test` but only does something while a re-baseline is staged in
//! `tests/replays.next/`. It compares village-format replays (seed,
//! scenario, villagers, ticks, commands, checkpoint ticks). Staged testbed
//! replays and session records are skipped here: the village build cannot
//! run them, and `content/testbed_content/tests/corpus_equivalence.rs`
//! checks them the same way.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::Value;
use village_content::replay::{REPLAY_VERSION, Replay, verify};

fn dir(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests")
        .join(name)
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

#[test]
fn staged_corpus_is_the_same_runs() {
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
    assert!(!names.is_empty(), "replays.next exists but is empty");
    for staged in names {
        let file = staged.file_name().expect("file name").to_owned();
        let original = dir("replays").join(&file);
        let read = |p: &PathBuf| -> Value {
            serde_json::from_str(&std::fs::read_to_string(p).expect("readable")).expect("json")
        };
        let (old, new) = (read(&original), read(&staged));
        let name = file.to_string_lossy();
        let testbed = new["scenario"]
            .as_str()
            .is_some_and(|s| s == "testbed" || s.starts_with("testbed:"));
        if testbed || new.get("replay").is_some_and(Value::is_object) {
            eprintln!("{name}: not a village replay (testbed_content checks it)");
            continue;
        }
        for key in ["seed", "scenario", "villagers", "ticks"] {
            assert_eq!(old[key], new[key], "{name}: `{key}` differs");
        }
        assert_eq!(
            remap_commands(&name, &staged, &old["commands"]),
            new["commands"],
            "{name}: `commands` differ (after the id remap, if any)"
        );
        let ticks = |v: &Value| -> Vec<u64> {
            v["checkpoints"]
                .as_array()
                .expect("checkpoints")
                .iter()
                .map(|c| c[0].as_u64().expect("tick"))
                .collect()
        };
        assert_eq!(ticks(&old), ticks(&new), "{name}: checkpoint ticks differ");
        assert_eq!(new["version"], REPLAY_VERSION, "{name}: staged version");
        let replay: Replay = serde_json::from_value(new).expect("a replay");
        let report = verify(&replay);
        assert!(
            report.ok,
            "{name}: staged replay does not verify: {report:?}"
        );
        // The original still parses. A v1 original (P3) says why it cannot
        // be verified by this build; a v2 original (a behaviour re-baseline,
        // e.g. C2's eat claim, `docs/_notes/c2-eat-claim-evidence.md`) is
        // only re-staged when some checkpoint hash changed.
        let orig: Replay = serde_json::from_value(old).expect("original replay still loads");
        let r1 = verify(&orig);
        if orig.version == 1 {
            assert!(!r1.ok && r1.error.as_deref().is_some_and(|e| e.contains("version 1")));
        } else {
            assert_ne!(
                orig.checkpoints, replay.checkpoints,
                "{name}: staged without any checkpoint change"
            );
        }
    }
}

/// The id remap itself (no staged corpus needed): listed fields at any depth
/// are remapped, other numbers are not, and no file means no change.
#[test]
fn idmap_remaps_listed_fields_only() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("corpus_idmap");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let staged = dir.join("r.json");
    let commands = serde_json::json!([
        [50, 0, {"type": "SpawnVillager", "pos": [6, 6]}],
        [60, 1, {"type": "Order", "unit": 6, "order": {"type": "GoTo", "pos": [6, 0]}}],
        [70, 2, {"type": "IssueOrder", "units": [5, 6], "order": {"type": "Move", "pos": [0, 0]}}]
    ]);
    assert_eq!(remap_commands("r.json", &staged, &commands), commands);
    std::fs::write(
        dir.join("r.idmap.json"),
        r#"{"reason": "a well spawns after the villager", "fields": ["unit", "units"], "ids": {"6": 7}}"#,
    )
    .unwrap();
    assert_eq!(
        remap_commands("r.json", &staged, &commands),
        serde_json::json!([
            [50, 0, {"type": "SpawnVillager", "pos": [6, 6]}],
            [60, 1, {"type": "Order", "unit": 7, "order": {"type": "GoTo", "pos": [6, 0]}}],
            [70, 2, {"type": "IssueOrder", "units": [5, 7], "order": {"type": "Move", "pos": [0, 0]}}]
        ])
    );
}
