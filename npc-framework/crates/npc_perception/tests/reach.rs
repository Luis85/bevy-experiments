//! Plan Task 27 (spec F3, proposals 007 and 011): the opt-in reach sense
//! ([`ReachSense`]): all round, within its radius, never through walls; it
//! feeds `PerceptionMemory` like vision, and it is snapshot-only (no hash).

use bevy_app::App;
use bevy_ecs::prelude::*;
use npc_kernel::hash::{SimHashRegistry, state_hash};
use npc_kernel::registry::content_hash;
use npc_kernel::snapshot::SnapshotRegistry;
use npc_kernel::spatial::{GridBounds, GridOccluder, SpatialGrid, SpatialPlugin};
use npc_kernel::{JournalPlugin, NpcKernelPlugin, SimId, SimPos, run_ticks, spawn_sim};
use npc_perception::{
    Facing, FixedFacing, Perceivable, PerceptionConfig, PerceptionMemory, PerceptionPlugin,
    REACH_SENSE, ReachSense, SenseKind, Sensors,
};

const THING: u32 = 1;
/// Facing +z.
const NORTH: Facing = Facing::new(0, 1);

fn app() -> App {
    let mut app = App::new();
    app.insert_resource(SpatialGrid::new(1000, GridBounds::centered(64, 64)))
        .insert_resource(PerceptionConfig {
            stagger: 1,
            ..Default::default()
        });
    app.add_plugins((
        NpcKernelPlugin::default(),
        SpatialPlugin,
        PerceptionPlugin,
        JournalPlugin::default(),
    ));
    app.finish();
    app.cleanup();
    app
}

/// A human-like observer: 15 m / 60° sight along `facing` (fixed), plus
/// reach when `reach` is given.
fn observer(app: &mut App, pos: SimPos, facing: Facing, reach: Option<i32>) -> Entity {
    let (_, e) = spawn_sim(
        app.world_mut(),
        (pos, facing, FixedFacing, Sensors::new().vision(15_000, 60)),
    );
    if let Some(radius_mm) = reach {
        app.world_mut()
            .entity_mut(e)
            .insert(ReachSense { radius_mm });
    }
    e
}

fn thing(app: &mut App, pos: SimPos) -> SimId {
    spawn_sim(app.world_mut(), (pos, Perceivable::new(THING))).0
}

fn memory(app: &App, e: Entity) -> PerceptionMemory {
    app.world().get::<PerceptionMemory>(e).unwrap().clone()
}

/// The sense a target is sensed by right now (`None`: not sensed).
fn sensed_via(app: &App, e: Entity, target: SimId) -> Option<SenseKind> {
    memory(app, e)
        .get(target)
        .filter(|p| p.sensed)
        .map(|p| p.via)
}

#[test]
fn chest_behind_in_reach_is_sensed() {
    let mut app = app();
    let with = observer(&mut app, SimPos::new(500, 500), NORTH, Some(1500));
    let without = observer(&mut app, SimPos::new(30_500, 500), NORTH, None);
    // Half a metre behind each, and one just beyond reach.
    let chest = thing(&mut app, SimPos::new(500, 0));
    let other = thing(&mut app, SimPos::new(30_500, 0));
    let beyond = thing(&mut app, SimPos::new(500, -1001));
    // In view and in reach: vision wins.
    let ahead = thing(&mut app, SimPos::new(500, 1500));
    run_ticks(app.world_mut(), 1);
    assert_eq!(sensed_via(&app, with, chest), Some(SenseKind::Reach));
    let p = *memory(&app, with).get(chest).unwrap();
    assert_eq!(
        (p.pos, p.kinds, p.last_seen_tick),
        (SimPos::new(500, 0), THING, 0)
    );
    assert_eq!(
        sensed_via(&app, with, beyond),
        None,
        "1.501 m: out of reach"
    );
    assert_eq!(sensed_via(&app, with, ahead), Some(SenseKind::Vision));
    assert_eq!(sensed_via(&app, without, other), None, "no reach sense");
    assert_eq!(SenseKind::Reach.name(), "reach");
}

#[test]
fn reach_keeps_a_thing_sensed_after_turning() {
    let mut app = app();
    let eye = observer(&mut app, SimPos::new(500, 500), NORTH, Some(1500));
    let station = thing(&mut app, SimPos::new(500, 1800));
    run_ticks(app.world_mut(), 1);
    assert_eq!(sensed_via(&app, eye, station), Some(SenseKind::Vision));
    *app.world_mut().get_mut::<Facing>(eye).unwrap() = Facing::new(0, -1);
    let forget = app
        .world()
        .resource::<PerceptionConfig>()
        .forget_after_ticks;
    run_ticks(app.world_mut(), forget + 10);
    assert_eq!(sensed_via(&app, eye, station), Some(SenseKind::Reach));
}

/// Review fix 6: two wall cells touching only at a corner, (1, 0) and
/// (0, 1), between the eye in cell (0, 0) and a thing 1.2 m away in cell
/// (1, 1). Neither a segment just beside the corner nor one through the
/// exact corner point slips through the diagonal gap.
#[test]
fn reach_does_not_slip_between_diagonal_walls() {
    // `(eye, thing)`: beside the corner (y = x - 100), then exactly through
    // the corner (1000, 1000) (y = x).
    let cases = [
        (SimPos::new(950, 850), SimPos::new(1800, 1700)),
        (SimPos::new(900, 900), SimPos::new(1750, 1750)),
    ];
    for (walls, (at, there)) in [true, false]
        .into_iter()
        .flat_map(|w| cases.into_iter().map(move |c| (w, c)))
    {
        let mut app = app();
        // Facing away (south-west): only reach could sense it.
        let eye = observer(&mut app, at, Facing::new(-1, -1), Some(1500));
        if walls {
            for cell in [SimPos::new(1500, 500), SimPos::new(500, 1500)] {
                spawn_sim(app.world_mut(), (cell, GridOccluder::default()));
            }
        }
        let target = thing(&mut app, there);
        assert!(at.dist2(there) <= 1500 * 1500 && at.dist2(there) >= 1200 * 1200);
        run_ticks(app.world_mut(), 1);
        let want = if walls { None } else { Some(SenseKind::Reach) };
        assert_eq!(
            sensed_via(&app, eye, target),
            want,
            "walls {walls}, {at:?} -> {there:?}"
        );
    }
}

#[test]
fn reach_sense_does_not_cross_walls() {
    let mut app = app();
    // Facing away from both: only reach can sense them.
    let eye = observer(
        &mut app,
        SimPos::new(900, 500),
        Facing::new(-1, 0),
        Some(1500),
    );
    // A wall in the next cell east; a thing just past it, 1.2 m away.
    spawn_sim(
        app.world_mut(),
        (SimPos::new(1500, 500), GridOccluder::default()),
    );
    let past_wall = thing(&mut app, SimPos::new(2100, 500));
    // Same room, half a metre behind (south) of the eye.
    let same_room = thing(&mut app, SimPos::new(900, 0));
    run_ticks(app.world_mut(), 1);
    assert_eq!(sensed_via(&app, eye, past_wall), None, "through a wall");
    assert_eq!(sensed_via(&app, eye, same_room), Some(SenseKind::Reach));
}

#[test]
fn reach_sense_is_snapshot_only() {
    let mut a = app();
    let hashes = a.world().resource::<SimHashRegistry>();
    assert!(
        hashes.components.iter().all(|c| c.name != REACH_SENSE),
        "reach_sense must not join the state hash"
    );
    let snapshots = a.world().resource::<SnapshotRegistry>();
    assert!(snapshots.component_names().contains(&REACH_SENSE));

    // Same world with and without the component: same content and state
    // hash while nothing is in reach.
    let mut b = app();
    let ea = observer(&mut a, SimPos::new(500, 500), NORTH, Some(1500));
    let _ = ea;
    observer(&mut b, SimPos::new(500, 500), NORTH, None);
    assert_eq!(content_hash(a.world_mut()), content_hash(b.world_mut()));
    run_ticks(a.world_mut(), 4);
    run_ticks(b.world_mut(), 4);
    assert_eq!(state_hash(a.world_mut()), state_hash(b.world_mut()));
}
