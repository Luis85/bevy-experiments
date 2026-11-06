//! Opt-in, bounded native frame evidence (`LASTLIGHT_PERF_REPORT`), report
//! version 3. `capture_performance` runs at the end of `Last`, after
//! `PerfSet::CloseFrame`, so every phase of frame i lands in sample i. Every v2
//! key keeps its name and meaning; v3 adds `phases`, the interface breakdown,
//! egui and scene counts, the present and build context and GPU time, and
//! (W1-64) `egui.texture_uploads`. W1-68 adds the between-frames gap and its
//! hand-off and channel wait, render-thread idle, per-frame `events`,
//! wall-clock instants and a window `attribution` in `longest_frames`, and
//! `slow_frames`.
use super::{
    EguiCount, FrameSample, PerfFrame, PerfProbe, RenderSample,
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
    /// Wall clock (ms since the probe epoch) of the instant `Time<Real>`
    /// closed `frame_ms` at: the frame interval is `time_ms - frame_ms ..
    /// time_ms`. `None` when the time had no update (W1-68).
    pub time_ms: Option<f64>,
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
    /// W1-68: the main thread between the previous end of `Last` and this
    /// First, its render hand-off, the pipelined channel wait inside that,
    /// the rest of the gap (winit and the OS), and the render-thread idle
    /// before the latest render sample.
    gap_ms: T,
    handoff_ms: T,
    channel_wait_ms: T,
    outside_update_ms: T,
    render_idle_ms: T,
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
    texture_uploads: TextureUploadTotals,
}
/// Every egui texture upload over the sampled (post-warmup) frames: the
/// `textures_delta.set` entries and their bytes, gated at
/// `perf_budget::EGUI_TEXTURE_UPLOADS_AFTER_WARMUP_MAX` in battle.
#[derive(Serialize)]
struct TextureUploadTotals {
    count: u64,
    bytes: u64,
    /// Sampled frames with at least one upload.
    frames: usize,
}
#[derive(Serialize)]
struct SceneLoad {
    visible_enemies: CountStats,
    corpses: CountStats,
    settled_corpses_drawn: CountStats,
    collapsing_corpses_drawn: CountStats,
    particles: CountStats,
    fx_views: CountStats,
    /// Hostiles drawn with a hit flash, and the quads those flashes drew (W1-66).
    flashing_enemies: CountStats,
    flash_quads: CountStats,
}
/// Per-frame window, image-asset and audio-asset events and audio voices
/// spawned (W1-68).
#[derive(Serialize)]
struct EventReport {
    window: CountStats,
    image_assets: CountStats,
    audio_assets: CountStats,
    audio_voices: CountStats,
}
/// One slow frame's events, with the egui texture bytes it uploaded.
#[derive(Serialize)]
struct SlowFrameEvents {
    window: u64,
    image_assets: u64,
    audio_assets: u64,
    audio_voices: u64,
    texture_upload_bytes: u64,
}
/// Where one frame's wall-clock window `window_begin_ms .. time_ms` went
/// (W1-68). Main thread: the main schedules, the pipelined channel waits,
/// the rest of the render hand-offs, and the time outside App::update, as
/// overlaps with the window; `unattributed_ms` is the window no main-thread
/// span covers and `coverage` the covered share. `outside_update_ms` is a
/// residual (gap minus hand-off), so consecutive samples tile the main
/// thread and `coverage` is 1.0 by construction inside the recorded range;
/// `named_coverage` leaves the residual out, so a stall outside every named
/// span shows as a low share (W1-68 review). Render thread: present
/// waits, the rest of the render frames, and the window outside any recorded
/// render frame (idle, or a frame no sample kept).
#[derive(Serialize)]
struct Attribution {
    window_begin_ms: f64,
    main_cpu_ms: f64,
    channel_wait_ms: f64,
    handoff_other_ms: f64,
    outside_update_ms: f64,
    unattributed_ms: f64,
    coverage: f64,
    named_coverage: f64,
    render_present_ms: f64,
    render_other_ms: f64,
    render_unbusy_ms: f64,
    /// W1-61: the main thread's render hand-offs (not pipelined: its own
    /// extract and Render schedule; pipelined: the channel receive) that
    /// overlap a render frame's swap-chain acquire (`prepare_windows`), inside
    /// the window. The driver-stall rule reads it.
    acquire_wait_ms: f64,
}
/// One sampled frame the driver-stall rule excluded (W1-61 user ruling):
/// its acquire wait and share, its in-process work (main + render CPU) and
/// its named coverage.
#[derive(Serialize)]
struct DriverStall {
    sample_index: usize,
    frame_ms: f64,
    acquire_wait_ms: f64,
    acquire_share: f64,
    in_process_ms: f64,
    named_coverage: f64,
}
/// The frame-interval statistics and counts of every sampled frame outside
/// the driver stalls: the gated `frames_over_50ms`, and the pipelined
/// decision's frames over 33 ms, mean FPS and p99 interval.
#[derive(Serialize)]
struct OutsideStalls {
    sample_frames: usize,
    mean_observed_fps: f64,
    p99_frame_ms: f64,
    frames_over_33ms: usize,
    frames_over_50ms: usize,
}
#[derive(Serialize)]
struct DriverStallReport {
    rule: &'static str,
    frames: Vec<DriverStall>,
    outside: OutsideStalls,
}
/// Every sampled frame over 50 ms (W1-68): how many had a wall-clock window,
/// how many of those main-thread spans cover to at least 95%, the lowest
/// coverage and the most unattributed time. The `_named` counts and
/// `max_outside_update_ms` leave out the `outside_update` residual (W1-68
/// review).
#[derive(Serialize)]
struct SlowFrameSummary {
    over_50ms: usize,
    timed: usize,
    covered_95: usize,
    min_coverage: Option<f64>,
    max_unattributed_ms: f64,
    covered_95_named: usize,
    min_named_coverage: Option<f64>,
    max_outside_update_ms: f64,
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
    /// W1-61: of `frames_over_50ms`, the driver stalls (`driver_stalls`).
    frames_over_50ms_driver_stall: usize,
    driver_stalls: DriverStallReport,
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
    events: EventReport,
    attribution_scope: &'static str,
    slow_frames: SlowFrameSummary,
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
    /// Wall clock, ms since the probe epoch (W1-68): the frame interval's
    /// end, this frame's First begin and end of `Last`, and the latest render
    /// sample's sequence number, begin, present begin and end.
    time_ms: Option<f64>,
    main_begin_ms: f64,
    main_end_ms: f64,
    render_seq: u64,
    render_begin_ms: Option<f64>,
    render_present_begin_ms: Option<f64>,
    render_end_ms: Option<f64>,
    events: SlowFrameEvents,
    attribution: Option<Attribution>,
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
        gap_ms: sample.gap_ms,
        handoff_ms: sample.handoff_ms,
        channel_wait_ms: sample.channel_wait_ms,
        outside_update_ms: (sample.gap_ms - sample.handoff_ms).max(0.),
        render_idle_ms: render.idle_ms,
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
        gap_ms: stats(&column(|p| p.gap_ms)),
        handoff_ms: stats(&column(|p| p.handoff_ms)),
        channel_wait_ms: stats(&column(|p| p.channel_wait_ms)),
        outside_update_ms: stats(&column(|p| p.outside_update_ms)),
        render_idle_ms: stats(&column(|p| p.render_idle_ms)),
    }
}

/// Frames above this are slow for `slow_frames` and are attributed.
const SLOW_FRAME_MS: f64 = 50.;
/// The coverage `slow_frames.covered_95` counts.
const COVERED_SHARE: f64 = 0.95;

fn overlap(begin: f64, end: f64, window: (f64, f64)) -> f64 {
    (end.min(window.1) - begin.max(window.0)).max(0.)
}
/// Attributes the wall-clock window of `frames[index]` to every recorded
/// main-thread and render-thread span it overlaps. Each sample's main thread
/// is, in order: the hand-off right after the previous `Last` (channel wait
/// first, it is the receive), the rest of the gap, then First..Last. Render
/// frames are counted once each (`render_frames`).
fn attribute(
    frames: &[RecordedFrame],
    renders: &[RenderSample],
    index: usize,
) -> Option<Attribution> {
    let frame = &frames[index];
    let end = frame.time_ms?;
    let window = (end - frame.frame_ms, end);
    let (mut main_cpu, mut wait, mut handoff_other, mut outside) = (0., 0., 0., 0.);
    let mut handoffs = Vec::new();
    for sample in frames.iter().map(|f| &f.probe) {
        if sample.main_begin_ms <= 0. {
            continue;
        }
        main_cpu += overlap(sample.main_begin_ms, sample.main_end_ms, window);
        let gap_begin = sample.main_begin_ms - sample.gap_ms;
        let handoff_end = gap_begin + sample.handoff_ms.min(sample.gap_ms);
        let wait_end = gap_begin + sample.channel_wait_ms.min(handoff_end - gap_begin);
        wait += overlap(gap_begin, wait_end, window);
        handoff_other += overlap(wait_end, handoff_end, window);
        outside += overlap(handoff_end, sample.main_begin_ms, window);
        let begin = gap_begin.max(window.0);
        let end = handoff_end.min(window.1);
        if end > begin {
            handoffs.push((begin, end));
        }
    }
    let (mut busy, mut present, mut acquire) = (0., 0., 0.);
    for render in renders {
        busy += overlap(render.begin_ms, render.end_ms, window);
        let acquire_span = (
            render.present_begin_ms,
            render.present_begin_ms + render.present_wait_ms,
        );
        present += overlap(acquire_span.0, acquire_span.1, window);
        // Both are disjoint sets of spans (one main thread, one render
        // thread), so the pairwise overlaps add up without double counting.
        for &handoff in &handoffs {
            acquire += overlap(acquire_span.0, acquire_span.1, handoff);
        }
    }
    let named = main_cpu + wait + handoff_other;
    let covered = named + outside;
    let share = |part: f64| {
        if frame.frame_ms > 0. {
            (part / frame.frame_ms).min(1.)
        } else {
            1.
        }
    };
    Some(Attribution {
        window_begin_ms: window.0,
        main_cpu_ms: main_cpu,
        channel_wait_ms: wait,
        handoff_other_ms: handoff_other,
        outside_update_ms: outside,
        unattributed_ms: (frame.frame_ms - covered).max(0.),
        coverage: share(covered),
        named_coverage: share(named),
        render_present_ms: present,
        render_other_ms: (busy - present).max(0.),
        render_unbusy_ms: (frame.frame_ms - busy).max(0.),
        acquire_wait_ms: acquire,
    })
}
/// Every distinct completed render frame the samples kept, in end order.
fn render_frames(frames: &[RecordedFrame]) -> Vec<RenderSample> {
    let mut renders: Vec<RenderSample> = frames
        .iter()
        .flat_map(|f| f.probe.render.into_iter().chain(f.probe.slowest_present))
        .filter(|render| render.end_ms > 0.)
        .collect();
    renders.sort_by(|a, b| a.end_ms.total_cmp(&b.end_ms));
    renders.dedup_by(|a, b| a.end_ms == b.end_ms);
    renders
}

const DRIVER_STALL_RULE: &str = "W1-61, user ruling on gate 3 (2026-11-06, 'Exclude driver stalls'). A sampled frame over 50 ms is an environmental driver stall when its swap-chain acquire wait holds at least 80% of it (attribution.acquire_wait_ms: the main thread's render hand-off overlapping a render frame's prepare_windows, i.e. present_wait when not pipelined, or the channel wait while the render thread is in present), its named spans cover at least 95% (attribution.named_coverage) and its in-process work stays below 50 ms (in_process_ms = main CPU + render CPU. Main CPU is the larger of attribution.main_cpu_ms, the main schedules overlapping the window, which closes at First and so holds the previous sample's First..Last, and the sample's own phases.main_cpu_ms. Render CPU is the larger of the latest and the stalled render frame's extract + Render schedule - present wait). A frame without attribution is never a stall. frames lists every stall; outside holds the interval statistics and counts of every other sampled frame: measure_performance gates frames_over_50ms on outside.frames_over_50ms, and the pipelined decision reads outside.frames_over_33ms, mean_observed_fps and p99_frame_ms. The v2 frames_over_* keys still count every frame.";

/// The driver-stall rule (`perf_budget::DRIVER_STALL_*`) for one frame.
fn driver_stall(
    index: usize,
    frame: &RecordedFrame,
    phases: &Phases<f64>,
    attribution: Option<&Attribution>,
) -> Option<DriverStall> {
    use crate::perf_budget::{
        DRIVER_STALL_ACQUIRE_SHARE_MIN, DRIVER_STALL_FRAME_MS_OVER,
        DRIVER_STALL_IN_PROCESS_MS_BELOW, DRIVER_STALL_NAMED_COVERAGE_MIN,
    };
    if frame.frame_ms <= DRIVER_STALL_FRAME_MS_OVER {
        return None;
    }
    let attribution = attribution?;
    let acquire_share = attribution.acquire_wait_ms / frame.frame_ms;
    let stalled_render = frame
        .probe
        .slowest_present
        .map_or(0., |render| render.cpu_ms());
    // The window closes at First, so the sample's own First..Last runs after
    // it: the main schedule inside the stalled frame is the previous one's
    // (attribution.main_cpu_ms). The larger of the two stays conservative.
    let main_cpu = attribution.main_cpu_ms.max(phases.main_cpu_ms);
    let in_process_ms = main_cpu + phases.render_world_cpu_ms.max(stalled_render);
    (acquire_share >= DRIVER_STALL_ACQUIRE_SHARE_MIN
        && attribution.named_coverage >= DRIVER_STALL_NAMED_COVERAGE_MIN
        && in_process_ms < DRIVER_STALL_IN_PROCESS_MS_BELOW)
        .then_some(DriverStall {
            sample_index: index,
            frame_ms: frame.frame_ms,
            acquire_wait_ms: attribution.acquire_wait_ms,
            acquire_share,
            in_process_ms,
            named_coverage: attribution.named_coverage,
        })
}
fn driver_stall_report(
    frames: &[RecordedFrame],
    renders: &[RenderSample],
    phases: &[Phases<f64>],
) -> DriverStallReport {
    let stalls: Vec<DriverStall> = (0..frames.len())
        .filter(|&i| frames[i].frame_ms > crate::perf_budget::DRIVER_STALL_FRAME_MS_OVER)
        .filter_map(|i| {
            let attribution = attribute(frames, renders, i);
            driver_stall(i, &frames[i], &phases[i], attribution.as_ref())
        })
        .collect();
    let outside: Vec<f64> = frames
        .iter()
        .enumerate()
        .filter(|(i, _)| !stalls.iter().any(|stall| stall.sample_index == *i))
        .map(|(_, frame)| frame.frame_ms)
        .collect();
    let mut sorted = outside.clone();
    sorted.sort_by(f64::total_cmp);
    let mean_ms = mean(&outside);
    let over = |limit: f64| outside.iter().filter(|&&ms| ms > limit).count();
    DriverStallReport {
        rule: DRIVER_STALL_RULE,
        frames: stalls,
        outside: OutsideStalls {
            sample_frames: outside.len(),
            mean_observed_fps: if mean_ms > 0. { 1000. / mean_ms } else { 0. },
            p99_frame_ms: quantile(&sorted, 0.99),
            frames_over_33ms: over(33.),
            frames_over_50ms: over(50.),
        },
    }
}
fn slow_frame_summary(frames: &[RecordedFrame], renders: &[RenderSample]) -> SlowFrameSummary {
    let slow: Vec<usize> = (0..frames.len())
        .filter(|&i| frames[i].frame_ms > SLOW_FRAME_MS)
        .collect();
    let attributed: Vec<Attribution> = slow
        .iter()
        .filter_map(|&i| attribute(frames, renders, i))
        .collect();
    SlowFrameSummary {
        over_50ms: slow.len(),
        timed: attributed.len(),
        covered_95: attributed
            .iter()
            .filter(|a| a.coverage >= COVERED_SHARE)
            .count(),
        min_coverage: attributed.iter().map(|a| a.coverage).reduce(f64::min),
        max_unattributed_ms: attributed
            .iter()
            .map(|a| a.unattributed_ms)
            .fold(0., f64::max),
        covered_95_named: attributed
            .iter()
            .filter(|a| a.named_coverage >= COVERED_SHARE)
            .count(),
        min_named_coverage: attributed.iter().map(|a| a.named_coverage).reduce(f64::min),
        max_outside_update_ms: attributed
            .iter()
            .map(|a| a.outside_update_ms)
            .fold(0., f64::max),
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

fn texture_upload_totals(frames: &[RecordedFrame]) -> TextureUploadTotals {
    let count = |f: &RecordedFrame| f.probe.egui[EguiCount::TextureUploads as usize];
    TextureUploadTotals {
        count: frames.iter().map(count).sum(),
        bytes: frames
            .iter()
            .map(|f| f.probe.egui[EguiCount::TextureUploadBytes as usize])
            .sum(),
        frames: frames.iter().filter(|f| count(f) > 0).count(),
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
    let renders = render_frames(frames);
    let driver_stalls = driver_stall_report(frames, &renders, &phases);
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
        frames_over_50ms_driver_stall: driver_stalls.frames.len(),
        driver_stalls,
        work_scope: "Simulation, persistence, interface elapsed application work in milliseconds, in that order (wall clock, including waits). Interface is ui::draw. Sampled at the end of Last, so each phase is attributed to the native frame that did the work; aggregates and maxima locate expensive phases, not GPU timestamps or CPU-cycle samples.",
        work_mean_ms,
        work_max_ms,
        longest_frames: longest
            .into_iter()
            .map(|(sample_index, frame_ms)| {
                let frame = &frames[sample_index];
                let sample = &frame.probe;
                let events = sample.events;
                SlowFrame {
                    sample_index,
                    frame_ms,
                    work_ms: sample.work_ms,
                    phases: phases[sample_index],
                    time_ms: frame.time_ms,
                    main_begin_ms: sample.main_begin_ms,
                    main_end_ms: sample.main_end_ms,
                    render_seq: sample.render_seq,
                    render_begin_ms: sample.render.map(|r| r.begin_ms),
                    render_present_begin_ms: sample.render.map(|r| r.present_begin_ms),
                    render_end_ms: sample.render.map(|r| r.end_ms),
                    events: SlowFrameEvents {
                        window: events.window,
                        image_assets: events.image_assets,
                        audio_assets: events.audio_assets,
                        audio_voices: events.audio_voices,
                        texture_upload_bytes: sample.egui[EguiCount::TextureUploadBytes as usize],
                    },
                    attribution: attribute(frames, &renders, sample_index),
                }
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
            texture_uploads: texture_upload_totals(frames),
        },
        scene_load: SceneLoad {
            visible_enemies: scene(|f| f.visible_enemies),
            corpses: scene(|f| f.corpses),
            settled_corpses_drawn: scene(|f| f.probe.scene.settled_corpses),
            collapsing_corpses_drawn: scene(|f| f.probe.scene.collapsing_corpses),
            particles: scene(|f| f.probe.scene.particles),
            fx_views: scene(|f| f.probe.scene.fx_views),
            flashing_enemies: scene(|f| f.probe.scene.flashing_enemies),
            flash_quads: scene(|f| f.probe.scene.flash_quads),
        },
        events: EventReport {
            window: count_stats(frames.iter().map(|f| f.probe.events.window)),
            image_assets: count_stats(frames.iter().map(|f| f.probe.events.image_assets)),
            audio_assets: count_stats(frames.iter().map(|f| f.probe.events.audio_assets)),
            audio_voices: count_stats(frames.iter().map(|f| f.probe.events.audio_voices)),
        },
        attribution_scope: "W1-68. Instants are wall clock in ms since the probe epoch. A frame's window is time_ms - frame_ms .. time_ms, where time_ms is the instant Time<Real> used (the render world's send_time when it arrived, else First). gap = previous end of Last to First on the main thread; handoff = its first part, the render hand-off (pipelined: RenderExtractApp extract = channel receive + RenderApp extract + send, channel_wait = that minus the extract; not pipelined: RenderApp extract + Render schedule); outside_update = gap - handoff (winit event loop, OS). render_idle = previous render end to this render begin. attribution overlaps the window with every sample's main-thread spans and every distinct render sample. outside_update is a residual, so coverage (which counts it) is 1.0 by construction inside the recorded range; named_coverage = (main_cpu + channel_wait + handoff_other) / frame_ms leaves it out, and an unexplained stall shows as a large outside_update and a low named_coverage. slow_frames counts frames over 50 ms; covered_95_named, min_named_coverage and max_outside_update_ms are the named-span counterparts.",
        slow_frames: slow_frame_summary(frames, &renders),
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
    // The plan says whether a launch override set the present mode.
    (window, plan): (Single<&Window>, Option<Res<crate::render_plan::RenderPlan>>),
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
                time_ms: time.last_update().and_then(|at| probe.epoch_ms(at)),
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
            source: plan.map_or("settings", |plan| plan.source.label()),
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
