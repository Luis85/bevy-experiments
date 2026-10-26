//! The remote debug endpoint over HTTP: JSON-RPC `fl.*` calls against a headless client
//! (`MinimalPlugins`) with a fake runner on `fl_view::runner_channel` that answers every tick
//! with an empty output (spec §15 C).
#![cfg(all(feature = "debug_remote", debug_assertions))]

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use bevy::prelude::*;
use bevy::remote::{RemoteMethodSystemId, RemoteMethods};
use fl_client::debug::{DebugCommandsPlugin, RemoteDebugPlugin};
use fl_client::driver::LocalDriver;
use fl_client::link::{RunnerBacklog, RunnerLink, RunnerStatus};
use fl_client::view::{LocalPlayer, SimViewPlugin};
use fl_protocol::{MatchHeader, PlayerId, StringTables, Tick, tables};
use fl_view::{
    FromRunner, RunnerPort, SimQuery, SimReply, SimSnapshot, TickDriver, TickOutput, ToRunner,
    runner_channel,
};
use serde_json::{Value, json};

fn header() -> MatchHeader {
    let mut string_tables = StringTables::default();
    string_tables.insert(
        tables::COMMANDS,
        ["core.move", "core.stop", "debug.spawn"]
            .map(str::to_owned)
            .to_vec(),
    );
    MatchHeader {
        format_version: fl_protocol::FORMAT_VERSION,
        build_id: "test".to_owned(),
        sim_version: 1,
        features: Vec::new(),
        mods: Vec::new(),
        rules_hash: 0,
        cheats_allowed: true,
        seed: 1,
        string_tables,
    }
}

/// The state hash the fake runner reports for `tick`.
fn fake_hash(tick: u64) -> u64 {
    0xF1_0000 + tick
}

/// A runner that answers every tick, after `delay`, with an empty output hashed with
/// [`fake_hash`], until the client drops its port.
fn fake_runner(delay: Duration) -> (RunnerPort, JoinHandle<()>) {
    let (port, end) = runner_channel();
    let thread = std::thread::spawn(move || {
        while let Ok(message) = end.rx.recv() {
            if let ToRunner::Query {
                id,
                query: SimQuery::Debug { name, args },
            } = &message
            {
                let reply = match name.as_str() {
                    "core.debug_queries" => {
                        SimReply::Debug(json!({"queries": ["core.debug_queries", "core.paths"]}))
                    }
                    "core.paths" if args.get("units").is_some() => SimReply::Debug(json!({})),
                    "core.paths" => SimReply::Error("give {\"units\": [id, ..]}".to_owned()),
                    other => SimReply::Error(format!("no debug query '{other}'")),
                };
                if end.tx.send(FromRunner::Reply { id: *id, reply }).is_err() {
                    return;
                }
            }
            if let ToRunner::Tick(input) = message {
                std::thread::sleep(delay);
                let output = TickOutput {
                    tick: input.tick,
                    hash: Some(fake_hash(u64::from(input.tick.0))),
                    snapshot: Arc::new(SimSnapshot {
                        tick: input.tick,
                        entities: Vec::new(),
                        channels: Vec::new(),
                    }),
                    events: Vec::new(),
                    journal: Vec::new(),
                };
                if end.tx.send(FromRunner::Tick(output)).is_err() {
                    return;
                }
            }
        }
    });
    (port, thread)
}

/// A free local port.
fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .and_then(|l| l.local_addr())
        .expect("a free port")
        .port()
}

/// A headless client serving BRP on `port`, driving a fake runner.
fn client_app(port: u16) -> App {
    client_app_with(port, Duration::ZERO)
}

/// [`client_app`] whose fake runner takes `delay` per tick.
fn client_app_with(port: u16, delay: Duration) -> App {
    let (runner, _thread) = fake_runner(delay);
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        SimViewPlugin,
        DebugCommandsPlugin,
        RemoteDebugPlugin { port },
    ))
    .insert_resource(RunnerLink {
        port: runner,
        header: header(),
    })
    .insert_resource(LocalDriver::new(Tick(0)))
    .insert_resource(LocalPlayer(PlayerId(0)));
    app
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

/// One JSON-RPC call; returns the whole response object.
fn rpc(port: u16, method: &str, params: Value) -> Value {
    let body = json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}).to_string();
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connects");
    stream
        .set_read_timeout(Some(Duration::from_secs(20)))
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
    serde_json::from_slice(&http_body(&raw)).expect("a JSON-RPC response")
}

/// Waits until the server accepts connections.
fn wait_for_server(port: u16) {
    for _ in 0..400 {
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    panic!("the BRP server never started on port {port}");
}

/// Runs `calls` on a client thread while updating `app` on this one; returns their result.
fn with_client<T: Send + 'static>(
    app: &mut App,
    port: u16,
    calls: impl FnOnce(u16) -> T + Send + 'static,
) -> T {
    let client = std::thread::spawn(move || {
        wait_for_server(port);
        calls(port)
    });
    for _ in 0..4000 {
        if client.is_finished() {
            break;
        }
        app.update();
        std::thread::sleep(Duration::from_millis(5));
    }
    client.join().expect("the client thread finished")
}

#[test]
fn brp_issue_command_stamps_and_queues() {
    let port = free_port();
    let mut app = client_app(port);
    app.world_mut().resource_mut::<LocalDriver>().pause();
    let reply = with_client(&mut app, port, |port| {
        rpc(
            port,
            "fl.issue_command",
            json!({"player": 0, "units": [1], "command": "core.move",
                   "payload": {"target": [200, 100]}}),
        )
    });
    assert_eq!(
        reply["result"],
        json!({"exec_tick": 1, "seq": 0, "player": 0}),
        "{reply}"
    );
    let pending = app.world().resource::<LocalDriver>().pending();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].exec_tick, Tick(1));
    assert_eq!(pending[0].units, [fl_protocol::SimId(1)]);
}

#[test]
fn brp_unknown_command_is_error() {
    let port = free_port();
    let mut app = client_app(port);
    let reply = with_client(&mut app, port, |port| {
        rpc(
            port,
            "fl.issue_command",
            json!({"player": 0, "units": [1], "command": "core.fly", "payload": {}}),
        )
    });
    let message = reply["error"]["message"].as_str().unwrap_or_default();
    assert!(message.contains("core.fly"), "{reply}");
    assert!(app.world().resource::<LocalDriver>().pending().is_empty());
}

#[test]
fn brp_step_keeps_tick_outputs_for_the_view() {
    let port = free_port();
    let mut app = client_app(port);
    let (first, second, hash) = with_client(&mut app, port, |port| {
        (
            rpc(port, "fl.step", json!({"n": 5})),
            rpc(port, "fl.step", json!({"n": 3})),
            rpc(port, "fl.hash", Value::Null),
        )
    });
    let first = first["result"]["tick"].as_u64().expect("a tick");
    assert!(first >= 5, "{first}");
    assert_eq!(
        second["result"]["tick"].as_u64(),
        Some(first + 3),
        "{second}"
    );
    // The hash of the last tick stepped.
    let expected = format!("{:016x}", fake_hash(first + 2));
    assert_eq!(hash["result"]["hash"], json!(expected), "{hash}");
    assert_eq!(hash["result"]["last_tick"], json!(first + 2), "{hash}");
    assert_eq!(second["result"]["finished"], true, "{second}");
    assert_eq!(second["result"]["completed"], 3, "{second}");
    // Every tick output read while waiting reached the view: the last one is the last tick run.
    app.update();
    let driver = app.world().resource::<LocalDriver>();
    assert!(driver.is_paused());
    let status = app.world().resource::<RunnerStatus>();
    assert_eq!(
        status.last_tick,
        Some(Tick(u32::try_from(first + 2).expect("small")))
    );
}

#[test]
fn brp_methods_are_published_as_agent_tools() {
    let port = free_port();
    let mut app = client_app(port);
    let (tools, discover) = with_client(&mut app, port, |port| {
        (
            rpc(port, "brp_extras/agent_tools", Value::Null),
            rpc(port, "rpc.discover", Value::Null),
        )
    });
    let names: Vec<&str> = tools["result"]["tools"]
        .as_array()
        .expect("a tool list")
        .iter()
        .filter_map(|t| t["name"].as_str())
        .collect();
    for method in [
        "fl.sim_state",
        "fl.step",
        "fl.issue_command",
        "fl.screenshot",
    ] {
        assert!(names.contains(&method), "{method} missing from {names:?}");
        assert!(
            discover.to_string().contains(method),
            "{method} not discoverable"
        );
    }
}

#[test]
fn brp_step_is_capped() {
    let port = free_port();
    let mut app = client_app(port);
    let reply = with_client(&mut app, port, |port| {
        rpc(port, "fl.step", json!({"n": 601}))
    });
    assert_eq!(reply["error"]["code"], -32602, "{reply}");
    assert!(app.world().resource::<LocalDriver>().pending().is_empty());
}

#[test]
fn brp_step_past_the_deadline_reports_progress_and_queues_the_rest() {
    let port = free_port();
    // 100 ticks of 50 ms each take 5 s, more than the 2 s a method may wait.
    let mut app = client_app_with(port, Duration::from_millis(50));
    let reply = with_client(&mut app, port, |port| {
        rpc(port, "fl.step", json!({"n": 100}))
    });
    let result = &reply["result"];
    assert_eq!(result["finished"], false, "{reply}");
    assert_eq!(result["requested"], 100, "{reply}");
    let completed = result["completed"].as_u64().expect("a count");
    assert!(completed > 0 && completed < 100, "{reply}");
    let first = result["tick"].as_u64().expect("a tick") - completed;
    // The rest run over the next frames.
    let last = Tick(u32::try_from(first + 99).expect("small"));
    for _ in 0..2000 {
        if app.world().resource::<RunnerStatus>().last_tick >= Some(last) {
            break;
        }
        app.update();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(app.world().resource::<RunnerStatus>().last_tick, Some(last));
    app.update();
    assert_eq!(app.world().resource::<RunnerStatus>().last_tick, Some(last));
}

/// Calls the BRP method `name` directly, as the remote plugin does, without HTTP or a frame.
fn call(app: &mut App, name: &str, params: Value) -> Value {
    let Some(&RemoteMethodSystemId::Instant(id)) =
        app.world().resource::<RemoteMethods>().get(name)
    else {
        panic!("{name} is not an instant method");
    };
    app.world_mut()
        .run_system_with(id, Some(params))
        .expect("the method runs")
        .unwrap_or_else(|err| panic!("{name}: {err:?}"))
}

#[test]
fn brp_hash_after_step_counts_the_backlog() {
    let mut app = client_app(free_port());
    app.update();
    let stepped = call(&mut app, "fl.step", json!({"n": 4}));
    let next = stepped["tick"].as_u64().expect("a tick");
    // No frame ran since: the outputs sit in the backlog, not yet in `RunnerStatus`.
    assert!(!app.world().resource::<RunnerBacklog>().0.is_empty());
    let hash = call(&mut app, "fl.hash", Value::Null);
    assert_eq!(
        hash["hash"],
        json!(format!("{:016x}", fake_hash(next - 1))),
        "{hash}"
    );
    assert_eq!(hash["last_tick"], json!(next - 1), "{hash}");
}

#[test]
fn brp_select_and_read_selection() {
    use fl_client::link::SnapshotBuffer;
    use fl_client::selection::SelectionPlugin;
    use fl_math::{Angle, FxVec2};
    use fl_protocol::{SimId, UnitTypeId};
    use fl_view::EntityRecord;

    let record = |id: u32, owner: u8| EntityRecord {
        sim_id: SimId(id),
        unit: UnitTypeId(0),
        owner: PlayerId(owner),
        pos: FxVec2::ZERO,
        heading: Angle::from_raw(0),
        turrets: Vec::new(),
        hp_frac: u16::MAX,
        flags: 0,
        visible_to: 0xFF,
    };
    let port = free_port();
    let mut app = client_app(port);
    app.add_plugins(SelectionPlugin);
    // Paused, so the fake runner's empty snapshots never replace this one.
    app.world_mut().resource_mut::<LocalDriver>().pause();
    app.world_mut()
        .resource_mut::<SnapshotBuffer>()
        .push(Arc::new(SimSnapshot {
            tick: Tick(0),
            entities: vec![record(1, 0), record(6, 1)],
            channels: Vec::new(),
        }));
    let (selected, read) = with_client(&mut app, port, |port| {
        (
            rpc(port, "fl.select", json!({"sim_ids": [1, 6, 99]})),
            rpc(port, "fl.selection", Value::Null),
        )
    });
    assert_eq!(selected["result"], json!({"selected": [1]}), "{selected}");
    assert_eq!(read["result"], json!({"selected": [1]}), "{read}");
}

#[test]
fn brp_debug_query_round_trip() {
    let port = free_port();
    let mut app = client_app(port);
    let (list, no_args, bad_args, unknown, extra) = with_client(&mut app, port, |port| {
        (
            rpc(
                port,
                "fl.debug_query",
                json!({"name": "core.debug_queries"}),
            ),
            rpc(
                port,
                "fl.debug_query",
                json!({"name": "core.paths", "args": {"units": [1]}}),
            ),
            rpc(
                port,
                "fl.debug_query",
                json!({"name": "core.paths", "args": {}}),
            ),
            rpc(port, "fl.debug_query", json!({"name": "core.nope"})),
            rpc(
                port,
                "fl.debug_query",
                json!({"name": "core.paths", "arg": 1}),
            ),
        )
    });
    assert_eq!(
        list["result"]["queries"],
        json!(["core.debug_queries", "core.paths"]),
        "{list}"
    );
    assert_eq!(no_args["result"], json!({}), "{no_args}");
    // A query's error is the error's message.
    let message = bad_args["error"]["message"].as_str().unwrap_or_default();
    assert!(message.contains("units"), "{bad_args}");
    let message = unknown["error"]["message"].as_str().unwrap_or_default();
    assert!(message.contains("core.nope"), "{unknown}");
    assert_eq!(extra["error"]["code"], -32602, "{extra}");
}

#[test]
fn brp_step_records_its_inputs() {
    // A replay of the match counts the ticks fl.step sends past drive_runner.
    let mut app = client_app(free_port());
    app.init_resource::<fl_client::link::InputLog>();
    app.update();
    let before = app.world().resource::<fl_client::link::InputLog>().ticks;
    let stepped = call(&mut app, "fl.step", json!({"n": 4}));
    assert_eq!(stepped["completed"], json!(4), "{stepped}");
    let log = app.world().resource::<fl_client::link::InputLog>();
    assert_eq!(log.ticks, before + 4);
    assert!(log.inputs.is_empty(), "no commands, no inputs kept");
}
