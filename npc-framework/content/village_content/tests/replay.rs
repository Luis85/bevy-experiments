//! Task G: replays, verification and divergence search.

mod common;

use common::*;
use village_content::replay::{REPLAY_VERSION, Replay, diverge, record, try_new_sim_app, verify};

/// A seed-1 village with three commands queued at ticks 100, 300 and 700.
fn with_commands() -> App {
    let mut app = village(1);
    let ids = ids(&app);
    send(
        &mut app,
        Some(100),
        SimCommand::content(&SetFood {
            stockpile: ids.stockpile.0,
            food: 9,
        }),
    );
    send(
        &mut app,
        Some(300),
        SimCommand::Order {
            unit: ids.villager.0,
            order: Order::GoTo { pos: (8000, 8000) },
        },
    );
    send(
        &mut app,
        Some(700),
        SimCommand::SetNeed {
            unit: ids.villager.0,
            need: HUNGER.into(),
            value: 15_000,
        },
    );
    app
}

#[test]
fn record_then_verify_ok() {
    let mut app = with_commands();
    let replay = record(&mut app, 3000, 500);
    assert_eq!(replay.version, REPLAY_VERSION);
    assert_eq!((replay.seed, replay.scenario.as_str()), (1, "village"));
    assert_eq!(replay.ticks, 3000);
    assert_eq!(replay.commands.len(), 3);
    assert_eq!(
        replay.checkpoints.iter().map(|c| c.0).collect::<Vec<_>>(),
        [500, 1000, 1500, 2000, 2500, 3000]
    );
    assert_eq!(tick(&app), 3000);

    let report = verify(&replay);
    assert!(report.ok, "{report:?}");
    assert_eq!(report.checked, 6);
    assert_eq!(report.first_mismatch, None);

    let json = serde_json::to_string(&replay).unwrap();
    let back: Replay = serde_json::from_str(&json).unwrap();
    assert_eq!(back, replay);
    assert!(verify(&back).ok);
}

#[test]
fn tampered_replay_reports_mismatch_tick() {
    let mut app = with_commands();
    let replay = record(&mut app, 3000, 500);

    let mut tampered = replay.clone();
    tampered.commands[0].2 = SimCommand::content(&SetFood {
        stockpile: 3,
        food: 2,
    });
    let report = verify(&tampered);
    assert!(!report.ok);
    let (tick, expected, actual) = report.first_mismatch.expect("mismatch");
    assert_eq!(
        tick, 500,
        "command at tick 100 shows at the first checkpoint"
    );
    assert_eq!(expected, replay.checkpoints[0].1);
    assert_ne!(actual, expected);
    assert_eq!(report.checked, 1);

    let mut bad_hash = replay.clone();
    bad_hash.checkpoints[3].1 ^= 1;
    let report = verify(&bad_hash);
    assert_eq!(report.first_mismatch.map(|m| m.0), Some(2000));
    assert_eq!(report.checked, 4);

    let mut unknown = replay;
    unknown.scenario = "nope".into();
    let report = verify(&unknown);
    assert!(!report.ok);
    assert!(report.error.is_some());
}

#[test]
fn replay_same_tick_commands_ordered() {
    let mut app = village(1);
    let stockpile = ids(&app).stockpile.0;
    let food = |food| SimCommand::content(&SetFood { stockpile, food });
    assert_eq!(send(&mut app, Some(0), food(2)), (0, 0));
    assert_eq!(send(&mut app, Some(0), food(9)), (0, 1));
    assert_eq!(send(&mut app, Some(0), food(6)), (0, 2));
    let replay = record(&mut app, 1000, 100);
    assert_eq!(
        replay.commands,
        [(0, 0, food(2)), (0, 1, food(9)), (0, 2, food(6))]
    );
    assert!(verify(&replay).ok);

    // A fresh run applies them in (tick, seq) order: the last one wins.
    let mut fresh = village(1);
    for (t, _, cmd) in &replay.commands {
        send(&mut fresh, Some(*t), cmd.clone());
    }
    run_ticks(fresh.world_mut(), 1);
    assert_eq!(common::food(&fresh), 6);

    // Vec order does not matter; (tick, seq) does.
    let mut reversed = replay.clone();
    reversed.commands.reverse();
    assert!(verify(&reversed).ok);
    let mut swapped = replay.clone();
    swapped.commands[1].1 = 2;
    swapped.commands[2].1 = 1;
    assert!(!verify(&swapped).ok);
}

#[test]
fn diverge_seeds_reports_first_tick_and_entity() {
    let a = Replay::new(1, "village", 1, 5000);
    let b = Replay::new(2, "village", 1, 5000);
    let d = diverge(&a, &b).expect("seeds 1 and 2 diverge");
    assert!(d.tick > 0, "{d:?}");
    assert!(!d.entities.is_empty());
    let v = d
        .entities
        .iter()
        .find(|e| e.sim_id == ids(&village(1)).villager)
        .expect("villager differs");
    assert!(!v.components.is_empty());
    for c in &v.components {
        assert_ne!(v.a.get(c), v.b.get(c), "component {c} listed but equal");
    }
    // Hashes agree on every tick before `d.tick`.
    let mut aa = village(1);
    let mut bb = village(2);
    run_ticks(aa.world_mut(), d.tick - 1);
    run_ticks(bb.world_mut(), d.tick - 1);
    assert_eq!(
        npc_framework::kernel::hash::state_hash(aa.world_mut()),
        npc_framework::kernel::hash::state_hash(bb.world_mut())
    );
    let json = serde_json::to_value(&d).unwrap();
    assert!(json["entities"][0]["components"].is_array());
}

#[test]
fn diverge_identical_is_none() {
    let a = Replay::new(1, "village", 1, 2000);
    assert!(diverge(&a, &a.clone()).is_none());
    let mut app = with_commands();
    let r = record(&mut app, 2000, 0);
    assert_eq!(
        r.checkpoints.iter().map(|c| c.0).collect::<Vec<_>>(),
        [2000]
    );
    assert!(diverge(&r, &r.clone()).is_none());
    // The same seed with and without commands diverges right after the first.
    let d = diverge(&a, &r).expect("commands change the run");
    assert_eq!(d.tick, 101);
}

#[test]
fn new_sim_app_scenarios() {
    assert!(try_new_sim_app(1, "nope", 1).is_err());
    let app = village(1);
    assert_eq!(ids(&app).villager, SimId(5));
    // house, bed, stockpile, field, villager, well
    assert_eq!(app.world().resource::<SimIndex>().map.len(), 6);
}
