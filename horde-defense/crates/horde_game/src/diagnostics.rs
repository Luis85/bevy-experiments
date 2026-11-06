//! Opt-in performance probe. `install` adds `PerfPlugin` only to a `review`
//! build launched with a frame report (`LASTLIGHT_PERF_REPORT`); otherwise no
//! `PerfProbe` resource, plugin or system exists and every timer below is a
//! `None` check. The span systems and the bounded frame report are review
//! tooling (`egui_probe`, `render_probe`, `gpu_span`, `frame_report`).
#![cfg_attr(
    not(feature = "review"),
    allow(
        dead_code,
        reason = "without `review` no PerfProbe is ever constructed; callers keep their Option<&PerfProbe> seams"
    )
)]
use crate::launch::LaunchConfig;
use bevy::prelude::*;
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Instant,
};
#[cfg(feature = "review")]
mod egui_probe;
#[cfg(feature = "review")]
mod frame_report;
#[cfg(feature = "review")]
mod gpu_span;
#[cfg(feature = "review")]
mod render_probe;
#[cfg(feature = "review")]
pub use frame_report::capture_performance;
#[cfg(test)]
mod tests;

/// Aggregate application work, reported as v2 `work_mean_ms`/`work_max_ms`.
#[derive(Clone, Copy)]
pub enum WorkPhase {
    Simulation,
    Persistence,
    Interface,
}
/// Sub-spans inside `ui::draw` (`interface_breakdown_mean_ms`).
#[derive(Clone, Copy)]
pub enum InterfaceSpan {
    BattleFrameBuild,
    BattlefieldPaint,
    Backdrop,
}
/// What the main battlefield painter drew this pass (`scene_load`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SceneDraw {
    pub settled_corpses: usize,
    pub collapsing_corpses: usize,
    pub particles: usize,
    pub fx_views: usize,
}

/// Main-world begin marks, as nanoseconds since `PerfCounters::epoch`.
#[derive(Clone, Copy)]
enum Mark {
    Main,
    EguiPass,
    Tessellate,
    Render,
    PresentWait,
}
const MARKS: usize = 5;
/// Per-frame egui counts (`egui`), in report order.
#[derive(Clone, Copy)]
enum EguiCount {
    Shapes,
    ClippedPrimitives,
    MeshVertices,
    MeshIndices,
    TexturedQuads,
    Passes,
    /// `textures_delta.set` entries and their bytes (W1-64).
    TextureUploads,
    TextureUploadBytes,
}
const EGUI_COUNTS: usize = 8;

/// One completed render-world frame: extraction, the Render schedule and the
/// swap-chain wait inside it (`prepare_windows`), in milliseconds.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RenderSample {
    pub extract_ms: f64,
    pub render_schedule_ms: f64,
    pub present_wait_ms: f64,
}
impl RenderSample {
    /// Render-world CPU: extract + Render schedule − present wait.
    pub fn cpu_ms(&self) -> f64 {
        (self.extract_ms + self.render_schedule_ms - self.present_wait_ms).max(0.)
    }
}

/// Everything one main-world frame recorded, closed at the end of `Last`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FrameSample {
    /// First-schedule begin mark to the end of `Last`.
    pub main_cpu_ms: f64,
    pub egui_pass_ms: f64,
    pub egui_tessellate_ms: f64,
    /// The latest completed render-world sample (frame i−1 when pipelined),
    /// `None` until the first one completes.
    pub render: Option<RenderSample>,
    /// Sequence number of `render`, so the report counts distinct samples.
    pub render_seq: u64,
    /// `[Simulation, Persistence, Interface]`.
    pub work_ms: [f64; 3],
    /// `[battle_frame_build, battlefield_paint, backdrop]`.
    pub interface_ms: [f64; 3],
    /// `[shapes, clipped_primitives, mesh_vertices, mesh_indices, textured_quads, passes,
    /// texture_uploads, texture_upload_bytes]`.
    pub egui: [u64; EGUI_COUNTS],
    pub scene: SceneDraw,
}

/// The atomics one perf launch shares between the main and render worlds.
pub struct PerfCounters {
    epoch: Instant,
    work_ns: [AtomicU64; 3],
    interface_ns: [AtomicU64; 3],
    marks_ns: [AtomicU64; MARKS],
    egui_pass_ns: AtomicU64,
    tessellate_ns: AtomicU64,
    egui: [AtomicU64; EGUI_COUNTS],
    scene: [AtomicU64; 4],
    extract_ns: AtomicU64,
    present_wait_ns: AtomicU64,
    /// The latest completed render-world sample and its sequence number.
    render: Mutex<(u64, Option<RenderSample>)>,
    pipelined: AtomicBool,
}
impl Default for PerfCounters {
    fn default() -> Self {
        Self {
            epoch: Instant::now(),
            work_ns: Default::default(),
            interface_ns: Default::default(),
            marks_ns: Default::default(),
            egui_pass_ns: AtomicU64::new(0),
            tessellate_ns: AtomicU64::new(0),
            egui: Default::default(),
            scene: Default::default(),
            extract_ns: AtomicU64::new(0),
            present_wait_ns: AtomicU64::new(0),
            render: Mutex::new((0, None)),
            pipelined: AtomicBool::new(false),
        }
    }
}

/// The one probe of a perf launch. The same `Arc` is a resource of the main
/// world and of `RenderApp`. Absent in normal play.
#[derive(Resource, Clone, Default)]
pub struct PerfProbe(Arc<PerfCounters>);

fn ms(ns: u64) -> f64 {
    ns as f64 / 1_000_000.
}
fn elapsed_ns(start: Instant) -> u64 {
    start.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64
}
impl PerfProbe {
    fn now_ns(&self) -> u64 {
        elapsed_ns(self.0.epoch)
    }
    fn mark(&self, mark: Mark) {
        self.0.marks_ns[mark as usize].store(self.now_ns(), Ordering::Relaxed);
    }
    /// Nanoseconds since `mark`; 0 when the mark was never set this frame.
    fn since(&self, mark: Mark) -> u64 {
        let begin = self.0.marks_ns[mark as usize].load(Ordering::Relaxed);
        if begin == 0 {
            0
        } else {
            self.now_ns().saturating_sub(begin)
        }
    }
    /// One egui pass of `ui::draw` (`passes_per_frame`).
    pub fn count_egui_pass(&self) {
        self.0.egui[EguiCount::Passes as usize].fetch_add(1, Ordering::Relaxed);
    }
    /// The main battlefield's drawn load; the last pass of a frame wins.
    pub fn record_scene(&self, draw: SceneDraw) {
        let counts = [
            draw.settled_corpses,
            draw.collapsing_corpses,
            draw.particles,
            draw.fx_views,
        ];
        for (slot, count) in self.0.scene.iter().zip(counts) {
            slot.store(count as u64, Ordering::Relaxed);
        }
    }
    /// Takes this frame's counters and the latest completed render sample.
    fn close_frame(&self) -> FrameSample {
        let take = |slot: &AtomicU64| slot.swap(0, Ordering::Relaxed);
        let main_cpu_ms = ms(self.since(Mark::Main));
        self.0.marks_ns[Mark::Main as usize].store(0, Ordering::Relaxed);
        let scene: [u64; 4] = std::array::from_fn(|i| take(&self.0.scene[i]));
        let (render_seq, render) = self.0.render.lock().map_or((0, None), |latest| *latest);
        FrameSample {
            main_cpu_ms,
            egui_pass_ms: ms(take(&self.0.egui_pass_ns)),
            egui_tessellate_ms: ms(take(&self.0.tessellate_ns)),
            render,
            render_seq,
            work_ms: std::array::from_fn(|i| ms(take(&self.0.work_ns[i]))),
            interface_ms: std::array::from_fn(|i| ms(take(&self.0.interface_ns[i]))),
            egui: std::array::from_fn(|i| take(&self.0.egui[i])),
            scene: SceneDraw {
                settled_corpses: scene[0] as usize,
                collapsing_corpses: scene[1] as usize,
                particles: scene[2] as usize,
                fx_views: scene[3] as usize,
            },
        }
    }
    /// Whether the render world runs pipelined on its own thread.
    pub fn pipelined(&self) -> bool {
        self.0.pipelined.load(Ordering::Relaxed)
    }
}

/// Adds its elapsed time to one counter when dropped.
pub struct WorkTimer<'a> {
    slot: &'a AtomicU64,
    start: Instant,
}
impl Drop for WorkTimer<'_> {
    fn drop(&mut self) {
        self.slot
            .fetch_add(elapsed_ns(self.start), Ordering::Relaxed);
    }
}
/// Opt-in aggregate work costs. Without a probe: no timer, allocation or IO.
pub fn work_timer(probe: Option<&PerfProbe>, phase: WorkPhase) -> Option<WorkTimer<'_>> {
    probe.map(|probe| WorkTimer {
        slot: &probe.0.work_ns[phase as usize],
        start: Instant::now(),
    })
}
/// Opt-in sub-span inside `ui::draw`, perf launches only.
pub fn interface_span(probe: Option<&PerfProbe>, span: InterfaceSpan) -> Option<WorkTimer<'_>> {
    probe.map(|probe| WorkTimer {
        slot: &probe.0.interface_ns[span as usize],
        start: Instant::now(),
    })
}

/// The latest closed main-world frame, for the report (end of `Last`).
#[derive(Resource, Default)]
pub struct PerfFrame(pub Option<FrameSample>);

/// `PerfSet::CloseFrame` ends the main-world frame in `Last`; the report
/// system runs after it.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PerfSet {
    CloseFrame,
}

/// The one install entry point, called after `DefaultPlugins`. Adds
/// `PerfPlugin` only when a perf report is configured and `review` is built;
/// otherwise it adds nothing at all.
pub fn install(app: &mut App, config: &LaunchConfig) {
    #[cfg(feature = "review")]
    if config.perf().is_some() {
        app.add_plugins(PerfPlugin);
    }
    #[cfg(not(feature = "review"))]
    let _ = (app, config);
}

/// Inserts the probe into both worlds and adds the span systems, the extract
/// wrapper, the egui probes, the GPU span and `RenderDiagnosticsPlugin`.
/// Tolerates a missing `RenderApp` (MinimalPlugins tests).
#[cfg(feature = "review")]
pub struct PerfPlugin;
#[cfg(feature = "review")]
impl Plugin for PerfPlugin {
    fn build(&self, app: &mut App) {
        let probe = PerfProbe::default();
        app.insert_resource(probe.clone())
            .init_resource::<PerfFrame>()
            .add_systems(First, mark_main_begin)
            .add_systems(Last, close_frame.in_set(PerfSet::CloseFrame));
        egui_probe::add(app);
        if let Some(render_app) = app.get_sub_app_mut(bevy::render::RenderApp) {
            render_app.insert_resource(probe);
            render_probe::add(render_app);
            gpu_span::add(render_app);
            app.add_plugins(bevy::render::diagnostic::RenderDiagnosticsPlugin);
        }
    }
    fn finish(&self, app: &mut App) {
        let probe = app.world().resource::<PerfProbe>().clone();
        let pipelined = app
            .get_sub_app(bevy::render::pipelined_rendering::RenderExtractApp)
            .is_some();
        probe.0.pipelined.store(pipelined, Ordering::Relaxed);
        render_probe::wrap_extract(app, probe);
    }
}
#[cfg(feature = "review")]
fn mark_main_begin(probe: Res<PerfProbe>) {
    probe.mark(Mark::Main);
}
#[cfg(feature = "review")]
fn close_frame(probe: Res<PerfProbe>, mut frame: ResMut<PerfFrame>) {
    frame.0 = Some(probe.close_frame());
}
