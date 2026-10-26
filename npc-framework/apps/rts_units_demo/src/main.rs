//! RTS units demo: the reference integration of `npc_framework` into a
//! foreign Bevy game (see the crate docs in `lib.rs`).
//!
//! ```text
//! cargo run -p rts_units_demo -- [--units N] [--seed N] [--idle]
//!     [--screenshot PATH --after-secs S] [--panels journal,perf]
//! cargo run -p rts_units_demo -- --headless-ticks N [--units N] [--seed N]
//! ```
//!
//! `--headless-ticks N` runs N sim ticks without a window and prints one JSON
//! summary (units arrived, ticks, state hash; plus wall time) — exit 0 when
//! every unit arrived, 1 otherwise, 2 on a usage error. `--idle` skips the
//! scripted opening orders.

use std::path::PathBuf;
use std::time::Instant;

use bevy::prelude::*;
use npc_framework::prelude::JournalPlugin;
use npc_framework::view_helpers::{DebugPanels, ScreenshotRequest};
use rts_units_demo::sim::{self, DEFAULT_UNITS};
use rts_units_demo::{host, view};

const USAGE: &str = "usage: rts_units_demo [--units N] [--seed N] [--idle] \
[--screenshot PATH --after-secs S] [--panels journal,perf] | --headless-ticks N [--units N] [--seed N]";

struct Args {
    units: u32,
    seed: u64,
    idle: bool,
    screenshot: Option<PathBuf>,
    after_secs: f32,
    panels: DebugPanels,
    headless_ticks: Option<u64>,
}

fn usage(msg: &str) -> ! {
    if !msg.is_empty() {
        eprintln!("rts_units_demo: {msg}");
    }
    eprintln!("{USAGE}");
    std::process::exit(2);
}

fn parse_args() -> Args {
    let mut args = Args {
        units: DEFAULT_UNITS,
        seed: 1,
        idle: false,
        screenshot: None,
        after_secs: 4.0,
        panels: DebugPanels::default(),
        headless_ticks: None,
    };
    let mut it = std::env::args().skip(1);
    while let Some(flag) = it.next() {
        match flag.as_str() {
            "--idle" => {
                args.idle = true;
                continue;
            }
            "-h" | "--help" => usage(""),
            _ => {}
        }
        let Some(value) = it.next() else {
            usage(&format!("{flag} needs a value"))
        };
        let bad = |what: &str| -> ! { usage(&format!("{flag}: `{value}` is not {what}")) };
        match flag.as_str() {
            "--units" => args.units = value.parse().unwrap_or_else(|_| bad("a number")),
            "--seed" => args.seed = value.parse().unwrap_or_else(|_| bad("a number")),
            "--screenshot" => args.screenshot = Some(PathBuf::from(value)),
            "--after-secs" => args.after_secs = value.parse().unwrap_or_else(|_| bad("a number")),
            "--headless-ticks" => {
                args.headless_ticks = Some(value.parse().unwrap_or_else(|_| bad("a number")));
            }
            "--panels" => {
                for name in value.split(',').map(str::trim).filter(|s| !s.is_empty()) {
                    match name {
                        "journal" => args.panels.journal = true,
                        "perf" => args.panels.perf = true,
                        _ => bad("journal or perf"),
                    }
                }
            }
            _ => usage(&format!("unknown flag {flag}")),
        }
    }
    if args.units == 0 {
        usage("--units must be > 0");
    }
    args
}

fn main() {
    let args = parse_args();
    if let Some(ticks) = args.headless_ticks {
        headless(&args, ticks);
    }
    windowed(args);
}

/// `--headless-ticks`: no window, no renderer, JSON on stdout.
fn headless(args: &Args, ticks: u64) -> ! {
    let start = Instant::now();
    let summary = sim::run_headless(args.seed, args.units, ticks);
    let secs = start.elapsed().as_secs_f64();
    let mut json = serde_json::to_value(&summary).expect("summary serializes");
    json["wall_ms"] = serde_json::json!((secs * 1000.0).round());
    json["ticks_per_sec"] = serde_json::json!((summary.ticks as f64 / secs.max(1e-9)).round());
    println!("{json}");
    std::process::exit(i32::from(summary.arrived != summary.units));
}

/// What the startup system spawns.
#[derive(Resource, Clone, Copy)]
struct Setup {
    units: u32,
    script: bool,
}

fn spawn_demo(world: &mut World) {
    let setup = *world.resource::<Setup>();
    sim::spawn_world(world, setup.units);
    if setup.script
        && let Err(e) = sim::issue_script(world)
    {
        error!("rts_units_demo: script orders rejected: {e}");
    }
}

fn windowed(args: Args) {
    let mut app = App::new();
    let plugins = DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "npc-framework — RTS units demo".into(),
            resolution: (1280, 720).into(),
            ..default()
        }),
        ..default()
    });
    // The profiler's layer on Bevy's LogPlugin (F4 panel; `NpcPerfPlugin` below).
    #[cfg(feature = "perf")]
    let plugins = plugins.set(npc_framework::perf::log_plugin());
    app.add_plugins(plugins);
    // The host's world first: its own fixed rate and physics.
    host::add_host(&mut app);
    // The framework at 10 Hz in the same FixedUpdate, plus nav and our content.
    sim::add_sim(&mut app, sim::npc_plugins(args.seed));
    app.add_plugins((JournalPlugin::default(), view::RtsViewPlugin))
        .insert_resource(args.panels)
        .insert_resource(Setup {
            units: args.units,
            script: !args.idle,
        })
        .add_systems(Startup, (spawn_demo, view::preset_groups).chain());
    if let Some(path) = args.screenshot {
        app.insert_resource(ScreenshotRequest {
            path,
            after_secs: args.after_secs,
        })
        .insert_resource(view::DemoDrag {
            squad: 1,
            after_secs: args.after_secs * 0.5,
        });
    }
    #[cfg(feature = "perf")]
    app.add_plugins(
        npc_framework::perf::NpcPerfPlugin::new()
            .tag("rts_units_demo::sim", "rts.sim")
            .tag("rts_units_demo::view", "rts.view")
            .tag("rts_units_demo::host", "rts.host"),
    );
    app.run();
}
