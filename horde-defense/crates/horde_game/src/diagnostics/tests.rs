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
