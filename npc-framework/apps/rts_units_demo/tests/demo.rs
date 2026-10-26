//! The portability proof, headless: the demo's own content moves its squads
//! across the map around the walls deterministically, the `--headless-ticks`
//! flag reports it as JSON, and the framework leaves the host's fixed
//! timestep alone (hardening spec goal 5, plan Task P4).

use std::process::Command;
use std::time::Duration;

use bevy::prelude::*;
use bevy::time::{TimePlugin, TimeUpdateStrategy};
use npc_framework::prelude::{SimClock, SimRate};
use rts_units_demo::host::{self, HOST_FIXED_HZ, HostSteps};
use rts_units_demo::sim::{self, DEFAULT_UNITS, Summary};

/// Every unit must have arrived by then (they do at ~1050 at seed 1).
const TICK_BUDGET: u64 = 1500;

#[test]
fn rts_demo_orders_move_squads() {
    let a = sim::run_headless(1, DEFAULT_UNITS, TICK_BUDGET);
    assert_eq!(a.units, 200);
    assert_eq!(a.squads, 4);
    assert_eq!(a.failed, 0, "{a:?}");
    assert_eq!(a.arrived, a.units, "every unit arrives: {a:?}");
    let at = a.all_arrived_at.expect("all arrived");
    assert!(at <= TICK_BUDGET, "{a:?}");
    // The squads really crossed the map (not an instant no-op).
    assert!(at > 500, "arrived suspiciously early: {a:?}");
    let b = sim::run_headless(1, DEFAULT_UNITS, TICK_BUDGET);
    assert_eq!(a, b, "same seed and orders: same summary and state hash");
}

#[test]
fn headless_flag_prints_the_summary() {
    let out = Command::new(env!("CARGO_BIN_EXE_rts_units_demo"))
        .args(["--headless-ticks", "1500", "--units", "80", "--seed", "3"])
        .output()
        .expect("run the demo binary");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("one JSON object on stdout");
    assert!(json["ticks_per_sec"].as_f64().is_some_and(|t| t > 0.0));
    let summary: Summary = serde_json::from_value(json).expect("summary fields");
    assert_eq!(summary.units, 80);
    assert_eq!(summary.arrived, 80);
    assert_eq!(summary, sim::run_headless(3, 80, 1500), "binary == library");
}

#[test]
fn host_fixed_timestep_is_untouched_in_demo() {
    // The demo's host wiring (60 Hz physics in FixedUpdate, the framework at
    // 10 Hz in the same FixedUpdate), advanced by exactly 1 s.
    let mut app = App::new();
    app.add_plugins(TimePlugin)
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
            10,
        )));
    host::add_host(&mut app);
    sim::add_sim(&mut app, sim::npc_plugins(1));
    app.finish();
    app.cleanup();
    sim::spawn_world(app.world_mut(), 40);
    sim::issue_script(app.world_mut()).unwrap();
    // The first update only starts the clock; 100 more advance 1 s.
    for _ in 0..101 {
        app.update();
    }
    let world = app.world();
    assert_eq!(
        world.resource::<Time<Fixed>>().timestep(),
        Time::<Fixed>::from_hz(HOST_FIXED_HZ).timestep(),
        "the host's fixed timestep must stay {HOST_FIXED_HZ} Hz"
    );
    let steps = world.resource::<HostSteps>().0;
    assert!(
        (59..=61).contains(&steps),
        "host ran {steps} physics steps in 1 s"
    );
    let ticks = world.resource::<SimClock>().tick;
    assert!(
        (9..=11).contains(&ticks),
        "sim ran {ticks} ticks in 1 s at 10 Hz"
    );
    assert_eq!(world.resource::<SimRate>().hz, sim::TICK_HZ);
}
