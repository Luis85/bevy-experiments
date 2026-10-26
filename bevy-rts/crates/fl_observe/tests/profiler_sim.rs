//! The profiler and the JSON log against the real sim, through the global subscriber that
//! `init_headless` installs (its own test binary, so the global subscriber is ours).

use std::path::PathBuf;

use fl_observe::{DebugConfig, global_run_dir, init_headless, new_run_dir};
use fl_sim::{
    MatchSetup, RunConfig, SimClock, build_sim_app, load_content, setup_scenario, step_tick,
};
use fl_view::TickInput;

fn core_mod() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../fl_sim/tests/fixtures/core_mod")
}

#[test]
fn profiler_times_real_sim_systems() {
    let cfg = DebugConfig {
        log_filter: "info,fl_sim=debug".to_owned(),
        json_log: true,
        profiler: true,
        ..DebugConfig::default()
    };
    let root = tempfile::tempdir().expect("temp dir");
    let run = new_run_dir(root.path()).expect("run dir");
    // Before the sim is built: bevy_ecs creates each system's span when the system is made.
    let profiler = init_headless(&cfg, Some(&run));
    assert!(std::ptr::eq(profiler, fl_observe::global_profiler()));
    assert_eq!(global_run_dir(), Some(&run));

    // fl_sim has no threaded runner yet (Task 12), so the test steps the sim on a thread of
    // its own, as the runner will.
    std::thread::spawn(|| {
        let content = load_content(&[], &[core_mod()]).expect("core_mod loads");
        let setup = MatchSetup {
            seed: 1,
            cheats_allowed: false,
        };
        let mut app = build_sim_app(
            &[],
            content,
            setup,
            RunConfig {
                phase_markers: true,
                ..RunConfig::default()
            },
        )
        .expect("the sim builds");
        setup_scenario(app.world_mut(), "core_test.duel").expect("duel sets up");
        for _ in 0..30 {
            let tick = app.world().resource::<SimClock>().tick;
            let input = TickInput {
                tick,
                commands: Vec::new(),
            };
            step_tick(app.world_mut(), input);
        }
    })
    .join()
    .expect("the sim thread runs");

    let snapshot = profiler.snapshot();
    assert_eq!(snapshot.ticks, 30);
    assert_eq!(snapshot.tick.samples, 30);
    let phases: Vec<&str> = snapshot.phases.iter().map(|(p, _)| p.as_str()).collect();
    assert_eq!(phases.len(), 8, "{phases:?}");
    assert!(snapshot.phases.iter().all(|(_, s)| s.samples >= 30));
    let movement = snapshot
        .systems
        .iter()
        .find(|s| s.name == "fl_sim::movement::move_system")
        .unwrap_or_else(|| panic!("move_system in {:?}", snapshot.systems));
    assert_eq!(movement.feature, "core.movement");
    assert!(movement.stat.samples >= 30, "{:?}", movement.stat);
    assert!(
        snapshot.features.iter().any(|(f, _)| f == "core.movement"),
        "{:?}",
        snapshot.features
    );

    // The state hash is logged at debug on fl_sim::step (every 20 ticks with core_mod's
    // rules), and each line carries its tick.
    let log = std::fs::read_to_string(run.path.join("log.jsonl")).expect("log.jsonl");
    let hash_ticks: Vec<u64> = log
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("valid JSON"))
        .filter(|v| v["target"] == "fl_sim::step" && v["fields"]["message"] == "state hash")
        .map(|v| v["tick"].as_u64().expect("a tick on every sim record"))
        .collect();
    assert_eq!(hash_ticks, [0, 20], "{log}");
}
