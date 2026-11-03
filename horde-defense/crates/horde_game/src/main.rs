mod animation;
mod art;
mod atmosphere_audio;
mod audio;
mod capture_sequence;
mod cursor;
mod defender_presentation;
mod diagnostics;
mod environment;
mod gear_appearance;
mod inspector;
mod player_model;
mod runner;
mod settings;
mod ui;
mod visual_assets;
mod worldmap;

use bevy::{
    app::AppExit,
    prelude::*,
    window::{WindowCloseRequested, WindowResolution},
};
use bevy_egui::{EguiPlugin, EguiPrimaryContextPass};

fn validate_startup_catalogs() -> Result<(), String> {
    horde_core::content::content().validate()?;
    horde_core::world::world().validate()?;
    horde_core::skill_catalog::skills().validate()?;
    let _ = horde_core::enemy_catalog::enemy_catalog();
    horde_core::ordnance::validate_catalog()?;
    horde_core::progression::catalog().validate()?;
    horde_core::quality::catalog().validate()?;
    horde_core::balance::catalog().validate()?;
    player_model::startup::initialize_player_models(&asset_directory())?;
    for (name, source) in environment::catalog_sources() {
        eprintln!("Lastlight catalogue {name}: {source}");
    }
    Ok(())
}
fn main() -> AppExit {
    if let Err(error) = validate_startup_catalogs() {
        eprintln!("Invalid startup content: {error}");
        return AppExit::error();
    }
    if std::env::args().any(|a| a == "--smoke") {
        match smoke() {
            Ok(report) => println!("{report}"),
            Err(e) => {
                eprintln!("Smoke failed: {e}");
                std::process::exit(1);
            }
        }
        return AppExit::Success;
    }
    let (width, height) = std::env::var("LASTLIGHT_REVIEW_SIZE")
        .ok()
        .and_then(|s| {
            s.split_once('x')
                .and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?)))
        })
        .unwrap_or((1440, 900));
    let mut app = App::new();
    if environment::path("LASTLIGHT_CAPTURE_SEQUENCE").is_some() {
        if environment::path("LASTLIGHT_PERF_REPORT").is_some()
            || environment::string("LASTLIGHT_REVIEW_SCREEN").is_none()
            || environment::flag("LASTLIGHT_PRACTICE")
            || !environment::flag("LASTLIGHT_REVIEW_MOTION")
        {
            eprintln!(
                "Animation export requires an isolated motion review and cannot be combined with performance measurement or a persistent campaign"
            );
            return AppExit::error();
        }
        app.insert_resource(bevy::time::TimeUpdateStrategy::FixedTimesteps(1));
        app.insert_resource(bevy::winit::WinitSettings::continuous());
    }
    // Performance sampling keeps ordinary window scheduling. Only deterministic
    // visual export above overrides time and redraw cadence.
    app.insert_resource(ClearColor(Color::srgb(0.067, 0.09, 0.094)))
        .insert_resource(ui::Frontend::load())
        .insert_resource(Time::<Fixed>::from_hz(60.0))
        .add_plugins(
            DefaultPlugins
                .set(AssetPlugin {
                    file_path: asset_directory().to_string_lossy().into_owned(),
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
        )
        .add_plugins(EguiPlugin::default())
        .add_plugins(atmosphere_audio::AtmosphereAudioPlugin)
        .add_systems(
            Startup,
            (
                |mut commands: Commands| {
                    commands.spawn(Camera2d);
                },
                visual_assets::load_assets,
            ),
        )
        .add_systems(EguiPrimaryContextPass, ui::draw)
        .add_systems(
            PostUpdate,
            cursor::sync_native_cursor.after(bevy_egui::EguiPostUpdateSet::ProcessOutput),
        )
        .add_systems(PreUpdate, runner::apply_commands::<ui::Frontend>)
        .add_systems(FixedUpdate, runner::advance::<ui::Frontend>)
        .add_systems(
            Update,
            (
                close_window,
                runner::persist::<ui::Frontend>,
                runner::play_sounds::<ui::Frontend>,
                runner::update_effects_gain::<ui::Frontend>,
                capture,
                show_window,
                diagnostics::capture_performance,
                capture_sequence::capture_frames,
            )
                .chain(),
        )
        .add_systems(
            Update,
            publish_scene_audio.before(atmosphere_audio::AmbienceUpdate),
        )
        .run()
}

fn publish_scene_audio(
    frontend: Res<ui::Frontend>,
    mut target: ResMut<atmosphere_audio::SceneAudioTarget>,
) {
    *target = frontend.ambience_target();
}

fn asset_directory() -> std::path::PathBuf {
    if let Some(path) = environment::path("LASTLIGHT_ASSET_DIR") {
        return path;
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
fn close_window(
    mut close: MessageReader<WindowCloseRequested>,
    mut frontend: ResMut<ui::Frontend>,
) {
    if close.read().next().is_some() {
        frontend
            .runtime
            .enqueue(runner::GameCommand::SetPaused(true), "");
        frontend
            .runtime
            .enqueue(runner::GameCommand::Save(runner::SaveIntent::Quit), "");
    }
}

fn show_window(mut window: Single<&mut Window>, frames: Res<bevy::diagnostic::FrameCount>) {
    if frames.0 == 3 {
        window.visible = true;
    }
}

#[derive(Default)]
struct StaticCapture {
    frames: u32,
    ready_frames: u32,
    index: u32,
    requested: Option<(u32, std::path::PathBuf)>,
}
fn capture(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    mut state: Local<StaticCapture>,
    assets: Res<visual_assets::VisualAssets>,
    images: Res<Assets<Image>>,
    mut contexts: bevy_egui::EguiContexts,
    mut exit: MessageWriter<AppExit>,
) {
    state.frames += 1;
    if assets.all_loaded(&images) {
        state.ready_frames += 1;
    } else {
        state.ready_frames = 0;
    }
    // Warm up after real asset readiness, rather than waiting 120 frames in an
    // unfocused window. Extra completed frames allow GPU uploads/fonts to settle.
    let automated = if state.requested.is_none() && state.frames >= 12 && state.ready_frames >= 8 {
        environment::path("LASTLIGHT_REVIEW_CAPTURE")
    } else {
        None
    };
    if keys.just_pressed(KeyCode::F12) || automated.is_some() {
        use bevy::render::view::screenshot::{Screenshot, save_to_disk};
        let path = automated.clone().unwrap_or_else(|| {
            environment::path("LASTLIGHT_CAPTURE_DIR")
                .unwrap_or_else(|| settings::save_directory().join("screenshots"))
                .join(format!("lastlight-{}.png", state.index))
        });
        if let Some(folder) = path.parent()
            && let Err(error) = std::fs::create_dir_all(folder)
        {
            eprintln!("Could not create screenshot folder: {error}");
            return;
        }
        if let Some(report_path) = environment::path("LASTLIGHT_PLAYER_MODEL_REVIEW_REPORT")
            .or_else(|| environment::path("LASTLIGHT_PLAYER_MODEL_PARITY_REPORT"))
        {
            let report = contexts
                .ctx_mut()
                .ok()
                .and_then(|ctx| ui::player_model_review_report(ctx));
            let result = report
                .ok_or_else(|| "Native player-model review metadata is unavailable".to_owned())
                .and_then(|report| {
                    serde_json::to_vec_pretty(&report).map_err(|error| error.to_string())
                })
                .and_then(|bytes| {
                    if let Some(folder) = report_path.parent() {
                        std::fs::create_dir_all(folder).map_err(|error| error.to_string())?;
                    }
                    std::fs::write(&report_path, bytes).map_err(|error| error.to_string())
                });
            if let Err(error) = result {
                eprintln!("Native player-model review manifest failed: {error}");
                exit.write(AppExit::error());
                return;
            }
        }
        state.index += 1;
        if automated.is_some() {
            state.requested = Some((state.frames, path.clone()));
        }
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(path));
    }
    if environment::flag("LASTLIGHT_REVIEW_EXIT")
        && environment::path("LASTLIGHT_PERF_REPORT").is_none()
        && environment::path("LASTLIGHT_CAPTURE_SEQUENCE").is_none()
    {
        if state
            .requested
            .as_ref()
            .is_some_and(|(frame, path)| state.frames >= frame + 4 && path.is_file())
        {
            exit.write(AppExit::Success);
        } else if state.frames >= 120 {
            eprintln!("Native capture timed out before assets or the completed PNG were ready");
            exit.write(AppExit::error());
        }
    }
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
