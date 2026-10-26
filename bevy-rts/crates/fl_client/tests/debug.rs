//! Debug tool tests: the console and the inspector, at World level without egui.
//!
//! Every sim change goes through `LocalDriver` as a `debug.*` command; the client never sees
//! `fl_sim`. A fake runner on `fl_view::runner_channel` answers ticks with empty outputs and
//! entity queries with a canned reply.

use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use fl_client::debug::{DebugCommandsPlugin, InspectorState, run_console_line};
use fl_client::driver::LocalDriver;
use fl_client::link::{RunnerLink, SnapshotBuffer};
use fl_client::view::{LocalPlayer, RevealAll, SimViewPlugin};
use fl_math::{Angle, Fx, FxVec2};
use fl_protocol::{
    DebugInvulnerableCmd, DebugSetHpCmd, DebugSetOwnerCmd, DebugSpawnCmd, DebugTeleportCmd,
    DebugWanderCmd, MatchHeader, PlayerCommand, PlayerId, SimId, StringTables, Tick, UnitTypeId,
    tables,
};
use fl_view::{
    EntityRecord, FromRunner, RunnerPort, SimQuery, SimReply, SimSnapshot, TickDriver, TickInput,
    TickOutput, ToRunner, runner_channel,
};

const FRAME: Duration = Duration::from_millis(50);
const ACK_TIMEOUT: Duration = Duration::from_secs(10);

fn header() -> MatchHeader {
    let mut string_tables = StringTables::default();
    string_tables.insert(
        tables::COMMANDS,
        [
            "core.move",
            "core.stop",
            "debug.invulnerable",
            "debug.set_hp",
            "debug.set_owner",
            "debug.spawn",
            "debug.teleport",
            "debug.wander",
        ]
        .map(str::to_owned)
        .to_vec(),
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

fn record(id: u32, owner: u8) -> EntityRecord {
    EntityRecord {
        sim_id: SimId(id),
        unit: UnitTypeId(0),
        owner: PlayerId(owner),
        pos: FxVec2::new(Fx::from_int(10), Fx::from_int(10)),
        heading: Angle::from_raw(0),
        turrets: Vec::new(),
        hp_frac: u16::MAX,
        flags: 0,
        visible_to: 0xFF,
    }
}

fn empty_output(tick: Tick) -> TickOutput {
    TickOutput {
        tick,
        hash: None,
        snapshot: Arc::new(SimSnapshot {
            tick,
            entities: Vec::new(),
            channels: Vec::new(),
        }),
        events: Vec::new(),
        journal: Vec::new(),
    }
}

/// A headless client: `MinimalPlugins`, `SimViewPlugin` and the debug commands.
fn client_app(port: RunnerPort) -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, SimViewPlugin, DebugCommandsPlugin))
        .insert_resource(TimeUpdateStrategy::ManualDuration(FRAME))
        .insert_resource(RunnerLink {
            port,
            header: header(),
        })
        .insert_resource(LocalDriver::new(Tick(0)))
        .insert_resource(LocalPlayer(PlayerId(0)));
    app
}

/// A client app whose snapshot buffer holds `records` (tick 0), not connected to a runner.
fn app_with_units(records: Vec<EntityRecord>) -> App {
    let (port, _end) = runner_channel();
    let mut app = client_app(port);
    app.world_mut()
        .resource_mut::<SnapshotBuffer>()
        .push(Arc::new(SimSnapshot {
            tick: Tick(0),
            entities: records,
            channels: Vec::new(),
        }));
    app
}

fn console(app: &mut App, line: &str) -> String {
    run_console_line(app.world_mut(), line)
}

fn pending(app: &App) -> Vec<PlayerCommand> {
    app.world().resource::<LocalDriver>().pending()
}

/// The name of a command's type in the match header's commands table.
fn kind_name(command: &PlayerCommand) -> String {
    header().string_tables.get(tables::COMMANDS).unwrap()[usize::from(command.kind.type_id.0)]
        .clone()
}

/// A runner thread that answers ticks with empty outputs and `SimQuery::Entity` with a canned
/// JSON value, records what it received, and acknowledges every answer.
///
/// A tick's answer is held until [`FakeRunner::update`] releases it after the frame that sent
/// the tick has finished, so the client always receives it in the next frame instead of racing
/// the same frame's drain. Queries are answered at once.
struct FakeRunner {
    inputs: Arc<Mutex<Vec<TickInput>>>,
    release: Option<crossbeam_channel::Sender<()>>,
    queries: Arc<Mutex<Vec<SimQuery>>>,
    acks: crossbeam_channel::Receiver<()>,
    shutdown: crossbeam_channel::Sender<ToRunner>,
    thread: Option<JoinHandle<()>>,
}

impl FakeRunner {
    fn spawn() -> (Self, RunnerPort) {
        let (port, end) = runner_channel();
        let inputs = Arc::new(Mutex::new(Vec::new()));
        let queries = Arc::new(Mutex::new(Vec::new()));
        let (seen_inputs, seen_queries) = (Arc::clone(&inputs), Arc::clone(&queries));
        let (ack_tx, acks) = crossbeam_channel::unbounded();
        let (release, released) = crossbeam_channel::unbounded::<()>();
        let thread = std::thread::spawn(move || {
            while let Ok(message) = end.rx.recv() {
                let reply = match message {
                    ToRunner::Tick(input) => {
                        let out = FromRunner::Tick(empty_output(input.tick));
                        seen_inputs.lock().unwrap().push(input);
                        if released.recv().is_err() {
                            return;
                        }
                        out
                    }
                    ToRunner::Query { id, query } => {
                        let reply = match &query {
                            SimQuery::Entity(sim_id) => SimReply::Entity(Some(
                                serde_json::json!({ "sim_id": sim_id.0, "hp": 42 }),
                            )),
                            _ => SimReply::Error("unsupported".to_owned()),
                        };
                        seen_queries.lock().unwrap().push(query);
                        FromRunner::Reply { id, reply }
                    }
                    ToRunner::Shutdown => return,
                    // `ToRunner` is non-exhaustive; this fake never receives a `Control`.
                    _ => continue,
                };
                if end.tx.send(reply).is_err() {
                    return;
                }
                let _ = ack_tx.send(());
            }
        });
        let runner = FakeRunner {
            inputs,
            release: Some(release),
            queries,
            acks,
            shutdown: port.tx.clone(),
            thread: Some(thread),
        };
        (runner, port)
    }

    /// Waits until `n` more messages have been answered.
    fn wait(&self, n: u32) {
        for _ in 0..n {
            self.acks
                .recv_timeout(ACK_TIMEOUT)
                .expect("the fake runner answers every message");
        }
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
            self.wait(1);
        }
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

#[test]
fn console_spawn_issues_debug_spawn() {
    let mut app = app_with_units(Vec::new());
    let out = console(&mut app, "spawn base.de.pz3 0 500 500");
    assert!(!out.starts_with("error"), "{out}");
    let commands = pending(&app);
    assert_eq!(commands.len(), 1);
    assert_eq!(kind_name(&commands[0]), "debug.spawn");
    assert_eq!(commands[0].player, PlayerId(0));
    assert!(commands[0].units.is_empty());
    let payload: DebugSpawnCmd = postcard::from_bytes(&commands[0].kind.payload).unwrap();
    assert_eq!(payload.unit, "base.de.pz3");
    assert_eq!(payload.owner, PlayerId(0));
    assert_eq!(
        payload.pos,
        FxVec2::new(Fx::from_int(500), Fx::from_int(500))
    );
}

#[test]
fn console_unknown_command() {
    let mut app = app_with_units(Vec::new());
    assert_eq!(
        console(&mut app, "frobnicate"),
        "unknown command: frobnicate"
    );
    assert!(pending(&app).is_empty());
}

#[test]
fn console_spawn_bad_args() {
    let mut app = app_with_units(Vec::new());
    let out = console(&mut app, "spawn base.de.pz3 0 x 1");
    assert!(out.starts_with("error"), "{out}");
    assert!(out.contains('x'), "{out}");
    assert!(pending(&app).is_empty());
    let out = console(&mut app, "spawn base.de.pz3 0 1");
    assert!(out.starts_with("error"), "{out}");
    assert!(pending(&app).is_empty());
}

#[test]
fn console_debug_cheats_map_to_kinds() {
    let mut app = app_with_units(vec![record(3, 1), record(4, 1)]);
    for line in [
        "setowner 3 0",
        "invuln 3 on",
        "sethp 4 25",
        "teleport 4 100 200.5",
    ] {
        let out = console(&mut app, line);
        assert!(!out.starts_with("error"), "{line}: {out}");
    }
    let commands = pending(&app);
    let kinds: Vec<String> = commands.iter().map(kind_name).collect();
    assert_eq!(
        kinds,
        [
            "debug.set_owner",
            "debug.invulnerable",
            "debug.set_hp",
            "debug.teleport"
        ]
    );
    // Issued by the local player, whoever owns the units (ruling R44).
    assert!(commands.iter().all(|c| c.player == PlayerId(0)));
    assert_eq!(commands[0].units, [SimId(3)]);
    assert_eq!(commands[3].units, [SimId(4)]);
    let set_owner: DebugSetOwnerCmd = postcard::from_bytes(&commands[0].kind.payload).unwrap();
    assert_eq!(set_owner.owner, PlayerId(0));
    let invuln: DebugInvulnerableCmd = postcard::from_bytes(&commands[1].kind.payload).unwrap();
    assert!(invuln.on);
    let set_hp: DebugSetHpCmd = postcard::from_bytes(&commands[2].kind.payload).unwrap();
    assert_eq!(set_hp.hp, 25);
    let teleport: DebugTeleportCmd = postcard::from_bytes(&commands[3].kind.payload).unwrap();
    assert_eq!(
        teleport.pos,
        FxVec2::new(Fx::from_int(100), Fx::from_ratio(401, 2))
    );

    // Any sim id may be targeted, even one missing from the snapshot.
    let out = console(&mut app, "sethp 9 10");
    assert!(!out.starts_with("error"), "{out}");
    assert_eq!(pending(&app)[4].units, [SimId(9)]);
    // A bad flag queues nothing.
    let out = console(&mut app, "invuln 3 maybe");
    assert!(out.starts_with("error"), "{out}");
    assert_eq!(pending(&app).len(), 5);
}

#[test]
fn console_wander_targets_the_players_units() {
    let mut app = app_with_units(vec![record(1, 0), record(2, 1), record(5, 1)]);
    let out = console(&mut app, "wander 1 30");
    assert!(!out.starts_with("error"), "{out}");
    let commands = pending(&app);
    assert_eq!(commands.len(), 1);
    assert_eq!(kind_name(&commands[0]), "debug.wander");
    // The player argument selects the units; the local player issues the command.
    assert_eq!(commands[0].player, PlayerId(0));
    assert_eq!(commands[0].units, [SimId(2), SimId(5)]);
    let wander: DebugWanderCmd = postcard::from_bytes(&commands[0].kind.payload).unwrap();
    assert_eq!(wander.radius, Fx::from_int(30));
    let out = console(&mut app, "wander 2 30");
    assert!(out.starts_with("error"), "{out}");
    assert_eq!(pending(&app).len(), 1);
}

#[test]
fn console_cheats_need_cheats_allowed() {
    let mut app = app_with_units(vec![record(3, 0)]);
    app.world_mut()
        .resource_mut::<RunnerLink>()
        .header
        .cheats_allowed = false;
    let out = console(&mut app, "spawn base.de.pz3 0 1 1");
    assert!(out.starts_with("error"), "{out}");
    assert!(out.contains("cheats"), "{out}");
    assert!(pending(&app).is_empty());
}

#[test]
fn console_debug_commands_issued_as_local_player() {
    let mut app = app_with_units(vec![record(7, 0)]);
    console(&mut app, "player 1");
    let out = console(&mut app, "spawn base.de.pz3 0 1 1");
    assert!(!out.starts_with("error"), "{out}");
    console(&mut app, "player 0");
    let out = console(&mut app, "sethp 7 5");
    assert!(!out.starts_with("error"), "{out}");
    let commands = pending(&app);
    assert_eq!(
        commands.iter().map(|c| c.player).collect::<Vec<_>>(),
        [PlayerId(0), PlayerId(1)],
        "pending is ordered by (exec_tick, player, seq)"
    );
    let spawn = commands.iter().find(|c| c.player == PlayerId(1)).unwrap();
    assert_eq!(kind_name(spawn), "debug.spawn");
    let payload: DebugSpawnCmd = postcard::from_bytes(&spawn.kind.payload).unwrap();
    assert_eq!(
        payload.owner,
        PlayerId(0),
        "the argument is the unit's owner"
    );
    let set_hp = commands.iter().find(|c| c.player == PlayerId(0)).unwrap();
    assert_eq!(kind_name(set_hp), "debug.set_hp");

    app.world_mut().remove_resource::<LocalPlayer>();
    let out = console(&mut app, "sethp 7 5");
    assert!(out.starts_with("error"), "{out}");
    assert_eq!(pending(&app).len(), 2);
}

#[test]
fn console_player_switch() {
    let mut app = app_with_units(Vec::new());
    let out = console(&mut app, "player 1");
    assert!(!out.starts_with("error"), "{out}");
    assert_eq!(
        app.world().get_resource::<LocalPlayer>(),
        Some(&LocalPlayer(PlayerId(1)))
    );
    assert!(pending(&app).is_empty());
}

#[test]
fn console_step_while_paused() {
    let (runner, port) = FakeRunner::spawn();
    let mut app = client_app(port);
    assert!(!console(&mut app, "pause").starts_with("error"));
    let out = console(&mut app, "step 2");
    assert!(!out.starts_with("error"), "{out}");
    for _ in 0..5 {
        runner.update(&mut app);
    }
    let inputs = runner.inputs.lock().unwrap().clone();
    assert_eq!(
        inputs.iter().map(|i| i.tick).collect::<Vec<_>>(),
        [Tick(0), Tick(1)]
    );
    assert!(app.world().resource::<LocalDriver>().is_paused());
}

#[test]
fn console_time_controls() {
    let mut app = app_with_units(Vec::new());
    assert!(console(&mut app, "step").starts_with("error"), "running");
    assert!(!console(&mut app, "speed 4").starts_with("error"));
    assert_eq!(app.world().resource::<LocalDriver>().speed(), 4.0);
    assert!(console(&mut app, "speed 9").starts_with("error"));
    assert!(console(&mut app, "speed fast").starts_with("error"));
    assert_eq!(app.world().resource::<LocalDriver>().speed(), 4.0);
    console(&mut app, "pause");
    assert!(app.world().resource::<LocalDriver>().is_paused());
    console(&mut app, "resume");
    assert!(!app.world().resource::<LocalDriver>().is_paused());
    assert!(pending(&app).is_empty());
}

#[test]
fn console_reveal_toggles_render_override() {
    let mut app = app_with_units(Vec::new());
    assert!(!app.world().resource::<RevealAll>().0);
    console(&mut app, "reveal");
    assert!(app.world().resource::<RevealAll>().0);
    console(&mut app, "reveal");
    assert!(!app.world().resource::<RevealAll>().0);
    assert!(pending(&app).is_empty());
}

#[test]
fn console_help_lists_commands() {
    let mut app = app_with_units(Vec::new());
    let out = console(&mut app, "help");
    for name in [
        "spawn", "wander", "setowner", "invuln", "sethp", "teleport", "player", "reveal", "pause",
        "resume", "step", "speed", "hash", "tick", "inspect",
    ] {
        assert!(
            out.lines().any(|line| line.starts_with(name)),
            "{name} in {out}"
        );
    }
}

#[test]
fn console_hash_and_tick_report_runner_status() {
    let (runner, port) = FakeRunner::spawn();
    let mut app = client_app(port);
    assert!(console(&mut app, "hash").contains("no"));
    for _ in 0..3 {
        runner.update(&mut app);
    }
    let last = app
        .world()
        .resource::<fl_client::link::RunnerStatus>()
        .last_tick
        .expect("ticks arrived");
    assert!(
        console(&mut app, "tick").contains(&last.0.to_string()),
        "{last:?}"
    );
}

#[test]
fn console_inspect_sends_query() {
    let (runner, port) = FakeRunner::spawn();
    let mut app = client_app(port);
    console(&mut app, "pause");
    let out = console(&mut app, "inspect 3");
    assert!(!out.starts_with("error"), "{out}");
    assert_eq!(
        app.world().resource::<InspectorState>().sim_id,
        Some(SimId(3))
    );
    runner.wait(1);
    assert_eq!(
        *runner.queries.lock().unwrap(),
        [SimQuery::Entity(SimId(3))]
    );
    app.update();
    let state = app.world().resource::<InspectorState>();
    assert_eq!(state.pending, None);
    assert_eq!(
        state.last,
        Some(serde_json::json!({ "sim_id": 3, "hp": 42 }))
    );
    assert!(pending(&app).is_empty());
}
