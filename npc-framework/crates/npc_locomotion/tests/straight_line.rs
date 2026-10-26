//! Plan Task C1: the locomotion seam and the straight-line mover.

use bevy_app::App;
use bevy_ecs::prelude::*;
use npc_kernel::hash::state_hash;
use npc_kernel::spatial::{CellCoord, CellFlags, CellInfo, SpatialGrid, SpatialPlugin};
use npc_kernel::{NpcKernelPlugin, SimPos, run_ticks, spawn_sim};
use npc_locomotion::{
    ExternalMover, MoveIntent, MoveStatus, StraightLineMover, register_locomotion_hash,
    set_move_intent,
};

fn app() -> App {
    let mut app = App::new();
    app.add_plugins((NpcKernelPlugin::default(), SpatialPlugin, StraightLineMover));
    app.finish();
    app.cleanup();
    app
}

fn status(app: &App, e: Entity) -> MoveStatus {
    *app.world().get::<MoveStatus>(e).unwrap()
}

fn pos(app: &App, e: Entity) -> SimPos {
    *app.world().get::<SimPos>(e).unwrap()
}

#[test]
fn walks_straight_and_arrives_exactly() {
    let mut app = app();
    let (_, e) = spawn_sim(app.world_mut(), SimPos::new(0, 0));
    set_move_intent(
        app.world_mut(),
        e,
        MoveIntent::to(SimPos::new(300, 400), 60),
    );
    run_ticks(app.world_mut(), 1);
    assert_eq!(pos(&app, e), SimPos::new(36, 48));
    assert_eq!(status(&app, e), MoveStatus::Moving);
    // 500 mm at 60 mm/tick: arrives in tick 9, in the same tick it gets there.
    run_ticks(app.world_mut(), 7);
    assert_eq!(status(&app, e), MoveStatus::Moving);
    run_ticks(app.world_mut(), 1);
    assert_eq!(pos(&app, e), SimPos::new(300, 400));
    assert_eq!(status(&app, e), MoveStatus::Arrived);
}

#[test]
fn tolerance_stops_short() {
    let mut app = app();
    let (_, e) = spawn_sim(app.world_mut(), SimPos::new(0, 0));
    set_move_intent(
        app.world_mut(),
        e,
        MoveIntent {
            goal: SimPos::new(1000, 0),
            speed_mm_per_tick: 100,
            tolerance_mm: 250,
        },
    );
    run_ticks(app.world_mut(), 8);
    assert_eq!(pos(&app, e), SimPos::new(800, 0));
    assert_eq!(status(&app, e), MoveStatus::Arrived);
    run_ticks(app.world_mut(), 3);
    assert_eq!(
        pos(&app, e),
        SimPos::new(800, 0),
        "arrived: no further steps"
    );
}

#[test]
fn blocked_by_a_blocking_cell_and_resumes_when_cleared() {
    let mut app = app();
    let wall = CellInfo {
        flags: CellFlags {
            blocking: true,
            ..Default::default()
        },
        extra_cost: 0,
    };
    app.world_mut()
        .resource_mut::<SpatialGrid>()
        .set_static(CellCoord::new(1, 0), wall);
    let (_, e) = spawn_sim(app.world_mut(), SimPos::new(500, 500));
    set_move_intent(
        app.world_mut(),
        e,
        MoveIntent::to(SimPos::new(2500, 500), 100),
    );
    run_ticks(app.world_mut(), 4);
    assert_eq!(pos(&app, e), SimPos::new(900, 500));
    run_ticks(app.world_mut(), 1);
    assert_eq!(
        pos(&app, e),
        SimPos::new(900, 500),
        "never enters the blocked cell"
    );
    assert_eq!(status(&app, e), MoveStatus::Blocked { ticks: 1 });
    run_ticks(app.world_mut(), 9);
    assert_eq!(status(&app, e), MoveStatus::Blocked { ticks: 10 });

    app.world_mut()
        .resource_mut::<SpatialGrid>()
        .set_static(CellCoord::new(1, 0), CellInfo::default());
    run_ticks(app.world_mut(), 1);
    assert_eq!(pos(&app, e), SimPos::new(1000, 500));
    assert_eq!(status(&app, e), MoveStatus::Moving);
}

#[test]
fn external_movers_are_left_alone() {
    let mut app = app();
    let (_, e) = spawn_sim(app.world_mut(), (SimPos::new(0, 0), ExternalMover));
    set_move_intent(app.world_mut(), e, MoveIntent::to(SimPos::new(1000, 0), 60));
    run_ticks(app.world_mut(), 5);
    assert_eq!(pos(&app, e), SimPos::new(0, 0));
    assert_eq!(status(&app, e), MoveStatus::Moving, "untouched");
}

#[test]
fn intents_are_unhashed_unless_registered() {
    // Without registration: the state hash ignores intents (the village keeps
    // its hash); with it, they count.
    let hash_with = |register: bool, goal: i32| {
        let mut app = App::new();
        app.add_plugins((NpcKernelPlugin::default(), SpatialPlugin, StraightLineMover));
        if register {
            register_locomotion_hash(&mut app);
            register_locomotion_hash(&mut app); // idempotent
        }
        app.finish();
        let (_, e) = spawn_sim(app.world_mut(), SimPos::new(0, 0));
        set_move_intent(app.world_mut(), e, MoveIntent::to(SimPos::new(goal, 0), 0));
        state_hash(app.world_mut())
    };
    assert_eq!(hash_with(false, 1000), hash_with(false, 2000));
    assert_ne!(hash_with(true, 1000), hash_with(true, 2000));
}
