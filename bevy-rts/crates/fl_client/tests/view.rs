//! Client view tests: `LocalDriver`, the runner link and snapshot-driven visuals.
//!
//! The client never sees `fl_sim`: these tests drive `SimViewPlugin` with a fake runner built on
//! `fl_view::runner_channel` that answers each `ToRunner::Tick` with a hand-made `TickOutput`.

use std::f32::consts::PI;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use fl_client::driver::LocalDriver;
use fl_client::link::{
    JournalBuffer, PendingReplies, RunnerLink, RunnerStatus, SimEventMsg, SimJournalMsg,
    SnapshotAge, SnapshotBuffer, encode_command, send_query,
};
use fl_client::placeholder::{PlaceholderPlugin, PlayerColors, placeholder_parts};
use fl_client::view::{
    RevealAll, SimViewPlugin, UnitView, ViewIndex, ViewPose, heading_to_yaw,
    interpolation_fraction, lerp_yaw, sim_to_world,
};
use fl_content::Placeholder;
use fl_math::{Angle, Fx, FxVec2};
use fl_protocol::{
    CommandTypeId, DebugSpawnCmd, MatchHeader, MoveCmd, PlayerId, SimId, StopCmd, StringTables,
    Tick, UnitTypeId, tables,
};
use fl_view::{
    CommandDraft, EntityRecord, FromRunner, JournalEntry, RunnerFailure, RunnerPort, SimEvent,
    SimQuery, SimReply, SimSnapshot, TickDriver, TickInput, TickOutput, ToRunner, runner_channel,
};

const FRAME: Duration = Duration::from_millis(50);
const ACK_TIMEOUT: Duration = Duration::from_secs(10);

fn record(id: u32, x: i32, y: i32) -> EntityRecord {
    EntityRecord {
        sim_id: SimId(id),
        unit: UnitTypeId(0),
        owner: PlayerId(0),
        pos: FxVec2::new(Fx::from_int(x), Fx::from_int(y)),
        heading: Angle::from_raw(0),
        turrets: Vec::new(),
        hp_frac: u16::MAX,
        flags: 0,
        visible_to: 0xFF,
    }
}

fn output(tick: u32, entities: Vec<EntityRecord>, events: Vec<SimEvent>) -> TickOutput {
    TickOutput {
        tick: Tick(tick),
        hash: Some(u64::from(tick) * 7),
        snapshot: Arc::new(SimSnapshot {
            tick: Tick(tick),
            entities,
            channels: Vec::new(),
        }),
        events,
        journal: Vec::new(),
    }
}

fn header() -> MatchHeader {
    let mut string_tables = StringTables::default();
    string_tables.insert(
        tables::COMMANDS,
        vec!["core.move".to_owned(), "core.stop".to_owned()],
    );
    MatchHeader {
        format_version: fl_protocol::FORMAT_VERSION,
        build_id: "test".to_owned(),
        sim_version: 1,
        features: Vec::new(),
        mods: Vec::new(),
        rules_hash: 0,
        cheats_allowed: true,
        seed: 1,
        string_tables,
    }
}

fn move_draft(player: u8) -> CommandDraft {
    let kind = encode_command(
        &header().string_tables,
        &MoveCmd {
            target: FxVec2::new(Fx::from_int(10), Fx::from_int(20)),
        },
    )
    .unwrap();
    CommandDraft {
        player: PlayerId(player),
        units: vec![SimId(1)],
        kind,
        queue: false,
    }
}

/// A headless client: `MinimalPlugins` + `SimViewPlugin`, `frame` of real time per update.
fn client_app(port: RunnerPort, next: Tick, frame: Duration) -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, SimViewPlugin))
        .insert_resource(TimeUpdateStrategy::ManualDuration(frame))
        .insert_resource(RunnerLink {
            port,
            header: header(),
        })
        .insert_resource(LocalDriver::new(next));
    app
}

fn unit_views(app: &mut App) -> usize {
    let world = app.world_mut();
    world.query::<&UnitView>().iter(world).count()
}

/// A runner thread that answers every `ToRunner::Tick` with `respond(&input)`, records the
/// inputs, and acknowledges each reply so a test can wait until a frame's ticks are answered.
///
/// A tick's answer is held until [`FakeRunner::update`] releases it after the frame that sent
/// the tick has finished, so the client always receives it in the next frame. Without this the
/// answer could race the same frame's drain, and a test would see a snapshot one tick early.
struct FakeRunner {
    inputs: Arc<Mutex<Vec<TickInput>>>,
    release: Option<crossbeam_channel::Sender<()>>,
    acks: crossbeam_channel::Receiver<()>,
    shutdown: crossbeam_channel::Sender<ToRunner>,
    thread: Option<JoinHandle<()>>,
}

impl FakeRunner {
    /// Spawns the runner thread; returns it and the client's port.
    fn spawn(respond: impl Fn(&TickInput) -> TickOutput + Send + 'static) -> (Self, RunnerPort) {
        let (port, end) = runner_channel();
        let inputs = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&inputs);
        let (ack_tx, acks) = crossbeam_channel::unbounded();
        let (release, released) = crossbeam_channel::unbounded::<()>();
        let thread = std::thread::spawn(move || {
            while let Ok(message) = end.rx.recv() {
                match message {
                    ToRunner::Tick(input) => {
                        let out = respond(&input);
                        recorded.lock().unwrap().push(input);
                        if released.recv().is_err() || end.tx.send(FromRunner::Tick(out)).is_err() {
                            return;
                        }
                        let _ = ack_tx.send(());
                    }
                    ToRunner::Query { .. } => {}
                    ToRunner::Shutdown => return,
                    // `ToRunner` is non-exhaustive; this fake never receives a `Control`.
                    _ => {}
                }
            }
        });
        let runner = FakeRunner {
            inputs,
            release: Some(release),
            acks,
            shutdown: port.tx.clone(),
            thread: Some(thread),
        };
        (runner, port)
    }

    /// One `app.update()`, then releases the answer to every tick it sent and waits until each
    /// is in the client's channel, for the next update to receive.
    fn update(&self, app: &mut App) {
        let before = app.world().resource::<LocalDriver>().next_tick();
        app.update();
        let after = app.world().resource::<LocalDriver>().next_tick();
        let release = self.release.as_ref().expect("released until drop");
        for _ in before.0..after.0 {
            release.send(()).expect("the fake runner thread is alive");
            self.acks
                .recv_timeout(ACK_TIMEOUT)
                .expect("the fake runner answers every tick");
        }
    }

    fn inputs(&self) -> Vec<TickInput> {
        self.inputs.lock().unwrap().clone()
    }
}

impl Drop for FakeRunner {
    fn drop(&mut self) {
        // Unblocks a thread still holding an answer.
        self.release = None;
        let _ = self.shutdown.send(ToRunner::Shutdown);
        if let Some(thread) = self.thread.take() {
            thread
                .join()
                .expect("the fake runner thread does not panic");
        }
    }
}

// --- pure conversions ---

#[test]
fn sim_to_world_axes() {
    let pos = FxVec2::new(Fx::from_int(3), Fx::from_int(7));
    assert_eq!(sim_to_world(pos, 2.0), Vec3::new(3.0, 2.0, 7.0));
}

#[test]
fn heading_90_faces_plus_z() {
    let yaw = heading_to_yaw(Angle::from_degrees(Fx::from_int(90)));
    let forward = Quat::from_rotation_y(yaw) * Vec3::X;
    assert!((forward - Vec3::Z).length() < 1e-5, "{forward:?}");
}

#[test]
fn lerp_yaw_wraps() {
    let mid = lerp_yaw(170f32.to_radians(), (-170f32).to_radians(), 0.5);
    assert!((mid.abs() - PI).abs() < 1e-5, "{mid}");
}

#[test]
fn interpolation_midpoint() {
    let (port, _end) = runner_channel();
    let mut app = client_app(port, Tick(0), Duration::from_millis(25));
    let entity = app
        .world_mut()
        .spawn((
            ViewPose {
                prev: Vec3::ZERO,
                curr: Vec3::new(10.0, 0.0, 0.0),
                prev_yaw: 0.0,
                curr_yaw: 0.0,
            },
            Transform::default(),
        ))
        .id();
    // The first update has no elapsed time; the second adds 25 ms, half a tick period, to the
    // time since the (absent) current snapshot arrived.
    app.update();
    app.update();
    let age = app.world().resource::<SnapshotAge>().0;
    assert_eq!(age, Duration::from_millis(25));
    let x = app.world().get::<Transform>(entity).unwrap().translation.x;
    assert!((x - 5.0).abs() < 1e-3, "{x}");
}

// --- LocalDriver ---

#[test]
fn local_driver_stamps_delay_one() {
    let mut driver = LocalDriver::new(Tick(5));
    assert_eq!(driver.input_delay(), 1);
    let first = driver.issue(move_draft(0));
    let second = driver.issue(move_draft(0));
    assert_eq!((first.exec_tick, first.seq), (Tick(6), 0));
    assert_eq!((second.exec_tick, second.seq), (Tick(6), 1));
    assert_eq!(driver.pending(), vec![first, second]);
}

#[test]
fn local_driver_accumulates_fixed_steps() {
    let mut driver = LocalDriver::new(Tick(0));
    driver.advance(Duration::from_millis(120));
    assert_eq!(driver.poll().map(|input| input.tick), Some(Tick(0)));
    assert_eq!(driver.poll().map(|input| input.tick), Some(Tick(1)));
    assert_eq!(driver.poll(), None);
    assert_eq!(driver.next_tick(), Tick(2));
    let overstep = driver.overstep_fraction();
    assert!((overstep - 0.4).abs() < 1e-3, "{overstep}");
}

#[test]
fn speed_clamped() {
    let mut driver = LocalDriver::new(Tick(0));
    driver.set_speed(100.0);
    assert_eq!(driver.speed(), 8.0);
    driver.set_speed(0.0);
    assert_eq!(driver.speed(), 0.25);
    driver.set_speed(2.0);
    driver.advance(Duration::from_millis(50));
    assert_eq!(driver.poll().map(|input| input.tick), Some(Tick(0)));
    assert_eq!(driver.poll().map(|input| input.tick), Some(Tick(1)));
    assert_eq!(driver.poll(), None);
}

#[test]
fn paused_driver_emits_nothing_until_step() {
    let mut driver = LocalDriver::new(Tick(0));
    driver.pause();
    assert!(driver.is_paused());
    driver.advance(Duration::from_secs(1));
    assert_eq!(driver.poll(), None);
    driver.step(2);
    assert_eq!(driver.poll().map(|input| input.tick), Some(Tick(0)));
    assert_eq!(driver.poll().map(|input| input.tick), Some(Tick(1)));
    assert_eq!(driver.poll(), None);
    assert!(driver.is_paused());
}

#[test]
fn local_driver_restore_continues_seq() {
    let mut driver = LocalDriver::new(Tick(0));
    let pending = vec![fl_protocol::PlayerCommand {
        exec_tick: Tick(41),
        player: PlayerId(0),
        seq: 7,
        units: vec![SimId(1)],
        kind: move_draft(0).kind,
        queue: false,
    }];
    let last_seq = [(PlayerId(0), 6), (PlayerId(1), 2)].into_iter().collect();
    driver.restore(Tick(40), pending.clone(), &last_seq);
    assert_eq!(driver.next_tick(), Tick(40));
    assert_eq!(driver.pending(), pending);
    assert_eq!(driver.issue(move_draft(0)).seq, 8);
    assert_eq!(driver.issue(move_draft(1)).seq, 3);
}

#[test]
fn paused_orders_apply_on_resume() {
    let (runner, port) = FakeRunner::spawn(|input| output(input.tick.0, Vec::new(), Vec::new()));
    let mut app = client_app(port, Tick(10), FRAME);
    app.world_mut().resource_mut::<LocalDriver>().pause();
    for _ in 0..3 {
        runner.update(&mut app);
    }
    assert!(runner.inputs().is_empty(), "a paused driver sends nothing");

    let command = app
        .world_mut()
        .resource_mut::<LocalDriver>()
        .issue(move_draft(0));
    assert_eq!(command.exec_tick, Tick(11));
    app.world_mut().resource_mut::<LocalDriver>().resume();
    for _ in 0..3 {
        runner.update(&mut app);
    }

    let inputs = runner.inputs();
    let ticks: Vec<Tick> = inputs.iter().map(|input| input.tick).collect();
    assert_eq!(&ticks[..2], &[Tick(10), Tick(11)]);
    assert!(inputs[0].commands.is_empty());
    assert_eq!(inputs[1].commands, vec![command]);
    assert!(inputs[2..].iter().all(|input| input.commands.is_empty()));
}

// --- runner link and views ---

#[test]
fn visuals_follow_snapshots() {
    let (runner, port) = FakeRunner::spawn(|input| {
        let records = (0..10)
            .map(|i| record(i, 100 + 10 * i as i32, 200))
            .collect();
        output(input.tick.0, records, Vec::new())
    });
    let mut app = client_app(port, Tick(0), FRAME);
    for _ in 0..3 {
        runner.update(&mut app);
    }
    assert_eq!(unit_views(&mut app), 10);
    let index = &app.world().resource::<ViewIndex>().0;
    assert_eq!(index.len(), 10);
    let entity = index[&SimId(3)];
    let view = *app.world().get::<UnitView>(entity).unwrap();
    assert_eq!(view.sim_id, SimId(3));
    assert_eq!(view.unit, UnitTypeId(0));
    assert_eq!(view.owner, PlayerId(0));
    let pose = *app.world().get::<ViewPose>(entity).unwrap();
    assert_eq!(pose.curr, Vec3::new(130.0, 0.0, 200.0));
}

#[test]
fn despawned_record_removes_visual() {
    let (runner, port) = FakeRunner::spawn(|input| {
        let ids: &[u32] = if input.tick == Tick(0) {
            &[0, 1, 2]
        } else {
            &[0, 1]
        };
        let records = ids.iter().map(|&i| record(i, 10, 10)).collect();
        output(input.tick.0, records, Vec::new())
    });
    let mut app = client_app(port, Tick(0), FRAME);
    for _ in 0..3 {
        runner.update(&mut app);
    }
    assert_eq!(unit_views(&mut app), 3);
    let gone = app.world().resource::<ViewIndex>().0[&SimId(2)];

    for _ in 0..3 {
        runner.update(&mut app);
    }
    assert_eq!(unit_views(&mut app), 2);
    assert!(
        !app.world()
            .resource::<ViewIndex>()
            .0
            .contains_key(&SimId(2))
    );
    assert!(app.world().get_entity(gone).is_err());
}

#[test]
fn multiple_outputs_per_frame_keep_all_events() {
    #[derive(Resource, Default)]
    struct Seen(Vec<SimEvent>);

    fn collect(mut reader: MessageReader<SimEventMsg>, mut seen: ResMut<Seen>) {
        seen.0.extend(reader.read().map(|msg| msg.0.clone()));
    }

    let (port, end) = runner_channel();
    let mut app = client_app(port, Tick(0), FRAME);
    app.init_resource::<Seen>().add_systems(PostUpdate, collect);
    for tick in 0..3 {
        let event = SimEvent::Spawned {
            sim_id: SimId(tick),
            unit: Some(UnitTypeId(0)),
            owner: PlayerId(0),
        };
        let out = output(tick, vec![record(tick, 5, 5)], vec![event]);
        end.tx.send(FromRunner::Tick(out)).unwrap();
    }
    app.update();

    let seen = &app.world().resource::<Seen>().0;
    let ids: Vec<SimId> = seen
        .iter()
        .map(|event| match event {
            SimEvent::Spawned { sim_id, .. } => *sim_id,
            other => panic!("unexpected event {other:?}"),
        })
        .collect();
    assert_eq!(ids, vec![SimId(0), SimId(1), SimId(2)]);
    let buffer = app.world().resource::<SnapshotBuffer>();
    assert_eq!(buffer.prev.as_ref().map(|s| s.tick), Some(Tick(1)));
    assert_eq!(buffer.curr.as_ref().map(|s| s.tick), Some(Tick(2)));
    let status = app.world().resource::<RunnerStatus>();
    assert_eq!(status.last_tick, Some(Tick(2)));
    assert_eq!(status.last_hash, Some(14));
}

#[test]
fn first_frame_without_snapshot_draws_nothing() {
    let (port, _end) = runner_channel();
    let mut app = client_app(port, Tick(0), FRAME);
    app.update();
    app.update();
    assert_eq!(unit_views(&mut app), 0);
    assert!(app.world().resource::<SnapshotBuffer>().curr.is_none());
    assert!(app.world().resource::<RunnerStatus>().failed.is_none());
}

#[test]
fn runner_failure_is_surfaced() {
    let (port, end) = runner_channel();
    let mut app = client_app(port, Tick(0), FRAME);
    let failure = RunnerFailure {
        tick: Tick(4),
        message: "system panicked".to_owned(),
    };
    end.tx.send(FromRunner::Failed(failure.clone())).unwrap();
    app.update();
    assert_eq!(app.world().resource::<RunnerStatus>().failed, Some(failure));
    assert!(app.world().resource::<LocalDriver>().is_paused());
}

#[test]
fn runner_disconnect_is_surfaced() {
    let (port, end) = runner_channel();
    let mut app = client_app(port, Tick(0), FRAME);
    drop(end);
    app.update();
    let status = app.world().resource::<RunnerStatus>();
    let failed = status
        .failed
        .as_ref()
        .expect("a vanished runner is a failure");
    assert!(
        failed.message.contains("disconnected"),
        "{}",
        failed.message
    );
    assert!(app.world().resource::<LocalDriver>().is_paused());
}

#[test]
fn reveal_all_draws_unseen_records() {
    let (port, end) = runner_channel();
    let mut app = client_app(port, Tick(0), FRAME);
    let mut unseen = record(1, 5, 5);
    unseen.visible_to = 0;
    let out = output(0, vec![record(0, 5, 5), unseen.clone()], Vec::new());
    end.tx.send(FromRunner::Tick(out)).unwrap();
    app.update();
    assert_eq!(unit_views(&mut app), 1);

    app.insert_resource(RevealAll(true));
    let out = output(1, vec![record(0, 5, 5), unseen], Vec::new());
    end.tx.send(FromRunner::Tick(out)).unwrap();
    app.update();
    assert_eq!(unit_views(&mut app), 2);
}

#[test]
fn send_query_routes_reply() {
    let (port, end) = runner_channel();
    let mut app = client_app(port, Tick(0), FRAME);
    let id = {
        let world = app.world_mut();
        world.resource_scope(|world, mut replies: Mut<PendingReplies>| {
            send_query(
                world.resource::<RunnerLink>(),
                &mut replies,
                SimQuery::Entity(SimId(3)),
            )
        })
    };
    let Ok(ToRunner::Query { id: sent, query }) = end.rx.try_recv() else {
        panic!("the query reaches the runner");
    };
    assert_eq!((sent, query), (id, SimQuery::Entity(SimId(3))));
    let reply = SimReply::Entity(None);
    end.tx
        .send(FromRunner::Reply {
            id,
            reply: reply.clone(),
        })
        .unwrap();
    app.update();
    let mut replies = app.world_mut().resource_mut::<PendingReplies>();
    assert_eq!(replies.take(id), Some(reply));
    assert_eq!(replies.take(id), None);
}

#[test]
fn encode_command_uses_string_table() {
    let tables = header().string_tables;
    let target = FxVec2::new(Fx::from_int(1), Fx::from_int(2));
    let kind = encode_command(&tables, &MoveCmd { target }).unwrap();
    assert_eq!(kind.type_id, CommandTypeId(0));
    let decoded: MoveCmd = postcard::from_bytes(&kind.payload).unwrap();
    assert_eq!(decoded, MoveCmd { target });
    assert_eq!(
        encode_command(&tables, &StopCmd).unwrap().type_id,
        CommandTypeId(1)
    );
    let spawn = DebugSpawnCmd {
        unit: "base.de.pz3".to_owned(),
        owner: PlayerId(0),
        pos: target,
        heading: Angle::from_raw(0),
    };
    let err = encode_command(&tables, &spawn).unwrap_err();
    assert!(err.contains("debug.spawn"), "{err}");
}

// --- placeholders ---

#[test]
fn tank_placeholder_barrel_points_plus_x() {
    let tank = Placeholder::Tank {
        hull: (3.0, 1.5, 6.0),
        turret: (2.0, 1.0, 2.5),
        barrel_len: 3.0,
    };
    let parts = placeholder_parts(&tank, 2.0);
    assert_eq!(parts.len(), 3);
    let (_, hull) = &parts[0];
    assert_eq!(hull.translation, Vec3::new(0.0, 1.5, 0.0));
    assert_eq!(hull.scale, Vec3::splat(2.0));
    let (_, barrel) = &parts[2];
    assert!(
        barrel.translation.x > 2.5,
        "in front of the turret: {barrel:?}"
    );
    assert!((barrel.rotation * Vec3::Y - Vec3::X).length() < 1e-5);
}

#[test]
fn placeholders_share_handles() {
    let (port, _end) = runner_channel();
    let mut app = client_app(port, Tick(0), FRAME);
    app.add_plugins((AssetPlugin::default(), PlaceholderPlugin))
        .init_asset::<Mesh>()
        .init_asset::<StandardMaterial>();
    let mut spawn = |sim_id: u32, owner: u8| {
        let view = UnitView {
            sim_id: SimId(sim_id),
            unit: UnitTypeId(0),
            owner: PlayerId(owner),
        };
        app.world_mut()
            .spawn((view, Transform::default(), Visibility::default()))
            .id()
    };
    let (a, b, c) = (spawn(0, 0), spawn(1, 0), spawn(2, 1));
    app.update();

    let part = |app: &App, entity: Entity| {
        let children = app.world().get::<Children>(entity).expect("parts attached");
        assert_eq!(children.len(), 1, "the fallback box is one part");
        let child = children[0];
        let mesh = app.world().get::<Mesh3d>(child).unwrap().0.clone();
        let material = app
            .world()
            .get::<MeshMaterial3d<StandardMaterial>>(child)
            .unwrap()
            .0
            .clone();
        (mesh, material)
    };
    let (mesh_a, mat_a) = part(&app, a);
    let (mesh_b, mat_b) = part(&app, b);
    let (mesh_c, mat_c) = part(&app, c);
    assert_eq!(mesh_a, mesh_b);
    assert_eq!(mesh_a, mesh_c);
    assert_eq!(mat_a, mat_b);
    assert_ne!(mat_a, mat_c);
}

// --- fix round 1: receive-timeline interpolation, journal, catch-up, placeholder cache ---

#[test]
fn interpolation_fraction_follows_receive_timeline() {
    let period = Duration::from_millis(50);
    let half = interpolation_fraction(Duration::from_millis(25), period, false);
    assert!((half - 0.5).abs() < 1e-6, "{half}");
    assert_eq!(interpolation_fraction(Duration::ZERO, period, false), 0.0);
    assert_eq!(
        interpolation_fraction(Duration::from_secs(1), period, false),
        1.0
    );
    assert_eq!(interpolation_fraction(Duration::ZERO, period, true), 1.0);
    let mut driver = LocalDriver::new(Tick(0));
    driver.set_speed(2.0);
    assert_eq!(driver.tick_period(), Duration::from_millis(25));
}

fn view_x(app: &App, sim_id: SimId) -> Option<f32> {
    let entity = *app.world().resource::<ViewIndex>().0.get(&sim_id)?;
    Some(app.world().get::<Transform>(entity)?.translation.x)
}

fn latest_x(app: &App) -> f32 {
    let buffer = app.world().resource::<SnapshotBuffer>();
    buffer.curr.as_ref().unwrap().entities[0].pos.x.to_f32()
}

#[test]
fn interpolation_never_moves_backwards() {
    // The unit moves +1 m east per tick.
    let (runner, port) = FakeRunner::spawn(|input| {
        let x = i32::try_from(input.tick.0).unwrap();
        output(input.tick.0, vec![record(0, x, 0)], Vec::new())
    });
    let mut app = client_app(port, Tick(0), Duration::from_millis(16));
    let mut xs = Vec::new();
    for _ in 0..30 {
        runner.update(&mut app);
        xs.extend(view_x(&app, SimId(0)));
    }
    assert!(xs.len() >= 20, "{xs:?}");
    assert!(
        xs.windows(2).all(|pair| pair[1] >= pair[0] - 1e-4),
        "the view never moves backwards: {xs:?}"
    );
    assert!(xs[xs.len() - 1] - xs[0] >= 3.0, "the view moves: {xs:?}");

    // Paused, the view shows the latest snapshot, also after a single step.
    app.world_mut().resource_mut::<LocalDriver>().pause();
    runner.update(&mut app);
    runner.update(&mut app);
    let before = latest_x(&app);
    assert_eq!(view_x(&app, SimId(0)), Some(before));
    app.world_mut().resource_mut::<LocalDriver>().step(1);
    runner.update(&mut app);
    runner.update(&mut app);
    assert_eq!(latest_x(&app), before + 1.0);
    assert_eq!(view_x(&app, SimId(0)), Some(before + 1.0));
}

fn journal_entry(tick: u32) -> JournalEntry {
    JournalEntry {
        tick: Tick(tick),
        phase: 2,
        kind: "movement.arrived".to_owned(),
        sim_id: Some(SimId(tick)),
        player: None,
        emitted: 0,
        data: serde_json::Value::Null,
    }
}

#[test]
fn multiple_outputs_per_frame_keep_all_journal_entries() {
    #[derive(Resource, Default)]
    struct Seen(Vec<Tick>);

    fn collect(mut reader: MessageReader<SimJournalMsg>, mut seen: ResMut<Seen>) {
        seen.0.extend(reader.read().map(|msg| msg.0.tick));
    }

    let (port, end) = runner_channel();
    let mut app = client_app(port, Tick(0), FRAME);
    app.init_resource::<Seen>().add_systems(PostUpdate, collect);
    for tick in 0..3 {
        let mut out = output(tick, Vec::new(), Vec::new());
        out.journal.push(journal_entry(tick));
        end.tx.send(FromRunner::Tick(out)).unwrap();
    }
    app.update();

    assert_eq!(
        app.world().resource::<Seen>().0,
        vec![Tick(0), Tick(1), Tick(2)]
    );
    let journal = app.world().resource::<JournalBuffer>();
    let all: Vec<JournalEntry> = journal.entries().cloned().collect();
    assert_eq!(
        all,
        vec![journal_entry(0), journal_entry(1), journal_entry(2)]
    );
    let since: Vec<Tick> = journal.since(Tick(1)).map(|entry| entry.tick).collect();
    assert_eq!(since, vec![Tick(1), Tick(2)]);
    assert_eq!(journal.dropped(), 0);
}

#[test]
fn journal_buffer_drops_oldest_when_full() {
    let mut journal = JournalBuffer::with_capacity(2);
    for tick in 0..3 {
        journal.push(journal_entry(tick));
    }
    let ticks: Vec<Tick> = journal.entries().map(|entry| entry.tick).collect();
    assert_eq!(ticks, vec![Tick(1), Tick(2)]);
    assert_eq!(journal.dropped(), 1);
}

#[test]
fn advance_caps_catch_up() {
    let mut driver = LocalDriver::new(Tick(0));
    driver.advance(Duration::from_secs(10));
    let mut polls = 0;
    while driver.poll().is_some() {
        polls += 1;
    }
    assert_eq!(polls, 5, "a 10 s stall counts as 250 ms");
}

fn with_placeholders(app: &mut App) {
    app.add_plugins((AssetPlugin::default(), PlaceholderPlugin))
        .init_asset::<Mesh>()
        .init_asset::<StandardMaterial>();
}

fn part_color(app: &App, entity: Entity) -> Color {
    let children = app.world().get::<Children>(entity).expect("parts attached");
    assert_eq!(children.len(), 1);
    let handle = &app
        .world()
        .get::<MeshMaterial3d<StandardMaterial>>(children[0])
        .unwrap()
        .0;
    let materials = app.world().resource::<Assets<StandardMaterial>>();
    materials.get(handle).unwrap().base_color
}

#[test]
fn placeholders_rebuild_when_colors_change() {
    let (port, _end) = runner_channel();
    let mut app = client_app(port, Tick(0), FRAME);
    with_placeholders(&mut app);
    let view = UnitView {
        sim_id: SimId(0),
        unit: UnitTypeId(0),
        owner: PlayerId(0),
    };
    let entity = app
        .world_mut()
        .spawn((view, Transform::default(), Visibility::default()))
        .id();
    app.update();
    let red = Color::linear_rgb(1.0, 0.0, 0.0);
    assert_ne!(part_color(&app, entity), red);

    let colors = PlayerColors([(PlayerId(0), red)].into_iter().collect());
    app.insert_resource(colors);
    app.update();
    assert_eq!(part_color(&app, entity), red);
}

#[test]
fn despawned_record_removes_placeholder_meshes() {
    let (runner, port) = FakeRunner::spawn(|input| {
        let ids: &[u32] = if input.tick == Tick(0) {
            &[0, 1, 2]
        } else {
            &[0, 1]
        };
        let records = ids.iter().map(|&i| record(i, 10, 10)).collect();
        output(input.tick.0, records, Vec::new())
    });
    let mut app = client_app(port, Tick(0), FRAME);
    with_placeholders(&mut app);
    let meshes = |app: &mut App| {
        let world = app.world_mut();
        world.query::<&Mesh3d>().iter(world).count()
    };
    for _ in 0..3 {
        runner.update(&mut app);
    }
    assert_eq!(meshes(&mut app), 3);
    let gone = app.world().resource::<ViewIndex>().0[&SimId(2)];
    let parts: Vec<Entity> = app
        .world()
        .get::<Children>(gone)
        .expect("parts attached")
        .iter()
        .collect();
    assert_eq!(parts.len(), 1);

    for _ in 0..3 {
        runner.update(&mut app);
    }
    assert_eq!(meshes(&mut app), 2);
    for part in parts {
        assert!(app.world().get_entity(part).is_err());
    }
}
