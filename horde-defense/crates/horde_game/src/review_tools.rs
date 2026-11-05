//! `ReviewToolsPlugin`: isolated review and measurement services. Compiled only
//! with the `review` feature and added only when `LaunchConfig::review` is set,
//! so ordinary play and practice never schedule these systems.
use crate::{capture_sequence, diagnostics, launch::LaunchConfig, ui, visual_assets};
use bevy::{
    app::AppExit,
    prelude::*,
    render::view::screenshot::{Screenshot, save_to_disk},
};

pub struct ReviewToolsPlugin;
impl Plugin for ReviewToolsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (automated_capture, capture_sequence::capture_frames),
        );
        // The report reads the frame `PerfPlugin` closes at the end of `Last`,
        // so each phase is attributed to the frame that did the work.
        if app.world().resource::<LaunchConfig>().perf().is_some() {
            app.add_systems(
                Last,
                diagnostics::capture_performance.after(diagnostics::PerfSet::CloseFrame),
            );
        }
    }
}

#[derive(Default)]
struct StaticCapture {
    frames: u32,
    ready_frames: u32,
    requested: Option<(u32, std::path::PathBuf)>,
}
/// `LASTLIGHT_REVIEW_CAPTURE`: one screenshot once assets are ready, plus the
/// optional player-model manifest; `LASTLIGHT_REVIEW_EXIT` exits once it exists.
fn automated_capture(
    mut commands: Commands,
    mut state: Local<StaticCapture>,
    assets: Res<visual_assets::VisualAssets>,
    mut contexts: bevy_egui::EguiContexts,
    config: Res<LaunchConfig>,
    mut exit: MessageWriter<AppExit>,
) {
    state.frames += 1;
    if assets.ready() {
        state.ready_frames += 1;
    } else {
        state.ready_frames = 0;
    }
    // Warm up after real asset readiness, rather than waiting 120 frames in an
    // unfocused window. Extra completed frames allow GPU uploads/fonts to settle.
    // Readiness is published after the upload pass, one frame after the images
    // appear, so 7 ready frames keep the capture on the same frame as before.
    let automated = if state.requested.is_none() && state.frames >= 12 && state.ready_frames >= 7 {
        config.capture_path().cloned()
    } else {
        None
    };
    if let Some(path) = automated {
        if let Some(folder) = path.parent()
            && let Err(error) = std::fs::create_dir_all(folder)
        {
            error!("Could not create screenshot folder: {error}");
            return;
        }
        if let Some(report_path) = config.review().and_then(|r| r.model_report.as_ref()) {
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
                    std::fs::write(report_path, bytes).map_err(|error| error.to_string())
                });
            if let Err(error) = result {
                error!("Native player-model review manifest failed: {error}");
                exit.write(AppExit::error());
                return;
            }
        }
        state.requested = Some((state.frames, path.clone()));
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(path));
    }
    if config.review().is_some_and(|r| r.exit)
        && config.perf().is_none()
        && config.sequence_path().is_none()
    {
        if state
            .requested
            .as_ref()
            .is_some_and(|(frame, path)| state.frames >= frame + 4 && path.is_file())
        {
            exit.write(AppExit::Success);
        } else if state.frames >= 120 {
            error!("Native capture timed out before assets or the completed PNG were ready");
            exit.write(AppExit::error());
        }
    }
}
