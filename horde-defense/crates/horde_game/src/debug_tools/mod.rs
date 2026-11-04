//! Developer tooling, compiled only with the `dev` feature: frame-time and
//! simulation overlays, single-tick stepping, state dumps, and a command
//! recorder whose output `replay` re-runs headless.
//!
//! Keys (F12 is the screenshot key): F2 frame time, F3 simulation panel,
//! F5 one tick while paused, F6 dump the game as JSON, F7 toggle recording.
//! (`bevy_dev_tools` has no release matching the locked Bevy, so the overlays
//! are plain egui.)
pub mod recording;
pub mod replay;

use crate::{
    launch::LaunchConfig,
    runner::{GameSession, Runtime},
};
use bevy::{
    diagnostic::{DiagnosticPath, DiagnosticsStore, FrameTimeDiagnosticsPlugin},
    prelude::*,
};
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};
use horde_core::sim::Game;
use std::path::{Path, PathBuf};

/// `--replay <path>`: replays headless and prints the final digest.
/// Returns the process exit code.
#[allow(clippy::print_stdout, clippy::print_stderr)]
// A headless command-line mode: stdout is its interface (scripts read the
// digest) and there is no logger yet.
pub fn run_replay(path: &Path) -> i32 {
    match replay::replay_file(path) {
        Ok(report) => {
            for warning in &report.warnings {
                eprintln!("warning: {warning}");
            }
            println!(
                "replayed {} ticks, {} commands",
                report.ticks, report.commands
            );
            println!("state_digest: {}", recording::hex(report.digest));
            match (&report.recorded_digest, report.matches_recording()) {
                (Some(recorded), true) => println!("matches recorded digest {recorded}"),
                (Some(recorded), false) => {
                    eprintln!("MISMATCH: recording ended with digest {recorded}");
                    return 1;
                }
                (None, _) => println!("recording has no end digest to compare"),
            }
            0
        }
        Err(error) => {
            eprintln!("Replay failed: {error}");
            1
        }
    }
}

#[derive(Resource, Default)]
struct DebugOverlays {
    frame_time: bool,
    sim_panel: bool,
}

pub struct DebugPlugin;
impl Plugin for DebugPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<FrameTimeDiagnosticsPlugin>() {
            app.add_plugins(FrameTimeDiagnosticsPlugin::default());
        }
        app.init_resource::<DebugOverlays>()
            .add_systems(Startup, record_from_launch)
            .add_systems(Update, hotkeys)
            .add_systems(Last, flush_recording)
            .add_systems(EguiPrimaryContextPass, draw_overlays);
    }
}

fn unix_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn start_recording(runtime: &mut Runtime, game: &Game, path: &Path) {
    match recording::Recorder::create(path, game, runtime.debug_snapshot().simulated_ticks) {
        Ok(recorder) => {
            runtime.start_recording(recorder);
            info!("Recording commands to {}", path.display());
        }
        Err(error) => error!("Could not start recording: {error}"),
    }
}

/// `--record` / `LASTLIGHT_RECORD`: start with the session.
fn record_from_launch(
    config: Res<LaunchConfig>,
    session: Res<GameSession>,
    mut runtime: ResMut<Runtime>,
) {
    if let Some(path) = &config.record {
        start_recording(&mut runtime, &session.game, path);
    }
}

fn flush_recording(mut runtime: ResMut<Runtime>) {
    runtime.flush_recording();
}

fn hotkeys(
    keys: Res<ButtonInput<KeyCode>>,
    config: Res<LaunchConfig>,
    session: Res<GameSession>,
    mut runtime: ResMut<Runtime>,
    mut overlays: ResMut<DebugOverlays>,
) {
    if keys.just_pressed(KeyCode::F2) {
        overlays.frame_time = !overlays.frame_time;
    }
    if keys.just_pressed(KeyCode::F3) {
        overlays.sim_panel = !overlays.sim_panel;
    }
    if keys.just_pressed(KeyCode::F5) {
        // Consumed by the runner's next fixed pass, never applied here.
        runtime.request_step();
    }
    if keys.just_pressed(KeyCode::F6) {
        match dump_game(&session.game, &config.save_directory) {
            Ok(path) => info!("Dumped game state to {}", path.display()),
            Err(error) => error!("Could not dump game state: {error}"),
        }
    }
    if keys.just_pressed(KeyCode::F7) {
        if runtime.stop_recording(&session.game, "stopped by F7") {
            info!("Recording stopped");
        } else {
            let path = config
                .save_directory
                .join("recordings")
                .join(format!("rec-{}.jsonl", unix_seconds()));
            start_recording(&mut runtime, &session.game, &path);
        }
    }
}

/// Writes the save payload as `<save dir>/dumps/game-<unix-ts>.json`.
fn dump_game(game: &Game, save_directory: &Path) -> Result<PathBuf, String> {
    let folder = save_directory.join("dumps");
    std::fs::create_dir_all(&folder).map_err(|e| format!("{}: {e}", folder.display()))?;
    let path = folder.join(format!("game-{}.json", unix_seconds()));
    let payload = serde_json::to_vec(game).map_err(|e| e.to_string())?;
    std::fs::write(&path, payload).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path)
}

fn smoothed(store: &DiagnosticsStore, path: &DiagnosticPath) -> f64 {
    store
        .get(path)
        .and_then(|diagnostic| diagnostic.smoothed())
        .unwrap_or(0.0)
}

fn draw_overlays(
    mut contexts: EguiContexts,
    overlays: Res<DebugOverlays>,
    diagnostics: Res<DiagnosticsStore>,
    session: Res<GameSession>,
    runtime: Res<Runtime>,
    config: Res<LaunchConfig>,
) -> Result {
    if !overlays.frame_time && !overlays.sim_panel {
        return Ok(());
    }
    let ctx = contexts.ctx_mut()?;
    if overlays.frame_time {
        let read = |path: &DiagnosticPath| smoothed(&diagnostics, path);
        egui::Window::new("Frame time")
            .title_bar(false)
            .resizable(false)
            .anchor(egui::Align2::LEFT_TOP, [8.0, 8.0])
            .show(ctx, |ui| {
                ui.monospace(format!(
                    "{:5.1} fps  {:6.2} ms",
                    read(&FrameTimeDiagnosticsPlugin::FPS),
                    read(&FrameTimeDiagnosticsPlugin::FRAME_TIME)
                ));
            });
    }
    if overlays.sim_panel {
        let lines = sim_panel_lines(&session.game, &runtime, config.record.is_some());
        egui::Window::new("Simulation")
            .resizable(false)
            .anchor(egui::Align2::RIGHT_TOP, [-8.0, 8.0])
            .show(ctx, |ui| {
                for line in lines {
                    ui.monospace(line);
                }
            });
    }
    Ok(())
}

/// The F3 panel text, separate from egui so it is testable.
fn sim_panel_lines(game: &Game, runtime: &Runtime, record_flag: bool) -> Vec<String> {
    let debug = runtime.debug_snapshot();
    let mut lines = vec![format!("seed          {}", game.seed())];
    match game.attempt() {
        Some(a) => {
            lines.push(format!("attempt       {} ({:?})", a.id, a.mode));
            lines.push(format!("tick          {}", a.tick));
            lines.push(format!(
                "paused        {}",
                a.paused || game.global_paused()
            ));
            lines.push(format!(
                "enemies       {}/{}",
                a.enemies.len(),
                a.enemy_cap()
            ));
            lines.push(format!("corpses       {}", a.corpses.len()));
        }
        None => lines.push(format!(
            "attempt       none (next id {})",
            game.next_attempt_id()
        )),
    }
    lines.push(format!(
        "speed         x{}{}",
        debug.speed,
        if debug.controls_enabled {
            ""
        } else {
            " (controls off)"
        }
    ));
    lines.push(format!(
        "runner ticks  {} (steps queued {})",
        debug.simulated_ticks, debug.step_requests
    ));
    lines.push(format!(
        "event seq     {}",
        debug
            .last_event_seq
            .map_or_else(|| "-".to_owned(), |s| s.to_string())
    ));
    lines.push(format!(
        "persistence   queued {} saves {} user-pending {} durable {} reset {}",
        debug.queued_commands,
        debug.save_requests,
        debug.user_save_pending,
        debug.durable_transition,
        debug.reset_in_progress
    ));
    lines.push(format!(
        "settlement    {} issues",
        debug.settlement_issue_total
    ));
    lines.push(match runtime.recording_path() {
        Some(path) => format!("recording     {}", path.display()),
        None if record_flag => "recording     stopped".to_owned(),
        None => "recording     off (F7)".to_owned(),
    });
    lines.push("F2 fps  F5 step  F6 dump  F7 record".to_owned());
    lines
}

#[cfg(test)]
mod tests;
