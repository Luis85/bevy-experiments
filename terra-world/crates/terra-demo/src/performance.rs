//! Native diagnostics are opt-in presentation; they never grant world authority.
use crate::{DemoOptions, interface::UiState};
use bevy::{app::AppExit, ecs::system::SystemParam, prelude::*};
use std::time::Instant;
use terra_bevy::{
    LivingDiagnostics, NetworkStatus, SandboxRuntime, WorldDiagnostics, WorldRuntime, WorldStage,
};

#[derive(Resource)]
struct CaptureRuntime {
    started: Instant,
    session: String,
    last_attempt: Option<Instant>,
    writer: Option<crate::telemetry_capture::Recorder>,
    sequence: u64,
    pending_captures: usize,
}
impl Default for CaptureRuntime {
    fn default() -> Self {
        Self {
            started: Instant::now(),
            session: crate::telemetry_capture::session_id(),
            last_attempt: None,
            writer: None,
            sequence: 0,
            pending_captures: 0,
        }
    }
}

/// Observed publication, separate from enabled capture and world checkpoint state.
/// UI reads this resource; it cannot request a save or grant game authority.
#[derive(Resource, Default, Debug)]
pub(crate) struct CaptureStatus {
    pub last_success_unix_ms: Option<u128>,
    pub last_sequence: Option<u64>,
    pub last_error: Option<String>,
    pub pending: bool,
    pub skipped: u64,
}

impl CaptureStatus {
    fn published(&mut self, capture: crate::telemetry_capture::PublishedCapture) {
        self.last_success_unix_ms = Some(capture.captured_unix_ms);
        self.last_sequence = Some(capture.sequence);
        self.last_error = None;
    }

    pub(crate) fn describe(&self, options: &DemoOptions) -> String {
        let Some(path) = &options.telemetry else {
            return "Local diagnostics · Off. Enable playtest diagnostics in the main menu Settings before opening a world.".into();
        };
        let mut text = if let Some(error) = &self.last_error {
            format!(
                "Diagnostics could not be updated · {}\nAny earlier complete report is retained. Captures will retry; further evidence is unconfirmed until the error clears.",
                bounded_error(error)
            )
        } else if let Some(captured) = self.last_success_unix_ms {
            let age = crate::telemetry_capture::unix_millis().saturating_sub(captured) / 1000;
            let mut description = format!("Local diagnostics · last captured {age} s ago");
            if options.telemetry_interval_seconds.is_some_and(|interval| {
                age > u128::from(interval.saturating_mul(2).saturating_add(1))
            }) {
                description
                    .push_str("\nNew diagnostic evidence is delayed; current health is unknown.");
            }
            description
        } else if options.telemetry_interval_seconds.is_some() {
            "Local diagnostics · awaiting first successful capture".into()
        } else {
            "Local diagnostics · final capture requested on exit".into()
        };
        if self.pending {
            text.push_str("\nA diagnostic capture is queued or publishing.");
        }
        if self.skipped > 0 {
            text.push_str(&format!(
                "\n{} captures skipped while the writer was busy.",
                self.skipped
            ));
        }
        if let Some(interval) = options.telemetry_interval_seconds {
            text.push_str(&format!(
                "\nEvery {interval} real seconds · latest 16 captures"
            ));
        }
        text.push_str(&format!("\nReport · {}", path.display()));
        text
    }
}

fn bounded_error(error: &str) -> String {
    error
        .chars()
        .filter(|character| !character.is_control())
        .take(240)
        .collect()
}

#[derive(SystemParam)]
struct SessionDiagnosticInputs<'w> {
    virtual_time: Res<'w, Time<Virtual>>,
    flow: Option<Res<'w, crate::flow::GameFlow>>,
    queue: Option<Res<'w, terra_bevy::SandboxQueue>>,
    control: Option<Res<'w, crate::control::PlayerControl>>,
}

pub struct DemoTelemetryPlugin;
impl Plugin for DemoTelemetryPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CaptureRuntime>()
            .init_resource::<CaptureStatus>()
            .add_systems(
                Last,
                export_telemetry
                    .in_set(bevy::app::OnAppExitSystems)
                    .after(bevy::window::ExitSystems)
                    .after(terra_bevy::RegionPersistenceSet::FinalCheckpoint),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};
    use terra_bevy::{TerraWorldPlugin, WorldConfig};

    fn directory(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!("terra-telemetry-{label}-{}", std::process::id()))
    }

    fn closing_app(path: &Path, directory: &Path) -> App {
        test_app(path, directory, bevy::window::ExitCondition::OnAllClosed)
    }

    fn test_app(path: &Path, directory: &Path, exit_condition: bevy::window::ExitCondition) -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            // Exercise the real Bevy window-exit system without a GPU/window.
            .add_plugins(WindowPlugin {
                primary_window: None,
                exit_condition,
                ..default()
            })
            .insert_resource(DemoOptions {
                telemetry: Some(path.to_owned()),
                ..default()
            })
            .init_resource::<UiState>()
            .init_resource::<terra_bevy::render::RenderQualitySettings>()
            .init_resource::<terra_bevy::render::RenderCapabilities>()
            .init_resource::<terra_bevy::render::RenderTelemetry>()
            .add_plugins(TerraWorldPlugin::new(WorldConfig {
                chunk_radius: 0,
                terrain_resolution: 9,
                time_scale: 0.,
                live_weather: false,
                cache_dir: directory.join("cache"),
                save_path: directory.join("unused-save.json"),
                ..default()
            }))
            .add_plugins(DemoTelemetryPlugin);
        app
    }

    #[test]
    fn normal_window_exit_exports_in_the_same_final_update() {
        let directory = directory("normal-exit");
        let path = directory.join("report.json");
        let mut app = closing_app(&path, &directory);
        app.update();
        assert_eq!(app.should_exit(), Some(AppExit::Success));
        let report: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(report["release"], env!("CARGO_PKG_VERSION"));
        assert_eq!(report["network_authority"], false);
        assert_eq!(report["atmosphere_active"], false);
        assert!(report.get("resume_token").is_none());
        let status = app.world().resource::<CaptureStatus>();
        assert_eq!(status.last_sequence, Some(1));
        assert!(status.last_success_unix_ms.is_some());
        assert!(!status.pending);
        assert!(status.last_error.is_none());
        drop(app);
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn failed_requested_export_overrides_successful_window_exit() {
        let directory = directory("failed-exit");
        std::fs::create_dir_all(&directory).unwrap();
        // A directory cannot be overwritten as a JSON file.
        let mut app = closing_app(&directory, &directory);
        app.update();
        assert!(app.should_exit().unwrap().is_error());
        assert!(directory.is_dir());
        let status = app.world().resource::<CaptureStatus>();
        assert!(status.last_success_unix_ms.is_none());
        assert!(status.last_error.is_some());
        assert!(!status.pending);
        assert!(
            status
                .describe(app.world().resource::<DemoOptions>())
                .contains("could not be updated")
        );
        drop(app);
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn paused_running_app_retains_periodic_evidence_before_final_exit() {
        let directory = directory("paused-periodic");
        let path = directory.join("report.json");
        let mut app = test_app(&path, &directory, bevy::window::ExitCondition::DontExit);
        app.world_mut()
            .resource_mut::<DemoOptions>()
            .telemetry_interval_seconds = Some(1);
        app.world_mut().resource_mut::<Time<Virtual>>().pause();
        app.world_mut().init_resource::<NetworkStatus>();
        app.world_mut().resource_mut::<NetworkStatus>().resume_token =
            Some("PRIVATE_DIAGNOSTIC_TEST_CAPABILITY".into());
        app.world_mut().resource_mut::<UiState>().gps_text =
            "PRIVATE_DIAGNOSTIC_TEST_TYPED_TEXT".into();
        app.world_mut().resource_mut::<UiState>().location =
            "PRIVATE_DIAGNOSTIC_TEST_LOCATION".into();
        app.update();
        assert_eq!(app.should_exit(), None);
        let read_sequence = |sequence| {
            let deadline = Instant::now() + std::time::Duration::from_secs(3);
            loop {
                if let Ok(bytes) = std::fs::read(&path) {
                    let report: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                    if report["capture"]["sequence"] == sequence {
                        break report;
                    }
                }
                assert!(
                    Instant::now() < deadline,
                    "diagnostic worker did not publish"
                );
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        };
        let first = read_sequence(1);
        assert_eq!(first["capture"]["status"], "running");
        assert_eq!(first["capture"]["periodic_interval_seconds"], 1);
        assert_eq!(first["session_state"]["virtual_time_paused"], true);
        assert_eq!(first["session_state"]["world_clock_paused"], false);
        assert_eq!(first["session_state"]["world_time_scale"], 0.);
        // A virtual-clock pause must not gate wall-clock incident evidence.
        app.world_mut()
            .resource_mut::<CaptureRuntime>()
            .last_attempt = Some(Instant::now() - std::time::Duration::from_secs(2));
        app.update();
        let second = read_sequence(2);
        assert_eq!(second["history"]["snapshots"].as_array().unwrap().len(), 2);
        assert!(app.world().resource::<Time<Virtual>>().is_paused());
        app.world_mut().write_message(AppExit::Success);
        app.update();
        let final_report = read_sequence(3);
        assert_eq!(final_report["capture"]["status"], "graceful_exit");
        assert_eq!(app.should_exit(), Some(AppExit::Success));
        assert_eq!(
            app.world().resource::<CaptureStatus>().last_sequence,
            Some(3)
        );
        assert!(!app.world().resource::<CaptureStatus>().pending);
        for private in [
            "resume_token",
            "resume_capability",
            "inventory",
            "players",
            "PRIVATE_DIAGNOSTIC_TEST_CAPABILITY",
            "PRIVATE_DIAGNOSTIC_TEST_TYPED_TEXT",
            "PRIVATE_DIAGNOSTIC_TEST_LOCATION",
        ] {
            assert!(
                !serde_json::to_string(&final_report)
                    .unwrap()
                    .contains(private)
            );
        }
        drop(app);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn periodic_export_failure_is_visible_and_recovers_only_after_publication() {
        let directory = directory("visible-retry");
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("report.json");
        let lock = crate::telemetry_capture::lock_path(&path);
        let unrelated = b"unrelated file: preserve me";
        std::fs::write(&lock, unrelated).unwrap();
        let mut app = test_app(&path, &directory, bevy::window::ExitCondition::DontExit);
        app.world_mut()
            .resource_mut::<DemoOptions>()
            .telemetry_interval_seconds = Some(10);
        let deadline = Instant::now() + std::time::Duration::from_secs(3);
        loop {
            app.update();
            if app.world().resource::<CaptureStatus>().last_error.is_some() {
                break;
            }
            assert!(Instant::now() < deadline, "failed write was not observed");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(
            app.should_exit(),
            None,
            "periodic diagnostics do not stop gameplay"
        );
        assert_eq!(std::fs::read(&lock).unwrap(), unrelated);
        assert!(!path.exists());
        let status = app.world().resource::<CaptureStatus>();
        assert!(status.last_success_unix_ms.is_none());
        assert!(!status.pending);
        assert!(
            status
                .describe(app.world().resource::<DemoOptions>())
                .contains("could not be updated")
        );
        std::fs::remove_file(&lock).unwrap();
        app.world_mut()
            .resource_mut::<CaptureRuntime>()
            .last_attempt = None;
        let deadline = Instant::now() + std::time::Duration::from_secs(3);
        loop {
            app.update();
            if app.world().resource::<CaptureStatus>().last_sequence == Some(2) {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "successful publication was not observed"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let status = app.world().resource::<CaptureStatus>();
        assert!(status.last_success_unix_ms.is_some());
        assert!(status.last_error.is_none());
        assert!(!status.pending);
        let report: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(report["capture"]["prior_write_failures"], 1);
        assert_eq!(report["history"]["snapshots"].as_array().unwrap().len(), 1);
        app.world_mut().write_message(AppExit::Success);
        app.update();
        drop(app);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn disabled_diagnostics_create_no_report_or_lock() {
        let directory = directory("disabled");
        let path = crate::telemetry_capture::menu_report_path(&directory);
        let mut app = test_app(&path, &directory, bevy::window::ExitCondition::DontExit);
        app.world_mut().resource_mut::<DemoOptions>().telemetry = None;
        app.update();
        app.world_mut().write_message(AppExit::Success);
        app.update();
        assert!(!path.exists());
        assert!(!crate::telemetry_capture::lock_path(&path).exists());
        assert!(
            app.world()
                .resource::<CaptureStatus>()
                .last_success_unix_ms
                .is_none()
        );
        drop(app);
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn stopped_diagnostic_worker_clears_pending_without_replacing_prior_evidence() {
        let directory = directory("stopped-worker");
        let path = directory.join("report.json");
        let writer =
            crate::telemetry_capture::Recorder::start(path.clone(), WorldConfig::default(), None)
                .unwrap();
        writer.finish(serde_json::json!({
            "release":env!("CARGO_PKG_VERSION"),"timings":{},
            "capture":{"session":"completed-worker","sequence":1,"status":"graceful_exit","captured_unix_ms":1}
        })).unwrap();
        let before = std::fs::read(&path).unwrap();
        let mut app = test_app(&path, &directory, bevy::window::ExitCondition::DontExit);
        app.world_mut()
            .resource_mut::<DemoOptions>()
            .telemetry_interval_seconds = Some(10);
        {
            let mut capture = app.world_mut().resource_mut::<CaptureRuntime>();
            capture.writer = Some(writer);
            capture.pending_captures = 1;
        }
        app.world_mut().resource_mut::<CaptureStatus>().pending = true;
        app.update();
        let status = app.world().resource::<CaptureStatus>();
        assert!(!status.pending);
        assert!(
            status
                .last_error
                .as_ref()
                .unwrap()
                .contains("writer stopped")
        );
        assert!(status.last_success_unix_ms.is_none());
        assert_eq!(app.world().resource::<CaptureRuntime>().pending_captures, 0);
        assert_eq!(app.should_exit(), None);
        assert_eq!(std::fs::read(&path).unwrap(), before);
        drop(app);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn failed_final_checkpoint_is_recorded_as_an_error_exit() {
        let directory = directory("checkpoint-error");
        let path = directory.join("report.json");
        let mut app = test_app(&path, &directory, bevy::window::ExitCondition::DontExit);
        app.world_mut()
            .resource_mut::<WorldRuntime>()
            .config
            .region_store = Some(directory.join("world.db"));
        app.update();
        assert!(
            app.world()
                .resource::<terra_bevy::RegionalPersistence>()
                .enabled
        );
        // An invalid game-owned mutation must fail the real final checkpoint;
        // its error message must precede the diagnostic capture in Last.
        app.world_mut()
            .resource_mut::<SandboxRuntime>()
            .state
            .players
            .get_mut(&1)
            .unwrap()
            .inventory
            .insert("not_in_catalog".into(), 1);
        app.world_mut().write_message(AppExit::Success);
        app.update();
        assert!(app.should_exit().unwrap().is_error());
        let report: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(report["capture"]["status"], "exit_error");
        assert_eq!(report["regional_persistence"]["blocked"], true);
        assert!(report["regional_persistence"]["last_error"].is_string());
        drop(app);
        std::fs::remove_dir_all(directory).unwrap();
    }
}

pub fn describe(
    timings: &WorldDiagnostics,
    runtime: &WorldRuntime,
    living: Option<&LivingDiagnostics>,
    render: &terra_bevy::render::RenderTelemetry,
    network: Option<&NetworkStatus>,
    quality: &terra_bevy::render::RenderQualitySettings,
    persistence: &terra_bevy::RegionalPersistence,
    asset_pack: Option<&terra_bevy::render::AssetPackDiagnostics>,
) -> String {
    let frame = timings.frame.summary();
    let mut lines = vec![format!(
        "{} graphics\nFrame interval · {} samples\nLast {:.1} ms · p50 {:.1} · p95 {:.1}\nCPU stages below; GPU time is not measured.",
        quality.quality.label(),
        frame.window_samples,
        frame.last_ms,
        frame.p50_ms,
        frame.p95_ms
    )];
    if let Some(pack) = asset_pack {
        // The full immutable identity remains in the telemetry export. Keep
        // its UI preview short enough to wrap inside enlarged-text panels.
        let identity = if pack.identity.len() > 32 && pack.identity.is_ascii() {
            format!(
                "{}…{}",
                &pack.identity[..19],
                &pack.identity[pack.identity.len() - 8..]
            )
        } else {
            pack.identity.clone()
        };
        let mut text = format!(
            "Presentation pack · {}\n{} models · {} ready / {} loading\n{} instances using procedural fallback\n{}",
            pack.name,
            pack.model_count,
            pack.ready_instances,
            pack.loading_instances,
            pack.fallback_instances,
            identity
        );
        let budget = &pack.scene_budget;
        text.push_str(&format!(
            "\nImported scene budget · {} selected / {} instance slots\n{} eligible deferred · {} distant · {} hidden\n{} vertices · {} primitives · {} animation nodes\n{} retained models / {} retiring / {} loading\nEstimated payload {:.1} / {:.0} MiB · peak {:.1} MiB\nCPU/GPU allocation and GPU time are not measured.",
            budget.selected.instances, budget.limits.instances,
            budget.deferred_instances, budget.distant_instances, budget.hidden_instances,
            budget.reserved.vertices, budget.reserved.primitives, budget.reserved.animation_nodes,
            budget.reserved.resident_models, budget.retiring_models, budget.loading_models,
            budget.reserved.estimated_bytes as f64 / 1_048_576., budget.limits.estimated_bytes as f64 / 1_048_576.,
            budget.peak_reserved_estimated_bytes as f64 / 1_048_576.,
        ));
        if budget.immutable_pack_estimated_bytes > budget.limits.estimated_bytes {
            text.push_str("\nInstalled source snapshots exceed this byte allowance. Increase the limit or restart with a smaller pack.");
        } else if budget.draining {
            text.push_str(
                "\nWaiting for outgoing assets to release before admitting replacements.",
            );
        }
        if budget.unmetered_instances > 0 {
            text.push_str(&format!(
                "\n{} direct scene overrides have game-owned geometry/memory limits.",
                budget.unmetered_instances
            ));
        }
        if let Some(error) = &pack.last_error {
            text.push_str(&format!("\n{error}\nProcedural actors remain visible. Repair the pack and restart, or use --procedural-models."));
        }
        lines.push(text);
    } else {
        lines.push("Presentation · procedural models".into());
    }
    for (stage, label) in [
        (WorldStage::Streaming, "Stream integration"),
        (WorldStage::BaseWorld, "World rules"),
        (WorldStage::Ecology, "Wildlife decisions"),
        (WorldStage::Living, "Bodies / production"),
        (WorldStage::Economy, "Economy / contracts"),
        (WorldStage::ControlledMotion, "Controlled movement"),
        (WorldStage::Persistence, "Storage scheduling / polling"),
    ] {
        let s = timings.stage(stage);
        if s.window_samples > 0 {
            lines.push(format!(
                "{label}\n  {:.2} ms last · {:.2} ms p95",
                s.last_ms, s.p95_ms
            ));
        }
    }
    lines.push(format!("Terrain · {} rendered / {} resident\n{} pending builds · {:.2}M source vertices\nTerrain vertices {} / {} full detail\nNear / middle / far: {} / {} / {}",
        render.chunks, runtime.stream_stats.resident, render.pending_meshes + render.building_meshes,
        render.vertices as f64 / 1_000_000., render.terrain_vertices, render.terrain_full_vertices,
        render.lod_chunks[0], render.lod_chunks[1], render.lod_chunks[2]));
    if render.mesh_failures > 0 {
        lines.push(format!(
            "Mesh recovery · {} retrying · {} failures observed\nLast failure: {}",
            render.retrying_meshes,
            render.mesh_failures,
            render.last_mesh_error.as_deref().unwrap_or("unavailable")
        ));
    }
    if persistence.enabled && !runtime.network_authority {
        let mut detail = format!("Regional persistence\n{}", persistence.status_line());
        if let Some(s) = &persistence.status
            && s.generation > 0
        {
            detail.push_str(&format!(
                "\n{} active / {} dormant records\n{} global records · {} pinned / {} dependency regions\nWorker commit {:.1} ms · {:.0} s ago\n{} · {:.1} KiB checkpoint data\nDormant records freeze; global economy stays resident.",
                s.active_records, s.dormant_records, s.global_records,
                s.pinned_regions, s.dependency_regions,
                persistence.last_checkpoint_ms, persistence.seconds_since_checkpoint,
                persistence.last_checkpoint_reason,
                s.checkpoint_bytes as f64 / 1024.,
            ));
            detail.push_str(&format!(
                "\nMain-thread submission {:.2} ms\n{}",
                persistence.last_submission_ms,
                if persistence.pending() {
                    "Storage job active · authority waiting"
                } else {
                    "Storage worker idle"
                }
            ));
        }
        let work = &persistence.last_checkpoint_metrics;
        detail.push_str(&format!("\n{} cold environment records certified\n{} pages encoded · {} unchanged pages reused\nCompose {:.1} · select {:.1} · encode {:.1} ms\nTransaction {:.1} · install {:.1} ms",
            work.validation.certified_environment_records, work.encoded_pages, work.reused_pages,
            work.compose_ms, work.select_ms, work.encode_ms, work.transaction_ms, work.install_ms));
        if let Some(error) = &persistence.last_error {
            detail.push_str(&format!("\nError: {error}"));
        }
        lines.push(detail);
    }
    if runtime.network_authority {
        lines.push(
            "Simulation timings belong to the host. Client counts describe the received view."
                .into(),
        );
    } else if let Some(d) = living {
        let w = &d.ecology_work;
        lines.push(format!("Wildlife AI · last behavior pass\n{} evaluated · {} deferred\n{} near / {} far decisions\n{} threat candidates checked", w.evaluated, w.deferred, w.near_decisions, w.far_decisions, w.threat_candidates));
        if let Some(error) = &d.last_error {
            lines.push(format!("Simulation error: {error}"));
        }
    }
    if let Some(net) = network {
        let r = &net.replication;
        let change = r.saved_fraction() * 100.;
        let comparison = if change < 0. {
            format!("Payload overhead vs full views: {:.1}%", -change)
        } else {
            format!("Payload saving vs full views: {change:.1}%")
        };
        let connection = if !net.connected {
            "Connection inactive; counters retain the last received evidence"
        } else if net.resync_pending {
            "Recovering an authoritative baseline…"
        } else if net.actor_id.is_none() || r.keyframes < 2 {
            "Waiting for initial authoritative views…"
        } else {
            "Baseline synchronized"
        };
        lines.push(format!("Network · protocol {}\nReceived {:.1} KiB · sent {:.1} KiB\n{} keyframes · {} deltas\n{}\n{} removals · {} resync requests\n{}", terra_bevy::NETWORK_PROTOCOL_VERSION,
            net.transport.bytes_received as f64 / 1024., net.transport.bytes_sent as f64 / 1024.,
            r.keyframes, r.deltas, comparison, r.entity_removals, r.resync_requests,
            connection));
        lines.push(format!("Owner display prediction\n{} reconciliations · {} queued inputs\nDisplay / authority gap {:.2} m\n{} horizon resets · 250 ms maximum look-ahead\nDisplay only; gameplay remains on the host", net.prediction.stats.reconciliations,
            net.prediction.pending_len(), net.prediction.stats.last_authority_gap_m,
            net.prediction.stats.horizon_resets));
    }
    lines.join("\n\n")
}

fn export_telemetry(
    mut exits: ParamSet<(MessageReader<AppExit>, MessageWriter<AppExit>)>,
    options: Res<DemoOptions>,
    timings: Res<WorldDiagnostics>,
    runtime: Res<WorldRuntime>,
    sandbox: Res<SandboxRuntime>,
    living: Option<Res<LivingDiagnostics>>,
    network: Option<Res<NetworkStatus>>,
    quality: Res<terra_bevy::render::RenderQualitySettings>,
    capabilities: Res<terra_bevy::render::RenderCapabilities>,
    render: Res<terra_bevy::render::RenderTelemetry>,
    ui: Res<UiState>,
    persistence: Res<terra_bevy::RegionalPersistence>,
    asset_pack: Option<Res<terra_bevy::render::AssetPackDiagnostics>>,
    mut capture: ResMut<CaptureRuntime>,
    mut status: ResMut<CaptureStatus>,
    session: SessionDiagnosticInputs,
) {
    let exit_error = exits
        .p0()
        .read()
        .map(AppExit::is_error)
        .reduce(|left, right| left || right);
    let Some(path) = &options.telemetry else {
        return;
    };
    if let Some(writer) = &capture.writer {
        for result in writer.poll() {
            capture.pending_captures = capture.pending_captures.saturating_sub(1);
            match result {
                Ok(published) => status.published(published),
                Err(error) => {
                    if status.last_error.as_ref() != Some(&error) {
                        eprintln!(
                            "Periodic diagnostics could not be saved: {error}. Any earlier complete report is retained; capture will retry."
                        );
                    }
                    status.last_error = Some(error);
                }
            }
        }
    }
    status.pending = capture.pending_captures > 0;
    let now = Instant::now();
    let periodic_due = options.telemetry_interval_seconds.is_some_and(|seconds| {
        capture
            .last_attempt
            .is_none_or(|previous| now.duration_since(previous).as_secs() >= seconds)
    });
    if exit_error.is_none() && !periodic_due {
        return;
    }
    capture.last_attempt = Some(now);
    capture.sequence = capture.sequence.saturating_add(1);
    let report = serde_json::json!({
        "release":env!("CARGO_PKG_VERSION"), "timings":timings.report(),
        "capture":{
            "session":capture.session, "sequence":capture.sequence,
            "captured_unix_ms":crate::telemetry_capture::unix_millis(),
            "elapsed_seconds":now.duration_since(capture.started).as_secs_f64(),
            "status":match exit_error { Some(true)=>"exit_error", Some(false)=>"graceful_exit", None=>"running" },
            "periodic_interval_seconds":options.telemetry_interval_seconds,
            "skipped_busy_captures":status.skipped,
            "last_export_error":status.last_error,
        },
        "quality":*quality, "network_authority":runtime.network_authority,
        "atmosphere_active":quality.uses_atmosphere() && capabilities.atmospheric_sky,
        "source":{"revision":runtime.config.base_data_revision,"epoch":runtime.config.data_epoch_unix,"heightmap":runtime.config.heightmap_manifest.is_some(),"political_revision":runtime.config.political_dataset_revision},
        "session_state":{
            "phase":session.flow.as_ref().map(|flow|flow.diagnostic_phase()),
            "input_blocked":session.flow.as_ref().map(|flow|flow.blocks_input()),
            "managed_world":options.world_profile_path.is_some(),
            "virtual_time_paused":session.virtual_time.is_paused(),
            "world_clock_paused":runtime.clock.paused,
            "world_time_scale":runtime.clock.time_scale,
            "unsaved_changes":sandbox.dirty,
            "checkpoint_queued":session.queue.as_ref().is_some_and(|queue|queue.0.iter().any(|intent|matches!(intent,terra_bevy::SandboxIntent::Save))),
            "restore_queued":session.queue.as_ref().is_some_and(|queue|queue.0.iter().any(|intent|matches!(intent,terra_bevy::SandboxIntent::Load | terra_bevy::SandboxIntent::LoadReviewed(_)))),
            "network_connected":network.as_ref().map(|network|network.connected),
        },
        "render":{"chunks":render.chunks,"source_vertices":render.vertices,"terrain_vertices":render.terrain_vertices,"terrain_full_vertices":render.terrain_full_vertices,"lod_chunks":render.lod_chunks,"pending":render.pending_meshes+render.building_meshes,"retrying":render.retrying_meshes,"failures":render.mesh_failures,"last_error":render.last_mesh_error.as_deref()},
        "population":{"bodies":sandbox.state.living.units.len(),"wildlife":sandbox.state.ecology.animals.len(),"counts_are_received_view":runtime.network_authority,"counts_are_active_working_set":persistence.enabled && !runtime.network_authority},
        "ecology_work":living.as_ref().map(|d| &d.ecology_work),
        "replication":network.as_ref().map(|n| &n.replication),
        "transport":network.as_ref().map(|n| &n.transport),
        "owner_display_prediction":network.as_ref().map(|n| &n.prediction.stats),
        "last_simulation_error":living.as_ref().and_then(|d|d.last_error.as_deref()),
        "last_world_error":runtime.last_error,
        "presentation":{
            "inspector_open":ui.context_panel,"immersive_controls":ui.immersive_controls,
            "world_panel_open":ui.world_panel,"performance_panel_open":ui.performance_panel,
            "text_edit_active":ui.editing_gps,
            "control_mode":session.control.as_ref().map(|control|match control.mode {
                crate::control::ControlMode::Rts=>"rts",
                crate::control::ControlMode::ThirdPerson=>"third_person",
                crate::control::ControlMode::FirstPerson=>"first_person",
            }),
        },
        "asset_pack":asset_pack.as_ref().map(|pack|serde_json::json!({
            "name":pack.name,"identity":pack.identity,"model_count":pack.model_count,
            "ready_instances":pack.ready_instances,"loading_instances":pack.loading_instances,
            "rejected_instances":pack.rejected_instances,"fallback_instances":pack.fallback_instances,
            "failed_models":pack.failed_models,"last_error":pack.last_error,
            "scene_budget":pack.scene_budget,
        })),
        "regional_persistence":{
            "enabled":persistence.enabled,
            "restored":persistence.restored,
            "blocked":persistence.blocked(),
            "pending":persistence.pending(),
            "authority_paused":persistence.authority_paused(),
            "last_submission_ms":persistence.last_submission_ms,
            "committed":persistence.status,
            "last_checkpoint_ms":persistence.last_checkpoint_ms,
            "checkpoint_stages":persistence.last_checkpoint_metrics,
            "seconds_since_checkpoint":persistence.seconds_since_checkpoint,
            "last_checkpoint_reason":persistence.last_checkpoint_reason,
            "last_error":persistence.last_error,
        },
        "measurement_scope":"Rolling frame intervals and local CPU stage timings; no GPU timestamps or server-load claim. No session capabilities are exported."
    });
    let result = (|| -> Result<bool, String> {
        // Periodic filesystem validation and writes belong to the worker.
        // Final exit also consults the active store at its maintenance barrier.
        if exit_error.is_some() {
            persistence.check_output_path(path)?;
        }
        if capture.writer.is_none() {
            capture.writer = Some(crate::telemetry_capture::Recorder::start(
                path.clone(),
                runtime.config.clone(),
                options.world_profile_path.clone(),
            )?);
        }
        let writer = capture
            .writer
            .as_ref()
            .expect("diagnostic writer initialized");
        if exit_error.is_some() {
            let published = writer.finish(report)?;
            capture.pending_captures = 0;
            status.pending = false;
            status.published(published);
            Ok(true)
        } else {
            writer.try_capture(report)
        }
    })();
    match result {
        Ok(true) if exit_error.is_some() => println!("TERRA_TELEMETRY {}", path.display()),
        Ok(true) => {
            capture.pending_captures = capture.pending_captures.saturating_add(1);
            status.pending = true;
        }
        Ok(false) => status.skipped = status.skipped.saturating_add(1),
        Err(error) => {
            // A failed queue submission means there is no live writer capable
            // of completing the previously accepted jobs. Do not leave the UI
            // claiming that a dead worker is still publishing a capture.
            capture.pending_captures = 0;
            status.pending = false;
            if status.last_error.as_ref() != Some(&error) {
                eprintln!("Could not export telemetry to {}: {error}", path.display());
            }
            status.last_error = Some(error.clone());
            if exit_error.is_some() {
                let notice = format!(
                    "The diagnostic report could not be updated: {}. World checkpoint success is separate; check any storage error before reopening. An earlier report does not describe this exit.",
                    bounded_error(&error)
                );
                if let Ok(payload) = serde_json::to_string(&notice) {
                    eprintln!(
                        "{}{}",
                        crate::telemetry_capture::FINAL_ERROR_PREFIX,
                        payload
                    );
                }
                exits.p1().write(AppExit::error());
            }
        }
    }
}
