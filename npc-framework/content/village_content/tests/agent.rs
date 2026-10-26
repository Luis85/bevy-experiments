//! External agent control of villagers (agent-control spec §5): attach and
//! suspend, act, queue, Advisor and Observer modes, staleness, idle
//! policies, replays without the agent, the greedy reference controller and
//! the session host's events long-poll.

use std::sync::mpsc;
use std::time::Duration;

use bevy_app::App;
use bevy_ecs::prelude::*;
use npc_framework::agent_io::api;
use npc_framework::agent_io::greedy::{Decision, Greedy, NeedRule, decide};
use npc_framework::agent_io::http::RpcClient;
use npc_framework::agent_io::scenario::{self, ScenarioDoc, ScenarioLoaders, SessionRecord};
use npc_framework::agent_io::session::{Session, SessionOptions};
use npc_framework::agent_io::stats::wall_now;
use npc_framework::ai::BrainState;
use npc_framework::kernel::journal::{Journal, JournalKind};
use npc_framework::kernel::replay::Recorder;
use npc_framework::prelude::*;
use serde_json::{Value, json};
use village_content::agent::register_scenario_loader;
use village_content::villager::VillagerTree;

/// The iteration-1 village's villager (`VillageIds::villager`; scenario
/// entities keep their ids when new ones spawn after them, see
/// `.claude/rules/content.md` "SimId stability"; checked by
/// `eat_and_sleep_run_the_villager_actions`).
const VILLAGER: u32 = 5;

fn loaders() -> ScenarioLoaders {
    let mut l = ScenarioLoaders::default();
    register_scenario_loader(&mut l);
    l
}

fn doc(v: Value) -> ScenarioDoc {
    ScenarioDoc::from_value(v).expect("valid scenario")
}

fn plain() -> ScenarioDoc {
    doc(json!({"version": 1, "content": "village"}))
}

fn app(d: &ScenarioDoc) -> App {
    scenario::load(&loaders(), d, Some(1)).expect("loads").app
}

fn call(app: &mut App, method: &str, params: Value) -> Value {
    api::call(app.world_mut(), method, Some(params)).unwrap_or_else(|e| panic!("{method}: {e:?}"))
}

fn ticks(app: &mut App, n: u64) {
    run_ticks(app.world_mut(), n);
}

fn tick(app: &App) -> u64 {
    app.world().resource::<SimClock>().tick
}

fn entity(app: &App, id: u32) -> Entity {
    app.world()
        .resource::<SimIndex>()
        .get(SimId(id))
        .expect("unit")
}

fn pos(app: &App, id: u32) -> SimPos {
    *app.world().get::<SimPos>(entity(app, id)).expect("pos")
}

fn path(app: &App, id: u32) -> String {
    let state = app
        .world()
        .get::<BrainState>(entity(app, id))
        .expect("brain");
    let tree = app.world().resource::<VillagerTree>().0;
    app.world()
        .resource::<AiRegistry>()
        .tree(tree)
        .path_string(&state.running_path)
}

/// `(tick, handle, verb, status)` of every `ExternalAction` entry.
fn actions(app: &App) -> Vec<(u64, u32, String, String)> {
    app.world()
        .resource::<Journal>()
        .entries
        .iter()
        .filter_map(|e| match &e.kind {
            JournalKind::ExternalAction {
                handle,
                verb,
                status,
                ..
            } => Some((e.tick, *handle, verb.clone(), status.clone())),
            _ => None,
        })
        .collect()
}

fn attach(app: &mut App, mode: &str, on_idle: &str) {
    let r = call(
        app,
        "agent/attach",
        json!({"unit": VILLAGER, "controller": "a", "mode": mode, "on_idle": on_idle}),
    );
    assert_eq!(r["controller"], "a");
}

fn act(app: &mut App, params: Value) -> Value {
    let mut p = params;
    p["unit"] = json!(VILLAGER);
    p["controller"] = json!("a");
    call(app, "agent/act", p)
}

const UNDER_CONTROL: &str = "ExternalControlRoot > ExternalControl > ExternalAction";

#[test]
fn plain_village_has_no_external_branch() {
    let app = village_content::new_sim_app(1, "village", 1);
    let tree = app.world().resource::<VillagerTree>().0;
    let def = app.world().resource::<AiRegistry>().tree(tree);
    assert_eq!(
        def.nodes[0].name, "Villager",
        "no branch without AgentIoPlugin"
    );
    let mut with = app_with_plugin();
    let tree = with.world().resource::<VillagerTree>().0;
    assert_eq!(
        with.world().resource::<AiRegistry>().tree(tree).nodes[0].name,
        "ExternalControlRoot"
    );
    // Uncontrolled, the villager behaves as without the plugin (same path
    // below the wrapper, same position).
    let mut plain = village_content::new_sim_app(1, "village", 1);
    run_ticks(plain.world_mut(), 600);
    ticks(&mut with, 600);
    assert_eq!(
        pos(&with, VILLAGER),
        *plain
            .world()
            .get::<SimPos>(entity(&plain, VILLAGER))
            .unwrap()
    );
    assert_eq!(
        path(&with, VILLAGER),
        format!("ExternalControlRoot > {}", path(&plain, VILLAGER))
    );
}

fn app_with_plugin() -> App {
    app(&plain())
}

#[test]
fn direct_attach_suspends_the_tree_and_detach_resumes_it() {
    let mut a = app(&plain());
    attach(&mut a, "Direct", "Idle");
    ticks(&mut a, 300);
    assert_eq!(path(&a, VILLAGER), UNDER_CONTROL);
    assert_eq!(pos(&a, VILLAGER), SimPos::new(0, 0), "idles until acted on");
    let obs = call(&mut a, "agent/observe", json!({"unit": VILLAGER}));
    assert_eq!(obs["controller"]["mode"], "Direct");
    assert_eq!(obs["omniscient"], true);
    assert!(obs["queue"]["current"].is_null());
    let verbs: Vec<&str> = obs["affordances"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["verb"].as_str().unwrap())
        .collect();
    for v in ["walk_to", "eat", "sleep", "work", "wait"] {
        assert!(verbs.contains(&v), "{v} offered: {verbs:?}");
    }
    assert!(obs["sections"]["needs"]["Hunger"]["value"].is_i64());
    call(
        &mut a,
        "agent/detach",
        json!({"unit": VILLAGER, "controller": "a"}),
    );
    ticks(&mut a, 300);
    assert!(
        !path(&a, VILLAGER).contains("ExternalControl >"),
        "the tree took over: {}",
        path(&a, VILLAGER)
    );
    assert_ne!(
        pos(&a, VILLAGER),
        SimPos::new(0, 0),
        "the villager went about its day"
    );
}

#[test]
fn act_applies_at_the_next_tick_and_completes() {
    let mut a = app(&plain());
    attach(&mut a, "Direct", "Idle");
    ticks(&mut a, 10);
    let t = tick(&a);
    let r = act(&mut a, json!({"verb": "walk_to", "pos": [1200, 0]}));
    assert_eq!(r["accepted"], true, "{r}");
    assert_eq!(r["applies_at"], t, "stamped with the next tick to run");
    let handle = r["handle"].as_u64().unwrap() as u32;
    ticks(&mut a, 1);
    let log = actions(&a);
    assert!(
        log.contains(&(t, handle, "walk_to".into(), "queued".into())),
        "{log:?}"
    );
    assert!(
        log.contains(&(t, handle, "walk_to".into(), "started".into())),
        "{log:?}"
    );
    assert_ne!(
        pos(&a, VILLAGER),
        SimPos::new(0, 0),
        "moving in the tick it applied"
    );
    ticks(&mut a, 40);
    assert_eq!(pos(&a, VILLAGER), SimPos::new(1200, 0));
    assert!(actions(&a).iter().any(|x| x.1 == handle && x.3 == "done"));
    assert_eq!(
        path(&a, VILLAGER),
        UNDER_CONTROL,
        "idle again, still controlled"
    );
}

#[test]
fn queued_actions_run_back_to_back_while_the_world_ticks() {
    let mut a = app(&plain());
    attach(&mut a, "Direct", "Idle");
    ticks(&mut a, 1);
    let h1 = act(&mut a, json!({"verb": "walk_to", "pos": [600, 0]}))["handle"].clone();
    let h2 = act(
        &mut a,
        json!({"verb": "wait", "args": {"ticks": 20}, "queue": true}),
    )["handle"]
        .clone();
    let h3 = act(
        &mut a,
        json!({"verb": "walk_to", "pos": [600, 600], "queue": true}),
    )["handle"]
        .clone();
    let obs = call(&mut a, "agent/observe", json!({"unit": VILLAGER}));
    assert_eq!(
        obs["queue"]["pending"].as_array().map(Vec::len),
        Some(0),
        "not applied yet"
    );
    ticks(&mut a, 1);
    let obs = call(
        &mut a,
        "agent/observe",
        json!({"unit": VILLAGER, "format": "text"}),
    );
    let text = obs["text"].as_str().unwrap();
    assert!(text.contains("Doing: #1"), "{text}");
    assert!(text.contains("Queued: #2 Wait 20 ticks; #3"), "{text}");
    ticks(&mut a, 80);
    assert_eq!(pos(&a, VILLAGER), SimPos::new(600, 600));
    let log = actions(&a);
    let at = |h: &Value, s: &str| {
        log.iter()
            .find(|x| u64::from(x.1) == h.as_u64().unwrap() && x.3 == s)
            .map(|x| x.0)
            .unwrap_or_else(|| panic!("{h} {s}: {log:?}"))
    };
    assert_eq!(
        at(&h1, "done"),
        at(&h2, "started"),
        "no gap between actions"
    );
    assert_eq!(at(&h2, "done"), at(&h3, "started"));
    assert_eq!(
        at(&h2, "done") - at(&h2, "started"),
        20,
        "the wait took 20 ticks"
    );
    assert!(at(&h3, "done") > at(&h2, "done"));
}

#[test]
fn cancel_and_clear() {
    let mut a = app(&plain());
    attach(&mut a, "Direct", "Idle");
    ticks(&mut a, 1);
    let h1 = act(&mut a, json!({"verb": "walk_to", "pos": [8000, 0]}))["handle"]
        .as_u64()
        .unwrap();
    let h2 = act(&mut a, json!({"verb": "wait", "queue": true}))["handle"]
        .as_u64()
        .unwrap();
    ticks(&mut a, 10);
    call(
        &mut a,
        "agent/cancel",
        json!({"unit": VILLAGER, "controller": "a", "handle": h2}),
    );
    call(
        &mut a,
        "agent/clear",
        json!({"unit": VILLAGER, "controller": "a"}),
    );
    ticks(&mut a, 1);
    let stopped_at = pos(&a, VILLAGER);
    assert_ne!(stopped_at, SimPos::new(0, 0), "it had started walking");
    ticks(&mut a, 20);
    assert_eq!(pos(&a, VILLAGER), stopped_at, "the walk was cancelled");
    let log = actions(&a);
    assert!(
        log.iter()
            .any(|x| u64::from(x.1) == h1 && x.3 == "cancelled"),
        "{log:?}"
    );
    assert!(
        log.iter()
            .any(|x| u64::from(x.1) == h2 && x.3 == "cancelled"),
        "{log:?}"
    );
    assert!(
        a.world()
            .get::<npc_framework::locomotion::MoveIntent>(entity(&a, VILLAGER))
            .is_none()
    );
}

#[test]
fn advisor_gives_orders_and_the_tree_resumes() {
    let mut a = app(&plain());
    attach(&mut a, "Advisor", "Idle");
    ticks(&mut a, 1);
    let r = act(&mut a, json!({"verb": "walk_to", "pos": [3000, 3000]}));
    assert_eq!(r["accepted"], true, "{r}");
    assert!(r["handle"].is_null(), "an order, not a queued action");
    let r = act(&mut a, json!({"verb": "eat"}));
    assert_eq!(
        (r["accepted"].as_bool(), r["reason"].as_str()),
        (Some(false), Some("direct_only"))
    );
    ticks(&mut a, 2);
    assert!(
        path(&a, VILLAGER).ends_with("FollowOrder > MoveToOrder"),
        "{}",
        path(&a, VILLAGER)
    );
    ticks(&mut a, 200);
    assert!(
        a.world()
            .get::<CurrentOrder>(entity(&a, VILLAGER))
            .unwrap()
            .0
            .is_none(),
        "the order was done and cleared"
    );
    assert!(
        !path(&a, VILLAGER).contains("FollowOrder"),
        "the tree resumed: {}",
        path(&a, VILLAGER)
    );
}

#[test]
fn observer_is_read_only() {
    let mut a = app(&plain());
    attach(&mut a, "Observer", "Idle");
    ticks(&mut a, 1);
    let obs = call(&mut a, "agent/observe", json!({"unit": VILLAGER}));
    assert_eq!(obs["controller"]["mode"], "Observer");
    assert!(
        obs["affordances"]
            .as_array()
            .unwrap()
            .iter()
            .all(|x| x["ok"] == false),
        "nothing is available to an observer"
    );
    let r = act(&mut a, json!({"verb": "wait"}));
    assert_eq!(r["reason"], "read_only");
    let rejected = a.world().resource::<Journal>().entries.iter().any(
        |e| matches!(&e.kind, JournalKind::ActionRejected { reason, .. } if reason == "read_only"),
    );
    assert!(rejected, "rejections are journaled");
    // The tree still runs.
    ticks(&mut a, 200);
    assert_ne!(pos(&a, VILLAGER), SimPos::new(0, 0));
}

#[test]
fn stale_actions_are_rejected_unless_allowed() {
    let mut a = app(&plain());
    attach(&mut a, "Direct", "Idle");
    ticks(&mut a, 1);
    let obs = call(
        &mut a,
        "agent/observe",
        json!({"unit": VILLAGER, "controller": "a"}),
    );
    let (t0, s0) = (obs["tick"].as_u64().unwrap(), obs["seq"].as_u64().unwrap());
    // The default bound is `api::DEFAULT_STALE_TICKS` (600).
    ticks(&mut a, 650);
    let r = act(&mut a, json!({"verb": "wait", "expect_tick": t0}));
    assert_eq!(r["reason"], "stale", "{r}");
    let r = act(
        &mut a,
        json!({"verb": "wait", "args": {"ticks": 5}, "expect_tick": t0, "allow_stale": true}),
    );
    assert_eq!(r["accepted"], true, "{r}");
    ticks(&mut a, 10);
    // The wait finished after seq s0: the unit's situation changed.
    let r = act(&mut a, json!({"verb": "wait", "expect_seq": s0}));
    assert_eq!(r["reason"], "stale", "{r}");
    let fresh = call(&mut a, "agent/observe", json!({"unit": VILLAGER}))["seq"].clone();
    let now = tick(&a);
    let r = act(
        &mut a,
        json!({"verb": "wait", "expect_seq": fresh, "expect_tick": now}),
    );
    assert_eq!(r["accepted"], true, "{r}");
    let stats = call(&mut a, "agent/stats", json!({"controller": "a"}));
    let c = &stats["controllers"][0];
    assert_eq!(
        (c["rejected"].as_u64(), c["stale"].as_u64()),
        (Some(2), Some(2)),
        "{stats}"
    );
    assert!(c["latency_ticks"]["n"].as_u64().unwrap() >= 3, "{stats}");
}

#[test]
fn idle_policy_idle_vs_autopilot() {
    // Autopilot: the tree runs while the queue is empty, the queue wins when
    // it has work, then the tree takes over again.
    let mut a = app(&plain());
    attach(&mut a, "Direct", "Autopilot");
    ticks(&mut a, 120);
    assert!(
        !path(&a, VILLAGER).starts_with(&UNDER_CONTROL[..30]),
        "{}",
        path(&a, VILLAGER)
    );
    let moved = pos(&a, VILLAGER);
    assert_ne!(
        moved,
        SimPos::new(0, 0),
        "autopilot runs the villager's own tree"
    );
    let h = act(&mut a, json!({"verb": "wait", "args": {"ticks": 30}}))["handle"]
        .as_u64()
        .unwrap();
    ticks(&mut a, 2);
    assert_eq!(
        path(&a, VILLAGER),
        UNDER_CONTROL,
        "queued work interrupts the tree"
    );
    let here = pos(&a, VILLAGER);
    ticks(&mut a, 20);
    assert_eq!(pos(&a, VILLAGER), here, "waiting");
    ticks(&mut a, 30);
    assert!(
        actions(&a)
            .iter()
            .any(|x| u64::from(x.1) == h && x.3 == "done")
    );
    assert!(
        !path(&a, VILLAGER).starts_with(&UNDER_CONTROL[..30]),
        "back on autopilot"
    );

    // Idle: nothing happens without actions.
    let mut b = app(&plain());
    attach(&mut b, "Direct", "Idle");
    ticks(&mut b, 120);
    assert_eq!(pos(&b, VILLAGER), SimPos::new(0, 0));
}

#[test]
fn eat_and_sleep_run_the_villager_actions() {
    let d = doc(json!({
        "version": 1, "content": "village",
        "actors": [{"kind": "villager", "pos": [1000, 0], "needs": {"Hunger": 20000, "Energy": 30000}}],
        "controllers": [{"actor": 0, "controller": "a"}]
    }));
    let loaded = scenario::load(&loaders(), &d, Some(1)).expect("loads");
    let mut a = loaded.app;
    let unit = loaded.actors[0].0;
    let ids = *a
        .world()
        .resource::<village_content::scenario::VillageIds>();
    assert_eq!(ids.villager.0, VILLAGER);
    assert!(unit > ids.villager.0, "actors spawn after the layout");
    let eat = format!("eat@{}", ids.stockpile.0);
    let r = call(
        &mut a,
        "agent/act",
        json!({"unit": unit, "controller": "a", "affordance": eat}),
    );
    assert_eq!(r["accepted"], true, "{r}");
    let r = call(
        &mut a,
        "agent/act",
        json!({"unit": unit, "controller": "a", "verb": "sleep", "queue": true}),
    );
    assert_eq!(r["accepted"], true, "{r}");
    ticks(&mut a, 2);
    // Villager 5 works the only field: `work` is offered with a reason.
    let obs = call(&mut a, "agent/observe", json!({"unit": unit}));
    let work = obs["affordances"]
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["verb"] == "work")
        .unwrap()
        .clone();
    assert_eq!(
        (work["ok"].as_bool(), work["why_not"].as_str()),
        (Some(false), Some("no free work site")),
        "{work}"
    );
    ticks(&mut a, 800);
    let log = actions(&a);
    assert!(log.iter().any(|x| x.2 == "eat" && x.3 == "done"), "{log:?}");
    assert!(
        log.iter().any(|x| x.2 == "sleep" && x.3 == "done"),
        "{log:?}"
    );
    let needs = a.world().get::<Needs>(entity(&a, unit)).unwrap().clone();
    let reg = a.world().resource::<NeedRegistry>();
    let (hunger, energy) = (reg.id("Hunger").unwrap(), reg.id("Energy").unwrap());
    assert!(needs.get(hunger).value > 20_000, "ate: {needs:?}");
    assert!(
        needs.get(energy).value > 90_000,
        "slept until full: {needs:?}"
    );
    let held = a.world().resource::<Reservations>().held_by(SimId(unit));
    assert!(held.is_empty(), "no claim outlives its action: {held:?}");
}

#[test]
fn a_recorded_agent_session_replays_bit_for_bit_without_the_agent() {
    let d = doc(json!({
        "version": 1, "content": "village", "seed": 7,
        "controllers": [{"unit": VILLAGER, "controller": "a", "on_idle": "Autopilot"}]
    }));
    let l = loaders();
    let mut a = scenario::load(&l, &d, None).expect("loads").app;
    let end = 1500;
    let mut rec = Recorder::aligned(100, end);
    while tick(&a) < end {
        // The "agent" acts at irregular, real-time-like moments.
        match tick(&a) {
            50 => {
                act(&mut a, json!({"verb": "walk_to", "pos": [-3000, 2000]}));
            }
            51 => {
                act(
                    &mut a,
                    json!({"verb": "wait", "args": {"ticks": 40}, "queue": true}),
                );
            }
            400 => {
                act(&mut a, json!({"affordance": "work@4"}));
            }
            430 => {
                call(
                    &mut a,
                    "agent/cancel",
                    json!({"unit": VILLAGER, "controller": "a"}),
                );
            }
            900 => {
                call(
                    &mut a,
                    "agent/detach",
                    json!({"unit": VILLAGER, "controller": "a"}),
                );
            }
            1000 => {
                attach(&mut a, "Direct", "Idle");
            }
            _ => {}
        }
        ticks(&mut a, 1);
        rec.after_tick(a.world_mut());
    }
    let replay = rec.finish(a.world_mut()).expect("replay");
    assert!(replay.commands.len() >= 6, "{:?}", replay.commands);
    let record = SessionRecord {
        version: scenario::SESSION_VERSION,
        scenario: d.clone(),
        seed: 7,
        replay,
    };
    let report = scenario::verify_session(&l, &record);
    assert!(report.ok && report.checked == 15, "{report:?}");
    // Through JSON, as `npcctl serve --record` writes it.
    let back: SessionRecord =
        serde_json::from_str(&serde_json::to_string(&record).unwrap()).unwrap();
    assert!(scenario::verify_session(&l, &back).ok);
    // A tampered action does not verify.
    let mut bad = record.clone();
    let i = bad
        .replay
        .commands
        .iter()
        .position(|c| c.2.kind() == "AgentAct")
        .unwrap();
    bad.replay.commands.remove(i);
    assert!(!scenario::verify_session(&l, &bad).ok);
}

#[test]
fn greedy_controller_reaches_its_goal() {
    let d = doc(json!({
        "version": 1, "content": "village",
        "actors": [{"kind": "villager", "pos": [0, 2000], "needs": {"Hunger": 25000}}],
        "controllers": [{"actor": 0, "controller": "greedy"}]
    }));
    let loaded = scenario::load(&loaders(), &d, Some(3)).expect("loads");
    let mut a = loaded.app;
    let unit = loaded.actors[0].0;
    let g = Greedy {
        unit,
        controller: "greedy".into(),
        rules: vec![NeedRule {
            need: "Hunger".into(),
            below: 40_000,
            verb: "eat".into(),
        }],
        goal: Some((-4000, 4000)),
        tolerance_mm: 50,
    };
    let mut acts = 0;
    let done = loop {
        let obs = call(
            &mut a,
            "agent/observe",
            json!({"unit": unit, "controller": "greedy"}),
        );
        match decide(&g, &obs) {
            Decision::Done => break true,
            Decision::Act(p) => {
                let r = call(&mut a, "agent/act", p);
                assert_eq!(r["accepted"], true, "{r}");
                acts += 1;
            }
            Decision::Wait => {}
        }
        if tick(&a) > 4000 {
            break false;
        }
        // The agent "thinks" for 7 ticks while the world runs.
        ticks(&mut a, 7);
    };
    assert!(done, "goal reached by tick {}", tick(&a));
    assert_eq!(pos(&a, unit), SimPos::new(-4000, 4000));
    assert_eq!(acts, 2, "eat first (hungry), then walk");
    let log = actions(&a);
    let eat = log.iter().position(|x| x.2 == "eat" && x.3 == "done");
    let walk = log.iter().position(|x| x.2 == "walk_to" && x.3 == "done");
    assert!(eat.is_some() && walk.is_some() && eat < walk, "{log:?}");
}

/// Runs a session on a free port in a thread; returns its client and the
/// thread (stopped by `session/stop`).
fn session(d: ScenarioDoc, tick_hz: u32) -> (RpcClient, std::thread::JoinHandle<()>) {
    let (tx, rx) = mpsc::channel();
    let handle = std::thread::spawn(move || {
        let options = SessionOptions {
            port: 0,
            tick_hz,
            ..SessionOptions::default()
        };
        let s = Session::start(loaders(), d, options).expect("session starts");
        tx.send(s.url()).unwrap();
        s.run().expect("session runs");
    });
    let url = rx
        .recv_timeout(Duration::from_secs(60))
        .expect("session url");
    (RpcClient::new(&url).unwrap(), handle)
}

#[test]
fn session_events_long_poll_returns_on_action_done_and_times_out() {
    let (c, h) = session(plain(), 50);
    c.call("agent/attach", json!({"unit": VILLAGER, "controller": "a"}))
        .unwrap();
    // Timeout path: no events of this kind can happen.
    let t = wall_now();
    let r = c
        .call(
            "agent/events",
            json!({"unit": VILLAGER, "kinds": ["PathFailed"], "timeout_ms": 300}),
        )
        .unwrap();
    let waited = t.elapsed();
    assert_eq!(r["timed_out"], true, "{r}");
    assert!(
        waited >= Duration::from_millis(300) && waited < Duration::from_secs(5),
        "{waited:?}"
    );
    // Done path: a 10-tick wait at 50 Hz ends ~200 ms later.
    let seq = c.call("agent/observe", json!({"unit": VILLAGER})).unwrap()["seq"]
        .as_u64()
        .unwrap();
    let r = c
        .call(
            "agent/act",
            json!({"unit": VILLAGER, "controller": "a", "verb": "wait", "args": {"ticks": 10}}),
        )
        .unwrap();
    let handle = r["handle"].clone();
    let t = wall_now();
    let mut since = seq;
    let done = loop {
        let r = c
            .call(
                "agent/events",
                json!({"unit": VILLAGER, "since_seq": since, "kinds": "ExternalAction", "timeout_ms": 10000}),
            )
            .unwrap();
        assert_eq!(r["timed_out"], false, "{r}");
        since = r["next_seq"].as_u64().unwrap();
        let ended = r["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["event"]["handle"] == handle && e["event"]["status"] == "done");
        if ended {
            break t.elapsed();
        }
    };
    assert!(
        done < Duration::from_secs(5),
        "returned promptly after done ({done:?})"
    );
    let r = c.call("session/stop", json!({})).unwrap();
    assert_eq!(r["stopped"], true);
    h.join().unwrap();
}
