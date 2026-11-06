mod animation;
#[cfg(test)]
mod architecture_tests;
mod art;
#[cfg(test)]
mod asset_audit;
mod atmosphere_audio;
mod audio;
mod battle_view;
#[cfg(feature = "review")]
mod capture_sequence;
#[cfg(feature = "dev")]
mod debug_tools;
mod defender_presentation;
mod diagnostics;
mod gear_appearance;
mod inspector;
mod launch;
mod logging;
mod perf_budget;
mod player_model;
mod profile;
mod render_plan;
#[cfg(test)]
mod repo_checks;
mod review_scenes;
#[cfg(feature = "review")]
mod review_tools;
mod runner;
mod settings;
mod sfx;
mod ui;
mod visual_assets;
mod worldmap;

use bevy::{app::AppExit, prelude::*};

fn validate_startup_catalogs(config: &launch::LaunchConfig) -> Result<(), String> {
    // Loads, validates and installs every catalogue; a bad override is a clean error.
    horde_core::catalog::init_catalogs().map_err(|error| error.to_string())?;
    player_model::startup::initialize_player_models(&asset_directory(config))?;
    // After the enemy catalogue: every sprite it names must have atlas data.
    visual_assets::enemy_visuals::initialize(&asset_directory(config))?;
    for (name, source) in config.catalog_sources() {
        // Runs before LogPlugin installs the subscriber, so stderr is the only channel.
        #[allow(clippy::print_stderr)]
        {
            eprintln!("Lastlight catalogue {name}: {source}");
        }
    }
    Ok(())
}
// Startup validation and `--smoke` run before LogPlugin installs the subscriber, and
// scripts (verify_playtest.ps1) read the smoke report from stdout, so these keep
// plain stdout/stderr.
#[allow(clippy::print_stdout, clippy::print_stderr)]
fn main() -> AppExit {
    // Parsed once, before anything else; nothing below reads the environment.
    let config = match launch::LaunchConfig::from_process() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("{error}");
            return AppExit::error();
        }
    };
    if config.action == launch::LaunchAction::Help {
        print!("{}", launch::HELP);
        return AppExit::Success;
    }
    if config.action == launch::LaunchAction::ListScenes {
        // The capture scripts read scene names only from this listing.
        for line in review_scenes::list_lines(cfg!(feature = "review")) {
            println!("{line}");
        }
        return AppExit::Success;
    }
    logging::install_panic_hook(config.log_directory());
    if let Err(error) = validate_startup_catalogs(&config) {
        eprintln!("Invalid startup content: {error}");
        return AppExit::error();
    }
    if config.action == launch::LaunchAction::Smoke {
        match horde_core::sim::desktop_smoke() {
            Ok(report) => println!("{report}"),
            Err(e) => {
                eprintln!("Smoke failed: {e}");
                std::process::exit(1);
            }
        }
        return AppExit::Success;
    }
    #[cfg(feature = "dev")]
    if config.action == launch::LaunchAction::Replay
        && let Some(path) = &config.replay
    {
        // Headless: builds the recorded Game directly, no window or App.
        return match debug_tools::run_replay(path) {
            0 => AppExit::Success,
            code => AppExit::from_code(code as u8),
        };
    }
    // The one read of settings.json for this process, before the App: the
    // present mode and pipelined rendering are fixed from it (or a perf
    // override), and the frontend reuses it instead of reading again.
    let startup_settings =
        profile::read_settings(&profile::ProfilePaths::from_config(&config).settings());
    let plan = render_plan::render_plan(&startup_settings.settings, config.perf());
    let mut app = App::new();
    if config.sequence_path().is_some() {
        // LaunchConfig already rejected exports outside an isolated motion review.
        app.insert_resource(bevy::time::TimeUpdateStrategy::FixedTimesteps(1));
        app.insert_resource(bevy::winit::WinitSettings::continuous());
    }
    let plugins = render_plan::default_plugins(&config, &asset_directory(&config), &plan);
    // Inserted before LogPlugin so the file layer can find the save directory.
    app.insert_resource(config);
    // Performance sampling keeps ordinary window scheduling. Only deterministic
    // visual export above overrides time and redraw cadence.
    app.insert_resource(ClearColor(Color::srgb(0.067, 0.09, 0.094)))
        .add_plugins(plugins)
        .add_plugins(render_plan::DisplayPlugin(plan));
    let config = app.world().resource::<launch::LaunchConfig>().clone();
    // Perf launches only: the probe, its span systems and the GPU span.
    diagnostics::install(&mut app, &config);
    // After LogPlugin so Game::load startup events reach the console and log file.
    ui::FrontendParts::load(&config, startup_settings).insert_into(&mut app);
    let review = config.review().is_some();
    app.add_systems(Startup, log_launch_warnings).add_plugins((
        runner::SimRuntimePlugin,
        battle_view::BattleViewPlugin,
        sfx::SfxPlugin,
        ui::UiPlugin,
        atmosphere_audio::AtmosphereAudioPlugin,
    ));
    #[cfg(feature = "review")]
    if review {
        app.add_plugins(review_tools::ReviewToolsPlugin);
    }
    #[cfg(not(feature = "review"))]
    let _ = review;
    #[cfg(feature = "dev")]
    app.add_plugins(debug_tools::DebugPlugin);
    app.run()
}

/// Parse warnings are collected before logging exists; report them once it does.
fn log_launch_warnings(config: Res<launch::LaunchConfig>) {
    for warning in &config.warnings {
        warn!("{warning}");
    }
}

fn asset_directory(config: &launch::LaunchConfig) -> std::path::PathBuf {
    if let Some(path) = &config.asset_dir {
        return path.clone();
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(folder) = exe.parent()
    {
        let assets = folder.join("assets");
        if assets.is_dir() {
            return assets;
        }
    }
    std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets"))
}
