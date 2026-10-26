//! Task I: the `npc/*` JSON-RPC methods over real HTTP (and the read-only
//! `agent/*` / `world/state` methods of `AgentRemotePlugin`).
//!
//! Each test builds a headless app (`MinimalPlugins` + the village scenario +
//! journal + `HeadlessDriverPlugin` + `NpcRemotePlugin` on a free port) and
//! drives it with `app.update()` on the test thread while a helper thread
//! makes the blocking HTTP call.

use std::collections::BTreeSet;
use std::net::TcpListener;
use std::sync::mpsc;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use bevy::prelude::*;
use npc_framework::agent_io::remote::AgentRemotePlugin;
use npc_framework::kernel::registry::content_hash;
use npc_framework::perf::{PerfCollector, PerfHandle, install_global};
use npc_framework::prelude::JournalPlugin;
use npc_framework::tools::HeadlessDriverPlugin;
use npc_framework::tools::remote::NpcRemotePlugin;
use serde_json::{Value, json};
use village_content::replay::{Replay, new_sim_app, verify};
use village_content::scenario::VillageIds;

/// The process-wide profiler, installed before any app (and so before any
/// system span) exists.
fn collector() -> &'static PerfCollector {
    static INSTALLED: OnceLock<()> = OnceLock::new();
    INSTALLED.get_or_init(|| {
        assert!(install_global(PerfCollector::global()), "global subscriber");
    });
    PerfCollector::global()
}

/// A port no other test of this process got (the OS may hand the same
/// ephemeral port out twice once the probe listener is closed).
fn free_port() -> u16 {
    static TAKEN: Mutex<BTreeSet<u16>> = Mutex::new(BTreeSet::new());
    loop {
        let port = TcpListener::bind("127.0.0.1:0")
            .and_then(|l| l.local_addr())
            .map(|a| a.port())
            .expect("free port");
        if TAKEN.lock().unwrap().insert(port) {
            return port;
        }
    }
}

struct Rpc {
    app: App,
    url: String,
    next_id: u64,
}

impl Rpc {
    fn new() -> Self {
        Self::build(false, false)
    }

    /// With `perf`, the app gets the (process-wide) collector as `PerfHandle`
    /// and closes a profiler frame per tick. Only one test does, so the
    /// frame count it sees is its own.
    ///
    /// With `agent`, `AgentRemotePlugin` is added too (as `apps/village`
    /// does with `debug-tools`).
    fn build(perf: bool, agent: bool) -> Self {
        let collector = collector().clone();
        let port = free_port();
        let mut app = new_sim_app(1, "village", 1);
        app.add_plugins((
            MinimalPlugins,
            JournalPlugin::default(),
            HeadlessDriverPlugin,
            NpcRemotePlugin::new(port),
        ));
        if perf {
            app.insert_resource(PerfHandle(collector));
        }
        if agent {
            app.add_plugins(AgentRemotePlugin);
        }
        app.finish();
        app.cleanup();
        // Startup: opens the mailbox and starts the HTTP server.
        app.update();
        Self {
            app,
            url: format!("http://127.0.0.1:{port}/"),
            next_id: 1,
        }
    }

    /// The full JSON-RPC response of `method(params)`.
    fn raw(&mut self, method: &str, params: Value) -> Value {
        let mut body = json!({"jsonrpc": "2.0", "id": self.next_id, "method": method});
        if !params.is_null() {
            body["params"] = params;
        }
        self.next_id += 1;
        let url = self.url.clone();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            // The server may still be binding on the first call: retry briefly.
            let deadline = Instant::now() + Duration::from_secs(5);
            let result = loop {
                match ureq::post(&url).send_json(&body) {
                    Ok(mut resp) => {
                        break resp
                            .body_mut()
                            .read_json::<Value>()
                            .map_err(|e| e.to_string());
                    }
                    Err(e) if Instant::now() > deadline => break Err(e.to_string()),
                    Err(_) => std::thread::sleep(Duration::from_millis(20)),
                }
            };
            let _ = tx.send(result);
        });
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            self.app.update();
            if let Ok(result) = rx.try_recv() {
                return result.unwrap_or_else(|e| panic!("{method}: http error {e}"));
            }
            assert!(Instant::now() < deadline, "{method}: no response");
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    /// The `result` of `method(params)`; panics on an error response.
    fn call(&mut self, method: &str, params: Value) -> Value {
        let resp = self.raw(method, params);
        match resp.get("result") {
            Some(result) => result.clone(),
            None => panic!("{method} failed: {resp}"),
        }
    }

    /// The `error` of `method(params)`; panics on success.
    fn error(&mut self, method: &str, params: Value) -> Value {
        let resp = self.raw(method, params);
        match resp.get("error") {
            Some(err) => err.clone(),
            None => panic!("{method} unexpectedly succeeded: {resp}"),
        }
    }

    fn ids(&self) -> VillageIds {
        *self.app.world().resource::<VillageIds>()
    }

    /// Sim entities alive now (the scenario's, counted instead of
    /// hard-coded: a new scenario entity changes it).
    fn sim_entities(&self) -> usize {
        self.app
            .world()
            .resource::<npc_framework::kernel::ids::SimIndex>()
            .map
            .len()
    }
}

#[test]
fn rpc_pause_step_state() {
    let mut rpc = Rpc::new();
    let paused = rpc.call("npc/pause", Value::Null);
    assert_eq!(paused["paused"], true);
    let t0 = rpc.call("npc/state", Value::Null)["tick"].as_u64().unwrap();
    // Paused: frames pass, ticks do not.
    for _ in 0..5 {
        rpc.app.update();
    }
    assert_eq!(
        rpc.call("npc/state", Value::Null)["tick"].as_u64().unwrap(),
        t0
    );

    let step = rpc.call("npc/step", json!({"ticks": 10}));
    assert_eq!(step["tick"].as_u64().unwrap(), t0 + 10, "{step}");
    assert!(step["hash"].is_u64(), "{step}");
    assert_eq!(step["hash_hex"].as_str().unwrap().len(), 16, "{step}");

    let state = rpc.call("npc/state", Value::Null);
    assert_eq!(state["tick"].as_u64().unwrap(), t0 + 10);
    let entities = state["entities"].as_array().unwrap();
    assert_eq!(
        entities.len(),
        6,
        "house, bed, stockpile, field, villager, well"
    );
    assert!(entities.iter().any(|e| e["kind"] == "villager"));

    // A default step is one tick; speed and resume report the control state.
    let one = rpc.call("npc/step", Value::Null);
    assert_eq!(one["tick"].as_u64().unwrap(), t0 + 11);
    let speed = rpc.call("npc/speed", json!({"multiplier": 3}));
    assert_eq!(speed["multiplier"], 3);
    assert_eq!(speed["paused"], true);
    let resumed = rpc.call("npc/resume", Value::Null);
    assert_eq!(resumed["paused"], false);
    for _ in 0..3 {
        rpc.app.update();
    }
    let now = rpc.call("npc/state", Value::Null)["tick"].as_u64().unwrap();
    assert!(now > t0 + 11, "running again: {now}");
}

#[test]
fn rpc_command_order_and_unit() {
    let mut rpc = Rpc::new();
    rpc.call("npc/pause", Value::Null);
    let t = rpc.call("npc/step", json!({"ticks": 5}))["tick"]
        .as_u64()
        .unwrap();
    let v = rpc.ids().villager.0;
    let accepted = rpc.call(
        "npc/command",
        json!({"cmd": {"type": "Order", "unit": v, "order": {"type": "GoTo", "pos": [8000, 8000]}}}),
    );
    assert_eq!(accepted["tick"].as_u64().unwrap(), t, "{accepted}");
    assert_eq!(accepted["seq"], 0);
    rpc.call("npc/step", json!({"ticks": 1}));

    let unit = rpc.call("npc/unit", json!({"id": v}));
    let path: Vec<&str> = unit["tree_path"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert!(path.contains(&"FollowOrder"), "{unit}");
    assert_eq!(unit["order"]["type"], "GoTo", "{unit}");
    assert_eq!(unit["active_leaf"]["action"], "FollowOrder", "{unit}");
    // The applied command is in the unit's recent journal.
    let journal = unit["recent_journal"].as_array().unwrap();
    assert!(
        journal.iter().any(|e| e["kind"]["type"] == "Command"),
        "{unit}"
    );

    // Scheduling in the future, and in the past.
    let later = rpc.call(
        "npc/command",
        json!({"cmd": {"type": "Order", "unit": v, "order": {"type": "Clear"}}, "at_tick": t + 100}),
    );
    assert_eq!(later["tick"].as_u64().unwrap(), t + 100);
    let past = rpc.error(
        "npc/command",
        json!({"cmd": {"type": "Order", "unit": v, "order": {"type": "Clear"}}, "at_tick": 1}),
    );
    assert_eq!(past["data"]["error"], "TickInPast", "{past}");
}

#[test]
fn rpc_unknown_unit_error() {
    let mut rpc = Rpc::new();
    let err = rpc.error("npc/unit", json!({"id": 999}));
    assert_eq!(err["data"]["error"], "UnknownUnit", "{err}");
    assert_eq!(err["data"]["unit"], 999, "{err}");
    assert!(err["message"].as_str().unwrap().contains("999"), "{err}");

    let err = rpc.error(
        "npc/command",
        json!({"cmd": {"type": "Order", "unit": 999, "order": {"type": "GoTo", "pos": [0, 0]}}}),
    );
    assert_eq!(err["data"]["error"], "UnknownUnit", "{err}");

    // Wrong kind, bad params and a missing id are errors too, never panics.
    let house = rpc.ids().house.0;
    let err = rpc.error(
        "npc/command",
        json!({"cmd": {"type": "Order", "unit": house, "order": {"type": "Clear"}}}),
    );
    assert_eq!(err["data"]["error"], "WrongKind", "{err}");
    let err = rpc.error("npc/command", json!({"cmd": {"type": "Fly"}}));
    assert_eq!(err["code"], -32602, "{err}");
    let err = rpc.error("npc/unit", Value::Null);
    assert_eq!(err["code"], -32602, "{err}");
    let err = rpc.error("npc/step", json!({"ticks": "ten"}));
    assert_eq!(err["code"], -32602, "{err}");
    // The server still works afterwards.
    rpc.call("npc/state", Value::Null);
}

#[test]
fn rpc_journal_filter() {
    let mut rpc = Rpc::new();
    rpc.call("npc/pause", Value::Null);
    let t = rpc.call("npc/step", json!({"ticks": 50}))["tick"]
        .as_u64()
        .unwrap();
    let v = rpc.ids().villager.0;
    let stockpile = rpc.ids().stockpile.0;
    rpc.call(
        "npc/command",
        json!({"cmd": {"type": "SetFood", "stockpile": stockpile, "food": 7}}),
    );
    rpc.call("npc/step", json!({"ticks": 1}));

    let all = rpc.call("npc/journal", Value::Null);
    let all = all.as_array().unwrap();
    assert!(!all.is_empty());

    let mine = rpc.call("npc/journal", json!({"filter": {"unit": v}}));
    let mine = mine.as_array().unwrap();
    assert!(!mine.is_empty());
    assert!(mine.iter().all(|e| e["unit"] == v), "only unit {v}");
    assert!(mine.len() < all.len());

    let cmds = rpc.call("npc/journal", json!({"filter": {"kinds": ["command"]}}));
    let cmds = cmds.as_array().unwrap();
    assert_eq!(cmds.len(), 1, "{cmds:?}");
    assert_eq!(cmds[0]["kind"]["cmd"]["type"], "SetFood");
    assert_eq!(cmds[0]["tick"].as_u64().unwrap(), t);

    // The filter can also be given directly as the params.
    let last = rpc.call("npc/journal", json!({"limit": 3, "unit": v}));
    let last = last.as_array().unwrap();
    assert_eq!(last.len(), 3);
    assert_eq!(last[2], mine[mine.len() - 1]);

    let window = rpc.call("npc/journal", json!({"filter": {"from": 10, "to": 20}}));
    assert!(
        window
            .as_array()
            .unwrap()
            .iter()
            .all(|e| (10..=20).contains(&e["tick"].as_u64().unwrap()))
    );
}

#[test]
fn rpc_perf_has_mechanics() {
    let mut rpc = Rpc::build(true, false);
    // Let the headless driver run (one tick and one `end_frame` per update).
    for _ in 0..30 {
        rpc.app.update();
    }
    let report = rpc.call("npc/perf", Value::Null);
    let mechanics: Vec<&str> = report["mechanics"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|m| m["mechanic"].as_str())
        .collect();
    for want in ["think", "needs", "inputs"] {
        assert!(
            mechanics.contains(&want),
            "{want} missing from {mechanics:?}"
        );
    }
    assert!(report["frames"].as_u64().unwrap() > 0);
    assert!(
        report["systems"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["name"] == "npc_kernel::schedule::think_system"),
        "systems: {}",
        report["systems"]
    );
    let top = rpc.call("npc/perf", json!({"top": 2}));
    assert!(top["systems"].as_array().unwrap().len() <= 2);
    assert!(top["mechanics"].as_array().unwrap().len() <= 2);

    // Paused, nothing closes a frame, so a reset leaves exactly 0 frames;
    // a headless `npc/step` then closes one frame per tick.
    rpc.call("npc/pause", Value::Null);
    rpc.call("npc/perf/reset", Value::Null);
    let after = rpc.call("npc/perf", Value::Null);
    assert_eq!(after["frames"], 0, "{after}");
    rpc.call("npc/step", json!({"ticks": 3}));
    assert_eq!(rpc.call("npc/perf", Value::Null)["frames"], 3);

    // Without a PerfHandle the method says so.
    let mut plain = Rpc::new();
    let err = plain.error("npc/perf", Value::Null);
    assert!(
        err["message"].as_str().unwrap().contains("profiler"),
        "{err}"
    );
}

#[test]
fn rpc_builtin_world_query_sees_simid() {
    let mut rpc = Rpc::new();
    let rows = rpc.call(
        "world.query",
        json!({"data": {"components": ["npc_sim::ids::SimId", "npc_sim::math::SimPos"]}}),
    );
    let rows = rows.as_array().unwrap();
    let n = rpc.sim_entities();
    assert_eq!(rows.len(), n, "{rows:?}");
    let ids: Vec<u64> = rows
        .iter()
        .filter_map(|r| r["components"]["npc_sim::ids::SimId"].as_u64())
        .collect();
    assert_eq!(ids.len(), n, "SimId serializes as a number: {rows:?}");
    assert!(ids.contains(&u64::from(rpc.ids().villager.0)));
}

#[test]
fn rpc_record_start_stop_verifies() {
    let mut rpc = Rpc::new();
    rpc.call("npc/pause", Value::Null);
    let started = rpc.call("npc/record/start", json!({"every": 20}));
    assert_eq!(started["recording"], true, "{started}");
    let v = rpc.ids().villager.0;
    rpc.call("npc/step", json!({"ticks": 30}));
    rpc.call(
        "npc/command",
        json!({"cmd": {"type": "Order", "unit": v, "order": {"type": "GoTo", "pos": [2000, -3000]}}}),
    );
    rpc.call("npc/step", json!({"ticks": 25}));
    // Unpaused frames between steps are checkpointed too.
    rpc.call("npc/resume", Value::Null);
    for _ in 0..30 {
        rpc.app.update();
    }
    rpc.call("npc/pause", Value::Null);

    let dir = std::env::temp_dir().join(format!("npc_tools_rpc_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    rpc.app
        .insert_resource(npc_framework::tools::out_path::OutputRoot::new(&dir));
    // Client paths are confined under the output root: an absolute path or
    // `..` is refused before the recording stops.
    for bad in [
        json!({"path": dir.join("abs.json")}),
        json!({"path": "../escape.json"}),
        json!({"path": "C:/x/run.json"}),
    ] {
        let err = rpc.error("npc/record/stop", bad.clone());
        assert!(
            err["message"].as_str().unwrap().contains("refused"),
            "{bad}: {err}"
        );
    }
    let path = dir.join("runs").join("run.replay.json");
    let replay = rpc.call("npc/record/stop", json!({"path": "runs/run.replay"}));
    let replay: Replay = serde_json::from_value(replay).expect("a Replay");
    assert_eq!(replay.scenario, "village");
    assert_eq!(replay.commands.len(), 1);
    assert!(replay.checkpoints.len() >= 3, "{:?}", replay.checkpoints);
    assert_eq!(replay.checkpoints.last().unwrap().0, replay.ticks);

    let from_file: Replay =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("replay file")).unwrap();
    assert_eq!(from_file, replay);
    let report = verify(&replay);
    assert!(report.ok, "{report:?}");
    let _ = std::fs::remove_dir_all(dir);

    // Stopping twice is an error.
    let err = rpc.error("npc/record/stop", Value::Null);
    assert!(
        err["message"].as_str().unwrap().contains("not recording"),
        "{err}"
    );
}

/// `apps/village` serves `agent/*` with `AgentRemotePlugin` but no
/// `AgentIoPlugin`: the methods read (list, observe, events, world state),
/// attaching is rejected, and the village's content hash is unchanged.
#[test]
fn rpc_agent_methods_read_the_village() {
    let plain = content_hash(new_sim_app(1, "village", 1).world_mut());
    let mut rpc = Rpc::build(false, true);
    assert_eq!(
        content_hash(rpc.app.world_mut()),
        plain,
        "AgentRemotePlugin registers nothing in the sim"
    );
    let villager = rpc.ids().villager.0;
    let units = rpc.call("agent/list_units", json!({}));
    let listed: Vec<u64> = units["units"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|u| u["id"].as_u64())
        .collect();
    assert!(listed.contains(&u64::from(villager)), "{units}");
    let obs = rpc.call("agent/observe", json!({"unit": villager, "format": "text"}));
    assert!(obs["text"].as_str().is_some_and(|t| !t.is_empty()), "{obs}");
    let state = rpc.call("world/state", Value::Null);
    assert!(state.is_object(), "{state}");
    rpc.call("agent/events", json!({"since_seq": 0}));
    rpc.error("agent/attach", json!({"unit": villager}));
}
