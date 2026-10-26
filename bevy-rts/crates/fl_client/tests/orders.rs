//! Order input tests: `OrderInput` messages and key presses are fed into a headless client
//! (`MinimalPlugins` + `SimViewPlugin` + `DebugCommandsPlugin` + `OrdersPlugin`) and the
//! commands show up in `LocalDriver`. No runner thread is involved and the clock is frozen, so
//! every frame is deterministic. Picking and the cursor ray need a window and a GPU; they are
//! covered by the demo checklist.

use std::sync::Arc;
use std::time::Duration;

use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use fl_client::debug::{CursorGround, DebugCommandsPlugin, run_console_line};
use fl_client::driver::{LocalDriver, TICK_LENGTH};
use fl_client::link::{RunnerLink, SnapshotBuffer};
use fl_client::orders::{
    OrderInput, OrderMode, OrdersPlugin, PlannedOrder, SPEED_STEPS, TestbedKeys, ground_to_sim,
    next_player, next_speed, order_for,
};
use fl_client::placeholder::PlayerColors;
use fl_client::selection::{SelectInput, Selection};
use fl_client::view::{LocalPlayer, SimViewPlugin};
use fl_math::{Angle, Fx, FxVec2};
use fl_protocol::{
    DebugSetTerrainCmd, FollowCmd, MatchHeader, MoveCmd, PlayerCommand, PlayerId, RotateCmd, SimId,
    StringTables, Tick, UnitTypeId, tables,
};
use fl_view::{EntityRecord, RunnerEnd, SimSnapshot, TickDriver, runner_channel};

const ME: PlayerId = PlayerId(0);
const FOE: PlayerId = PlayerId(1);
const COMMANDS: [&str; 5] = [
    "core.follow",
    "core.move",
    "core.rotate",
    "core.stop",
    "debug.set_terrain",
];

fn header(commands: &[&str]) -> MatchHeader {
    let mut string_tables = StringTables::default();
    string_tables.insert(
        tables::COMMANDS,
        commands.iter().map(|&c| c.to_owned()).collect(),
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

fn rec(id: u32, owner: PlayerId, x: i32, y: i32) -> EntityRecord {
    EntityRecord {
        sim_id: SimId(id),
        unit: UnitTypeId(0),
        owner,
        pos: FxVec2::new(Fx::from_int(x), Fx::from_int(y)),
        heading: Angle::from_raw(0),
        turrets: Vec::new(),
        hp_frac: u16::MAX,
        flags: 0,
        visible_to: 0xFF,
    }
}

/// Keeps the runner's end of the channel open: a closed channel would fail the match and pause
/// the driver.
#[derive(Resource)]
struct Runner(#[expect(dead_code, reason = "only keeps the channel open")] RunnerEnd);

/// A client with own units #1 #2 #3 and enemy #6, a frozen clock and a driver at tick 0.
fn app_with(commands: &[&str]) -> App {
    let (port, end) = runner_channel();
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        SimViewPlugin,
        DebugCommandsPlugin,
        OrdersPlugin,
    ))
    .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::ZERO))
    .insert_resource(ButtonInput::<KeyCode>::default())
    .insert_resource(ButtonInput::<MouseButton>::default())
    .insert_resource(RunnerLink {
        port,
        header: header(commands),
    })
    .insert_resource(Runner(end))
    .insert_resource(LocalDriver::new(Tick(0)))
    .insert_resource(LocalPlayer(ME));
    app.world_mut()
        .resource_mut::<SnapshotBuffer>()
        .push(Arc::new(SimSnapshot {
            tick: Tick(0),
            entities: vec![
                rec(1, ME, 10, 10),
                rec(2, ME, 20, 10),
                rec(3, ME, 30, 10),
                rec(6, FOE, 60, 40),
            ],
            channels: Vec::new(),
        }));
    app.update();
    app
}

fn select(app: &mut App, ids: &[u32]) {
    app.world_mut().resource_mut::<Selection>().0 = ids.iter().map(|&i| SimId(i)).collect();
}

fn send(app: &mut App, input: OrderInput) {
    app.world_mut()
        .resource_mut::<Messages<OrderInput>>()
        .write(input);
    app.update();
}

fn right_click(ground: Option<Vec3>, unit: Option<u32>, shift: bool, alt: bool) -> OrderInput {
    OrderInput::RightClick {
        ground,
        unit: unit.map(SimId),
        shift,
        alt,
    }
}

/// Presses `key` for one frame.
fn tap(app: &mut App, key: KeyCode) {
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(key);
    app.update();
    let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
    keys.release(key);
    keys.clear();
}

fn pending(app: &App) -> Vec<PlayerCommand> {
    app.world().resource::<LocalDriver>().pending()
}

fn name(command: &PlayerCommand) -> String {
    header(&COMMANDS)
        .string_tables
        .get(tables::COMMANDS)
        .unwrap()[usize::from(command.kind.type_id.0)]
    .clone()
}

fn ids(command: &PlayerCommand) -> Vec<u32> {
    command.units.iter().map(|id| id.0).collect()
}

fn xy(x: i32, y: i32) -> FxVec2 {
    FxVec2::new(Fx::from_int(x), Fx::from_int(y))
}

fn ground(x: f32, z: f32) -> Option<Vec3> {
    Some(Vec3::new(x, 0.0, z))
}

#[test]
fn right_click_ground_issues_move() {
    let mut app = app_with(&COMMANDS);
    select(&mut app, &[2, 1]);
    send(
        &mut app,
        right_click(ground(100.0, 50.0), None, false, false),
    );
    let commands = pending(&app);
    assert_eq!(commands.len(), 1);
    assert_eq!(name(&commands[0]), "core.move");
    assert_eq!(ids(&commands[0]), [1, 2]);
    assert_eq!(commands[0].player, ME);
    assert!(!commands[0].queue);
    let payload: MoveCmd = postcard::from_bytes(&commands[0].kind.payload).unwrap();
    assert_eq!(payload.target, xy(100, 50));
}

#[test]
fn shift_right_click_queues() {
    let mut app = app_with(&COMMANDS);
    select(&mut app, &[1]);
    send(&mut app, right_click(ground(5.0, 6.0), None, true, false));
    let commands = pending(&app);
    assert_eq!(commands.len(), 1);
    assert_eq!(name(&commands[0]), "core.move");
    assert!(commands[0].queue);
}

#[test]
fn right_click_own_unit_follows() {
    let mut app = app_with(&COMMANDS);
    select(&mut app, &[1, 2]);
    send(
        &mut app,
        right_click(ground(30.0, 10.0), Some(3), false, false),
    );
    let commands = pending(&app);
    assert_eq!(commands.len(), 1);
    assert_eq!(name(&commands[0]), "core.follow");
    assert_eq!(ids(&commands[0]), [1, 2]);
    let payload: FollowCmd = postcard::from_bytes(&commands[0].kind.payload).unwrap();
    assert_eq!(payload.leader, SimId(3));
}

#[test]
fn alt_right_click_unit_forces_move() {
    let mut app = app_with(&COMMANDS);
    select(&mut app, &[1]);
    send(
        &mut app,
        right_click(ground(30.0, 10.0), Some(3), false, true),
    );
    let commands = pending(&app);
    assert_eq!(commands.len(), 1);
    assert_eq!(name(&commands[0]), "core.move");
    let payload: MoveCmd = postcard::from_bytes(&commands[0].kind.payload).unwrap();
    assert_eq!(payload.target, xy(30, 10));
}

#[test]
fn right_click_enemy_moves_to_it() {
    let mut app = app_with(&COMMANDS);
    select(&mut app, &[1]);
    // With a ground hit the move goes to the ground point under the cursor ...
    send(
        &mut app,
        right_click(ground(61.0, 41.0), Some(6), false, false),
    );
    // ... and with none, to the enemy's own position.
    send(&mut app, right_click(None, Some(6), false, false));
    let commands = pending(&app);
    assert_eq!(commands.len(), 2);
    let targets: Vec<FxVec2> = commands
        .iter()
        .map(|c| {
            assert_eq!(name(c), "core.move");
            postcard::from_bytes::<MoveCmd>(&c.kind.payload)
                .unwrap()
                .target
        })
        .collect();
    assert_eq!(targets, [xy(61, 41), xy(60, 40)]);
}

#[test]
fn rotate_mode_left_click_issues_rotate() {
    let mut app = app_with(&COMMANDS);
    select(&mut app, &[1, 2]);
    tap(&mut app, TestbedKeys::default().rotate);
    assert_eq!(*app.world().resource::<OrderMode>(), OrderMode::Rotate);
    send(
        &mut app,
        OrderInput::LeftClickInMode {
            ground: ground(0.0, 90.0),
            unit: None,
            shift: false,
        },
    );
    let commands = pending(&app);
    assert_eq!(commands.len(), 1);
    assert_eq!(name(&commands[0]), "core.rotate");
    assert_eq!(ids(&commands[0]), [1, 2]);
    let payload: RotateCmd = postcard::from_bytes(&commands[0].kind.payload).unwrap();
    assert_eq!(payload.toward, xy(0, 90));
    assert_eq!(*app.world().resource::<OrderMode>(), OrderMode::Normal);
}

#[test]
fn mode_click_is_taken_from_selection_and_shift_keeps_the_mode() {
    let mut app = app_with(&COMMANDS);
    select(&mut app, &[1]);
    tap(&mut app, TestbedKeys::default().rotate);
    app.world_mut().resource_mut::<CursorGround>().0 = ground(7.0, 8.0);
    // The selection's own click on empty ground would clear the selection; in a mode it
    // becomes the order instead.
    app.world_mut()
        .resource_mut::<Messages<SelectInput>>()
        .write(SelectInput::Click {
            sim_id: None,
            shift: true,
            at: Duration::ZERO,
        });
    app.update();
    let commands = pending(&app);
    assert_eq!(commands.len(), 1);
    assert_eq!(name(&commands[0]), "core.rotate");
    assert!(commands[0].queue);
    assert_eq!(app.world().resource::<Selection>().0.len(), 1);
    assert_eq!(*app.world().resource::<OrderMode>(), OrderMode::Rotate);
}

#[test]
fn follow_mode_left_click_follows_the_unit() {
    let mut app = app_with(&COMMANDS);
    select(&mut app, &[1]);
    tap(&mut app, TestbedKeys::default().follow);
    send(
        &mut app,
        OrderInput::LeftClickInMode {
            ground: None,
            unit: Some(SimId(6)),
            shift: false,
        },
    );
    let commands = pending(&app);
    assert_eq!(commands.len(), 1);
    assert_eq!(name(&commands[0]), "core.follow");
}

#[test]
fn stop_key_issues_stop() {
    let mut app = app_with(&COMMANDS);
    select(&mut app, &[3, 1]);
    tap(&mut app, TestbedKeys::default().stop);
    let commands = pending(&app);
    assert_eq!(commands.len(), 1);
    assert_eq!(name(&commands[0]), "core.stop");
    assert_eq!(ids(&commands[0]), [1, 3]);
    assert!(!commands[0].queue);
}

#[test]
fn no_selection_no_command() {
    let mut app = app_with(&COMMANDS);
    send(&mut app, right_click(ground(1.0, 1.0), None, false, false));
    tap(&mut app, TestbedKeys::default().stop);
    assert!(pending(&app).is_empty());
}

#[test]
fn orders_while_paused_apply_on_resume() {
    let mut app = app_with(&COMMANDS);
    app.world_mut().insert_resource(LocalDriver::new(Tick(10)));
    select(&mut app, &[1]);
    tap(&mut app, TestbedKeys::default().pause);
    assert!(app.world().resource::<LocalDriver>().is_paused());
    send(&mut app, right_click(ground(3.0, 4.0), None, false, false));
    let mut driver = app.world_mut().resource_mut::<LocalDriver>();
    driver.advance(TICK_LENGTH);
    assert!(driver.poll().is_none(), "no tick runs while paused");
    tap(&mut app, TestbedKeys::default().pause);
    let mut driver = app.world_mut().resource_mut::<LocalDriver>();
    assert!(!driver.is_paused());
    driver.advance(TICK_LENGTH * 2);
    let first = driver.poll().expect("tick 10");
    assert_eq!(first.tick, Tick(10));
    assert!(first.commands.is_empty());
    let second = driver.poll().expect("tick 11");
    assert_eq!(second.tick, Tick(11));
    assert_eq!(second.commands.len(), 1);
    assert_eq!(name(&second.commands[0]), "core.move");
}

#[test]
fn speed_keys_step_through_speeds() {
    let mut app = app_with(&COMMANDS);
    let keys = TestbedKeys::default();
    let speed = |app: &App| app.world().resource::<LocalDriver>().speed();
    let mut seen = Vec::new();
    for _ in 0..4 {
        tap(&mut app, keys.speed_up);
        seen.push(speed(&app));
    }
    assert_eq!(seen, [2.0, 4.0, 8.0, 8.0]);
    let mut seen = Vec::new();
    for _ in 0..8 {
        tap(&mut app, keys.speed_down);
        seen.push(speed(&app));
    }
    assert_eq!(seen, [4.0, 2.0, 1.0, 0.5, 0.25, 0.25, 0.25, 0.25]);
    assert_eq!(SPEED_STEPS.len(), 6);
    assert_eq!(next_speed(1.7, true), 2.0);
    assert_eq!(next_speed(1.7, false), 1.0);
}

#[test]
fn pause_key_toggles() {
    let mut app = app_with(&COMMANDS);
    let paused = |app: &App| app.world().resource::<LocalDriver>().is_paused();
    assert!(!paused(&app));
    tap(&mut app, TestbedKeys::default().pause);
    assert!(paused(&app));
    tap(&mut app, TestbedKeys::default().pause);
    assert!(!paused(&app));
}

#[test]
fn cycle_player_wraps() {
    let mut app = app_with(&COMMANDS);
    app.world_mut().insert_resource(PlayerColors(
        [(ME, Color::WHITE), (FOE, Color::BLACK)]
            .into_iter()
            .collect(),
    ));
    select(&mut app, &[1, 2]);
    let local = |app: &App| app.world().resource::<LocalPlayer>().0;
    tap(&mut app, TestbedKeys::default().cycle_player);
    assert_eq!(local(&app), FOE);
    assert!(app.world().resource::<Selection>().0.is_empty());
    select(&mut app, &[6]);
    tap(&mut app, TestbedKeys::default().cycle_player);
    assert_eq!(local(&app), ME);
    assert!(app.world().resource::<Selection>().0.is_empty());
    assert_eq!(next_player(&[], Some(ME)), None);
}

#[test]
fn unknown_command_name_is_reported() {
    // A header without "core.rotate": the order is dropped with a warning, nothing panics,
    // and the mode still ends.
    let mut app = app_with(&["core.follow", "core.move", "core.stop"]);
    select(&mut app, &[1]);
    tap(&mut app, TestbedKeys::default().rotate);
    let click = OrderInput::LeftClickInMode {
        ground: ground(1.0, 2.0),
        unit: None,
        shift: false,
    };
    // The warning is counted per thread, so the systems must run on this one.
    app.edit_schedule(Update, |schedule| {
        schedule.set_executor(bevy::ecs::schedule::SingleThreadedExecutor::new());
    });
    let (warns, ()) = count_warns(|| send(&mut app, click));
    assert_eq!(warns, 1, "one warn for the dropped order");
    assert!(pending(&app).is_empty());
    assert_eq!(*app.world().resource::<OrderMode>(), OrderMode::Normal);
    // The plan itself is fine.
    let plan = order_for(
        &click,
        OrderMode::Rotate,
        &Selection([SimId(1)].into_iter().collect()),
        |_| true,
    );
    assert_eq!(
        plan,
        Some(PlannedOrder::Rotate {
            toward: xy(1, 2),
            queue: false
        })
    );
}

/// Counts the `WARN` events logged while `f` runs on this thread.
fn count_warns<R>(f: impl FnOnce() -> R) -> (usize, R) {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tracing::{Event, Level, Metadata, span, subscriber::Interest};

    struct Counter(Arc<AtomicUsize>);
    impl tracing::Subscriber for Counter {
        fn enabled(&self, _: &Metadata<'_>) -> bool {
            true
        }
        fn register_callsite(&self, _: &'static Metadata<'static>) -> Interest {
            Interest::sometimes()
        }
        fn new_span(&self, _: &span::Attributes<'_>) -> span::Id {
            span::Id::from_u64(1)
        }
        fn record(&self, _: &span::Id, _: &span::Record<'_>) {}
        fn record_follows_from(&self, _: &span::Id, _: &span::Id) {}
        fn event(&self, event: &Event<'_>) {
            if *event.metadata().level() == Level::WARN {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }
        fn enter(&self, _: &span::Id) {}
        fn exit(&self, _: &span::Id) {}
    }
    let count = Arc::new(AtomicUsize::new(0));
    let out = tracing::subscriber::with_default(Counter(Arc::clone(&count)), f);
    (count.load(Ordering::SeqCst), out)
}

#[test]
fn big_selection_is_split_into_commands_of_at_most_256() {
    let mut app = app_with(&COMMANDS);
    app.world_mut().resource_mut::<Selection>().0 = (1000..1300).map(SimId).collect();
    send(&mut app, right_click(ground(9.0, 9.0), None, true, false));
    let commands = pending(&app);
    assert_eq!(commands.len(), 2);
    assert_eq!(commands[0].units.len(), 256);
    assert_eq!(commands[1].units.len(), 44);
    let all: Vec<u32> = commands.iter().flat_map(ids).collect();
    assert_eq!(all, (1000..1300).collect::<Vec<u32>>());
    for c in &commands {
        assert_eq!(name(c), "core.move");
        assert!(c.queue);
        assert_eq!(c.kind.payload, commands[0].kind.payload);
    }
    assert_eq!(commands[1].seq, commands[0].seq + 1);
}

#[test]
fn console_terrain_issues_debug_set_terrain() {
    let mut app = app_with(&COMMANDS);
    let out = run_console_line(app.world_mut(), "terrain 70 122 72 127 base.water");
    assert!(!out.starts_with("error"), "{out}");
    let commands = pending(&app);
    assert_eq!(commands.len(), 1);
    assert_eq!(name(&commands[0]), "debug.set_terrain");
    assert_eq!(commands[0].player, ME);
    assert!(commands[0].units.is_empty());
    let payload: DebugSetTerrainCmd = postcard::from_bytes(&commands[0].kind.payload).unwrap();
    assert_eq!(
        payload,
        DebugSetTerrainCmd {
            min_cell: (70, 122),
            max_cell: (72, 127),
            terrain: "base.water".to_owned(),
        }
    );
    let bad = run_console_line(app.world_mut(), "terrain 1 2 3");
    assert!(bad.starts_with("error"), "{bad}");
}

#[test]
fn ground_to_sim_uses_x_and_z() {
    assert_eq!(
        ground_to_sim(Vec3::new(12.5, 99.0, -3.0)),
        FxVec2::new(Fx::from_ratio(25, 2), Fx::from_int(-3))
    );
}
