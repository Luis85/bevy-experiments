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
