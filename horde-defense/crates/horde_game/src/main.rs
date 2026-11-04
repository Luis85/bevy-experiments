mod animation;
mod art;
mod atmosphere_audio;
mod audio;
#[cfg(feature = "review")]
mod capture_sequence;
mod cursor;
mod defender_presentation;
mod diagnostics;
mod gear_appearance;
mod inspector;
mod launch;
mod logging;
mod player_model;
#[cfg(feature = "review")]
mod review_tools;
mod runner;
mod settings;
mod ui;
mod visual_assets;
mod worldmap;

use bevy::{app::AppExit, prelude::*, window::WindowResolution};

fn validate_startup_catalogs(config: &launch::LaunchConfig) -> Result<(), String> {
    // Loads, validates and installs every catalogue; a bad override is a clean error.
    horde_core::catalog::init_catalogs().map_err(|error| error.to_string())?;
    player_model::startup::initialize_player_models(&asset_directory(config))?;
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
    logging::install_panic_hook(config.log_directory());
    if let Err(error) = validate_startup_catalogs(&config) {
        eprintln!("Invalid startup content: {error}");
        return AppExit::error();
    }
    if config.action == launch::LaunchAction::Smoke {
        match smoke() {
            Ok(report) => println!("{report}"),
            Err(e) => {
                eprintln!("Smoke failed: {e}");
                std::process::exit(1);
            }
        }
        return AppExit::Success;
    }
    let (width, height) = config.window;
    diagnostics::enable_work_timing(config.perf().is_some());
    let mut app = App::new();
    if config.sequence_path().is_some() {
        // LaunchConfig already rejected exports outside an isolated motion review.
        app.insert_resource(bevy::time::TimeUpdateStrategy::FixedTimesteps(1));
        app.insert_resource(bevy::winit::WinitSettings::continuous());
    }
    let asset_directory = asset_directory(&config);
    // Inserted before LogPlugin so the file layer can find the save directory.
    app.insert_resource(config);
    // Performance sampling keeps ordinary window scheduling. Only deterministic
    // visual export above overrides time and redraw cadence.
    app.insert_resource(ClearColor(Color::srgb(0.067, 0.09, 0.094)))
        .insert_resource(Time::<Fixed>::from_hz(60.0))
        .add_plugins(
            DefaultPlugins
                // `RUST_LOG`, when set, overrides this filter entirely.
                .set(bevy::log::LogPlugin {
                    filter: logging::DEFAULT_FILTER.into(),
                    custom_layer: logging::file_layer,
                    ..default()
                })
                .set(AssetPlugin {
                    file_path: asset_directory.to_string_lossy().into_owned(),
                    ..default()
                })
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Lastlight · Horde Defense".into(),
                        visible: false,
                        resolution: WindowResolution::new(width, height),
                        resize_constraints: bevy::window::WindowResizeConstraints {
                            min_width: 1050.0,
                            min_height: 700.0,
                            ..default()
                        },
                        ..default()
                    }),
                    close_when_requested: false,
                    ..default()
                }),
        );
    // After LogPlugin so Game::load startup events reach the console and log file.
    let frontend = ui::Frontend::load(app.world().resource::<launch::LaunchConfig>());
    let review = app
        .world()
        .resource::<launch::LaunchConfig>()
        .review()
        .is_some();
    app.insert_resource(frontend)
        .add_systems(Startup, log_launch_warnings)
        .add_plugins((ui::UiPlugin, atmosphere_audio::AtmosphereAudioPlugin))
        .add_systems(PreUpdate, runner::apply_commands::<ui::Frontend>)
        .add_systems(FixedUpdate, runner::advance::<ui::Frontend>)
        .add_systems(
            Update,
            (
                runner::persist::<ui::Frontend>,
                runner::play_sounds::<ui::Frontend>,
                runner::update_effects_gain::<ui::Frontend>,
            ),
        );
    #[cfg(feature = "review")]
    if review {
        app.add_plugins(review_tools::ReviewToolsPlugin);
    }
    #[cfg(not(feature = "review"))]
    let _ = review;
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
fn smoke() -> Result<String, String> {
    use horde_core::{
        content::{Mode, content},
        sim::{Game, Input},
        world::world,
    };
    content().validate()?;
    world().validate()?;
    let mut game = Game::default();
    game.campaign
        .choose_survivor(horde_core::survivors::SurvivorId::Rook)?;
    game.campaign.choose_start_country("DEU")?;
    game.deploy_country_stage(0, Mode::Personal)?;
    game.attempt.as_mut().unwrap().paused = false;
    for _ in 0..180 {
        game.tick(Input::default());
    }
    let path = std::env::temp_dir().join(format!("lastlight-smoke-{}.json", std::process::id()));
    game.save(&path)?;
    let resumed = Game::load(&path)?;
    if serde_json::to_value(&game).map_err(|e| e.to_string())?
        != serde_json::to_value(&resumed).map_err(|e| e.to_string())?
    {
        return Err("Save changed simulation state".into());
    }
    std::fs::remove_file(&path).map_err(|e| e.to_string())?;
    game.withdraw();
    if game.campaign.npc_available() {
        return Err("Fresh campaign unlocked the companion before its tree requirements".into());
    }
    let fresh = serde_json::to_value(&game).map_err(|e| e.to_string())?;
    if game.deploy_country_stage(0, Mode::Npc).is_ok()
        || serde_json::to_value(&game).map_err(|e| e.to_string())? != fresh
    {
        return Err("Locked companion deployment succeeded or mutated fresh state".into());
    }
    // Separately retain the old-save autonomous compatibility exercise. Fresh
    // progression above must not bypass the new authored companion unlock.
    game.campaign.legacy_features = true;
    game.deploy_country_stage(0, Mode::Npc)?;
    game.attempt.as_mut().unwrap().paused = false;
    for _ in 0..60 * 420 {
        game.tick(Input::default());
        if game.attempt.is_none() {
            break;
        }
    }
    let report = game
        .last_result
        .as_ref()
        .ok_or("NPC attempt did not terminate in seven simulated minutes")?;
    let baseline = format!(
        "PASS: {} country profiles; {} encounters; atomic resume identical; fresh companion gate is atomic; legacy autonomous {:?}: {} kills, {} coins in {:.1}s.\n{}",
        world().countries.len(),
        content().levels.len(),
        report.outcome,
        report.kills,
        report.coins,
        report.elapsed,
        report.cause
    );
    Ok(format!("{baseline}\n{}", horde_core::sim::smoke()?))
}
