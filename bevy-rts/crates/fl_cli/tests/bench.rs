//! The bench harness: sizes, median, regression check, instrumentation neutrality.

use fl_cli::{
    BenchConfig, BenchRow, CompareError, compare, exclusive, median, profile_json, render_markdown,
    rows_from_json, run_bench, run_bench_full,
};
use fl_observe::{DebugConfig, init_headless};
use fl_protocol::Tick;
use fl_sim::{MatchSetup, RunConfig, ScaleScenario, build_sim_app, load_content, step_tick};
use fl_view::TickInput;

fn cfg(units: &[u32], ticks: u32) -> BenchConfig {
    BenchConfig {
        units: units.to_vec(),
        ticks,
        warmup: 5,
        seed: 1,
        repeat: 1,
        flat: false,
    }
}

fn row(units: u32, mean_ms: f64) -> BenchRow {
    BenchRow {
        units,
        map: "open".to_owned(),
        entities: units as usize,
        mean_ms,
        p95_ms: mean_ms,
        max_ms: mean_ms,
        per_phase_mean_ms: Vec::new(),
        counters_per_tick: Vec::new(),
    }
}

#[test]
fn bench_reports_each_size() {
    let rows = run_bench(&cfg(&[50, 100], 20));
    assert_eq!(rows.len(), 2);
    for (row, units) in rows.iter().zip([50, 100]) {
        assert_eq!(row.units, units);
        assert_eq!(row.map, "base.scale_2km", "terrain by default");
        assert!(row.entities > units as usize, "entity mix: {row:?}");
        assert_eq!(
            row.per_phase_mean_ms.len(),
            8,
            "{:?}",
            row.per_phase_mean_ms
        );
        assert!(row.mean_ms > 0.0 && row.max_ms >= row.p95_ms, "{row:?}");
    }
}

#[test]
fn median_of_repeats() {
    assert!((median(&[3., 1., 2., 5., 4.]) - 3.0).abs() < f64::EPSILON);
    assert!((median(&[4., 1., 3., 2.]) - 2.5).abs() < f64::EPSILON);
}

#[test]
fn compare_flags_regression() {
    let base = [row(500, 1.0)];
    let err = compare(&[row(500, 1.3)], &base, 0.20).expect_err("30 % is a regression");
    let CompareError::Regression(sizes) = &err else {
        panic!("a regression, got {err:?}");
    };
    assert_eq!(sizes.len(), 1);
    assert!(err.to_string().contains("500 units"), "{err}");
    compare(&[row(500, 1.1)], &base, 0.20).expect("10 % is fine");
}

#[test]
fn compare_without_common_sizes_fails() {
    let err = compare(&[row(500, 1.0)], &[row(3000, 1.0)], 0.20).expect_err("nothing compared");
    assert_eq!(
        err,
        CompareError::NoCommonSize {
            run: vec![500],
            baseline: vec![3000]
        }
    );
    assert!(err.to_string().contains("no size in common"), "{err}");
}

#[test]
fn markdown_states_the_profiler_window() {
    let config = cfg(&[50], 1000);
    assert_eq!(config.profiler_window_ticks(), 240);
    assert_eq!(cfg(&[50], 30).profiler_window_ticks(), 30);
    let mut r = row(50, 1.0);
    r.per_phase_mean_ms = vec![("Input".to_owned(), 0.1)];
    let markdown = render_markdown(&[r], "meta", config.profiler_window_ticks());
    assert!(
        markdown.contains("Mean ms per phase (profiler window: last 240 ticks)"),
        "{markdown}"
    );
}

#[test]
fn repeat_zero_runs_once() {
    let mut config = cfg(&[50], 5);
    config.repeat = 0;
    assert_eq!(config.runs(), 1);
}

/// The final state hash of 100 scale units after 30 ticks, hashing every tick.
fn final_hash(phase_markers: bool) -> u64 {
    let features = fl_catalog::features();
    let content =
        load_content(&features, &[fl_catalog::base_mod_dir()]).expect("base content loads");
    let setup = MatchSetup {
        seed: 1,
        cheats_allowed: false,
    };
    let run = RunConfig {
        hash_interval: Some(1),
        phase_markers,
        ..RunConfig::default()
    };
    let mut app = build_sim_app(&features, content, setup, run).expect("sim builds");
    let scale = ScaleScenario {
        units: 100,
        ..ScaleScenario::default()
    };
    scale.populate(app.world_mut()).expect("scale populates");
    let mut hash = 0;
    for t in 0..30 {
        let input = TickInput {
            tick: Tick(t),
            commands: Vec::new(),
        };
        hash = step_tick(app.world_mut(), input)
            .hash
            .expect("hashed every tick");
    }
    hash
}

#[test]
fn instrumented_hash_unchanged() {
    let _guard = exclusive();
    // nextest runs every test in its own process, so no subscriber is installed yet: the
    // profiler-off hash is taken with nothing observing the sim.
    let plain = final_hash(false);
    // The profiler layer is global: with it installed, "markers on" is really observed.
    let cfg = DebugConfig {
        log_filter: "error".to_owned(),
        json_log: false,
        journal_file: false,
        profiler: true,
        ..DebugConfig::default()
    };
    init_headless(&cfg, None);
    assert_eq!(final_hash(true), plain);
}

#[test]
fn profile_json_has_features_and_phases() {
    let results = run_bench_full(&cfg(&[100], 30)).expect("bench runs");
    let (rows, perf): (Vec<_>, Vec<_>) = results.into_iter().unzip();
    let doc = profile_json(&rows, &perf, &serde_json::json!({"seed": 1}));
    let movement = &doc["features"]["core.movement"];
    assert!(movement["p95_ms"].as_f64().expect("p95") > 0.0, "{doc:#}");
    assert_eq!(doc["phases"].as_object().expect("phases").len(), 8);
    assert_eq!(doc["rows"][0]["units"], 100);
    assert_eq!(movement["samples"], 30, "{doc:#}");
    assert_eq!(movement["window_samples"], 30, "{doc:#}");
}

#[test]
fn bench_json_on_stdout() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_fl_cli"))
        .args([
            "--json", "bench", "--units", "20", "--ticks", "5", "--warmup", "1",
        ])
        .args(["--repeat", "1", "--profile"])
        .output()
        .expect("run fl_cli");
    assert_eq!(output.status.code(), Some(0));
    let doc: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout is one JSON document");
    assert_eq!(doc["rows"][0]["units"], 20);
    assert_eq!(doc["meta"]["profiler_window_ticks"], 5);
    assert!(doc["features"].is_object(), "--profile adds the breakdown");
}

#[test]
fn baseline_build_mismatch_warns() {
    let tmp = tempfile::tempdir().expect("temp dir");
    let this = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    let other = if cfg!(debug_assertions) {
        "release"
    } else {
        "debug"
    };
    let baseline = tmp.path().join("baseline.json");
    let doc = serde_json::json!({"meta": {"build": other}, "rows": [row(20, 1000.0)]});
    std::fs::write(&baseline, doc.to_string()).expect("write the baseline");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_fl_cli"))
        .args([
            "--json", "bench", "--units", "20", "--ticks", "3", "--warmup", "0",
        ])
        .args(["--repeat", "1", "--flat", "--baseline"])
        .arg(&baseline)
        .output()
        .expect("run fl_cli");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(0), "{stderr}");
    assert!(
        stderr.contains(&format!(
            "warning: the baseline is a {other} build, this run is a {this} build"
        )),
        "{stderr}"
    );
    let doc: serde_json::Value = serde_json::from_slice(&output.stdout).expect("one JSON document");
    assert_eq!(doc["baseline"]["ok"], true);
}

#[test]
fn bench_flat_flag_uses_open_map() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_fl_cli"))
        .args([
            "--json", "bench", "--units", "20", "--ticks", "3", "--warmup", "0",
        ])
        .args(["--repeat", "1", "--flat"])
        .output()
        .expect("run fl_cli");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(0), "{stderr}");
    let doc: serde_json::Value = serde_json::from_slice(&output.stdout).expect("one JSON document");
    assert_eq!(doc["meta"]["map"], "open");
    assert_eq!(doc["rows"][0]["map"], "open");
    let rows = rows_from_json(&doc).expect("the rows parse");
    assert_eq!(rows[0].map, "open");

    // A baseline row written before rows named their map (docs/perf/M0.json) ran on the
    // open map.
    let mut old = doc.clone();
    old["rows"][0]
        .as_object_mut()
        .expect("a row is an object")
        .remove("map");
    assert_eq!(rows_from_json(&old).expect("old rows parse")[0].map, "open");
}

#[test]
fn baseline_on_another_map_needs_force() {
    let tmp = tempfile::tempdir().expect("temp dir");
    let build = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    // An open-map baseline (as docs/perf/M0.json), and a run on the terrain map.
    let baseline = tmp.path().join("baseline.json");
    let doc = serde_json::json!({"meta": {"build": build}, "rows": [row(20, 1000.0)]});
    std::fs::write(&baseline, doc.to_string()).expect("write the baseline");
    let run = |force: bool| {
        let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_fl_cli"));
        command
            .args([
                "--json", "bench", "--units", "20", "--ticks", "3", "--warmup", "0",
            ])
            .args(["--repeat", "1", "--baseline"])
            .arg(&baseline);
        if force {
            command.arg("--force");
        }
        command.output().expect("run fl_cli")
    };

    let refused = run(false);
    let stderr = String::from_utf8_lossy(&refused.stderr);
    assert_eq!(refused.status.code(), Some(1), "{stderr}");
    assert!(
        stderr.contains("the baseline ran on map open, this run on map base.scale_2km")
            && stderr.contains("pass --force"),
        "{stderr}"
    );

    let forced = run(true);
    let stderr = String::from_utf8_lossy(&forced.stderr);
    assert_eq!(forced.status.code(), Some(0), "{stderr}");
    assert!(
        stderr.contains("warning: the baseline ran on map open"),
        "{stderr}"
    );
    let doc: serde_json::Value = serde_json::from_slice(&forced.stdout).expect("one JSON document");
    assert_eq!(doc["baseline"]["ok"], true);
}
