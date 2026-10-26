//! The control layer on a toy unit (no content): the injected branch, the
//! queue running registered actions by name, and handing control back.

use bevy_app::App;
use bevy_ecs::prelude::*;
use npc_agent_io::{
    ActionPlan, ActionRequest, AgentCtx, AgentIoAppExt, AgentIoPlugin, Rejection, Step, api,
};
use npc_ai::{ActiveLeaf, AiRegistry, Brain, ParamValue, Status, StopReason};
use npc_kernel::registry::KernelAppExt;
use npc_kernel::{NpcKernelPlugin, NpcSet, NpcSimTick, SimClock, run_ticks, spawn_sim};
use serde_json::{Value, json};

/// Counts the ticks a unit spent in each action.
#[derive(Component, Default, Debug)]
struct Counters {
    spins: u32,
    hops: u32,
}

#[derive(Component)]
#[component(storage = "SparseSet")]
struct Hopping(u32);

#[derive(Component)]
#[component(storage = "SparseSet")]
struct Spinning;

fn spin_start(world: &mut World, e: Entity, _: &ParamValue) -> Status {
    world.entity_mut(e).insert(Spinning);
    Status::Running
}
fn spin_stop(world: &mut World, e: Entity, _: StopReason) {
    world.entity_mut(e).remove::<Spinning>();
}
fn hop_start(world: &mut World, e: Entity, p: &ParamValue) -> Status {
    world
        .entity_mut(e)
        .insert(Hopping(p.as_int().unwrap_or(3) as u32));
    Status::Running
}
fn hop_stop(world: &mut World, e: Entity, _: StopReason) {
    world.entity_mut(e).remove::<Hopping>();
}

type Spinners<'w, 's> = Query<'w, 's, &'static mut Counters, (With<Spinning>, Without<Hopping>)>;

fn toy_system(
    mut spinners: Spinners,
    mut hoppers: Query<(&mut Counters, &mut Hopping, &mut ActiveLeaf), Without<Spinning>>,
) {
    for mut c in &mut spinners {
        c.spins += 1;
    }
    for (mut c, mut h, mut leaf) in &mut hoppers {
        c.hops += 1;
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

fn toy_app(with_agent_io: bool) -> (App, Entity) {
    let mut app = App::new();
    app.add_plugins(NpcKernelPlugin::default())
        .register_action("Spin", spin_start, spin_stop)
        .register_action("Hop", hop_start, hop_stop)
        .register_tree("toy", |b| {
            b.leaf("Spin", "Spin", ());
        })
        .add_systems(NpcSimTick, toy_system.in_set(NpcSet::Actions));
    if with_agent_io {
        app.add_plugins(AgentIoPlugin::default())
            .register_action_handler("hop", hop_handler);
    }
    npc_kernel::ensure_frozen(app.world_mut());
    let tree = app.world().resource::<AiRegistry>().tree_id("toy").unwrap();
    let world = app.world_mut();
    let (_, e) = spawn_sim(world, (Brain { tree }, Counters::default()));
    (app, e)
}

fn call(app: &mut App, m: &str, p: Value) -> Value {
    api::call(app.world_mut(), m, Some(p)).unwrap_or_else(|e| panic!("{m}: {e:?}"))
}

fn counters(app: &App, e: Entity) -> (u32, u32) {
    let c = app.world().get::<Counters>(e).unwrap();
    (c.spins, c.hops)
}

#[test]
fn without_the_plugin_trees_are_untouched() {
    let (app, _) = toy_app(false);
    let r = app.world().resource::<AiRegistry>();
    let tree = r.tree(r.tree_id("toy").unwrap());
    assert_eq!(tree.nodes.len(), 1);
    let (app2, _) = toy_app(true);
    let r2 = app2.world().resource::<AiRegistry>();
    assert_eq!(r2.tree(r2.tree_id("toy").unwrap()).nodes.len(), 4);
}

#[test]
fn direct_control_runs_queued_actions_then_hands_back() {
    let (mut app, e) = toy_app(true);
    run_ticks(app.world_mut(), 5);
    assert_eq!(counters(&app, e), (5, 0), "spinning on its own");
    call(
        &mut app,
        "agent/attach",
        json!({"unit": 1, "controller": "c"}),
    );
    run_ticks(app.world_mut(), 5);
    let spins = counters(&app, e).0;
    assert!(spins <= 6, "suspended: {spins}");
    let r = call(
        &mut app,
        "agent/act",
        json!({"unit": 1, "controller": "c", "verb": "hop", "args": {"n": 4}}),
    );
    assert_eq!(r["accepted"], true, "{r}");
    let r = call(
        &mut app,
        "agent/act",
        json!({"unit": 1, "controller": "c", "verb": "wait", "args": {"ticks": 3}, "queue": true}),
    );
    assert_eq!(r["accepted"], true, "{r}");
    run_ticks(app.world_mut(), 10);
    assert_eq!(counters(&app, e), (spins, 4), "hopped 4 ticks, then waited");
    let ev = call(
        &mut app,
        "agent/events",
        json!({"unit": 1, "kinds": ["ExternalAction"]}),
    );
    let statuses: Vec<String> = ev["events"]
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
        .collect();
    assert_eq!(
        statuses,
        [
            "1:queued",
            "2:queued",
            "1:started",
            "1:done",
            "2:started",
            "2:done"
        ]
    );
    call(
        &mut app,
        "agent/detach",
        json!({"unit": 1, "controller": "c"}),
    );
    run_ticks(app.world_mut(), 5);
    assert!(counters(&app, e).0 > spins, "spinning again");
    assert_eq!(app.world().resource::<SimClock>().tick, 25);
}

#[test]
fn unknown_verbs_and_units_are_reported() {
    let (mut app, _) = toy_app(true);
    call(
        &mut app,
        "agent/attach",
        json!({"unit": 1, "controller": "c"}),
    );
    run_ticks(app.world_mut(), 1);
    let r = call(
        &mut app,
        "agent/act",
        json!({"unit": 1, "controller": "c", "verb": "fly"}),
    );
    assert_eq!(r["reason"], "unknown_verb");
    let e = api::call(app.world_mut(), "agent/observe", Some(json!({"unit": 9}))).unwrap_err();
    assert_eq!(e.code, api::codes::UNKNOWN_UNIT);
    let e = api::call(
        app.world_mut(),
        "agent/attach",
        Some(json!({"unit": 1, "controller": "d"})),
    )
    .unwrap_err();
    assert!(e.message.contains("already controlled by `c`"), "{e:?}");
    let e = api::call(app.world_mut(), "agent/nope", None).unwrap_err();
    assert_eq!(e.code, api::codes::METHOD_NOT_FOUND);
}
