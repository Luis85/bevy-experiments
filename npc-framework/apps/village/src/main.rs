//! Iteration-1 demo: one villager living on one tile.
//!
//! ```text
//! cargo run -p village -- [--seed N] [--speed N] [--screenshot PATH --after-secs S]
//!     [--panels journal,perf] [--journal FILE] [--record FILE]
//!     [--remote | --no-remote] [--remote-port N] [--render-remote-port N | --no-render-remote]
//!     [--out-dir DIR]
//! ```
//!
//! With the default `debug-tools` feature the game runs the built-in
//! profiler (F4 panel, `npc/perf`) and can serve the Bevy Remote Protocol on
//! `127.0.0.1:15702` (the `npc/*` methods of `npc_framework::tools::remote`
//! plus `npc/screenshot {path}` and the game menu's `app/quit {pid?}`, and
//! the `agent/*` / `world/state` methods of `npc_agent_io`; Bevy's
//! render-app server on the next port, 15703).
//! BRP is **on by default in dev builds and off in release builds**
//! (`--remote` / `--no-remote` override; `npc_framework::view_helpers::
//! remote`): it has no Origin/Host checks, so it is a development tool. RPC
//! file paths are relative to `--out-dir` (default `target/npc-out`).
//! `--features tracy` / `--features chrome-trace` add those profilers on top.
//!
//! The app owns the host side: the sim runs at 20 ticks/s from `Update` with
//! the framework's own accumulator (Bevy's `Time<Fixed>` is untouched), and
//! the speed keys (Space pause, 1/2/3/4 = 1x/3x/10x/0x, `.` one tick while
//! stopped) are bound here, on the kernel's `SimControl`.

use std::path::PathBuf;

use bevy::prelude::*;
use npc_framework::kernel::journal::{Journal, JournalSink};
use npc_framework::kernel::replay::ScenarioInfo;
use npc_framework::prelude::{JournalPlugin, NpcPlugins, SimControl};
use npc_framework::view_helpers::remote::RemoteArgs;
use npc_framework::view_helpers::{DebugPanels, ScreenshotRequest, ViewSystems};
use village_content::VillagePlugin;
use village_view::VillageViewPlugin;

/// Sim ticks per second.
const TICK_HZ: u32 = 20;
/// Multipliers selected by the keys 1/2/3/4.
pub const SPEED_PRESETS: [u32; 4] = [1, 3, 10, 0];

const USAGE: &str = "usage: village [--seed N] [--speed N] [--screenshot PATH] [--after-secs S] \
[--panels journal,perf] [--journal FILE] [--record FILE] [--remote | --no-remote] [--remote-port N] \
[--render-remote-port N | --no-render-remote] [--out-dir DIR]";

#[cfg_attr(not(feature = "debug-tools"), allow(dead_code))]
struct Args {
    seed: u64,
    speed: u32,
    screenshot: Option<PathBuf>,
    after_secs: f32,
    panels: DebugPanels,
    journal: Option<PathBuf>,
    record: Option<PathBuf>,
    remote: RemoteArgs,
}

fn usage(msg: &str) -> ! {
    if !msg.is_empty() {
        eprintln!("village: {msg}");
    }
    eprintln!("{USAGE}");
    std::process::exit(2);
}

fn parse_panels(list: &str) -> DebugPanels {
    let mut panels = DebugPanels::default();
    for name in list.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        match name {
            "journal" => panels.journal = true,
            "perf" => panels.perf = true,
            other => usage(&format!("unknown panel `{other}` (expected journal, perf)")),
        }
    }
    panels
}

fn parse_args() -> Args {
    let mut args = Args {
        seed: 1,
        speed: 1,
        screenshot: None,
        after_secs: 3.0,
        panels: DebugPanels::default(),
        journal: None,
        record: None,
        // BRP on in dev builds, off in release builds (and never without
        // `debug-tools`).
        remote: RemoteArgs::new(cfg!(debug_assertions) && cfg!(feature = "debug-tools")),
    };
    let mut it = std::env::args().skip(1);
    while let Some(flag) = it.next() {
        if args.remote.flag(&flag) {
            continue;
        }
        if matches!(flag.as_str(), "-h" | "--help") {
            usage("");
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
            "--seed" => args.seed = value.parse().unwrap_or_else(|_| bad("a number")),
            "--speed" => args.speed = value.parse().unwrap_or_else(|_| bad("a number")),
            "--screenshot" => args.screenshot = Some(PathBuf::from(value)),
            "--after-secs" => args.after_secs = value.parse().unwrap_or_else(|_| bad("a number")),
            "--panels" => args.panels = parse_panels(&value),
            "--journal" => args.journal = Some(PathBuf::from(value)),
            "--record" => args.record = Some(PathBuf::from(value)),
            _ => usage(&format!("unknown flag {flag}")),
        }
    }
    if !cfg!(feature = "debug-tools") && args.record.is_some() {
        usage("--record needs the debug-tools feature");
    }
    if !cfg!(feature = "debug-tools") && args.remote.remote {
        usage("--remote needs the debug-tools feature");
    }
    args
}

fn main() {
    let args = parse_args();
    let mut app = App::new();
    let plugins = DefaultPlugins
        .set(WindowPlugin {
            primary_window: Some(Window {
                title: "npc-framework — village".into(),
                resolution: (1280, 720).into(),
                ..default()
            }),
            ..default()
        })
        .set(village_view::asset_plugin());
    #[cfg(feature = "debug-tools")]
    let plugins = plugins.set(npc_framework::perf::log_plugin());
    app.add_plugins(plugins)
        .insert_resource(SimControl {
            multiplier: args.speed,
            ..default()
        })
        .insert_resource(args.panels)
        .add_plugins((
            NpcPlugins::new()
                .seed(args.seed)
                .tick_hz(TICK_HZ)
                .run_in(Update),
            VillagePlugin,
            // The journal never changes sim results; it feeds F3 and `npc/journal`.
            JournalPlugin::default(),
            VillageViewPlugin,
        ))
        .add_systems(
            Update,
            (speed_input, step_input).chain().in_set(ViewSystems::Input),
        )
        // Same world as `village_content::new_sim_app(seed, "village", 1)`,
        // so live recordings replay headless.
        .insert_resource(ScenarioInfo {
            name: "village".into(),
            villagers: 1,
        })
        .add_systems(Startup, spawn_village);
    if let Some(path) = &args.journal {
        match JournalSink::create(path) {
            Ok(sink) => app.world_mut().resource_mut::<Journal>().sink = Some(sink),
            Err(e) => usage(&format!("--journal {}: {e}", path.display())),
        }
    }
    #[cfg(feature = "debug-tools")]
    debug_tools::install(&mut app, &args);
    if let Some(path) = args.screenshot {
        app.insert_resource(ScreenshotRequest {
            path,
            after_secs: args.after_secs,
        });
    }
    app.run();
}

fn spawn_village(world: &mut World) {
    village_content::spawn_village(world);
}

/// Space pauses, 1/2/3/4 select 1x/3x/10x/0x.
fn speed_input(keys: Option<Res<ButtonInput<KeyCode>>>, mut control: ResMut<SimControl>) {
    let Some(keys) = keys else { return };
    if keys.just_pressed(KeyCode::Space) {
        control.paused = !control.paused;
    }
    let digits = [
        KeyCode::Digit1,
        KeyCode::Digit2,
        KeyCode::Digit3,
        KeyCode::Digit4,
    ];
    for (key, multiplier) in digits.into_iter().zip(SPEED_PRESETS) {
        if keys.just_pressed(key) {
            control.multiplier = multiplier;
        }
    }
}

/// `.` (period) runs exactly one tick while the sim is stopped.
fn step_input(keys: Option<Res<ButtonInput<KeyCode>>>, mut control: ResMut<SimControl>) {
    if keys.is_some_and(|k| k.just_pressed(KeyCode::Period)) && control.is_stopped() {
        control.step();
    }
}

#[cfg(feature = "debug-tools")]
mod debug_tools {
    use bevy::prelude::*;
    use npc_framework::agent_io::remote::AgentRemotePlugin;
    use npc_framework::perf::NpcPerfPlugin;
    use npc_framework::view_helpers::remote::{
        install_app_quit, install_remote, record_from_startup,
    };

    use crate::Args;

    pub fn install(app: &mut App, args: &Args) {
        // The profiler (its layer is in `perf::log_plugin()`, set on
        // `DefaultPlugins`): the default budgets for one villager, scaled for
        // the build profile (x10 in dev builds), checked once a mechanic has
        // `DEFAULT_MIN_SAMPLES` samples; `PerfHandle`; `end_frame` in `Last`.
        app.add_plugins(
            NpcPerfPlugin::new()
                .tags_with(village_content::perf_tags)
                .tags_with(village_view::perf_tags),
        );
        if install_remote(app, "village", &args.remote) {
            // agent/* + world/* (read-only here: the village has no
            // AgentIoPlugin, so its sim and content hash are unchanged).
            app.add_plugins(AgentRemotePlugin);
            // The menu's graceful quit (`app/quit`): exit 0.
            install_app_quit(app);
        }
        if let Some(path) = &args.record {
            record_from_startup(app, "village", path.clone());
        }
    }
}
