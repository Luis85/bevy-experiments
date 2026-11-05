//! Opt-in, bounded native frame evidence (`LASTLIGHT_PERF_REPORT`), report
//! version 3. `capture_performance` runs at the end of `Last`, after
//! `PerfSet::CloseFrame`, so every phase of frame i lands in sample i. Every v2
//! key keeps its name and meaning; v3 adds `phases`, the interface breakdown,
//! egui and scene counts, the present and build context and GPU time.
use super::{
    FrameSample, PerfFrame, PerfProbe,
    gpu_span::{EGUI_GPU_PATH, gpu_timing_support},
};
use bevy::{
    app::AppExit,
    diagnostic::{DiagnosticPath, DiagnosticsStore},
    ecs::system::SystemParam,
    platform::time::Instant,
    prelude::*,
    render::{
        renderer::{RenderAdapterInfo, RenderDevice},
        settings::Backends,
    },
};
use serde::Serialize;
use std::{collections::BTreeMap, path::PathBuf};

const REPORT_VERSION: u32 = 3;
const EGUI_GPU: DiagnosticPath = DiagnosticPath::const_new(EGUI_GPU_PATH);

#[derive(Default)]
pub struct FrameCapture {
    initialized: bool,
    path: Option<PathBuf>,
    frames: u32,
    samples: Vec<RecordedFrame>,
    unfocused_samples: usize,
    warmup: u32,
    target_samples: u32,
    active_only: bool,
    initial_battle: Option<BattleContext>,
    peak_enemies: usize,
    peak_corpses: usize,
    last_gpu_measurement: Option<Instant>,
    done: bool,
}
#[derive(Clone, Debug, Serialize)]
pub(super) struct BattleContext {
    mode: horde_core::content::Mode,
    simulation_tick: u64,
    lane_count: u8,
    width_yards: f32,
    enemies: usize,
    corpses: usize,
}
/// One sampled native frame: its interval, the probe's frame and the scene.
#[derive(Clone, Debug, Default)]
pub(super) struct RecordedFrame {
    pub frame_ms: f64,
    pub probe: FrameSample,
    pub visible_enemies: usize,
    pub corpses: usize,
    /// A GPU egui-pass measurement that arrived this frame (it lags the
    /// frame it timed by the readback latency).
    pub gpu_egui_ms: Option<f64>,
}
/// The final-frame context of the v2 report.
#[derive(Clone, Debug, Default)]
pub(super) struct EndState {
    pub visible_enemies: usize,
    pub lane_count: Option<u8>,
    pub width_yards: Option<f32>,
    pub population_cap: Option<usize>,
    pub threat_rank: Option<u32>,
    pub visible_structures: usize,
    pub parallel_battles: usize,
    pub simulation_ticks: u64,
    pub paused: bool,
    pub window_focused: bool,
}
#[derive(Clone, Debug, Default, Serialize)]
pub(super) struct Present {
    pub requested_mode: String,
    /// `settings` or `launch-override`.
    pub source: &'static str,
    pub pipelined_rendering: bool,
    pub desired_maximum_frame_latency: Option<u32>,
    pub low_latency: bool,
}
/// Everything `build_report` reads; the system gathers it, tests build it.
#[derive(Default)]
pub(super) struct ReportInput {
    pub scene: String,
    pub cpu: String,
    pub adapter: String,
    pub backend: String,
    pub width_physical: u32,
    pub height_physical: u32,
    pub windows_scale: f64,
    pub ui_scale: f32,
    pub warmup_frames: u32,
    pub requested_sample_frames: u32,
    pub frames: Vec<RecordedFrame>,
    pub unfocused_sample_frames: usize,
    pub initial_battle: Option<BattleContext>,
    pub peak_visible_enemies: usize,
    pub peak_corpses: usize,
    pub persistence: crate::runner::PersistenceMetrics,
    pub end: EndState,
    pub present: Present,
    /// `LASTLIGHT_PERF_PROFILE`.
    pub profile_label: Option<String>,
    /// `None` when no render device or adapter was reported.
    pub gpu_support: Option<Result<(), &'static str>>,
    /// Mean GPU ms of every timed render pass, by span path.
    pub gpu_passes: BTreeMap<String, f64>,
    pub pipelined: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
pub(super) struct Stats {
    mean: f64,
    p50: f64,
    p95: f64,
    p99: f64,
    max: f64,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
pub(super) struct CountStats {
    mean: f64,
    max: u64,
}
/// The frame spans, per sample (`f64`) or aggregated (`Stats`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
pub(super) struct Phases<T> {
    main_cpu_ms: T,
    egui_pass_ms: T,
    egui_tessellate_ms: T,
    interface_total_ms: T,
    extract_ms: T,
    render_schedule_ms: T,
    present_wait_ms: T,
    render_world_cpu_ms: T,
    cpu_frame_ms: T,
    critical_path_ms: T,
}
#[derive(Serialize)]
struct InterfaceBreakdown {
    battle_frame_build: f64,
    battlefield_paint: f64,
    backdrop: f64,
}
#[derive(Serialize)]
struct EguiReport {
    shapes: CountStats,
    clipped_primitives: CountStats,
    mesh_vertices: CountStats,
    mesh_indices: CountStats,
    textured_quads: CountStats,
    passes_per_frame: CountStats,
}
#[derive(Serialize)]
struct SceneLoad {
    visible_enemies: CountStats,
    corpses: CountStats,
    settled_corpses_drawn: CountStats,
    collapsing_corpses_drawn: CountStats,
    particles: CountStats,
    fx_views: CountStats,
}
#[derive(Serialize)]
struct BuildInfo {
    profile_label: String,
    features: Vec<&'static str>,
    debug_assertions: bool,
}
#[derive(Serialize)]
struct GpuReport {
    supported: bool,
    reason: Option<String>,
    egui_pass_ms: Option<Stats>,
    passes: BTreeMap<String, f64>,
}

#[derive(Serialize)]
pub(super) struct FrameReport {
    version: u32,
    scene: String,
    scope: &'static str,
    cpu: String,
    adapter: String,
    backend: String,
    width_physical: u32,
    height_physical: u32,
    windows_scale: f64,
    ui_scale: f32,
    warmup_frames: u32,
    sample_frames: usize,
    requested_sample_frames: u32,
    completed_sample_target: bool,
    initial_battle: Option<BattleContext>,
    peak_visible_enemies: usize,
    peak_corpses: usize,
    persistence: crate::runner::PersistenceMetrics,
    mean_frame_ms: f64,
    median_frame_ms: f64,
    p95_frame_ms: f64,
    p99_frame_ms: f64,
    mean_observed_fps: f64,
    visible_enemies: usize,
    lane_count: Option<u8>,
    width_yards: Option<f32>,
    population_cap: Option<usize>,
    threat_rank: Option<u32>,
    visible_structures: usize,
    parallel_battles: usize,
    simulation_ticks: u64,
    paused: bool,
    window_focused: bool,
    unfocused_sample_frames: usize,
    frames_over_33ms: usize,
    frames_over_50ms: usize,
    frames_over_100ms: usize,
    frames_over_250ms: usize,
    work_scope: &'static str,
    work_mean_ms: [f64; 3],
    work_max_ms: [f64; 3],
    longest_frames: Vec<SlowFrame>,
    phase_scope: &'static str,
    phases: Phases<Stats>,
    /// Distinct completed render-world samples behind the sampled frames.
    render_samples: usize,
    /// Sampled frames before any render-world sample completed.
    frames_without_render_sample: usize,
    interface_breakdown_mean_ms: InterfaceBreakdown,
    egui: EguiReport,
    scene_load: SceneLoad,
    present: Present,
    build: BuildInfo,
    gpu: GpuReport,
}
#[derive(Serialize)]
struct SlowFrame {
    sample_index: usize,
    frame_ms: f64,
    work_ms: [f64; 3],
    phases: Phases<f64>,
}

fn quantile(sorted: &[f64], percent: f64) -> f64 {
    if sorted.is_empty() {
        return 0.;
    }
    sorted[((sorted.len() - 1) as f64 * percent).round() as usize]
}
fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        0.
    } else {
        values.iter().sum::<f64>() / values.len() as f64
    }
}
/// Mean, median, p95, p99 and max of per-sample values.
pub(super) fn stats(values: &[f64]) -> Stats {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    Stats {
        mean: mean(values),
        p50: quantile(&sorted, 0.5),
        p95: quantile(&sorted, 0.95),
        p99: quantile(&sorted, 0.99),
        max: sorted.last().copied().unwrap_or(0.),
    }
}
fn count_stats(values: impl Iterator<Item = u64> + Clone) -> CountStats {
    let samples: Vec<f64> = values.clone().map(|v| v as f64).collect();
    CountStats {
        mean: mean(&samples),
        max: values.max().unwrap_or(0),
    }
}
/// CPU frame time per sample: main(i) + render world(latest completed render
/// sample). Quantiles are taken over these sums, never summed from the parts.
pub(super) fn cpu_frames(main: &[f64], render_world: &[f64]) -> Vec<f64> {
    main.iter()
        .zip(render_world)
        .map(|(main, render)| main + render)
        .collect()
}
fn frame_phases(sample: &FrameSample, pipelined: bool) -> Phases<f64> {
    let render = sample.render.unwrap_or_default();
    let render_world = render.cpu_ms();
    Phases {
        main_cpu_ms: sample.main_cpu_ms,
        egui_pass_ms: sample.egui_pass_ms,
        egui_tessellate_ms: sample.egui_tessellate_ms,
        interface_total_ms: sample.egui_pass_ms + sample.egui_tessellate_ms,
        extract_ms: render.extract_ms,
        render_schedule_ms: render.render_schedule_ms,
        present_wait_ms: render.present_wait_ms,
        render_world_cpu_ms: render_world,
        cpu_frame_ms: sample.main_cpu_ms + render_world,
        critical_path_ms: if pipelined {
            sample.main_cpu_ms.max(render_world)
        } else {
            sample.main_cpu_ms + render_world
        },
    }
}
fn phase_stats(frames: &[Phases<f64>]) -> Phases<Stats> {
    let column = |field: fn(&Phases<f64>) -> f64| frames.iter().map(field).collect::<Vec<_>>();
    let main = column(|p| p.main_cpu_ms);
    let render_world = column(|p| p.render_world_cpu_ms);
    Phases {
        main_cpu_ms: stats(&main),
        egui_pass_ms: stats(&column(|p| p.egui_pass_ms)),
        egui_tessellate_ms: stats(&column(|p| p.egui_tessellate_ms)),
        interface_total_ms: stats(&column(|p| p.interface_total_ms)),
        extract_ms: stats(&column(|p| p.extract_ms)),
        render_schedule_ms: stats(&column(|p| p.render_schedule_ms)),
        present_wait_ms: stats(&column(|p| p.present_wait_ms)),
        render_world_cpu_ms: stats(&render_world),
        cpu_frame_ms: stats(&cpu_frames(&main, &render_world)),
        critical_path_ms: stats(&column(|p| p.critical_path_ms)),
    }
}
fn build_info(profile_label: Option<&str>) -> BuildInfo {
    BuildInfo {
        profile_label: profile_label.unwrap_or("unlabelled").to_owned(),
        features: [
            ("review", cfg!(feature = "review")),
            ("dev", cfg!(feature = "dev")),
        ]
        .into_iter()
        .filter_map(|(name, on)| on.then_some(name))
        .collect(),
        debug_assertions: cfg!(debug_assertions),
    }
}
fn gpu_report(input: &ReportInput) -> GpuReport {
    let unsupported = |reason: &str| GpuReport {
        supported: false,
        reason: Some(reason.to_owned()),
        egui_pass_ms: None,
        passes: input.gpu_passes.clone(),
    };
    match input.gpu_support {
        None => unsupported("no render device reported"),
        Some(Err(reason)) => unsupported(reason),
        Some(Ok(())) => {
            let samples: Vec<_> = input
                .frames
                .iter()
                .filter_map(|f| f.gpu_egui_ms.map(|gpu| (gpu, f.frame_ms)))
                .collect();
            if samples.iter().any(|&(gpu, frame)| gpu < 0. || gpu > frame) {
                return unsupported("unsupported: inconsistent timestamps");
            }
            let values: Vec<f64> = samples.iter().map(|&(gpu, _)| gpu).collect();
            GpuReport {
                supported: true,
                reason: values
                    .is_empty()
                    .then(|| "no egui GPU span measured".to_owned()),
                egui_pass_ms: (!values.is_empty()).then(|| stats(&values)),
                passes: input.gpu_passes.clone(),
            }
        }
    }
}

/// Pure assembly of the report from what the capture system gathered.
pub(super) fn build_report(input: &ReportInput) -> FrameReport {
    let frames = &input.frames;
    let frame_ms: Vec<f64> = frames.iter().map(|f| f.frame_ms).collect();
    let mut sorted = frame_ms.clone();
    sorted.sort_by(f64::total_cmp);
    let mean_frame_ms = mean(&frame_ms);
    let phases: Vec<Phases<f64>> = frames
        .iter()
        .map(|f| frame_phases(&f.probe, input.pipelined))
        .collect();
    let mut longest: Vec<_> = frame_ms.iter().copied().enumerate().collect();
    longest.sort_by(|a, b| b.1.total_cmp(&a.1));
    longest.truncate(20);
    let work_mean_ms = std::array::from_fn(|i| {
        mean(
            &frames
                .iter()
                .map(|f| f.probe.work_ms[i])
                .collect::<Vec<_>>(),
        )
    });
    let work_max_ms =
        std::array::from_fn(|i| frames.iter().map(|f| f.probe.work_ms[i]).fold(0., f64::max));
    let breakdown = |i: usize| {
        mean(
            &frames
                .iter()
                .map(|f| f.probe.interface_ms[i])
                .collect::<Vec<_>>(),
        )
    };
    let egui = |i: usize| count_stats(frames.iter().map(move |f| f.probe.egui[i]));
    let scene = |field: fn(&RecordedFrame) -> usize| {
        count_stats(frames.iter().map(move |f| field(f) as u64))
    };
    let mut render_seqs: Vec<u64> = frames
        .iter()
        .filter(|f| f.probe.render.is_some())
        .map(|f| f.probe.render_seq)
        .collect();
    render_seqs.dedup();
    let over = |limit: f64| frame_ms.iter().filter(|&&ms| ms > limit).count();
    let end = &input.end;
    FrameReport {
        version: REPORT_VERSION,
        scene: input.scene.clone(),
        scope: "Observed native application frame intervals including VSync and OS scheduling; not GPU timestamps or a minimum-spec guarantee",
        cpu: input.cpu.clone(),
        adapter: input.adapter.clone(),
        backend: input.backend.clone(),
        width_physical: input.width_physical,
        height_physical: input.height_physical,
        windows_scale: input.windows_scale,
        ui_scale: input.ui_scale,
        warmup_frames: input.warmup_frames,
        sample_frames: frames.len(),
        requested_sample_frames: input.requested_sample_frames,
        completed_sample_target: frames.len() >= input.requested_sample_frames as usize,
        initial_battle: input.initial_battle.clone(),
        peak_visible_enemies: input.peak_visible_enemies,
        peak_corpses: input.peak_corpses,
        persistence: input.persistence,
        mean_frame_ms,
        median_frame_ms: quantile(&sorted, 0.5),
        p95_frame_ms: quantile(&sorted, 0.95),
        p99_frame_ms: quantile(&sorted, 0.99),
        mean_observed_fps: 1000. / mean_frame_ms,
        visible_enemies: end.visible_enemies,
        lane_count: end.lane_count,
        width_yards: end.width_yards,
        population_cap: end.population_cap,
        threat_rank: end.threat_rank,
        visible_structures: end.visible_structures,
        parallel_battles: end.parallel_battles,
        simulation_ticks: end.simulation_ticks,
        paused: end.paused,
        window_focused: end.window_focused,
        unfocused_sample_frames: input.unfocused_sample_frames,
        frames_over_33ms: over(33.),
        frames_over_50ms: over(50.),
        frames_over_100ms: over(100.),
        frames_over_250ms: over(250.),
        work_scope: "Simulation, persistence, interface elapsed application work in milliseconds, in that order (wall clock, including waits). Interface is ui::draw. Sampled at the end of Last, so each phase is attributed to the native frame that did the work; aggregates and maxima locate expensive phases, not GPU timestamps or CPU-cycle samples.",
        work_mean_ms,
        work_max_ms,
        longest_frames: longest
            .into_iter()
            .map(|(sample_index, frame_ms)| SlowFrame {
                sample_index,
                frame_ms,
                work_ms: frames[sample_index].probe.work_ms,
                phases: phases[sample_index],
            })
            .collect(),
        phase_scope: "Wall-clock spans per sampled frame, as {mean, p50, p95, p99, max} in milliseconds. main_cpu: First begin to the end of Last. egui_pass: EguiPostUpdateSet::EndPass (every multipass draw plus end_pass); egui_tessellate: ProcessOutput; interface_total = egui_pass + egui_tessellate. extract: the RenderApp extract function only (never the pipelined channel wait); render_schedule: the Render schedule; present_wait: prepare_windows (swap-chain acquire); render_world_cpu = extract + render_schedule - present_wait of the latest completed render sample (frame i-1 under pipelining). cpu_frame = main_cpu + render_world_cpu summed per sample before taking quantiles, never a sum of means or quantiles. critical_path = max(main_cpu, render_world_cpu) when pipelined, else their sum; informational only.",
        phases: phase_stats(&phases),
        render_samples: render_seqs.len(),
        frames_without_render_sample: frames.iter().filter(|f| f.probe.render.is_none()).count(),
        interface_breakdown_mean_ms: InterfaceBreakdown {
            battle_frame_build: breakdown(0),
            battlefield_paint: breakdown(1),
            backdrop: breakdown(2),
        },
        egui: EguiReport {
            shapes: egui(0),
            clipped_primitives: egui(1),
            mesh_vertices: egui(2),
            mesh_indices: egui(3),
            textured_quads: egui(4),
            passes_per_frame: egui(5),
        },
        scene_load: SceneLoad {
            visible_enemies: scene(|f| f.visible_enemies),
            corpses: scene(|f| f.corpses),
            settled_corpses_drawn: scene(|f| f.probe.scene.settled_corpses),
            collapsing_corpses_drawn: scene(|f| f.probe.scene.collapsing_corpses),
            particles: scene(|f| f.probe.scene.particles),
            fx_views: scene(|f| f.probe.scene.fx_views),
        },
        present: input.present.clone(),
        build: build_info(input.profile_label.as_deref()),
        gpu: gpu_report(input),
    }
}

/// The renderer resources the report reads; any may be absent.
#[derive(SystemParam)]
pub struct RendererInfo<'w> {
    adapter: Option<Res<'w, RenderAdapterInfo>>,
    device: Option<Res<'w, RenderDevice>>,
    store: Option<Res<'w, DiagnosticsStore>>,
}

/// End of `Last`: samples the closed frame and writes the report once the
/// sample target is met (or the active attempt ends).
pub fn capture_performance(
    mut state: Local<FrameCapture>,
    time: Res<Time<Real>>,
    (session, mut runtime, mut status): (
        Res<crate::runner::GameSession>,
        ResMut<crate::runner::Runtime>,
        ResMut<crate::ui::StatusUi>,
    ),
    window: Single<&Window>,
    RendererInfo {
        adapter,
        device,
        store,
    }: RendererInfo,
    (config, settings, probe, closed): (
        Res<crate::launch::LaunchConfig>,
        Option<Res<crate::settings::Settings>>,
        Res<PerfProbe>,
        Res<PerfFrame>,
    ),
    mut exit: MessageWriter<AppExit>,
) {
    if !state.initialized {
        state.initialized = true;
        if let Some(perf) = config.perf() {
            state.path = perf.report.clone();
            state.warmup = perf.warmup_frames;
            state.target_samples = perf.frames;
            state.active_only = perf.active_only;
        }
    }
    if state.path.is_none() || state.done {
        return;
    }
    let gpu_measurement = store
        .as_ref()
        .and_then(|store| store.get(&EGUI_GPU))
        .and_then(|diagnostic| diagnostic.measurement())
        .map(|m| (m.time, m.value));
    let gpu_egui_ms = match gpu_measurement {
        Some((at, value)) if state.last_gpu_measurement != Some(at) => {
            state.last_gpu_measurement = Some(at);
            Some(value)
        }
        _ => None,
    };
    if let Some(a) = session.game.attempt() {
        if state.initial_battle.is_none() {
            state.initial_battle = Some(BattleContext {
                mode: a.mode,
                simulation_tick: a.tick,
                lane_count: a.lane_count,
                width_yards: a.width_yards,
                enemies: a.enemies.len(),
                corpses: a.corpses.len(),
            });
        }
        state.peak_enemies = state.peak_enemies.max(a.enemies.len());
        state.peak_corpses = state.peak_corpses.max(a.corpses.len());
    }
    let attempt_finished =
        state.active_only && session.game.attempt().is_none() && !state.samples.is_empty();
    if !attempt_finished {
        if state.active_only
            && (session.game.global_paused() || session.game.attempt().is_none_or(|a| a.paused))
        {
            return;
        }
        state.frames += 1;
        if state.frames <= state.warmup {
            return;
        }
        let ms = time.delta_secs_f64() * 1000.;
        if ms.is_finite() && ms > 0. {
            let attempt = session.game.attempt();
            state.samples.push(RecordedFrame {
                frame_ms: ms,
                probe: closed.0.clone().unwrap_or_default(),
                visible_enemies: attempt.map_or(0, |a| a.enemies.len()),
                corpses: attempt.map_or(0, |a| a.corpses.len()),
                gpu_egui_ms,
            });
            state.unfocused_samples += usize::from(!window.focused);
        }
        if state.samples.len() < state.target_samples as usize {
            return;
        }
    }
    state.done = true;
    let attempt = session.game.attempt();
    let perf = config.perf();
    let input = ReportInput {
        scene: config
            .scenario
            .clone()
            .unwrap_or_else(|| "ordinary-session".into()),
        cpu: perf
            .and_then(|perf| perf.cpu.clone())
            .unwrap_or_else(|| "not recorded".into()),
        adapter: adapter
            .as_ref()
            .map_or_else(|| "not reported".into(), |a| a.name.clone()),
        backend: adapter
            .as_ref()
            .map_or_else(|| "not reported".into(), |a| format!("{:?}", a.backend)),
        width_physical: window.resolution.physical_width(),
        height_physical: window.resolution.physical_height(),
        windows_scale: f64::from(window.resolution.scale_factor()),
        ui_scale: config.review().and_then(|r| r.scale).unwrap_or(1.),
        warmup_frames: state.warmup,
        requested_sample_frames: state.target_samples,
        frames: std::mem::take(&mut state.samples),
        unfocused_sample_frames: state.unfocused_samples,
        initial_battle: state.initial_battle.clone(),
        peak_visible_enemies: state.peak_enemies,
        peak_corpses: state.peak_corpses,
        persistence: runtime.persistence_metrics(),
        end: EndState {
            visible_enemies: attempt.map_or(0, |a| a.enemies.len()),
            lane_count: attempt.map(|a| a.lane_count),
            width_yards: attempt.map(|a| a.width_yards),
            population_cap: attempt.map(|a| a.enemy_cap()),
            threat_rank: attempt.map(|a| a.progression.rank),
            visible_structures: attempt.map_or(0, |a| a.structures.len()),
            parallel_battles: session.game.battles().iter().filter(|b| !b.settled).count(),
            simulation_ticks: runtime.simulated_ticks,
            paused: session.game.global_paused() || attempt.is_some_and(|a| a.paused),
            window_focused: window.focused,
        },
        present: Present {
            requested_mode: format!("{:?}", window.present_mode),
            source: "settings",
            pipelined_rendering: probe.pipelined(),
            desired_maximum_frame_latency: window.desired_maximum_frame_latency.map(|n| n.get()),
            low_latency: settings.as_ref().is_some_and(|s| s.low_latency),
        },
        profile_label: perf.and_then(|perf| perf.profile_label.clone()),
        gpu_support: device
            .as_ref()
            .zip(adapter.as_ref())
            .map(|(device, adapter)| {
                gpu_timing_support(device.features(), Backends::from(adapter.backend))
            }),
        gpu_passes: store.as_ref().map_or_else(BTreeMap::new, |store| {
            store
                .iter()
                .filter_map(|d| {
                    let name = d
                        .path()
                        .as_str()
                        .strip_prefix("render/")?
                        .strip_suffix("/elapsed_gpu")?;
                    Some((name.to_owned(), d.average()?))
                })
                .collect()
        }),
        pipelined: probe.pipelined(),
    };
    let report = build_report(&input);
    let path = state.path.as_ref().expect("configured report");
    let result = (|| -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let bytes = serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?;
        std::fs::write(path, bytes).map_err(|e| e.to_string())
    })();
    match result {
        Ok(()) => {
            info!("Native frame evidence: {}", path.display());
            // The ordinary persistence service owns the durable close barrier.
            // Do not exit over an older periodic checkpoint still in flight.
            crate::ui::request_save(&mut runtime, &mut status, crate::runner::SaveIntent::Quit);
        }
        Err(error) => {
            error!("Frame report failed: {error}");
            exit.write(AppExit::error());
        }
    }
}

#[cfg(test)]
mod tests;
