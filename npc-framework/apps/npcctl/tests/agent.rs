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
        "{:?}\nstdout: {}\nstderr: {}",
        safe_args(args),
        safe_text(&stdout, args),
        safe_text(&String::from_utf8_lossy(&out.stderr), args)
    );
    stdout
}

fn json_out(args: &[&str], want: i32) -> Value {
    let s = run(args, want);
    serde_json::from_str(&s).unwrap_or_else(|e| {
        panic!(
            "{:?}: not JSON ({e}): {}",
            safe_args(args),
            safe_text(&s, args)
        )
    })
}

fn safe_args<'a>(args: &'a [&'a str]) -> Vec<&'a str> {
    let mut secret = false;
    args.iter()
        .map(|arg| {
            if secret {
                secret = false;
                return "[redacted]";
            }
            secret = matches!(*arg, "--token" | "--bearer-token");
            *arg
        })
        .collect()
}

fn safe_text(text: &str, args: &[&str]) -> String {
    args.windows(2)
        .filter(|pair| matches!(pair[0], "--token" | "--bearer-token") && !pair[1].is_empty())
        .fold(text.to_owned(), |text, pair| {
            text.replace(pair[1], "[redacted]")
        })
}

fn tool_call(
    input: &mut impl Write,
    output: &mut impl BufRead,
    id: u64,
    name: &str,
    args: Value,
) -> Value {
    let request = json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":name,"arguments":args}});
    writeln!(input, "{request}").unwrap();
    input.flush().unwrap();
    let mut line = String::new();
    output.read_line(&mut line).unwrap();
    serde_json::from_str(&line).expect("MCP JSON response")
}

#[test]
fn registered_cli_and_mcp_lifecycle_and_token_rotation() {
    use npc_framework::agent_io::http::RpcClient;
    let base = temp_dir("registered");
    let agents = base.join("agents");
    let actors = base.join("actors");
    let mut host = KillOnDrop(
        Command::new(env!("CARGO_BIN_EXE_npcctl"))
            .args([
                "serve",
                "--explorer-home",
                agents.to_str().unwrap(),
                "--actors-dir",
                actors.to_str().unwrap(),
                "--agent",
                "Wren",
                "--world",
                "Field",
                "--port",
                "0",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap(),
    );
    let stdout = host.0.stdout.take().unwrap();
    let (tx, rx) = mpsc::channel();
    let reader = std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            if let Ok(line) = line {
                let _ = tx.send(line);
            } else {
                break;
            }
        }
    });
    let ready: Value = serde_json::from_str(
        &rx.recv_timeout(Duration::from_secs(180))
            .expect("host ready"),
    )
    .unwrap();
    let url = ready["listening"].as_str().unwrap();
    let raw = RpcClient::new(url).unwrap();
    let meta: Value =
        serde_json::from_str(&std::fs::read_to_string(agents.join("wren/agent.json")).unwrap())
            .unwrap();
    let agent_id = meta["agent_id"].as_str().unwrap();
    let token = meta["token"].as_str().unwrap();
    let failure = raw
        .call("agent/observe", json!({}))
        .expect_err("anonymous actor calls are locked");
    let npc_framework::agent_io::http::ClientError::Rpc(failure) = failure else {
        panic!("wrong error type")
    };
    assert_eq!(failure.data.unwrap()["reason"], "not_registered");
    let observed = json_out(
        &[
            "agent",
            "observe",
            "--agent-id",
            agent_id,
            "--token",
            token,
            "--url",
            url,
        ],
        0,
    );
    assert!(
        observed["unit"].is_u64(),
        "the developer CLI resolves its own actor"
    );
    let mut mcp = KillOnDrop(
        Command::new(env!("CARGO_BIN_EXE_npcctl"))
            .args(["mcp", "--registered", "--url", url])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap(),
    );
    let mut input = mcp.0.stdin.take().unwrap();
    let mut output = BufReader::new(mcp.0.stdout.take().unwrap());
    let registered = tool_call(
        &mut input,
        &mut output,
        1,
        "register",
        json!({"agent_id":agent_id,"token":token}),
    );
    assert_eq!(registered["result"]["isError"], false);
    assert!(
        registered["result"]["structuredContent"]
            .get("session")
            .is_none()
    );
    assert!(!registered.to_string().contains(token));
    let entered = tool_call(&mut input, &mut output, 2, "enter_world", json!({}));
    assert_eq!(entered["result"]["isError"], false);
    assert!(entered["result"]["structuredContent"]["character"].is_object());
    assert!(entered["result"]["structuredContent"].get("unit").is_none());
    let marked = tool_call(
        &mut input,
        &mut output,
        40,
        "map_add",
        json!({"name":"Arrival","place":"here","kind":"camp","note":"My first landmark."}),
    );
    assert_eq!(marked["result"]["isError"], false, "{marked}");
    assert_eq!(marked["result"]["structuredContent"]["accepted"], true);
    assert!(marked["result"]["structuredContent"].get("unit").is_none());
    let landmark = &marked["result"]["structuredContent"]["landmark"];
    let changed = tool_call(
        &mut input,
        &mut output,
        41,
        "map_update",
        json!({"id":landmark["id"],"expected":landmark["updated"],"kind":null,"note":"Kept before taking control."}),
    );
    assert_eq!(changed["result"]["isError"], false, "{changed}");
    assert!(changed["result"]["structuredContent"]["landmark"]["kind"].is_null());
    let mapped = tool_call(&mut input, &mut output, 42, "map_list", json!({}));
    assert_eq!(mapped["result"]["isError"], false, "{mapped}");
    assert_eq!(
        mapped["result"]["structuredContent"]["landmarks"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let forbidden = tool_call(&mut input, &mut output, 3, "observe", json!({}));
    assert_eq!(
        forbidden["result"]["structuredContent"]["error"]["reason"],
        "not_in_control"
    );
    let taken = tool_call(&mut input, &mut output, 4, "take_control", json!({}));
    assert_eq!(
        taken["result"]["structuredContent"]["in_control"], true,
        "{taken}"
    );
    let acted = tool_call(
        &mut input,
        &mut output,
        5,
        "act_and_wait",
        json!({"verb":"wait","args":{"ticks":2},"timeout_ms":10000}),
    );
    assert_eq!(acted["result"]["isError"], false, "{acted}");
    assert_eq!(
        acted["result"]["structuredContent"]["act"]["accepted"],
        true
    );
    let note = tool_call(
        &mut input,
        &mut output,
        6,
        "notes_add",
        json!({"book":"memory","title":"Registration","body":"Use the world's tools."}),
    );
    assert_eq!(note["result"]["structuredContent"]["accepted"], true);
    let rotated = json_out(
        &[
            "agent-token",
            "rotate",
            "--explorer-home",
            agents.to_str().unwrap(),
            "--agent",
            "Wren",
        ],
        0,
    );
    assert_eq!(rotated, json!({"rotated":true,"agent_id":agent_id}));
    let new_meta: Value =
        serde_json::from_str(&std::fs::read_to_string(agents.join("wren/agent.json")).unwrap())
            .unwrap();
    let new_token = new_meta["token"].as_str().unwrap();
    assert!(token != new_token, "rotation changes the secret");
    let denied = tool_call(&mut input, &mut output, 7, "observe", json!({}));
    assert_eq!(
        denied["result"]["structuredContent"]["error"]["data"]["revoked"],
        true
    );
    assert!(!denied.to_string().contains(token) && !denied.to_string().contains(new_token));
    let still_denied = tool_call(&mut input, &mut output, 8, "observe", json!({}));
    assert_eq!(
        still_denied["result"]["structuredContent"]["error"]["reason"],
        "not_registered"
    );
    let refreshed = tool_call(
        &mut input,
        &mut output,
        9,
        "register",
        json!({"agent_id":agent_id,"token":new_token}),
    );
    assert_eq!(refreshed["result"]["isError"], false);
    let notes = tool_call(
        &mut input,
        &mut output,
        10,
        "notes_list",
        json!({"book":"memory"}),
    );
    assert_eq!(
        notes["result"]["structuredContent"]["notes"][0]["title"],
        "Registration"
    );
    drop(input);
    assert!(mcp.0.wait().unwrap().success());
    raw.call("session/stop", json!({})).unwrap();
    assert!(host.0.wait().unwrap().success());
    reader.join().unwrap();
    let saved = std::fs::read_to_string(agents.join("wren/field.world.json")).unwrap();
    assert!(
        !saved.contains(token) && !saved.contains(new_token),
        "credentials are never in a world snapshot"
    );
}

#[test]
fn registration_credentials_and_http_bearer_are_separate() {
    use npc_framework::agent_io::http::{HttpServer, ServerOptions};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    let bearer = "transport-bearer-only";
    let credential = "registration-credential-only";
    let server = HttpServer::bind_with(
        0,
        ServerOptions {
            token: Some(bearer.into()),
        },
    )
    .unwrap();
    let url = server.url();
    let stop = Arc::new(AtomicBool::new(false));
    let done = Arc::clone(&stop);
    let host = std::thread::spawn(move || {
        while !done.load(Ordering::Relaxed) {
            while let Some(call) = server.try_recv() {
                let answer = match call.method.as_str() {
                    "agent/register" => {
                        assert!(
                            call.params.as_ref().unwrap()["token"] == credential,
                            "wrong registration credential"
                        );
                        json!({"session":"private-session-key-for-this-test","agent_id":"wren","world":"Field"})
                    }
                    "agent/take_control" => {
                        json!({"accepted":true,"in_control":true,"unit":42,"controller":"wren"})
                    }
                    "agent/observe" => json!({"unit":42,"text":"You are here."}),
                    "session/info" => json!({"token_required":true}),
                    _ => panic!("unexpected test method"),
                };
                call.reply(Ok(answer));
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    });
    let observed = json_out(
        &[
            "agent",
            "observe",
            "--url",
            &url,
            "--agent-id",
            "wren",
            "--token",
            credential,
            "--bearer-token",
            bearer,
        ],
        0,
    );
    assert_eq!(observed["unit"], 42);
    let info = json_out(&["session", "info", "--url", &url, "--token", bearer], 0);
    assert_eq!(
        info["token_required"], true,
        "legacy token still authenticates HTTP"
    );
    stop.store(true, Ordering::Relaxed);
    host.join().unwrap();
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
            "notes_update",
            "map_list",
            "map_add",
            "map_update"
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
fn map_add_list_update_over_cli_preserves_identity_and_rejects_stale() {
    use npc_framework::agent_io::http::RpcClient;
    use npc_framework::agent_io::map::Landmark;
    struct SessionGuard {
        url: String,
        thread: Option<std::thread::JoinHandle<()>>,
    }
    impl Drop for SessionGuard {
        fn drop(&mut self) {
            let _ = RpcClient::new(&self.url)
                .unwrap()
                .call("session/stop", json!({}));
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
        }
    }
    let dir = temp_dir("map_cli");
    let (url, thread, actor) = explorer_session(dir.clone());
    let guard = SessionGuard {
        url,
        thread: Some(thread),
    };
    let unit = actor.to_string();
    let agent = |args: &[&str], expected| {
        json_out(
            &[&["agent", "map"][..], args, &["--url", &guard.url]].concat(),
            expected,
        )
    };
    let player = |args: &[&str], expected| {
        json_out(
            &[&["map"][..], args, &["--as", "player", "--url", &guard.url]].concat(),
            expected,
        )
    };
    let identity = ["--unit", unit.as_str(), "--controller", "explorer"];
    let added = agent(
        &[
            &["add"][..],
            &identity,
            &[
                "--name",
                "Arrival",
                "--here",
                "--kind",
                "camp",
                "--note",
                "At the compound.",
            ],
        ]
        .concat(),
        0,
    );
    assert_eq!(added["accepted"], true, "{added}");
    let first: Landmark = serde_json::from_value(added["landmark"].clone()).unwrap();
    assert_eq!(first.id, 1);
    assert_eq!(first.kind.as_deref(), Some("camp"));
    let listed = player(&["list", "--unit", &unit], 0);
    assert_eq!(listed["landmarks"][0]["id"], first.id);
    let tick = first.updated.tick.to_string();
    let id = first.id.to_string();
    let changed = player(
        &[
            "update",
            "--unit",
            &unit,
            "--id",
            &id,
            "--expected-tick",
            &tick,
            "--expected-date",
            &first.updated.date,
            "--name",
            "Home marker",
            "--pos",
            "-100,200",
            "--clear-kind",
            "--note",
            "Return here.",
        ],
        0,
    );
    assert_eq!(changed["accepted"], true, "{changed}");
    let current: Landmark = serde_json::from_value(changed["landmark"].clone()).unwrap();
    assert_eq!(current.id, first.id);
    assert_eq!(current.pos, [-100, 200]);
    assert_eq!(current.thing, first.thing);
    assert_eq!(current.kind, None);
    assert_eq!(current.note, "Return here.");
    let stale = agent(
        &[
            &["update"][..],
            &identity,
            &[
                "--id",
                &id,
                "--expected-tick",
                &tick,
                "--expected-date",
                &first.updated.date,
                "--note",
                "Lost edit",
            ],
        ]
        .concat(),
        1,
    );
    assert_eq!(stale["reason"], "stale", "{stale}");
    let final_map = agent(&[&["list"][..], &identity].concat(), 0);
    assert_eq!(final_map["landmarks"][0]["note"], "Return here.");
    assert!(dir.join("map").join("Home marker.md").is_file());
    drop(guard);
    let _ = std::fs::remove_dir_all(dir);
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
