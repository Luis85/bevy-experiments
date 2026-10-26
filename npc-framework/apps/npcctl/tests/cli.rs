//! Task J: integration tests running the real `npcctl` binary.

use std::path::PathBuf;
use std::process::{Command, Output};

use npc_framework::kernel::ids::{SimIdAllocator, SimIndex};
use serde_json::Value;
use village_content::scenario::VillageIds;

/// What the seed-1 `village` scenario spawned, looked up in the content
/// instead of hard-coded (today: house 1, bed 2, stockpile 3, field 4,
/// villager 5; a runtime spawn gets 6). A new scenario entity renumbers
/// these (`.claude/rules/content.md`, "SimId stability").
struct Village {
    ids: VillageIds,
    /// Sim entities after the scenario spawn.
    entities: usize,
    /// The id the first runtime spawn (`SpawnVillager`) gets.
    next_id: u64,
}

fn village() -> Village {
    let app = village_content::replay::new_sim_app(1, "village", 1);
    let world = app.world();
    Village {
        ids: *world.resource::<VillageIds>(),
        entities: world.resource::<SimIndex>().map.len(),
        next_id: u64::from(world.resource::<SimIdAllocator>().next.max(1)),
    }
}

/// The scenario villager's `SimId`.
fn villager() -> u64 {
    u64::from(village().ids.villager.0)
}

fn npcctl(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_npcctl"))
        .args(args)
        .output()
        .expect("npcctl runs")
}

fn code(out: &Output) -> i32 {
    out.status.code().expect("exited normally")
}

fn stdout(out: &Output) -> String {
    String::from_utf8(out.stdout.clone()).expect("utf-8 stdout")
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// Runs `args`, expects exit `want`, parses stdout as one JSON value.
fn json(args: &[&str], want: i32) -> Value {
    let out = npcctl(args);
    assert_eq!(
        code(&out),
        want,
        "{args:?}\nstdout: {}\nstderr: {}",
        stdout(&out),
        stderr(&out)
    );
    serde_json::from_str(&stdout(&out))
        .unwrap_or_else(|e| panic!("{args:?}: stdout is not JSON ({e}):\n{}", stdout(&out)))
}

/// A fresh, empty directory for one test.
fn temp_dir(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("npcctl")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn path_str(p: &std::path::Path) -> &str {
    p.to_str().unwrap()
}

#[test]
fn run_outputs_summary_json() {
    let v = json(&["run", "--seed", "1", "--ticks", "5000"], 0);
    assert!(v["final_hash"].is_u64(), "{v}");
    assert_eq!(v["final_tick"], 5000);
    let days = v["days"].as_array().expect("days array");
    assert_eq!(days.len(), 3, "5000 ticks of 2400 span 3 days: {v}");
    let day0 = &days[0];
    for field in ["meals", "work_shifts", "ticks_asleep", "aborts"] {
        assert!(day0[field].is_u64(), "day field {field}: {day0}");
    }
    assert!(
        day0["meals"].as_u64().unwrap() >= 1,
        "villager eats on day 1: {day0}"
    );
    assert!(
        day0["ticks_asleep"].as_u64().unwrap() > 0,
        "villager sleeps on day 1: {day0}"
    );
    assert!(day0["min_needs"]["hunger"].is_i64());
    assert!(v["needs"]["energy"]["min"].is_i64());
    assert!(v["aborts"].is_u64());
}

#[test]
fn run_is_deterministic_and_pretty_and_out() {
    let dir = temp_dir("run_out");
    let file = dir.join("summary.json");
    let a = json(&["run", "--seed", "3", "--ticks", "600"], 0);
    let out = npcctl(&[
        "run",
        "--seed",
        "3",
        "--ticks",
        "600",
        "--pretty",
        "--out",
        path_str(&file),
    ]);
    assert_eq!(code(&out), 0, "{}", stderr(&out));
    assert!(stdout(&out).is_empty(), "--out writes the file, not stdout");
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.contains("\n  "), "--pretty indents");
    let b: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(a["final_hash"], b["final_hash"]);
}

#[test]
fn run_with_commands_and_record_verifies() {
    let dir = temp_dir("run_commands");
    let cmds = dir.join("cmds.json");
    // Both accepted shapes: `{tick, cmd}` objects and replay `[tick, seq, cmd]` tuples.
    let ids = village().ids;
    let (stockpile, villager) = (ids.stockpile.0, ids.villager.0);
    std::fs::write(
        &cmds,
        format!(
            r#"[
              {{"tick": 100, "cmd": {{"type": "SetFood", "stockpile": {stockpile}, "food": 9}}}},
              [300, 0, {{"type": "Order", "unit": {villager}, "order": {{"type": "GoTo", "pos": [8000, 8000]}}}}]
            ]"#
        ),
    )
    .unwrap();
    let replay = dir.join("replay.json");
    let journal = dir.join("journal.jsonl");
    let v = json(
        &[
            "run",
            "--seed",
            "1",
            "--ticks",
            "1000",
            "--commands",
            path_str(&cmds),
            "--record",
            path_str(&replay),
            "--checkpoint-every",
            "250",
            "--journal",
            path_str(&journal),
        ],
        0,
    );
    assert_eq!(v["commands_applied"], 2, "{v}");
    let plain = json(&["run", "--seed", "1", "--ticks", "1000"], 0);
    assert_ne!(
        v["final_hash"], plain["final_hash"],
        "commands change the run"
    );

    let r: Value = serde_json::from_str(&std::fs::read_to_string(&replay).unwrap()).unwrap();
    assert_eq!(r["commands"].as_array().unwrap().len(), 2);
    assert_eq!(r["checkpoints"].as_array().unwrap().len(), 4);
    assert_eq!(r["checkpoints"][3][1], v["final_hash"]);

    let lines = std::fs::read_to_string(&journal).unwrap();
    assert!(lines.lines().count() > 10);
    assert!(lines.lines().any(|l| l.contains(r#""type":"Command""#)));

    let report = json(&["replay", "verify", path_str(&replay)], 0);
    assert_eq!(report["ok"], true, "{report}");
    assert_eq!(report["checked"], 4);
}

#[test]
fn run_rejects_unknown_unit_command_with_json_error() {
    let dir = temp_dir("bad_command");
    let cmds = dir.join("cmds.json");
    std::fs::write(
        &cmds,
        r#"[{"tick": 10, "cmd": {"type": "Despawn", "unit": 999}}]"#,
    )
    .unwrap();
    let v = json(
        &[
            "run",
            "--seed",
            "1",
            "--ticks",
            "100",
            "--commands",
            path_str(&cmds),
        ],
        2,
    );
    assert_eq!(v["reason"]["error"], "UnknownUnit", "{v}");
    assert_eq!(v["reason"]["unit"], 999);
}

#[test]
fn inspect_unit_at_tick() {
    let village = village();
    let unit = village.ids.villager.0.to_string();
    let v = json(
        &["inspect", "--seed", "1", "--tick", "300", "--unit", &unit],
        0,
    );
    assert_eq!(v["sim_id"], village.ids.villager.0);
    assert_eq!(v["kind"], "villager");
    assert!(v["needs"]["hunger"]["value"].is_i64(), "{v}");
    assert!(v["tree_path"].is_array());
    assert!(
        !v["recent_journal"].as_array().unwrap().is_empty(),
        "inspect records the journal so the unit's history is there: {v}"
    );
    let world = json(&["inspect", "--seed", "1", "--tick", "300", "--all"], 0);
    assert_eq!(world["tick"], 300);
    assert_eq!(
        world["entities"].as_array().unwrap().len(),
        village.entities
    );
    let default = json(&["inspect", "--seed", "1", "--tick", "300"], 0);
    assert_eq!(default["tick"], 300, "no --unit means the world snapshot");

    let missing = json(
        &["inspect", "--seed", "1", "--tick", "10", "--unit", "999"],
        2,
    );
    assert_eq!(missing["reason"]["error"], "UnknownUnit", "{missing}");
}

#[test]
fn journal_filter_kinds() {
    let out = npcctl(&[
        "journal",
        "--seed",
        "1",
        "--ticks",
        "3000",
        "--kind",
        "LeafStart,Abort",
    ]);
    assert_eq!(code(&out), 0, "{}", stderr(&out));
    let text = stdout(&out);
    let mut n = 0;
    for line in text.lines() {
        let e: Value = serde_json::from_str(line).unwrap_or_else(|e| panic!("{e}: {line}"));
        let kind = e["kind"]["type"].as_str().unwrap();
        assert!(["LeafStart", "Abort"].contains(&kind), "{line}");
        n += 1;
    }
    assert!(n > 5, "found {n} entries");
}

#[test]
fn journal_unit_range_grep_limit() {
    let villager = villager();
    let unit = villager.to_string();
    let out = npcctl(&[
        "journal",
        "--seed",
        "1",
        "--ticks",
        "3000",
        "--unit",
        &unit,
        "--from",
        "100",
        "--to",
        "2000",
        "--grep",
        "MoveToDest",
        "--limit",
        "3",
    ]);
    assert_eq!(code(&out), 0, "{}", stderr(&out));
    let lines: Vec<Value> = stdout(&out)
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(lines.len(), 3, "{lines:?}");
    let mut last = (0, 0);
    for e in &lines {
        assert_eq!(e["unit"], villager);
        let tick = e["tick"].as_u64().unwrap();
        assert!((100..=2000).contains(&tick));
        assert!(e.to_string().contains("MoveToDest"));
        let key = (tick, e["seq"].as_u64().unwrap());
        assert!(key > last, "(tick, seq) ascending");
        last = key;
    }
}

#[test]
fn journal_unknown_kind_is_usage_error() {
    let out = npcctl(&["journal", "--seed", "1", "--ticks", "10", "--kind", "Bogus"]);
    assert_eq!(code(&out), 2);
}

#[test]
fn replay_record_verify_roundtrip() {
    let dir = temp_dir("replay_roundtrip");
    let file = dir.join("replay.json");
    let out = npcctl(&[
        "replay",
        "record",
        "--seed",
        "2",
        "--ticks",
        "1200",
        "--checkpoint-every",
        "300",
        "--out",
        path_str(&file),
    ]);
    assert_eq!(code(&out), 0, "{}", stderr(&out));
    let report = json(&["replay", "verify", path_str(&file)], 0);
    assert_eq!(report["ok"], true, "{report}");
    assert_eq!(report["checked"], 4);
    assert!(report["first_mismatch"].is_null());

    // Tamper with a checkpoint: verification fails with exit 1.
    let mut r: Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    r["checkpoints"][1][1] = Value::from(12345u64);
    let bad = dir.join("tampered.json");
    std::fs::write(&bad, r.to_string()).unwrap();
    let report = json(&["replay", "verify", path_str(&bad)], 1);
    assert_eq!(report["ok"], false);
    assert_eq!(report["first_mismatch"][0], 600);

    // Not a replay at all: usage/input error.
    let junk = dir.join("junk.json");
    std::fs::write(&junk, "{not json").unwrap();
    assert_eq!(code(&npcctl(&["replay", "verify", path_str(&junk)])), 2);
}

#[test]
fn diverge_two_seeds() {
    let v = json(
        &[
            "diverge", "--seed-a", "1", "--seed-b", "2", "--ticks", "3000",
        ],
        1,
    );
    let tick = v["first_divergent_tick"]
        .as_u64()
        .expect("a divergent tick");
    assert!(tick <= 3000);
    let entities = v["entities"].as_array().unwrap();
    assert!(!entities.is_empty(), "{v}");
    let e = &entities[0];
    assert!(e["sim_id"].is_u64());
    assert!(!e["components_differing"].as_array().unwrap().is_empty());
    assert!(e["a"].is_object() && e["b"].is_object(), "{e}");

    let same = json(
        &[
            "diverge", "--seed-a", "4", "--seed-b", "4", "--ticks", "500",
        ],
        0,
    );
    assert!(same["first_divergent_tick"].is_null(), "{same}");
}

#[test]
fn diverge_two_replay_files() {
    let dir = temp_dir("diverge_files");
    let (a, b) = (dir.join("a.json"), dir.join("b.json"));
    for (file, seed) in [(&a, "1"), (&b, "2")] {
        let out = npcctl(&[
            "replay",
            "record",
            "--seed",
            seed,
            "--ticks",
            "3000",
            "--out",
            path_str(file),
        ]);
        assert_eq!(code(&out), 0, "{}", stderr(&out));
    }
    let from_files = json(&["diverge", "--a", path_str(&a), "--b", path_str(&b)], 1);
    let from_seeds = json(
        &[
            "diverge", "--seed-a", "1", "--seed-b", "2", "--ticks", "3000",
        ],
        1,
    );
    assert_eq!(
        from_files["first_divergent_tick"],
        from_seeds["first_divergent_tick"]
    );
}

#[test]
fn bench_small() {
    let v = json(&["bench", "--villagers", "1,10", "--ticks", "100"], 0);
    let results = v["results"].as_array().unwrap();
    assert_eq!(results.len(), 2, "{v}");
    for (r, n) in results.iter().zip([1, 10]) {
        assert_eq!(r["villagers"], n);
        assert_eq!(r["ticks"], 100);
        assert!(r["ticks_per_sec"].as_f64().unwrap() > 0.0);
        assert!(r["wall_ms"].as_f64().unwrap() > 0.0);
        let mechanics: Vec<&str> = r["mechanics"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["mechanic"].as_str().unwrap())
            .collect();
        for want in ["think", "needs"] {
            assert!(mechanics.contains(&want), "{n} villagers: {mechanics:?}");
        }
        let think = r["mechanics"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["mechanic"] == "think")
            .unwrap();
        assert_eq!(
            think["samples"], 100,
            "one sample per measured tick: {think}"
        );
        let systems = r["systems"].as_array().unwrap();
        assert!(!systems.is_empty() && systems.len() <= 15);
    }
}

#[test]
fn bench_baseline_regressions_exit_1() {
    let dir = temp_dir("bench_baseline");
    let base = dir.join("base.json");
    let out = npcctl(&[
        "bench",
        "--villagers",
        "1",
        "--ticks",
        "50",
        "--out",
        path_str(&base),
    ]);
    assert_eq!(code(&out), 0, "{}", stderr(&out));
    let mut b: Value = serde_json::from_str(&std::fs::read_to_string(&base).unwrap()).unwrap();

    // A baseline far slower than any real run: no regression.
    let slow = dir.join("slow.json");
    for m in b["results"][0]["mechanics"].as_array_mut().unwrap() {
        m["p95_us"] = Value::from(1.0e9);
        m["total_us"] = Value::from(1.0e12);
    }
    std::fs::write(&slow, b.to_string()).unwrap();
    let v = json(
        &[
            "bench",
            "--villagers",
            "1",
            "--ticks",
            "50",
            "--baseline",
            path_str(&slow),
        ],
        0,
    );
    assert_eq!(
        v["comparison"]["regressions"].as_array().unwrap().len(),
        0,
        "{v}"
    );

    // A baseline far faster than any real run: `think` regresses.
    let fast = dir.join("fast.json");
    for m in b["results"][0]["mechanics"].as_array_mut().unwrap() {
        m["p95_us"] = Value::from(1.0e-6);
        m["total_us"] = Value::from(1.0e-6);
    }
    std::fs::write(&fast, b.to_string()).unwrap();
    let v = json(
        &[
            "bench",
            "--villagers",
            "1",
            "--ticks",
            "50",
            "--baseline",
            path_str(&fast),
            "--min-us",
            "0",
            "--min-samples",
            "1",
        ],
        1,
    );
    let regressions = v["comparison"]["regressions"].as_array().unwrap();
    assert!(regressions.iter().any(|r| r["mechanic"] == "think"), "{v}");
    assert_eq!(v["comparison"]["metric"], "p95");

    // The mean metric compares total_us / samples.
    let v = json(
        &[
            "bench",
            "--villagers",
            "1",
            "--ticks",
            "50",
            "--baseline",
            path_str(&fast),
            "--min-us",
            "0",
            "--min-samples",
            "1",
            "--metric",
            "mean",
        ],
        1,
    );
    assert_eq!(v["comparison"]["metric"], "mean");
    let r = &v["comparison"]["regressions"][0];
    assert!(
        r["value_us"].as_f64().unwrap() > r["baseline_us"].as_f64().unwrap(),
        "{r}"
    );
    json(
        &[
            "bench",
            "--villagers",
            "1",
            "--ticks",
            "50",
            "--baseline",
            path_str(&slow),
            "--metric",
            "mean",
        ],
        0,
    );
}

#[test]
fn usage_error_exit_2() {
    assert_eq!(code(&npcctl(&[])), 2, "no subcommand");
    assert_eq!(code(&npcctl(&["frobnicate"])), 2, "unknown subcommand");
    assert_eq!(code(&npcctl(&["run", "--ticks", "abc"])), 2, "bad number");
    assert_eq!(
        code(&npcctl(&["inspect", "--tick", "5", "--unit", "5", "--all"])),
        2,
        "conflict"
    );
    assert_eq!(
        code(&npcctl(&["diverge", "--seed-a", "1"])),
        2,
        "missing --seed-b"
    );
    let missing = npcctl(&["replay", "verify", "does/not/exist.json"]);
    assert_eq!(code(&missing), 2, "missing file");
    let v: Value = serde_json::from_str(&stdout(&missing)).expect("JSON error on stdout");
    assert!(v["error"].is_string());
}

#[test]
fn help_has_examples() {
    for sub in [
        &["run"][..],
        &["inspect"],
        &["journal"],
        &["replay", "verify"],
        &["replay", "record"],
        &["diverge"],
        &["bench"],
    ] {
        let mut args = sub.to_vec();
        args.push("--help");
        let out = npcctl(&args);
        assert_eq!(code(&out), 0);
        assert!(
            stdout(&out).contains("Example: npcctl"),
            "{sub:?} --help:\n{}",
            stdout(&out)
        );
    }
}

/// C2 follow-up: command files are validated at apply time
/// (`submit_deferred`), so a command may target a unit that an earlier
/// command of the same tick spawns.
#[test]
fn same_tick_spawn_then_order_applies() {
    let dir = temp_dir("same_tick_spawn");
    let cmds = dir.join("cmds.json");
    // The spawned villager gets the first id after the scenario's.
    let new = village().next_id;
    std::fs::write(
        &cmds,
        format!(
            r#"[
          {{"tick": 50, "cmd": {{"type": "SpawnVillager", "pos": [1000, 1000]}}}},
          {{"tick": 50, "cmd": {{"type": "Order", "unit": {new}, "order": {{"type": "GoTo", "pos": [4000, 4000]}}}}}}
        ]"#
        ),
    )
    .unwrap();
    let replay = dir.join("replay.json");
    let v = json(
        &[
            "run",
            "--seed",
            "1",
            "--ticks",
            "200",
            "--commands",
            path_str(&cmds),
            "--record",
            path_str(&replay),
        ],
        0,
    );
    assert_eq!(v["commands_applied"], 2, "{v}");
    let r: Value = serde_json::from_str(&std::fs::read_to_string(&replay).unwrap()).unwrap();
    let ticks: Vec<(u64, u64)> = r["commands"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| (c[0].as_u64().unwrap(), c[1].as_u64().unwrap()))
        .collect();
    assert_eq!(
        ticks,
        [(50, 0), (50, 1)],
        "both applied on tick 50, in file order"
    );
    let report = json(&["replay", "verify", path_str(&replay)], 0);
    assert_eq!(report["ok"], true, "{report}");
    let new_arg = new.to_string();
    let unit = json(
        &[
            "inspect",
            "--seed",
            "1",
            "--tick",
            "51",
            "--unit",
            &new_arg,
            "--commands",
            path_str(&cmds),
        ],
        0,
    );
    assert_eq!(unit["sim_id"], new, "{unit}");
}

/// C2 follow-up: `journal` starts the journal before the scenario spawns
/// (tick-0 `Spawn` entries) unless `--journal-from-build false`.
#[test]
fn journal_includes_scenario_spawns_by_default() {
    let spawns = |extra: &[&str]| {
        let mut args = vec!["journal", "--seed", "1", "--ticks", "5", "--kind", "Spawn"];
        args.extend_from_slice(extra);
        let out = npcctl(&args);
        assert_eq!(code(&out), 0, "{}", stderr(&out));
        stdout(&out)
            .lines()
            .map(|l| serde_json::from_str::<Value>(l).unwrap())
            .collect::<Vec<_>>()
    };
    let on = spawns(&[]);
    let village = village();
    assert_eq!(
        on.len(),
        village.entities,
        "house, bed, stockpile, field, villager, well: {on:?}"
    );
    for (i, e) in on.iter().enumerate() {
        assert_eq!(e["tick"], 0);
        assert_eq!(e["seq"], i as u64);
        assert_eq!(e["unit"], i as u64 + 1);
    }
    let villager = on
        .iter()
        .find(|e| e["unit"] == village.ids.villager.0)
        .expect("the villager's spawn");
    assert_eq!(villager["kind"]["kind"], "villager", "{on:?}");
    assert!(spawns(&["--journal-from-build", "false"]).is_empty());
    // The rest of the journal is unchanged by the option.
    let all = |extra: &[&str]| {
        let mut args = vec!["journal", "--seed", "1", "--ticks", "300"];
        args.extend_from_slice(extra);
        let out = npcctl(&args);
        assert_eq!(code(&out), 0, "{}", stderr(&out));
        stdout(&out)
            .lines()
            .map(|l| serde_json::from_str::<Value>(l).unwrap())
            .filter(|e| e["kind"]["type"] != "Spawn")
            .map(|mut e| {
                // seq within tick 0 shifts by the 5 Spawn entries.
                e.as_object_mut().unwrap().remove("seq");
                e
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(all(&[]), all(&["--journal-from-build", "false"]));
}
