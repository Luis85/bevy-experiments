//! Review-only render-world spans (bevy =0.20.0, lock 0.20.1):
//! - `extract_ms` wraps the `RenderApp` extract function installed by
//!   `bevy_render` (`SubApp::take_extract`/`set_extract`, bevy_app sub_app.rs
//!   :174, :207). It runs after the pipelined channel receive, so it times
//!   extraction only, never the wait for the render thread;
//! - `render_schedule_ms` runs from before `RenderSystems::ExtractCommands`
//!   to after `RenderSystems::PostCleanup`;
//! - `present_wait_ms` spans `prepare_windows` in `PrepareViews`, where
//!   `get_current_texture` blocks on the swap chain (view/window/mod.rs:239-258).
//!
//! The end mark publishes one `RenderSample` for the main world to read.
use super::{Mark, PerfProbe, RenderSample, elapsed_ns, ms};
use bevy::{
    app::SubApp,
    prelude::*,
    render::{Render, RenderApp, RenderSystems, view::window::prepare_windows},
};
use std::{sync::atomic::Ordering, time::Instant};

pub(super) fn add(render_app: &mut SubApp) {
    render_app.add_systems(
        Render,
        (
            begin_render.before(RenderSystems::ExtractCommands),
            begin_present_wait
                .in_set(RenderSystems::PrepareViews)
                .before(prepare_windows),
            end_present_wait
                .in_set(RenderSystems::PrepareViews)
                .after(prepare_windows),
            end_render.after(RenderSystems::PostCleanup),
        ),
    );
}

/// Wraps the render extract function; a missing `RenderApp` is tolerated.
pub(super) fn wrap_extract(app: &mut App, probe: PerfProbe) {
    let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
        return;
    };
    let mut extract = render_app.take_extract();
    render_app.set_extract(move |main, render| {
        let start = Instant::now();
        if let Some(extract) = extract.as_mut() {
            extract(main, render);
        }
        probe
            .0
            .extract_ns
            .store(elapsed_ns(start), Ordering::Relaxed);
    });
}

fn begin_render(probe: Res<PerfProbe>) {
    probe.mark(Mark::Render);
    probe.0.present_wait_ns.store(0, Ordering::Relaxed);
}
fn begin_present_wait(probe: Res<PerfProbe>) {
    probe.mark(Mark::PresentWait);
}
fn end_present_wait(probe: Res<PerfProbe>) {
    probe
        .0
        .present_wait_ns
        .store(probe.since(Mark::PresentWait), Ordering::Relaxed);
}
fn end_render(probe: Res<PerfProbe>) {
    let sample = RenderSample {
        extract_ms: ms(probe.0.extract_ns.load(Ordering::Relaxed)),
        render_schedule_ms: ms(probe.since(Mark::Render)),
        present_wait_ms: ms(probe.0.present_wait_ns.load(Ordering::Relaxed)),
    };
    if let Ok(mut latest) = probe.0.render.lock() {
        *latest = (latest.0 + 1, Some(sample));
    }
}
