//! Probe: `bevy/dynamic_linking` together with sim crates that depend only on
//! `bevy_app`/`bevy_ecs` (risk: duplicate-crate link errors). Throwaway.
//!
//! Besides running both apps side by side, the probe crosses the boundary three ways, so a
//! second copy of `bevy_ecs` (separate type ids, statics, registries) would show up:
//! (a) an `fl_sim` resource inserted into the `bevy` app and read by a `bevy::prelude` system;
//! (b) a `bevy::prelude` system run against the sim app's world, reading a sim resource;
//! (c) the sim `World` moved into a resource of the `bevy` app and stepped from a system.

use std::path::PathBuf;
use std::sync::Arc;

use bevy::prelude::*;
use fl_protocol::Tick;
use fl_sim::{GameFeature, SimClock, SimConfig, SimTick, build_sim_app, load_content};

const TICKS: u32 = 10;

#[derive(Resource, Default)]
struct Updates(u32);

/// (a) result, written by a system declared through `bevy::prelude`.
#[derive(Resource, Default)]
struct SeenClock(Option<Tick>);

/// (c): the sim world, owned by the bevy app.
#[derive(Resource)]
struct SimHost(World);

fn count(mut updates: ResMut<Updates>) {
    updates.0 += 1;
}

fn read_clock(clock: Res<SimClock>, mut seen: ResMut<SeenClock>) {
    seen.0 = Some(clock.tick);
}

fn step_hosted_sim(mut host: ResMut<SimHost>) {
    host.0.run_schedule(SimTick);
}

fn build_sim() -> App {
    let core_mod = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates/fl_sim/tests/fixtures/core_mod");
    let features: Vec<Arc<dyn GameFeature>> = Vec::new();
    let content = load_content(&features, &[core_mod]).expect("core_mod loads");
    let config = SimConfig {
        seed: 1,
        hash_interval: None,
        allow_debug_commands: false,
    };
    build_sim_app(&features, content, config)
}

fn sim_clock(world: &World) -> Tick {
    world.resource::<SimClock>().tick
}

fn main() {
    // Baseline: both apps side by side.
    let mut sim = build_sim();
    for _ in 0..TICKS {
        sim.world_mut().run_schedule(SimTick);
    }
    let sim_ticks: u32 = TICKS;
    assert_eq!(
        sim_clock(sim.world()),
        Tick(TICKS as _),
        "sim clock advanced"
    );

    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<Updates>()
        .add_systems(Update, count);
    for _ in 0..TICKS {
        app.update();
    }
    let app_updates: u32 = app.world().resource::<Updates>().0;
    assert_eq!(app_updates, TICKS);

    // (a) fl_sim resource inside a bevy app, read by a bevy::prelude system.
    let mut a = App::new();
    a.add_plugins(MinimalPlugins)
        .insert_resource(*sim.world().resource::<SimClock>())
        .init_resource::<SeenClock>()
        .add_systems(Update, read_clock);
    a.update();
    assert_eq!(a.world().resource::<SeenClock>().0, Some(Tick(TICKS as _)));

    // (b) a bevy::prelude Schedule run against the sim app's world.
    let mut schedule = Schedule::default();
    schedule.add_systems((read_clock,));
    sim.world_mut().insert_resource(SeenClock::default());
    schedule.run(sim.world_mut());
    assert_eq!(
        sim.world().resource::<SeenClock>().0,
        Some(Tick(TICKS as _))
    );

    // (c) the sim World moved into a bevy resource and stepped from a bevy system.
    let mut sim2 = build_sim();
    let world = std::mem::take(sim2.world_mut());
    let mut c = App::new();
    c.add_plugins(MinimalPlugins)
        .insert_resource(SimHost(world))
        .add_systems(Update, step_hosted_sim);
    for _ in 0..TICKS {
        c.update();
    }
    assert_eq!(
        sim_clock(&c.world().resource::<SimHost>().0),
        Tick(TICKS as _)
    );

    println!("dynlink probe: OK sim_ticks={sim_ticks} app_updates={app_updates}");
}
