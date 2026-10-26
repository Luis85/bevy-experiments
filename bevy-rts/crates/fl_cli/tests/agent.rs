//! The agent debugging commands (spec §15 C): every test runs the `fl_cli` binary with `--json`
//! and parses its stdout, which must be exactly one JSON document.

use std::path::{Path, PathBuf};
use std::process::Command;

use fl_math::{Fx, FxVec2};
use fl_protocol::{DebugSpawnCmd, MoveCmd, PlayerId, Tick};
use fl_sim::{Replay, ReplayEvent};
use fl_test_utils::ScenarioBuilder;
use serde_json::Value;

/// The exit code, the single JSON document on stdout, and stderr.
struct Run {
    code: Option<i32>,
    json: Value,
    stderr: String,
}

/// Run `fl_cli` with `args` (which must contain `--json`).
fn fl_cli(args: &[&str]) -> Run {
    assert!(args.contains(&"--json"), "agent tests always ask for JSON");
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

/// [`fl_cli`], asserting exit code 0.
fn ok(args: &[&str]) -> Value {
    let run = fl_cli(args);
    assert_eq!(
        run.code,
        Some(0),
        "{args:?} failed: {:#}\n{}",
        run.json,
        run.stderr
    );
    run.json
}

fn s(path: &Path) -> &str {
    path.to_str().expect("UTF-8 path")
}

fn v(x: i32, y: i32) -> FxVec2 {
    FxVec2::new(Fx::from_int(x), Fx::from_int(y))
}

fn hex(value: &Value) -> &str {
    let text = value.as_str().expect("a hash string");
    assert!(
        text.starts_with("0x") && text.len() == 18,
        "not a 0x-prefixed 64-bit hex hash: {text}"
    );
    text
}

/// `run-scenario` into `out`; returns its JSON.
fn run_scenario(scenario: &str, ticks: u32, out: &Path, extra: &[&str]) -> Value {
    let ticks = ticks.to_string();
    let mut args = vec![
        "--json",
        "run-scenario",
        scenario,
        "--ticks",
        &ticks,
        "--wander",
        "100",
        "--out",
        s(out),
    ];
    args.extend_from_slice(extra);
    ok(&args)
}

fn lines(path: &Path) -> Vec<String> {
    std::fs::read_to_string(path)
        .unwrap_or_else(|err| panic!("reading {}: {err}", path.display()))
        .lines()
        .map(str::to_owned)
        .collect()
}

/// The base testbed, every unit wandering; player 0 moves #1 at tick 10 and to `target_37` at
/// tick 37; 60 ticks, hashed every tick.
fn testbed_replay(target_37: FxVec2) -> Replay {
    let mut sc = ScenarioBuilder::new()
        .scenario("base.testbed")
        .wander_all(100)
        .command(
            10,
            0,
            &[1, 2],
            MoveCmd {
                target: v(600, 500),
            },
        )
        .command(37, 0, &[1], MoveCmd { target: target_37 })
        .build();
    sc.run_ticks(60);
    sc.replay()
}

fn write_replay(dir: &Path, name: &str, replay: &Replay) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, replay.to_bytes()).expect("write the replay");
    path
}

#[test]
fn run_scenario_json_outputs_paths() {
    let tmp = tempfile::tempdir().expect("temp dir");
    let json = run_scenario(
        "base.testbed",
        30,
        tmp.path(),
        &["--dump-state-every", "10"],
    );
    let run_dir = PathBuf::from(json["run_dir"].as_str().expect("run_dir"));
    assert!(run_dir.starts_with(tmp.path()), "{}", run_dir.display());
    for file in ["log.jsonl", "journal.jsonl", "run.replay", "states.jsonl"] {
        assert!(run_dir.join(file).is_file(), "{file} missing in {json:#}");
    }
    assert_eq!(json["ticks"], 30);
    hex(&json["final_hash"]);
    hex(&json["rules_hash"]);
    assert_eq!(json["rejected_commands"], 0);
    let entries = json["journal_entries"].as_u64().expect("journal_entries");
    assert!(entries >= 1, "{json:#}");
    assert_eq!(
        lines(&run_dir.join("journal.jsonl")).len() as u64,
        entries,
        "journal_entries counts journal.jsonl's lines"
    );
    assert!(
        !lines(&run_dir.join("log.jsonl")).is_empty(),
        "the sim logs at debug level by default"
    );
    // The same logs go to stderr.
    let logged = fl_cli(&[
        "--json",
        "run-scenario",
        "base.testbed",
        "--ticks",
        "2",
        "--out",
        s(tmp.path()),
    ]);
    assert_eq!(logged.code, Some(0));
    assert!(logged.stderr.contains("fl_sim"), "{}", logged.stderr);

    let states: Vec<Value> = lines(&run_dir.join("states.jsonl"))
        .iter()
        .map(|line| serde_json::from_str(line).expect("a states line is JSON"))
        .collect();
    let ticks: Vec<u64> = states
        .iter()
        .map(|state| state["world"]["tick"].as_u64().expect("tick"))
        .collect();
    assert_eq!(ticks, [0, 10, 20, 30]);
    assert_eq!(
        states[3]["entities"].as_array().expect("entities").len(),
        10
    );
    assert!(states[3]["entities"][0]["components"]["core.pose"].is_object());

    let bytes = std::fs::read(run_dir.join("run.replay")).expect("run.replay");
    let replay = Replay::from_bytes(&bytes).expect("run.replay decodes");
    assert_eq!(replay.ticks, 30);
    let verified = ok(&["--json", "replay-verify", s(&run_dir.join("run.replay"))]);
    assert_eq!(verified["status"], "verified");
}

#[test]
fn inspect_entity_at_tick() {
    let json = ok(&[
        "--json",
        "inspect",
        "base.testbed",
        "--tick",
        "10",
        "--entity",
        "1",
    ]);
    assert_eq!(json["sim_id"], 1);
    assert!(json["components"]["core.pose"].is_object(), "{json:#}");

    let world = ok(&["--json", "inspect", "base.testbed", "--tick", "10"]);
    assert_eq!(world["tick"], 10);
    assert_eq!(world["entities"].as_array().expect("entities").len(), 10);

    let missing = fl_cli(&[
        "--json",
        "inspect",
        "base.testbed",
        "--tick",
        "10",
        "--entity",
        "99",
    ]);
    assert_eq!(missing.code, Some(1));
    assert!(
        missing.json["error"]
            .as_str()
            .is_some_and(|e| e.contains("99")),
        "{:#}",
        missing.json
    );
}

#[test]
fn journal_filter_by_kind() {
    // A journal with several kinds: commands applied, move orders and a spawn.
    let mut sc = ScenarioBuilder::new()
        .scenario("base.testbed")
        .command(
            3,
            0,
            &[1, 2],
            MoveCmd {
                target: v(600, 500),
            },
        )
        .command(
            5,
            1,
            &[],
            DebugSpawnCmd {
                unit: "base.su.t34".to_owned(),
                owner: PlayerId(1),
                pos: v(500, 700),
                heading: fl_math::Angle::ZERO,
            },
        )
        .build();
    sc.run_ticks(10);
    let tmp = tempfile::tempdir().expect("temp dir");
    let path = tmp.path().join("journal.jsonl");
    let text: String = sc
        .journal()
        .iter()
        .map(|entry| serde_json::to_string(entry).expect("an entry serializes") + "\n")
        .collect();
    std::fs::write(&path, text).expect("write the journal");
    let kinds: std::collections::BTreeSet<&str> =
        sc.journal().iter().map(|e| e.kind.as_str()).collect();
    assert!(kinds.len() >= 3, "{kinds:?}");

    let json = ok(&["--json", "journal", s(&path), "--kind", "core.unit_spawned"]);
    let entries = json["entries"].as_array().expect("entries");
    assert_eq!(entries.len(), 1, "{json:#}");
    assert_eq!(entries[0]["kind"], "core.unit_spawned");
    assert_eq!(entries[0]["tick"], 5);
    assert_eq!(json["matched"], 1);

    // The other filters, combined: player 0's records of ticks 3..=3, at most 2.
    let json = ok(&[
        "--json",
        "journal",
        s(&path),
        "--player",
        "0",
        "--from",
        "3",
        "--to",
        "3",
        "--limit",
        "2",
    ]);
    let entries = json["entries"].as_array().expect("entries");
    let expected = sc
        .journal()
        .iter()
        .filter(|e| e.tick == Tick(3) && e.player == Some(PlayerId(0)))
        .count();
    assert!(expected > 2, "the limit must cut: {expected}");
    assert_eq!(entries.len(), 2);
    assert_eq!(json["matched"], expected);
    let by_id = ok(&["--json", "journal", s(&path), "--sim-id", "2"]);
    for entry in by_id["entries"].as_array().expect("entries") {
        assert_eq!(entry["sim_id"], 2);
    }
    assert!(by_id["matched"].as_u64().expect("matched") >= 1);

    // A run directory works too.
    let run = run_scenario("base.testbed", 5, tmp.path(), &[]);
    let json = ok(&[
        "--json",
        "journal",
        run["run_dir"].as_str().expect("run_dir"),
        "--kind",
        "core.command_applied",
    ]);
    let entries = json["entries"].as_array().expect("entries");
    assert!(!entries.is_empty());
    assert!(entries.iter().all(|e| e["kind"] == "core.command_applied"));
}

#[test]
fn replay_verify_ok_and_tampered() {
    let tmp = tempfile::tempdir().expect("temp dir");
    let replay = testbed_replay(v(800, 200));
    let good = write_replay(tmp.path(), "good.replay", &replay);
    let json = ok(&["--json", "replay-verify", s(&good)]);
    assert_eq!(json["status"], "verified");
    assert_eq!(json["ticks"], 60);

    let mut tampered = replay;
    let input = tampered
        .inputs
        .iter_mut()
        .find(|input| input.tick == Tick(37))
        .expect("the tick 37 input");
    input.commands[0].kind.payload = postcard::to_allocvec(&MoveCmd {
        target: v(100, 100),
    })
    .expect("encode");
    let bad = write_replay(tmp.path(), "bad.replay", &tampered);
    let run = fl_cli(&["--json", "replay-verify", s(&bad)]);
    assert_eq!(run.code, Some(1), "{:#}", run.json);
    assert_eq!(run.json["status"], "diverged");
    assert_eq!(run.json["tick"], 37);
    assert_ne!(hex(&run.json["expected"]), hex(&run.json["actual"]));
}

#[test]
fn replay_dump_json_lists_inputs() {
    let tmp = tempfile::tempdir().expect("temp dir");
    let replay = testbed_replay(v(800, 200));
    let path = write_replay(tmp.path(), "dump.replay", &replay);
    let json = ok(&["replay", "dump", s(&path), "--json"]);
    assert_eq!(json, replay.to_json());
    let ticks: Vec<u64> = json["inputs"]
        .as_array()
        .expect("inputs")
        .iter()
        .map(|input| input["tick"].as_u64().expect("tick"))
        .collect();
    assert_eq!(ticks, [0, 10, 37], "wander at 0, moves at 10 and 37");
}

#[test]
fn save_then_load_continues_identically() {
    let tmp = tempfile::tempdir().expect("temp dir");
    let full = run_scenario("base.testbed", 100, tmp.path(), &[]);
    let save = tmp.path().join("at40.save");
    let saved = ok(&[
        "--json",
        "save",
        "base.testbed",
        "--wander",
        "100",
        "--tick",
        "40",
        "--out",
        s(&save),
    ]);
    assert_eq!(saved["tick"], 40);
    assert!(save.is_file());
    let loaded = ok(&["--json", "load", s(&save), "--ticks", "60"]);
    assert_eq!(loaded["tick"], 100);
    assert_eq!(hex(&loaded["final_hash"]), hex(&full["final_hash"]));
    assert_ne!(
        hex(&saved["hash"]),
        hex(&full["final_hash"]),
        "the units moved after tick 40"
    );

    // From a run directory: its replay to tick 20, saved with the rest of its inputs, then
    // loaded and run to 60, ends where the straight 60-tick run did.
    let run = run_scenario("base.testbed", 60, tmp.path(), &[]);
    let dir = run["run_dir"].as_str().expect("run_dir");
    let save = tmp.path().join("run_at20.save");
    let saved = ok(&["--json", "save", dir, "--tick", "20", "--out", s(&save)]);
    assert_eq!(saved["tick"], 20);
    let loaded = ok(&["--json", "load", s(&save), "--ticks", "40"]);
    assert_eq!(loaded["tick"], 60);
    assert_eq!(hex(&loaded["final_hash"]), hex(&run["final_hash"]));
    assert_ne!(hex(&saved["hash"]), hex(&run["final_hash"]));
}

#[test]
fn bisect_desync_finds_tick() {
    let tmp = tempfile::tempdir().expect("temp dir");
    let a = write_replay(tmp.path(), "a.replay", &testbed_replay(v(800, 200)));
    let b = write_replay(tmp.path(), "b.replay", &testbed_replay(v(200, 800)));
    let run = fl_cli(&["--json", "bisect-desync", s(&a), s(&b)]);
    assert_eq!(run.code, Some(1), "diverged: {:#}", run.json);
    assert_eq!(run.json["diverged"], true);
    assert_eq!(run.json["tick"], 37, "{:#}", run.json);
    let ids = run.json["entities"].as_array().expect("entities");
    assert_eq!(ids, &[Value::from(1)], "only #1 got another order");
    assert_eq!(run.json["ticks_a"], 60);
    assert_eq!(run.json["ticks_b"], 60);

    // A prefix of a replay matches it, but the lengths differ: not "identical".
    let full = testbed_replay(v(800, 200));
    let mut prefix = full.clone();
    prefix.ticks = 30;
    prefix.inputs.retain(|input| input.tick < Tick(30));
    prefix.hashes.retain(|(tick, _)| *tick < Tick(30));
    let short = write_replay(tmp.path(), "short.replay", &prefix);
    let run = fl_cli(&["--json", "bisect-desync", s(&a), s(&short)]);
    assert_eq!(run.code, Some(1), "{:#}", run.json);
    assert_eq!(run.json["diverged"], false);
    assert_eq!(run.json["ticks_compared"], 30);
    assert_eq!(
        (&run.json["ticks_a"], &run.json["ticks_b"]),
        (&Value::from(60), &Value::from(30))
    );
    assert!(
        run.json["message"]
            .as_str()
            .is_some_and(|m| m.contains("different lengths")),
        "{:#}",
        run.json
    );

    // No ticks: nothing to compare.
    let mut empty = full;
    empty.ticks = 0;
    empty.inputs.clear();
    empty.hashes.clear();
    let empty = write_replay(tmp.path(), "empty.replay", &empty);
    let run = fl_cli(&["--json", "bisect-desync", s(&a), s(&empty)]);
    assert_eq!(run.code, Some(1), "{:#}", run.json);
    assert!(
        run.json["error"]
            .as_str()
            .is_some_and(|e| e.contains("nothing to compare")),
        "{:#}",
        run.json
    );

    // A run directory against its own replay: identical.
    let run_json = run_scenario("base.testbed", 20, tmp.path(), &[]);
    let dir = run_json["run_dir"].as_str().expect("run_dir");
    let replay = Path::new(dir).join("run.replay");
    let same = ok(&["--json", "bisect-desync", dir, s(&replay)]);
    assert_eq!(same["diverged"], false);
    assert_eq!(same["ticks_compared"], 20);
    assert_eq!(same["ticks_a"], 20);
    assert_eq!(same["ticks_b"], 20);
}

#[test]
fn scenario_file_path_accepted() {
    let tmp = tempfile::tempdir().expect("temp dir");
    let text =
        std::fs::read_to_string(fl_catalog::base_mod_dir().join("scenarios/testbed.scenario.ron"))
            .expect("the testbed scenario");
    let copy = text.replace("\"base.testbed\"", "\"agent.testbed_copy\"");
    assert_ne!(copy, text);
    let path = tmp.path().join("copy.scenario.ron");
    std::fs::write(&path, copy).expect("write the scenario");

    let by_file = run_scenario(s(&path), 30, tmp.path(), &[]);
    let by_id = run_scenario("base.testbed", 30, tmp.path(), &[]);
    assert_eq!(hex(&by_file["final_hash"]), hex(&by_id["final_hash"]));

    let replay = Path::new(by_file["run_dir"].as_str().expect("run_dir")).join("run.replay");
    let verified = ok(&["--json", "replay-verify", s(&replay)]);
    assert_eq!(verified["status"], "verified");
    let world = ok(&["--json", "inspect", s(&path), "--tick", "5"]);
    assert_eq!(world["tick"], 5);
}

#[test]
fn non_reproducible_replay_reported() {
    let tmp = tempfile::tempdir().expect("temp dir");
    let mut replay = testbed_replay(v(800, 200));
    replay.events.push(ReplayEvent::ContentReload {
        tick: Tick(20),
        rules_hash: 1,
    });
    let path = write_replay(tmp.path(), "reloaded.replay", &replay);
    let run = fl_cli(&["--json", "replay-verify", s(&path)]);
    assert_eq!(run.code, Some(3), "{:#}", run.json);
    assert_eq!(run.json["status"], "non-reproducible");
}

#[test]
fn errors_are_json_too() {
    let run = fl_cli(&["--json", "replay-verify", "no/such/file.replay"]);
    assert_eq!(run.code, Some(1));
    assert!(run.json["error"].is_string(), "{:#}", run.json);

    // Usage errors, clap's and ours, exit 2.
    let run = fl_cli(&["--json", "no-such-command"]);
    assert_eq!(run.code, Some(2));
    assert!(run.json["error"].is_string(), "{:#}", run.json);
    let run = fl_cli(&[
        "--json",
        "inspect",
        "base.testbed",
        "--tick",
        "5",
        "--wander",
    ]);
    assert_eq!(run.code, Some(2), "--wander needs a value");
    assert!(
        run.json["error"]
            .as_str()
            .is_some_and(|e| e.contains("--wander")),
        "{:#}",
        run.json
    );
    let run = fl_cli(&[
        "--json",
        "run-scenario",
        "base.testbed",
        "--ticks",
        "5",
        "--dump-state-every",
        "0",
    ]);
    assert_eq!(run.code, Some(2));
    assert!(run.json["error"].is_string(), "{:#}", run.json);

    // An unknown scenario fails before a run directory is made.
    let tmp = tempfile::tempdir().expect("temp dir");
    let run = fl_cli(&[
        "--json",
        "run-scenario",
        "base.no_such_scenario",
        "--ticks",
        "5",
        "--out",
        s(tmp.path()),
    ]);
    assert_eq!(run.code, Some(1), "{:#}", run.json);
    assert!(
        run.json["error"]
            .as_str()
            .is_some_and(|e| e.contains("base.no_such_scenario")),
        "{:#}",
        run.json
    );
    let made: Vec<_> = std::fs::read_dir(tmp.path())
        .expect("read the out dir")
        .collect();
    assert!(made.is_empty(), "nothing created: {made:?}");
}

#[test]
fn help_and_version_are_json() {
    let help = ok(&["--json", "--help"]);
    assert!(
        help["help"]
            .as_str()
            .is_some_and(|h| h.contains("run-scenario")),
        "{help:#}"
    );
    let help = ok(&["inspect", "--help", "--json"]);
    assert!(
        help["help"]
            .as_str()
            .is_some_and(|h| h.contains("--entity")),
        "{help:#}"
    );
    let version = ok(&["--json", "--version"]);
    assert!(
        version["version"]
            .as_str()
            .is_some_and(|v| v.starts_with("fl_cli ")),
        "{version:#}"
    );
}

#[test]
fn query_lists_debug_queries() {
    let json = ok(&[
        "--json",
        "query",
        "base.testbed",
        "--tick",
        "0",
        "--name",
        "core.debug_queries",
    ]);
    let queries = json["queries"].as_array().expect("a queries array");
    assert!(
        queries.iter().any(|name| name == "core.debug_queries"),
        "{json:#}"
    );
}

#[test]
fn query_unknown_name_fails() {
    let run = fl_cli(&[
        "--json",
        "query",
        "base.testbed",
        "--tick",
        "0",
        "--name",
        "nope",
        "--args",
        "{\"x\": 1}",
    ]);
    assert_eq!(run.code, Some(1), "{:#}", run.json);
    assert!(
        run.stderr.contains("unknown debug query 'nope'"),
        "{}",
        run.stderr
    );
    assert_eq!(run.json["error"], "unknown debug query 'nope'");

    // Args that are not JSON are a usage error.
    let run = fl_cli(&[
        "--json",
        "query",
        "base.testbed",
        "--tick",
        "0",
        "--name",
        "core.debug_queries",
        "--args",
        "{not json",
    ]);
    assert_eq!(run.code, Some(2), "{:#}", run.json);
}
