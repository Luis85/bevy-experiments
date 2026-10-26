//! The game menu (game menu spec §5.1, G1/G2): a window listing everything
//! playable. The chosen game starts as a child process with the right flags;
//! the menu window hides while it runs and comes back when it exits (a
//! non-zero exit code is shown on the main screen, with the last line the
//! game wrote to stderr; the game's stderr still reaches the terminal).
//!
//! ```text
//! cargo run -p game_menu -- [--settings FILE] [--screen NAME] [--resume AGENT]
//!     [--detached AGENT] [--screenshot PATH [--after-secs S]]
//! ```
//!
//! Screens: Main (Explore with an AI agent, Actor Testbed, Village, RTS
//! units demo, Settings, Quit), Testbed (one button per embedded scenario),
//! Settings (the agents folder and the fog of war checkbox), Explorer (New / Resume, then New's names
//! or Resume's agent list, the save check and the mismatch dialog), the
//! Detached screen (a world running without a window, [`detached`]), Running
//! and Missing. The games are the executables next to the menu's own
//! (`actor_testbed`, `village`, `rts_units_demo`); a missing one shows its
//! `cargo build -p <name>` command, and **Build** runs it when `cargo` is on
//! `PATH` and the workspace is found above the menu's folder (else the one
//! the menu was compiled in); **Cancel**, or closing the menu, stops it.
//! The menu renders only when something changes ([`power`]).
//!
//! **Quit** (Main's button, the window's close button, or Ctrl+C on the
//! menu's console) stops every process the game started before the menu
//! exits ([`quit`]): detached worlds save and stop, an explorer window gets
//! `app/quit` (it saves), windows with nothing to save are killed at once,
//! and what does not answer in 15 s is killed by its own pid; a world that
//! cannot be stopped (a failed save, or no answer) keeps the menu on the
//! Quitting screen with **Quit anyway** and **Back**.
//!
//! The agents folder and the fog of war setting (passed to explorer
//! windows as `actor_testbed --fog on|off`) are kept in the per-user
//! settings file
//! (`testbed_content::explorer::settings::Settings::file`, e.g.
//! `%APPDATA%\npc-framework\settings.json`), or in `--settings FILE`.
//! `--screen` opens a screen directly (`main`, `testbed`, `settings`,
//! `explorer`, `explorer_new`, `explorer_resume`); with `--screenshot` the
//! menu saves a PNG after `--after-secs` (default 3) and exits. `--resume
//! AGENT` presses Resume on that agent at start (the save check, then the
//! game or the mismatch dialog), for visual checks of the dialog; `--detached
//! AGENT` opens the Detached screen of that agent's running detached world
//! (its live `run.json`). Escape goes
//! back; Tab moves between buttons, Enter or Space presses one; Ctrl+C,
//! Ctrl+X and Ctrl+V (Cmd on macOS) in a text field copy, cut and paste
//! through the OS clipboard ([`paste`]). Usage errors exit with code 2.

mod detached;
mod explorer;
mod launch;
mod paste;
mod power;
mod quit;
mod run;
mod screens;
mod ui;

use std::path::PathBuf;

use bevy::input_focus::tab_navigation::TabNavigationPlugin;
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk};
use launch::Entry;
use paste::FieldClipboardPlugin;
use power::PowerPlugin;
use run::BuildTools;
use screens::{Action, Screen, ScreensPlugin};
use testbed_content::explorer::settings::Settings;

const USAGE: &str = "usage: game_menu [--settings FILE] [--screen NAME] [--resume AGENT] \
     [--detached AGENT] [--screenshot PATH [--after-secs S]]";

/// The settings in use.
#[derive(Resource, Clone, Debug)]
pub struct AppSettings(pub Settings);

/// Where the menu's files are.
#[derive(Resource, Clone, Debug)]
pub struct Paths {
    /// The folder of the menu's executable (the games are its siblings).
    pub exe_dir: PathBuf,
    /// The settings file (`None` without a config folder).
    pub settings_file: Option<PathBuf>,
}

/// `--screenshot`: capture after `after_secs`, then exit.
#[derive(Resource, Clone, Debug)]
pub struct ScreenshotAt {
    path: PathBuf,
    after_secs: f32,
}

struct Args {
    settings: Option<PathBuf>,
    screen: Screen,
    resume: Option<String>,
    detached: Option<String>,
    screenshot: Option<PathBuf>,
    after_secs: f32,
}

fn usage(msg: &str) -> ! {
    if !msg.is_empty() {
        eprintln!("game_menu: {msg}");
    }
    eprintln!("{USAGE}");
    std::process::exit(2);
}

fn parse_args() -> Args {
    let mut args = Args {
        settings: None,
        screen: Screen::Main,
        resume: None,
        detached: None,
        screenshot: None,
        after_secs: 3.0,
    };
    let mut it = std::env::args().skip(1);
    while let Some(flag) = it.next() {
        if matches!(flag.as_str(), "-h" | "--help") {
            usage("");
        }
        let Some(value) = it.next() else {
            usage(&format!("{flag} needs a value"))
        };
        match flag.as_str() {
            "--settings" => args.settings = Some(PathBuf::from(value)),
            "--screen" => {
                args.screen = Screen::from_name(&value).unwrap_or_else(|| {
                    usage(&format!(
                        "--screen: `{value}` is not one of {}",
                        Screen::NAMES
                    ))
                });
            }
            "--resume" => args.resume = Some(value),
            "--detached" => args.detached = Some(value),
            "--screenshot" => args.screenshot = Some(PathBuf::from(value)),
            "--after-secs" => {
                args.after_secs = value
                    .parse()
                    .unwrap_or_else(|_| usage(&format!("--after-secs: `{value}` is not a number")));
            }
            other => usage(&format!("unknown flag `{other}`")),
        }
    }
    args
}

fn main() {
    let args = parse_args();
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(PathBuf::from))
        .unwrap_or_default();
    let settings_file = screens::settings_file(args.settings);
    let home = launch::home_dir(&|k| std::env::var(k).ok());
    let settings = settings_file.as_deref().map_or_else(
        || Settings::default_for(&home),
        |f| Settings::load(f, &home),
    );

    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "npc-framework: game menu".into(),
            resolution: (960, 720).into(),
            ..default()
        }),
        // Closing quits: every process the game started stops first
        // (`screens::close_quits`, `quit`).
        close_when_requested: false,
        ..default()
    }));
    if !app.is_plugin_added::<TabNavigationPlugin>() {
        app.add_plugins(TabNavigationPlugin);
    }
    app.insert_resource(ClearColor(ui::BACKDROP))
        .insert_resource(AppSettings(settings))
        .insert_resource(BuildTools::detect(&exe_dir))
        .insert_resource(Paths {
            exe_dir,
            settings_file,
        })
        .add_plugins((
            ScreensPlugin { start: args.screen },
            FieldClipboardPlugin,
            PowerPlugin,
        ))
        .add_systems(Startup, |mut commands: Commands| {
            commands.spawn(Camera2d);
        })
        .add_systems(Update, screens::close_quits);
    if let Some(agent) = args.resume {
        app.add_systems(Startup, move |mut commands: Commands| {
            let entry = Entry::ExplorerResume {
                agent: agent.clone(),
                choice: None,
                mode: launch::RunMode::Window,
            };
            commands.queue(move |w: &mut World| screens::apply_action(w, Action::Launch(entry)));
        });
    }
    if let Some(agent) = args.detached {
        app.add_systems(Startup, move |mut commands: Commands| {
            let agent = agent.clone();
            commands.queue(move |w: &mut World| detached::watch_running(w, &agent));
        });
    }
    if let Some(path) = args.screenshot {
        app.insert_resource(ScreenshotAt {
            path,
            after_secs: args.after_secs,
        })
        .add_systems(Last, take_screenshot);
    }
    app.run();
}

fn take_screenshot(
    mut commands: Commands,
    time: Res<Time<Real>>,
    shot: Res<ScreenshotAt>,
    mut requested: Local<bool>,
) {
    if *requested || time.elapsed_secs() < shot.after_secs {
        return;
    }
    *requested = true;
    if let Some(dir) = shot.path.parent().filter(|d| !d.as_os_str().is_empty())
        && let Err(e) = std::fs::create_dir_all(dir)
    {
        error!("game_menu: cannot create {}: {e}", dir.display());
    }
    info!("game_menu: taking screenshot -> {}", shot.path.display());
    let mut save = save_to_disk(shot.path.clone());
    commands.spawn(Screenshot::primary_window()).observe(
        move |captured: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>| {
            save(captured);
            exit.write(AppExit::Success);
        },
    );
}
