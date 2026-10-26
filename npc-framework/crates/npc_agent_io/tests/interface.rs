//! The agent interface on a toy unit (no content), after the fresh-agent
//! play-test and the final tools review: atomic multi-step acts, wrongly
//! typed args, acts skipped at apply time, same-tick queue limits, epochs
//! and sequence gaps, event kind validation, relevant-only staleness and
//! `until_idle` polls.

use bevy_app::App;
use bevy_ecs::prelude::*;
use npc_agent_io::control::MAX_QUEUE;
use npc_agent_io::events::AgentEvents;
use npc_agent_io::{
    ActionPlan, ActionRequest, AgentCtx, AgentIoAppExt, AgentIoPlugin, Rejection, Step, api,
};
use npc_ai::{ActiveLeaf, AiRegistry, Brain, ParamValue, Status, StopReason};
use npc_kernel::journal::{self, JournalKind};
use npc_kernel::registry::KernelAppExt;
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
    let n = req.opt_int("n")?.unwrap_or(3) as i32;
    Ok(ActionPlan::Queue {
        label: format!("hop {n}"),
        steps: vec![Step::Action {
            action: "Hop".into(),
            param: ParamValue::Int(n),
        }],
        est_ticks: Some(n as u32),
    })
}

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
    api::call(
        app.world_mut(),
        "agent/attach",
        Some(json!({"unit": 1, "controller": "c"})),
    )
    .unwrap();
    run_ticks(app.world_mut(), 1);
    app
}

fn call(app: &mut App, m: &str, p: Value) -> Value {
    api::call(app.world_mut(), m, Some(p)).unwrap_or_else(|e| panic!("{m}: {e:?}"))
}

fn act(app: &mut App, p: Value) -> Value {
    let mut base = json!({"unit": 1, "controller": "c"});
    for (k, v) in p.as_object().unwrap() {
        base[k] = v.clone();
    }
    call(app, "agent/act", base)
}

fn statuses(app: &mut App) -> Vec<String> {
    call(
        app,
        "agent/events",
        json!({"unit": 1, "kinds": "ExternalAction", "limit": 1000}),
    )["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| {
            format!(
                "{}:{}",
                x["event"]["handle"],
                x["event"]["status"].as_str().unwrap()
            )
        })
        .collect()
}

#[test]
fn multi_step_acts_are_all_or_nothing() {
    let mut app = toy_app();
    let r = act(
        &mut app,
        json!({"steps": [{"verb": "hop", "args": {"n": 2}}, {"verb": "fly"}]}),
    );
    assert_eq!(r["accepted"], json!(false), "{r}");
    assert_eq!(r["reason"], json!("unknown_verb"));
    assert_eq!(r["step"], json!(1));
    run_ticks(app.world_mut(), 5);
    assert!(statuses(&mut app).is_empty(), "nothing was queued");
    let r = act(
        &mut app,
        json!({"steps": [{"verb": "hop", "args": {"n": 2}}, {"verb": "wait", "args": {"ticks": 2}}]}),
    );
    assert_eq!(r["accepted"], json!(true), "{r}");
    assert_eq!(r["handles"], json!([1, 2]));
    run_ticks(app.world_mut(), 10);
    assert_eq!(
        statuses(&mut app),
        [
            "1:queued",
            "2:queued",
            "1:started",
            "1:done",
            "2:started",
            "2:done"
        ]
    );
    // steps and verb together are a usage error.
    let e = api::call(
        app.world_mut(),
        "agent/act",
        Some(json!({"unit": 1, "controller": "c", "verb": "hop", "steps": [{"verb": "hop"}]})),
    )
    .unwrap_err();
    assert_eq!(e.code, api::codes::INVALID_PARAMS);
}

#[test]
fn wrongly_typed_args_are_invalid_args() {
    let mut app = toy_app();
    let r = act(&mut app, json!({"verb": "wait", "args": {"ticks": "50"}}));
    assert_eq!(r["reason"], json!("invalid_args"), "{r}");
    assert!(r["message"].as_str().unwrap().contains("integer"), "{r}");
    let r = act(&mut app, json!({"verb": "hop", "args": "fast"}));
    assert_eq!(r["reason"], json!("invalid_args"), "{r}");
    let r = act(&mut app, json!({"verb": "wait", "args": {"ticks": null}}));
    assert_eq!(r["accepted"], json!(true), "null means the default: {r}");
}

#[test]
fn an_act_skipped_at_apply_time_is_journaled_and_wakes_pollers() {
    let mut app = toy_app();
    let seq = call(&mut app, "agent/stats", json!({}))["seq"]
        .as_u64()
        .unwrap();
    // Detach first, then act in the same tick: both validate now, but the
    // act no longer does when it applies (no controller any more).
    call(
        &mut app,
        "agent/detach",
        json!({"unit": 1, "controller": "c"}),
    );
    let r = act(&mut app, json!({"verb": "hop"}));
    assert_eq!(r["accepted"], json!(true), "{r}");
    let handle = r["handle"].clone();
    let poll = api::parse_events(Some(json!({"unit": 1, "since_seq": seq,
        "kinds": "ExternalAction,ActionRejected"})))
    .unwrap();
    assert!(!api::events_ready(app.world_mut(), &poll));
    run_ticks(app.world_mut(), 1);
    assert!(
        api::events_ready(app.world_mut(), &poll),
        "the poll wakes up"
    );
    let ev = api::events_result(app.world_mut(), &poll, false).unwrap();
    let kinds: Vec<Value> = ev["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["event"].clone())
        .collect();
    assert!(
        kinds.iter().any(|e| e["type"] == json!("ExternalAction")
            && e["handle"] == handle
            && e["status"] == json!("rejected")),
        "{kinds:?}"
    );
    assert!(
        kinds.iter().any(|e| e["type"] == json!("ActionRejected")
            && e["reason"].as_str().unwrap().starts_with("skipped at tick")),
        "{kinds:?}"
    );
}

#[test]
fn acts_queued_in_the_same_tick_count_toward_the_queue_limit() {
    let mut app = toy_app();
    for i in 0..MAX_QUEUE {
        let r = act(&mut app, json!({"verb": "wait", "queue": true}));
        assert_eq!(r["accepted"], json!(true), "act {i}: {r}");
    }
    let r = act(&mut app, json!({"verb": "wait", "queue": true}));
    assert_eq!(r["accepted"], json!(false), "{r}");
    assert!(
        r["message"].as_str().unwrap().contains("queue is full"),
        "{r}"
    );
    run_ticks(app.world_mut(), 1);
    let skipped = statuses(&mut app)
        .iter()
        .filter(|s| s.ends_with(":rejected"))
        .count();
    assert_eq!(skipped, 0, "nothing accepted was dropped at apply time");
}

#[test]
fn responses_carry_an_epoch_and_sequence_gaps_are_reported() {
    let mut app = toy_app();
    let a = call(&mut app, "agent/stats", json!({}));
    let other = toy_app();
    let b = api::epoch(other.world());
    assert!(a["epoch"].as_u64().unwrap() > 0);
    assert_ne!(a["epoch"].as_u64().unwrap(), b, "a new world, a new epoch");
    // A since_seq beyond the latest seq (e.g. from before a reset) errors.
    let last = a["seq"].as_u64().unwrap();
    let e = api::call(
        app.world_mut(),
        "agent/events",
        Some(json!({"unit": 1, "since_seq": last + 5})),
    )
    .unwrap_err();
    assert!(e.message.contains("ahead of the latest seq"), "{e:?}");
    let p = api::parse_events(Some(json!({"unit": 1, "since_seq": last + 5}))).unwrap();
    assert!(api::events_ready(app.world_mut(), &p), "answered at once");
    // Evicted events are reported as dropped.
    app.world_mut().resource_mut::<AgentEvents>().capacity = 3;
    for _ in 0..6 {
        act(
            &mut app,
            json!({"verb": "wait", "args": {"ticks": 1}, "queue": true}),
        );
    }
    run_ticks(app.world_mut(), 10);
    let ev = call(
        &mut app,
        "agent/events",
        json!({"unit": 1, "since_seq": 0, "kinds": "all"}),
    );
    assert!(ev["dropped"].as_u64().unwrap() > 0, "{ev}");
    assert!(ev["oldest_seq"].as_u64().unwrap() > 1, "{ev}");
    assert_eq!(ev["events"].as_array().unwrap().len(), 3);
}

#[test]
fn unknown_event_kinds_are_rejected_with_the_valid_list() {
    let mut app = toy_app();
    let e = api::call(
        app.world_mut(),
        "agent/events",
        Some(json!({"unit": 1, "kinds": "OrderDone,OrderDon"})),
    )
    .unwrap_err();
    assert_eq!(e.code, api::codes::INVALID_PARAMS);
    assert!(e.message.contains("unknown event kind `OrderDon`"), "{e:?}");
    assert!(e.message.contains("OrderFailed"), "{e:?}");
    // Case-insensitive names are fine.
    call(
        &mut app,
        "agent/events",
        json!({"unit": 1, "kinds": "orderdone"}),
    );
}

#[test]
fn only_relevant_events_make_an_act_stale() {
    let mut app = toy_app();
    let obs = call(&mut app, "agent/observe", json!({"unit": 1}));
    let (tick, seq) = (obs["tick"].as_u64().unwrap(), obs["seq"].as_u64().unwrap());
    // Something the unit perceives that the act does not refer to.
    journal::record(app.world_mut(), Some(SimId(1)), || JournalKind::Perceived {
        target: SimId(99),
        via: "vision".into(),
    });
    let r = act(
        &mut app,
        json!({"verb": "hop", "expect_tick": tick, "expect_seq": seq}),
    );
    assert_eq!(r["accepted"], json!(true), "an unrelated percept: {r}");
    // A percept of the act's own target counts, and is listed.
    let r = act(
        &mut app,
        json!({"verb": "hop", "target": 99, "expect_tick": tick, "expect_seq": seq}),
    );
    assert_eq!(r["reason"], json!("stale"), "{r}");
    assert!(r["message"].as_str().unwrap().contains("Perceived"), "{r}");
    assert_eq!(r["interrupting"].as_array().unwrap().len(), 1, "{r}");
    // The default tick bound is generous (an LLM turn).
    assert_eq!(api::DEFAULT_STALE_TICKS, 600);
}

#[test]
fn until_idle_polls_wait_for_the_whole_queue() {
    let mut app = toy_app();
    act(&mut app, json!({"verb": "hop", "args": {"n": 4}}));
    act(
        &mut app,
        json!({"verb": "hop", "args": {"n": 4}, "queue": true}),
    );
    let p = api::parse_events(Some(json!({"unit": 1, "until_idle": true}))).unwrap();
    assert!(
        !api::events_ready(app.world_mut(), &p),
        "the acts wait to apply"
    );
    run_ticks(app.world_mut(), 5);
    assert!(
        !api::events_ready(app.world_mut(), &p),
        "the second hop is pending"
    );
    run_ticks(app.world_mut(), 6);
    assert!(api::events_ready(app.world_mut(), &p));
    let r = api::events_result(app.world_mut(), &p, false).unwrap();
    assert_eq!(r["idle"], json!(true), "{r}");
    assert!(
        api::parse_events(Some(json!({"until_idle": true}))).is_err(),
        "needs a unit"
    );
}

/// Task 25 (proposal 003): a position that is not integer millimetres is
/// refused with the unit and an example, wherever it is given (the act's
/// `pos`, a step's `pos`, `args`, `agent/act_and_wait`); integer positions
/// still work.
#[test]
fn fractional_pos_hint() {
    const HINT: &str = "positions are integer millimetres: 6.5 m is 6500";
    let mut app = toy_app();
    let err = |app: &mut App, m: &str, p: Value| {
        let mut base = json!({"unit": 1, "controller": "c"});
        for (k, v) in p.as_object().unwrap() {
            base[k] = v.clone();
        }
        api::call(app.world_mut(), m, Some(base)).unwrap_err()
    };
    let e = err(
        &mut app,
        "agent/act",
        json!({"verb": "hop", "pos": [-6.5, 6.0]}),
    );
    assert_eq!(e.code, api::codes::INVALID_PARAMS);
    assert!(e.message.contains(HINT), "{}", e.message);
    assert!(e.message.contains("[-6.5,6.0]"), "{}", e.message);
    let e = err(
        &mut app,
        "agent/act",
        json!({"steps": [{"verb": "hop", "pos": [1, 3_000_000_000_u64]}]}),
    );
    assert!(e.message.contains(HINT), "{}", e.message);
    let e = err(
        &mut app,
        "agent/act_and_wait",
        json!({"verb": "hop", "pos": [0.5, 2], "timeout_ms": 0}),
    );
    assert!(e.message.contains(HINT), "{}", e.message);
    // In args (an order's pos, drive_to's pos): rejected like other bad args.
    let r = act(
        &mut app,
        json!({"verb": "hop", "args": {"order": {"type": "Move", "pos": [-6.5, 6.0]}}}),
    );
    assert_eq!(r["accepted"], json!(false), "{r}");
    assert_eq!(r["reason"], json!("invalid_args"), "{r}");
    assert!(r["message"].as_str().unwrap().contains(HINT), "{r}");
    let r = act(&mut app, json!({"verb": "hop", "args": {"pos": [6.5, 1]}}));
    assert!(r["message"].as_str().unwrap().contains(HINT), "{r}");
    let r = act(
        &mut app,
        json!({"verb": "hop", "args": {"points": [[0, 0], [1.25, 0]]}}),
    );
    assert!(r["message"].as_str().unwrap().contains(HINT), "{r}");
    // Integer millimetres are fine.
    let r = act(&mut app, json!({"verb": "hop", "pos": [6500, -30]}));
    assert_eq!(r["accepted"], json!(true), "{r}");
}
