//! FX4: game extension points of `npc_command`: custom order kinds by name,
//! per-verb `Interact` executors, `skip_invalid`, and the approach engine.

use bevy_app::App;
use bevy_ecs::prelude::*;
use npc_command::{
    ApproachSpec, ApproachStep, CommandAppExt, CommandConfig, CommandPlugin, Commandable,
    IssueOrder, OrderExecutor, OrderQueue, OrderStep, Owner, PlayerId, Reach, UnitOrder,
    approach_step, approach_then, submit_order_report,
};
use npc_kernel::hash::state_hash;
use npc_kernel::journal::{JournalEntry, JournalKind};
use npc_kernel::json::JsonObject;
use npc_kernel::registry::ContentHash;
use npc_kernel::spatial::{Footprint, GridBlocker, GridBounds, SpatialGrid, SpatialPlugin};
use npc_kernel::{
    Journal, JournalPlugin, NpcKernelPlugin, SimCommand, SimId, SimPos, run_ticks, spawn_sim,
    submit,
};
use npc_locomotion::{MoveIntent, StraightLineMover};
use serde::{Deserialize, Serialize};

const P1: PlayerId = PlayerId(1);
const P2: PlayerId = PlayerId(2);

fn build(setup: impl FnOnce(&mut App)) -> App {
    let mut app = App::new();
    app.insert_resource(SpatialGrid::new(1000, GridBounds::centered(64, 64)));
    app.add_plugins((
        NpcKernelPlugin::default(),
        SpatialPlugin,
        StraightLineMover,
        CommandPlugin,
        JournalPlugin::default(),
    ));
    setup(&mut app);
    app.finish();
    app.cleanup();
    app
}

fn unit(app: &mut App, player: PlayerId, at: SimPos) -> (SimId, Entity) {
    spawn_sim(app.world_mut(), (at, Commandable::new(100), Owner(player)))
}

fn journal(app: &App) -> Vec<JournalEntry> {
    app.world()
        .resource::<Journal>()
        .entries
        .iter()
        .cloned()
        .collect()
}

fn order_log(app: &App, id: SimId) -> Vec<String> {
    journal(app)
        .into_iter()
        .filter(|e| e.unit == Some(id))
        .filter_map(|e| match e.kind {
            JournalKind::OrderIssued { order, .. } => Some(format!("issued {order}")),
            JournalKind::OrderStarted { order } => Some(format!("started {order}")),
            JournalKind::OrderDone { order } => Some(format!("done {order}")),
            JournalKind::OrderFailed { order, reason, .. } => {
                Some(format!("failed {order} ({reason})"))
            }
            _ => None,
        })
        .collect()
}

fn manifest(app: &App) -> Vec<(String, String)> {
    app.world().resource::<ContentHash>().manifest.clone()
}

// --- a custom order: "dig" for `ticks` ticks -------------------------------

#[derive(Serialize, Deserialize)]
struct DigArgs {
    ticks: u32,
}

#[derive(Component)]
struct Digging(u32);

#[derive(Component)]
struct CanDig;

fn dig_validate(world: &World, unit: Entity, order: &UnitOrder) -> Result<(), String> {
    if world.get::<CanDig>(unit).is_none() {
        return Err("cannot dig".into());
    }
    let (_, args) = order.custom().ok_or("not custom")?;
    args.to_typed::<DigArgs>().map(|_| ())
}

fn dig_start(world: &mut World, unit: Entity, order: &UnitOrder) -> OrderStep {
    let args: DigArgs = order.custom().unwrap().1.to_typed().unwrap();
    world.entity_mut(unit).insert(Digging(args.ticks));
    OrderStep::Running
}

fn dig_tick(world: &mut World, unit: Entity, _: &UnitOrder) -> OrderStep {
    let mut d = world.get_mut::<Digging>(unit).unwrap();
    d.0 = d.0.saturating_sub(1);
    if d.0 == 0 {
        OrderStep::Done
    } else {
        OrderStep::Running
    }
}

fn dig_cancel(world: &mut World, unit: Entity, _: &UnitOrder) {
    world.entity_mut(unit).remove::<Digging>();
}

fn dig_executor() -> OrderExecutor {
    OrderExecutor::new(dig_start, dig_tick)
        .with_validate(dig_validate)
        .with_cancel(dig_cancel)
}

fn dig(ticks: u32) -> UnitOrder {
    UnitOrder::custom_with("dig", &DigArgs { ticks }).unwrap()
}

#[test]
fn custom_orders_run_by_name_and_round_trip_as_json() {
    let order = dig(3);
    let json = serde_json::to_string(&order).unwrap();
    assert_eq!(json, r#"{"type":"Custom","name":"dig","args":{"ticks":3}}"#);
    assert_eq!(serde_json::from_str::<UnitOrder>(&json).unwrap(), order);
    let bare: UnitOrder = serde_json::from_str(r#"{"type":"Custom","name":"x"}"#).unwrap();
    assert_eq!(bare.custom(), Some(("x", &JsonObject::default())));
    assert_eq!(order.kind(), "Custom");
    assert_eq!(order.journal_name(), "dig");

    // Without an executor: rejected, naming the known custom orders.
    let mut plain = build(|_| {});
    let (a, _) = unit(&mut plain, P1, SimPos::new(0, 0));
    let err = submit(
        plain.world_mut(),
        None,
        IssueOrder::new(P1, [a], dig(3)).to_command(),
    )
    .unwrap_err();
    assert!(format!("{err}").contains("custom order `dig`"), "{err}");

    let mut app = build(|app| {
        app.register_custom_order("dig", dig_executor());
    });
    let (a, e) = unit(&mut app, P1, SimPos::new(0, 0));
    app.world_mut().entity_mut(e).insert(CanDig);
    submit(
        app.world_mut(),
        None,
        IssueOrder::new(P1, [a], dig(3)).to_command(),
    )
    .unwrap();
    run_ticks(app.world_mut(), 4);
    assert!(app.world().get::<OrderQueue>(e).unwrap().is_empty());
    assert!(
        app.world().get::<Digging>(e).is_none(),
        "cancel cleans up after Done"
    );
    assert_eq!(
        order_log(&app, a),
        ["issued dig", "started dig", "done dig"],
        "the journal names the custom order"
    );
    assert!(manifest(&app).contains(&("order_custom".into(), "dig".into())));
}

#[test]
#[should_panic(expected = "shadow a built-in order kind")]
fn custom_orders_cannot_shadow_built_in_kinds() {
    let mut app = App::new();
    app.add_plugins((NpcKernelPlugin::default(), CommandPlugin));
    app.register_custom_order("Move", dig_executor());
}

#[test]
fn custom_args_change_the_state_hash() {
    let run = |ticks: u32| {
        let mut app = build(|app| {
            app.register_custom_order("dig", dig_executor());
        });
        let (a, e) = unit(&mut app, P1, SimPos::new(0, 0));
        app.world_mut().entity_mut(e).insert(CanDig);
        submit(
            app.world_mut(),
            None,
            IssueOrder::new(P1, [a], dig(ticks)).to_command(),
        )
        .unwrap();
        run_ticks(app.world_mut(), 1);
        state_hash(app.world_mut())
    };
    assert_eq!(run(5), run(5));
    assert_ne!(run(5), run(6), "args are hashed");
}

// --- per-verb Interact executors -------------------------------------------

fn generic_start(_: &mut World, _: Entity, _: &UnitOrder) -> OrderStep {
    OrderStep::Failed("generic".into())
}

fn repair_start(_: &mut World, _: Entity, _: &UnitOrder) -> OrderStep {
    OrderStep::Done
}

fn repair_validate(world: &World, unit: Entity, _: &UnitOrder) -> Result<(), String> {
    world
        .get::<CanDig>(unit)
        .map(|_| ())
        .ok_or_else(|| "only engineers repair".into())
}

#[test]
fn verb_executors_win_over_the_generic_interact_executor() {
    let mut app = build(|app| {
        app.register_order_executor(
            "Interact",
            "generic",
            OrderExecutor::new(generic_start, generic_start),
        )
        .register_interact_verb_executor(
            "repair",
            "test.repair",
            OrderExecutor::new(repair_start, repair_start).with_validate(repair_validate),
        );
    });
    let (a, ea) = unit(&mut app, P1, SimPos::new(0, 0));
    app.world_mut().entity_mut(ea).insert(CanDig);
    let (wall, _) = spawn_sim(app.world_mut(), SimPos::new(5000, 0));
    let interact = |verb: &str| UnitOrder::Interact {
        verb: verb.into(),
        target: wall,
    };
    submit(
        app.world_mut(),
        None,
        IssueOrder::new(P1, [a], interact("repair")).to_command(),
    )
    .unwrap();
    run_ticks(app.world_mut(), 1);
    submit(
        app.world_mut(),
        None,
        IssueOrder::new(P1, [a], interact("open")).to_command(),
    )
    .unwrap();
    run_ticks(app.world_mut(), 1);
    assert_eq!(
        order_log(&app, a),
        [
            "issued Interact",
            "started Interact",
            "done Interact",
            "issued Interact",
            "started Interact",
            "failed Interact (generic)",
        ]
    );
    let m = manifest(&app);
    assert!(m.contains(&("order_verb".into(), "repair=test.repair".into())));
    assert!(m.contains(&("order_executor".into(), "Interact=generic".into())));
}

// --- skip_invalid ------------------------------------------------------------

#[test]
fn skip_invalid_orders_the_units_that_can_take_it() {
    let mut app = build(|app| {
        app.register_custom_order("dig", dig_executor());
    });
    let (a, ea) = unit(&mut app, P1, SimPos::new(0, 0));
    let (b, _) = unit(&mut app, P1, SimPos::new(1000, 0));
    let (c, ec) = unit(&mut app, P1, SimPos::new(2000, 0));
    let (enemy, _) = unit(&mut app, P2, SimPos::new(3000, 0));
    for e in [ea, ec] {
        app.world_mut().entity_mut(e).insert(CanDig);
    }
    // All-or-nothing by default.
    let strict = IssueOrder::new(P1, [a, b, c, enemy], dig(2));
    assert!(submit(app.world_mut(), None, strict.to_command()).is_err());
    assert!(
        !serde_json::to_string(&strict.to_command())
            .unwrap()
            .contains("skip_invalid"),
        "the default is not serialized (recorded commands stay identical)"
    );

    let cmd = strict.clone().skip_invalid(true);
    let receipt = submit_order_report(app.world_mut(), &cmd).unwrap();
    assert_eq!(receipt.units, [a, c]);
    let skipped: Vec<(SimId, &str)> = receipt
        .skipped
        .iter()
        .map(|s| (s.unit, s.reason.as_str()))
        .collect();
    assert_eq!(skipped.len(), 2, "{skipped:?}");
    assert_eq!(skipped[0], (b, "cannot dig"));
    assert_eq!(skipped[1].0, enemy);
    assert!(skipped[1].1.contains("owned by player 2"), "{skipped:?}");
    let text = serde_json::to_string(&cmd.to_command()).unwrap();
    assert!(text.ends_with(r#""skip_invalid":true}"#), "{text}");
    let back: SimCommand = serde_json::from_str(&text).unwrap();
    assert_eq!(back, cmd.to_command());

    run_ticks(app.world_mut(), 1);
    assert_eq!(order_log(&app, a), ["issued dig", "started dig"]);
    assert_eq!(
        order_log(&app, b),
        ["failed dig (skipped: cannot dig)"],
        "skipped units are journaled"
    );
    assert!(order_log(&app, enemy)[0].starts_with("failed dig (skipped: unit "));

    // Nobody can take it: still rejected.
    let none = IssueOrder::new(P1, [b], dig(2)).skip_invalid(true);
    let err = submit(app.world_mut(), None, none.to_command()).unwrap_err();
    assert_eq!(format!("{err}"), "cannot dig");
}

// --- the approach engine ----------------------------------------------------

#[test]
fn approach_walks_into_reach_then_acts() {
    let mut app = build(|_| {});
    let (_, e) = unit(&mut app, P1, SimPos::new(0, 0));
    let spec = ApproachSpec::to_point(SimPos::new(3000, 0), 1000);
    assert_eq!(
        approach_step(app.world_mut(), e, &spec),
        ApproachStep::Walking
    );
    let intent = *app.world().get::<MoveIntent>(e).unwrap();
    assert_eq!(intent.goal, SimPos::new(3000, 0));
    assert_eq!((intent.speed_mm_per_tick, intent.tolerance_mm), (100, 750));
    let mut ticks = 0;
    loop {
        run_ticks(app.world_mut(), 1);
        ticks += 1;
        let step = approach_then(app.world_mut(), e, &spec, |_, _| OrderStep::Done);
        if step == OrderStep::Done {
            break;
        }
        assert_eq!(step, OrderStep::Running);
        assert!(ticks < 40);
    }
    assert!(app.world().get::<SimPos>(e).unwrap().x >= 2000);
    assert!(
        app.world().get::<MoveIntent>(e).is_none(),
        "the walk stops in reach"
    );
}

#[test]
fn approach_to_a_footprint_stands_next_to_it() {
    let mut app = build(|_| {});
    // A wall across x = 10..=11 m, z = -5..=4 m.
    let wall_pos = SimPos::new(11_000, 0);
    let fp = Footprint {
        half_x_mm: 500,
        half_z_mm: 4500,
    };
    spawn_sim(app.world_mut(), (wall_pos, GridBlocker(fp)));
    let (_, e) = unit(&mut app, P1, SimPos::new(0, 0));
    run_ticks(app.world_mut(), 1); // the grid sees the blocker
    let grid = app.world().resource::<SpatialGrid>();
    let spec =
        ApproachSpec::to_footprint(grid, SimPos::new(0, 0), wall_pos, fp, 600, 200, 4).unwrap();
    assert_eq!(
        spec.goal,
        SimPos::new(9500, -500),
        "the free cell nearest the unit (ties: lowest cell)"
    );
    assert!(matches!(spec.reach, Reach::Area { mm: 600, .. }));
    let mut done = false;
    for _ in 0..200 {
        if approach_step(app.world_mut(), e, &spec) == ApproachStep::InReach {
            done = true;
            break;
        }
        run_ticks(app.world_mut(), 1);
    }
    assert!(done, "reached the wall's edge");
    let at = *app.world().get::<SimPos>(e).unwrap();
    assert!(at.x >= 9200 && at.x < 10_000, "next to the wall: {at:?}");
    assert!(spec.in_reach(at));
}

#[test]
fn a_blocked_approach_fails_after_the_configured_ticks() {
    let mut app = build(|app| {
        app.insert_resource(CommandConfig {
            block_fail_ticks: 3,
            ..CommandConfig::default()
        });
    });
    let (_, e) = unit(&mut app, P1, SimPos::new(0, 0));
    let spec = ApproachSpec::to_point(SimPos::new(5000, 0), 100);
    assert_eq!(
        approach_step(app.world_mut(), e, &spec),
        ApproachStep::Walking
    );
    app.world_mut()
        .entity_mut(e)
        .insert(npc_locomotion::MoveStatus::Blocked { ticks: 3 });
    assert_eq!(
        approach_step(app.world_mut(), e, &spec),
        ApproachStep::Blocked
    );
    assert!(app.world().get::<MoveIntent>(e).is_none());
}
