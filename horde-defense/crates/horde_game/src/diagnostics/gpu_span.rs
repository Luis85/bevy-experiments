//! Review-only GPU time of the egui pass. Pinned to bevy =0.20.0 (lock 0.20.1)
//! and bevy_egui =0.43.1: it relies on `RenderDiagnosticsPlugin`, the
//! doc-hidden `RecordDiagnostics::begin_time_span`/`end_time_span`, and on
//! `bevy_egui::render::egui_pass` running in `Core2d` after `MainPass` and
//! before `upscaling`. Any mismatch degrades the report to "unsupported"; GPU
//! numbers are informational and never gated.
//!
//! The recorder pairs a span's begin and end by thread, so both marks are
//! exclusive systems, which the executor always runs on the scope thread.
//! Exclusive systems push their own encoders at once, so an `ApplyDeferred`
//! before each first queues the encoders of the passes ordered before it:
//! the begin timestamp lands after the main pass and the end after egui.
use bevy::{
    core_pipeline::{Core2d, Core2dSystems, upscaling::upscaling},
    prelude::*,
    render::{
        diagnostic::{DiagnosticsRecorder, RecordDiagnostics},
        render_resource::WgpuFeatures,
        renderer::{CurrentView, PendingCommandBuffers, RenderDevice},
        settings::Backends,
    },
};
use bevy_egui::render::{EguiCameraView, egui_pass};

/// The span name and the `DiagnosticsStore` path it reports under.
const EGUI_SPAN: &str = "lastlight/egui";
pub(super) const EGUI_GPU_PATH: &str = "render/lastlight/egui/elapsed_gpu";

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum EguiGpuSpan {
    Begin,
    End,
}
pub(super) fn add(render_app: &mut bevy::app::SubApp) {
    render_app
        .configure_sets(
            Core2d,
            (
                EguiGpuSpan::Begin
                    .after(Core2dSystems::MainPass)
                    .before(egui_pass),
                EguiGpuSpan::End.after(egui_pass).before(upscaling),
            ),
        )
        .add_systems(
            Core2d,
            (
                (ApplyDeferred, perf_egui_gpu_begin)
                    .chain()
                    .in_set(EguiGpuSpan::Begin),
                (ApplyDeferred, perf_egui_gpu_end)
                    .chain()
                    .in_set(EguiGpuSpan::End),
            ),
        );
}

/// Timestamps need both query features and a backend that writes them.
pub fn gpu_timing_support(features: WgpuFeatures, backends: Backends) -> Result<(), &'static str> {
    if !features.contains(WgpuFeatures::TIMESTAMP_QUERY) {
        Err("no TIMESTAMP_QUERY")
    } else if !features.contains(WgpuFeatures::TIMESTAMP_QUERY_INSIDE_ENCODERS) {
        Err("no TIMESTAMP_QUERY_INSIDE_ENCODERS")
    } else if !backends.intersects(Backends::VULKAN | Backends::DX12) {
        Err("backend without timestamps")
    } else {
        Ok(())
    }
}

/// Only the view egui draws into, as `egui_pass` itself filters.
fn egui_view(world: &World) -> bool {
    world
        .get_resource::<CurrentView>()
        .is_some_and(|view| world.get::<EguiCameraView>(view.0).is_some())
}
fn mark(world: &mut World, begin: bool) {
    if !egui_view(world) || !world.contains_resource::<DiagnosticsRecorder>() {
        return;
    }
    let mut encoder = world
        .resource::<RenderDevice>()
        .create_command_encoder(&default());
    let recorder = world.resource::<DiagnosticsRecorder>();
    if begin {
        recorder.begin_time_span(&mut encoder, EGUI_SPAN.into());
    } else {
        recorder.end_time_span(&mut encoder);
    }
    world
        .resource_mut::<PendingCommandBuffers>()
        .push_encoder(encoder, EGUI_SPAN);
}
fn perf_egui_gpu_begin(world: &mut World) {
    mark(world, true);
}
fn perf_egui_gpu_end(world: &mut World) {
    mark(world, false);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gpu_timing_support_reports_missing_timestamp_features() {
        let both = WgpuFeatures::TIMESTAMP_QUERY | WgpuFeatures::TIMESTAMP_QUERY_INSIDE_ENCODERS;
        assert_eq!(
            gpu_timing_support(WgpuFeatures::empty(), Backends::VULKAN),
            Err("no TIMESTAMP_QUERY")
        );
        assert_eq!(
            gpu_timing_support(WgpuFeatures::TIMESTAMP_QUERY, Backends::VULKAN),
            Err("no TIMESTAMP_QUERY_INSIDE_ENCODERS")
        );
        assert_eq!(
            gpu_timing_support(both, Backends::GL),
            Err("backend without timestamps")
        );
        assert_eq!(gpu_timing_support(both, Backends::VULKAN), Ok(()));
        assert_eq!(gpu_timing_support(both, Backends::DX12), Ok(()));
    }
}
