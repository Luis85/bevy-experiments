//! Task G: the command input layer and orders.

mod common;

use common::*;
use npc_framework::ai::ActiveLeaf;
use npc_framework::kernel::commands::{CommandError, CommandLog, CommandQueue};
use npc_framework::kernel::json::JsonObject;
use village_content::{Activity, Villager};

fn goto(unit: SimId, x: i32, z: i32) -> SimCommand {
    SimCommand::Order {
        unit: unit.0,
        order: Order::GoTo { pos: (x, z) },
    }
}

/// Upper bound on the ticks to walk from `a` to `b` at 60 mm/tick (integer
/// steps truncate per axis, so diagonal steps are up to ~1.5 mm short).
fn walk_ticks(a: SimPos, b: SimPos) -> u64 {
    let d2 = a.dist2(b);
    let root = d2.isqrt();
    let dist = if root * root < d2 { root + 1 } else { root };
    dist as u64 / 58 + 2
}

/// A villager wandering at tick ~600 with a full stockpile.
fn wandering() -> (App, SimId) {
    let mut app = village(1);
    let v = ids(&app).villager;
    set_tick(&mut app, 600);
    set_food(&mut app, 10);
    run_until(&mut app, 1000, |app| {
        path(app, v).contains("Wander > MoveToDest")
    })
    .expect("wanders");
    (app, v)
}

#[test]
fn order_goto_interrupts_wander_not_urgent() {
    let (mut app, v) = wandering();
    let goal = SimPos::new(8000, 8000);
    send(&mut app, None, goto(v, goal.x, goal.z));
    run_ticks(app.world_mut(), 1);
    assert!(
        path(&app, v).starts_with("Villager > FollowOrder"),
        "path {:?}",
        path(&app, v)
    );
    assert_eq!(order(&app, v), Some(OrderKind::GoTo { pos: goal }));
    let budget = walk_ticks(pos(&app, v), goal) + 3;
    run_until(&mut app, budget, |app| pos(app, v) == goal)
        .unwrap_or_else(|| panic!("not there in {budget} ticks; at {:?}", pos(&app, v)));
    run_ticks(app.world_mut(), 2);
    assert_eq!(order(&app, v), None, "order not cleared on arrival");
    assert!(!path(&app, v).contains("FollowOrder"), "{}", path(&app, v));

    // Urgent hunger with food beats a fresh order; the order waits.
    let (mut app, v) = wandering();
    set_hunger(&mut app, 10_000);
    send(&mut app, None, goto(v, goal.x, goal.z));
    run_ticks(app.world_mut(), 1);
    assert!(
        path(&app, v).starts_with("Villager > UrgentEat"),
        "path {:?}",
        path(&app, v)
    );
    assert_eq!(order(&app, v), Some(OrderKind::GoTo { pos: goal }));
}

#[test]
fn order_goto_interrupts_work() {
    let mut app = village(1);
    let v = ids(&app).villager;
    run_until(&mut app, 1000, |app| {
        path(app, v).contains("DayWork > WorkSeq > DoWork")
    })
    .expect("works");
    send(&mut app, None, goto(v, -8000, 8000));
    run_ticks(app.world_mut(), 1);
    assert!(path(&app, v).starts_with("Villager > FollowOrder"));
    assert_eq!(
        *app.world().get::<Activity>(ent(&app, v)).unwrap(),
        Activity::Walk
    );
    let res = app.world().resource::<Reservations>();
    assert!(!res.holds(ids(&app).field, v), "field kept: {res:?}");
}

#[test]
fn order_clear() {
    let (mut app, v) = wandering();
    let goal = SimPos::new(12_000, 12_000);
    send(&mut app, None, goto(v, goal.x, goal.z));
    run_ticks(app.world_mut(), 5);
    assert!(path(&app, v).contains("FollowOrder"));
    send(
        &mut app,
        None,
        SimCommand::Order {
            unit: v.0,
            order: Order::Clear,
        },
    );
    run_until(&mut app, 3, |app| !path(app, v).contains("FollowOrder"))
        .unwrap_or_else(|| panic!("still following; path {:?}", path(&app, v)));
    assert_eq!(order(&app, v), None);
    assert_ne!(pos(&app, v), goal);
}

#[test]
fn order_replaced_while_walking_retargets() {
    let (mut app, v) = wandering();
    send(&mut app, None, goto(v, 12_000, 12_000));
    run_ticks(app.world_mut(), 5);
    let goal = SimPos::new(-12_000, 12_000);
    send(&mut app, None, goto(v, goal.x, goal.z));
    let budget = walk_ticks(pos(&app, v), goal) + 4;
    run_until(&mut app, budget, |app| pos(app, v) == goal)
        .unwrap_or_else(|| panic!("not there; at {:?} path {:?}", pos(&app, v), path(&app, v)));
}

#[test]
fn spawn_villager_mid_run_acts() {
    let mut app = village(1);
    run_ticks(app.world_mut(), 100);
    let new = next_sim_id(&app);
    send(
        &mut app,
        None,
        SimCommand::content(&SpawnVillager {
            pos: (1000, 0),
            home: None,
        }),
    );
    run_until(&mut app, 2, |app| {
        app.world()
            .resource::<SimIndex>()
            .get(new)
            .and_then(|e| app.world().get::<BrainState>(e))
            .is_some_and(|s| !s.running_path.is_empty())
    })
    .expect("spawned villager thinks within 2 ticks");
    let e = ent(&app, new);
    assert_eq!(
        app.world().get::<Villager>(e).unwrap().home,
        ids(&app).house
    );
    assert!(app.world().get::<ActiveLeaf>(e).is_some());
    assert!(app.world().get::<CurrentOrder>(e).is_some());
}

#[test]
fn despawn_command_releases_reservations() {
    let mut app = village(1);
    let ids = ids(&app);
    set_tick(&mut app, 600);
    set_food(&mut app, 0);
    run_until(&mut app, 1000, |app| {
        app.world()
            .resource::<Reservations>()
            .holds(ids.field, ids.villager)
    })
    .expect("reserves the field");
    send(
        &mut app,
        None,
        SimCommand::Despawn {
            unit: ids.villager.0,
        },
    );
    run_ticks(app.world_mut(), 1);
    assert!(
        app.world()
            .resource::<SimIndex>()
            .get(ids.villager)
            .is_none()
    );
    let res = app.world().resource::<Reservations>();
    assert!(res.claims.is_empty(), "leaked: {res:?}");
    assert_eq!(app.world().resource::<CommandLog>().entries.len(), 2);
}

#[test]
fn set_need_and_set_food_apply_at_tick() {
    let mut app = village(1);
    let ids = ids(&app);
    assert_eq!(
        send(
            &mut app,
            Some(50),
            SimCommand::content(&SetFood {
                stockpile: ids.stockpile.0,
                food: 7,
            }),
        ),
        (50, 0)
    );
    assert_eq!(
        send(
            &mut app,
            Some(60),
            SimCommand::SetNeed {
                unit: ids.villager.0,
                need: HUNGER.into(),
                value: 30_000,
            },
        ),
        (60, 0)
    );
    run_ticks(app.world_mut(), 50);
    assert_eq!(food(&app), 4);
    run_ticks(app.world_mut(), 1);
    assert_eq!(food(&app), 7);
    run_ticks(app.world_mut(), 9);
    assert_eq!(needs(&app, ids.villager).hunger.value, 100_000 - 60 * 40);
    run_ticks(app.world_mut(), 1);
    // Applied in Inputs, then decayed once in Needs of the same tick.
    assert_eq!(needs(&app, ids.villager).hunger.value, 30_000 - 40);
    assert!(app.world().resource::<CommandQueue>().pending.is_empty());
    let log = &app.world().resource::<CommandLog>().entries;
    assert_eq!(
        log.iter().map(|&(t, s, _)| (t, s)).collect::<Vec<_>>(),
        [(50, 0), (60, 0)]
    );
}

#[test]
fn command_unknown_unit_is_rejected() {
    let mut app = village(1);
    let ids = ids(&app);
    let world = app.world_mut();
    assert!(matches!(
        submit(world, None, SimCommand::Despawn { unit: 999 }),
        Err(CommandError::UnknownUnit { unit: 999 })
    ));
    assert!(matches!(
        submit(world, None, goto(SimId(999), 0, 0)),
        Err(CommandError::UnknownUnit { .. })
    ));
    // Right id, wrong kind of entity.
    assert!(matches!(
        submit(
            world,
            None,
            SimCommand::content(&SetFood {
                stockpile: ids.villager.0,
                food: 1
            })
        ),
        Err(CommandError::WrongKind { .. })
    ));
    assert!(matches!(
        submit(world, None, goto(ids.stockpile, 0, 0)),
        Err(CommandError::WrongKind { .. })
    ));
    assert!(matches!(
        submit(
            world,
            None,
            SimCommand::content(&SpawnVillager {
                pos: (0, 0),
                home: Some(ids.bed.0)
            })
        ),
        Err(CommandError::WrongKind { .. })
    ));
    assert!(matches!(
        submit(
            world,
            None,
            SimCommand::SetNeed {
                unit: ids.villager.0,
                need: ENERGY.into(),
                value: -1
            }
        ),
        Err(CommandError::InvalidValue { .. })
    ));
    run_ticks(world, 10);
    assert!(matches!(
        submit(
            world,
            Some(5),
            SimCommand::Despawn {
                unit: ids.villager.0
            }
        ),
        Err(CommandError::TickInPast { tick: 5, now: 10 })
    ));
    assert!(world.resource::<CommandQueue>().pending.is_empty());

    // An error serializes to JSON with a stable tag.
    let err = CommandError::UnknownUnit { unit: 7 };
    let json = serde_json::to_value(&err).unwrap();
    assert_eq!(json["error"], "UnknownUnit");
    assert!(!err.to_string().is_empty());
}

#[test]
fn command_for_unit_despawned_before_it_applies_is_skipped() {
    let mut app = village(1);
    let v = ids(&app).villager;
    send(&mut app, Some(10), SimCommand::Despawn { unit: v.0 });
    send(
        &mut app,
        Some(20),
        SimCommand::SetNeed {
            unit: v.0,
            need: HUNGER.into(),
            value: 1,
        },
    );
    run_ticks(app.world_mut(), 30);
    assert_eq!(app.world().resource::<CommandLog>().entries.len(), 1);
}

#[test]
fn commands_json_shape() {
    let cmd = goto(SimId(5), 1000, -2000);
    let json = serde_json::to_value(&cmd).unwrap();
    assert_eq!(
        json,
        serde_json::json!({"type": "Order", "unit": 5, "order": {"type": "GoTo", "pos": [1000, -2000]}})
    );
    let back: SimCommand = serde_json::from_value(json).unwrap();
    assert_eq!(back, cmd);
    let need: SimCommand =
        serde_json::from_str(r#"{"type":"SetNeed","unit":5,"need":"Hunger","value":20000}"#)
            .unwrap();
    assert_eq!(
        need,
        SimCommand::SetNeed {
            unit: 5,
            need: HUNGER.into(),
            value: 20_000
        }
    );
}

/// Plan Task P3: a content command survives JSON → `SimCommand::Content` →
/// submit (canonical form) → journal/log → JSON, with the pre-split wire
/// shape and field order.
#[test]
fn content_command_roundtrip() {
    // The old wire form parses as a content command, field order kept.
    let text = r#"{"type":"SetFood","stockpile":3,"food":0}"#;
    let cmd: SimCommand = serde_json::from_str(text).unwrap();
    assert_eq!(
        cmd,
        SimCommand::Content {
            kind: "SetFood".into(),
            payload: JsonObject(vec![
                ("stockpile".into(), serde_json::json!(3)),
                ("food".into(), serde_json::json!(0)),
            ]),
        }
    );
    assert_eq!(serde_json::to_string(&cmd).unwrap(), text);
    assert_eq!(
        cmd,
        SimCommand::content(&SetFood {
            stockpile: 3,
            food: 0
        })
    );

    // Missing optional fields are filled in canonically when submitted, so
    // the journal and the log carry the full typed form.
    let mut app = village(1);
    app.add_plugins(npc_framework::kernel::JournalPlugin::default());
    let spawn: SimCommand =
        serde_json::from_str(r#"{"type":"SpawnVillager","pos":[1000,1000]}"#).unwrap();
    send(&mut app, None, spawn);
    run_ticks(app.world_mut(), 1);
    let log = &app.world().resource::<CommandLog>().entries;
    assert_eq!(
        serde_json::to_string(&log[0].2).unwrap(),
        r#"{"type":"SpawnVillager","pos":[1000,1000],"home":null}"#
    );
    assert_eq!(
        log[0].2,
        SimCommand::content(&SpawnVillager {
            pos: (1000, 1000),
            home: None
        })
    );

    // An unknown kind parses (any `"type"` may be content) but is rejected
    // with the list of known commands.
    let fly: SimCommand = serde_json::from_str(r#"{"type":"Fly","to":[1,2]}"#).unwrap();
    match submit(app.world_mut(), None, fly) {
        Err(CommandError::UnknownCommand { kind, known }) => {
            assert_eq!(kind, "Fly");
            assert_eq!(known, ["SetFood", "SpawnVillager"]);
        }
        other => panic!("expected UnknownCommand, got {other:?}"),
    }
    // A known kind with bad fields is an invalid value, not a panic.
    let bad: SimCommand = serde_json::from_str(r#"{"type":"SetFood","stockpile":"x"}"#).unwrap();
    assert!(matches!(
        submit(app.world_mut(), None, bad),
        Err(CommandError::InvalidValue { .. })
    ));
}
