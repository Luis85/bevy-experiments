//! The remote debug endpoint against the real threaded runner (spec §15 C verification):
//! `fl.step`, `fl.sim_state`, `fl.issue_command` and `fl.entity` over HTTP on a headless game,
//! and `fl.screenshot` on the full game with a hidden window (GPU).
#![cfg(all(feature = "debug_remote", debug_assertions))]

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::time::Duration;

use bevy::asset::RenderAssetUsages;
use bevy::image::{CompressedImageFormats, ImageSampler, ImageType};
use bevy::prelude::*;
use fl_game::GameBuilder;
use serde_json::{Value, json};

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .and_then(|l| l.local_addr())
        .expect("a free port")
        .port()
}

/// The body of an HTTP response, de-chunked.
fn http_body(raw: &[u8]) -> Vec<u8> {
    let split = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .expect("an HTTP response");
    let head = String::from_utf8_lossy(&raw[..split]).to_ascii_lowercase();
    let mut body = &raw[split + 4..];
    if !head.contains("transfer-encoding: chunked") {
        return body.to_vec();
    }
    let mut out = Vec::new();
    loop {
        let line_end = body
            .windows(2)
            .position(|w| w == b"\r\n")
            .expect("a chunk size line");
        let size = usize::from_str_radix(String::from_utf8_lossy(&body[..line_end]).trim(), 16)
            .expect("a hex chunk size");
        if size == 0 {
            return out;
        }
        let data = &body[line_end + 2..];
        out.extend_from_slice(&data[..size]);
        body = &data[size + 2..];
    }
}

/// One JSON-RPC call; returns `result`, panicking on a JSON-RPC error.
fn rpc(port: u16, method: &str, params: Value) -> Value {
    let body = json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}).to_string();
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connects");
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .expect("timeout");
    write!(
        stream,
        "POST / HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .expect("sends");
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).expect("reads");
    let reply: Value = serde_json::from_slice(&http_body(&raw)).expect("a JSON-RPC response");
    assert!(reply.get("error").is_none(), "{method}: {reply}");
    reply["result"].clone()
}

fn wait_for_server(port: u16) {
    for _ in 0..2400 {
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    panic!("the BRP server never started on port {port}");
}

/// The `core.pose` component of `fl.entity {sim_id}`.
fn pose(port: u16, sim_id: u32) -> Value {
    let entity = rpc(port, "fl.entity", json!({ "sim_id": sim_id }));
    let pose = entity["components"]["core.pose"].clone();
    assert!(!pose.is_null(), "unit {sim_id} has no pose: {entity}");
    pose
}

/// Runs `calls` against a headless testbed serving BRP while updating it on this thread.
fn with_headless_game(calls: impl FnOnce(u16) + Send + 'static) {
    let port = free_port();
    let mut app = GameBuilder::new()
        .debug_remote(Some(port))
        .prepare_headless()
        .expect("the testbed prepares");
    let client = std::thread::spawn(move || {
        wait_for_server(port);
        calls(port);
    });
    for _ in 0..12_000 {
        if client.is_finished() {
            break;
        }
        app.update();
        std::thread::sleep(Duration::from_millis(5));
    }
    client.join().expect("the client calls succeed");
}

#[test]
fn brp_step_state_command() {
    with_headless_game(|port| {
        let stepped = rpc(port, "fl.step", json!({"n": 5}));
        assert!(stepped["tick"].as_u64().expect("a tick") >= 5, "{stepped}");
        let state = rpc(port, "fl.sim_state", Value::Null);
        assert!(state["tick"].as_u64().expect("a tick") >= 5, "{state}");
        assert_eq!(state["paused"], true, "{state}");
        let before = pose(port, 1);
        let issued = rpc(
            port,
            "fl.issue_command",
            json!({"player": 0, "units": [1], "command": "core.move",
                   "payload": {"target": [200, 100]}}),
        );
        assert!(issued["exec_tick"].as_u64().is_some(), "{issued}");
        rpc(port, "fl.step", json!({"n": 40}));
        let after = pose(port, 1);
        assert_ne!(before, after, "unit 1 did not move");
    });
}

/// `fl.issue_command {player 0, units, command, payload}`; returns its `exec_tick`.
fn issue(port: u16, units: &[u32], command: &str, payload: Value) -> u64 {
    let issued = rpc(
        port,
        "fl.issue_command",
        json!({"player": 0, "units": units, "command": command, "payload": payload}),
    );
    issued["exec_tick"].as_u64().expect("an exec tick")
}

/// Steps the paused game 20 ticks at a time, at most 40 times, until `done` holds.
fn step_until(port: u16, what: &str, mut done: impl FnMut() -> bool) {
    for _ in 0..40 {
        if done() {
            return;
        }
        rpc(port, "fl.step", json!({"n": 20}));
    }
    assert!(done(), "{what} within 800 ticks");
}

/// The heading of `pose`, degrees.
fn heading(pose: &Value) -> f64 {
    pose["heading"]
        .as_f64()
        .unwrap_or_else(|| panic!("a heading in {pose}"))
}

/// The `y` of `pose`, metres.
fn north(pose: &Value) -> f64 {
    pose["pos"][1]
        .as_f64()
        .unwrap_or_else(|| panic!("a position in {pose}"))
}

/// The M1 payloads' JSON field names, decoded by `fl.issue_command` and applied by the sim
/// (Task 21 review Minor 5): `core.rotate {toward}`, `core.follow {leader}` and
/// `debug.set_terrain {min_cell, max_cell, terrain}`.
#[test]
fn brp_issue_command_m1_payloads_apply() {
    with_headless_game(|port| {
        rpc(port, "fl.step", json!({"n": 2}));
        // The tiger #3 at (410, 350) faces north (90°); it turns east toward (510, 350).
        assert_eq!(heading(&pose(port, 3)), 90.0);
        issue(port, &[3], "core.rotate", json!({"toward": [510.0, 350.0]}));
        step_until(port, "#3 faces east", || {
            let east = heading(&pose(port, 3)).rem_euclid(360.0);
            east.min(360.0 - east) < 1.0
        });

        // The truck #4 follows #3 while #3 drives 150 m north.
        issue(port, &[4], "core.follow", json!({"leader": 3}));
        issue(port, &[3], "core.move", json!({"target": [410.0, 500.0]}));
        rpc(port, "fl.step", json!({"n": 2}));
        let entity = rpc(port, "fl.entity", json!({"sim_id": 4}));
        let orders = entity["components"]["core.move_order"]["orders"].to_string();
        assert!(orders.contains("Follow"), "#4 follows: {entity}");
        step_until(port, "#4 follows #3 north", || {
            north(&pose(port, 4)) > 400.0
        });

        // The west bridge floods.
        let terrain = |port| {
            rpc(
                port,
                "fl.debug_query",
                json!({"name": "core.terrain_at", "args": {"cell": [71, 124]}}),
            )["terrain"]
                .clone()
        };
        assert_ne!(terrain(port), "base.water");
        issue(
            port,
            &[],
            "debug.set_terrain",
            json!({"min_cell": [70, 122], "max_cell": [72, 127], "terrain": "base.water"}),
        );
        step_until(port, "the bridge cells are water", || {
            terrain(port) == "base.water"
        });
    });
}

#[test]
fn brp_save_load_and_scenarios() {
    let dir = tempfile::tempdir().expect("temp dir");
    let save = dir.path().join("match.save");
    let scenario = dir.path().join("snap.scenario.ron");
    let (save_path, scenario_path) = (save.clone(), scenario.clone());
    with_headless_game(move |port| {
        let saved_at = rpc(port, "fl.step", json!({"n": 10}))["tick"].clone();
        let state = rpc(port, "fl.sim_state", Value::Null);
        let written = rpc(port, "fl.save", json!({ "path": save_path }));
        assert!(written["bytes"].as_u64().expect("a size") > 0, "{written}");
        rpc(port, "fl.step", json!({"n": 5}));

        let loaded = rpc(port, "fl.load", json!({ "path": save_path }));
        assert_eq!(loaded["tick"], saved_at, "{loaded}");
        let reloaded = rpc(port, "fl.sim_state", Value::Null);
        assert_eq!(reloaded["state_hash"], state["state_hash"], "{reloaded}");
        let next = saved_at.as_u64().expect("a tick") + 1;
        assert_eq!(rpc(port, "fl.step", json!({"n": 1}))["tick"], next);

        // A loaded save has no scenario to base a scenario file on; a loaded scenario has.
        let restarted = rpc(port, "fl.load_scenario", json!({"id": "base.testbed"}));
        assert_eq!(restarted["tick"], 0, "{restarted}");
        rpc(port, "fl.step", json!({"n": 2}));
        let exported = rpc(port, "fl.save_scenario", json!({ "path": scenario_path }));
        assert_eq!(exported["id"], "snap", "{exported}");
        assert_eq!(exported["units"], 10, "{exported}");

        let perf = rpc(port, "fl.perf", Value::Null);
        assert!(perf["sim"]["counters"].is_object(), "{perf}");
        assert!(perf["frame"]["entities"].as_u64().is_some(), "{perf}");
        let journal = rpc(port, "fl.journal_since", json!({"tick": 0, "limit": 5}));
        assert!(
            journal.as_array().is_some_and(|j| j.len() <= 5),
            "{journal}"
        );
    });
    let text = std::fs::read_to_string(&scenario).expect("the scenario file");
    let def: fl_content::ScenarioDef = ron::Options::default()
        .from_str(&text)
        .unwrap_or_else(|err| panic!("{err}: {text}"));
    assert_eq!(def.id, "snap");
    assert_eq!(def.map, "base.testbed_1km");
    assert_eq!(def.units.len(), 10);
    assert_eq!(def.units[0].unit, "base.de.pz2");
}

/// Whether the RGBA8 image has more than one colour.
fn has_detail(png: &[u8]) -> bool {
    let image = Image::from_buffer(
        png,
        ImageType::Extension("png"),
        CompressedImageFormats::NONE,
        true,
        ImageSampler::Default,
        RenderAssetUsages::default(),
    )
    .expect("a PNG");
    let rgba = image
        .try_into_dynamic()
        .expect("a decodable image")
        .to_rgba8();
    let mut pixels = rgba.pixels();
    let first = *pixels.next().expect("pixels");
    let not_black = rgba.pixels().any(|p| p.0[..3] != [0, 0, 0]);
    not_black && pixels.any(|p| *p != first)
}

#[test]
fn gpu_screenshot_offscreen() {
    let port = free_port();
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("shots").join("offscreen.png");
    let mut app = GameBuilder::new()
        .debug_remote(Some(port))
        .agent_mode(true)
        .hidden_window(true)
        .logs_dir(dir.path().join("logs"))
        // A watchdog: the client thread shuts the app down long before this.
        .exit_after(Some(180.0))
        .prepare()
        .expect("the testbed prepares");
    let shot = path.clone();
    let client = std::thread::spawn(move || {
        let result = std::panic::catch_unwind(|| {
            wait_for_server(port);
            rpc(port, "fl.step", json!({"n": 2}));
            let reply = rpc(port, "fl.screenshot", json!({ "path": shot }));
            assert_eq!(reply["width"], 1280, "{reply}");
            for _ in 0..600 {
                if std::fs::metadata(&shot).is_ok_and(|m| m.len() > 0) {
                    // Let the writer finish.
                    std::thread::sleep(Duration::from_millis(300));
                    return;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            panic!("no screenshot at {} after 60 s", shot.display());
        });
        rpc(port, "brp_extras/shutdown", Value::Null);
        if let Err(panic) = result {
            std::panic::resume_unwind(panic);
        }
    });
    app.run();
    client.join().expect("the screenshot is written");
    let png = std::fs::read(Path::new(&path)).expect("the screenshot");
    assert!(png.len() > 1024, "{} bytes", png.len());
    assert!(has_detail(&png), "the screenshot is blank");
}
