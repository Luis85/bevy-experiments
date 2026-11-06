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
//! The end mark publishes one `RenderSample` for the main world to read, with
//! its wall-clock begin, present begin and end and the render-thread idle
//! before it (W1-68). The main-thread hand-off after `Last` (W1-68) is timed
//! here too: pipelined, a second wrapper around the `RenderExtractApp`
//! extract (pipelined_rendering.rs `renderer_extract`: channel receive, the
//! `RenderApp` extract, send) whose time minus the inner extract is the
//! channel wait; otherwise the `RenderApp` extract begin to the end mark.
use super::{Mark, PerfProbe, RenderSample, elapsed_ns, ms};
use bevy::{
    app::SubApp,
    prelude::*,
    render::{
        Render, RenderApp, RenderSystems, pipelined_rendering::RenderExtractApp,
        view::window::prepare_windows,
    },
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

/// Wraps the render extract function, and when pipelined the
/// `RenderExtractApp` hand-off around it; a missing `RenderApp` is tolerated.
pub(super) fn wrap_extract(app: &mut App, probe: PerfProbe) {
    let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
        return;
    };
    let mut extract = render_app.take_extract();
    let inner = probe.clone();
    render_app.set_extract(move |main, render| {
        inner.mark(Mark::Extract);
        let start = Instant::now();
        if let Some(extract) = extract.as_mut() {
            extract(main, render);
        }
        inner
            .0
            .extract_ns
            .store(elapsed_ns(start), Ordering::Relaxed);
    });
    let Some(handoff_app) = app.get_sub_app_mut(RenderExtractApp) else {
        return;
    };
    let mut handoff = handoff_app.take_extract();
    handoff_app.set_extract(move |main, world| {
        let start = Instant::now();
        if let Some(handoff) = handoff.as_mut() {
            handoff(main, world);
        }
        let total = elapsed_ns(start);
        let extract = probe.0.extract_ns.load(Ordering::Relaxed);
        probe.0.handoff_ns.fetch_add(total, Ordering::Relaxed);
        probe
            .0
            .channel_wait_ns
            .fetch_add(total.saturating_sub(extract), Ordering::Relaxed);
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
    let end = probe.now_ns();
    let begin = probe.mark_ns(Mark::Render);
    let previous_end = probe.0.last_render_end_ns.swap(end, Ordering::Relaxed);
    if !probe.pipelined() {
        // Not pipelined: extract and Render ran on the main thread, in the gap
        // before the next First.
        let extract = probe.mark_ns(Mark::Extract);
        if extract != 0 {
            probe
                .0
                .handoff_ns
                .fetch_add(end.saturating_sub(extract), Ordering::Relaxed);
        }
    }
    let sample = RenderSample {
        extract_ms: ms(probe.0.extract_ns.load(Ordering::Relaxed)),
        render_schedule_ms: ms(end.saturating_sub(begin)),
        present_wait_ms: ms(probe.0.present_wait_ns.load(Ordering::Relaxed)),
        begin_ms: ms(begin),
        present_begin_ms: ms(probe.mark_ns(Mark::PresentWait)),
        end_ms: ms(end),
        idle_ms: if previous_end == 0 {
            0.
        } else {
            ms(begin.saturating_sub(previous_end))
        },
    };
    if let Ok(mut latest) = probe.0.render.lock() {
        *latest = (latest.0 + 1, Some(sample));
    }
}
