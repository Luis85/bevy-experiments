//! The windowed Actor Testbed (actor-capabilities spec §4.9): the
//! `testbed_content` compound with every capability on, drawn by
//! `testbed_view`, driven at 20 ticks/s by the kernel's driver.
//!
//! ```text
//! cargo run -p actor_testbed -- [--scenario NAME|FILE] [--seed N] [--speed N] [--player N]
//!     [--select ID[,ID..]] [--camera X,Z,DIST[,YAW_DEG[,PITCH_DEG]]] [--commands FILE]
//!     [--screenshot PATH --after-secs S] [--panels journal,perf,sheet,perception,explorer]
//!     [--journal FILE] [--record FILE] [--paused] [--perf-warn] [--fog on|off]
//!     [--remote | --no-remote] [--remote-port N] [--render-remote-port N | --no-render-remote]
//!     [--out-dir DIR]
//! cargo run -p actor_testbed -- --explorer-home DIR --agent NAME [--world NAME [--preset ID]]
//!     [--resume [--load-anyway | --fresh-world] [--exact-port N]] [--check-save] [--close-dialog]
//!     [--actors-dir DIR]
//!     [other flags but --scenario/--seed]
//!     (`--world` is New only: a resumed world comes from the agent's folder)
//! ```
//!
//! `--scenario` takes a scenario file path or an embedded scenario name
//! (`testbed`, `doors`, `vehicles`, `formation`, ..; default `testbed`); an
//! existing file wins over a name (the same order as `npcctl serve`).
//! `--player` is the faction the RTS UI commands (default 1). `--select`
//! preselects units (and centres the camera on it) for screenshots;
//! `--commands` submits a `{"tick", "cmd"}` list (the `npcctl run` format)
//! when each tick comes up. The names of what the scenario spawned and their
//! `SimId`s are logged at startup. `--fog on|off` draws the fog of war of the
//! actor that reveals fog (the explorer's; spec G8): black where it has never
//! been, greyed as last seen where it is not looking, units only while it
//! senses them. Default on in explorer mode, off elsewhere (a scenario without
//! such an actor shows everything anyway); **F8** toggles it.
//!
//! With the default `debug-tools` feature the game can serve the Bevy
//! Remote Protocol on `127.0.0.1:15702`: the `npc/*` methods (plus
//! `npc/screenshot {path}`) and the agent methods (`agent/attach|observe|
//! act|events|..`, `world/state`) of `npc_agent_io`, so an external agent
//! can drive a unit in the running window. BRP is **on by default in dev
//! builds and off in release builds** (`--remote` / `--no-remote`
//! override): it has no Origin/Host checks, so it is a development tool.
//! RPC file paths are relative to `--out-dir` (default `target/npc-out`).
//! `app/quit {pid?}` is the game menu's graceful quit: it answers, then the
//! app exits 0 (an explorer window saves and removes `run.json` first, as
//! when it is closed with "No").
//! The built-in profiler feeds F4; a mechanic over its p95 budget is logged
//! once (`warn!`, then `debug!`), `--perf-warn` warns on every breach (at
//! most once per mechanic per 5 s).
//!
//! **Explorer mode** (`--explorer-home`, game menu spec §5.2–5.4): one AI
//! agent's world, the embedded `explorer` scenario with its actor named after
//! the agent, in the agent folder `DIR/<slug(NAME)>/`. Without `--resume` it
//! is New: the folder is created (it must not exist), the world gets a
//! wall-clock seed and is saved at tick 0 (`<world>.world.json`, its header
//! holding the scenario and seed). `--resume` rebuilds from the save's
//! scenario and seed and loads it before the first frame. A save from another
//! build (format version or content hash) exits with code 3 unless
//! `--load-anyway` (load what fits) or `--fresh-world` (back up the save as
//! `<world>.world.<date>.json`, new world, diary kept) is given.
//! `--check-save` prints `{"ok":true}` or `{"ok":false,"error":..,"kind":
//! "content_hash"|"version"|"unreadable"}` and exits 0 / 1 without a window.
//! Explorer mode always serves BRP, on the first free port from
//! `--remote-port N` (default 15702) up to `N + 19` whose next port (the
//! render-app server) is free too; a port taken before BRP could bind it
//! exits 2 (try again) instead of advertising a dead URL.
//! `--resume --exact-port N` moves a detached world (`npcctl serve
//! --explorer-home`, stopped first) into the window on its port: exactly `N`,
//! retried for up to 10 s, never another port (exit 4, "port N is still in
//! use"); no prompt modal (the agent is already playing): the sim runs and a
//! notice says "moved here from detached" for 5 s. Every explorer window
//! writes `run.json` (mode `window`) into the agent folder and removes it on
//! exit. Closing an explorer window asks "Keep it running detached?": Yes
//! saves, starts `npcctl serve --explorer-home DIR --agent NAME --resume
//! --port N --wait-port` (its own console, it outlives the window) on this
//! window's port, prints `{"detached":{"pid","port","agent","world"}}` for
//! the menu and exits; No saves and exits; Escape goes back. **F9** moves the
//! world to detached without asking. `--close-dialog` opens that dialog at
//! start (for visual checks with `--screenshot`).
//! The sim starts paused in explorer mode. Usage errors exit with code 2.
//!
//! The app owns the host side: the speed keys (Space pause, `-`/`=` slower /
//! faster through 1x/3x/10x, `.` one tick while paused) act on the kernel's
//! `SimControl` (digits are the control groups of the command UI).

mod explorer;

use std::collections::VecDeque;
use std::path::PathBuf;

use bevy::prelude::*;
use explorer::{ExplorerArgs, ExplorerFlags, ExplorerSession, Start};
use npc_framework::agent_io::scenario::{ScenarioDoc, attach_controllers};
use npc_framework::kernel::journal::{Journal, JournalSink};
use npc_framework::kernel::replay::ScenarioInfo;
use npc_framework::kernel::{NpcDriverPlugin, driver::NpcDriverSet};
use npc_framework::prelude::{
    JournalPlugin, PlayerId, SimClock, SimCommand, SimControl, SimId, SimIndex, submit,
};
use npc_framework::tools::commands_file::load_commands;
use npc_framework::view_helpers::command_ui::select_units;
use npc_framework::view_helpers::remote::RemoteArgs;
use npc_framework::view_helpers::{DebugPanels, ScreenshotRequest, ViewSystems};
use testbed_content::TestbedIds;
use testbed_content::explorer::host as explorer_host;
use testbed_view::{
    CameraFocusOnSelected, CameraStart, CommandUi, FogOfWar, RtsCamera, Selected, TestbedPanels,
    TestbedViewPlugin,
};

/// Sim ticks per second.
const TICK_HZ: u32 = 20;
/// Multipliers the `-` / `=` keys step through.
pub const SPEED_PRESETS: [u32; 3] = [1, 3, 10];

const USAGE: &str = "usage: actor_testbed [--scenario NAME|FILE] [--seed N] [--speed N] [--player N] \
[--select ID[,ID..]] [--camera X,Z,DIST[,YAW_DEG[,PITCH_DEG]]] [--commands FILE] [--screenshot PATH] \
[--after-secs S] [--panels journal,perf,sheet,perception,explorer] [--journal FILE] [--record FILE] [--paused] [--perf-warn] \
[--fog on|off] [--remote | --no-remote] [--remote-port N] [--render-remote-port N | --no-render-remote] [--out-dir DIR]
       actor_testbed --explorer-home DIR --agent NAME [--world NAME [--preset ID] | --resume [--load-anyway | --fresh-world] [--exact-port N]] [--check-save] [--close-dialog] [--actors-dir DIR] [other flags except --scenario and --seed]
       (explorer mode: BRP is always on, on the first free port from --remote-port N on, N..=N+19, default 15702;
        --exact-port N: exactly N, waited for up to 10 s, else exit 4)";

#[cfg_attr(not(feature = "debug-tools"), allow(dead_code))]
struct Args {
    scenario: String,
    seed: Option<u64>,
    speed: u32,
    player: u32,
    select: Vec<u32>,
    camera: Option<RtsCamera>,
    commands: Option<PathBuf>,
    screenshot: Option<PathBuf>,
    after_secs: f32,
    panels: DebugPanels,
    testbed_panels: TestbedPanels,
    journal: Option<PathBuf>,
    record: Option<PathBuf>,
    paused: bool,
    /// `--perf-warn`: warn on every budget breach, not once per mechanic.
    perf_warn: bool,
    /// `--fog on|off` (`None`: on in explorer mode, off elsewhere).
    fog: Option<bool>,
    remote: RemoteArgs,
    /// Explorer mode (`--explorer-home`).
    explorer: Option<ExplorerArgs>,
}

fn usage(msg: &str) -> ! {
    if !msg.is_empty() {
        eprintln!("actor_testbed: {msg}");
    }
    eprintln!("{USAGE}");
    std::process::exit(2);
}

fn parse_panels(list: &str) -> (DebugPanels, TestbedPanels) {
    let (mut debug, mut testbed) = (DebugPanels::default(), TestbedPanels::default());
    for name in list.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        match name {
            "journal" => debug.journal = true,
            "perf" => debug.perf = true,
            "sheet" => testbed.sheet = true,
            "perception" => testbed.perception = true,
            "explorer" => testbed.explorer = true,
            other => usage(&format!(
                "unknown panel `{other}` (expected journal, perf, sheet, perception, explorer)"
            )),
        }
    }
    (debug, testbed)
}

/// `--fog on|off`.
fn parse_fog(value: &str) -> Option<bool> {
    match value {
        "on" => Some(true),
        "off" => Some(false),
        _ => None,
    }
}

/// Fog of war in the window: `--fog`, else on in explorer mode only.
fn fog_enabled(flag: Option<bool>, explorer: bool) -> bool {
    flag.unwrap_or(explorer)
}

fn parse_camera(value: &str) -> Option<RtsCamera> {
    let v: Vec<f32> = value
        .split(',')
        .map(|s| s.trim().parse().ok())
        .collect::<Option<_>>()?;
    if !(3..=5).contains(&v.len()) {
        return None;
    }
    let mut cam = RtsCamera {
        focus: Vec3::new(v[0], 0.0, v[1]),
        distance: v[2],
        ..default()
    };
    if let Some(yaw) = v.get(3) {
        cam.yaw = yaw.to_radians();
    }
    if let Some(pitch) = v.get(4) {
        cam.pitch = pitch.to_radians();
    }
    Some(cam)
}

fn parse_args() -> Args {
    let mut args = Args {
        scenario: "testbed".into(),
        seed: None,
        speed: 1,
        player: 1,
        select: Vec::new(),
        camera: None,
        commands: None,
        screenshot: None,
        after_secs: 3.0,
        panels: DebugPanels::default(),
        testbed_panels: TestbedPanels::default(),
        journal: None,
        record: None,
        paused: false,
        perf_warn: false,
        fog: None,
        // BRP on in dev builds, off in release builds (and never without
        // `debug-tools`).
        remote: RemoteArgs::new(cfg!(debug_assertions) && cfg!(feature = "debug-tools")),
        explorer: None,
    };
    let (mut explorer, mut scenario_given) = (ExplorerFlags::default(), false);
    let mut it = std::env::args().skip(1);
    while let Some(flag) = it.next() {
        if args.remote.flag(&flag) || explorer.switch(&flag) {
            continue;
        }
        match flag.as_str() {
            "--paused" => {
                args.paused = true;
                continue;
            }
            "--perf-warn" => {
                args.perf_warn = true;
                continue;
            }
            "-h" | "--help" => usage(""),
            _ => {}
        }
        let Some(value) = it.next() else {
            usage(&format!("{flag} needs a value"))
        };
        let bad = |what: &str| -> ! { usage(&format!("{flag}: `{value}` is not {what}")) };
        if let Some(r) = args.remote.value(&flag, &value) {
            r.unwrap_or_else(|e| usage(&e));
            continue;
        }
        if explorer.value(&flag, &value) {
            continue;
        }
        match flag.as_str() {
            "--scenario" => {
                args.scenario.clone_from(&value);
                scenario_given = true;
            }
            "--seed" => args.seed = Some(value.parse().unwrap_or_else(|_| bad("a number"))),
            "--speed" => args.speed = value.parse().unwrap_or_else(|_| bad("a number")),
            "--player" => args.player = value.parse().unwrap_or_else(|_| bad("a player id")),
            "--select" => {
                args.select = value
                    .split(',')
                    .map(|v| v.trim().parse().ok())
                    .collect::<Option<_>>()
                    .unwrap_or_else(|| bad("a SimId list"));
            }
            "--camera" => {
                args.camera =
                    Some(parse_camera(&value).unwrap_or_else(|| bad("X,Z,DIST[,YAW[,PITCH]]")));
            }
            "--commands" => args.commands = Some(PathBuf::from(value)),
            "--screenshot" => args.screenshot = Some(PathBuf::from(value)),
            "--after-secs" => args.after_secs = value.parse().unwrap_or_else(|_| bad("a number")),
            "--panels" => (args.panels, args.testbed_panels) = parse_panels(&value),
            "--journal" => args.journal = Some(PathBuf::from(value)),
            "--record" => args.record = Some(PathBuf::from(value)),
            "--fog" => args.fog = Some(parse_fog(&value).unwrap_or_else(|| bad("on or off"))),
            _ => usage(&format!("unknown flag {flag}")),
        }
    }
    if !cfg!(feature = "debug-tools") && args.record.is_some() {
        usage("--record needs the debug-tools feature");
    }
    if !cfg!(feature = "debug-tools") && args.remote.remote {
        usage("--remote needs the debug-tools feature");
    }
    args.explorer = explorer
        .finish(scenario_given, args.seed.is_some())
        .unwrap_or_else(|e| usage(&e));
    if let Some(ex) = &args.explorer {
        explorer::refuse_recording_on_resume(ex, args.record.is_some(), args.commands.is_some())
            .unwrap_or_else(|e| usage(&e));
    }
    args
}

/// The scenario file at path `name` when one exists, else the embedded
/// scenario `name` (the same lookup order as `npcctl serve`); `true` for an
/// embedded one.
fn scenario_doc(name: &str) -> Result<(ScenarioDoc, bool), String> {
    if std::path::Path::new(name).is_file() {
        let text = std::fs::read_to_string(name).map_err(|e| format!("cannot read {name}: {e}"))?;
        return ScenarioDoc::parse(&text)
            .map(|d| (d, false))
            .map_err(|e| format!("scenario {name}: {e}"));
    }
    match testbed_content::scenario::scenario_text(name) {
        Some(text) => ScenarioDoc::parse(text).map(|d| (d, true)),
        None => Err(format!(
            "`{name}` is neither a scenario file nor an embedded scenario ({:?})",
            testbed_content::SCENARIO_FILES.map(|(n, _)| n)
        )),
    }
}

/// The host side added to the testbed app before its content freezes:
/// window, renderer, driver, view, journal and the remote interface.
fn host(app: &mut App, args: &Args) {
    let plugins = DefaultPlugins
        .set(WindowPlugin {
            primary_window: Some(Window {
                title: "npc-framework — actor testbed".into(),
                resolution: (1280, 720).into(),
                ..default()
            }),
            // An explorer window asks before it closes
            // (`explorer::install_close_to_detached`).
            close_when_requested: args.explorer.is_none(),
            ..default()
        })
        .set(testbed_view::asset_plugin());
    // `main` installed the profiler's subscriber already
    // (`perf::install_log_plugin_now`).
    #[cfg(feature = "debug-tools")]
    let plugins = plugins.disable::<bevy::log::LogPlugin>();
    app.add_plugins(plugins)
        .add_plugins(NpcDriverPlugin::new(TICK_HZ, Update))
        .insert_resource(SimControl {
            multiplier: args.speed,
            paused: args.paused,
            ..default()
        })
        .insert_resource(args.panels)
        .insert_resource(args.testbed_panels)
        .insert_resource(CommandUi {
            player: PlayerId(args.player),
            ..default()
        })
        .insert_resource(CameraStart(args.camera.unwrap_or_default()))
        .insert_resource(FogOfWar {
            enabled: fog_enabled(args.fog, args.explorer.is_some()),
        })
        .add_plugins(TestbedViewPlugin)
        .add_systems(
            Update,
            (speed_input, step_input).chain().in_set(ViewSystems::Input),
        )
        .add_systems(Update, scripted_commands.before(NpcDriverSet))
        .add_systems(Startup, (log_names, preselect));
    if !app.is_plugin_added::<JournalPlugin>() {
        app.add_plugins(JournalPlugin::default());
    }
    #[cfg(feature = "debug-tools")]
    debug_tools::install(app, args);
}

/// The scenario app (`--scenario`, the mode without explorer flags).
fn scenario_app(args: &Args) -> App {
    let (doc, embedded) = scenario_doc(&args.scenario).unwrap_or_else(|e| usage(&e));
    let seed = args.seed.or(doc.seed).unwrap_or(1);
    if embedded {
        testbed_content::try_new_sim_app_with(seed, &args.scenario, |app| host(app, args))
            .unwrap_or_else(|e| usage(&e))
    } else {
        let loaded = testbed_content::scenario::load_with(&doc, seed, |app| host(app, args))
            .unwrap_or_else(|e| usage(&e));
        let mut loaded = loaded;
        attach_controllers(&doc, &mut loaded).unwrap_or_else(|e| usage(&e));
        loaded.app.insert_resource(ScenarioInfo {
            name: doc.info_name(),
            villagers: u32::try_from(loaded.actors.len()).unwrap_or(u32::MAX),
        });
        loaded.app
    }
}

/// Picks the BRP port for explorer mode and turns the remote interface on:
/// the first free one from `--remote-port` (default 15702) on, at most
/// [`explorer::PORT_SPAN`] further. Exits 2 without the `debug-tools`
/// feature or when no port is free.
fn explorer_port(args: &mut Args) -> u16 {
    if !cfg!(feature = "debug-tools") {
        usage("explorer mode needs the remote interface (build with the debug-tools feature)");
    }
    let Some(ex) = args.explorer.clone() else {
        usage("explorer mode needs --explorer-home");
    };
    let dir = explorer::agent_dir(&ex);
    if let Some(port) = ex.exact_port {
        // A detached world is moving here: its port only, never another
        // (the agent's settings pin the URL), once its host is gone.
        match explorer::wait_for_handover(&dir, port, explorer::EXACT_PORT_WAIT) {
            explorer::Handover::Ready => {}
            explorer::Handover::PortInUse => port_in_use(port),
            explorer::Handover::Running(run) => already_running(&ex, &run),
        }
        args.remote.remote = true;
        args.remote.port = port;
        return port;
    }
    // One owner per world: both would autosave the same save.
    if let Some(run) = explorer::running_elsewhere(&dir) {
        already_running(&ex, &run);
    }
    let first = args.remote.port;
    let last = first.saturating_add(explorer::PORT_SPAN);
    let Some(port) = explorer::pick_port(first, last) else {
        usage(&format!(
            "no free remote port in {first}..={last} (each also needs the next port free for \
             the render server): close another game or agent first, or pick another \
             --remote-port"
        ));
    };
    args.remote.remote = true;
    args.remote.port = port;
    port
}

/// The agent's world runs in another live process: exits 2.
fn already_running(ex: &ExplorerArgs, run: &testbed_content::explorer::host::RunFile) -> ! {
    eprintln!(
        "actor_testbed: {}: stop it first, or delete {} if no game runs it",
        explorer::already_running(&ex.agent, run),
        testbed_content::explorer::host::RunFile::path(&explorer::agent_dir(ex)).display()
    );
    std::process::exit(2);
}

/// `--exact-port N` stayed taken: exits 4.
fn port_in_use(port: u16) -> ! {
    eprintln!("actor_testbed: {}", explorer::port_in_use(port));
    std::process::exit(explorer::EXIT_PORT_IN_USE);
}

/// Explorer mode: `--check-save` (prints, exits 0 / 1) or the app built New
/// or resumed. Exits 3 for a save that needs `--load-anyway` /
/// `--fresh-world`, 2 for any other failure.
fn explorer_app(ex: &ExplorerArgs, args: &Args, port: u16) -> App {
    if ex.check_only {
        let result = explorer_host::check_save(&ex.home, &ex.agent);
        println!("{}", explorer_host::check_json(&result));
        std::process::exit(i32::from(result.is_err()));
    }
    match explorer::build_explorer_app(ex, |app| host(app, args)) {
        Ok((mut app, start, dir)) => {
            if let Err(e) = explorer::prepare_session(&mut app, &start, &dir, port) {
                eprintln!("actor_testbed: {e}");
                std::process::exit(2);
            }
            if ex.exact_port.is_some() {
                // The agent is already playing: no modal.
                explorer::show_moved_notice(&mut app);
            } else {
                explorer::show_prompt_modal(&mut app);
            }
            explorer::install_close_to_detached(&mut app, &ex.home, &ex.agent, port);
            if ex.show_close_dialog {
                explorer::open_close_dialog(&mut app);
            }
            app.insert_resource(testbed_view::ExplorerFiles {
                dir: dir.dir.clone(),
            });
            let session = app.world().resource::<ExplorerSession>();
            let how = match &start {
                Start::New => "new world".to_owned(),
                Start::Resumed(r) if r.defaulted.is_empty() => "resumed".to_owned(),
                Start::Resumed(r) => format!("resumed (defaulted {:?})", r.defaulted),
                Start::ResumedChanged(r) => format!(
                    "resumed a save from another build (skipped {:?}, defaulted {:?})",
                    r.skipped, r.defaulted
                ),
                Start::Rebuilt => "fresh world (the old save was backed up)".to_owned(),
            };
            info!(
                "actor_testbed: explorer `{}` in `{}` (seed {}): {how}; folder {}",
                session.meta.agent,
                session.meta.world,
                session.seed,
                dir.dir.display()
            );
            app
        }
        // The port was taken again between the wait and BRP's bind.
        Err(e) if ex.exact_port.is_some() && explorer::is_busy_remote(&e) => port_in_use(port),
        Err(e) => {
            eprintln!("actor_testbed: {e}");
            std::process::exit(if explorer_host::needs_choice(&e) {
                3
            } else {
                2
            });
        }
    }
}

fn main() {
    let mut args = parse_args();
    // The profiler's subscriber before the sim app exists: the sim's system
    // spans are created as its plugins add them, before `host` adds the
    // window (`DefaultPlugins` without its `LogPlugin` there).
    #[cfg(feature = "debug-tools")]
    npc_framework::perf::install_log_plugin_now();
    let mut app = match args.explorer.clone() {
        Some(ex) => {
            let port = if ex.check_only {
                0
            } else {
                explorer_port(&mut args)
            };
            explorer_app(&ex, &args, port)
        }
        None => scenario_app(&args),
    };
    if let Some(path) = &args.journal {
        match JournalSink::create(path) {
            Ok(sink) => app.world_mut().resource_mut::<Journal>().sink = Some(sink),
            Err(e) => usage(&format!("--journal {}: {e}", path.display())),
        }
    }
    if let Some(path) = &args.commands {
        let cmds = load_commands(path)
            .unwrap_or_else(|f| usage(&format!("--commands {}: {}", path.display(), f.body)));
        app.insert_resource(Scripted(
            cmds.into_iter().map(|c| (c.tick, c.cmd)).collect(),
        ));
    }
    if !args.select.is_empty() {
        app.insert_resource(Preselect(args.select.iter().map(|&i| SimId(i)).collect()));
        if args.camera.is_none() {
            app.insert_resource(CameraFocusOnSelected {
                distance: Some(16.0),
            });
        }
    }
    if let Some(path) = args.screenshot {
        app.insert_resource(ScreenshotRequest {
            path,
            after_secs: args.after_secs,
        });
    }
    app.run();
}

/// Commands from `--commands`, by tick.
#[derive(Resource, Default)]
struct Scripted(VecDeque<(u64, SimCommand)>);

/// Submits the `--commands` due at the next tick (recorded like any input).
fn scripted_commands(world: &mut World) {
    let Some(tick) = world.get_resource::<SimClock>().map(|c| c.tick) else {
        return;
    };
    loop {
        let due = world
            .get_resource::<Scripted>()
            .and_then(|s| s.0.front().map(|(t, _)| *t <= tick))
            .unwrap_or(false);
        if !due {
            return;
        }
        let Some((_, cmd)) = world.resource_mut::<Scripted>().0.pop_front() else {
            return;
        };
        if let Err(e) = submit(world, None, cmd) {
            warn!("actor_testbed: --commands: rejected at tick {tick}: {e}");
        }
    }
}

#[derive(Resource, Clone)]
struct Preselect(Vec<SimId>);

/// `--select`: the first unit is inspected; the player's own units are
/// selected for commands (`command_ui::select_units`).
fn preselect(world: &mut World) {
    let Some(ids) = world.get_resource::<Preselect>().map(|p| p.0.clone()) else {
        return;
    };
    for &id in &ids {
        if world
            .get_resource::<SimIndex>()
            .and_then(|i| i.get(id))
            .is_none()
        {
            warn!("actor_testbed: --select {}: no such unit", id.0);
        }
    }
    select_units(world, ids.iter().copied(), false);
    world.resource_mut::<Selected>().0 = ids.first().copied();
}

fn log_names(ids: Option<Res<TestbedIds>>, info: Option<Res<ScenarioInfo>>) {
    let Some(ids) = ids else { return };
    let list: Vec<String> = ids
        .names
        .iter()
        .map(|(n, id)| format!("{n}=#{}", id.0))
        .collect();
    info!(
        "actor_testbed: scenario `{}`: {}",
        info.map_or_else(|| "?".into(), |i| i.name.clone()),
        list.join(" ")
    );
}

/// Space pauses; `-` / `=` step down / up through [`SPEED_PRESETS`].
fn speed_input(keys: Option<Res<ButtonInput<KeyCode>>>, mut control: ResMut<SimControl>) {
    let Some(keys) = keys else { return };
    if keys.just_pressed(KeyCode::Space) {
        control.paused = !control.paused;
    }
    let at = SPEED_PRESETS
        .iter()
        .position(|&m| m >= control.multiplier)
        .unwrap_or(SPEED_PRESETS.len() - 1);
    if keys.just_pressed(KeyCode::Minus) || keys.just_pressed(KeyCode::NumpadSubtract) {
        control.multiplier = SPEED_PRESETS[at.saturating_sub(1)];
    }
    if keys.just_pressed(KeyCode::Equal) || keys.just_pressed(KeyCode::NumpadAdd) {
        control.multiplier = SPEED_PRESETS[(at + 1).min(SPEED_PRESETS.len() - 1)];
    }
}

/// `.` (period) runs exactly one tick while the sim is stopped.
fn step_input(keys: Option<Res<ButtonInput<KeyCode>>>, mut control: ResMut<SimControl>) {
    if keys.is_some_and(|k| k.just_pressed(KeyCode::Period)) && control.is_stopped() {
        control.step();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fog_flag_defaults_to_explorer_mode() {
        assert_eq!(parse_fog("on"), Some(true));
        assert_eq!(parse_fog("off"), Some(false));
        assert_eq!(parse_fog("yes"), None);
        assert!(fog_enabled(None, true), "on in explorer mode");
        assert!(!fog_enabled(None, false), "off elsewhere");
        assert!(!fog_enabled(Some(false), true));
        assert!(fog_enabled(Some(true), false));
    }
}

#[cfg(feature = "debug-tools")]
mod debug_tools {
    use bevy::prelude::*;
    use npc_framework::agent_io::remote::AgentRemotePlugin;
    use npc_framework::perf::{BreachLog, NpcPerfPlugin};
    use npc_framework::view_helpers::remote::{
        install_app_quit, install_remote, record_from_startup,
    };

    use crate::Args;

    pub fn install(app: &mut App, args: &Args) {
        // The profiler (its layer comes from `perf::install_log_plugin_now()`
        // at the start of `main`; Bevy's `LogPlugin` is disabled).
        let breach_log = if args.perf_warn {
            BreachLog::Warn
        } else {
            BreachLog::WarnOnce
        };
        app.add_plugins(
            NpcPerfPlugin::new()
                .tags_with(testbed_content::perf_tags)
                .tags_with(testbed_view::perf_tags)
                .breach_log(breach_log),
        );
        if install_remote(app, "actor_testbed", &args.remote) {
            app.add_plugins(AgentRemotePlugin);
            // The menu's graceful quit: the same exit as closing the window
            // with "No" (an explorer window saves and removes run.json in
            // `explorer::save_on_exit`), then exit 0.
            install_app_quit(app);
        }
        if let Some(path) = &args.record {
            record_from_startup(app, "actor_testbed", path.clone());
        }
    }
}
