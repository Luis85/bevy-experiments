//! Plan Task 12 (spec G10): the camera. Detections only, never images,
//! shaped like ROS `vision_msgs`: things in its cone and range with line of
//! sight, each with a class, a bearing, a distance, a size, a confidence
//! (990000 ppm at 0 falling to 600000 at max range) and a tracker id that
//! stays put while the thing stays in view. It feeds the memory (`camera`).

use bevy_app::App;
use bevy_ecs::prelude::*;
use npc_kernel::spatial::{GridBounds, GridOccluder, SpatialGrid, SpatialPlugin};
use npc_kernel::{NpcKernelPlugin, SimId, SimIndex, SimPos, run_ticks, spawn_sim};
use npc_perception::{
    CameraDetection, Facing, FixedFacing, Perceivable, PerceptionConfig, PerceptionMemory,
    PerceptionPlugin, SenseKind, SensorReadings, Sensors,
};

const NORTH: Facing = Facing::new(0, 1);

fn app() -> App {
    let mut app = App::new();
    app.insert_resource(SpatialGrid::new(1000, GridBounds::centered(64, 64)))
        .insert_resource(PerceptionConfig {
            stagger: 1,
            ..Default::default()
        });
    app.add_plugins((NpcKernelPlugin::default(), SpatialPlugin, PerceptionPlugin));
    app.finish();
    app.cleanup();
    app
}

/// A 15 m camera, 35 degrees to each side, at (0.5 m, 0.5 m) facing north.
fn robot(app: &mut App) -> Entity {
    spawn_sim(
        app.world_mut(),
        (
            SimPos::new(500, 500),
            NORTH,
            FixedFacing,
            Sensors::new().camera(15_000, 35),
        ),
    )
    .1
}

fn thing(app: &mut App, pos: SimPos) -> SimId {
    spawn_sim(app.world_mut(), (pos, Perceivable::new(1))).0
}

fn detections(app: &App, e: Entity) -> Vec<CameraDetection> {
    app.world()
        .get::<SensorReadings>(e)
        .map(|r| r.camera.clone())
        .unwrap_or_default()
}

fn memory(app: &App, e: Entity) -> PerceptionMemory {
    app.world().get::<PerceptionMemory>(e).unwrap().clone()
}

fn teleport(app: &mut App, id: SimId, to: SimPos) {
    let e = app.world().resource::<SimIndex>().get(id).unwrap();
    *app.world_mut().get_mut::<SimPos>(e).unwrap() = to;
}

#[test]
fn detects_in_view_with_line_of_sight() {
    let mut app = app();
    let e = robot(&mut app);
    let ahead = thing(&mut app, SimPos::new(500, 5500));
    // 30 degrees to the left, 5 m away.
    let left = thing(&mut app, SimPos::new(500 - 2500, 500 + 4330));
    let behind = thing(&mut app, SimPos::new(500, -3000));
    let too_far = thing(&mut app, SimPos::new(500, 16_600));
    let too_wide = thing(&mut app, SimPos::new(500 - 4000, 4500));
    run_ticks(app.world_mut(), 1);
    let d = detections(&app, e);
    assert_eq!(d.len(), 2, "{d:?}");
    // Nearest first; both 5 m away, so by bearing order of the scan.
    let a = d
        .iter()
        .find(|x| x.bearing_mdeg == 0)
        .expect("the thing ahead");
    assert_eq!(a.distance_mm, 5000);
    assert_eq!(a.confidence_ppm, 990_000 - 390_000 * 5000 / 15_000);
    assert_eq!(a.class, "entity");
    assert!(a.size_mm > 0);
    let l = d.iter().find(|x| x.bearing_mdeg != 0).unwrap();
    assert!(
        (29_900..=30_100).contains(&l.bearing_mdeg),
        "left is positive: {}",
        l.bearing_mdeg
    );
    assert_ne!(a.track_id, l.track_id);
    // The memory follows the camera.
    let m = memory(&app, e);
    for id in [ahead, left] {
        let p = m.get(id).expect("remembered");
        assert!(p.sensed);
        assert_eq!(p.via, SenseKind::Camera);
        assert_eq!(SenseKind::Camera.name(), "camera");
    }
    assert_eq!(m.get(ahead).unwrap().confidence, 860);
    for id in [behind, too_far, too_wide] {
        assert!(m.get(id).is_none(), "{id:?} is out of view");
    }
}

#[test]
fn track_ids_are_stable() {
    let mut app = app();
    let e = robot(&mut app);
    let a = thing(&mut app, SimPos::new(500, 5500));
    let _b = thing(&mut app, SimPos::new(1500, 8500));
    run_ticks(app.world_mut(), 1);
    let first = detections(&app, e);
    let track_of = |d: &[CameraDetection], dist: i32| {
        d.iter().find(|x| x.distance_mm == dist).map(|x| x.track_id)
    };
    let (ta, tb) = (
        track_of(&first, 5000).unwrap(),
        track_of(&first, 8062).unwrap(),
    );
    // A moves a little while in view: same ids.
    for z in [5600, 5700, 5800] {
        teleport(&mut app, a, SimPos::new(500, z));
        run_ticks(app.world_mut(), 1);
        let d = detections(&app, e);
        assert_eq!(track_of(&d, z - 500), Some(ta));
        assert_eq!(track_of(&d, 8062), Some(tb));
    }
    // A leaves the view: its track is dropped; back in view it is new.
    teleport(&mut app, a, SimPos::new(500, -4000));
    run_ticks(app.world_mut(), 1);
    assert_eq!(detections(&app, e).len(), 1);
    teleport(&mut app, a, SimPos::new(500, 5500));
    run_ticks(app.world_mut(), 1);
    let d = detections(&app, e);
    let again = track_of(&d, 5000).unwrap();
    assert!(again != ta && again != tb, "{again} vs {ta}, {tb}");
    assert_eq!(track_of(&d, 8062), Some(tb));
}

#[test]
fn occluded_targets_are_not_detected() {
    let mut app = app();
    let e = robot(&mut app);
    let hidden = thing(&mut app, SimPos::new(500, 5500));
    // A wall cell between them (cell (0, 3)).
    spawn_sim(
        app.world_mut(),
        (SimPos::new(500, 3500), GridOccluder::default()),
    );
    run_ticks(app.world_mut(), 2);
    assert!(detections(&app, e).is_empty());
    assert!(memory(&app, e).get(hidden).is_none());
}
