//! `SimRunner` (inline and threaded), the Test, Replay and loopback Lockstep drivers, and
//! snapshot publishing (spec §5.3, ADR-0002).

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use bevy_app::App;
use bevy_ecs::prelude::*;
use fl_math::{Angle, Fx, FxVec2};
use fl_protocol::{
    CommandPayload, DebugInvulnerableCmd, DebugSetHpCmd, DebugSpawnCmd, DebugTeleportCmd,
    DebugWanderCmd, MoveCmd, PlayerCommand, PlayerId, SimId, StopCmd, Tick,
};
use fl_sim::{
    ActSet, CommandRegistry, Content, Health, Invulnerable, Journal, LockstepDriver, MatchSetup,
    MoveOrder, Order, Owner, PathFollow, PathState, Pose, ReplayDriver, RunConfig, ScaleScenario,
    SimBuilder, SimClock, SimCounters, SimEntity, SimFeature, SimIndex, SimRunner, TestDriver,
    UnitType, build_sim_app, load_content, loopback, setup_scenario,
};
use fl_view::{
    CommandDraft, EntityRecord, FromRunner, SimEvent, SimQuery, SimReply, SimSnapshot, TickDriver,
    TickInput, TickOutput, ToRunner, flags,
};

const RECV_TIMEOUT: Duration = Duration::from_secs(10);

fn core_mod() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/core_mod")
}

/// A sim with core_mod, `features`, cheats on and the state hash every tick, nothing set up.
fn empty_app(features: Vec<Arc<dyn SimFeature>>) -> App {
    let content = load_content(&features, &[core_mod()]).expect("core_mod loads");
    let setup = MatchSetup {
        seed: 7,
        cheats_allowed: true,
    };
    let run = RunConfig {
        hash_interval: Some(1),
        ..RunConfig::default()
    };
    build_sim_app(&features, content, setup, run).expect("the sim builds")
}

/// The duel: #1 (player 0) at (100, 100), #2 (player 1) at (900, 900).
fn duel_with(features: Vec<Arc<dyn SimFeature>>) -> SimRunner {
    let mut app = empty_app(features);
    let ids = setup_scenario(app.world_mut(), "core_test.duel").expect("duel sets up");
    assert_eq!(ids, [SimId(1), SimId(2)]);
    SimRunner::new(app)
}

fn duel() -> SimRunner {
    duel_with(Vec::new())
}

fn v(x: i32, y: i32) -> FxVec2 {
    FxVec2::new(Fx::from_int(x), Fx::from_int(y))
}

fn draft<P: CommandPayload>(
    runner: &SimRunner,
    player: u8,
    units: &[u32],
    payload: &P,
) -> CommandDraft {
    CommandDraft {
        player: PlayerId(player),
        units: units.iter().map(|&id| SimId(id)).collect(),
        kind: runner.world().resource::<CommandRegistry>().encode(payload),
        queue: false,
    }
}

fn wander_100(runner: &SimRunner) -> CommandDraft {
    let radius = Fx::from_int(100);
    draft(runner, 0, &[1, 2], &DebugWanderCmd { radius })
}

fn hashes(outputs: &[TickOutput]) -> Vec<(Tick, u64)> {
    outputs
        .iter()
        .map(|out| (out.tick, out.hash.expect("hash interval is 1")))
        .collect()
}

/// The targets of #`id`'s orders, all moves.
fn waypoints(runner: &SimRunner, id: u32) -> Vec<FxVec2> {
    let world = runner.world();
    let entity = world.resource::<SimIndex>().get(SimId(id)).expect("alive");
    world
        .get::<MoveOrder>(entity)
        .expect("unit has a MoveOrder")
        .orders
        .iter()
        .map(|order| match order {
            Order::Move { target } => *target,
            other => panic!("not a move: {other:?}"),
        })
        .collect()
}

fn recv(port_rx: &crossbeam_channel::Receiver<FromRunner>) -> FromRunner {
    port_rx
        .recv_timeout(RECV_TIMEOUT)
        .expect("the runner answers")
}

/// The snapshot of `world` built from scratch: one record per entity with `SimEntity`,
/// `UnitType`, `Owner` and `Pose`, in `SimId` order.
fn publish_full(world: &World) -> SimSnapshot {
    let mut entities = Vec::new();
    for (sim_id, entity) in world.resource::<SimIndex>().iter() {
        let e = world.entity(entity);
        let (Some(unit), Some(owner), Some(pose)) =
            (e.get::<UnitType>(), e.get::<Owner>(), e.get::<Pose>())
        else {
            continue;
        };
        assert_eq!(e.get::<SimEntity>().map(SimEntity::id), Some(sim_id));
        let hp_frac = e
            .get::<Health>()
            .map_or(u16::MAX, |h| EntityRecord::hp_frac(h.hp, h.max));
        let mut bits = 0;
        if e.get::<PathFollow>()
            .is_some_and(|f| matches!(f.state, PathState::Following | PathState::Flow { .. }))
        {
            bits |= flags::MOVING;
        }
        if e.contains::<Invulnerable>() {
            bits |= flags::INVULNERABLE;
        }
        entities.push(EntityRecord {
            sim_id,
            unit: unit.0,
            owner: owner.0,
            pos: pose.pos,
            heading: pose.heading,
            turrets: Vec::new(),
            hp_frac,
            flags: bits,
            visible_to: 0xFF,
        });
    }
    SimSnapshot {
        tick: Tick(world.resource::<SimClock>().tick.0 - 1),
        entities,
        channels: Vec::new(),
    }
}

/// Panics with "boom" in tick 5.
struct BoomFeature;

impl SimFeature for BoomFeature {
    fn id(&self) -> &'static str {
        "boom"
    }

    fn version(&self) -> u32 {
        1
    }

    fn build(&self, sim: &mut SimBuilder) {
        sim.add_systems(ActSet::Fire, |clock: Res<SimClock>| {
            assert!(clock.tick != Tick(5), "boom");
        });
    }
}

#[test]
fn inline_and_threaded_hash_streams_equal() {
    let mut inline = duel();
    let mut driver = TestDriver::new();
    driver.issue(wander_100(&inline));
    let expected = hashes(&inline.run(&mut driver, 200));
    assert_eq!(expected.len(), 200);

    let threaded = duel();
    let mut driver = TestDriver::new();
    driver.issue(wander_100(&threaded));
    let (port, handle) = threaded.spawn_threaded();
    for _ in 0..200 {
        let input = driver.poll().expect("the test driver always ticks");
        port.tx
            .send(ToRunner::Tick(input))
            .expect("the runner is alive");
    }
    let mut actual = Vec::new();
    for _ in 0..200 {
        match recv(&port.rx) {
            FromRunner::Tick(out) => actual.push((out.tick, out.hash.expect("hash every tick"))),
            other => panic!("unexpected message {other:?}"),
        }
    }
    assert_eq!(actual, expected);
    // The units wandered, so the stream is not trivially constant.
    assert_ne!(expected[1].1, expected[199].1);

    port.tx
        .send(ToRunner::Shutdown)
        .expect("the runner is alive");
    handle.join().expect("the runner thread exits cleanly");
}

#[test]
fn threaded_panic_is_reported() {
    let runner = duel_with(vec![Arc::new(BoomFeature)]);
    let (port, handle) = runner.spawn_threaded();
    for tick in 0..8 {
        // The runner stops at tick 5, so the last sends may fail; that is fine.
        let _ = port.tx.send(ToRunner::Tick(TickInput {
            tick: Tick(tick),
            commands: Vec::new(),
        }));
    }
    for tick in 0..5 {
        match recv(&port.rx) {
            FromRunner::Tick(out) => assert_eq!(out.tick, Tick(tick)),
            other => panic!("unexpected message {other:?}"),
        }
    }
    match recv(&port.rx) {
        FromRunner::Failed(failure) => {
            assert_eq!(failure.tick, Tick(5));
            assert!(
                failure.message.contains("boom"),
                "message: {}",
                failure.message
            );
        }
        other => panic!("expected Failed, got {other:?}"),
    }
    handle.join().expect("the panic is caught, not propagated");
    assert!(
        port.rx.recv_timeout(RECV_TIMEOUT).is_err(),
        "nothing follows a failure"
    );
}

#[test]
fn drop_port_stops_thread() {
    let (port, handle) = duel().spawn_threaded();
    drop(port);
    for _ in 0..200 {
        if handle.is_finished() {
            handle.join().expect("the runner thread exits cleanly");
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("the runner thread is still running 2 s after the port was dropped");
}

#[test]
fn query_between_ticks() {
    let mut runner = duel();
    runner.run(&mut TestDriver::new(), 10);
    let SimReply::Entity(Some(json)) = runner.query(SimQuery::Entity(SimId(1))) else {
        panic!("entity 1 exists");
    };
    assert!(json["components"]["core.pose"].is_object(), "{json}");
    assert_eq!(
        runner.query(SimQuery::Entity(SimId(99))),
        SimReply::Entity(None)
    );

    // The same through the threaded runner: replies carry the query's id.
    let (port, handle) = runner.spawn_threaded();
    for (id, query) in [(7, SimQuery::Entity(SimId(2))), (8, SimQuery::WorldSummary)] {
        port.tx
            .send(ToRunner::Query { id, query })
            .expect("the runner is alive");
    }
    match recv(&port.rx) {
        FromRunner::Reply {
            id: 7,
            reply: SimReply::Entity(Some(json)),
        } => assert_eq!(json["sim_id"], 2),
        other => panic!("unexpected message {other:?}"),
    }
    match recv(&port.rx) {
        FromRunner::Reply {
            id: 8,
            reply: SimReply::WorldSummary(json),
        } => assert_eq!(json["tick"], 10),
        other => panic!("unexpected message {other:?}"),
    }
    drop(port);
    handle.join().expect("the runner thread exits cleanly");
}

#[test]
fn other_queries_answer() {
    let mut runner = duel();
    let mut driver = TestDriver::new();
    let move_to = draft(
        &runner,
        1,
        &[2],
        &MoveCmd {
            target: v(500, 500),
        },
    );
    driver.issue(move_to);
    runner.run(&mut driver, 3);

    let SimReply::Perf(perf) = runner.query(SimQuery::Perf) else {
        panic!("perf answers");
    };
    assert_eq!(perf["counters"]["publish.records"], 2, "{perf}");

    let SimReply::Header(header) = runner.query(SimQuery::Header) else {
        panic!("header answers");
    };
    assert_eq!(header.seed, 7);
    assert!(header.cheats_allowed);

    // The move's journal records (the journal is on in debug builds).
    let SimReply::Journal(journal) = runner.query(SimQuery::JournalSince {
        tick: Tick(0),
        limit: 10,
    }) else {
        panic!("the journal answers");
    };
    let kinds: Vec<&str> = journal.iter().map(|e| e.kind.as_str()).collect();
    let expected: &[&str] = if runner.world().resource::<Journal>().enabled() {
        &[
            "core.command_applied",
            "core.order_changed",
            "core.path_request",
            "core.path_result",
        ]
    } else {
        &[]
    };
    assert_eq!(kinds, expected);
    assert!(matches!(
        runner.query(SimQuery::Save {
            pending: Vec::new()
        }),
        SimReply::Save(bytes) if !bytes.is_empty()
    ));

    // The world summary carries the last seq per player, for CommandStamper::restore.
    let SimReply::WorldSummary(summary) = runner.query(SimQuery::WorldSummary) else {
        panic!("world summary answers");
    };
    assert_eq!(summary["last_seq"], serde_json::json!({ "1": 0 }));
}

#[test]
fn test_driver_delay_zero_applies_same_tick() {
    let mut runner = duel();
    let mut driver = TestDriver::new();
    runner.run(&mut driver, 3);
    assert_eq!(driver.next_tick(), Tick(3));
    let cmd = driver.issue(draft(
        &runner,
        0,
        &[1],
        &MoveCmd {
            target: v(300, 300),
        },
    ));
    assert_eq!(cmd.exec_tick, Tick(3));
    assert_eq!(driver.pending(), std::slice::from_ref(&cmd));

    let out = runner.run(&mut driver, 1);
    assert_eq!(out[0].tick, Tick(3));
    assert_eq!(waypoints(&runner, 1), [v(300, 300)]);
    assert!(driver.pending().is_empty());
}

#[test]
fn delay_one_applies_next_tick() {
    let mut runner = duel();
    let mut driver = TestDriver::with_delay(1);
    assert_eq!(driver.input_delay(), 1);
    runner.run(&mut driver, 3);
    let cmd = driver.issue(draft(
        &runner,
        0,
        &[1],
        &MoveCmd {
            target: v(300, 300),
        },
    ));
    assert_eq!(cmd.exec_tick, Tick(4));

    runner.run(&mut driver, 1);
    assert!(waypoints(&runner, 1).is_empty(), "not applied in tick 3");
    runner.run(&mut driver, 1);
    assert_eq!(waypoints(&runner, 1), [v(300, 300)]);
}

#[test]
fn test_driver_starting_at_and_restore() {
    let mut driver = TestDriver::new().starting_at(Tick(40));
    assert_eq!(driver.next_tick(), Tick(40));
    let held = PlayerCommand {
        exec_tick: Tick(41),
        player: PlayerId(0),
        seq: 6,
        units: vec![SimId(1)],
        kind: fl_protocol::CommandKind {
            type_id: fl_protocol::CommandTypeId(0),
            payload: Vec::new(),
        },
        queue: false,
    };
    let last = [(PlayerId(0), 4), (PlayerId(1), 9)].into_iter().collect();
    driver.restore(vec![held.clone()], &last);
    assert_eq!(driver.pending(), std::slice::from_ref(&held));

    let next = driver.issue(CommandDraft {
        player: PlayerId(1),
        units: Vec::new(),
        kind: held.kind.clone(),
        queue: false,
    });
    assert_eq!((next.exec_tick, next.seq), (Tick(40), 10));
    let p0 = driver.issue(CommandDraft {
        player: PlayerId(0),
        units: Vec::new(),
        kind: held.kind.clone(),
        queue: false,
    });
    assert_eq!(p0.seq, 7, "after the highest pending seq");

    assert_eq!(driver.poll().expect("ticks").commands, [p0, next]);
    assert_eq!(driver.poll().expect("ticks").commands, [held]);
}

#[test]
fn replay_driver_reproduces_hashes() {
    let mut runner = duel();
    let mut driver = TestDriver::new();
    let mut inputs = Vec::new();
    let mut expected = Vec::new();
    for tick in 0..150 {
        match tick {
            5 => {
                driver.issue(wander_100(&runner));
            }
            40 => {
                driver.issue(draft(&runner, 1, &[2], &MoveCmd { target: v(10, 990) }));
            }
            90 => {
                driver.issue(draft(&runner, 0, &[1], &StopCmd));
            }
            _ => {}
        }
        let input = driver.poll().expect("the test driver always ticks");
        if !input.commands.is_empty() {
            inputs.push(input.clone());
        }
        expected.push(runner.step(input));
    }
    assert_eq!(inputs.len(), 3, "only ticks with commands are recorded");

    let mut replayed = duel();
    let outputs = replayed.run(&mut ReplayDriver::new(inputs, Tick(0), None), 150);
    assert_eq!(hashes(&outputs), hashes(&expected));
}

#[test]
fn replay_driver_stops_at_until() {
    let mut replay = ReplayDriver::new(Vec::new(), Tick(3), Some(Tick(5)));
    assert_eq!(replay.next_tick(), Tick(3));
    assert_eq!(replay.poll().map(|i| i.tick), Some(Tick(3)));
    assert_eq!(replay.poll().map(|i| i.tick), Some(Tick(4)));
    assert_eq!(replay.poll(), None);
    assert_eq!(replay.next_tick(), Tick(5));
}

#[test]
#[should_panic(expected = "driver stalled at tick 2")]
fn run_panics_when_the_driver_stalls() {
    let mut runner = duel();
    runner.run(
        &mut ReplayDriver::new(Vec::new(), Tick(0), Some(Tick(2))),
        3,
    );
}

#[test]
fn loopback_reaches_every_session() {
    let players = [PlayerId(0), PlayerId(1)];
    let mut sessions = loopback(&players);
    fl_sim::NetSession::send(&mut sessions[1], Tick(4), PlayerId(1), Vec::new());
    for session in &mut sessions {
        assert_eq!(
            fl_sim::NetSession::recv(session),
            [(Tick(4), PlayerId(1), Vec::new())]
        );
        assert!(fl_sim::NetSession::recv(session).is_empty());
    }
}

#[test]
fn loopback_lockstep_two_sims_identical() {
    let players = vec![PlayerId(0), PlayerId(1)];
    let mut sessions = loopback(&players).into_iter();
    let mut drivers = [
        LockstepDriver::new(PlayerId(0), players.clone(), sessions.next().unwrap(), 2),
        LockstepDriver::new(PlayerId(1), players.clone(), sessions.next().unwrap(), 2),
    ];
    let mut runners = [duel(), duel()];
    let mut inputs: [Vec<TickInput>; 2] = [Vec::new(), Vec::new()];
    let mut outputs: [Vec<TickOutput>; 2] = [Vec::new(), Vec::new()];

    for tick in 0..300 {
        for side in 0..2 {
            assert_eq!(drivers[side].next_tick(), Tick(tick));
            if side == 0 && tick == 10 {
                let cmd = drivers[0].issue(draft(
                    &runners[0],
                    0,
                    &[1],
                    &MoveCmd {
                        target: v(500, 100),
                    },
                ));
                assert_eq!(cmd.exec_tick, Tick(12));
            }
            if side == 1 && tick == 37 {
                let cmd = drivers[1].issue(draft(
                    &runners[1],
                    1,
                    &[2],
                    &MoveCmd {
                        target: v(500, 900),
                    },
                ));
                assert_eq!(cmd.exec_tick, Tick(39));
            }
            let input = drivers[side]
                .poll()
                .unwrap_or_else(|| panic!("side {side} stalled at tick {tick}"));
            inputs[side].push(input.clone());
            outputs[side].push(runners[side].step(input));
        }
    }

    assert_eq!(hashes(&outputs[0]), hashes(&outputs[1]));
    for side_inputs in &inputs {
        let with_commands: Vec<(u32, Vec<(PlayerId, u32)>)> = side_inputs
            .iter()
            .filter(|input| !input.commands.is_empty())
            .map(|input| {
                let ids = input.commands.iter().map(|c| (c.player, c.seq)).collect();
                (input.tick.0, ids)
            })
            .collect();
        assert_eq!(
            with_commands,
            [(12, vec![(PlayerId(0), 0)]), (39, vec![(PlayerId(1), 0)])]
        );
    }
    // Both moves applied.
    assert_ne!(waypoints(&runners[0], 1), Vec::new());
    assert_eq!(runners[0].world().resource::<SimClock>().tick, Tick(300));
}

#[test]
fn lockstep_waits_for_missing_peer() {
    let players = vec![PlayerId(0), PlayerId(1)];
    // Player 1's session exists but never sends.
    let mut sessions = loopback(&players).into_iter();
    let mut driver = LockstepDriver::new(PlayerId(0), players, sessions.next().unwrap(), 2);
    let _silent_peer = sessions.next().unwrap();
    assert_eq!(driver.poll().map(|i| i.tick), Some(Tick(0)));
    assert_eq!(driver.poll().map(|i| i.tick), Some(Tick(1)));
    assert_eq!(driver.poll(), None);
    assert_eq!(driver.poll(), None, "still waiting");
    assert_eq!(driver.next_tick(), Tick(2));
}

#[test]
fn lockstep_command_issued_while_waiting_goes_to_the_next_batch() {
    let players = vec![PlayerId(0), PlayerId(1)];
    let mut sessions = loopback(&players).into_iter();
    let mut d0 = LockstepDriver::new(PlayerId(0), players.clone(), sessions.next().unwrap(), 2);
    let mut d1 = LockstepDriver::new(PlayerId(1), players, sessions.next().unwrap(), 2);
    let runner = duel();
    let stop = || draft(&runner, 0, &[1], &StopCmd);

    // d0 runs ahead to tick 2 and waits for d1's batch for it; its batch for 4 is sent.
    assert!(d0.poll().is_some());
    assert!(d0.poll().is_some());
    assert_eq!(d0.poll(), None);
    // Issued while waiting: the batch for tick 4 is gone, so the command goes into tick 5.
    let cmd = d0.issue(stop());
    assert_eq!(cmd.exec_tick, Tick(5));
    assert_eq!(d0.pending(), std::slice::from_ref(&cmd));

    let mut d1_inputs = Vec::new();
    for _ in 0..6 {
        d1_inputs.push(d1.poll().expect("d0 sent every batch d1 needs so far"));
        let _ = d0.poll();
        let _ = d0.poll();
    }
    assert_eq!(d1_inputs[5].tick, Tick(5));
    assert_eq!(d1_inputs[5].commands, [cmd]);
}

#[test]
fn snapshot_matches_world() {
    let mut runner = duel();
    let mut driver = TestDriver::new();
    driver.issue(wander_100(&runner));
    let outputs = runner.run(&mut driver, 12);
    let snapshot = &outputs[11].snapshot;
    assert_eq!(snapshot.tick, Tick(11));

    let world = runner.world();
    let index = world.resource::<SimIndex>();
    assert_eq!(snapshot.entities.len(), index.len());
    let pose = *world
        .get::<Pose>(index.get(SimId(1)).expect("alive"))
        .expect("unit has a Pose");
    let record = snapshot.get(SimId(1)).expect("record 1");
    assert_eq!(record.pos, pose.pos);
    assert_eq!(record.heading, pose.heading);
    assert_eq!(record.owner, PlayerId(0));
    assert_eq!(record.hp_frac, u16::MAX);
    assert_eq!(record.visible_to, 0xFF);
    assert_eq!(record.flags & flags::MOVING, flags::MOVING);
    assert!(
        snapshot
            .entities
            .windows(2)
            .all(|pair| pair[0].sim_id < pair[1].sim_id)
    );
}

#[test]
fn incremental_publish_equals_full_rebuild() {
    let mut app = empty_app(Vec::new());
    let ids = ScaleScenario {
        units: 500,
        players: 2,
        seed: 7,
        ..ScaleScenario::default()
    }
    .populate(app.world_mut())
    .expect("scale populates");
    let mut runner = SimRunner::new(app);
    let some: Vec<u32> = ids.iter().step_by(7).map(|id| id.0).collect();
    let mut driver = TestDriver::new();

    for tick in 0..200 {
        let script: Vec<CommandDraft> = match tick {
            // `some` stop for good, so from tick 16 on their records are copied unless one
            // of the commands below touches them.
            10 => vec![draft(
                &runner,
                0,
                &some,
                &DebugWanderCmd { radius: Fx::ZERO },
            )],
            // Each to a point of its own, 12 m apart: units teleported onto one point would
            // push apart for the rest of the run (separation steering, Task 11).
            15 => some
                .iter()
                .zip(0..)
                .map(|(&id, i): (&u32, i32)| {
                    draft(
                        &runner,
                        0,
                        &[id],
                        &DebugTeleportCmd {
                            pos: v(500 + 12 * (i % 9), 500 + 12 * (i / 9)),
                        },
                    )
                })
                .collect(),
            30 => vec![draft(&runner, 0, &some, &DebugSetHpCmd { hp: 30 })],
            40 => vec![draft(&runner, 0, &some, &DebugInvulnerableCmd { on: true })],
            // A removal is not a change: the publish must still notice it.
            60 => vec![draft(
                &runner,
                0,
                &some,
                &DebugInvulnerableCmd { on: false },
            )],
            120 => vec![draft(
                &runner,
                0,
                &[],
                &DebugSpawnCmd {
                    unit: "core_test.tank".to_owned(),
                    owner: PlayerId(1),
                    pos: v(10, 10),
                    heading: Angle::ZERO,
                },
            )],
            _ => Vec::new(),
        };
        for script in script {
            driver.issue(script);
        }
        let input = driver.poll().expect("the test driver always ticks");
        let out = runner.step(input);
        assert!(
            out.events
                .iter()
                .all(|e| !matches!(e, SimEvent::CommandRejected { .. }))
        );
        assert_eq!(*out.snapshot, publish_full(runner.world()), "tick {tick}");
    }
    let last = publish_full(runner.world());
    assert_eq!(last.entities.len(), 501);
    let counters = runner.world().resource::<SimCounters>();
    assert_eq!(counters.get("publish.records"), 501);
    let rebuilt = counters.get("publish.rebuilt");
    assert!(
        rebuilt > 0 && rebuilt <= 501 - some.len() as u64,
        "only moving units are rebuilt, got {rebuilt}"
    );
}

#[test]
fn snapshot_buffers_are_recycled() {
    let mut runner = duel();
    let mut driver = TestDriver::new();
    let out0 = runner.run(&mut driver, 1).remove(0);
    let first = Arc::as_ptr(&out0.snapshot);
    drop(out0);
    drop(runner.run(&mut driver, 1));
    let out2 = runner.run(&mut driver, 1).remove(0);
    assert_eq!(Arc::as_ptr(&out2.snapshot), first);
    assert_eq!(out2.snapshot.tick, Tick(2));

    // A snapshot the client still holds is never overwritten.
    let held = runner.run(&mut driver, 1).remove(0);
    let held_tick = held.snapshot.tick;
    runner.run(&mut driver, 3);
    assert_eq!(held.snapshot.tick, held_tick);
}

#[test]
fn events_drained_per_tick() {
    let mut runner = duel();
    let mut driver = TestDriver::new();
    driver.issue(draft(
        &runner,
        0,
        &[],
        &DebugSpawnCmd {
            unit: "core_test.tank".to_owned(),
            owner: PlayerId(0),
            pos: v(200, 200),
            heading: Angle::ZERO,
        },
    ));
    let tank = runner
        .world()
        .resource::<Content>()
        .0
        .unit_id("core_test.tank")
        .expect("the tank exists");
    let outputs = runner.run(&mut driver, 2);
    let spawned: Vec<&SimEvent> = outputs[0]
        .events
        .iter()
        .filter(|e| matches!(e, SimEvent::Spawned { .. }))
        .collect();
    assert_eq!(
        spawned,
        [&SimEvent::Spawned {
            sim_id: SimId(3),
            unit: Some(tank),
            owner: PlayerId(0),
        }]
    );
    assert!(outputs[1].events.is_empty());
    assert_eq!(outputs[0].snapshot.entities.len(), 3);
}
