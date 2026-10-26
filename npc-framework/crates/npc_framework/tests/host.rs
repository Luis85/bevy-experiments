//! Plan Task P3: the framework inside a foreign host app (ADR 0008).
//!
//! - The host keeps its own fixed timestep; the framework's driver keeps its
//!   own accumulator and never writes `Time<Fixed>`.
//! - Registration problems fail loudly at freeze time.
//! - Relaxed determinism lets a host provide positions.

use std::time::Duration;

use bevy_app::prelude::*;
use bevy_ecs::prelude::*;
use bevy_time::{Fixed, Time, TimePlugin, TimeUpdateStrategy};
use npc_framework::prelude::*;

/// Host-side counter of its own `FixedUpdate` steps (physics stand-in).
#[derive(Resource, Default)]
struct HostSteps(u32);

fn host_physics(mut steps: ResMut<HostSteps>) {
    steps.0 += 1;
}

/// A host with a 64 Hz fixed step and the framework at 20 Hz in
/// `FixedUpdate`, advanced by exactly 1 s of virtual time.
fn host_app() -> App {
    let mut app = App::new();
    app.add_plugins(TimePlugin)
        .insert_resource(Time::<Fixed>::from_hz(64.0))
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
            10,
        )))
        .init_resource::<HostSteps>()
        .add_systems(FixedUpdate, host_physics)
        .add_plugins(NpcPlugins::new().seed(1).tick_hz(20).run_in(FixedUpdate));
    app.finish();
    app.cleanup();
    app
}

#[test]
fn host_fixed_timestep_untouched() {
    let mut app = host_app();
    // The first update only starts the clock; 100 more advance 1 s.
    for _ in 0..101 {
        app.update();
    }
    let fixed = app.world().resource::<Time<Fixed>>();
    assert_eq!(
        fixed.timestep(),
        Time::<Fixed>::from_hz(64.0).timestep(),
        "the host's fixed timestep must stay 64 Hz"
    );
    let host_steps = app.world().resource::<HostSteps>().0;
    assert!(
        (63..=65).contains(&host_steps),
        "host ran {host_steps} fixed steps in 1 s"
    );
    let ticks = app.world().resource::<SimClock>().tick;
    assert!(
        (19..=21).contains(&ticks),
        "sim ran {ticks} ticks in 1 s at 20 Hz"
    );
    assert_eq!(app.world().resource::<SimRate>().hz, 20);
}

#[test]
fn driver_respects_pause_speed_and_single_steps() {
    let mut app = host_app();
    for _ in 0..101 {
        app.update();
    }
    let t0 = app.world().resource::<SimClock>().tick;
    app.world_mut().resource_mut::<SimControl>().paused = true;
    for _ in 0..50 {
        app.update();
    }
    assert_eq!(app.world().resource::<SimClock>().tick, t0, "paused");
    app.world_mut().resource_mut::<SimControl>().step();
    app.world_mut().resource_mut::<SimControl>().step();
    for _ in 0..10 {
        app.update();
    }
    assert_eq!(
        app.world().resource::<SimClock>().tick,
        t0 + 2,
        "two single steps"
    );
    {
        let mut c = app.world_mut().resource_mut::<SimControl>();
        c.paused = false;
        c.multiplier = 3;
    }
    for _ in 0..100 {
        app.update();
    }
    let ran = app.world().resource::<SimClock>().tick - (t0 + 2);
    assert!((57..=63).contains(&ran), "3x for 1 s: {ran} ticks");
}

fn ok_start(_: &mut World, _: Entity, _: &ParamValue) -> Status {
    Status::Success
}

fn no_stop(_: &mut World, _: Entity, _: StopReason) {}

#[test]
#[should_panic(
    expected = "tree `scout` node `Go` references unknown action `Teleport`; known actions: [Look, Wait]"
)]
fn unknown_action_name_panics_helpfully() {
    let mut app = App::new();
    app.add_plugins(NpcPlugins::new().without_driver())
        .register_action("Wait", ok_start, no_stop)
        .register_action("Look", ok_start, no_stop)
        .register_tree("scout", |b| {
            b.sequence("Root", |b| {
                b.leaf("Look", "Look", ());
                b.leaf("Go", "Teleport", ());
            });
        });
    app.finish();
}

#[test]
#[should_panic(expected = "content is already frozen")]
fn registering_after_finish_panics() {
    let mut app = App::new();
    app.add_plugins(NpcPlugins::new().without_driver());
    app.finish();
    app.register_action("Late", ok_start, no_stop);
}

/// A host mover: writes `HostPosition` from "physics" every frame.
fn host_mover(mut q: Query<&mut HostPosition>) {
    for mut p in &mut q {
        p.0.x += 10;
    }
}

fn mover_app(mode: DeterminismMode) -> (App, Entity) {
    let mut app = App::new();
    app.add_plugins(NpcPlugins::new().without_driver().determinism(mode))
        .add_systems(Update, host_mover);
    app.finish();
    app.cleanup();
    let world = app.world_mut();
    let (_, e) = spawn_sim(world, (SimPos::new(0, 0), HostPosition(SimPos::new(0, 0))));
    (app, e)
}

/// `DeterminismMode::Relaxed`: host-provided positions flow into `SimPos`
/// at the start of each tick; `Strict` ignores them.
#[test]
fn relaxed_mode_takes_host_positions() {
    for (mode, expect) in [(DeterminismMode::Relaxed, 30), (DeterminismMode::Strict, 0)] {
        let (mut app, e) = mover_app(mode);
        for _ in 0..3 {
            app.update();
        }
        run_ticks(app.world_mut(), 1);
        let pos = *app.world().get::<SimPos>(e).unwrap();
        assert_eq!(pos.x, expect, "{mode:?}");
    }
}
