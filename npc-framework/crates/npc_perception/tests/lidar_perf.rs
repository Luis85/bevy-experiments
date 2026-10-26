//! The cost of one robot's scan (plan Task 12): a 360-beam, 20 m LIDAR
//! (open ground and a walled room), a camera, a radar. Ignored by default;
//! run in release:
//! `cargo test --release -p npc_perception --test lidar_perf -- --ignored --nocapture`.

// determinism: wall-clock timing of the test harness only; never sim state.
#![allow(
    clippy::disallowed_methods,
    clippy::disallowed_types,
    clippy::float_arithmetic
)]

use bevy_app::App;
use npc_kernel::spatial::{CellCoord, GridBounds, SpatialGrid, SpatialPlugin};
use npc_kernel::{NpcKernelPlugin, SimPos, run_ticks, spawn_sim};
use npc_perception::{Facing, Perceivable, PerceptionConfig, PerceptionPlugin, Sensors};

fn world(sensors: Sensors, room: bool) -> App {
    let mut app = App::new();
    app.insert_resource(SpatialGrid::new(1000, GridBounds::centered(128, 128)))
        .insert_resource(PerceptionConfig {
            stagger: 1,
            ..Default::default()
        });
    app.add_plugins((NpcKernelPlugin::default(), SpatialPlugin, PerceptionPlugin));
    app.finish();
    app.cleanup();
    if room {
        let mut grid = app.world_mut().resource_mut::<SpatialGrid>();
        for i in -6..=6 {
            for c in [(i, -6), (i, 6), (-6, i), (6, i)] {
                let mut info = grid.static_cell(CellCoord::new(c.0, c.1));
                info.flags.occluding = true;
                info.flags.blocking = true;
                grid.set_static(CellCoord::new(c.0, c.1), info);
            }
        }
    }
    for k in 0..60 {
        let (x, z) = ((k * 7919) % 40 - 20, (k * 104_729) % 40 - 20);
        spawn_sim(
            app.world_mut(),
            (
                SimPos::new(x * 1000 + 300, z * 1000 + 700),
                Perceivable::new(1),
            ),
        );
    }
    spawn_sim(
        app.world_mut(),
        (SimPos::new(500, 500), Facing::new(1, 2), sensors),
    );
    app
}

fn time(label: &str, sensors: impl Fn() -> Sensors, room: bool) {
    for walk in [false, true] {
        let mut app = world(sensors(), room);
        let robot = app
            .world_mut()
            .query_filtered::<bevy_ecs::entity::Entity, bevy_ecs::query::With<Sensors>>()
            .single(app.world())
            .unwrap();
        run_ticks(app.world_mut(), 50);
        let n: u64 = 2000;
        let t = std::time::Instant::now();
        for k in 0..n {
            if walk {
                // A step of 7 mm a tick, back and forth: never idle.
                let x = 500 + i32::try_from(k % 100).unwrap() * 7;
                *app.world_mut().get_mut::<SimPos>(robot).unwrap() = SimPos::new(x, 500);
            }
            run_ticks(app.world_mut(), 1);
        }
        let us = t.elapsed().as_secs_f64() * 1e6 / n as f64;
        let how = if walk { "moving" } else { "idle" };
        println!("{label}, {how}: {us:.1} µs per tick (a scan every tick)");
    }
}

#[test]
#[ignore = "perf numbers; run in release with --nocapture"]
fn one_robot_scan_cost() {
    for room in [false, true] {
        let tag = if room { "room" } else { "open" };
        time(&format!("{tag}, no sensors"), Sensors::new, room);
        time(
            &format!("{tag}, lidar 360"),
            || Sensors::new().lidar(20_000, 360, 360, 10_000),
            room,
        );
        time(
            &format!("{tag}, lidar 360 x 2 m"),
            || Sensors::new().lidar(2_000, 360, 360, 10_000),
            room,
        );
        time(
            &format!("{tag}, lidar 36 x 20 m"),
            || Sensors::new().lidar(20_000, 360, 36, 10_000),
            room,
        );
        time(
            &format!("{tag}, camera"),
            || Sensors::new().camera(15_000, 35),
            room,
        );
        time(
            &format!("{tag}, radar"),
            || Sensors::new().radar(30_000, 60, 500, 2000),
            room,
        );
    }
}
