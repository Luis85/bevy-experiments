//! Perception cost with 200 agents (plan wave D1 perf numbers). Ignored by
//! default; run in release:
//! `cargo test --release -p npc_perception --test perf -- --ignored --nocapture`.

use bevy_app::App;
use bevy_ecs::prelude::*;
use npc_kernel::spatial::{CellCoord, GridBounds, SpatialGrid, SpatialPlugin};
use npc_kernel::{
    NpcKernelPlugin, NpcSet, NpcSimTick, SimClock, SimId, SimPos, run_ticks, spawn_sim,
};
use npc_perception::{
    Noise, Perceivable, PerceptionConfig, PerceptionMemory, PerceptionPlugin, Sensors,
};

fn walk(mut q: Query<(&SimId, &mut SimPos), With<Sensors>>, clock: Res<SimClock>) {
    for (id, mut p) in &mut q {
        let phase = (clock.tick / 60 + u64::from(id.0)) % 4;
        let (dx, dz) = [(60, 0), (0, 60), (-60, 0), (0, -60)][phase as usize];
        p.x += dx;
        p.z += dz;
    }
}

fn chatter(
    q: Query<(&SimId, &SimPos), With<Sensors>>,
    clock: Res<SimClock>,
    mut commands: Commands,
) {
    for (id, p) in &q {
        if (clock.tick + u64::from(id.0)).is_multiple_of(40) {
            commands.queue(npc_perception::EmitNoise(
                Noise::at(*p, 10_000).from_source(*id),
            ));
        }
    }
}

fn crowd(agents: i32, stagger: u32, plugin: bool) -> App {
    let mut app = App::new();
    app.insert_resource(SpatialGrid::new(1000, GridBounds::centered(128, 128)))
        .insert_resource(PerceptionConfig {
            stagger,
            ..Default::default()
        });
    app.add_plugins((NpcKernelPlugin::default(), SpatialPlugin));
    if plugin {
        app.add_plugins(PerceptionPlugin);
    }
    app.add_systems(NpcSimTick, (walk, chatter).chain().in_set(NpcSet::Actions));
    app.finish();
    {
        // Scattered occluding cells (every 7th cell on every 5th row).
        let mut grid = app.world_mut().resource_mut::<SpatialGrid>();
        for x in -50i32..50 {
            for z in (-50..50).step_by(5) {
                if (x * 3 + z).rem_euclid(7) == 0 {
                    let mut info = grid.static_cell(CellCoord::new(x, z));
                    info.flags.occluding = true;
                    grid.set_static(CellCoord::new(x, z), info);
                }
            }
        }
    }
    // A 14 × 14-ish block with 3 m spacing: ~40 m across, so most agents
    // have dozens of others within their 15 m vision range.
    for i in 0..agents {
        let pos = SimPos::new((i % 14) * 3000 - 20_000, (i / 14) * 3000 - 20_000);
        spawn_sim(
            app.world_mut(),
            (
                pos,
                Sensors::new()
                    .vision(15_000, 60)
                    .hearing(10_000)
                    .proximity(1500),
                Perceivable::new(1),
            ),
        );
    }
    app
}

#[test]
#[ignore = "perf numbers; run in release with --nocapture"]
// determinism: wall-clock timing of the test harness only, never sim state.
#[allow(
    clippy::disallowed_methods,
    clippy::disallowed_types,
    clippy::float_arithmetic
)]
fn perception_200_agents_us_per_tick() {
    let mut base = 0.0;
    for (stagger, plugin) in [(1, false), (1, true), (4, true)] {
        let mut app = crowd(200, stagger, plugin);
        run_ticks(app.world_mut(), 20); // warm up
        let ticks = 400u32;
        let t = std::time::Instant::now();
        run_ticks(app.world_mut(), u64::from(ticks));
        let us = t.elapsed().as_secs_f64() * 1e6 / f64::from(ticks);
        let world = app.world_mut();
        let mut q = world.query::<&PerceptionMemory>();
        let percepts: usize = q.iter(world).map(|m| m.percepts.len()).sum();
        if !plugin {
            base = us;
            println!("200 agents without the plugin: {us:.1} µs/tick (baseline)");
            continue;
        }
        println!(
            "200 agents, stagger {stagger}: {us:.1} µs/tick whole tick, {:.1} µs/tick \
             perception, {percepts} percepts held",
            us - base
        );
    }
}

/// Plan Task 9: the fog update's cost. One actor revealing fog (the
/// explorer) and all 200, against the same crowd without fog, stagger 1
/// (every tick a scan, the worst case) and 4 (the default).
#[test]
#[ignore = "perf numbers; run in release with --nocapture"]
// determinism: wall-clock timing of the test harness only, never sim state.
#[allow(
    clippy::disallowed_methods,
    clippy::disallowed_types,
    clippy::float_arithmetic
)]
fn fog_us_per_tick() {
    for stagger in [1, 4] {
        let mut base = 0.0;
        for fog in [0usize, 1, 200] {
            let mut app = crowd(200, stagger, true);
            {
                let world = app.world_mut();
                let mut q = world.query_filtered::<(Entity, &SimId), With<Sensors>>();
                let mut agents: Vec<(Entity, SimId)> =
                    q.iter(world).map(|(e, &id)| (e, id)).collect();
                agents.sort_unstable_by_key(|a| a.1);
                for (e, _) in agents.into_iter().take(fog) {
                    world.entity_mut(e).insert(npc_perception::RevealsFog);
                }
            }
            run_ticks(app.world_mut(), 20); // warm up
            let ticks = 400u32;
            let t = std::time::Instant::now();
            run_ticks(app.world_mut(), u64::from(ticks));
            let us = t.elapsed().as_secs_f64() * 1e6 / f64::from(ticks);
            if fog == 0 {
                base = us;
                println!("stagger {stagger}, no fog: {us:.1} µs/tick");
                continue;
            }
            println!(
                "stagger {stagger}, {fog} revealing fog: {us:.1} µs/tick, fog {:.1} µs/tick \
                 ({:.2} µs per actor scan)",
                us - base,
                (us - base) * f64::from(stagger) / fog as f64
            );
        }
    }
}
