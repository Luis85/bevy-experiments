//! `ReviewToolsPlugin`: isolated review and measurement services. Compiled only
//! with the `review` feature and added only when `LaunchConfig::review` is set,
//! so ordinary play and practice never schedule these systems.
use crate::{
    capture_sequence, diagnostics, launch::LaunchConfig, runner::GameSession, settings::Settings,
    ui, visual_assets,
};
use bevy::{
    app::AppExit,
    prelude::*,
    render::view::screenshot::{Screenshot, save_to_disk},
    window::PrimaryWindow,
};
use bevy_egui::EguiPrimaryContextPass;

pub struct ReviewToolsPlugin;
impl Plugin for ReviewToolsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ui::ReviewFrameFacts>()
            .add_systems(
                Update,
                (automated_capture, capture_sequence::capture_frames),
            )
            .add_systems(
                EguiPrimaryContextPass,
                record_frame_facts
                    .after(ui::draw)
                    .run_if(|config: Res<LaunchConfig>| config.capture_path().is_some()),
            );
        // `LASTLIGHT_REVIEW_CUE_LOG`: the recorder `SfxPlugin` fills, and the
        // writer that appends its frames to the file.
        if app
            .world()
            .resource::<LaunchConfig>()
            .cue_log_path()
            .is_some()
        {
            app.init_resource::<crate::sfx::CueLogRecorder>()
                .add_systems(Last, write_cue_log);
        }
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

/// The open cue log, or why it could not be written (reported once).
#[derive(Default)]
enum CueLogFile {
    #[default]
    Unopened,
    Open(std::io::BufWriter<std::fs::File>),
    Failed,
}
/// Appends `frames` to the cue log at `path` as `horde_audio::cue_log` lines:
/// the header when the file is created, then one line per frame, flushed so a
/// session that is closed from outside keeps what it played.
fn append_cue_log(
    file: &mut CueLogFile,
    path: &std::path::Path,
    frames: &[horde_audio::cue_log::CueLogFrame],
) -> std::io::Result<()> {
    use std::io::Write;
    if matches!(file, CueLogFile::Unopened) {
        if let Some(folder) = path.parent() {
            std::fs::create_dir_all(folder)?;
        }
        let mut writer = std::io::BufWriter::new(std::fs::File::create(path)?);
        writeln!(writer, "{}", horde_audio::cue_log::header())?;
        *file = CueLogFile::Open(writer);
    }
    let CueLogFile::Open(writer) = file else {
        return Ok(());
    };
    for frame in frames {
        writeln!(writer, "{}", frame.line())?;
    }
    writer.flush()
}
/// The review service of the cue log: a file writer, not a widget. Each frame
/// it drains what `SfxPlugin` recorded into `LASTLIGHT_REVIEW_CUE_LOG`.
fn write_cue_log(
    config: Res<LaunchConfig>,
    mut recorder: ResMut<crate::sfx::CueLogRecorder>,
    mut file: Local<CueLogFile>,
) {
    let frames = std::mem::take(&mut recorder.frames);
    let Some(path) = config.cue_log_path() else {
        return;
    };
    if let Err(error) = append_cue_log(&mut file, path, &frames) {
        error!("Cue log {} failed: {error}", path.display());
        *file = CueLogFile::Failed;
    }
}

#[derive(Default)]
struct StaticCapture {
    frames: u32,
    ready_frames: u32,
    requested: Option<(u32, std::path::PathBuf)>,
}
/// Still captures only: refreshes `ReviewFrameFacts` from the pass `ui::draw`
/// just drew, plus the canvas and the window's scale factor.
#[allow(clippy::too_many_arguments)]
fn record_frame_facts(
    mut facts: ResMut<ui::ReviewFrameFacts>,
    session: Res<GameSession>,
    config: Res<LaunchConfig>,
    settings: Res<Settings>,
    status: Option<Res<ui::StatusUi>>,
    screen: Res<State<ui::Screen>>,
    tab: Option<Res<State<ui::Tab>>>,
    battle: Res<ui::BattleUi>,
    window: Option<Single<&Window, With<PrimaryWindow>>>,
    mut contexts: bevy_egui::EguiContexts,
) {
    let mut next = ui::ReviewFrameFacts::collect(
        &session.game,
        &config,
        &settings,
        status.as_deref(),
        *screen.get(),
        tab.map_or(ui::Tab::default(), |tab| *tab.get()),
        &battle,
    );
    next.canvas_rect = contexts.ctx_mut().ok().map(|ctx| {
        let r = ctx.content_rect();
        [r.left(), r.top(), r.right(), r.bottom()]
    });
    next.windows_scale = window.map_or(1.0, |window| window.scale_factor());
    *facts = next;
}

/// The capture sidecar: format version 1 plus the frame facts.
fn sidecar_json(facts: &ui::ReviewFrameFacts) -> Result<Vec<u8>, String> {
    #[derive(serde::Serialize)]
    struct Sidecar<'a> {
        version: u32,
        #[serde(flatten)]
        facts: &'a ui::ReviewFrameFacts,
    }
    serde_json::to_vec_pretty(&Sidecar { version: 1, facts }).map_err(|error| error.to_string())
}

/// `LASTLIGHT_REVIEW_CAPTURE`: one screenshot once assets are ready, its
/// `<capture>.json` sidecar (written first) and the optional player-model
/// manifest; `LASTLIGHT_REVIEW_EXIT` exits once the PNG exists.
fn automated_capture(
    mut commands: Commands,
    mut state: Local<StaticCapture>,
    assets: Res<visual_assets::VisualAssets>,
    mut contexts: bevy_egui::EguiContexts,
    config: Res<LaunchConfig>,
    facts: Res<ui::ReviewFrameFacts>,
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
        // The workshop's player-model manifest already owns `<capture>.json`.
        let sidecar = path.with_extension("json");
        let model_manifest = config
            .review()
            .and_then(|r| r.model_report.as_ref())
            .is_some_and(|report| *report == sidecar);
        if !model_manifest
            && let Err(error) = sidecar_json(&facts)
                .and_then(|bytes| std::fs::write(&sidecar, bytes).map_err(|e| e.to_string()))
        {
            error!("Review capture sidecar failed: {error}");
            exit.write(AppExit::error());
            return;
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

#[cfg(test)]
mod tests {
    use super::*;

    /// W1 review: the report system consumes `PerfFrame`, so it must run in
    /// `Last` after `PerfSet::CloseFrame`, or frame i's work lands in sample
    /// i - 1. Checks the schedule `ReviewToolsPlugin` builds: the system is
    /// in `Last`, not `Update`; it runs after `close_frame`; and the two are
    /// not an unordered pair on `PerfFrame`.
    #[test]
    fn performance_capture_runs_after_the_frame_closes() {
        let config = LaunchConfig::parse(std::iter::empty(), |name| {
            (name == "LASTLIGHT_PERF_REPORT").then(|| "perf.json".into())
        })
        .unwrap();
        assert!(config.perf().is_some());
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(config.clone());
        diagnostics::install(&mut app, &config);
        app.add_plugins(ReviewToolsPlugin);
        let capture = "::capture_performance";
        let in_update = app
            .world()
            .resource::<Schedules>()
            .get(Update)
            .unwrap()
            .graph()
            .systems
            .iter()
            .any(|(_, system, _)| system.name().to_string().ends_with(capture));
        assert!(!in_update, "capture_performance is scheduled in Update");
        let mut last = app
            .world_mut()
            .resource_mut::<Schedules>()
            .remove(Last)
            .expect("a Last schedule");
        last.initialize(app.world_mut()).unwrap();
        // Executable order is topological; keys name the conflicting pairs.
        let order: Vec<_> = last
            .systems()
            .unwrap()
            .map(|(key, system)| (key, system.name().to_string()))
            .collect();
        let key = |suffix: &str| {
            order
                .iter()
                .position(|(_, name)| name.ends_with(suffix))
                .unwrap_or_else(|| panic!("no {suffix} in Last: {order:?}"))
        };
        let (capture_at, close_at) = (key(capture), key("::close_frame"));
        assert!(capture_at > close_at, "{order:?}");
        let pair = [order[capture_at].0, order[close_at].0];
        let unordered = last
            .graph()
            .conflicting_systems()
            .0
            .iter()
            .any(|(a, b, _)| pair.contains(a) && pair.contains(b));
        assert!(
            !unordered,
            "capture_performance and close_frame are unordered"
        );
    }

    /// A6-18: the cue log on disk is the header plus one line per recorded
    /// frame, appended across writes, and `horde_audio::cue_log` reads it back
    /// as recorded.
    #[test]
    fn cue_log_file_reads_back_as_recorded() {
        use horde_audio::cue_log::{self, CueLogFrame, LoggedStart};
        let frame = |us: u64| CueLogFrame {
            us,
            dt_us: 16_667,
            fades: vec![us],
            starts: vec![LoggedStart {
                voice: u64::MAX - us,
                cue: "Rifle".into(),
                clip: "audio/sfx/rifle_1.wav".into(),
                speed: 1.03,
                lowpass_hz: Some(4_200.0),
                gains: [0.31, 0.27],
                compensated: true,
            }],
            ambience: [0.1, 0.2, 0.0, 0.05, 0.0, 0.0],
            density: 0.9,
        };
        let folder = std::env::temp_dir().join(format!("lastlight-cue-log-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        let path = folder.join("run").join("cues.jsonl");
        let mut file = CueLogFile::default();
        append_cue_log(&mut file, &path, &[frame(1), frame(2)]).unwrap();
        append_cue_log(&mut file, &path, &[]).unwrap();
        append_cue_log(&mut file, &path, &[frame(3)]).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(text.lines().count(), 4);
        assert_eq!(
            cue_log::parse(&text).unwrap(),
            [frame(1), frame(2), frame(3)]
        );
        drop(file);
        std::fs::remove_dir_all(&folder).unwrap();
    }

    /// The keys capture_review.ps1 and the camera review read from every
    /// still's sidecar; a missing measurement is an explicit null.
    #[test]
    fn sidecar_json_has_required_keys() {
        let facts = ui::ReviewFrameFacts {
            scene: "max-lanes".into(),
            screen: "Battle".into(),
            arena_rect: Some([24.0, 124.0, 1416.0, 816.0]),
            lanes: Some(17),
            ui_scale: 1.0,
            windows_scale: 2.0,
            ..Default::default()
        };
        let bytes = sidecar_json(&facts).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let object = value.as_object().expect("a JSON object");
        let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "arena_rect",
                "boss_banner_rect",
                "canvas_rect",
                "effects_rect",
                "hud_rows",
                "lanes",
                "lighting",
                "observer_arena_rect",
                "overlays",
                "reduced_effects",
                "scene",
                "screen",
                "storm",
                "tab",
                "ui_scale",
                "version",
                "width_yards",
                "windows_scale",
            ]
        );
        assert_eq!(object["version"], 1);
        assert_eq!(object["windows_scale"], 2.0);
        assert_eq!(
            object["arena_rect"],
            serde_json::json!([24.0, 124.0, 1416.0, 816.0])
        );
        assert!(object["hud_rows"].is_null());
        assert!(object["tab"].is_null());
        assert_eq!(object["overlays"], serde_json::json!([]));
    }
}
