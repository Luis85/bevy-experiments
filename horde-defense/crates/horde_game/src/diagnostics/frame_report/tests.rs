use super::*;
use crate::diagnostics::RenderSample;
use serde_json::Value;

/// Every key of the version-2 report, which v3 keeps with the same meaning.
const V2_KEYS: &[&str] = &[
    "version",
    "scene",
    "scope",
    "cpu",
    "adapter",
    "backend",
    "width_physical",
    "height_physical",
    "windows_scale",
    "ui_scale",
    "warmup_frames",
    "sample_frames",
    "requested_sample_frames",
    "completed_sample_target",
    "initial_battle",
    "peak_visible_enemies",
    "peak_corpses",
    "persistence",
    "mean_frame_ms",
    "median_frame_ms",
    "p95_frame_ms",
    "p99_frame_ms",
    "mean_observed_fps",
    "visible_enemies",
    "lane_count",
    "width_yards",
    "population_cap",
    "threat_rank",
    "visible_structures",
    "parallel_battles",
    "simulation_ticks",
    "paused",
    "window_focused",
    "unfocused_sample_frames",
    "frames_over_33ms",
    "frames_over_50ms",
    "frames_over_100ms",
    "frames_over_250ms",
    "work_scope",
    "work_mean_ms",
    "work_max_ms",
    "longest_frames",
];
const PHASES: &[&str] = &[
    "main_cpu_ms",
    "egui_pass_ms",
    "egui_tessellate_ms",
    "interface_total_ms",
    "extract_ms",
    "render_schedule_ms",
    "present_wait_ms",
    "render_world_cpu_ms",
    "cpu_frame_ms",
    "critical_path_ms",
];

fn frame(frame_ms: f64, main: f64, render: Option<RenderSample>) -> RecordedFrame {
    RecordedFrame {
        frame_ms,
        probe: FrameSample {
            main_cpu_ms: main,
            egui_pass_ms: 1.0,
            egui_tessellate_ms: 0.25,
            render,
            render_seq: 1,
            work_ms: [0.5, 0.1, 1.2],
            ..default()
        },
        ..default()
    }
}
fn render(extract: f64, schedule: f64, present_wait: f64) -> Option<RenderSample> {
    Some(RenderSample {
        extract_ms: extract,
        render_schedule_ms: schedule,
        present_wait_ms: present_wait,
        ..default()
    })
}
fn report_json(input: &ReportInput) -> Value {
    serde_json::to_value(build_report(input)).unwrap()
}

#[test]
fn tail_frame_quantiles_use_measured_intervals_without_discarding_jank() {
    let data = [16., 16., 16., 17., 100.];
    assert_eq!(quantile(&data, 0.5), 16.);
    assert_eq!(quantile(&data, 0.95), 100.);
    assert_eq!(quantile(&data, 0.99), 100.);
    assert_eq!(quantile(&[], 0.95), 0.);
}

#[test]
fn v3_keeps_every_v2_key_and_adds_phases() {
    let input = ReportInput {
        requested_sample_frames: 2,
        frames: vec![
            frame(16.7, 4.0, render(0.5, 3.0, 1.5)),
            frame(16.6, 5.0, render(0.5, 4.0, 2.0)),
        ],
        ..default()
    };
    let report = report_json(&input);
    assert_eq!(report["version"], 3);
    for key in V2_KEYS {
        assert!(report.get(key).is_some(), "v2 key {key} missing");
    }
    // v2 meanings: the Interface work slot and the frame-interval stats.
    assert_eq!(report["work_mean_ms"][2], 1.2);
    assert_eq!(report["sample_frames"], 2);
    for name in PHASES {
        for field in ["mean", "p50", "p95", "p99", "max"] {
            assert!(
                report["phases"][name][field].is_number(),
                "phases.{name}.{field}"
            );
        }
    }
    // render world = extract + Render schedule − present wait.
    assert_eq!(report["phases"]["render_world_cpu_ms"]["max"], 2.5);
    assert_eq!(report["phases"]["interface_total_ms"]["mean"], 1.25);
    for key in [
        "interface_breakdown_mean_ms",
        "egui",
        "scene_load",
        "present",
        "build",
        "gpu",
    ] {
        assert!(report[key].is_object(), "{key}");
    }
    assert!(report["longest_frames"][0]["phases"]["cpu_frame_ms"].is_number());
    assert_eq!(report["longest_frames"][0]["sample_index"], 0);
}

/// W1-64: `egui.texture_uploads` totals every sampled frame's
/// `textures_delta.set` entries and bytes, so a battle run after warmup can
/// be held to zero.
#[test]
fn egui_texture_uploads_total_the_sampled_frames() {
    let mut frames = vec![
        frame(16.7, 4.0, None),
        frame(16.7, 4.0, None),
        frame(16.7, 4.0, None),
    ];
    for (frame, (count, bytes)) in frames.iter_mut().zip([(2, 4096), (0, 0), (1, 64)]) {
        frame.probe.egui[EguiCount::TextureUploads as usize] = count;
        frame.probe.egui[EguiCount::TextureUploadBytes as usize] = bytes;
    }
    let report = report_json(&ReportInput {
        frames,
        ..default()
    });
    let uploads = &report["egui"]["texture_uploads"];
    assert_eq!(uploads["count"], 3);
    assert_eq!(uploads["bytes"], 4160);
    assert_eq!(uploads["frames"], 2);
    let quiet = report_json(&ReportInput {
        frames: vec![frame(16.7, 4.0, None)],
        ..default()
    });
    assert_eq!(quiet["egui"]["texture_uploads"]["count"], 0);
}

#[test]
fn cpu_frame_quantiles_are_per_sample_sums() {
    // Main and render world peak on alternate frames: the CPU frame never
    // exceeds 10 ms, while summing the parts' p99s would claim 18 ms.
    let main = [1.0, 9.0, 1.0, 9.0];
    let render_world = [9.0, 1.0, 9.0, 1.0];
    assert_eq!(cpu_frames(&main, &render_world), vec![10.0; 4]);
    let frames = main
        .iter()
        .zip(render_world)
        .map(|(&main, rw)| frame(16.7, main, render(0., rw, 0.)))
        .collect();
    let report = report_json(&ReportInput {
        frames,
        ..default()
    });
    let phases = &report["phases"];
    assert_eq!(phases["cpu_frame_ms"]["p99"], 10.0);
    assert_eq!(phases["main_cpu_ms"]["p99"], 9.0);
    assert_eq!(phases["render_world_cpu_ms"]["p99"], 9.0);
    // Not pipelined: the critical path is the same per-sample sum.
    assert_eq!(phases["critical_path_ms"]["max"], 10.0);
    let pipelined = report_json(&ReportInput {
        frames: vec![frame(16.7, 4.0, render(0., 6.0, 0.))],
        pipelined: true,
        ..default()
    });
    assert_eq!(pipelined["phases"]["critical_path_ms"]["max"], 6.0);
    assert_eq!(pipelined["phases"]["cpu_frame_ms"]["max"], 10.0);
}

#[test]
fn report_records_build_profile_and_features() {
    let labelled = report_json(&ReportInput {
        profile_label: Some("playtest".into()),
        ..default()
    });
    assert_eq!(labelled["build"]["profile_label"], "playtest");
    let features: Vec<&str> = labelled["build"]["features"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(features.contains(&"review"), cfg!(feature = "review"));
    assert_eq!(features.contains(&"dev"), cfg!(feature = "dev"));
    assert_eq!(
        labelled["build"]["debug_assertions"],
        cfg!(debug_assertions)
    );
    let unlabelled = report_json(&ReportInput::default());
    assert_eq!(unlabelled["build"]["profile_label"], "unlabelled");
}

#[test]
fn gpu_report_rejects_inconsistent_timestamps() {
    let mut frames = vec![frame(16.7, 4.0, None), frame(16.7, 4.0, None)];
    frames[0].gpu_egui_ms = Some(0.8);
    let supported = report_json(&ReportInput {
        frames: frames.clone(),
        gpu_support: Some(Ok(())),
        ..default()
    });
    assert_eq!(supported["gpu"]["supported"], true);
    assert_eq!(supported["gpu"]["egui_pass_ms"]["mean"], 0.8);
    for bad in [-0.1, 40.0] {
        frames[1].gpu_egui_ms = Some(bad);
        let report = report_json(&ReportInput {
            frames: frames.clone(),
            gpu_support: Some(Ok(())),
            ..default()
        });
        assert_eq!(report["gpu"]["supported"], false);
        assert_eq!(
            report["gpu"]["reason"],
            "unsupported: inconsistent timestamps"
        );
        assert!(report["gpu"]["egui_pass_ms"].is_null());
    }
    let missing = report_json(&ReportInput {
        gpu_support: Some(Err("no TIMESTAMP_QUERY")),
        ..default()
    });
    assert_eq!(missing["gpu"]["reason"], "no TIMESTAMP_QUERY");
    assert_eq!(missing["gpu"]["supported"], false);
}

/// W1-66: `scene_load` reports how many hostiles flashed and how many quads
/// their hit flashes drew, recorded through the probe like the other counts.
/// Written after the plumbing, so its mutation proofs stand in for a
/// fail-first (W1 review): the two fields swapped in `report_json`'s
/// `SceneLoad` fail the `flashing_enemies` max (300, expected 4), and
/// `flash_quads` dropped from `PerfProbe::record_scene` fails the
/// `flash_quads` max (0, expected 300).
#[test]
fn scene_load_reports_hit_flash_quads() {
    let probe = crate::diagnostics::PerfProbe::default();
    let mut frames = Vec::new();
    for (flashing_enemies, flash_quads) in [(2, 140), (0, 0), (4, 300)] {
        probe.record_scene(crate::diagnostics::SceneDraw {
            flashing_enemies,
            flash_quads,
            ..Default::default()
        });
        let mut recorded = frame(16.7, 4.0, None);
        recorded.probe.scene = probe.close_frame().scene;
        frames.push(recorded);
    }
    let report = report_json(&ReportInput {
        frames,
        ..default()
    });
    let scene = &report["scene_load"];
    assert_eq!(scene["flashing_enemies"]["max"], 4);
    assert_eq!(scene["flashing_enemies"]["mean"], 2.0);
    assert_eq!(scene["flash_quads"]["max"], 300);
    assert_eq!(scene["flash_quads"]["mean"], 440.0 / 3.0);
}

/// A frame on the probe's wall clock (ms since its epoch): Time<Real> closed
/// its interval at `time`, the main schedule ran `begin..begin + main`, and
/// the main thread spent `gap` before `begin` outside it, `handoff` of that
/// in the render hand-off (of which `wait` on the pipelined channel).
fn timed(time: f64, frame_ms: f64, begin: f64, main: f64, gap: [f64; 3]) -> RecordedFrame {
    let [gap, handoff, wait] = gap;
    let mut recorded = frame(frame_ms, main, None);
    recorded.time_ms = Some(time);
    recorded.probe.main_begin_ms = begin;
    recorded.probe.main_end_ms = begin + main;
    recorded.probe.gap_ms = gap;
    recorded.probe.handoff_ms = handoff;
    recorded.probe.channel_wait_ms = wait;
    recorded
}
fn close(actual: &Value, expected: f64) -> bool {
    actual
        .as_f64()
        .is_some_and(|actual| (actual - expected).abs() < 1e-9)
}

/// W1-68: a 500 ms frame whose spans hold 9 ms says where the rest went.
/// With the between-frames gap recorded, the frame's wall-clock window is
/// covered by main-thread spans: the previous frame's main schedule, the
/// pipelined channel wait and the rest of the hand-off, and the time outside
/// App::update. Without it, the report says how much no span covers.
#[test]
fn slow_frame_reports_unattributed_time() {
    let quick = timed(990., 16.7, 990., 8., [8., 7., 6.5]);
    // 998..1489 between frames: 487.5 ms channel wait, 0.5 ms extract and
    // send, 3 ms outside App::update; the frame interval is 990..1489.
    let mut slow = timed(1489., 499., 1489., 9., [491., 488., 487.5]);
    slow.probe.render = Some(RenderSample {
        extract_ms: 0.3,
        render_schedule_ms: 485.,
        present_wait_ms: 480.,
        begin_ms: 1000.,
        present_begin_ms: 1002.,
        end_ms: 1485.,
        idle_ms: 12.,
    });
    slow.probe.events = crate::diagnostics::FrameEvents {
        window: 3,
        image_assets: 1,
        audio_assets: 0,
        audio_voices: 2,
    };
    slow.probe.egui[EguiCount::TextureUploadBytes as usize] = 4096;
    let report = report_json(&ReportInput {
        frames: vec![quick.clone(), slow.clone()],
        ..default()
    });
    let longest = &report["longest_frames"][0];
    assert_eq!(longest["sample_index"], 1);
    assert!(close(&longest["time_ms"], 1489.));
    assert!(close(&longest["main_begin_ms"], 1489.));
    assert!(close(&longest["main_end_ms"], 1498.));
    let phases = &longest["phases"];
    assert!(close(&phases["gap_ms"], 491.));
    assert!(close(&phases["handoff_ms"], 488.));
    assert!(close(&phases["channel_wait_ms"], 487.5));
    assert!(close(&phases["outside_update_ms"], 3.));
    assert!(close(&phases["render_idle_ms"], 12.));
    let window = &longest["attribution"];
    assert!(close(&window["window_begin_ms"], 990.));
    assert!(close(&window["main_cpu_ms"], 8.));
    assert!(close(&window["channel_wait_ms"], 487.5));
    assert!(close(&window["handoff_other_ms"], 0.5));
    assert!(close(&window["outside_update_ms"], 3.));
    assert!(close(&window["unattributed_ms"], 0.));
    assert!(close(&window["coverage"], 1.));
    assert!(close(&window["render_present_ms"], 480.));
    assert!(close(&window["render_other_ms"], 5.));
    assert!(close(&window["render_unbusy_ms"], 14.));
    let events = &longest["events"];
    assert_eq!(events["window"], 3);
    assert_eq!(events["image_assets"], 1);
    assert_eq!(events["audio_voices"], 2);
    assert_eq!(events["texture_upload_bytes"], 4096);
    let summary = &report["slow_frames"];
    assert_eq!(summary["over_50ms"], 1);
    assert_eq!(summary["timed"], 1);
    assert_eq!(summary["covered_95"], 1);
    assert!(close(&summary["min_coverage"], 1.));
    assert_eq!(report["events"]["audio_voices"]["max"], 2);
    assert!(close(&report["phases"]["gap_ms"]["max"], 491.));

    // Today's report: the same frame with no gap or earlier frame recorded.
    // 499 ms of its window lie outside every span.
    let mut bare = slow;
    bare.probe.gap_ms = 0.;
    bare.probe.handoff_ms = 0.;
    bare.probe.channel_wait_ms = 0.;
    let report = report_json(&ReportInput {
        frames: vec![bare],
        ..default()
    });
    let window = &report["longest_frames"][0]["attribution"];
    assert!(close(&window["unattributed_ms"], 499.));
    assert!(close(&window["coverage"], 0.));
    assert_eq!(report["slow_frames"]["covered_95"], 0);
    // A frame without a wall-clock end is counted, never attributed.
    let report = report_json(&ReportInput {
        frames: vec![frame(500., 9., None)],
        ..default()
    });
    assert!(report["longest_frames"][0]["attribution"].is_null());
    assert_eq!(report["slow_frames"]["over_50ms"], 1);
    assert_eq!(report["slow_frames"]["timed"], 0);
    assert!(report["slow_frames"]["min_coverage"].is_null());
}

/// W1-68 review: `coverage` counts the `outside_update` residual, so the
/// main-thread timeline tiles every window and a stall outside every named
/// span still reads as covered. `named_coverage` leaves the residual out: a
/// 499 ms frame whose gap is 489.5 ms outside App::update is named only for
/// its main schedule, channel wait and hand-off.
#[test]
fn slow_frame_outside_every_named_span_has_low_named_coverage() {
    let quick = timed(990., 16.7, 990., 8., [8., 7., 6.5]);
    // 998..1489 between frames: 1 ms channel wait, 0.5 ms extract and send,
    // 489.5 ms outside App::update.
    let slow = timed(1489., 499., 1489., 9., [491., 1.5, 1.]);
    let report = report_json(&ReportInput {
        frames: vec![quick, slow],
        ..default()
    });
    let window = &report["longest_frames"][0]["attribution"];
    assert!(close(&window["outside_update_ms"], 489.5), "{window}");
    assert!(
        close(&window["coverage"], 1.),
        "the residual tiles: {window}"
    );
    assert!(
        close(&window["named_coverage"], 9.5 / 499.),
        "only 8 + 1 + 0.5 ms are named: {window}"
    );
    let summary = &report["slow_frames"];
    assert_eq!(summary["covered_95"], 1);
    assert_eq!(summary["covered_95_named"], 0, "{summary}");
    assert!(
        close(&summary["min_named_coverage"], 9.5 / 499.),
        "{summary}"
    );
    assert!(close(&summary["max_outside_update_ms"], 489.5), "{summary}");
}

/// A render frame on the probe's wall clock: the Render schedule ran
/// `begin..end`, and `prepare_windows` (the swap-chain acquire) waited
/// `present_wait` from `present_begin`.
fn render_span(begin: f64, present_begin: f64, present_wait: f64, end: f64) -> RenderSample {
    RenderSample {
        extract_ms: 0.3,
        render_schedule_ms: end - begin,
        present_wait_ms: present_wait,
        begin_ms: begin,
        present_begin_ms: present_begin,
        end_ms: end,
        idle_ms: 0.5,
    }
}
/// The gated counts of a report: frames over 50 ms outside driver stalls,
/// and the stalls excluded.
fn gated(report: &Value) -> (Value, Value) {
    (
        report["driver_stalls"]["outside"]["frames_over_50ms"].clone(),
        report["frames_over_50ms_driver_stall"].clone(),
    )
}

/// W1-61, user ruling on gate 3 (2026-11-06, "Exclude driver stalls"): a
/// frame over 50 ms whose swap-chain acquire wait holds at least 80% of it,
/// named to at least 95% and with under 50 ms of in-process work, is an
/// environmental driver stall. It stays in the v2 `frames_over_50ms`, is
/// counted and listed in `driver_stalls`, and is left out of the gated
/// counts and the stall-free interval statistics.
#[test]
fn driver_stalls_are_excluded_from_the_gated_frame_counts() {
    // Not pipelined: frame 1's window 1000..1500 is the main thread's render
    // hand-off 1000..1498 (extract, Render schedule), 480 ms of it in
    // prepare_windows; 9 ms main schedule and 17.3 ms render CPU.
    let quick = timed(1000., 16.7, 992., 8., [0.5, 0.4, 0.]);
    let mut stall = timed(1500., 500., 1500., 9., [500., 498., 0.]);
    stall.probe.render = Some(render_span(1000.5, 1001., 480., 1497.5));
    let report = report_json(&ReportInput {
        frames: vec![quick.clone(), stall.clone()],
        ..default()
    });
    assert_eq!(
        report["frames_over_50ms"], 1,
        "the v2 count keeps its meaning"
    );
    assert_eq!(
        gated(&report),
        (0.into(), 1.into()),
        "{}",
        report["driver_stalls"]
    );
    let listed = &report["driver_stalls"]["frames"];
    assert_eq!(listed.as_array().map(Vec::len), Some(1));
    assert_eq!(listed[0]["sample_index"], 1);
    assert!(close(&listed[0]["acquire_wait_ms"], 480.), "{listed}");
    assert!(close(&listed[0]["acquire_share"], 0.96), "{listed}");
    // No main schedule overlaps the window (992..1000 and 1500..1509 lie
    // outside it), so the stall sample's own 9 ms main schedule stands in:
    // in_process_ms is max(in-window, own) main + render CPU.
    assert!(close(&listed[0]["in_process_ms"], 26.3), "{listed}");
    assert!(report["driver_stalls"]["rule"].is_string());
    // The interval statistics outside the stall see only the 16.7 ms frame.
    let outside = &report["driver_stalls"]["outside"];
    assert_eq!(outside["sample_frames"], 1);
    assert!(close(&outside["p99_frame_ms"], 16.7), "{outside}");
    assert!(
        close(&outside["mean_observed_fps"], 1000. / 16.7),
        "{outside}"
    );
    assert_eq!(outside["frames_over_33ms"], 0);

    // A 500 ms main schedule is in-process work, never a stall.
    let mut busy = timed(1500., 500., 1000.4, 499.6, [0.4, 0.3, 0.]);
    busy.probe.render = Some(render_span(1000., 1000.1, 0.1, 1000.3));
    let report = report_json(&ReportInput {
        frames: vec![quick.clone(), busy],
        ..default()
    });
    assert_eq!(
        gated(&report),
        (1.into(), 0.into()),
        "{}",
        report["driver_stalls"]
    );

    // 60 ms with 40% in the acquire is below the 80% share.
    let mut partial = timed(1060., 60., 1060., 2., [60., 58., 0.]);
    partial.probe.render = Some(render_span(1000.2, 1001., 24., 1057.8));
    let report = report_json(&ReportInput {
        frames: vec![quick.clone(), partial],
        ..default()
    });
    assert_eq!(
        gated(&report),
        (1.into(), 0.into()),
        "{}",
        report["driver_stalls"]
    );

    // 480 ms of acquire, but 60 ms of render CPU besides: in-process work.
    let mut heavy = stall.clone();
    heavy.probe.render = Some(RenderSample {
        render_schedule_ms: 540.,
        ..render_span(1000.5, 1001., 480., 1497.5)
    });
    let report = report_json(&ReportInput {
        frames: vec![quick.clone(), heavy],
        ..default()
    });
    assert_eq!(
        gated(&report),
        (1.into(), 0.into()),
        "{}",
        report["driver_stalls"]
    );

    // A frame without a wall-clock window cannot be attributed, so it counts.
    let mut untimed = stall.clone();
    untimed.time_ms = None;
    let report = report_json(&ReportInput {
        frames: vec![quick, untimed],
        ..default()
    });
    assert_eq!(
        gated(&report),
        (1.into(), 0.into()),
        "{}",
        report["driver_stalls"]
    );
}

/// W1-61 review: a frame's window `time_ms - frame_ms .. time_ms` closes at
/// First, so the sample's own First..Last runs after it and the main
/// schedule inside the stalled frame is the previous sample's. The ruling's
/// in-process limit reads the main CPU that overlaps the window (or the
/// sample's own, whichever is larger).
#[test]
fn driver_stall_in_process_work_reads_the_main_schedule_inside_the_window() {
    // Not pipelined: window 1000..1500. The previous sample's main schedule
    // runs 1000..1000 + main inside it, then the hand-off to 1498 holds a
    // 420 ms prepare_windows (share 0.84) and 7.3 ms of render CPU; the
    // stalled sample's own main schedule (1500..1508) is outside the window.
    let frames = |main: f64| {
        let previous = timed(1000., 16.7, 1000., main, [0.5, 0.4, 0.]);
        let gap = 500. - main;
        let mut stall = timed(1500., 500., 1500., 8., [gap, gap - 2., 0.]);
        stall.probe.render = Some(RenderSample {
            render_schedule_ms: 427.,
            ..render_span(1000. + main + 0.5, 1072., 420., 1497.5)
        });
        report_json(&ReportInput {
            frames: vec![previous, stall],
            ..default()
        })
    };
    // 70 ms of main schedule inside the window: in-process work, gated.
    let report = frames(70.);
    let window = &report["longest_frames"][0]["attribution"];
    assert!(close(&window["main_cpu_ms"], 70.), "{window}");
    assert!(close(&window["acquire_wait_ms"], 420.), "{window}");
    assert!(
        window["named_coverage"].as_f64().unwrap() >= 0.95,
        "{window}"
    );
    assert_eq!(
        gated(&report),
        (1.into(), 0.into()),
        "{}",
        report["driver_stalls"]
    );
    // 30 ms inside the window stays a stall, listed with the in-window main.
    let report = frames(30.);
    assert_eq!(
        gated(&report),
        (0.into(), 1.into()),
        "{}",
        report["driver_stalls"]
    );
    let listed = &report["driver_stalls"]["frames"][0];
    assert!(close(&listed["in_process_ms"], 30. + 7.3), "{listed}");
}

/// W1-61 review: the named-coverage and acquire-share legs of the rule each
/// decide on their own. A frame that qualifies on acquire share and in-process
/// work but whose window is 10% outside App::update stays gated; the share
/// threshold is probed on both sides of 0.8.
#[test]
fn driver_stall_needs_named_coverage_and_the_acquire_share() {
    let quick = timed(1000., 16.7, 992., 8., [0.5, 0.4, 0.]);
    // Window 1000..1500: hand-off 1000..1450 holding a 440 ms acquire (share
    // 0.88), then 50 ms outside App::update: named coverage 0.90.
    let mut unnamed = timed(1500., 500., 1500., 9., [500., 450., 0.]);
    unnamed.probe.render = Some(render_span(1000.5, 1001., 440., 1449.5));
    let report = report_json(&ReportInput {
        frames: vec![quick.clone(), unnamed],
        ..default()
    });
    let window = &report["longest_frames"][0]["attribution"];
    assert!(close(&window["named_coverage"], 0.9), "{window}");
    assert!(close(&window["acquire_wait_ms"], 440.), "{window}");
    assert_eq!(
        gated(&report),
        (1.into(), 0.into()),
        "{}",
        report["driver_stalls"]
    );
    // Fully named hand-off 1000..1498 with 8.3 ms of render CPU: an acquire
    // share of 0.79 is gated, 0.81 is a stall.
    for (acquire, expected) in [(395., (1, 0)), (405., (0, 1))] {
        let mut stall = timed(1500., 500., 1500., 9., [500., 498., 0.]);
        stall.probe.render = Some(RenderSample {
            render_schedule_ms: acquire + 8.,
            ..render_span(1000.5, 1001., acquire, 1497.5)
        });
        let report = report_json(&ReportInput {
            frames: vec![quick.clone(), stall],
            ..default()
        });
        assert_eq!(
            gated(&report),
            (expected.0.into(), expected.1.into()),
            "acquire {acquire}: {}",
            report["driver_stalls"]
        );
    }
}

/// W1-61 review: the frame-length leg of the rule. Under VSync-on Fifo an
/// ordinary frame can spend most of its window in `prepare_windows`; a frame
/// of 50 ms or less is never a driver stall, so it stays in the stall-free
/// interval statistics the pipelined decision reads.
#[test]
fn driver_stall_needs_a_frame_over_50ms() {
    // Not pipelined: the window opens at `start` (after the previous
    // sample's main schedule, `previous_main` long), then the main thread's
    // render hand-off fills it but for 0.2 ms outside App::update, and
    // prepare_windows holds 90% of the frame. 1 ms main schedule and 1.3 ms
    // render CPU: only the frame length keeps it from being a stall.
    let acquire_bound = |start: f64, frame_ms: f64, previous_main: f64| {
        let time = start + frame_ms;
        let gap = frame_ms - previous_main;
        let mut frame = timed(time, frame_ms, time, 1., [gap, gap - 0.2, 0.]);
        let begin = start + previous_main + 0.1;
        let acquire = 0.9 * frame_ms;
        frame.probe.render = Some(RenderSample {
            render_schedule_ms: acquire + 1.,
            ..render_span(begin, begin + 0.1, acquire, time - 0.3)
        });
        frame
    };
    let quick = timed(1000., 16.7, 992., 8., [0.5, 0.4, 0.]);
    let fifo = acquire_bound(1000., 16.7, 0.);
    let slow = acquire_bound(fifo.time_ms.unwrap(), 45., 1.);
    let report = report_json(&ReportInput {
        frames: vec![quick.clone(), fifo, slow],
        ..default()
    });
    // The 45 ms frame meets every other leg of the rule.
    let window = report["longest_frames"]
        .as_array()
        .and_then(|frames| frames.iter().find(|f| f["sample_index"] == 2))
        .map(|f| &f["attribution"])
        .expect("the 45 ms frame is among the longest");
    assert!(close(&window["acquire_wait_ms"], 40.5), "{window}");
    assert!(
        window["named_coverage"].as_f64().unwrap() >= 0.95,
        "{window}"
    );
    let stalls = &report["driver_stalls"];
    assert_eq!(
        stalls["frames"].as_array().map(Vec::len),
        Some(0),
        "{stalls}"
    );
    assert_eq!(report["frames_over_50ms_driver_stall"], 0);
    assert_eq!(stalls["outside"]["sample_frames"], 3, "{stalls}");
    assert_eq!(stalls["outside"]["frames_over_33ms"], 1, "{stalls}");

    // The boundary: exactly 50 ms is not over it, 50.1 ms is a stall.
    for (frame_ms, stalled) in [(50., 0), (50.1, 1)] {
        let report = report_json(&ReportInput {
            frames: vec![quick.clone(), acquire_bound(1000., frame_ms, 0.)],
            ..default()
        });
        let stalls = &report["driver_stalls"];
        assert_eq!(
            stalls["frames"].as_array().map(Vec::len),
            Some(stalled),
            "{frame_ms} ms: {stalls}"
        );
        assert_eq!(report["frames_over_50ms_driver_stall"], stalled);
    }
}

/// Pipelined, the main thread waits on the render channel while the render
/// thread sits in `prepare_windows`: the acquire wait is the channel wait
/// that overlaps the render thread's present. A channel wait with the render
/// thread anywhere else is not a driver stall.
#[test]
fn pipelined_channel_wait_is_a_stall_only_while_the_render_thread_presents() {
    let quick = timed(990., 16.7, 990., 8., [8., 7., 6.5]);
    // 998..1485.5 channel wait; the render thread presents 1002..1482.
    let mut stall = timed(1489., 499., 1489., 9., [491., 488., 487.5]);
    stall.probe.render = Some(render_span(1000., 1002., 480., 1485.));
    let report = report_json(&ReportInput {
        frames: vec![quick.clone(), stall.clone()],
        pipelined: true,
        ..default()
    });
    assert_eq!(
        gated(&report),
        (0.into(), 1.into()),
        "{}",
        report["driver_stalls"]
    );
    assert!(close(
        &report["driver_stalls"]["frames"][0]["acquire_wait_ms"],
        480.
    ));

    // The same channel wait, the render thread busy outside prepare_windows.
    let mut elsewhere = stall;
    elsewhere.probe.render = Some(render_span(1000., 1484., 0.2, 1485.));
    let report = report_json(&ReportInput {
        frames: vec![quick, elsewhere],
        pipelined: true,
        ..default()
    });
    assert_eq!(
        gated(&report),
        (1.into(), 0.into()),
        "{}",
        report["driver_stalls"]
    );
}

/// W1-61: pipelined, the render frame that stalled in `prepare_windows`
/// during a channel wait finished and was replaced before the close, so
/// `render` holds the next, quick one and only `slowest_present` holds the
/// stall. It is attributed and excluded all the same.
#[test]
fn a_replaced_render_frame_still_attributes_the_channel_wait() {
    let quick = timed(990., 16.7, 990., 8., [8., 7., 6.5]);
    let mut stall = timed(1489., 499., 1489., 9., [491., 488., 487.5]);
    stall.probe.render = Some(render_span(1486., 1486.2, 0.1, 1488.));
    let report = report_json(&ReportInput {
        frames: vec![quick.clone(), stall.clone()],
        pipelined: true,
        ..default()
    });
    assert_eq!(
        gated(&report),
        (1.into(), 0.into()),
        "{}",
        report["driver_stalls"]
    );
    stall.probe.slowest_present = Some(render_span(1000., 1002., 480., 1485.));
    let report = report_json(&ReportInput {
        frames: vec![quick, stall],
        pipelined: true,
        ..default()
    });
    assert_eq!(
        gated(&report),
        (0.into(), 1.into()),
        "{}",
        report["driver_stalls"]
    );
    let window = &report["longest_frames"][0]["attribution"];
    assert!(close(&window["render_present_ms"], 480.1), "{window}");
    assert!(close(&window["acquire_wait_ms"], 480.), "{window}");
}
