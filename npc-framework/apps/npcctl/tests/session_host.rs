//! The session host's transport rules (`npc_agent_io::session` / `http`):
//! long `npc/step`s run in slices so other requests are served meanwhile,
//! `world/load_scenario` never reads a client-named file, and
//! `serve --token` requires the bearer token from every client.

use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use npc_framework::agent_io::http::{ClientError, RpcClient};
use npc_framework::agent_io::scenario::{ScenarioDoc, ScenarioLoaders};
use npc_framework::agent_io::session::{STEP_SLICE, Session, SessionOptions};
use serde_json::{Value, json};

fn scenario_file() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../content/village_content/scenarios/agent_village.json")
}

/// A session in this process on a free port; returns a client and the
/// session thread (ended by `session/stop`).
fn session() -> (RpcClient, std::thread::JoinHandle<()>) {
    let (tx, rx) = mpsc::channel();
    let h = std::thread::spawn(move || {
        let mut loaders = ScenarioLoaders::default();
        village_content::agent::register_scenario_loader(&mut loaders);
        let text = std::fs::read_to_string(scenario_file()).unwrap();
        let doc = ScenarioDoc::parse(&text).unwrap();
        let options = SessionOptions {
            port: 0,
            tick_hz: 20,
            ..SessionOptions::default()
        };
        let s = Session::start(loaders, doc, options).expect("session");
        tx.send(s.url()).unwrap();
        s.run().expect("runs");
    });
    let url: String = rx.recv_timeout(Duration::from_secs(60)).unwrap();
    (RpcClient::new(&url).unwrap(), h)
}

fn tick(c: &RpcClient) -> u64 {
    c.call("session/info", json!({})).expect("info")["tick"]
        .as_u64()
        .expect("tick")
}

#[test]
fn a_long_step_does_not_block_other_requests() {
    let (c, h) = session();
    c.call("npc/pause", json!({})).expect("pause");
    let start = tick(&c);
    let ticks = 60 * STEP_SLICE;
    let url = c.url();
    let stepper = std::thread::spawn(move || {
        let c = RpcClient::new(&url).unwrap();
        c.call("npc/step", json!({"ticks": ticks})).expect("step")
    });
    // Answered while the step is still running: the loop services requests
    // between slices.
    // Poll until the step is done (a loaded machine may take a while to
    // even deliver the step request).
    let mut mid = None;
    let deadline = std::time::Instant::now() + Duration::from_secs(120);
    while !stepper.is_finished() && std::time::Instant::now() < deadline {
        let t = tick(&c);
        if t > start && t < start + ticks {
            mid = Some(t);
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    let done = stepper.join().expect("stepper");
    assert_eq!(done["ticks"], ticks, "{done}");
    assert_eq!(done["tick"], start + ticks, "{done}");
    assert!(
        mid.is_some(),
        "session/info was never answered in the middle of a {ticks}-tick step"
    );
    assert_eq!(tick(&c), start + ticks, "paused: no frames ran meanwhile");
    // Short steps and bad params keep their old behaviour.
    let one = c.call("npc/step", json!({})).expect("one");
    assert_eq!(one["ticks"], 1);
    let err = c
        .call("npc/step", json!({"ticks": 2_000_000}))
        .expect_err("too many");
    assert!(matches!(err, ClientError::Rpc(_)), "{err}");
    let err = c
        .call("npc/step", json!({"tick": 5}))
        .expect_err("unknown field");
    assert!(matches!(err, ClientError::Rpc(_)), "{err}");
    c.call("session/stop", json!({})).expect("stop");
    h.join().expect("session thread");
}

#[test]
fn load_scenario_takes_the_document_inline_only() {
    let (c, h) = session();
    let path = scenario_file().display().to_string();
    let err = c
        .call("world/load_scenario", json!({"path": path}))
        .expect_err("no path");
    let ClientError::Rpc(e) = err else {
        panic!("{err}")
    };
    assert!(e.message.contains("path"), "{}", e.message);
    let doc: Value =
        serde_json::from_str(&std::fs::read_to_string(scenario_file()).unwrap()).expect("json");
    let info = c
        .call("world/load_scenario", json!({"scenario": doc, "seed": 3}))
        .expect("inline");
    assert_eq!(info["seed"], 3, "{info}");
    c.call("session/stop", json!({})).expect("stop");
    h.join().expect("session thread");
}

/// A spawned `npcctl serve`, killed when dropped (a failing test must not
/// leak a server that keeps the target exe locked).
struct KillOnDrop(std::process::Child);

impl Drop for KillOnDrop {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn serve_with_a_token_requires_it() {
    let mut child = KillOnDrop(
        Command::new(env!("CARGO_BIN_EXE_npcctl"))
            .args([
                "serve",
                "--scenario",
                scenario_file().to_str().unwrap(),
                "--port",
                "0",
                "--token",
                "t0ken",
            ])
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
    assert_eq!(ready["token_required"], true, "{ready}");
    let url = ready["listening"].as_str().unwrap().to_owned();
    let npcctl = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_npcctl"))
            .args(args)
            .output()
            .expect("npcctl runs")
    };
    let out = npcctl(&["session", "info", "--url", &url]);
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("Bearer"), "{stderr}");
    let out = npcctl(&["session", "info", "--url", &url, "--token", "t0ken"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let out = npcctl(&["session", "stop", "--url", &url, "--token", "t0ken"]);
    assert_eq!(out.status.code(), Some(0));
    let status = child.0.wait().expect("serve exits");
    assert!(status.success());
}

/// `Session::start_app` (the detached explorer's host): `on_frame` runs
/// between frames, `on_stop` can refuse a `session/stop` (the world keeps
/// running) and runs once more when the stop flag (Ctrl+C) ends the loop;
/// `world/reset` without a rebuild hook and `world/load_scenario` are
/// refused.
#[test]
fn start_app_runs_its_hooks_and_stops_on_the_flag() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    let frames = Arc::new(AtomicUsize::new(0));
    let stops = Arc::new(AtomicUsize::new(0));
    let flag = Arc::new(AtomicBool::new(false));
    let (tx, rx) = mpsc::channel();
    let h = {
        let (frames, stops, flag) = (frames.clone(), stops.clone(), flag.clone());
        std::thread::spawn(move || {
            let mut loaders = ScenarioLoaders::default();
            village_content::agent::register_scenario_loader(&mut loaders);
            let text = std::fs::read_to_string(scenario_file()).unwrap();
            let doc = ScenarioDoc::parse(&text).unwrap();
            let loaded =
                npc_framework::agent_io::scenario::load(&loaders, &doc, Some(4)).expect("loads");
            let options = SessionOptions {
                port: 0,
                tick_hz: 50,
                seed: Some(4),
                on_frame: Some(Box::new(move |_world| {
                    frames.fetch_add(1, Ordering::SeqCst);
                })),
                // The first stop is refused, every later one accepted.
                on_stop: Some(Box::new(move |world| {
                    let tick = world.resource::<npc_framework::prelude::SimClock>().tick;
                    match stops.fetch_add(1, Ordering::SeqCst) {
                        0 => Err("the save failed".to_owned()),
                        _ => Ok(json!({"tick": tick})),
                    }
                })),
                stop: Some(flag),
                ..SessionOptions::default()
            };
            let s = Session::start_app(loaded.app, options).expect("session");
            tx.send(s.url()).unwrap();
            s.run()
        })
    };
    let url: String = rx.recv_timeout(Duration::from_secs(60)).unwrap();
    let c = RpcClient::new(&url).unwrap();
    let info = c.call("session/info", json!({})).expect("info");
    assert_eq!(info["seed"], 4, "{info}");
    let deadline = std::time::Instant::now() + Duration::from_secs(60);
    while frames.load(Ordering::SeqCst) < 3 {
        assert!(std::time::Instant::now() < deadline, "on_frame never ran");
        std::thread::sleep(Duration::from_millis(10));
    }
    let err = c.call("session/stop", json!({})).expect_err("refused");
    let ClientError::Rpc(e) = err else {
        panic!("{err}")
    };
    assert!(e.message.contains("the save failed"), "{}", e.message);
    // Still running.
    tick(&c);
    let err = c
        .call("world/reset", json!({}))
        .expect_err("no rebuild hook");
    assert!(matches!(err, ClientError::Rpc(_)), "{err}");
    let err = c
        .call("world/load_scenario", json!({"scenario": {}}))
        .expect_err("no scenario source");
    assert!(matches!(err, ClientError::Rpc(_)), "{err}");
    // The flag (what Ctrl+C sets) ends the loop through on_stop.
    flag.store(true, Ordering::SeqCst);
    let ended = h.join().expect("session thread");
    assert!(ended.is_ok(), "{ended:?}");
    assert_eq!(stops.load(Ordering::SeqCst), 2);
}

/// One JSON-RPC call on a raw connection: signals `sent` once the request
/// is written, then returns the response object (`result` or `error`).
fn raw_call(port: u16, method: &str, params: Value, sent: &mpsc::Sender<()>) -> Value {
    use std::io::{Read, Write};
    let body = json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}).to_string();
    let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).expect("connects");
    s.set_read_timeout(Some(Duration::from_secs(120))).unwrap();
    write!(
        s,
        "POST / HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .unwrap();
    s.flush().unwrap();
    sent.send(()).unwrap();
    let mut text = String::new();
    s.read_to_string(&mut text).expect("an answer");
    let (_, body) = text
        .split_once("\r\n\r\n")
        .unwrap_or_else(|| panic!("{text}"));
    serde_json::from_str(body).unwrap_or_else(|e| panic!("{e}: {text}"))
}

/// Fix round 1: once `session/stop` is accepted (its final save taken),
/// calls queued behind it are refused, so the save is the world's last
/// state; the hook's extra keys never replace `stopped` / `record`.
#[test]
fn calls_queued_behind_a_stop_are_refused() {
    use npc_framework::prelude::SimClock;
    use testbed_content::explorer::host::{HostArgs, build_world, save_now};
    let home = std::env::temp_dir().join(format!("npcctl-stop-queue-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).unwrap();
    let args = HostArgs {
        home: home.clone(),
        agent: "Wren".into(),
        world: Some("Moor".into()),
        resume: false,
        on_mismatch: None,
        preset: None,
        actors_dir: Some(home.join("actors")),
    };
    let (entered_tx, entered_rx) = mpsc::channel::<()>();
    let (gate_tx, gate_rx) = mpsc::channel::<()>();
    let (url_tx, url_rx) = mpsc::channel();
    let session = std::thread::spawn(move || {
        // Paused (as `build_world` leaves it) past the tick-0 New save:
        // only a call could move the tick.
        let (mut app, _, dir) = build_world(&args, 3, "2026-10-25T12:00:00Z", |_| {}).unwrap();
        npc_framework::prelude::run_ticks(app.world_mut(), 25);
        let unit = app
            .world()
            .resource::<testbed_content::TestbedIds>()
            .get("Wren")
            .unwrap()
            .0;
        let save = dir.save("Moor");
        let options = SessionOptions {
            port: 0,
            tick_hz: 20,
            // Holds the loop inside the stop until the test queued its calls.
            on_stop: Some(Box::new(move |world| {
                entered_tx.send(()).unwrap();
                gate_rx.recv_timeout(Duration::from_secs(120)).unwrap();
                let tick = world.resource::<SimClock>().tick;
                save_now(world, &dir, "2026-10-25T12:00:01Z")?;
                Ok(json!({"tick": tick, "stopped": "overridden?", "record": "overridden?"}))
            })),
            ..SessionOptions::default()
        };
        let s = Session::start_app(app, options).expect("session");
        url_tx.send((s.port(), unit, save)).unwrap();
        s.run()
    });
    let (port, unit, save): (u16, u32, PathBuf) =
        url_rx.recv_timeout(Duration::from_secs(120)).unwrap();
    let (sent_tx, sent_rx) = mpsc::channel::<()>();
    let call = |method: &'static str, params: Value| {
        let sent = sent_tx.clone();
        std::thread::spawn(move || raw_call(port, method, params, &sent))
    };
    let stop = call("session/stop", json!({}));
    sent_rx.recv_timeout(Duration::from_secs(60)).unwrap();
    entered_rx.recv_timeout(Duration::from_secs(60)).unwrap();
    let step = call("npc/step", json!({"ticks": 10}));
    let act = call(
        "agent/act",
        json!({"unit": unit, "controller": "explorer", "steps": [
            {"verb": "order", "args": {"order": {"type": "Move", "pos": [2000, 0]}}},
            {"verb": "execute_orders"}
        ]}),
    );
    let resume = call("npc/resume", json!({}));
    // Every request is written while the loop waits in the stop; a short
    // grace lets the server queue them.
    for _ in 0..3 {
        sent_rx.recv_timeout(Duration::from_secs(60)).unwrap();
    }
    std::thread::sleep(Duration::from_millis(300));
    gate_tx.send(()).unwrap();
    let stopped = stop.join().unwrap()["result"].clone();
    assert_eq!(stopped["stopped"], true, "{stopped}");
    assert_eq!(stopped["record"], Value::Null, "{stopped}");
    assert_eq!(stopped["tick"], 25, "{stopped}");
    for (name, answer) in [("step", step), ("act", act), ("resume", resume)] {
        let answer = answer.join().unwrap();
        // A call that wrongly ran answers a `result` (a step: tick 35).
        let message = answer["error"]["message"].as_str().unwrap_or_default();
        assert!(
            message.contains("the session is stopping"),
            "{name}: {answer}"
        );
    }
    session.join().unwrap().expect("runs");
    let snap = npc_framework::snapshot::read(&save).unwrap();
    assert_eq!(
        snap.tick,
        stopped["tick"].as_u64().unwrap(),
        "the save is the last state"
    );
    assert_eq!(snap.tick, 25, "saved on stop, not the tick-0 New save");
    let _ = std::fs::remove_dir_all(&home);
}

/// Fix round 1: an app finished without a driver is refused with a message
/// (it could never tick), not hosted.
#[test]
fn start_app_refuses_a_finished_app_without_a_driver() {
    let mut loaders = ScenarioLoaders::default();
    village_content::agent::register_scenario_loader(&mut loaders);
    let text = std::fs::read_to_string(scenario_file()).unwrap();
    let doc = ScenarioDoc::parse(&text).unwrap();
    let mut app = npc_framework::agent_io::scenario::load(&loaders, &doc, Some(1))
        .expect("loads")
        .app;
    app.finish();
    app.cleanup();
    let options = SessionOptions {
        port: 0,
        ..SessionOptions::default()
    };
    let Err(e) = Session::start_app(app, options) else {
        panic!("hosted an app that cannot tick");
    };
    assert!(e.contains("no driver"), "{e}");
}
