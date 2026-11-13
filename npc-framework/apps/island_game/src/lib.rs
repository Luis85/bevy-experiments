//! Island Village: the shipped game. One window holds the title,
//! New/Continue, play and pause (`frontend`, `island::session`); the world is
//! the `island_content` simulation drawn by `island_view`, driven at 20
//! ticks/s by the kernel's driver.
//!
//! ```text
//! cargo run -p island_game                       # the player flow (title first)
//! cargo run -p island_game -- [--island-save FILE] [--island-rules FILE] [--dev-keys]
//!     [--seed N] [--speed N] [--mute] [--screenshot PATH --after-secs S] [--ui-gallery PAGE]
//!     [--remote | --no-remote] [--remote-port N] [--render-remote-port N | --no-render-remote]
//!     [--out-dir DIR]
//! cargo run -p island_game -- --scenario NAME|FILE | --island-save FILE [--island-new]
//!     [--island-mode inspector|character|editor] [--commands FILE] [--record FILE] [--journal FILE]
//!     [--select ID[,ID..]] [--camera X,Z,DIST[,YAW_DEG[,PITCH_DEG]]] [--panels ..] [--paused]
//!     (the developer host: no title, straight into the world)
//! ```
//!
//! Without flags the game opens its title screen on the island-owned save
//! (`<home>/island-village/island.world.json`, rules beside it). A
//! `--scenario`, a bare `--island-save` (without `--front-end`),
//! `--island-new`, `--commands` or `--record` selects the developer host.
//!
//! With the default `debug-tools` feature the game can serve the Bevy Remote
//! Protocol on `127.0.0.1:15702` (`npc/*`, `agent/*`, `world/state`): on by
//! default in dev builds, off in release builds (`--remote` / `--no-remote`
//! override). `app/quit {pid?}` saves and quits a local island.
//!
//! The app owns the host side: the speed keys (Space pause, `-`/`=` slower /
//! faster through 1x/3x/10x, `.` one tick while paused) act on the kernel's
//! `SimControl`.

mod frontend;
mod island;

use std::collections::VecDeque;
use std::path::PathBuf;

use bevy::prelude::*;
use island_content::TestbedIds;
use island_view::{
    CameraFocusOnSelected, CameraStart, CommandUi, FogOfWar, RtsCamera, Selected, TestbedPanels,
    TestbedViewPlugin,
};
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

/// Sim ticks per second.
const TICK_HZ: u32 = 20;
/// Multipliers the `-` / `=` keys step through.
pub const SPEED_PRESETS: [u32; 3] = [1, 3, 10];

const USAGE: &str = "usage: island_game [--front-end] [--island-save FILE] [--island-rules FILE] [--dev-keys] [--seed N] [--speed N] \
[--mute] [--screenshot PATH] [--after-secs S] [--ui-gallery PAGE] [--remote | --no-remote] [--remote-port N] \
[--render-remote-port N | --no-render-remote] [--out-dir DIR]
       island_game (--scenario NAME|FILE | --island-save FILE [--island-new]) [--island-mode inspector|character|editor] \
[--seed N] [--speed N] [--player N] [--select ID[,ID..]] [--camera X,Z,DIST[,YAW_DEG[,PITCH_DEG]]] [--commands FILE] \
[--screenshot PATH] [--after-secs S] [--panels journal,perf,sheet,perception] [--journal FILE] [--record FILE] [--paused] \
[--perf-warn] [--mute] [--fog on|off] [remote flags]";

#[cfg_attr(not(feature = "debug-tools"), allow(dead_code))]
struct Args {
    scenario: String,
    island_mode: island_view::IslandMode,
    island_save: Option<PathBuf>,
    island_rules: Option<PathBuf>,
    island_new: bool,
    front_end: bool,
    /// `--dev-keys`: keep the developer toggles (F1, F3-F8) in the player session.
    dev_keys: bool,
    /// `--ui-gallery PAGE`: a developer screen that shows the widget kit in
    /// every state, for screenshot checks. Never offered in a menu.
    ui_gallery: Option<island_view::game_ui::gallery::Page>,
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
    /// No island audio (also implied by `--screenshot` capture runs).
    mute: bool,
    /// `--fog on|off` (`None`: on in explorer mode, off elsewhere).
    fog: Option<bool>,
    remote: RemoteArgs,
}

fn usage(msg: &str) -> ! {
    if !msg.is_empty() {
        eprintln!("island_game: {msg}");
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
            "map" => {
                testbed.explorer = true;
                testbed.map = true;
            }
            other => usage(&format!(
                "unknown panel `{other}` (expected journal, perf, sheet, perception, explorer, map)"
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

fn parse_args(raw: &[String]) -> Args {
    let mut args = Args {
        scenario: "island_colony".into(),
        island_mode: island_view::IslandMode::Inspector,
        island_save: None,
        island_rules: None,
        island_new: false,
        front_end: false,
        dev_keys: false,
        ui_gallery: None,
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
        mute: false,
        fog: None,
        // BRP on in dev builds, off in release builds (and never without
        // `debug-tools`).
        remote: RemoteArgs::new(cfg!(debug_assertions) && cfg!(feature = "debug-tools")),
    };
    let mut scenario_given = false;
    let mut front_end_flag = false;
    let mut it = raw.iter().cloned();
    while let Some(flag) = it.next() {
        if args.remote.flag(&flag) {
            continue;
        }
        match flag.as_str() {
            "--front-end" => {
                front_end_flag = true;
                continue;
            }
            "--dev-keys" => {
                args.dev_keys = true;
                continue;
            }
            "--island-new" => {
                args.island_new = true;
                continue;
            }
            "--paused" => {
                args.paused = true;
                continue;
            }
            "--perf-warn" => {
                args.perf_warn = true;
                continue;
            }
            "--mute" => {
                args.mute = true;
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
        match flag.as_str() {
            "--scenario" => {
                args.scenario.clone_from(&value);
                scenario_given = true;
            }
            "--seed" => args.seed = Some(value.parse().unwrap_or_else(|_| bad("a number"))),
            "--island-mode" => {
                args.island_mode = island::parse_mode(&value)
                    .unwrap_or_else(|| bad("inspector, character or editor"))
            }
            "--island-save" => args.island_save = Some(PathBuf::from(value)),
            "--island-rules" => args.island_rules = Some(PathBuf::from(value)),
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
            "--ui-gallery" => {
                args.ui_gallery = Some(
                    island_view::game_ui::gallery::Page::parse(&value)
                        .unwrap_or_else(|| bad("tokens, controls, data, messages or icons")),
                );
            }
            _ => usage(&format!("unknown flag {flag}")),
        }
    }
    if !cfg!(feature = "debug-tools") && args.record.is_some() {
        usage("--record needs the debug-tools feature");
    }
    if !cfg!(feature = "debug-tools") && args.remote.remote {
        usage("--remote needs the debug-tools feature");
    }
    // The player flow unless a developer host was asked for.
    args.front_end = front_end_flag
        || !(scenario_given
            || args.island_save.is_some()
            || args.island_new
            || args.record.is_some()
            || args.commands.is_some());
    if args.front_end {
        if args.scenario != "island_colony"
            || args.island_new
            || args.record.is_some()
            || args.commands.is_some()
        {
            usage(
                "--front-end is the local island flow; alternate scenarios, --island-new and startup replay commands use the developer host",
            );
        }
        if args.island_save.is_none() {
            let folder = island_home();
            args.island_save = Some(folder.join(SAVE_FILE));
            if args.island_rules.is_none() {
                args.island_rules = Some(folder.join(RULES_FILE));
            }
        }
    }
    if args.island_save.is_some() && args.scenario != "island_colony" {
        usage("--island-save requires the standalone island_colony scenario");
    }
    if args.island_rules.is_some() && args.island_save.is_none() {
        usage("--island-rules requires --island-save (saved worlds retain their original rules)");
    }
    if args.island_new && args.island_save.is_none() {
        usage("--island-new requires --island-save");
    }
    args
}

/// The island's own folder name under the user's home (D5; nothing is read
/// from the developer menu's agents folder).
pub const ISLAND_HOME_DIR: &str = "island-village";
/// The save file in [`island_home`].
pub const SAVE_FILE: &str = "island.world.json";
/// The world rules file in [`island_home`] (installed from the shipped
/// preset when missing).
pub const RULES_FILE: &str = "living_world.json";

/// `<home>/island-village`, home from `USERPROFILE` or `HOME`, else the
/// current folder.
pub fn island_home() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
        .join(ISLAND_HOME_DIR)
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
    match island_content::scenario::scenario_text(name) {
        Some(text) => ScenarioDoc::parse(text).map(|d| (d, true)),
        None => Err(format!(
            "`{name}` is neither a scenario file nor an embedded scenario ({:?})",
            island_content::SCENARIO_FILES.map(|(n, _)| n)
        )),
    }
}

/// The player session carries the game's name only; developer launches keep
/// the framework in theirs.
fn window_title(front_end: bool, colony: bool) -> &'static str {
    match (front_end, colony) {
        (true, _) => "Island Village",
        (false, true) => "Island Village | NPC Framework",
        (false, false) => "NPC Framework | Actor Testbed",
    }
}

/// The host side added to the testbed app before its content freezes:
/// window, renderer, driver, view, journal and the remote interface.
fn host(app: &mut App, args: &Args) {
    let colony = island_content::village::colony::enabled(app.world());
    if colony {
        app.insert_resource(island_view::OverlayState { visible: false });
    }
    if args.front_end {
        app.insert_resource(island_view::PlayerExperience { enabled: true })
            .insert_resource(island_view::FrontendGate(true))
            .insert_resource(island_view::DeveloperKeys(args.dev_keys));
    }
    // Capture runs are automation: they stay silent like `--mute`.
    if args.mute || args.screenshot.is_some() {
        app.insert_resource(island_view::IslandAudioSettings::muted());
    }
    let plugins = DefaultPlugins
        .set(WindowPlugin {
            primary_window: Some(Window {
                title: window_title(args.front_end, colony).into(),
                resolution: (1280, 720).into(),
                ..default()
            }),
            close_when_requested: args.island_save.is_none(),
            ..default()
        })
        .set(island_view::asset_plugin());
    // `main` installed the profiler's subscriber already
    // (`perf::install_log_plugin_now`).
    #[cfg(feature = "debug-tools")]
    let plugins = plugins.disable::<bevy::log::LogPlugin>();
    if let Some(camera) = args.camera {
        app.insert_resource(CameraStart(camera));
    }
    app.add_plugins(plugins)
        .insert_resource(if args.front_end {
            island_view::IslandMode::Inspector
        } else {
            args.island_mode
        })
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
        .insert_resource(FogOfWar {
            enabled: fog_enabled(args.fog, false),
        })
        .add_plugins(TestbedViewPlugin)
        .add_systems(
            Update,
            (speed_input, step_input).chain().in_set(ViewSystems::Input),
        )
        .add_systems(Update, scripted_commands.before(NpcDriverSet))
        .add_systems(Startup, (log_names, preselect));
    if args.front_end {
        frontend::install(app);
        // Shape's "Export world" writes beside the world rules, or beside
        // the save when no rules file was named.
        let folder = args
            .island_rules
            .as_deref()
            .or(args.island_save.as_deref())
            .and_then(std::path::Path::parent)
            .map(std::path::Path::to_path_buf);
        if let Some(folder) = folder {
            app.insert_resource(frontend::ExportFolder(folder));
        }
    }
    if let Some(page) = args.ui_gallery {
        island_view::game_ui::gallery::install(app, page);
    }
    if !app.is_plugin_added::<JournalPlugin>() {
        app.add_plugins(JournalPlugin::default());
    }
    #[cfg(feature = "debug-tools")]
    debug_tools::install(app, args);
}

/// The game's app: the player flow, a local save, or a scenario.
fn scenario_app(args: &Args) -> App {
    if args.front_end {
        return island::session::open(
            args.island_save.as_deref().expect("frontend save path"),
            args.seed,
            args.island_rules.as_deref(),
            |app| host(app, args),
        )
        .unwrap_or_else(|error| usage(&error));
    }
    if let Some(path) = &args.island_save {
        return island::open(
            path,
            args.seed,
            args.island_rules.as_deref(),
            args.island_new,
            |app| host(app, args),
        )
        .unwrap_or_else(|error| usage(&error));
    }
    let (doc, embedded) = scenario_doc(&args.scenario).unwrap_or_else(|e| usage(&e));
    let seed = args.seed.or(doc.seed).unwrap_or(1);
    if embedded {
        island_content::try_new_sim_app_with(seed, &args.scenario, |app| host(app, args))
            .unwrap_or_else(|e| usage(&e))
    } else {
        let loaded = island_content::scenario::load_with(&doc, seed, |app| host(app, args))
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

/// Run the native host with explicit arguments. The launcher calls this before
/// creating any window, so title, play and pause share one app lifecycle.
pub fn run_from_args(raw: Vec<String>) -> AppExit {
    let args = parse_args(&raw);
    // The profiler's subscriber before the sim app exists: the sim's system
    // spans are created as its plugins add them, before `host` adds the
    // window (`DefaultPlugins` without its `LogPlugin` there).
    #[cfg(feature = "debug-tools")]
    npc_framework::perf::install_log_plugin_now();
    let mut app = scenario_app(&args);
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
    app.run()
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
            warn!("island_game: --commands: rejected at tick {tick}: {e}");
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
            warn!("island_game: --select {}: no such unit", id.0);
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
        "island_game: scenario `{}`: {}",
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
    fn explicit_library_arguments_select_frontend_and_qa_paths() {
        let args = parse_args(
            &[
                "--front-end",
                "--island-save",
                "scratch.json",
                "--no-remote",
                "--screenshot",
                "title.png",
                "--after-secs",
                "2",
            ]
            .map(str::to_owned),
        );
        assert!(args.front_end);
        assert_eq!(args.scenario, "island_colony");
        assert_eq!(args.island_save, Some(PathBuf::from("scratch.json")));
        assert_eq!(args.screenshot, Some(PathBuf::from("title.png")));
        assert_eq!(args.after_secs, 2.0);
        assert!(!args.remote.remote);
        assert!(!args.dev_keys, "developer toggles are opt-in for players");
    }

    #[test]
    fn player_session_title_and_developer_keys_are_explicit() {
        let raw = ["--front-end", "--dev-keys", "--island-save", "scratch.json"];
        assert!(parse_args(&raw.map(str::to_owned)).dev_keys);
        assert!(!parse_args(&raw.map(str::to_owned)).mute);
        let quiet = ["--front-end", "--mute", "--island-save", "scratch.json"];
        assert!(parse_args(&quiet.map(str::to_owned)).mute);
        assert_eq!(window_title(true, true), "Island Village");
        assert_eq!(window_title(false, true), "Island Village | NPC Framework");
        assert_eq!(window_title(false, false), "NPC Framework | Actor Testbed");
    }

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
                .domain(
                    "island_view::village::",
                    npc_framework::perf::PerfDomain::UserInterface,
                )
                .domain(
                    "island_view::village::background",
                    npc_framework::perf::PerfDomain::Platform,
                )
                .domain(
                    "island_content",
                    npc_framework::perf::PerfDomain::Simulation,
                )
                .domain("island_game", npc_framework::perf::PerfDomain::Platform)
                .domain(
                    "island_game::speed_input",
                    npc_framework::perf::PerfDomain::UserInterface,
                )
                .domain(
                    "island_game::step_input",
                    npc_framework::perf::PerfDomain::UserInterface,
                )
                .tags_with(island_content::perf_tags)
                .tags_with(island_view::perf_tags)
                .breach_log(breach_log),
        );
        if install_remote(app, "island_game", &args.remote) {
            app.add_plugins(AgentRemotePlugin);
            // The menu's graceful quit: the same exit as closing the window
            // with "No" (an explorer window saves and removes run.json in
            // `explorer::save_on_exit`), then exit 0.
            if args.island_save.is_some() {
                use npc_framework::tools::remote::RemoteAppExt;
                app.add_plugins(npc_framework::view_helpers::remote::AppQuitPlugin)
                    .add_remote_method("app/quit", crate::island::app_quit);
            } else {
                install_app_quit(app);
            }
        }
        if let Some(path) = &args.record {
            record_from_startup(app, "island_game", path.clone());
        }
    }
}

#[cfg(test)]
mod map_panel_tests {
    #[test]
    fn panels_map_opens_explorer_on_the_map_tab() {
        let (debug, panels) = super::parse_panels("map");
        assert!(panels.explorer && panels.map);
        assert!(!panels.sheet && !panels.perception && !debug.journal && !debug.perf);
    }
}
