//! The stats window (FPS, tick, hashes, unit count, local player, runner status) and the time
//! controls, which drive [`LocalDriver`] directly: pause, step and speed are driver controls,
//! not commands (ADR-0008).
use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

use crate::driver::LocalDriver;
use crate::link::{RunnerLink, RunnerStatus, SnapshotBuffer};
use crate::orders::{SPEED_STEPS, TestbedKeys};
use crate::view::LocalPlayer;

/// Draws the stats window.
pub(crate) fn stats_window(
    mut contexts: EguiContexts,
    diagnostics: Option<Res<DiagnosticsStore>>,
    status: Res<RunnerStatus>,
    buffer: Res<SnapshotBuffer>,
    link: Option<Res<RunnerLink>>,
    player: Option<Res<LocalPlayer>>,
    driver: Option<Res<LocalDriver>>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    let fps = diagnostics
        .as_deref()
        .and_then(|d| d.get(&FrameTimeDiagnosticsPlugin::FPS))
        .and_then(|fps| fps.smoothed());
    egui::Window::new("Stats").show(ctx, |ui| {
        ui.monospace(match fps {
            Some(fps) => format!("fps        {fps:.0}"),
            None => "fps        -".to_owned(),
        });
        ui.monospace(format!(
            "tick       {}",
            status.last_tick.map_or("-".to_owned(), |t| t.0.to_string())
        ));
        ui.monospace(format!(
            "last hash  {}",
            status
                .last_hash
                .map_or("-".to_owned(), |h| format!("{h:016x}"))
        ));
        ui.monospace(format!(
            "rules hash {}",
            link.as_ref()
                .map_or("-".to_owned(), |l| format!("{:016x}", l.header.rules_hash))
        ));
        ui.monospace(format!(
            "units      {}",
            buffer.curr.as_ref().map_or(0, |s| s.entities.len())
        ));
        ui.monospace(format!(
            "player     {}",
            player.map_or("-".to_owned(), |p| p.0.0.to_string())
        ));
        let runner = match (&status.failed, driver.as_deref()) {
            (Some(failure), _) => format!("FAILED at tick {}: {}", failure.tick.0, failure.message),
            (None, None) => "no match".to_owned(),
            (None, Some(d)) if d.is_paused() => "paused".to_owned(),
            (None, Some(d)) => format!("running x{}", d.speed()),
        };
        ui.monospace(format!("runner     {runner}"));
    });
    Ok(())
}

/// Draws the time controls: pause or resume, single step, and speed.
pub(crate) fn time_controls_window(
    mut contexts: EguiContexts,
    driver: Option<ResMut<LocalDriver>>,
    keys: Option<Res<TestbedKeys>>,
) -> Result {
    let Some(mut driver) = driver else {
        return Ok(());
    };
    let ctx = contexts.ctx_mut()?;
    egui::Window::new("Time").show(ctx, |ui| {
        ui.horizontal(|ui| {
            if driver.is_paused() {
                if ui.button("Resume").clicked() {
                    driver.resume();
                }
                if ui.button("Step").clicked() {
                    driver.step(1);
                }
            } else if ui.button("Pause").clicked() {
                driver.pause();
            }
        });
        ui.horizontal(|ui| {
            let mut speed = driver.speed();
            for x in SPEED_STEPS {
                ui.selectable_value(&mut speed, x, format!("x{x}"));
            }
            // Writing only on change keeps `LocalDriver` unchanged on idle frames.
            if speed != driver.speed() {
                driver.set_speed(speed);
            }
        });
        if let Some(keys) = keys {
            ui.separator();
            ui.small(format!(
                "{:?} pause, {:?}/{:?} speed, {:?} next player",
                keys.pause, keys.speed_down, keys.speed_up, keys.cycle_player
            ));
            ui.small(format!(
                "{:?} stop, {:?} rotate, {:?} follow; right click moves, Shift queues",
                keys.stop, keys.rotate, keys.follow
            ));
        }
    });
    Ok(())
}
