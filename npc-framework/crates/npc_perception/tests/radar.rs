//! Plan Task 12 (spec G10): the radar. Tracks shaped like ROS `radar_msgs`:
//! bearing and range quantized to the sensor's resolution, a radial
//! velocity (+ = moving away), an amplitude that is higher for things that
//! move, blocked by walls. It feeds the memory (`radar`).

use bevy_app::App;
use bevy_ecs::prelude::*;
use npc_kernel::spatial::{GridBounds, GridOccluder, SpatialGrid, SpatialPlugin};
use npc_kernel::{NpcKernelPlugin, SimId, SimIndex, SimPos, run_ticks, spawn_sim};
use npc_perception::{
    Facing, FixedFacing, Perceivable, PerceptionConfig, PerceptionMemory, PerceptionPlugin,
    RadarTrack, SenseKind, SensorReadings, Sensors,
};

const NORTH: Facing = Facing::new(0, 1);

fn app() -> App {
    let mut app = App::new();
    app.insert_resource(SpatialGrid::new(1000, GridBounds::centered(96, 96)))
        .insert_resource(PerceptionConfig {
            stagger: 1,
            ..Default::default()
        });
    app.add_plugins((NpcKernelPlugin::default(), SpatialPlugin, PerceptionPlugin));
    app.finish();
    app.cleanup();
    app
}

/// The sentinel's radar: 30 m, 60 degrees to each side, 0.5 m and
/// 2 degree resolution, at (0.5 m, 0.5 m) facing north.
fn robot(app: &mut App) -> Entity {
    spawn_sim(
        app.world_mut(),
        (
            SimPos::new(500, 500),
            NORTH,
            FixedFacing,
            Sensors::new().radar(30_000, 60, 500, 2000),
        ),
    )
    .1
}

fn thing(app: &mut App, pos: SimPos) -> SimId {
    spawn_sim(app.world_mut(), (pos, Perceivable::new(1))).0
}

fn tracks(app: &App, e: Entity) -> Vec<RadarTrack> {
    app.world()
        .get::<SensorReadings>(e)
        .map(|r| r.radar.clone())
        .unwrap_or_default()
}

fn pos_of(app: &App, id: SimId) -> SimPos {
    let e = app.world().resource::<SimIndex>().get(id).unwrap();
    *app.world().get::<SimPos>(e).unwrap()
}

fn teleport(app: &mut App, id: SimId, to: SimPos) {
    let e = app.world().resource::<SimIndex>().get(id).unwrap();
    *app.world_mut().get_mut::<SimPos>(e).unwrap() = to;
}

#[test]
fn moving_targets_have_radial_velocity() {
    let mut app = app();
    let e = robot(&mut app);
    // Straight ahead, walking away at 50 mm per tick; another standing
    // still to the left; a third walking towards the radar to the right.
    let away = thing(&mut app, SimPos::new(500, 10_500));
    let still = thing(&mut app, SimPos::new(-4500, 10_500));
    let toward = thing(&mut app, SimPos::new(6500, 20_500));
    for _ in 0..6 {
        let a = pos_of(&app, away);
        teleport(&mut app, away, SimPos::new(a.x, a.z + 50));
        let t = pos_of(&app, toward);
        teleport(&mut app, toward, SimPos::new(t.x, t.z - 50));
        run_ticks(app.world_mut(), 1);
    }
    let t = tracks(&app, e);
    assert_eq!(t.len(), 3, "{t:?}");
    let by_bearing = |sign: i32| {
        t.iter()
            .find(|x| x.bearing_mdeg.signum() == sign)
            .copied()
            .unwrap()
    };
    let (ahead, left, right) = (by_bearing(0), by_bearing(1), by_bearing(-1));
    assert_eq!(ahead.radial_velocity_mm_per_tick, 50, "{ahead:?}");
    assert_eq!(left.radial_velocity_mm_per_tick, 0, "{left:?}");
    assert!(right.radial_velocity_mm_per_tick < -40, "{right:?}");
    assert!(ahead.amplitude_ppm > left.amplitude_ppm);
    assert!(right.amplitude_ppm > left.amplitude_ppm);
    // The memory follows the radar.
    let m = app.world().get::<PerceptionMemory>(e).unwrap();
    for id in [away, still, toward] {
        let p = m.get(id).expect("remembered");
        assert!(p.sensed && p.via == SenseKind::Radar, "{p:?}");
    }
    assert_eq!(SenseKind::Radar.name(), "radar");
}

#[test]
fn values_are_quantized() {
    let mut app = app();
    let e = robot(&mut app);
    // 0.734 m to the right, 7.277 m ahead: 7.314 m at -5.76 degrees.
    thing(&mut app, SimPos::new(1234, 7777));
    run_ticks(app.world_mut(), 1);
    let t = tracks(&app, e);
    assert_eq!(t.len(), 1, "{t:?}");
    assert_eq!((t[0].range_mm, t[0].bearing_mdeg), (7500, -6000));
    assert_eq!(t[0].radial_velocity_mm_per_tick, 0);
}

#[test]
fn walls_block_the_radar() {
    let mut app = app();
    let e = robot(&mut app);
    let hidden = thing(&mut app, SimPos::new(500, 10_500));
    spawn_sim(
        app.world_mut(),
        (SimPos::new(500, 5500), GridOccluder::default()),
    );
    run_ticks(app.world_mut(), 2);
    assert!(tracks(&app, e).is_empty());
    let m = app.world().get::<PerceptionMemory>(e).unwrap();
    assert!(m.get(hidden).is_none());
}
