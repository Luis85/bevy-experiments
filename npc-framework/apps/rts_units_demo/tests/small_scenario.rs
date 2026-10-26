//! A small-scenario harness for the demo's content: a handful of units on
//! the open demo map (no walls, squads or scripted opening), run headless
//! tick by tick. Copy it when you add a unit type, a behaviour or an order
//! executor to `src/sim.rs` (`/add-unit-type`): spawn just the units the
//! behaviour needs, issue the order (or not), assert on `ActiveLeaf`, the
//! action component, `OrderQueue` or the journal.

use bevy::prelude::*;
use npc_framework::ai::{AiRegistry, Brain};
use npc_framework::kernel::journal::{Journal, set_journal};
use npc_framework::prelude::*;
use rts_units_demo::sim::{self, RtsIdling, RtsUnit, UnitKind};

/// The demo's sim (grid, framework with `command`, nav, its content)
/// without a driver: the test runs the ticks. The journal is on.
fn small_app(seed: u64) -> App {
    let mut app = App::new();
    sim::add_sim(&mut app, sim::npc_plugins(seed).without_driver());
    app.finish();
    app.cleanup();
    set_journal(app.world_mut(), true, false);
    app
}

/// One unit as `sim::spawn_world` spawns it (no squad).
fn spawn_unit(app: &mut App, kind: UnitKind, player: u32, pos: (i32, i32)) -> SimId {
    let world = app.world_mut();
    let tree = world
        .resource::<AiRegistry>()
        .tree_id(kind.tree())
        .expect("the unit's tree is registered");
    spawn_sim(
        world,
        (
            SimPos::new(pos.0, pos.1),
            RtsUnit {
                kind,
                player,
                squad: 0,
            },
            Brain { tree },
            Commandable {
                speed_mm_per_tick: kind.speed_mm_per_tick(),
                arrive_mm: 200,
            },
            Owner(PlayerId(player)),
            NavAgent,
        ),
    )
    .0
}

fn entity(app: &App, id: SimId) -> Entity {
    app.world()
        .resource::<SimIndex>()
        .get(id)
        .expect("unit exists")
}

/// Runs one tick at a time until `pred` holds; the ticks it took.
fn run_until(app: &mut App, max: u64, mut pred: impl FnMut(&App) -> bool) -> Option<u64> {
    (1..=max).find(|_| {
        run_ticks(app.world_mut(), 1);
        pred(app)
    })
}

/// The kinds of `unit`'s journal entries, in order.
fn journal_kinds(app: &App, unit: SimId) -> Vec<String> {
    app.world()
        .resource::<Journal>()
        .entries
        .iter()
        .filter(|e| e.unit == Some(unit))
        .map(|e| {
            let v = serde_json::to_value(&e.kind).expect("journal entry is JSON");
            v["type"].as_str().unwrap_or_default().to_owned()
        })
        .collect()
}

#[test]
fn an_idle_infantry_unit_scans() {
    let mut app = small_app(1);
    let unit = spawn_unit(&mut app, UnitKind::Infantry, 0, (0, 0));
    run_ticks(app.world_mut(), 3);
    let e = entity(&app, unit);
    assert!(
        app.world().get::<RtsIdling>(e).is_some(),
        "no order: the tree falls through to its idle leaf"
    );
}

#[test]
fn a_move_order_wins_over_idling_and_finishes() {
    let mut app = small_app(1);
    let unit = spawn_unit(&mut app, UnitKind::Infantry, 0, (0, 0));
    run_ticks(app.world_mut(), 2);
    let order = IssueOrder::new(PlayerId(0), [unit], UnitOrder::Move { pos: (6000, 0) });
    submit_order(app.world_mut(), &order).expect("valid order");
    let e = entity(&app, unit);
    let took = run_until(&mut app, 200, |app| {
        app.world()
            .get::<OrderQueue>(e)
            .is_some_and(|q| !q.has_order() && q.last_ok)
    })
    .expect("the move finishes");
    assert!(took > 10, "it really walked: {took} ticks");
    let pos = *app.world().get::<SimPos>(e).unwrap();
    assert!(pos.dist2(SimPos::new(6000, 0)) <= 200 * 200, "{pos:?}");
    let kinds = journal_kinds(&app, unit);
    for k in ["OrderIssued", "OrderStarted", "OrderDone"] {
        assert!(kinds.iter().any(|x| x == k), "{k} missing: {kinds:?}");
    }
}

#[test]
fn another_player_cannot_order_the_unit() {
    let mut app = small_app(1);
    let unit = spawn_unit(&mut app, UnitKind::Engineer, 1, (0, 0));
    let order = IssueOrder::new(PlayerId(0), [unit], UnitOrder::Stop);
    assert!(submit_order(app.world_mut(), &order).is_err());
}

#[test]
fn small_scenarios_are_deterministic() {
    let run = || {
        let mut app = small_app(7);
        let a = spawn_unit(&mut app, UnitKind::Infantry, 0, (0, 0));
        let b = spawn_unit(&mut app, UnitKind::Engineer, 0, (1500, 0));
        let order = IssueOrder::new(PlayerId(0), [a, b], UnitOrder::Move { pos: (-8000, 4000) });
        submit_order(app.world_mut(), &order).expect("valid order");
        run_ticks(app.world_mut(), 120);
        state_hash(app.world_mut())
    };
    assert_eq!(run(), run());
}
