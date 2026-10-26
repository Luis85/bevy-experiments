//! `npcctl serve | agent | world | session | mcp | replay verify-session`
//! against a session: in-process (the library's `Session` on a free port)
//! for the stateless clients, and the real `serve` binary for the host.

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Output, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use npc_framework::agent_io::scenario::{ScenarioDoc, ScenarioLoaders};
use npc_framework::agent_io::session::{Session, SessionOptions};
use serde_json::{Value, json};
use village_content::scenario::VillageIds;

fn scenario_file() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../content/village_content/scenarios/agent_village.json")
}

/// Ids in `agent_village.json`, looked up by loading it in process instead of
/// hard-coded (today the actor is 6, after the plot's 1..=5, and the
/// stockpile 3): a new scenario entity renumbers them
/// (`.claude/rules/content.md`, "SimId stability").
struct ScenarioIds {
    /// `actors[0]`, the unit the scenario attaches controller `agent` to.
    actor: u64,
    /// The plot's stockpile (`eat@<id>`).
    stockpile: u64,
}

fn scenario_ids() -> ScenarioIds {
    let text = std::fs::read_to_string(scenario_file()).unwrap();
    let doc = ScenarioDoc::parse(&text).unwrap();
    let loaded = village_content::agent::load_scenario(&doc, 1).expect("the scenario loads");
    let ids = *loaded.app.world().resource::<VillageIds>();
    ScenarioIds {
        actor: u64::from(loaded.actors[0].0),
        stockpile: u64::from(ids.stockpile.0),
    }
}

/// A child process (`npcctl serve`, `npcctl mcp`) that is killed when the
/// guard drops, so a failing test does not leak a server that keeps the
/// test binary's target exe locked (Windows).
struct KillOnDrop(Child);

impl Drop for KillOnDrop {
    fn drop(&mut self) {
        // Already exited (the test stopped it) is fine: kill then fails.
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("npcctl_agent")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A session in this process on a free port; returns its URL and thread.
fn in_process_session() -> (String, std::thread::JoinHandle<()>) {
    let (tx, rx) = mpsc::channel();
    let h = std::thread::spawn(move || {
        let mut loaders = ScenarioLoaders::default();
        village_content::agent::register_scenario_loader(&mut loaders);
        let text = std::fs::read_to_string(scenario_file()).unwrap();
        let doc = ScenarioDoc::parse(&text).unwrap();
        let options = SessionOptions {
            port: 0,
            tick_hz: 50,
            ..SessionOptions::default()
        };
        let s = Session::start(loaders, doc, options).expect("session");
        tx.send(s.url()).unwrap();
        s.run().expect("runs");
    });
    (rx.recv_timeout(Duration::from_secs(60)).unwrap(), h)
}

fn npcctl(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_npcctl"))
        .args(args)
        .output()
        .expect("npcctl runs")
}

fn run(args: &[&str], want: i32) -> String {
    let out = npcctl(args);
    let stdout = String::from_utf8(out.stdout.clone()).unwrap();
    assert_eq!(
        out.status.code(),
        Some(want),
        "{args:?}\nstdout: {stdout}\nstderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    stdout
}

fn json_out(args: &[&str], want: i32) -> Value {
    let s = run(args, want);
    serde_json::from_str(&s).unwrap_or_else(|e| panic!("{args:?}: not JSON ({e}): {s}"))
}

#[test]
fn cli_clients_against_an_in_process_session() {
    let (url, h) = in_process_session();
    let u = url.as_str();
    let ids = scenario_ids();
    let unit = ids.actor.to_string();
    let unit = unit.as_str();
    // The scenario pre-attached `agent` (Direct) to actor 0.
    let units = json_out(&["agent", "list-units", "--url", u], 0);
    let actor = units["units"]
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["id"] == ids.actor)
        .expect("the actor")
        .clone();
    assert_eq!(actor["controller"]["id"], "agent");
    assert_eq!(actor["controllable"], true);

    let obs = json_out(&["agent", "observe", "--unit", unit, "--url", u], 0);
    assert_eq!(obs["controller"]["mode"], "Direct");
    let text = run(
        &[
            "agent", "observe", "--unit", unit, "--format", "text", "--url", u,
        ],
        0,
    );
    let intro = format!("You are villager #{unit} (controller `agent`, Direct mode");
    assert!(text.starts_with(&intro), "{text}");
    let eat = format!("[eat@{}]", ids.stockpile);
    assert!(text.contains(&eat), "{text}");

    // Act, then wait for it with a long-poll.
    let seq = obs["seq"].as_u64().unwrap().to_string();
    let r = json_out(
        &[
            "agent",
            "act",
            "--unit",
            unit,
            "--controller",
            "agent",
            "--verb",
            "walk_to",
            "--pos",
            "2600,1000",
            "--url",
            u,
        ],
        0,
    );
    assert_eq!(r["accepted"], true, "{r}");
    let handle = r["handle"].clone();
    let mut since = seq;
    let mut done = false;
    for _ in 0..20 {
        let ev = json_out(
            &[
                "agent",
                "events",
                "--unit",
                unit,
                "--since-seq",
                &since,
                "--kinds",
                "ExternalAction",
                "--timeout-ms",
                "5000",
                "--url",
                u,
            ],
            0,
        );
        since = ev["next_seq"].to_string();
        done = ev["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["event"]["handle"] == handle && e["event"]["status"] == "done");
        if done {
            break;
        }
    }
    assert!(done, "the walk finished");

    // A rejection exits 1 with the reason.
    let r = json_out(
        &[
            "agent",
            "act",
            "--unit",
            unit,
            "--controller",
            "intruder",
            "--verb",
            "wait",
            "--url",
            u,
        ],
        1,
    );
    assert_eq!(r["reason"], "not_controller");
    // An RPC error exits 2.
    let e = json_out(&["agent", "observe", "--unit", "999", "--url", u], 2);
    assert_eq!(e["code"], -23401, "{e}");

    let stats = json_out(&["agent", "stats", "--controller", "agent", "--url", u], 0);
    assert_eq!(stats["controllers"][0]["accepted"], 1, "{stats}");

    let state = json_out(&["world", "state", "--url", u], 0);
    let entities = state["entities"].as_array().unwrap().len() as u64;
    assert!(entities >= ids.actor, "the plot and the actor: {entities}");
    let reset = json_out(&["world", "reset", "--seed", "4", "--url", u], 0);
    assert_eq!(reset["seed"], 4);
    assert!(reset["tick"].as_u64().unwrap() < 5, "{reset}");
    let loaded = json_out(
        &[
            "world",
            "load-scenario",
            scenario_file().to_str().unwrap(),
            "--url",
            u,
        ],
        0,
    );
    assert_eq!(loaded["scenario"], "agent_village");

    let info = json_out(&["session", "info", "--url", u], 0);
    assert!(
        info["methods"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m == "agent/act")
    );
    json_out(&["session", "stop", "--url", u], 0);
    h.join().unwrap();

    // Nobody listening: exit 2 with a hint.
    let e = json_out(&["agent", "list-units", "--url", u], 2);
    assert!(e["error"].as_str().unwrap().contains("npcctl serve"), "{e}");
}

/// Spawns `npcctl serve` and returns it (killed on drop) with its URL (from
/// its first line).
fn serve(extra: &[&str]) -> (KillOnDrop, String) {
    let mut args = vec![
        "serve",
        "--scenario",
        scenario_file().to_str().unwrap().to_owned().leak(),
        "--port",
        "0",
        "--tick-hz",
        "100",
    ];
    args.extend_from_slice(extra);
    let mut child = KillOnDrop(
        Command::new(env!("CARGO_BIN_EXE_npcctl"))
            .args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("serve starts"),
    );
    let mut line = String::new();
    BufReader::new(child.0.stdout.as_mut().unwrap())
        .read_line(&mut line)
        .unwrap();
    let ready: Value = serde_json::from_str(&line).unwrap_or_else(|e| panic!("{e}: {line}"));
    (child, ready["listening"].as_str().unwrap().to_owned())
}

#[test]
fn serve_records_a_session_that_verifies_without_the_agent() {
    let dir = temp_dir("serve");
    let record = dir.join("session.json");
    let (mut child, url) = serve(&["--record", record.to_str().unwrap()]);
    let u = url.as_str();
    let ids = scenario_ids();
    let unit = ids.actor.to_string();
    let unit = unit.as_str();
    let eat = format!("eat@{}", ids.stockpile);
    let r = json_out(
        &[
            "agent",
            "act",
            "--unit",
            unit,
            "--controller",
            "agent",
            "--affordance",
            &eat,
            "--url",
            u,
        ],
        0,
    );
    assert_eq!(r["accepted"], true, "{r}");
    json_out(
        &[
            "agent",
            "act",
            "--unit",
            unit,
            "--controller",
            "agent",
            "--verb",
            "wait",
            "--args",
            "{\"ticks\":30}",
            "--queue",
            "--url",
            u,
        ],
        0,
    );
    std::thread::sleep(Duration::from_millis(1500));
    json_out(&["session", "stop", "--url", u], 0);
    let status = child.0.wait().unwrap();
    assert!(status.success());
    let report = json_out(&["replay", "verify-session", record.to_str().unwrap()], 0);
    assert_eq!(report["ok"], true, "{report}");
    assert!(report["checked"].as_u64().unwrap() >= 2, "{report}");
}

#[test]
fn mcp_stdio_round_trip() {
    let (url, h) = in_process_session();
    let actor = scenario_ids().actor;
    let mut mcp = KillOnDrop(
        Command::new(env!("CARGO_BIN_EXE_npcctl"))
            .args(["mcp", "--url", &url])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("mcp starts"),
    );
    let mut stdin = mcp.0.stdin.take().unwrap();
    let mut stdout = BufReader::new(mcp.0.stdout.take().unwrap());
    let mut send = |msg: Value| {
        writeln!(stdin, "{msg}").unwrap();
        stdin.flush().unwrap();
    };
    let mut recv = || {
        let mut line = String::new();
        stdout.read_line(&mut line).unwrap();
        serde_json::from_str::<Value>(&line).unwrap_or_else(|e| panic!("{e}: {line}"))
    };
    send(json!({"jsonrpc": "2.0", "id": 1, "method": "initialize",
                "params": {"protocolVersion": "2025-11-25", "capabilities": {}, "clientInfo": {"name": "test", "version": "1"}}}));
    let init = recv();
    assert_eq!(init["id"], 1);
    assert_eq!(init["result"]["protocolVersion"], "2025-11-25");
    assert!(init["result"]["capabilities"]["tools"].is_object());
    send(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
    send(json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}));
    let list = recv();
    assert_eq!(list["id"], 2, "the notification got no reply");
    let tools: Vec<&str> = list["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        tools,
        [
            "observe",
            "act",
            "act_and_wait",
            "cancel",
            "events",
            "attach",
            "list_units",
            "notes_list",
            "notes_add",
            "notes_update"
        ]
    );
    send(json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call",
                "params": {"name": "observe", "arguments": {"unit": actor}}}));
    let obs = recv();
    assert_eq!(obs["id"], 3);
    assert_eq!(obs["result"]["isError"], false, "{obs}");
    let text = obs["result"]["content"][0]["text"].as_str().unwrap();
    assert!(
        text.starts_with(&format!("You are villager #{actor}")),
        "{text}"
    );
    assert_eq!(obs["result"]["structuredContent"]["format"], "text");
    send(json!({"jsonrpc": "2.0", "id": 4, "method": "tools/call",
                "params": {"name": "act", "arguments": {"unit": actor, "controller": "agent", "verb": "wait"}}}));
    let act = recv();
    assert_eq!(
        act["result"]["structuredContent"]["accepted"], true,
        "{act}"
    );
    // act_and_wait: one tool call acts, waits until idle and observes.
    send(json!({"jsonrpc": "2.0", "id": 6, "method": "tools/call",
                "params": {"name": "act_and_wait", "arguments": {"unit": actor, "controller": "agent",
                           "verb": "wait", "args": {"ticks": 10}, "timeout_ms": 30000}}}));
    let waited = recv();
    assert_eq!(waited["id"], 6);
    assert_eq!(waited["result"]["isError"], false, "{waited}");
    let sc = &waited["result"]["structuredContent"];
    assert_eq!(sc["act"]["accepted"], true, "{waited}");
    assert_eq!(sc["events"]["idle"], true, "{waited}");
    assert_eq!(sc["observation"]["format"], "text", "{waited}");
    let text = waited["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.starts_with("Act accepted: "), "{text}");
    assert!(text.contains("Since your last look: "), "{text}");
    send(json!({"jsonrpc": "2.0", "id": 7, "method": "tools/call",
                "params": {"name": "act_and_wait", "arguments": {"unit": actor, "controller": "intruder",
                           "verb": "wait"}}}));
    let rejected = recv();
    assert_eq!(rejected["result"]["isError"], true, "{rejected}");
    send(json!({"jsonrpc": "2.0", "id": 5, "method": "tools/call",
                "params": {"name": "observe", "arguments": {"unit": 999}}}));
    let err = recv();
    assert_eq!(
        err["result"]["isError"], true,
        "a tool error, not a protocol error: {err}"
    );
    drop(send);
    drop(stdin);
    assert!(mcp.0.wait().unwrap().success(), "exits when stdin closes");
    let c = npc_framework::agent_io::http::RpcClient::new(&url).unwrap();
    c.call("session/stop", json!({})).unwrap();
    h.join().unwrap();
}

const NOTES_DATE: &str = "2026-10-25T12:00:00Z";

fn notes_date() -> String {
    NOTES_DATE.to_owned()
}

/// The testbed's `explorer` scenario document.
fn explorer_doc() -> ScenarioDoc {
    ScenarioDoc::parse(testbed_content::scenario::scenario_text("explorer").unwrap()).unwrap()
}

fn testbed_loaders() -> ScenarioLoaders {
    let mut loaders = ScenarioLoaders::default();
    testbed_content::register_scenario_loader(&mut loaders);
    loaders
}

/// A session of the `explorer` scenario in this process whose actor keeps
/// its notes in `dir` (the test hook: the host inserts `NotesDirs` and
/// `NotesClock` before running); returns the URL, the thread and the actor.
fn explorer_session(dir: PathBuf) -> (String, std::thread::JoinHandle<()>, u32) {
    use npc_framework::agent_io::notes::{NotesClock, NotesDirs};
    let (tx, rx) = mpsc::channel();
    let h = std::thread::spawn(move || {
        let loaders = testbed_loaders();
        let doc = explorer_doc();
        let actor = npc_framework::agent_io::scenario::load(&loaders, &doc, None)
            .expect("explorer loads")
            .actors[0];
        let options = SessionOptions {
            port: 0,
            tick_hz: 50,
            ..SessionOptions::default()
        };
        let mut s = Session::start(loaders, doc, options).expect("session");
        let world = s.app_mut().world_mut();
        world.insert_resource(NotesDirs(std::collections::BTreeMap::from([(actor, dir)])));
        world.insert_resource(NotesClock(notes_date));
        tx.send((s.url(), actor.0)).unwrap();
        s.run().expect("runs");
    });
    let (url, actor) = rx.recv_timeout(Duration::from_secs(120)).unwrap();
    (url, h, actor)
}

#[test]
fn notes_add_and_list_over_a_session() {
    let dir = temp_dir("notes");
    let (url, h, actor) = explorer_session(dir.clone());
    let u = url.as_str();
    let unit = actor.to_string();
    let unit = unit.as_str();
    let agent = |extra: &[&str], want: i32| {
        let mut args = vec!["agent", "notes"];
        args.extend_from_slice(extra);
        args.extend_from_slice(&["--url", u]);
        json_out(&args, want)
    };
    let player = |extra: &[&str], want: i32| {
        let mut args = vec!["notes"];
        args.extend_from_slice(extra);
        args.extend_from_slice(&["--url", u]);
        json_out(&args, want)
    };
    let me = ["--unit", unit, "--controller", "explorer"];
    // The agent writes its diary as the unit's controller.
    let added = agent(
        &[
            &["add"][..],
            &me,
            &[
                "--book",
                "diary",
                "--title",
                "Woke up",
                "--body",
                "At the gate.",
            ],
        ]
        .concat(),
        0,
    );
    assert_eq!(added["accepted"], true, "{added}");
    assert_eq!(added["note"]["id"], 1);
    assert_eq!(added["note"]["created"]["date"], NOTES_DATE);
    let listed = agent(&[&["list"][..], &me, &["--book", "diary"]].concat(), 0);
    assert_eq!(listed["notes"][0]["title"], "Woke up", "{listed}");
    assert!(
        std::fs::read_to_string(dir.join("diary").join("2026-10-25 Woke up.md"))
            .unwrap()
            .contains("At the gate.")
    );
    // Someone else is rejected (exit 1).
    let r = agent(
        &[
            "list",
            "--unit",
            unit,
            "--controller",
            "intruder",
            "--book",
            "diary",
        ],
        1,
    );
    assert_eq!(r["reason"], "not_controller", "{r}");
    // The agent updates with the stamp it got; an old stamp is stale (exit 1).
    let tick = added["note"]["updated"]["tick"].to_string();
    let date = added["note"]["updated"]["date"]
        .as_str()
        .unwrap()
        .to_owned();
    let expected = [
        "--book",
        "diary",
        "--id",
        "1",
        "--expected-tick",
        &tick,
        "--expected-date",
        &date,
    ];
    let updated = agent(
        &[
            &["update"][..],
            &me,
            &expected,
            &["--body", "At the north gate."],
        ]
        .concat(),
        0,
    );
    assert_eq!(updated["note"]["body"], "At the north gate.", "{updated}");
    let stale = agent(
        &[&["update"][..], &me, &expected, &["--title", "Again"]].concat(),
        1,
    );
    assert_eq!(stale["reason"], "stale", "{stale}");

    // The agent proposes; the player (no controller) likes it.
    let p = agent(
        &[
            &["add"][..],
            &me,
            &[
                "--book",
                "proposals",
                "--title",
                "Lamps",
                "--body",
                "Dark at night.",
                "--prompted-by",
                "the night walk",
            ],
        ]
        .concat(),
        0,
    );
    let id = p["note"]["id"].to_string();
    let ptick = p["note"]["updated"]["tick"].to_string();
    let pdate = p["note"]["updated"]["date"].as_str().unwrap().to_owned();
    let liked = player(
        &[
            "update",
            "--unit",
            unit,
            "--book",
            "proposals",
            "--id",
            &id,
            "--expected-tick",
            &ptick,
            "--expected-date",
            &pdate,
            "--status",
            "liked",
        ],
        0,
    );
    assert_eq!(liked["note"]["status"], "liked", "{liked}");
    player(
        &[
            "add",
            "--unit",
            unit,
            "--book",
            "memory",
            "--title",
            "The store",
            "--body",
            "Locked; the key is in the shed.",
        ],
        0,
    );
    let memory = player(&["list", "--unit", unit, "--book", "memory"], 0);
    assert_eq!(memory["notes"][0]["title"], "The store", "{memory}");
    // The agent CLI has no --status.
    let out = npcctl(
        &[
            &["agent", "notes", "update"][..],
            &me,
            &[
                "--book",
                "proposals",
                "--id",
                &id,
                "--expected-tick",
                "0",
                "--status",
                "liked",
                "--url",
                u,
            ],
        ]
        .concat(),
    );
    assert_eq!(out.status.code(), Some(2), "a usage error");

    // MCP: the notes tools take the controller from `--controller`.
    let mut mcp = KillOnDrop(
        Command::new(env!("CARGO_BIN_EXE_npcctl"))
            .args(["mcp", "--url", u, "--controller", "explorer"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("mcp starts"),
    );
    let mut stdin = mcp.0.stdin.take().unwrap();
    let mut stdout = BufReader::new(mcp.0.stdout.take().unwrap());
    let call = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call",
        "params": {"name": "notes_list", "arguments": {"unit": actor, "book": "proposals"}}});
    writeln!(stdin, "{call}").unwrap();
    stdin.flush().unwrap();
    let mut line = String::new();
    stdout.read_line(&mut line).unwrap();
    let r: Value = serde_json::from_str(&line).unwrap_or_else(|e| panic!("{e}: {line}"));
    assert_eq!(r["result"]["isError"], false, "{r}");
    assert_eq!(
        r["result"]["structuredContent"]["notes"][0]["status"], "liked",
        "{r}"
    );
    // A method error keeps its reason for the agent.
    let call = json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call",
        "params": {"name": "notes_add", "arguments": {"unit": actor, "book": "memory",
                   "title": "x".repeat(201), "body": ""}}});
    writeln!(stdin, "{call}").unwrap();
    stdin.flush().unwrap();
    let mut line = String::new();
    stdout.read_line(&mut line).unwrap();
    let r: Value = serde_json::from_str(&line).unwrap_or_else(|e| panic!("{e}: {line}"));
    assert_eq!(r["result"]["isError"], true, "{r}");
    assert_eq!(
        r["result"]["structuredContent"]["error"]["reason"], "too_long",
        "{r}"
    );
    let text = r["result"]["content"][0]["text"].as_str().unwrap();
    assert!(
        text.contains("at most 200") && text.contains("too_long"),
        "{text}"
    );
    drop(stdin);
    assert!(mcp.0.wait().unwrap().success());

    json_out(&["session", "stop", "--url", u], 0);
    h.join().unwrap();
}

#[test]
fn note_writes_leave_the_testbed_hash_unchanged() {
    use npc_framework::agent_io::api;
    use npc_framework::agent_io::notes::{NotesClock, NotesDirs};
    use npc_framework::kernel::hash::state_hash;
    let load = || {
        let loaded =
            npc_framework::agent_io::scenario::load(&testbed_loaders(), &explorer_doc(), None)
                .unwrap();
        (loaded.app, loaded.actors[0])
    };
    let (mut a, actor) = load();
    let (mut b, _) = load();
    npc_framework::kernel::run_ticks(a.world_mut(), 5);
    npc_framework::kernel::run_ticks(b.world_mut(), 5);
    let world = a.world_mut();
    world.insert_resource(NotesDirs(std::collections::BTreeMap::from([(
        actor,
        temp_dir("notes_hash"),
    )])));
    world.insert_resource(NotesClock(notes_date));
    let before = state_hash(world);
    let r = api::call(
        world,
        "agent/notes/add",
        Some(
            json!({"unit": actor.0, "controller": "explorer", "book": "diary",
                    "title": "Woke up", "body": "Here."}),
        ),
    )
    .unwrap();
    assert_eq!(r["accepted"], true, "{r}");
    api::call(
        world,
        "notes/update",
        Some(json!({"unit": actor.0, "book": "diary", "id": 1,
                    "expected": r["note"]["updated"], "title": "Woke up early"})),
    )
    .unwrap();
    assert_eq!(state_hash(world), before);
    npc_framework::kernel::run_ticks(a.world_mut(), 40);
    npc_framework::kernel::run_ticks(b.world_mut(), 40);
    assert_eq!(state_hash(a.world_mut()), state_hash(b.world_mut()));
}

#[test]
fn act_wait_flag() {
    let (url, h) = in_process_session();
    let u = url.as_str();
    let unit = scenario_ids().actor.to_string();
    let unit = unit.as_str();
    let seen = json_out(&["agent", "observe", "--unit", unit, "--url", u], 0)["seq"]
        .as_u64()
        .unwrap()
        .to_string();
    // One call: act, wait until idle, observe.
    let r = json_out(
        &[
            "agent",
            "act",
            "--unit",
            unit,
            "--controller",
            "agent",
            "--verb",
            "walk_to",
            "--pos",
            "2600,1000",
            "--wait",
            "--timeout-ms",
            "60000",
            "--url",
            u,
        ],
        0,
    );
    assert_eq!(r["act"]["accepted"], true, "{r}");
    assert_eq!(r["events"]["idle"], true, "{r}");
    let handle = r["act"]["handle"].clone();
    assert!(
        r["events"]["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["event"]["handle"] == handle && e["event"]["status"] == "done"),
        "{r}"
    );
    assert_eq!(r["observation"]["unit"].to_string(), unit, "{r}");
    // --kinds ends the wait at the first such event; text prints the act
    // line and the observation text.
    let text = run(
        &[
            "agent",
            "act",
            "--unit",
            unit,
            "--controller",
            "agent",
            "--verb",
            "wait",
            "--args",
            r#"{"ticks":40}"#,
            "--wait",
            "--kinds",
            "ExternalAction",
            "--format",
            "text",
            "--url",
            u,
        ],
        0,
    );
    assert!(text.starts_with("Act accepted:"), "{text}");
    assert!(text.contains("Since your last look: "), "{text}");
    // A rejected act with --wait exits 1 at once.
    let r = json_out(
        &[
            "agent",
            "act",
            "--unit",
            unit,
            "--controller",
            "intruder",
            "--verb",
            "wait",
            "--wait",
            "--url",
            u,
        ],
        1,
    );
    assert_eq!(r["act"]["reason"], "not_controller", "{r}");
    // --kinds / --timeout-ms need --wait.
    let out = npcctl(&[
        "agent",
        "act",
        "--unit",
        unit,
        "--controller",
        "agent",
        "--verb",
        "wait",
        "--kinds",
        "ExternalAction",
        "--url",
        u,
    ]);
    assert_eq!(out.status.code(), Some(2), "--kinds without --wait");
    // observe --since-seq: what changed since the first look.
    let obs = json_out(
        &[
            "agent",
            "observe",
            "--unit",
            unit,
            "--since-seq",
            &seen,
            "--url",
            u,
        ],
        0,
    );
    assert_eq!(obs["changes"]["truncated"], false, "{obs}");
    assert!(
        obs["changes"]["actions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["verb"] == "walk_to" && a["status"] == "done"),
        "{}",
        obs["changes"]
    );
    json_out(&["session", "stop", "--url", u], 0);
    h.join().unwrap();
}
