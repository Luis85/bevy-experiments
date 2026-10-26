//! Plan Task 12 (spec G10): the LIDAR. A planar scan shaped like ROS
//! `sensor_msgs/LaserScan` (mm / millidegrees): per beam the distance to the
//! first blocking or occluding cell or cell holding a perceivable thing, a
//! miss reported as `range_max_mm + 1`, deterministic noise from `sim_rand`,
//! and no identities (it feeds no memory).

use bevy_app::App;
use bevy_ecs::prelude::*;
use npc_kernel::spatial::{
    CellCoord, CellFlags, CellInfo, GridBlocker, GridBounds, SpatialGrid, SpatialPlugin,
};
use npc_kernel::{NpcKernelPlugin, SimPos, run_ticks, spawn_sim};
use npc_perception::{
    Facing, FixedFacing, LaserScan, LidarObstacles, Perceivable, PerceptionConfig,
    PerceptionMemory, PerceptionPlugin, SensorReadings, Sensors, is_due, is_lidar_due,
};

/// Facing +z (north).
const NORTH: Facing = Facing::new(0, 1);

fn app(seed: u64) -> App {
    let mut app = App::new();
    app.insert_resource(SpatialGrid::new(1000, GridBounds::centered(64, 64)))
        .insert_resource(PerceptionConfig {
            stagger: 1,
            ..Default::default()
        });
    app.add_plugins((
        NpcKernelPlugin {
            seed,
            ..Default::default()
        },
        SpatialPlugin,
        PerceptionPlugin,
    ));
    app.finish();
    app.cleanup();
    app
}

/// A rover-like LIDAR at the centre of cell (0, 0), facing north.
fn robot(app: &mut App, noise_ppm: u32) -> Entity {
    spawn_sim(
        app.world_mut(),
        (
            SimPos::new(500, 500),
            NORTH,
            FixedFacing,
            Sensors::new().lidar(20_000, 360, 360, noise_ppm),
        ),
    )
    .1
}

fn wall(app: &mut App, cells: impl IntoIterator<Item = (i32, i32)>) {
    let mut grid = app.world_mut().resource_mut::<SpatialGrid>();
    for (x, z) in cells {
        grid.set_static(
            CellCoord::new(x, z),
            CellInfo {
                flags: CellFlags {
                    blocking: true,
                    occluding: true,
                    tags: 0,
                },
                extra_cost: 0,
            },
        );
    }
}

fn scan(app: &App, e: Entity) -> LaserScan {
    app.world()
        .get::<SensorReadings>(e)
        .and_then(|r| r.lidar.clone())
        .expect("a laser scan")
}

/// The beam index at `angle_mdeg` (relative to the facing, + = left).
fn beam(s: &LaserScan, angle_mdeg: i32) -> usize {
    usize::try_from((angle_mdeg - s.angle_min_mdeg) / s.angle_increment_mdeg).unwrap()
}

#[test]
fn ranges_hit_the_wall() {
    let mut app = app(1);
    let e = robot(&mut app, 0);
    // A wall along z = 5 (x -10..=10) and a thing 2 m ahead.
    wall(&mut app, (-10..=10).map(|x| (x, 5)));
    spawn_sim(
        app.world_mut(),
        (SimPos::new(500, 2500), Perceivable::new(1)),
    );
    run_ticks(app.world_mut(), 1);
    let s = scan(&app, e);
    assert_eq!(s.ranges_mm.len(), 360);
    // Straight ahead: the thing's cell (z = 2) starts 1.5 m away.
    assert_eq!(s.ranges_mm[beam(&s, 0)], 1500);
    // 45 degrees to either side: the wall row (z = 5) starts 4.5 m north,
    // so 4.5 m · √2 along the beam.
    for angle in [45_000, -45_000] {
        let r = s.ranges_mm[beam(&s, angle)];
        assert!((6362..=6366).contains(&r), "{angle}: {r}");
    }
    // Sideways and behind: nothing within 20 m.
    for angle in [90_000, -90_000, -180_000] {
        assert_eq!(s.ranges_mm[beam(&s, angle)], s.range_max_mm + 1, "{angle}");
    }
    // A LIDAR reports ranges only: nothing enters the memory.
    assert!(
        app.world()
            .get::<PerceptionMemory>(e)
            .unwrap()
            .percepts
            .is_empty()
    );
}

#[test]
fn misses_report_max_plus_one() {
    let mut app = app(1);
    let e = robot(&mut app, 0);
    run_ticks(app.world_mut(), 1);
    let s = scan(&app, e);
    assert_eq!(
        (
            s.angle_min_mdeg,
            s.angle_increment_mdeg,
            s.range_max_mm,
            s.ranges_mm.len()
        ),
        (-180_000, 1000, 20_000, 360)
    );
    assert!(s.range_min_mm >= 0 && s.range_min_mm < s.range_max_mm);
    assert!(
        s.ranges_mm.iter().all(|&r| r == 20_001),
        "{:?}",
        s.ranges_mm
    );
}

#[test]
fn noise_is_deterministic() {
    let run = |seed: u64, noise: u32| {
        let mut app = app(seed);
        let e = robot(&mut app, noise);
        wall(&mut app, (-12..=12).flat_map(|x| [(x, 8), (x, -8)]));
        wall(&mut app, (-8..=8).flat_map(|z| [(-12, z), (12, z)]));
        run_ticks(app.world_mut(), 3);
        scan(&app, e).ranges_mm
    };
    let clean = run(1, 0);
    let a = run(1, 10_000);
    assert_eq!(a, run(1, 10_000), "same seed, same noise");
    assert_ne!(a, run(2, 10_000), "another seed, other noise");
    assert_ne!(a, clean, "noise moves some ranges");
    for (i, (&n, &c)) in a.iter().zip(&clean).enumerate() {
        assert!(c <= 20_000, "beam {i} hits the box");
        // ±1 % (and the integer rounding).
        assert!((n - c).abs() <= c / 100 + 1, "beam {i}: {n} vs {c}");
    }
}

/// Fix round 1: content picks which things stop a beam
/// (`LidarObstacles`): an item outside the mask does not fill its cell; a
/// door's cell stops a beam only while its blocker is there (closed), not
/// once it is gone (open).
#[test]
fn obstacles_follow_the_content_mask() {
    const ITEM: u32 = 1 << 1;
    const DOOR: u32 = 1 << 2;
    let mut app = app(1);
    app.insert_resource(LidarObstacles {
        kinds: !(ITEM | DOOR),
    });
    let e = robot(&mut app, 0);
    // An item 2 m ahead, a closed door (its blocker) 5 m to the left, and
    // a wall 8 m ahead behind the item.
    spawn_sim(
        app.world_mut(),
        (SimPos::new(500, 2500), Perceivable::new(ITEM)),
    );
    let (_, door) = spawn_sim(
        app.world_mut(),
        (
            SimPos::new(-4500, 500),
            Perceivable::new(DOOR),
            GridBlocker::default(),
        ),
    );
    wall(&mut app, (-3..=3).map(|x| (x, 8)));
    run_ticks(app.world_mut(), 2);
    let s = scan(&app, e);
    assert_eq!(
        s.ranges_mm[beam(&s, 0)],
        7500,
        "the item lets the beam through"
    );
    // The door's cell (x = -5) starts 4.5 m to the left.
    assert_eq!(
        s.ranges_mm[beam(&s, 90_000)],
        4500,
        "the closed door stops it"
    );
    // Opened: its blocker goes, the door entity stays.
    app.world_mut().entity_mut(door).remove::<GridBlocker>();
    run_ticks(app.world_mut(), 2);
    let s = scan(&app, e);
    assert!(
        s.is_miss(s.ranges_mm[beam(&s, 90_000)]),
        "the open door lets it through"
    );
    // The default mask: every perceivable thing stops a beam.
    app.insert_resource(LidarObstacles::default());
    run_ticks(app.world_mut(), 1);
    let s = scan(&app, e);
    assert_eq!(s.ranges_mm[beam(&s, 0)], 1500);
    assert_eq!(s.ranges_mm[beam(&s, 90_000)], 4500);
}

/// Fix round 1: an idle robot reuses its traced ranges (only the noise is
/// drawn again), and any change the trace depends on (a wall appearing, a
/// thing moving) is seen at once: the readings equal those of a world that
/// had the change from the start.
#[test]
fn reused_scans_follow_every_change() {
    let build = |wall_from_start: bool| {
        let mut app = app(3);
        let e = robot(&mut app, 10_000);
        if wall_from_start {
            wall(&mut app, (-10..=10).map(|x| (x, 6)));
        }
        let thing = spawn_sim(
            app.world_mut(),
            (SimPos::new(-2500, 500), Perceivable::new(1)),
        )
        .1;
        (app, e, thing)
    };
    let (mut late, e, thing) = build(false);
    let (mut early, _, thing_b) = build(true);
    run_ticks(late.world_mut(), 10);
    run_ticks(early.world_mut(), 10);
    wall(&mut late, (-10..=10).map(|x| (x, 6)));
    for app in [&mut late, &mut early] {
        run_ticks(app.world_mut(), 5);
    }
    assert_eq!(scan(&late, e), scan(&early, e));
    // The thing moves 2 m.
    for (app, t) in [(&mut late, thing), (&mut early, thing_b)] {
        *app.world_mut().get_mut::<SimPos>(t).unwrap() = SimPos::new(-4500, 500);
        run_ticks(app.world_mut(), 3);
    }
    let (a, b) = (scan(&late, e), scan(&early, e));
    assert_eq!(a, b);
    // Noise is ±1 %: the thing's cell (x = -5) starts 4.5 m to the left.
    assert!(a.ranges_mm[beam(&a, 90_000)].abs_diff(4500) <= 46, "{a:?}");
}

/// Fix round 1: with a stagger the LIDAR scans half a stagger after the
/// camera/radar pass, never in the same tick.
#[test]
fn lidar_scans_half_a_stagger_later() {
    let mut app = app(1);
    app.insert_resource(PerceptionConfig {
        stagger: 4,
        ..Default::default()
    });
    let e = spawn_sim(
        app.world_mut(),
        (
            SimPos::new(500, 500),
            NORTH,
            FixedFacing,
            Sensors::new().lidar(20_000, 360, 36, 0).camera(15_000, 35),
        ),
    )
    .1;
    let id = *app.world().get::<npc_kernel::SimId>(e).unwrap();
    for _ in 0..12 {
        run_ticks(app.world_mut(), 1);
        // The tick just run (the clock has moved on).
        let tick = app.world().resource::<npc_kernel::SimClock>().tick - 1;
        let r = app
            .world()
            .get::<SensorReadings>(e)
            .cloned()
            .unwrap_or_default();
        assert!(!(is_due(id, tick, 4) && is_lidar_due(id, tick, 4)));
        if is_lidar_due(id, tick, 4) {
            assert_eq!(r.lidar_tick, tick);
        }
        if is_due(id, tick, 4) {
            assert_eq!(r.tick, tick);
        }
    }
    let r = app.world().get::<SensorReadings>(e).unwrap();
    assert_eq!(
        r.tick.abs_diff(r.lidar_tick) % 4,
        2,
        "{} vs {}",
        r.tick,
        r.lidar_tick
    );
}
