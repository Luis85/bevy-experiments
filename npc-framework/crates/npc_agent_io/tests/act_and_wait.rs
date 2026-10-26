//! `agent/act_and_wait` (act, wait, observe in one call) and
//! `agent/observe {since_seq}` (what changed since the agent last looked)
//! on a toy unit, in process and through the session host (spec G13). The
//! Actor Testbed versions (a Move order, picking up an item) are in
//! `content/testbed_content/tests/agent_interface.rs`.

use std::sync::mpsc;
use std::time::Duration;

use bevy_app::App;
use bevy_ecs::prelude::*;
use npc_agent_io::http::RpcClient;
use npc_agent_io::scenario::{LoadedScenario, ScenarioDoc, ScenarioLoaders};
use npc_agent_io::session::{Session, SessionOptions};
use npc_agent_io::stats::wall_now;
use npc_agent_io::{
    ActionPlan, ActionRequest, AgentCtx, AgentEvents, AgentIoAppExt, AgentIoPlugin, Rejection,
    Step, api, mcp,
};
use npc_ai::{ActiveLeaf, AiRegistry, Brain, ParamValue, Status, StopReason};
use npc_kernel::journal::{self, JournalKind};
use npc_kernel::registry::KernelAppExt;
use npc_kernel::replay::ScenarioInfo;
use npc_kernel::{NpcKernelPlugin, NpcSet, NpcSimTick, SimId, run_ticks, spawn_sim};
use serde_json::{Value, json};

#[derive(Component)]
#[component(storage = "SparseSet")]
struct Hopping(u32);

fn idle_start(_: &mut World, _: Entity, _: &ParamValue) -> Status {
    Status::Running
}
fn idle_stop(_: &mut World, _: Entity, _: StopReason) {}

fn hop_start(world: &mut World, e: Entity, p: &ParamValue) -> Status {
    world
        .entity_mut(e)
        .insert(Hopping(p.as_int().unwrap_or(3) as u32));
    Status::Running
}
fn hop_stop(world: &mut World, e: Entity, _: StopReason) {
    world.entity_mut(e).remove::<Hopping>();
}

fn hop_system(mut hoppers: Query<(&mut Hopping, &mut ActiveLeaf)>) {
    for (mut h, mut leaf) in &mut hoppers {
        h.0 = h.0.saturating_sub(1);
        if h.0 == 0 {
            leaf.status = Status::Success;
        }
    }
}

fn hop_handler(_: &World, _: &AgentCtx, req: &ActionRequest) -> Result<ActionPlan, Rejection> {
    let n = req.int_arg("n").unwrap_or(3) as i32;
    Ok(ActionPlan::Queue {
        label: format!("hop {n}"),
        steps: vec![Step::Action {
            action: "Hop".into(),
            param: ParamValue::Int(n),
        }],
        est_ticks: Some(n as u32),
    })
}

/// A kernel app with the agent plugin, unit 1 (idle unless told to hop)
/// and two plain entities (2, 3) to stand for items.
fn toy_app() -> App {
    let mut app = App::new();
    app.add_plugins(NpcKernelPlugin::default())
        .register_action("Idle", idle_start, idle_stop)
        .register_action("Hop", hop_start, hop_stop)
        .register_tree("toy", |b| {
            b.leaf("Idle", "Idle", ());
        })
        .add_systems(NpcSimTick, hop_system.in_set(NpcSet::Actions))
        .add_plugins(AgentIoPlugin::default())
        .register_action_handler("hop", hop_handler);
    npc_kernel::ensure_frozen(app.world_mut());
    let tree = app.world().resource::<AiRegistry>().tree_id("toy").unwrap();
    spawn_sim(app.world_mut(), Brain { tree });
    spawn_sim(app.world_mut(), ());
    spawn_sim(app.world_mut(), ());
    app
}

fn attached() -> App {
    let mut app = toy_app();
    call(
        &mut app,
        "agent/attach",
        json!({"unit": 1, "controller": "c", "mode": "Direct"}),
    );
    run_ticks(app.world_mut(), 2);
    app
}

fn call(app: &mut App, m: &str, p: Value) -> Value {
    api::call(app.world_mut(), m, Some(p)).unwrap_or_else(|e| panic!("{m}: {e:?}"))
}

fn toy_loader(_: &ScenarioDoc, _: u64) -> Result<LoadedScenario, String> {
    let mut app = toy_app();
    app.world_mut().insert_resource(ScenarioInfo {
        name: "toy".into(),
        villagers: 1,
    });
    Ok(LoadedScenario {
        app,
        actors: vec![SimId(1)],
    })
}

/// A toy session (unit 1 attached Direct as `c`) on a free port.
fn session(tick_hz: u32) -> (RpcClient, std::thread::JoinHandle<()>) {
    let (tx, rx) = mpsc::channel();
    let h = std::thread::spawn(move || {
        let mut loaders = ScenarioLoaders::default();
        loaders.add("toy", toy_loader);
        let doc = ScenarioDoc::from_value(json!({
            "version": 1, "content": "toy",
            "controllers": [{"actor": 0, "controller": "c", "mode": "Direct"}],
        }))
        .unwrap();
        let options = SessionOptions {
            port: 0,
            tick_hz,
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
fn act_and_wait_is_listed_and_answers_at_once_in_process() {
    assert!(api::METHODS.contains(&"agent/act_and_wait"));
    let mut app = attached();
    // `api::call` never waits: the act is done, the events so far and a
    // fresh observation come back at once.
    let r = call(
        &mut app,
        "agent/act_and_wait",
        json!({"unit": 1, "controller": "c", "verb": "hop", "args": {"n": 5}}),
    );
    assert_eq!(r["act"]["accepted"], true, "{r}");
    assert_eq!(r["events"]["events"], json!([]), "{r}");
    assert_eq!(r["events"]["timed_out"], false, "{r}");
    assert_eq!(r["observation"]["unit"], 1, "{r}");
    for k in ["tick", "seq", "epoch"] {
        assert!(r[k].is_u64(), "{k}: {r}");
    }
    // Bad wait params are invalid params, and nothing was acted.
    let before = api::command_log(app.world()).len();
    for bad in [
        json!({"unit": 1, "controller": "c", "verb": "hop", "kinds": ["Nope"]}),
        json!({"unit": 1, "controller": "c", "verb": "hop", "timeout_ms": "soon"}),
        json!({"unit": 1, "controller": "c", "verb": "hop", "observe": {"format": "xml"}}),
        json!({"unit": 1, "controller": "c", "verb": "hop", "until_idle": 1}),
        json!({"unit": 1, "controller": "c", "verb": "hop", "flavour": 1}),
    ] {
        let e = api::call(app.world_mut(), "agent/act_and_wait", Some(bad.clone())).unwrap_err();
        assert_eq!(e.code, api::codes::INVALID_PARAMS, "{bad}: {e:?}");
    }
    run_ticks(app.world_mut(), 1);
    assert_eq!(
        api::command_log(app.world()).len(),
        before + 1,
        "only the first (valid) act became a command"
    );
}

#[test]
fn act_and_wait_waits_for_idle_and_returns_events_and_observation() {
    let mut app = attached();
    let w = api::act_and_wait_start(
        app.world_mut(),
        Some(
            json!({"unit": 1, "controller": "c", "verb": "hop", "args": {"n": 6},
                    "observe": {"format": "text"}}),
        ),
    )
    .expect("starts");
    assert!(!w.answers_at_once());
    assert!(
        !api::act_wait_ready(app.world_mut(), &w),
        "the act is pending"
    );
    let mut ticks = 0;
    while !api::act_wait_ready(app.world_mut(), &w) {
        run_ticks(app.world_mut(), 1);
        ticks += 1;
        assert!(ticks < 50, "never idle");
    }
    assert!(ticks >= 6, "hopped 6 ticks first: {ticks}");
    let r = api::act_wait_result(app.world_mut(), &w, false).expect("result");
    let statuses: Vec<&str> = r["events"]["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["kind"] == "ExternalAction")
        .map(|e| e["event"]["status"].as_str().unwrap())
        .collect();
    assert_eq!(statuses, ["queued", "started", "done"], "{r}");
    assert_eq!(r["events"]["idle"], true, "{r}");
    assert_eq!(r["events"]["timed_out"], false, "{r}");
    let text = r["observation"]["text"].as_str().expect("text observation");
    assert!(
        text.starts_with("Since your last look: "),
        "the observation covers the act: {text}"
    );
    assert!(text.contains("hop done"), "{text}");
}

#[test]
fn a_named_kind_ends_the_wait_before_idle() {
    let mut app = attached();
    let w = api::act_and_wait_start(
        app.world_mut(),
        Some(
            json!({"unit": 1, "controller": "c", "verb": "hop", "args": {"n": 30},
                    "kinds": "ExternalAction"}),
        ),
    )
    .expect("starts");
    let mut ticks = 0;
    while !api::act_wait_ready(app.world_mut(), &w) {
        run_ticks(app.world_mut(), 1);
        ticks += 1;
    }
    assert!(ticks < 5, "the `queued` event fired first: {ticks}");
    let r = api::act_wait_result(app.world_mut(), &w, false).expect("result");
    assert_eq!(r["events"]["idle"], false, "{r}");
    assert!(
        r["events"]["events"]
            .as_array()
            .unwrap()
            .iter()
            .all(|e| e["kind"] == "ExternalAction"),
        "{r}"
    );
}

#[test]
fn rejected_act_does_not_wait() {
    let (c, h) = session(20);
    let started = wall_now();
    let r = c
        .call(
            "agent/act_and_wait",
            json!({"unit": 1, "controller": "intruder", "verb": "hop", "timeout_ms": 20000}),
        )
        .expect("a rejection is a result");
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "answered at once: {:?}",
        started.elapsed()
    );
    assert_eq!(r["act"]["accepted"], false, "{r}");
    assert_eq!(r["act"]["reason"], "not_controller", "{r}");
    assert_eq!(r["events"]["timed_out"], false, "{r}");
    assert_eq!(
        r["observation"]["unit"], 1,
        "a fresh observation anyway: {r}"
    );
    c.call("session/stop", json!({})).expect("stop");
    h.join().expect("session thread");
}

#[test]
fn act_and_wait_times_out() {
    let (c, h) = session(20);
    let r = c
        .call(
            "agent/act_and_wait",
            json!({"unit": 1, "controller": "c", "verb": "hop", "args": {"n": 100000},
                   "timeout_ms": 300}),
        )
        .expect("answered at the timeout");
    assert_eq!(r["act"]["accepted"], true, "{r}");
    assert_eq!(r["events"]["timed_out"], true, "{r}");
    assert_eq!(r["events"]["idle"], false, "{r}");
    assert_eq!(
        r["observation"]["unit"], 1,
        "observation still returned: {r}"
    );
    assert!(
        r["observation"]["queue"]["current"]["verb"] == "hop",
        "still hopping: {r}"
    );
    // timeout_ms over the cap is clamped, not an error.
    let w = c
        .call(
            "agent/act_and_wait",
            json!({"unit": 1, "controller": "c", "verb": "hop", "args": {"n": 1},
                   "timeout_ms": 999_999_999u64}),
        )
        .expect("clamped");
    assert_eq!(w["events"]["timed_out"], false, "{w}");
    c.call("session/stop", json!({})).expect("stop");
    h.join().expect("session thread");
}

#[test]
fn act_and_wait_does_not_block_frames() {
    let (c, h) = session(50);
    let start = tick(&c);
    let url = c.url();
    let waiter = std::thread::spawn(move || {
        let c = RpcClient::new(&url).unwrap();
        c.call(
            "agent/act_and_wait",
            json!({"unit": 1, "controller": "c", "verb": "hop", "args": {"n": 60},
                   "timeout_ms": 60000}),
        )
        .expect("act_and_wait")
    });
    // The session keeps ticking and answering while the call is parked.
    let mut mid = None;
    let deadline = wall_now() + Duration::from_secs(60);
    while !waiter.is_finished() && wall_now() < deadline {
        let t = tick(&c);
        if t > start + 5 {
            mid = Some(t);
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let r = waiter.join().expect("waiter");
    assert!(
        mid.is_some(),
        "the session never ticked while act_and_wait was parked"
    );
    assert_eq!(r["act"]["accepted"], true, "{r}");
    assert_eq!(r["events"]["idle"], true, "{r}");
    assert!(
        r["tick"].as_u64().unwrap() >= start + 60,
        "answered after the hop: {r}"
    );
    let done = r["events"]["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["kind"] == "ExternalAction" && e["event"]["status"] == "done");
    assert!(done, "{r}");
    c.call("session/stop", json!({})).expect("stop");
    h.join().expect("session thread");
}

fn record(app: &mut App, kind: JournalKind) {
    journal::record(app.world_mut(), Some(SimId(1)), || kind);
}

#[test]
fn changes_are_built_from_the_event_buffer() {
    let mut app = attached();
    let since = call(&mut app, "agent/observe", json!({"unit": 1}))["seq"]
        .as_u64()
        .unwrap();
    let (a, b) = (SimId(2), SimId(3));
    record(
        &mut app,
        JournalKind::Perceived {
            target: a,
            via: "vision".into(),
        },
    );
    record(
        &mut app,
        JournalKind::Lost {
            target: b,
            forgotten: false,
        },
    );
    record(
        &mut app,
        JournalKind::ItemMoved {
            item: a,
            kind: "store_key".into(),
            stack: 1,
            from: None,
            to: Some(SimId(1)),
            slot: None,
        },
    );
    record(
        &mut app,
        JournalKind::ItemMoved {
            item: b,
            kind: "ore".into(),
            stack: 3,
            from: Some(SimId(1)),
            to: None,
            slot: None,
        },
    );
    record(&mut app, JournalKind::order_failed("Move", "blocked"));
    record(
        &mut app,
        JournalKind::Interacted {
            verb: "open".into(),
            target: b,
            item: None,
            outcome: "success".into(),
            reason: None,
            dropped: Vec::new(),
        },
    );
    let obs = call(
        &mut app,
        "agent/observe",
        json!({"unit": 1, "since_seq": since}),
    );
    let ch = &obs["changes"];
    assert_eq!(ch["since_seq"], since, "{ch}");
    assert_eq!(ch["truncated"], false, "{ch}");
    assert_eq!(ch["sensed_new"][0]["id"], 2, "{ch}");
    assert_eq!(ch["sensed_new"][0]["via"], "vision", "{ch}");
    assert_eq!(ch["lost"][0]["id"], 3, "{ch}");
    assert_eq!(ch["inventory_gained"][0]["item"], 2, "{ch}");
    assert_eq!(ch["inventory_gained"][0]["kind"], "store_key", "{ch}");
    assert_eq!(ch["inventory_lost"][0]["item"], 3, "{ch}");
    assert_eq!(ch["inventory_lost"][0]["stack"], 3, "{ch}");
    assert_eq!(
        ch["orders"],
        json!([{"order": "Move", "result": "failed", "reason": "blocked"}]),
        "{ch}"
    );
    assert_eq!(ch["interactions"][0]["verb"], "open", "{ch}");
    assert_eq!(ch["interactions"][0]["outcome"], "success", "{ch}");
    // Moving an item between the unit's own slots is no change; picking up
    // and dropping the same item cancels out.
    let since2 = obs["seq"].as_u64().unwrap();
    for (from, to) in [(None, Some(SimId(1))), (Some(SimId(1)), None)] {
        record(
            &mut app,
            JournalKind::ItemMoved {
                item: SimId(3),
                kind: "ore".into(),
                stack: 3,
                from,
                to,
                slot: None,
            },
        );
    }
    let obs = call(
        &mut app,
        "agent/observe",
        json!({"unit": 1, "since_seq": since2}),
    );
    assert_eq!(obs["changes"]["inventory_gained"], json!([]), "{obs}");
    assert_eq!(obs["changes"]["inventory_lost"], json!([]), "{obs}");
    // Without since_seq there is no `changes` (backward compatible).
    let plain = call(&mut app, "agent/observe", json!({"unit": 1}));
    assert!(plain.get("changes").is_none(), "{plain}");
    // A since_seq ahead of the buffer: the world was rebuilt.
    let e = api::call(
        app.world_mut(),
        "agent/observe",
        Some(json!({"unit": 1, "since_seq": 1_000_000})),
    )
    .unwrap_err();
    assert_eq!(e.code, api::codes::INVALID_PARAMS, "{e:?}");
}

#[test]
fn observe_since_old_seq_is_truncated() {
    let mut app = attached();
    app.world_mut().resource_mut::<AgentEvents>().capacity = 4;
    for i in 0..10 {
        record(
            &mut app,
            JournalKind::Perceived {
                target: SimId(2 + i % 2),
                via: "vision".into(),
            },
        );
        call(&mut app, "agent/events", json!({}));
    }
    let obs = call(
        &mut app,
        "agent/observe",
        json!({"unit": 1, "since_seq": 0, "format": "text"}),
    );
    let text = obs["text"].as_str().unwrap();
    let first = text.lines().next().unwrap();
    assert!(first.starts_with("Since your last look"), "{text}");
    assert!(first.contains("older events were dropped"), "{first}");
    let obs = call(
        &mut app,
        "agent/observe",
        json!({"unit": 1, "since_seq": 0}),
    );
    assert_eq!(obs["changes"]["truncated"], true, "{obs}");
    let last = obs["seq"].as_u64().unwrap();
    let recent = call(
        &mut app,
        "agent/observe",
        json!({"unit": 1, "since_seq": last - 1}),
    );
    assert_eq!(recent["changes"]["truncated"], false, "{recent}");
}

#[test]
fn text_has_since_line() {
    let mut app = attached();
    let seq = call(&mut app, "agent/observe", json!({"unit": 1}))["seq"]
        .as_u64()
        .unwrap();
    let text = |app: &mut App, since: u64| -> String {
        call(
            app,
            "agent/observe",
            json!({"unit": 1, "since_seq": since, "format": "text"}),
        )["text"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    let quiet = text(&mut app, seq);
    assert_eq!(
        quiet.lines().next(),
        Some("Nothing changed since your last look."),
        "{quiet}"
    );
    record(
        &mut app,
        JournalKind::Perceived {
            target: SimId(2),
            via: "vision".into(),
        },
    );
    record(
        &mut app,
        JournalKind::OrderDone {
            order: "Move".into(),
        },
    );
    let busy = text(&mut app, seq);
    let first = busy.lines().next().unwrap();
    assert!(first.starts_with("Since your last look: "), "{busy}");
    assert!(first.contains("you now sense entity #2"), "{first}");
    assert!(first.contains("order Move done"), "{first}");
    assert!(first.ends_with('.'), "{first}");
    // The rest is the ordinary observation text.
    let plain = call(
        &mut app,
        "agent/observe",
        json!({"unit": 1, "format": "text"}),
    )["text"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(busy.split_once('\n').unwrap().1, plain);
}

#[test]
fn observe_and_act_and_wait_leave_the_state_hash_alone() {
    // Same act through agent/act and agent/act_and_wait, plus since-last-
    // look observations on one side only: the same state hash.
    let mut plain = attached();
    let mut waited = attached();
    call(
        &mut plain,
        "agent/act",
        json!({"unit": 1, "controller": "c", "verb": "hop", "args": {"n": 4}}),
    );
    let w = api::act_and_wait_start(
        waited.world_mut(),
        Some(json!({"unit": 1, "controller": "c", "verb": "hop", "args": {"n": 4}})),
    )
    .expect("starts");
    for _ in 0..10 {
        run_ticks(plain.world_mut(), 1);
        run_ticks(waited.world_mut(), 1);
        api::act_wait_ready(waited.world_mut(), &w);
        call(
            &mut waited,
            "agent/observe",
            json!({"unit": 1, "since_seq": 0, "format": "text"}),
        );
    }
    api::act_wait_result(waited.world_mut(), &w, false).expect("result");
    assert_eq!(
        npc_kernel::hash::state_hash(plain.world_mut()),
        npc_kernel::hash::state_hash(waited.world_mut())
    );
}

#[test]
fn mcp_lists_act_and_wait() {
    let tools = mcp::tools();
    let tool = tools
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "act_and_wait")
        .expect("act_and_wait tool")
        .clone();
    let props: Vec<String> = tool["inputSchema"]["properties"]
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    let act = tools
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "act")
        .unwrap()
        .clone();
    for f in act["inputSchema"]["properties"].as_object().unwrap().keys() {
        assert!(props.contains(f), "act_and_wait has every act field: {f}");
    }
    for f in ["until_idle", "kinds", "timeout_ms", "observe"] {
        assert!(props.contains(&f.to_owned()), "act_and_wait.{f}");
    }
    assert_eq!(
        tool["inputSchema"]["required"],
        json!(["unit", "controller"])
    );
    let observe = tools
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "observe")
        .unwrap();
    assert!(
        observe["inputSchema"]["properties"]
            .get("since_seq")
            .is_some(),
        "observe.since_seq"
    );
}
