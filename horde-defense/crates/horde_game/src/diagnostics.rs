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
    /// Hostiles drawn with a hit flash this pass (W1-66).
    pub flashing_enemies: usize,
    /// Quads those hit flashes appended to the sprite layer (W1-66).
    pub flash_quads: usize,
}

/// Main-world begin marks, as nanoseconds since `PerfCounters::epoch`.
#[derive(Clone, Copy)]
enum Mark {
    Main,
    EguiPass,
    Tessellate,
    Render,
    PresentWait,
    /// Start of the `RenderApp` extract (the non-pipelined hand-off begins here).
    Extract,
}
const MARKS: usize = 6;
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
    /// The battlefield painter's own paint jobs and their mesh vertices
    /// (W2-47): those clipped to the clip it painted with.
    BattlefieldPrimitives,
    BattlefieldVertices,
}
const EGUI_COUNTS: usize = 10;
/// The [`SceneDraw`] fields, in `record_scene` order.
const SCENE_COUNTS: usize = 6;
/// The [`FrameEvents`] fields, in `record_events` order.
const EVENT_COUNTS: usize = 4;

/// What arrived or started in one main-world frame (`events`, W1-68): winit
/// window messages, `Image` and `AudioSource` asset events, and audio voices
/// (`AudioPlayer` entities) spawned.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FrameEvents {
    pub window: u64,
    pub image_assets: u64,
    pub audio_assets: u64,
    pub audio_voices: u64,
}

/// One completed render-world frame: extraction, the Render schedule and the
/// swap-chain wait inside it (`prepare_windows`), in milliseconds. The
/// instants (W1-68) are wall clock, milliseconds since the probe epoch.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RenderSample {
    pub extract_ms: f64,
    pub render_schedule_ms: f64,
    pub present_wait_ms: f64,
    /// Render schedule begin, `prepare_windows` begin and Render schedule end.
    pub begin_ms: f64,
    pub present_begin_ms: f64,
    pub end_ms: f64,
    /// From the previous render frame's end to this one's begin: the render
    /// thread waiting for the next extraction when pipelined, main-thread work
    /// otherwise. 0 for the first render frame.
    pub idle_ms: f64,
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
    /// Of every render frame completed since the previous close, the one with
    /// the longest swap-chain acquire (W1-61), or `None` when none completed.
    /// Pipelined, a render frame stalled in `prepare_windows` can complete
    /// and be replaced by the next one before any close reads `render`; the
    /// driver-stall rule still sees its present wait here.
    pub slowest_present: Option<RenderSample>,
    /// `[Simulation, Persistence, Interface]`.
    pub work_ms: [f64; 3],
    /// `[battle_frame_build, battlefield_paint, backdrop]`.
    pub interface_ms: [f64; 3],
    /// `[shapes, clipped_primitives, mesh_vertices, mesh_indices, textured_quads, passes,
    /// texture_uploads, texture_upload_bytes, battlefield_primitives, battlefield_vertices]`.
    pub egui: [u64; EGUI_COUNTS],
    pub scene: SceneDraw,
    /// First begin and the end of `Last`, wall clock in ms since the probe
    /// epoch (W1-68).
    pub main_begin_ms: f64,
    pub main_end_ms: f64,
    /// The main thread between the previous frame's end of `Last` and this
    /// frame's First: the render hand-off, then winit and the OS. 0 for the
    /// first frame.
    pub gap_ms: f64,
    /// Of `gap_ms`, the render hand-off right after the previous `Last`:
    /// pipelined, the `RenderExtractApp` extract (channel receive, extract,
    /// send); otherwise the `RenderApp` extract and Render schedule.
    pub handoff_ms: f64,
    /// Of `handoff_ms`, the pipelined wait for the render thread to return
    /// the render app (and the send back); 0 when not pipelined.
    pub channel_wait_ms: f64,
    pub events: FrameEvents,
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
    scene: [AtomicU64; SCENE_COUNTS],
    extract_ns: AtomicU64,
    present_wait_ns: AtomicU64,
    /// W1-68: the previous end of `Last` and render end (ns since `epoch`),
    /// the hand-off and channel wait since the last close, and the events.
    last_main_end_ns: AtomicU64,
    last_render_end_ns: AtomicU64,
    handoff_ns: AtomicU64,
    channel_wait_ns: AtomicU64,
    events: [AtomicU64; EVENT_COUNTS],
    /// The latest completed render-world sample and its sequence number.
    render: Mutex<RenderSlot>,
    pipelined: AtomicBool,
    /// The clip the main battlefield painter painted with this frame, taken
    /// by the tessellation probe (W2-47); `None` when no battlefield drew.
    battlefield_clip: Mutex<Option<bevy_egui::egui::Rect>>,
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
            last_main_end_ns: AtomicU64::new(0),
            last_render_end_ns: AtomicU64::new(0),
            handoff_ns: AtomicU64::new(0),
            channel_wait_ns: AtomicU64::new(0),
            events: Default::default(),
            render: Mutex::new(RenderSlot::default()),
            pipelined: AtomicBool::new(false),
            battlefield_clip: Mutex::new(None),
        }
    }
}

/// The render samples the render world publishes for the next close: the
/// latest and its sequence number, and the one with the longest present wait
/// since the previous close (W1-61).
#[derive(Default)]
struct RenderSlot {
    seq: u64,
    latest: Option<RenderSample>,
    slowest_present: Option<RenderSample>,
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
    /// `instant` as wall-clock ms since the probe epoch; `None` before it.
    pub fn epoch_ms(&self, instant: Instant) -> Option<f64> {
        instant
            .checked_duration_since(self.0.epoch)
            .map(|since| since.as_secs_f64() * 1000.)
    }
    /// The begin mark of `mark` (ns since `epoch`), 0 when never set.
    fn mark_ns(&self, mark: Mark) -> u64 {
        self.0.marks_ns[mark as usize].load(Ordering::Relaxed)
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
            draw.flashing_enemies,
            draw.flash_quads,
        ];
        for (slot, count) in self.0.scene.iter().zip(counts) {
            slot.store(count as u64, Ordering::Relaxed);
        }
    }
    /// The clip rect the main battlefield painter drew with this pass
    /// (W2-47); the last pass of a frame wins. The tessellation probe counts
    /// the paint jobs clipped to it as `egui.battlefield_primitives` and
    /// `egui.battlefield_vertices`.
    pub fn record_battlefield_clip(&self, clip: bevy_egui::egui::Rect) {
        if let Ok(mut slot) = self.0.battlefield_clip.lock() {
            *slot = Some(clip);
        }
    }
    /// Takes this frame's battlefield clip, so a frame without a battlefield
    /// counts none.
    fn take_battlefield_clip(&self) -> Option<bevy_egui::egui::Rect> {
        self.0
            .battlefield_clip
            .lock()
            .ok()
            .and_then(|mut slot| slot.take())
    }
    /// This frame's window, image and audio events and new voices (W1-68).
    fn record_events(&self, events: FrameEvents) {
        let counts = [
            events.window,
            events.image_assets,
            events.audio_assets,
            events.audio_voices,
        ];
        for (slot, count) in self.0.events.iter().zip(counts) {
            slot.fetch_add(count, Ordering::Relaxed);
        }
    }
    /// Publishes one completed render-world frame for the main world.
    fn publish_render(&self, sample: RenderSample) {
        if let Ok(mut slot) = self.0.render.lock() {
            slot.seq += 1;
            slot.latest = Some(sample);
            if slot
                .slowest_present
                .is_none_or(|slowest| sample.present_wait_ms > slowest.present_wait_ms)
            {
                slot.slowest_present = Some(sample);
            }
        }
    }
    /// Takes this frame's counters and the latest completed render sample.
    fn close_frame(&self) -> FrameSample {
        let take = |slot: &AtomicU64| slot.swap(0, Ordering::Relaxed);
        let end = self.now_ns();
        let begin = self.mark_ns(Mark::Main);
        let main_cpu_ms = if begin == 0 {
            0.
        } else {
            ms(end.saturating_sub(begin))
        };
        self.0.marks_ns[Mark::Main as usize].store(0, Ordering::Relaxed);
        let previous_end = self.0.last_main_end_ns.swap(end, Ordering::Relaxed);
        let gap_ms = if begin == 0 || previous_end == 0 {
            0.
        } else {
            ms(begin.saturating_sub(previous_end))
        };
        let events: [u64; EVENT_COUNTS] = std::array::from_fn(|i| take(&self.0.events[i]));
        let scene: [u64; SCENE_COUNTS] = std::array::from_fn(|i| take(&self.0.scene[i]));
        let (render_seq, render, slowest_present) =
            self.0.render.lock().map_or((0, None, None), |mut slot| {
                (slot.seq, slot.latest, slot.slowest_present.take())
            });
        FrameSample {
            main_cpu_ms,
            egui_pass_ms: ms(take(&self.0.egui_pass_ns)),
            egui_tessellate_ms: ms(take(&self.0.tessellate_ns)),
            render,
            render_seq,
            slowest_present,
            work_ms: std::array::from_fn(|i| ms(take(&self.0.work_ns[i]))),
            interface_ms: std::array::from_fn(|i| ms(take(&self.0.interface_ns[i]))),
            egui: std::array::from_fn(|i| take(&self.0.egui[i])),
            scene: SceneDraw {
                settled_corpses: scene[0] as usize,
                collapsing_corpses: scene[1] as usize,
                particles: scene[2] as usize,
                fx_views: scene[3] as usize,
                flashing_enemies: scene[4] as usize,
                flash_quads: scene[5] as usize,
            },
            main_begin_ms: ms(begin),
            main_end_ms: ms(end),
            gap_ms,
            handoff_ms: ms(take(&self.0.handoff_ns)),
            channel_wait_ms: ms(take(&self.0.channel_wait_ns)),
            events: FrameEvents {
                window: events[0],
                image_assets: events[1],
                audio_assets: events[2],
                audio_voices: events[3],
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
            .add_systems(
                Last,
                (
                    count_events.before(PerfSet::CloseFrame),
                    close_frame.in_set(PerfSet::CloseFrame),
                ),
            );
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
/// Message cursors of [`count_events`]. Every message type is optional, so a
/// MinimalPlugins app without windows, assets or audio counts zero.
#[cfg(feature = "review")]
#[derive(Default)]
struct EventCursors {
    window: bevy::ecs::message::MessageCursor<bevy::window::WindowEvent>,
    images: bevy::ecs::message::MessageCursor<AssetEvent<Image>>,
    audio: bevy::ecs::message::MessageCursor<AssetEvent<AudioSource>>,
}
#[cfg(feature = "review")]
fn count_events(
    probe: Res<PerfProbe>,
    mut cursors: Local<EventCursors>,
    window: Option<Res<Messages<bevy::window::WindowEvent>>>,
    images: Option<Res<Messages<AssetEvent<Image>>>>,
    audio: Option<Res<Messages<AssetEvent<AudioSource>>>>,
    voices: Query<(), Added<AudioPlayer>>,
) {
    let EventCursors {
        window: window_cursor,
        images: image_cursor,
        audio: audio_cursor,
    } = &mut *cursors;
    probe.record_events(FrameEvents {
        window: window.map_or(0, |m| window_cursor.read(&m).count() as u64),
        image_assets: images.map_or(0, |m| image_cursor.read(&m).count() as u64),
        audio_assets: audio.map_or(0, |m| audio_cursor.read(&m).count() as u64),
        audio_voices: voices.iter().count() as u64,
    });
}
#[cfg(feature = "review")]
fn close_frame(probe: Res<PerfProbe>, mut frame: ResMut<PerfFrame>) {
    frame.0 = Some(probe.close_frame());
}
