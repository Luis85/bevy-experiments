//! Engineers repair damaged wall blocks: autonomously (the nearest damaged
//! block within `REPAIR_SCAN_MM`, when no order is pending) and on an RTS
//! order (`UnitOrder::Interact { verb: "repair", target }`, the recorded
//! `IssueOrder`). Integer, gradual, deterministic.

use bevy::prelude::*;
use npc_framework::kernel::journal::{JournalKind, set_journal};
use npc_framework::prelude::*;
use rts_units_demo::sim::{
    self, REPAIR_HP_PER_TICK, REPAIR_VERB, UnitKind, WALL_MAX_HP, WallHealth,
};

/// A wall 2 x 4 cells east of the origin: cells x 5..=6, z 0..=3.
const WALL: sim::CellRect = ((5, 0), (6, 3));

fn hp(world: &mut World, wall: SimId) -> u32 {
    let e = world.resource::<SimIndex>().get(wall).expect("wall exists");
    world.get::<WallHealth>(e).expect("wall has health").hp
}

fn repair_order(player: u32, unit: SimId, wall: SimId) -> IssueOrder {
    IssueOrder::new(
        PlayerId(player),
        [unit],
        UnitOrder::Interact {
            verb: REPAIR_VERB.to_owned(),
            target: wall,
        },
    )
}

fn leaf_starts(world: &World, unit: SimId, action: &str) -> usize {
    world
        .resource::<Journal>()
        .entries
        .iter()
        .filter(|e| {
            e.unit == Some(unit)
                && matches!(&e.kind, JournalKind::LeafStart { action: a, .. } if a == action)
        })
        .count()
}

/// One damaged wall at `hp`, one engineer at `engineer` (mm).
fn scenario(seed: u64, hp: u32, engineer: (i32, i32), journal: bool) -> (App, SimId, SimId) {
    let mut app = sim::sim_app(seed);
    let world = app.world_mut();
    if journal {
        set_journal(world, true, false);
    }
    let wall = sim::spawn_wall(world, WALL, hp);
    let eng = sim::spawn_unit(
        world,
        UnitKind::Engineer,
        0,
        0,
        SimPos::new(engineer.0, engineer.1),
    );
    (app, wall, eng)
}

#[test]
fn idle_engineer_repairs_the_nearest_damaged_wall_over_time() {
    let (mut app, wall, eng) = scenario(1, 200, (-2_000, 1_500), true);
    let world = app.world_mut();
    let mut seen = vec![hp(world, wall)];
    for _ in 0..600 {
        run_ticks(world, 1);
        seen.push(hp(world, wall));
    }
    assert_eq!(*seen.last().unwrap(), WALL_MAX_HP, "repaired to full");
    assert!(seen.windows(2).all(|w| w[0] <= w[1]), "never loses hp");
    assert!(
        seen.windows(2).all(|w| w[1] - w[0] <= REPAIR_HP_PER_TICK),
        "gradual: at most {REPAIR_HP_PER_TICK} hp per tick for one engineer"
    );
    // It walked first: the first ticks restore nothing.
    assert_eq!(seen[10], 200, "repair starts only once in reach");
    assert!(
        leaf_starts(world, eng, sim::RTS_REPAIR) >= 1,
        "RtsRepair leaf ran"
    );
    let effects = world
        .resource::<Journal>()
        .entries
        .iter()
        .filter(|e| {
            matches!(&e.kind, JournalKind::Effect { effect, .. }
                if serde_json::to_value(effect).unwrap()["type"] == "WallRepaired")
        })
        .count();
    assert_eq!(effects, 1, "one WallRepaired effect");
    // Done: the engineer goes back to tinkering, the wall stays full.
    run_ticks(world, 200);
    assert_eq!(hp(world, wall), WALL_MAX_HP);
}

#[test]
fn engineer_ignores_damage_out_of_range_and_full_walls() {
    let (mut app, wall, _) = scenario(1, 200, (-30_000, 0), false);
    run_ticks(app.world_mut(), 400);
    assert_eq!(hp(app.world_mut(), wall), 200, "30 m away: not noticed");

    let (mut app, wall, eng) = scenario(1, WALL_MAX_HP, (-2_000, 1_500), true);
    run_ticks(app.world_mut(), 200);
    assert_eq!(hp(app.world_mut(), wall), WALL_MAX_HP);
    assert_eq!(leaf_starts(app.world(), eng, sim::RTS_REPAIR), 0);
}

#[test]
fn infantry_does_not_repair() {
    let mut app = sim::sim_app(1);
    let world = app.world_mut();
    let wall = sim::spawn_wall(world, WALL, 200);
    let inf = sim::spawn_unit(world, UnitKind::Infantry, 0, 0, SimPos::new(-2_000, 1_500));
    run_ticks(world, 300);
    assert_eq!(hp(world, wall), 200);
    let err = submit_order(world, &repair_order(0, inf, wall));
    assert!(err.is_err(), "only engineers take repair orders: {err:?}");
}

#[test]
fn repair_order_sends_an_engineer_across_the_map() {
    let (mut app, wall, eng) = scenario(1, 300, (-30_000, 0), true);
    let world = app.world_mut();
    submit_order(world, &repair_order(0, eng, wall)).expect("a valid repair order");
    for _ in 0..1500 {
        run_ticks(world, 1);
        let e = world.resource::<SimIndex>().get(eng).unwrap();
        if world.get::<OrderQueue>(e).is_some_and(|q| !q.has_order()) {
            break;
        }
    }
    assert_eq!(hp(world, wall), WALL_MAX_HP, "the order repaired it");
    let e = world.resource::<SimIndex>().get(eng).unwrap();
    let q = world.get::<OrderQueue>(e).unwrap();
    assert!(!q.has_order() && q.last_ok, "OrderDone: {q:?}");
    let done = world.resource::<Journal>().entries.iter().any(|e| {
        e.unit == Some(eng)
            && matches!(&e.kind, JournalKind::OrderDone { order } if order == "Interact")
    });
    assert!(done, "journal OrderDone for the Interact order");
}

#[test]
fn bad_repair_orders_are_rejected() {
    let (mut app, wall, eng) = scenario(1, 300, (-30_000, 0), false);
    let world = app.world_mut();
    // Not a wall (the engineer's own id is "cannot target itself"; use a 2nd unit).
    let other = sim::spawn_unit(world, UnitKind::Infantry, 0, 0, SimPos::new(-28_000, 0));
    assert!(submit_order(world, &repair_order(0, eng, other)).is_err());
    // Unknown verb.
    let cmd = IssueOrder::new(
        PlayerId(0),
        [eng],
        UnitOrder::Interact {
            verb: "demolish".to_owned(),
            target: wall,
        },
    );
    assert!(submit_order(world, &cmd).is_err());
    // Someone else's engineer.
    assert!(submit_order(world, &repair_order(1, eng, wall)).is_err());
}

#[test]
fn two_engineers_share_the_work_deterministically() {
    let run = |journal: bool| {
        let (mut app, wall, _) = scenario(7, 100, (-2_000, 1_500), journal);
        let world = app.world_mut();
        sim::spawn_unit(world, UnitKind::Engineer, 0, 0, SimPos::new(9_500, 2_000));
        let mut full_at = None;
        for _ in 0..600 {
            run_ticks(world, 1);
            if full_at.is_none() && hp(world, wall) == WALL_MAX_HP {
                full_at = Some(world.resource::<SimClock>().tick);
            }
        }
        (full_at, state_hash(world))
    };
    let (a_at, a_hash) = run(false);
    let (b_at, b_hash) = run(false);
    let (c_at, c_hash) = run(true);
    assert!(a_at.is_some());
    assert_eq!((a_at, a_hash), (b_at, b_hash), "same seed: same result");
    assert_eq!((a_at, a_hash), (c_at, c_hash), "journal on/off: same hash");
    // Alone it takes (1000 - 100) / 4 = 225 working ticks; two are faster.
    let (mut solo, wall, _) = scenario(7, 100, (-2_000, 1_500), false);
    let world = solo.world_mut();
    let mut solo_at = None;
    for _ in 0..600 {
        run_ticks(world, 1);
        if solo_at.is_none() && hp(world, wall) == WALL_MAX_HP {
            solo_at = Some(world.resource::<SimClock>().tick);
        }
    }
    assert!(a_at.unwrap() < solo_at.unwrap(), "{a_at:?} vs {solo_at:?}");
}

#[test]
fn demo_world_has_damaged_walls_that_engineers_fix() {
    let start = sim::run_headless(1, 200, 0);
    assert_eq!(start.damaged_walls, sim::DAMAGED_WALLS.len() as u32);
    let end = sim::run_headless(1, 200, 3000);
    assert!(
        end.damaged_walls < start.damaged_walls,
        "after the march some engineers repaired walls: {end:?}"
    );
    assert_eq!(end.failed, 0, "{end:?}");
}
