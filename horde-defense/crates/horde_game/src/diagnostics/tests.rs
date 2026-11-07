use super::*;
use std::ffi::OsString;

fn config(vars: &[(&str, &str)]) -> LaunchConfig {
    let vars: Vec<(String, OsString)> = vars
        .iter()
        .map(|(k, v)| ((*k).to_owned(), OsString::from(v)))
        .collect();
    LaunchConfig::parse(std::iter::empty(), |name| {
        vars.iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.clone())
    })
    .unwrap()
}
fn installed(config: &LaunchConfig) -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    install(&mut app, config);
    app
}
fn schedules_have_no_perf_systems(app: &App) -> bool {
    let schedules = app.world().resource::<Schedules>();
    schedules.iter().all(|(_, schedule)| {
        schedule
            .graph()
            .systems
            .iter()
            .all(|(_, system, _)| !system.name().to_string().contains("diagnostics"))
    })
}

#[test]
fn normal_play_installs_no_perf_probe() {
    let app = installed(&config(&[]));
    assert!(!app.world().contains_resource::<PerfProbe>());
    assert!(!app.world().contains_resource::<PerfFrame>());
    assert!(schedules_have_no_perf_systems(&app));
    // Without a probe every timer is a None check: nothing is recorded.
    assert!(work_timer(None, WorkPhase::Interface).is_none());
    assert!(interface_span(None, InterfaceSpan::Backdrop).is_none());
}

#[test]
fn review_launch_without_report_installs_no_probe() {
    let review = config(&[
        ("LASTLIGHT_REVIEW_SCREEN", "peak"),
        ("LASTLIGHT_PERF_FRAMES", "600"),
        ("LASTLIGHT_PERF_CPU", "test"),
    ]);
    assert_eq!(review.review().is_some(), cfg!(feature = "review"));
    let app = installed(&review);
    assert!(!app.world().contains_resource::<PerfProbe>());
    assert!(schedules_have_no_perf_systems(&app));
}

#[cfg(feature = "review")]
fn perf_app() -> App {
    let mut app = installed(&config(&[("LASTLIGHT_PERF_REPORT", "perf.json")]));
    app.add_systems(Update, |probe: Option<Res<PerfProbe>>| {
        let _work = work_timer(probe.as_deref(), WorkPhase::Simulation);
        std::thread::sleep(std::time::Duration::from_millis(2));
    });
    app
}

#[cfg(feature = "review")]
#[test]
fn perf_launch_installs_probe_and_records() {
    // The system scan the normal-play tests rely on does see the probe's systems.
    let probe_only = installed(&config(&[("LASTLIGHT_PERF_REPORT", "perf.json")]));
    assert!(!schedules_have_no_perf_systems(&probe_only));
    let mut app = perf_app();
    assert!(app.is_plugin_added::<PerfPlugin>());
    assert!(app.world().contains_resource::<PerfProbe>());
    app.update();
    let frame = app.world().resource::<PerfFrame>().0.clone().unwrap();
    assert!(frame.main_cpu_ms >= 2.0, "{frame:?}");
    assert!(
        frame.work_ms[WorkPhase::Simulation as usize] >= 2.0,
        "{frame:?}"
    );
    // No RenderApp under MinimalPlugins: no render sample, and no panic.
    assert_eq!(frame.render, None);
    // The frame was taken: the next frame starts from zero.
    let probe = app.world().resource::<PerfProbe>().clone();
    assert_eq!(probe.close_frame().work_ms, [0.; 3]);
}

#[cfg(feature = "review")]
#[test]
fn interface_work_is_attributed_to_the_same_frame() {
    // ui::draw runs in PostUpdate (EguiPostUpdateSet::EndPass). Its work
    // must be closed with the frame that did it, not the next one.
    let mut app = perf_app();
    app.add_systems(
        PostUpdate,
        |probe: Option<Res<PerfProbe>>, mut frame: Local<u32>| {
            *frame += 1;
            if *frame == 2 {
                probe.as_deref().unwrap().count_egui_pass();
                let _work = work_timer(probe.as_deref(), WorkPhase::Interface);
                std::thread::sleep(std::time::Duration::from_millis(3));
            }
        },
    );
    let mut interface = Vec::new();
    let mut passes = Vec::new();
    for _ in 0..3 {
        app.update();
        let frame = app.world().resource::<PerfFrame>().0.clone().unwrap();
        interface.push(frame.work_ms[WorkPhase::Interface as usize]);
        passes.push(frame.egui[EguiCount::Passes as usize]);
    }
    assert_eq!(interface[0], 0.);
    assert!(interface[1] >= 3.0, "{interface:?}");
    assert_eq!(interface[2], 0.);
    assert_eq!(passes, [0, 1, 0]);
}

/// W1-68: the probe records the main thread between frames (the gap from the
/// previous end of `Last` to First), wall-clock instants, and the frame's
/// window messages and spawned audio voices.
#[cfg(feature = "review")]
#[test]
fn perf_launch_records_the_between_frames_gap_and_events() {
    use bevy::window::{AppLifecycle, WindowEvent};
    let mut app = perf_app();
    app.add_message::<WindowEvent>();
    app.update();
    let first = app.world().resource::<PerfFrame>().0.clone().unwrap();
    assert_eq!(first.gap_ms, 0., "no earlier frame: {first:?}");
    assert!(first.main_end_ms > first.main_begin_ms, "{first:?}");
    std::thread::sleep(std::time::Duration::from_millis(5));
    app.world_mut()
        .write_message(WindowEvent::AppLifecycle(AppLifecycle::Running));
    app.world_mut()
        .write_message(WindowEvent::AppLifecycle(AppLifecycle::Running));
    app.world_mut()
        .spawn(AudioPlayer::<AudioSource>(Handle::default()));
    app.update();
    let second = app.world().resource::<PerfFrame>().0.clone().unwrap();
    assert!(second.gap_ms >= 5.0, "{second:?}");
    let between = second.main_begin_ms - first.main_end_ms;
    assert!((second.gap_ms - between).abs() < 1e-6, "{second:?}");
    // No RenderApp: no hand-off ran in the gap.
    assert_eq!((second.handoff_ms, second.channel_wait_ms), (0., 0.));
    assert_eq!(
        second.events,
        FrameEvents {
            window: 2,
            audio_voices: 1,
            ..default()
        }
    );
    app.update();
    let third = app.world().resource::<PerfFrame>().0.clone().unwrap();
    assert_eq!(third.events, FrameEvents::default());
}

#[cfg(feature = "review")]
fn sleep_ms(ms: u64) {
    std::thread::sleep(std::time::Duration::from_millis(ms));
}

/// W1-68 review: pipelined, the hand-off is the `RenderExtractApp` extract
/// (here a 5 ms wait, then the `RenderApp` extract of 3 ms), and the channel
/// wait is that minus the inner extract. As in `PipelinedRenderingPlugin`,
/// the `RenderApp` leaves the app and only the hand-off runs its extract.
#[cfg(feature = "review")]
#[test]
fn pipelined_handoff_holds_the_channel_wait_and_the_extract() {
    use bevy::app::SubApp;
    use bevy::render::{RenderApp, pipelined_rendering::RenderExtractApp};
    use std::sync::{Arc, Mutex};
    let mut app = perf_app();
    let mut render = SubApp::new();
    render.set_extract(|_, _| sleep_ms(3));
    app.insert_sub_app(RenderApp, render);
    let channel: Arc<Mutex<Option<SubApp>>> = Arc::default();
    let receive = channel.clone();
    let mut handoff = SubApp::new();
    handoff.set_extract(move |main, _| {
        sleep_ms(5);
        if let Some(render) = receive.lock().unwrap().as_mut() {
            render.extract(main);
        }
    });
    app.insert_sub_app(RenderExtractApp, handoff);
    app.finish();
    let probe = app.world().resource::<PerfProbe>().clone();
    assert!(probe.pipelined());
    *channel.lock().unwrap() = app.remove_sub_app(RenderApp);
    app.update();
    app.update();
    let frame = app.world().resource::<PerfFrame>().0.clone().unwrap();
    assert!(frame.handoff_ms >= 8.0, "{frame:?}");
    assert!(frame.channel_wait_ms >= 5.0, "{frame:?}");
    assert!(
        frame.channel_wait_ms <= frame.handoff_ms - 3.0,
        "the extract is not channel wait: {frame:?}"
    );
    assert!(frame.gap_ms >= frame.handoff_ms, "{frame:?}");
}

/// W1-68 review: not pipelined, the hand-off is the `RenderApp` extract
/// (3 ms) plus the Render schedule (4 ms), both on the main thread before
/// the next First, and there is no channel wait.
#[cfg(feature = "review")]
#[test]
fn unpipelined_handoff_holds_the_extract_and_the_render_schedule() {
    use bevy::app::SubApp;
    use bevy::ecs::schedule::ScheduleLabel;
    use bevy::render::{Render, RenderApp, RenderSystems};
    let mut app = perf_app();
    let probe = app.world().resource::<PerfProbe>().clone();
    let mut render = SubApp::new();
    render.insert_resource(probe.clone());
    render.update_schedule = Some(Render.intern());
    render.configure_sets(
        Render,
        (
            RenderSystems::ExtractCommands,
            RenderSystems::PrepareViews,
            RenderSystems::PostCleanup,
        )
            .chain(),
    );
    render.add_systems(
        Render,
        (|| sleep_ms(4))
            .after(RenderSystems::ExtractCommands)
            .before(RenderSystems::PostCleanup),
    );
    render_probe::add(&mut render);
    render.set_extract(|_, _| sleep_ms(3));
    app.insert_sub_app(RenderApp, render);
    app.finish();
    assert!(!probe.pipelined());
    app.update();
    app.update();
    let frame = app.world().resource::<PerfFrame>().0.clone().unwrap();
    assert!(frame.handoff_ms >= 7.0, "{frame:?}");
    assert_eq!(frame.channel_wait_ms, 0., "{frame:?}");
    assert!(frame.gap_ms >= frame.handoff_ms, "{frame:?}");
    let sample = frame.render.expect("the Render schedule ran");
    assert!(sample.extract_ms >= 3.0, "{sample:?}");
    assert!(sample.render_schedule_ms >= 4.0, "{sample:?}");
}

/// W1-61: a render frame stalled in `prepare_windows` that completes and is
/// replaced before the next close is still handed to that close, as the
/// longest present wait since the previous one.
#[test]
fn close_keeps_the_slowest_present_since_the_previous_close() {
    let probe = PerfProbe::default();
    let sample = |present_wait_ms: f64, end_ms: f64| RenderSample {
        present_wait_ms,
        end_ms,
        ..Default::default()
    };
    probe.publish_render(sample(480., 10.));
    probe.publish_render(sample(0.1, 12.));
    let closed = probe.close_frame();
    assert_eq!(closed.render, Some(sample(0.1, 12.)));
    assert_eq!(closed.render_seq, 2);
    assert_eq!(closed.slowest_present, Some(sample(480., 10.)));
    // The next close sees only what completed after this one.
    probe.publish_render(sample(0.2, 14.));
    let closed = probe.close_frame();
    assert_eq!(closed.slowest_present, Some(sample(0.2, 14.)));
    let idle = probe.close_frame();
    assert_eq!(idle.render, Some(sample(0.2, 14.)));
    assert_eq!(idle.slowest_present, None);
}

/// W1-33 review: egui_pass_ms opens only when bevy_egui's exclusive pass loop
/// can start, so a PostUpdate system running beside the probe (here 8 ms,
/// unordered against `EguiPostUpdateSet`) is never counted as egui work, in
/// either registration order, while work inside `EndPass` (5 ms) is.
#[cfg(feature = "review")]
#[test]
fn egui_pass_excludes_unrelated_post_update_systems() {
    use bevy_egui::{EguiPostUpdateSet, end_pass_system, run_egui_context_pass_loop_system};
    for sleeper_first in [true, false] {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        let sleeper = || sleep_ms(8);
        if sleeper_first {
            app.add_systems(PostUpdate, sleeper);
        }
        install(&mut app, &config(&[("LASTLIGHT_PERF_REPORT", "perf.json")]));
        app.configure_sets(
            PostUpdate,
            (
                EguiPostUpdateSet::EndPass,
                EguiPostUpdateSet::ProcessOutput,
                EguiPostUpdateSet::PostProcessOutput,
            )
                .chain(),
        );
        // As bevy_egui 0.43.1 registers them (lib.rs:1146-1150).
        app.add_systems(
            PostUpdate,
            (run_egui_context_pass_loop_system, end_pass_system)
                .chain()
                .in_set(EguiPostUpdateSet::EndPass),
        );
        // 5 ms of egui work (or a descheduling) inside the span. Exclusive,
        // so the sleeper never runs beside it.
        app.add_systems(
            PostUpdate,
            (|_: &mut World| sleep_ms(5))
                .in_set(EguiPostUpdateSet::EndPass)
                .after(run_egui_context_pass_loop_system)
                .before(end_pass_system),
        );
        if !sleeper_first {
            app.add_systems(PostUpdate, sleeper);
        }
        for _ in 0..3 {
            app.update();
            let frame = app.world().resource::<PerfFrame>().0.clone().unwrap();
            // Relative bounds, not absolute ones: a descheduling inside the
            // span inflates the span and the frame alike (W1 review).
            assert!(
                frame.egui_pass_ms >= 5.0,
                "sleeper first {sleeper_first}: {frame:?}"
            );
            assert!(
                frame.egui_pass_ms + 8.0 <= frame.main_cpu_ms,
                "sleeper first {sleeper_first}: the 8 ms sleeper is outside the span: {frame:?}"
            );
        }
    }
}

/// W1-33 review: present_wait_ms spans only `prepare_windows`. Surface
/// (re)configuration in `create_surfaces`, which bevy_render 0.20.1 orders
/// only before `prepare_windows`, and the `PrepareViews` work ordered before
/// it (here a 6 ms stand-in for `clear_view_attachments`) stay render-world
/// CPU, while a 4 ms wait inside the span is present wait. The real systems
/// are registered with false conditions: their render resources do not exist
/// here.
#[cfg(feature = "review")]
#[test]
fn present_wait_excludes_surface_configuration() {
    use bevy::app::SubApp;
    use bevy::ecs::schedule::ScheduleLabel;
    use bevy::render::{
        Render, RenderApp, RenderSystems,
        view::window::{create_surfaces, prepare_windows},
    };
    let mut app = perf_app();
    let probe = app.world().resource::<PerfProbe>().clone();
    let mut render = SubApp::new();
    render.insert_resource(probe.clone());
    render.update_schedule = Some(Render.intern());
    render.configure_sets(
        Render,
        (
            RenderSystems::ExtractCommands,
            RenderSystems::PrepareViews,
            RenderSystems::PostCleanup,
        )
            .chain(),
    );
    render.add_systems(
        Render,
        (
            (|| sleep_ms(6))
                .in_set(RenderSystems::PrepareViews)
                .before(create_surfaces),
            create_surfaces.run_if(|| false).before(prepare_windows),
            // Its condition runs inside the span, after the begin mark, and
            // stands in for a 4 ms swap-chain acquire (or a descheduling).
            prepare_windows
                .run_if(|| {
                    sleep_ms(4);
                    false
                })
                .in_set(RenderSystems::PrepareViews),
        ),
    );
    render_probe::add(&mut render);
    app.insert_sub_app(RenderApp, render);
    app.finish();
    // A render sample is published after the frame that closes it.
    app.update();
    for _ in 0..3 {
        app.update();
        let frame = app.world().resource::<PerfFrame>().0.clone().unwrap();
        let sample = frame.render.expect("the Render schedule ran");
        // Relative bounds, not absolute ones: a descheduling inside the span
        // inflates the span and the Render schedule alike (W1 review).
        assert!(sample.present_wait_ms >= 4.0, "{sample:?}");
        assert!(
            sample.present_wait_ms + 6.0 <= sample.render_schedule_ms,
            "the 6 ms PrepareViews work is outside the span: {sample:?}"
        );
    }
}
