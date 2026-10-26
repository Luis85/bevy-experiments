//! Plan wave F1: `npc_command` (orders, queues, groups, squads, BT leaf).

use bevy_app::App;
use bevy_ecs::prelude::*;
use npc_ai::{Abort, AiRegistry, Brain, ParamValue, Status, StopReason};
use npc_command::{
    CommandAppExt, CommandConfig, CommandPlugin, Commandable, CreateSquad, DisbandSquad,
    EXECUTE_ORDER_ACTION, Formation, FormationKind, HAS_UNIT_ORDER, IssueOrder, MemberOf,
    OrderExecutor, OrderQueue, OrderStep, Owner, PlayerId, Squad, UnitOrder,
};
use npc_kernel::commands::CommandError;
use npc_kernel::hash::state_hash;
use npc_kernel::journal::{JournalEntry, JournalKind};
use npc_kernel::registry::KernelAppExt;
use npc_kernel::spatial::{CellCoord, CellFlags, CellInfo, GridBounds, SpatialGrid, SpatialPlugin};
use npc_kernel::{
    Journal, JournalPlugin, NpcKernelPlugin, SimCommand, SimId, SimIndex, SimPos, run_ticks,
    spawn_sim, submit,
};
use npc_locomotion::{MoveIntent, StraightLineMover};
use npc_nav::{NavAgent, NavPlugin};

const P1: PlayerId = PlayerId(1);
const P2: PlayerId = PlayerId(2);

const WALL: CellInfo = CellInfo {
    flags: CellFlags {
        blocking: true,
        occluding: true,
        tags: 0,
    },
    extra_cost: 0,
};

fn build(nav: bool, setup: impl FnOnce(&mut App)) -> App {
    let mut app = App::new();
    app.insert_resource(SpatialGrid::new(1000, GridBounds::centered(64, 64)));
    app.add_plugins((
        NpcKernelPlugin::default(),
        SpatialPlugin,
        StraightLineMover,
        CommandPlugin,
        JournalPlugin::default(),
    ));
    if nav {
        app.add_plugins(NavPlugin);
    }
    setup(&mut app);
    app.finish();
    app.cleanup();
    app
}

fn app() -> App {
    build(false, |_| {})
}

fn unit(app: &mut App, player: PlayerId, at: SimPos, speed: i32) -> (SimId, Entity) {
    spawn_sim(
        app.world_mut(),
        (at, Commandable::new(speed), Owner(player)),
    )
}

fn order(app: &mut App, cmd: IssueOrder) {
    submit(app.world_mut(), None, cmd.to_command()).expect("order accepted");
}

fn issue(app: &mut App, player: PlayerId, units: &[SimId], o: UnitOrder) {
    order(app, IssueOrder::new(player, units.iter().copied(), o));
}

fn pos(app: &App, e: Entity) -> SimPos {
    *app.world().get::<SimPos>(e).unwrap()
}

fn queue(app: &App, e: Entity) -> OrderQueue {
    app.world().get::<OrderQueue>(e).unwrap().clone()
}

fn journal(app: &App) -> Vec<JournalEntry> {
    app.world()
        .resource::<Journal>()
        .entries
        .iter()
        .cloned()
        .collect()
}

/// `(kind name, order, reason)` of the unit's order entries.
fn order_log(app: &App, id: SimId) -> Vec<String> {
    journal(app)
        .into_iter()
        .filter(|e| e.unit == Some(id))
        .filter_map(|e| match e.kind {
            JournalKind::OrderIssued { order, queued, .. } => Some(format!(
                "issued {order}{}",
                if queued { " queued" } else { "" }
            )),
            JournalKind::OrderStarted { order } => Some(format!("started {order}")),
            JournalKind::OrderDone { order } => Some(format!("done {order}")),
            JournalKind::OrderFailed { order, reason, .. } => {
                Some(format!("failed {order} ({reason})"))
            }
            _ => None,
        })
        .collect()
}

fn run_until(app: &mut App, max: u64, mut done: impl FnMut(&App) -> bool) -> u64 {
    for t in 1..=max {
        run_ticks(app.world_mut(), 1);
        if done(app) {
            return t;
        }
    }
    panic!("condition not reached in {max} ticks");
}

fn idle(app: &App, e: Entity) -> bool {
    queue(app, e).is_empty()
}

#[test]
fn orders_from_another_player_are_rejected() {
    let mut app = app();
    let (a, _) = unit(&mut app, P1, SimPos::new(0, 0), 100);
    let (b, _) = unit(&mut app, P2, SimPos::new(0, 0), 100);
    let w = app.world_mut();
    let mv = UnitOrder::Move { pos: (5000, 0) };
    let err = submit(w, None, IssueOrder::new(P2, [a], mv.clone()).to_command()).unwrap_err();
    assert!(
        matches!(&err, CommandError::Unavailable { reason } if reason.contains("owned by player 1")),
        "{err:?}"
    );
    // One foreign unit rejects the whole group order.
    assert!(
        submit(
            w,
            None,
            IssueOrder::new(P1, [a, b], mv.clone()).to_command()
        )
        .is_err()
    );
    assert!(submit(w, None, IssueOrder::new(P1, [a], mv.clone()).to_command()).is_ok());
    // A plain entity is not commandable.
    let (rock, _) = spawn_sim(w, SimPos::new(0, 0));
    assert!(matches!(
        submit(w, None, IssueOrder::new(P1, [rock], mv).to_command()),
        Err(CommandError::WrongKind { .. })
    ));
    // JSON round trip: the wire form validates the same way.
    let json = format!(
        r#"{{"type":"IssueOrder","player":2,"units":[{}],"order":{{"type":"Stop"}}}}"#,
        a.0
    );
    let cmd: SimCommand = serde_json::from_str(&json).unwrap();
    assert!(submit(w, None, cmd).is_err());
}

#[test]
fn ownership_is_rechecked_when_the_order_applies() {
    let mut app = app();
    let (a, e) = unit(&mut app, P1, SimPos::new(0, 0), 100);
    issue(&mut app, P1, &[a], UnitOrder::Move { pos: (5000, 0) });
    // The unit changes hands before the tick runs.
    app.world_mut().entity_mut(e).insert(Owner(P2));
    run_ticks(app.world_mut(), 1);
    assert!(idle(&app, e), "skipped: {:?}", queue(&app, e));
    let skipped = &app
        .world()
        .resource::<npc_kernel::commands::SkippedCommands>()
        .entries;
    assert_eq!(skipped.len(), 1);
}

#[test]
fn queue_appends_with_shift_and_replaces_without() {
    let mut app = app();
    let (a, e) = unit(&mut app, P1, SimPos::new(0, 0), 500);
    issue(&mut app, P1, &[a], UnitOrder::Move { pos: (3000, 0) });
    order(
        &mut app,
        IssueOrder::new(P1, [a], UnitOrder::Move { pos: (3000, 3000) }).queued(true),
    );
    run_ticks(app.world_mut(), 1);
    let q = queue(&app, e);
    assert_eq!(q.len(), 2, "{q:?}");
    run_until(&mut app, 100, |app| idle(app, e));
    assert_eq!(pos(&app, e), SimPos::new(3000, 3000));
    assert_eq!(
        order_log(&app, a),
        [
            "issued Move",
            "issued Move queued",
            "started Move",
            "done Move",
            "started Move",
            "done Move"
        ]
    );

    // Replace: a new order drops the running one and the queue.
    issue(&mut app, P1, &[a], UnitOrder::Move { pos: (-10_000, 0) });
    order(
        &mut app,
        IssueOrder::new(
            P1,
            [a],
            UnitOrder::Move {
                pos: (-10_000, 5000),
            },
        )
        .queued(true),
    );
    run_ticks(app.world_mut(), 3);
    issue(&mut app, P1, &[a], UnitOrder::Move { pos: (0, 0) });
    run_ticks(app.world_mut(), 1);
    let q = queue(&app, e);
    assert_eq!(q.len(), 1, "{q:?}");
    assert_eq!(
        q.current.unwrap().order.order,
        UnitOrder::Move { pos: (0, 0) }
    );
    assert!(order_log(&app, a).contains(&"failed Move (replaced)".to_owned()));
    run_until(&mut app, 100, |app| idle(app, e));
    assert_eq!(pos(&app, e), SimPos::new(0, 0));
}

#[test]
fn full_queue_rejects_shift_orders() {
    let mut app = build(false, |app| {
        app.insert_resource(CommandConfig {
            max_queue: 2,
            ..Default::default()
        });
    });
    let (a, _) = unit(&mut app, P1, SimPos::new(0, 0), 10);
    for i in 0..2 {
        order(
            &mut app,
            IssueOrder::new(
                P1,
                [a],
                UnitOrder::Move {
                    pos: (1000 * i, 9000),
                },
            )
            .queued(true),
        );
        run_ticks(app.world_mut(), 1);
    }
    let third = IssueOrder::new(P1, [a], UnitOrder::Hold).queued(true);
    assert!(submit(app.world_mut(), None, third.to_command()).is_err());
}

#[test]
fn stop_drops_everything_and_hold_stands_until_replaced() {
    let mut app = app();
    let (a, e) = unit(&mut app, P1, SimPos::new(0, 0), 100);
    issue(&mut app, P1, &[a], UnitOrder::Move { pos: (20_000, 0) });
    order(
        &mut app,
        IssueOrder::new(P1, [a], UnitOrder::Move { pos: (0, 20_000) }).queued(true),
    );
    run_ticks(app.world_mut(), 10);
    issue(&mut app, P1, &[a], UnitOrder::Stop);
    run_ticks(app.world_mut(), 2);
    let stopped_at = pos(&app, e);
    run_ticks(app.world_mut(), 10);
    assert_eq!(pos(&app, e), stopped_at, "stopped");
    assert!(idle(&app, e));
    assert!(app.world().get::<MoveIntent>(e).is_none());
    let log = order_log(&app, a);
    assert!(log.contains(&"failed Move (stopped)".to_owned()), "{log:?}");
    assert!(log.ends_with(&["started Stop".to_owned(), "done Stop".to_owned()]));

    // Hold: never completes, so a queued order behind it waits.
    issue(&mut app, P1, &[a], UnitOrder::Hold);
    order(
        &mut app,
        IssueOrder::new(P1, [a], UnitOrder::Move { pos: (5000, 0) }).queued(true),
    );
    run_ticks(app.world_mut(), 30);
    assert_eq!(pos(&app, e), stopped_at, "holding");
    assert_eq!(queue(&app, e).len(), 2);
    // A replacing order ends the hold.
    issue(&mut app, P1, &[a], UnitOrder::Move { pos: (5000, 0) });
    run_until(&mut app, 200, |app| idle(app, e));
    assert_eq!(pos(&app, e), SimPos::new(5000, 0));
    assert!(order_log(&app, a).contains(&"failed Hold (replaced)".to_owned()));
}

#[test]
fn follow_tracks_a_moving_target_and_fails_when_it_is_gone() {
    let mut app = app();
    let (t, te) = unit(&mut app, P2, SimPos::new(0, 0), 50);
    let (f, fe) = unit(&mut app, P1, SimPos::new(-5000, 0), 120);
    issue(
        &mut app,
        P2,
        &[t],
        UnitOrder::Move {
            pos: (20_000, 10_000),
        },
    );
    issue(&mut app, P1, &[f], UnitOrder::Follow { target: t });
    let cfg = CommandConfig::default();
    let slack = i64::from(cfg.follow_distance_mm + cfg.follow_retarget_mm + 120);
    run_ticks(app.world_mut(), 100);
    for _ in 0..400 {
        run_ticks(app.world_mut(), 1);
        let d = pos(&app, fe).dist2(pos(&app, te)).isqrt();
        assert!(d <= slack, "follower {d} mm behind");
    }
    assert_eq!(pos(&app, te), SimPos::new(20_000, 10_000), "target arrived");
    // Follow never completes on its own.
    assert!(queue(&app, fe).current.is_some());
    app.world_mut().despawn(te);
    run_ticks(app.world_mut(), 1);
    assert!(idle(&app, fe));
    assert!(order_log(&app, f).contains(&"failed Follow (target gone)".to_owned()));
    // Following yourself is rejected.
    let me = IssueOrder::new(P1, [f], UnitOrder::Follow { target: f });
    assert!(submit(app.world_mut(), None, me.to_command()).is_err());
}

#[test]
fn patrol_loops_between_its_points() {
    let mut app = app();
    let (a, e) = unit(&mut app, P1, SimPos::new(0, 0), 250);
    issue(
        &mut app,
        P1,
        &[a],
        UnitOrder::Patrol {
            points: vec![(2000, 0), (2000, 2000)],
        },
    );
    let mut visits = Vec::new();
    for _ in 0..200 {
        run_ticks(app.world_mut(), 1);
        let p = pos(&app, e);
        if [SimPos::new(2000, 0), SimPos::new(2000, 2000)].contains(&p) && visits.last() != Some(&p)
        {
            visits.push(p);
        }
    }
    assert!(visits.len() >= 4, "{visits:?}");
    assert_eq!(visits[0], SimPos::new(2000, 0));
    assert_eq!(visits[1], SimPos::new(2000, 2000));
    assert_eq!(visits[2], SimPos::new(2000, 0));
    let log = order_log(&app, a);
    assert_eq!(
        log,
        ["issued Patrol", "started Patrol"],
        "started once, never done"
    );

    // One point: back and forth between it and where the patrol started.
    let (b, be) = unit(&mut app, P1, SimPos::new(-3000, 0), 250);
    issue(
        &mut app,
        P1,
        &[b],
        UnitOrder::Patrol {
            points: vec![(-3000, 2000)],
        },
    );
    let mut seen_home_again = false;
    let mut left = false;
    for _ in 0..100 {
        run_ticks(app.world_mut(), 1);
        let p = pos(&app, be);
        left |= p == SimPos::new(-3000, 2000);
        seen_home_again |= left && p == SimPos::new(-3000, 0);
    }
    assert!(seen_home_again);
}

#[test]
fn group_move_ends_in_exact_line_slots() {
    let mut app = app();
    let ids: Vec<(SimId, Entity)> = (0..3)
        .map(|i| unit(&mut app, P1, SimPos::new(-4000 + 4000 * i, 10_000), 200))
        .collect();
    let units: Vec<SimId> = ids.iter().map(|(id, _)| *id).collect();
    order(
        &mut app,
        IssueOrder::new(P1, units, UnitOrder::Move { pos: (0, 0) })
            .formation(Formation::new(FormationKind::Line, 2000)),
    );
    run_until(&mut app, 300, |app| ids.iter().all(|&(_, e)| idle(app, e)));
    let got: Vec<SimPos> = ids.iter().map(|&(_, e)| pos(&app, e)).collect();
    assert_eq!(
        got,
        [
            SimPos::new(-2000, 0),
            SimPos::new(0, 0),
            SimPos::new(2000, 0)
        ]
    );
}

#[test]
fn blocked_slot_is_relocated_to_the_nearest_free_cell() {
    let mut app = app();
    {
        let mut grid = app.world_mut().resource_mut::<SpatialGrid>();
        // The east slot (2000, 0) lies in cell (2, 0).
        grid.set_static(CellCoord::new(2, 0), WALL);
    }
    let ids: Vec<(SimId, Entity)> = (0..3)
        .map(|i| unit(&mut app, P1, SimPos::new(-4000 + 4000 * i, 10_000), 200))
        .collect();
    let units: Vec<SimId> = ids.iter().map(|(id, _)| *id).collect();
    order(
        &mut app,
        IssueOrder::new(P1, units, UnitOrder::Move { pos: (0, 0) })
            .formation(Formation::new(FormationKind::Line, 2000)),
    );
    run_ticks(app.world_mut(), 1);
    let slot = |app: &App, e: Entity| queue(app, e).current.unwrap().order.order;
    // Ring 1 around (2, 0), by (dist², x, z): (1, -1) centre (1500, -500).
    assert_eq!(slot(&app, ids[2].1), UnitOrder::Move { pos: (1500, -500) });
    assert_eq!(slot(&app, ids[1].1), UnitOrder::Move { pos: (0, 0) });
}

#[test]
fn nav_group_move_arrives_in_formation_around_an_obstacle() {
    let mut app = build(true, |_| {});
    {
        let mut grid = app.world_mut().resource_mut::<SpatialGrid>();
        // A wall across x = -8..8 at z = 0: the group must go around it.
        for x in -8..=8 {
            grid.set_static(CellCoord::new(x, 0), WALL);
        }
    }
    let ids: Vec<(SimId, Entity)> = (0..6)
        .map(|i| {
            let (id, e) = unit(
                &mut app,
                P1,
                SimPos::new(-3000 + 1200 * i, 8000 + (i % 2) * 1200),
                150,
            );
            app.world_mut().entity_mut(e).insert(NavAgent);
            (id, e)
        })
        .collect();
    let units: Vec<SimId> = ids.iter().map(|(id, _)| *id).collect();
    order(
        &mut app,
        IssueOrder::new(P1, units.clone(), UnitOrder::Move { pos: (0, -10_000) })
            .formation(Formation::new(FormationKind::Box, 2000)),
    );
    run_ticks(app.world_mut(), 1);
    let slots: Vec<SimPos> = ids
        .iter()
        .map(|&(_, e)| {
            queue(&app, e)
                .current
                .unwrap()
                .order
                .order
                .move_goal()
                .unwrap()
        })
        .collect();
    let ticks = run_until(&mut app, 2000, |app| {
        let grid = app.world().resource::<SpatialGrid>();
        for &(_, e) in &ids {
            let p = *app.world().get::<SimPos>(e).unwrap();
            assert!(!grid.is_blocking(grid.cell_of(p)), "in the wall at {p:?}");
        }
        ids.iter().all(|&(_, e)| idle(app, e))
    });
    for (i, &(id, e)) in ids.iter().enumerate() {
        assert_eq!(pos(&app, e), slots[i], "unit {id:?} in its slot");
        assert!(order_log(&app, id).ends_with(&["done Move".to_owned()]));
    }
    // 3 columns × 2 rows, 2 m apart, centred on the target, facing north.
    let mut xs: Vec<i32> = slots.iter().map(|p| p.x).collect();
    xs.sort_unstable();
    xs.dedup();
    assert_eq!(xs, [-2000, 0, 2000]);
    let mut zs: Vec<i32> = slots.iter().map(|p| p.z).collect();
    zs.sort_unstable();
    zs.dedup();
    assert_eq!(zs, [-11_000, -9_000]);
    let searches = journal(&app)
        .iter()
        .filter(|e| matches!(e.kind, JournalKind::PathFound { .. }))
        .count();
    println!("nav group move: {ticks} ticks, {searches} path searches for 6 units");
    assert!(
        searches < 6,
        "followers reuse the leader's path ({searches} searches)"
    );
}

#[test]
fn squads_take_orders_like_units() {
    let mut app = app();
    let ids: Vec<(SimId, Entity)> = (0..3)
        .map(|i| unit(&mut app, P1, SimPos::new(-1000 + 1000 * i, 5000), 200))
        .collect();
    let members: Vec<SimId> = ids.iter().map(|(id, _)| *id).collect();
    submit(
        app.world_mut(),
        None,
        SimCommand::content(&CreateSquad {
            player: P1,
            members: members.clone(),
            formation: Some(Formation::new(FormationKind::Wedge, 2000)),
        }),
    )
    .unwrap();
    run_ticks(app.world_mut(), 1);
    let mut q = app.world_mut().query::<(&SimId, &Squad, &Owner)>();
    let (squad, s, owner) = q
        .single(app.world())
        .map(|(i, s, o)| (*i, s.clone(), *o))
        .unwrap();
    assert_eq!(s.members, members);
    assert_eq!(owner, Owner(P1));
    for &(_, e) in &ids {
        assert_eq!(app.world().get::<MemberOf>(e), Some(&MemberOf(squad)));
    }
    // Player 2 may not order player 1's squad.
    let foreign = IssueOrder::new(P2, [squad], UnitOrder::Hold);
    assert!(submit(app.world_mut(), None, foreign.to_command()).is_err());
    issue(&mut app, P1, &[squad], UnitOrder::Move { pos: (0, -6000) });
    run_until(&mut app, 300, |app| ids.iter().all(|&(_, e)| idle(app, e)));
    let mut got: Vec<(i32, i32)> = ids
        .iter()
        .map(|&(_, e)| {
            let p = pos(&app, e);
            (p.x, p.z)
        })
        .collect();
    got.sort_unstable();
    // The squad's wedge: tip on the target, wings 2 m back and aside.
    assert_eq!(got, [(-2000, -4000), (0, -6000), (2000, -4000)]);

    // A member that leaves the sim is pruned; disbanding frees the rest.
    app.world_mut().despawn(ids[0].1);
    run_ticks(app.world_mut(), 1);
    let squad_e = app.world().resource::<SimIndex>().get(squad).unwrap();
    assert_eq!(
        app.world().get::<Squad>(squad_e).unwrap().members,
        members[1..]
    );
    submit(
        app.world_mut(),
        None,
        SimCommand::content(&DisbandSquad { player: P1, squad }),
    )
    .unwrap();
    run_ticks(app.world_mut(), 1);
    assert!(app.world().resource::<SimIndex>().get(squad).is_none());
    assert!(app.world().get::<MemberOf>(ids[1].1).is_none());
}

#[test]
fn journal_explains_a_blocked_move() {
    let mut app = build(false, |app| {
        app.insert_resource(CommandConfig {
            block_fail_ticks: 5,
            ..Default::default()
        });
    });
    app.world_mut()
        .resource_mut::<SpatialGrid>()
        .set_static(CellCoord::new(2, 0), WALL);
    let (a, e) = unit(&mut app, P1, SimPos::new(500, 500), 300);
    issue(
        &mut app,
        P1,
        &[a],
        UnitOrder::AttackMove { pos: (5500, 500) },
    );
    run_until(&mut app, 100, |app| idle(app, e));
    assert_eq!(
        order_log(&app, a),
        [
            "issued AttackMove",
            "started AttackMove",
            "failed AttackMove (blocked)"
        ]
    );
}

fn test_idle_start(_: &mut World, _: Entity, _: &ParamValue) -> Status {
    Status::Running
}

fn test_idle_stop(_: &mut World, _: Entity, _: StopReason) {}

#[test]
fn a_tree_runs_orders_through_the_execute_order_leaf() {
    let mut app = build(false, |app| {
        app.register_action("TestIdle", test_idle_start, test_idle_stop)
            .register_tree("rts_unit", |b| {
                b.selector("Unit", |b| {
                    b.guard("Orders", HAS_UNIT_ORDER, Abort::LowerPriority, |b| {
                        b.leaf("ExecuteOrder", EXECUTE_ORDER_ACTION, ());
                    });
                    b.leaf("Idle", "TestIdle", ());
                });
            });
    });
    let tree = app
        .world()
        .resource::<AiRegistry>()
        .tree_id("rts_unit")
        .unwrap();
    let (a, e) = unit(&mut app, P1, SimPos::new(0, 0), 200);
    app.world_mut().entity_mut(e).insert(Brain { tree });
    run_ticks(app.world_mut(), 5);
    let leaf_starts = |app: &App, action: &str| {
        journal(app)
            .iter()
            .filter(|j| j.unit == Some(a))
            .filter(|j| matches!(&j.kind, JournalKind::LeafStart { action: x, .. } if x == action))
            .count()
    };
    assert_eq!(leaf_starts(&app, "TestIdle"), 1);
    issue(&mut app, P1, &[a], UnitOrder::Move { pos: (2000, 0) });
    order(
        &mut app,
        IssueOrder::new(P1, [a], UnitOrder::Move { pos: (2000, 2000) }).queued(true),
    );
    run_until(&mut app, 100, |app| idle(app, e));
    assert_eq!(pos(&app, e), SimPos::new(2000, 2000));
    run_ticks(app.world_mut(), 2);
    assert_eq!(
        leaf_starts(&app, EXECUTE_ORDER_ACTION),
        1,
        "one leaf, two orders"
    );
    assert_eq!(
        leaf_starts(&app, "TestIdle"),
        2,
        "back to idle after the queue"
    );
    let aborts = journal(&app)
        .iter()
        .filter(|j| j.unit == Some(a) && matches!(j.kind, JournalKind::Abort { .. }))
        .count();
    assert_eq!(aborts, 1, "the order preempted idle");
}

fn interact_start(world: &mut World, unit: Entity, _: &UnitOrder) -> OrderStep {
    world.entity_mut(unit).insert(Countdown(3));
    OrderStep::Running
}

fn interact_tick(world: &mut World, unit: Entity, _: &UnitOrder) -> OrderStep {
    let mut c = world.get_mut::<Countdown>(unit).unwrap();
    c.0 -= 1;
    if c.0 == 0 {
        OrderStep::Done
    } else {
        OrderStep::Running
    }
}

#[derive(Component)]
struct Countdown(u32);

#[test]
fn data_only_orders_need_an_executor_by_name() {
    let mut plain = app();
    let (a, _) = unit(&mut plain, P1, SimPos::new(0, 0), 100);
    let (door, _) = spawn_sim(plain.world_mut(), SimPos::new(1000, 0));
    let interact = UnitOrder::Interact {
        verb: "open".into(),
        target: door,
    };
    let err = submit(
        plain.world_mut(),
        None,
        IssueOrder::new(P1, [a], interact.clone()).to_command(),
    )
    .unwrap_err();
    assert!(format!("{err}").contains("no executor"), "{err}");

    let mut app = build(false, |app| {
        app.register_order_executor(
            "Interact",
            "test_interact",
            OrderExecutor::new(interact_start, interact_tick),
        );
    });
    let (a, e) = unit(&mut app, P1, SimPos::new(0, 0), 100);
    let (door, _) = spawn_sim(app.world_mut(), SimPos::new(1000, 0));
    issue(
        &mut app,
        P1,
        &[a],
        UnitOrder::Interact {
            verb: "open".into(),
            target: door,
        },
    );
    let ticks = run_until(&mut app, 20, |app| idle(app, e));
    assert_eq!(ticks, 4, "start + 3 ticks");
    assert!(order_log(&app, a).ends_with(&["done Interact".to_owned()]));
    let manifest = &app
        .world()
        .resource::<npc_kernel::registry::ContentHash>()
        .manifest;
    assert!(manifest.contains(&("order_executor".into(), "Interact=test_interact".into())));
}

#[derive(Component)]
struct Driver(Entity);

fn to_driver(world: &World, unit: Entity) -> Option<Entity> {
    world.get::<Driver>(unit).map(|d| d.0)
}

#[test]
fn redirects_forward_orders_to_another_unit() {
    let mut app = build(false, |app| {
        app.register_order_redirect("test_driver", to_driver);
    });
    let (_, driver) = unit(&mut app, P1, SimPos::new(0, 0), 100);
    let (vehicle, ve) = unit(&mut app, P1, SimPos::new(0, 0), 100);
    app.world_mut().entity_mut(ve).insert(Driver(driver));
    issue(&mut app, P1, &[vehicle], UnitOrder::Move { pos: (3000, 0) });
    run_ticks(app.world_mut(), 1);
    assert!(idle(&app, ve), "the vehicle holds no order");
    assert!(!idle(&app, driver), "the driver got it");
}

fn scenario(seed: u64, journal_on: bool) -> u64 {
    let mut app = App::new();
    app.insert_resource(SpatialGrid::new(1000, GridBounds::centered(64, 64)));
    app.add_plugins((
        NpcKernelPlugin {
            seed,
            ..Default::default()
        },
        SpatialPlugin,
        StraightLineMover,
        NavPlugin,
        CommandPlugin,
    ));
    if journal_on {
        app.add_plugins(JournalPlugin::default());
    }
    app.finish();
    app.cleanup();
    {
        let mut grid = app.world_mut().resource_mut::<SpatialGrid>();
        for z in -5..=5 {
            grid.set_static(CellCoord::new(0, z), WALL);
        }
    }
    let ids: Vec<SimId> = (0..12)
        .map(|i| {
            let (id, e) = unit(
                &mut app,
                P1,
                SimPos::new(-9000 + (i % 4) * 1000, -3000 + (i / 4) * 1500),
                120 + (i % 3) * 20,
            );
            if i % 2 == 0 {
                app.world_mut().entity_mut(e).insert(NavAgent);
            }
            id
        })
        .collect();
    order(
        &mut app,
        IssueOrder::new(
            P1,
            ids[..8].iter().copied(),
            UnitOrder::Move { pos: (9000, 0) },
        ),
    );
    issue(
        &mut app,
        P1,
        &[ids[8]],
        UnitOrder::Follow { target: ids[0] },
    );
    issue(
        &mut app,
        P1,
        &[ids[9]],
        UnitOrder::Patrol {
            points: vec![(-9000, 8000), (-2000, 8000)],
        },
    );
    issue(&mut app, P1, &[ids[10], ids[11]], UnitOrder::Hold);
    run_ticks(app.world_mut(), 150);
    order(
        &mut app,
        IssueOrder::new(
            P1,
            ids[..4].iter().copied(),
            UnitOrder::Move { pos: (9000, 9000) },
        )
        .queued(true)
        .formation(Formation::new(FormationKind::Wedge, 1500)),
    );
    run_ticks(app.world_mut(), 400);
    state_hash(app.world_mut())
}

#[test]
fn same_commands_same_hash() {
    let a = scenario(7, false);
    assert_eq!(a, scenario(7, false), "two runs");
    assert_eq!(a, scenario(7, true), "the journal never changes the sim");
}

// ------------------------------------------------- final review FX1 fixes

/// Final review cleanup 2: a squad's formation spacing is validated like an
/// order's.
#[test]
fn create_squad_rejects_a_bad_formation_spacing() {
    let mut app = app();
    let (a, _) = unit(&mut app, P1, SimPos::new(0, 0), 200);
    for spacing in [0, -5, npc_kernel::commands::POS_LIMIT + 1] {
        let err = submit(
            app.world_mut(),
            None,
            SimCommand::content(&CreateSquad {
                player: P1,
                members: vec![a],
                formation: Some(Formation::new(FormationKind::Line, spacing)),
            }),
        )
        .unwrap_err();
        assert!(
            matches!(&err, CommandError::InvalidValue { field, .. } if field == "formation.spacing_mm"),
            "{spacing}: {err:?}"
        );
    }
    let huge = IssueOrder::new(P1, [a], UnitOrder::Hold)
        .formation(Formation::new(FormationKind::Line, i32::MAX));
    assert!(submit(app.world_mut(), None, huge.to_command()).is_err());
}

/// Cleanup 2: derived group slots never leave ±`POS_LIMIT`.
#[test]
fn group_slots_are_clamped_to_the_position_limit() {
    use npc_kernel::commands::POS_LIMIT;
    let units: Vec<(SimId, SimPos)> = (1..=9)
        .map(|i| (SimId(i), SimPos::new(i as i32 * 100, 0)))
        .collect();
    let plan = npc_command::formation::plan_group(
        Formation::new(FormationKind::Line, POS_LIMIT),
        SimPos::new(POS_LIMIT, POS_LIMIT),
        &units,
        None,
        0,
    );
    for (_, p) in &plan.slots {
        assert!(p.x.abs() <= POS_LIMIT && p.z.abs() <= POS_LIMIT, "{p:?}");
    }
}

/// Final review cleanup 3: a squad that leaves the sim (e.g. `Despawn`)
/// left `MemberOf(dead)` on its members.
#[test]
fn despawned_squad_frees_its_members() {
    let mut app = app();
    let ids: Vec<(SimId, Entity)> = (0..2)
        .map(|i| unit(&mut app, P1, SimPos::new(1000 * i, 0), 200))
        .collect();
    submit(
        app.world_mut(),
        None,
        SimCommand::content(&CreateSquad {
            player: P1,
            members: ids.iter().map(|(id, _)| *id).collect(),
            formation: None,
        }),
    )
    .unwrap();
    run_ticks(app.world_mut(), 1);
    let squad = app.world().get::<MemberOf>(ids[0].1).expect("member").0;
    submit(app.world_mut(), None, SimCommand::Despawn { unit: squad.0 }).unwrap();
    run_ticks(app.world_mut(), 1);
    for &(id, e) in &ids {
        assert!(
            app.world().get::<MemberOf>(e).is_none(),
            "#{} still a member of the gone squad",
            id.0
        );
    }
}
