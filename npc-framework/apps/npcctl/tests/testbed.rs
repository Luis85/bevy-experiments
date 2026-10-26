//! Wave G2b: npcctl on the Actor Testbed (`--scenario testbed[:<name>]` or a
//! scenario file), through the real binary, plus the insta golden of the
//! `run --scenario testbed --ticks 4800` summary.
//!
//! The snapshot in `tests/snapshots/` is an oracle: never edit or re-accept
//! it to make the test pass; re-baselining needs explicit approval.

use std::path::PathBuf;
use std::process::{Command, Output};

use npc_framework::kernel::ids::SimIndex;
use serde_json::{Value, json};
use testbed_content::TestbedIds;

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

fn lines(args: &[&str]) -> Vec<Value> {
    let out = npcctl(args);
    assert!(
        out.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| serde_json::from_str(l).expect("a JSON line"))
        .collect()
}

/// The ids of the embedded scenario `name` by spawn name (and its entity
/// count), looked up instead of hard-coded: a new scenario entity renumbers
/// what spawns after it (`.claude/rules/content.md`, "SimId stability").
fn testbed_ids(name: &str) -> (TestbedIds, usize) {
    let app = testbed_content::new_sim_app(1, name);
    let world = app.world();
    (
        world.resource::<TestbedIds>().clone(),
        world.resource::<SimIndex>().map.len(),
    )
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("npcctl_testbed")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn s(p: &std::path::Path) -> &str {
    p.to_str().unwrap()
}

fn scenario_file(name: &str) -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../content/testbed_content/scenarios")
        .join(format!("{name}.json"))
        .to_string_lossy()
        .into_owned()
}

#[test]
fn golden_run_testbed_4800() {
    let summary = json_out(&["run", "--scenario", "testbed", "--ticks", "4800"], 0);
    insta::assert_json_snapshot!("run_testbed_4800", summary);
}

#[test]
fn run_on_the_testbed_keeps_the_summary_shape_and_adds_the_content_summary() {
    let v = json_out(&["run", "--scenario", "testbed", "--ticks", "300"], 0);
    assert_eq!(v["scenario"], "testbed");
    assert_eq!(v["villagers"], 8, "the scenario's actors");
    assert!(v["final_hash"].is_u64());
    assert!(
        v["leaf_starts"].as_u64().unwrap() > 0,
        "testbed brains count"
    );
    let units = v["content_summary"]["units"].as_array().unwrap();
    assert_eq!(units.len(), 8);
    assert_eq!(v["content_summary"]["doors"]["store_door"], "locked");
    // A bare embedded name is qualified; the village has no content summary.
    let b = json_out(&["run", "--scenario", "smithy", "--ticks", "5"], 0);
    assert_eq!(b["scenario"], "testbed:smithy");
    let village = json_out(&["run", "--ticks", "5"], 0);
    assert!(village.get("content_summary").is_none(), "{village}");
    // Same seed, same hash; another seed, another hash.
    let again = json_out(&["run", "--scenario", "testbed", "--ticks", "300"], 0);
    assert_eq!(v["final_hash"], again["final_hash"]);
    let other = json_out(
        &[
            "run",
            "--scenario",
            "testbed",
            "--ticks",
            "300",
            "--seed",
            "2",
        ],
        0,
    );
    assert_ne!(v["final_hash"], other["final_hash"]);
}

#[test]
fn testbed_replays_record_their_scenario_and_verify() {
    let dir = temp_dir("record");
    let cmds = dir.join("cmds.json");
    // The cart is ordered through its driver (mounted first).
    let (ids, _) = testbed_ids("vehicles");
    let (cart, driver) = (ids.id("cart").0, ids.id("driver").0);
    let commands = json!([
        {"tick": 5, "cmd": {"type": "IssueOrder", "player": 1, "units": [driver],
                            "order": {"type": "Mount", "vehicle": cart}}},
        {"tick": 40, "cmd": {"type": "IssueOrder", "player": 1, "units": [cart],
                             "order": {"type": "Move", "pos": [4500, -2500]}}},
    ]);
    std::fs::write(&cmds, commands.to_string()).unwrap();
    let r = json_out(
        &[
            "replay",
            "record",
            "--scenario",
            "testbed:vehicles",
            "--ticks",
            "300",
            "--commands",
            s(&cmds),
        ],
        0,
    );
    assert_eq!(r["scenario"], "testbed:vehicles");
    assert_eq!(r["villagers"], 2);
    assert_eq!(r["commands"].as_array().unwrap().len(), 2);
    let file = dir.join("r.json");
    std::fs::write(&file, r.to_string()).unwrap();
    let v = json_out(&["replay", "verify", s(&file)], 0);
    assert_eq!(v, json!({"ok": true, "checked": 3, "first_mismatch": null}));
    // `run --record` writes the same replay.
    let rec = dir.join("run.json");
    json_out(
        &[
            "run",
            "--scenario",
            "testbed:vehicles",
            "--ticks",
            "300",
            "--commands",
            s(&cmds),
            "--record",
            s(&rec),
        ],
        0,
    );
    let from_run: Value = serde_json::from_str(&std::fs::read_to_string(&rec).unwrap()).unwrap();
    assert_eq!(from_run, r);
    // A tampered checkpoint fails (exit 1).
    let mut bad = r.clone();
    bad["checkpoints"][1][1] = json!(1);
    std::fs::write(&file, bad.to_string()).unwrap();
    let v = json_out(&["replay", "verify", s(&file)], 1);
    assert_eq!(v["ok"], false);
    assert_eq!(v["first_mismatch"][0], 200);
}

#[test]
fn a_scenario_file_records_a_session_record_that_verifies() {
    let dir = temp_dir("file");
    let r = json_out(
        &[
            "replay",
            "record",
            "--scenario",
            &scenario_file("doors"),
            "--ticks",
            "200",
        ],
        0,
    );
    assert_eq!(r["version"], 1);
    assert_eq!(r["scenario"]["content"], "testbed");
    assert_eq!(r["seed"], 1, "the file's seed");
    assert_eq!(r["replay"]["checkpoints"].as_array().unwrap().len(), 2);
    let file = dir.join("session.json");
    std::fs::write(&file, r.to_string()).unwrap();
    for cmd in ["verify", "verify-session"] {
        let v = json_out(&["replay", cmd, s(&file)], 0);
        assert_eq!(v["ok"], true, "{cmd}: {v}");
        assert_eq!(v["checked"], 2);
    }
    // The file's world equals the embedded scenario of the same name.
    let a = json_out(
        &[
            "run",
            "--scenario",
            &scenario_file("doors"),
            "--ticks",
            "100",
        ],
        0,
    );
    let b = json_out(&["run", "--scenario", "testbed:doors", "--ticks", "100"], 0);
    assert_eq!(a["final_hash"], b["final_hash"]);
    // A missing file is a usage error.
    let v = json_out(
        &["run", "--scenario", "no/such/scenario.json", "--ticks", "1"],
        2,
    );
    assert!(
        v["error"]
            .as_str()
            .unwrap()
            .contains("cannot read scenario"),
        "{v}"
    );
}

#[test]
fn inspect_and_journal_on_the_testbed() {
    let (ids, entities) = testbed_ids("smithy");
    let smith = ids.id("smith").0;
    let smith_arg = smith.to_string();
    let unit = json_out(
        &[
            "inspect",
            "--scenario",
            "testbed:smithy",
            "--tick",
            "10",
            "--unit",
            &smith_arg,
        ],
        0,
    );
    assert_eq!(unit["sim_id"], smith);
    assert_eq!(unit["kind"], "actor");
    assert_eq!(unit["components"]["testbed_unit"]["template"], "smith");
    let world = json_out(
        &["inspect", "--scenario", "testbed:smithy", "--tick", "1"],
        0,
    );
    assert_eq!(world["entities"].as_array().unwrap().len(), entities);
    let v = json_out(
        &[
            "inspect",
            "--scenario",
            "testbed:smithy",
            "--tick",
            "1",
            "--unit",
            "999",
        ],
        2,
    );
    assert!(v["error"].as_str().unwrap().contains("unknown unit 999"));
    let spawns = lines(&[
        "journal",
        "--scenario",
        "testbed:smithy",
        "--ticks",
        "1",
        "--kind",
        "Spawn",
    ]);
    assert_eq!(spawns.len(), entities, "every scenario spawn at tick 0");
    let rolls = lines(&[
        "journal",
        "--scenario",
        "testbed",
        "--ticks",
        "400",
        "--kind",
        "Roll",
        "--grep",
        "perception.detect",
        "--limit",
        "3",
    ]);
    assert_eq!(rolls.len(), 3, "Per-roll detection in the showcase");
}

#[test]
fn diverge_and_bench_on_the_testbed() {
    let v = json_out(
        &[
            "diverge",
            "--scenario",
            "testbed",
            "--seed-a",
            "1",
            "--seed-b",
            "2",
            "--ticks",
            "200",
        ],
        1,
    );
    assert!(v["first_divergent_tick"].as_u64().unwrap() <= 200, "{v}");
    let v = json_out(
        &[
            "diverge",
            "--scenario",
            "testbed:doors",
            "--seed-a",
            "1",
            "--seed-b",
            "1",
            "--ticks",
            "100",
        ],
        0,
    );
    assert_eq!(v["first_divergent_tick"], Value::Null);
    let v = json_out(
        &[
            "diverge",
            "--scenario",
            &scenario_file("doors"),
            "--seed-a",
            "1",
            "--seed-b",
            "2",
        ],
        2,
    );
    assert!(
        v["error"].as_str().unwrap().contains("named scenario"),
        "{v}"
    );
    let b = json_out(
        &[
            "bench",
            "--scenario",
            "testbed",
            "--ticks",
            "20",
            "--warmup",
            "5",
        ],
        0,
    );
    let sizes = b["results"].as_array().unwrap();
    assert_eq!(sizes.len(), 1, "a fixed-size scenario is measured once");
    assert_eq!(sizes[0]["villagers"], 8);
}
