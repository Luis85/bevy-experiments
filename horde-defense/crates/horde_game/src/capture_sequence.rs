//! Opt-in deterministic visual evidence, separate from real-time performance sampling.
use bevy::{
    app::AppExit,
    prelude::*,
    render::view::screenshot::{Screenshot, save_to_disk},
};
use serde::Serialize;
use std::path::PathBuf;

fn prepare_directory(directory: &std::path::Path) -> std::io::Result<()> {
    if let Some(parent) = directory.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    // Exclusive creation prevents interrupted-run PNGs becoming fresh evidence.
    std::fs::create_dir(directory)
}

const FIRST: u32 = 60;
const STEP: u32 = 4;
const COUNT: u32 = 24;
#[derive(Default)]
pub struct SequenceCapture {
    initialized: bool,
    directory: Option<PathBuf>,
    frame: u32,
    records: Vec<SequenceFrame>,
    done: bool,
}
#[derive(Serialize)]
struct SequenceFrame {
    file: String,
    native_frame: u32,
    simulation_tick: Option<u64>,
    lane_count: Option<u8>,
    width_yards: Option<f32>,
    live_enemies: usize,
    latest_throw_tick: Option<u64>,
    latest_melee_tick: Option<u64>,
    latest_melee_target_id: Option<u64>,
    latest_shot_tick: Option<u64>,
    throw_event_count: usize,
    melee_event_count: usize,
    active_projectile_count: usize,
    active_projectile_kinds: Vec<horde_core::ordnance::OrdnanceKind>,
    reload_left: Option<u32>,
    melee_commit: Option<u32>,
    corpse_count: usize,
}
/// The shortest decimal of an `f32`, so a JSON reader sees `1.35`, not
/// `1.350000023841858` (a `serde_json::Value` holds every number as `f64`).
fn decimal(value: f32) -> f64 {
    value.to_string().parse().unwrap_or(f64::from(value))
}
/// The motion `manifest.json`, version 2: version 1 plus the run's effective
/// reduced-effects setting, the logical window size in points, the UI scale
/// egui draws at (`settings::applied_scale` of the live settings, never the
/// raw saved or requested value) and the Windows display scale (PNG pixels per
/// point), so capture_motion can prove which mode and viewport produced the
/// frames.
fn sequence_manifest(
    scene: Option<&str>,
    settings: &crate::settings::Settings,
    (width, height): (u32, u32),
    windows_scale: f32,
    frames: &[SequenceFrame],
) -> serde_json::Value {
    serde_json::json!({
        "version": 2,
        "scene": scene,
        "reduced_effects": settings.reduced_effects,
        "width": width,
        "height": height,
        "scale": decimal(crate::settings::applied_scale(settings.scale)),
        "windows_scale": decimal(windows_scale),
        "timing": "Fixed presentation timestep per native frame; authoritative ticks advance only in combat fixtures. NOT a real-time FPS measurement",
        "nominal_video_fps": 60 / STEP,
        "frames": frames,
    })
}
/// The logical window size in points and the Windows display scale, read
/// from the primary window. Without one nothing was measured, so the export
/// fails instead of writing the requested size and a display scale of 1.0.
fn measured_viewport(window: Option<&Window>) -> Result<((u32, u32), f32), &'static str> {
    let window = window.ok_or("no primary window to measure the viewport and display scale")?;
    Ok((
        (
            window.resolution.width().round() as u32,
            window.resolution.height().round() as u32,
        ),
        window.resolution.scale_factor(),
    ))
}
#[allow(clippy::too_many_arguments)]
pub fn capture_frames(
    mut commands: Commands,
    session: Res<crate::runner::GameSession>,
    settings: Res<crate::settings::Settings>,
    mut battle: ResMut<crate::ui::BattleUi>,
    mut state: Local<SequenceCapture>,
    config: Res<crate::launch::LaunchConfig>,
    mut exit: MessageWriter<AppExit>,
    window: Option<Single<&Window, With<bevy::window::PrimaryWindow>>>,
) {
    if !state.initialized {
        state.initialized = true;
        state.directory = config.sequence_path().cloned();
        if let Some(directory) = &state.directory
            && let Err(error) = prepare_directory(directory)
        {
            error!("Animation export requires a fresh directory: {error}");
            state.done = true;
            exit.write(AppExit::error());
            return;
        }
    }
    if state.directory.is_none() || state.done {
        return;
    }
    state.frame += 1;
    let frame = state.frame;
    crate::ui::schedule_gesture_review(&config, &session.game, &mut battle, frame);
    if frame >= FIRST && (frame - FIRST).is_multiple_of(STEP) && (frame - FIRST) / STEP < COUNT {
        let Some(directory) = state.directory.clone() else {
            return;
        };
        let file = format!("frame-{:04}.png", (frame - FIRST) / STEP);
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(directory.join(&file)));
        let attempt = session.game.attempt();
        use horde_core::sim::EventKind;
        let latest = |kind| {
            attempt.and_then(|attempt| attempt.events.iter().rev().find(|event| event.kind == kind))
        };
        state.records.push(SequenceFrame {
            file,
            native_frame: frame,
            simulation_tick: attempt.map(|a| a.tick),
            lane_count: attempt.map(|a| a.lane_count),
            width_yards: attempt.map(|a| a.width_yards),
            live_enemies: attempt.map_or(0, |a| a.enemies.len()),
            latest_throw_tick: latest(EventKind::Throw).map(|event| event.tick),
            latest_melee_tick: latest(EventKind::Melee).map(|event| event.tick),
            latest_melee_target_id: latest(EventKind::Melee).and_then(|event| event.target_id),
            latest_shot_tick: latest(EventKind::Shot).map(|event| event.tick),
            throw_event_count: attempt.map_or(0, |a| {
                a.events
                    .iter()
                    .filter(|event| event.kind == EventKind::Throw)
                    .count()
            }),
            melee_event_count: attempt.map_or(0, |a| {
                a.events
                    .iter()
                    .filter(|event| event.kind == EventKind::Melee)
                    .count()
            }),
            active_projectile_count: attempt.map_or(0, |a| a.ordnance.projectiles.len()),
            active_projectile_kinds: attempt.map_or_else(Vec::new, |a| {
                a.ordnance
                    .projectiles
                    .iter()
                    .map(|projectile| projectile.kind)
                    .collect()
            }),
            reload_left: attempt.map(|a| a.reload_left),
            melee_commit: attempt.map(|a| a.melee_commit),
            corpse_count: attempt.map_or(0, |a| a.corpses.len()),
        });
    }
    if frame >= FIRST + STEP * (COUNT - 1) + 32 {
        state.done = true;
        let Some(directory) = state.directory.as_ref() else {
            return;
        };
        let valid = state.records.len() == COUNT as usize
            && state
                .records
                .iter()
                .all(|r| directory.join(&r.file).is_file());
        if !valid {
            error!("Animation export did not produce every frame");
            exit.write(AppExit::error());
            return;
        }
        let (viewport, windows_scale) = match measured_viewport(window.as_deref().copied()) {
            Ok(measured) => measured,
            Err(problem) => {
                error!("Animation manifest failed: {problem}");
                exit.write(AppExit::error());
                return;
            }
        };
        let report = sequence_manifest(
            config.scenario.as_deref(),
            &settings,
            viewport,
            windows_scale,
            &state.records,
        );
        let result = serde_json::to_vec_pretty(&report)
            .map_err(|e| e.to_string())
            .and_then(|bytes| {
                std::fs::write(directory.join("manifest.json"), bytes).map_err(|e| e.to_string())
            });
        match result {
            Ok(()) => {
                info!("Animation evidence: {}", directory.display());
                exit.write(AppExit::Success);
            }
            Err(e) => {
                error!("Animation manifest failed: {e}");
                exit.write(AppExit::error());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn manifest_records_reduced_effects_and_viewport() {
        let settings = |scale, reduced_effects| crate::settings::Settings {
            scale,
            reduced_effects,
            ..default()
        };
        let manifest =
            sequence_manifest(Some("peak"), &settings(1.35, true), (1050, 700), 2.0, &[]);
        assert_eq!(manifest["version"], 2);
        assert_eq!(manifest["scene"], "peak");
        assert_eq!(manifest["reduced_effects"], true);
        assert_eq!(manifest["width"], 1050);
        assert_eq!(manifest["height"], 700);
        // The shortest decimal of the f32, so the script compares 1.35, not 1.350000023841858.
        assert_eq!(manifest["scale"], 1.35);
        assert_eq!(manifest["windows_scale"], 2.0);
        assert_eq!(manifest["nominal_video_fps"], 15);
        assert_eq!(manifest["frames"], serde_json::json!([]));
        let normal = sequence_manifest(None, &settings(1.0, false), (1440, 900), 1.0, &[]);
        assert_eq!(normal["reduced_effects"], false);
        assert_eq!(normal["scene"], serde_json::Value::Null);
        assert_eq!(
            (normal["width"].clone(), normal["height"].clone()),
            (1440.into(), 900.into())
        );
        assert_eq!(normal["scale"], 1.0);
    }
    #[test]
    fn manifest_scale_is_the_drawn_scale_not_the_saved_value() {
        let scale = |scale| {
            let settings = crate::settings::Settings { scale, ..default() };
            sequence_manifest(None, &settings, (1440, 900), 1.0, &[])["scale"].clone()
        };
        // In range, the saved scale is the drawn one.
        assert_eq!(scale(1.2), 1.2);
        // Past the slider's range egui draws at the clamp, so the manifest does too.
        assert_eq!(scale(1.5), 1.35);
        assert_eq!(scale(0.5), 0.75);
    }
    #[test]
    fn manifest_viewport_is_measured_from_the_primary_window_or_refused() {
        let mut window = Window {
            resolution: bevy::window::WindowResolution::new(2100, 1400),
            ..default()
        };
        window.resolution.set_scale_factor(2.0);
        assert_eq!(measured_viewport(Some(&window)), Ok(((1050, 700), 2.0)));
        // Without a window nothing was measured, so the manifest must not
        // claim the requested size and an invented display scale of 1.0.
        assert!(measured_viewport(None).is_err());
    }
    #[test]
    fn interrupted_run_frames_are_never_accepted_or_overwritten() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory =
            std::env::temp_dir().join(format!("lastlight-sequence-{}-{nonce}", std::process::id()));
        prepare_directory(&directory).unwrap();
        let frame = directory.join("frame-0000.png");
        std::fs::write(&frame, b"previous capture").unwrap();
        assert_eq!(
            prepare_directory(&directory).unwrap_err().kind(),
            std::io::ErrorKind::AlreadyExists
        );
        assert_eq!(std::fs::read(&frame).unwrap(), b"previous capture");
        std::fs::remove_file(frame).unwrap();
        std::fs::remove_dir(directory).unwrap();
    }
}
