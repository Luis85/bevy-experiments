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
