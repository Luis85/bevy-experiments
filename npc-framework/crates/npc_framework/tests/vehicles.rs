//! Wave F2: vehicles through the facade (`NpcPlugins::with_vehicles`, the
//! prelude) and opt-in (nothing changes without it).

#![cfg(feature = "vehicles")]

use bevy_app::App;
use npc_framework::kernel::registry::ContentHash;
use npc_framework::prelude::*;

fn app_with(plugins: NpcPlugins) -> App {
    let mut app = App::new();
    app.add_plugins(plugins.without_driver());
    app.finish();
    app.cleanup();
    app
}

fn names(app: &App) -> Vec<String> {
    app.world()
        .resource::<ContentHash>()
        .manifest
        .iter()
        .map(|(_, n)| n.clone())
        .collect()
}

#[test]
fn vehicles_are_opt_in() {
    let plain = app_with(NpcPlugins::new());
    for name in ["mount", "Mount", "vehicle", "DriveVehicle", "mounted_in"] {
        assert!(!names(&plain).contains(&name.to_owned()), "{name}");
    }
    let with = app_with(NpcPlugins::new().with_vehicles());
    for name in [
        "mount",
        "dismount",
        "Mount",
        "Unload",
        "vehicle",
        "DriveVehicle",
    ] {
        assert!(names(&with).contains(&name.to_owned()), "{name}");
    }
    // Asking for interaction as well adds it once.
    let both = app_with(NpcPlugins::new().with_interaction().with_vehicles());
    assert_eq!(
        both.world().resource::<ContentHash>().hash,
        with.world().resource::<ContentHash>().hash
    );
}

#[test]
fn a_cart_is_mounted_and_driven_through_the_facade() {
    let mut app = app_with(NpcPlugins::new().with_vehicles());
    let world = app.world_mut();
    let (v, ve) = spawn_sim(
        world,
        (
            SimPos::new(500, 500),
            Vehicle::by_crew(100).seat(Seat::driver()),
        ),
    );
    let (d, de) = spawn_sim(world, (SimPos::new(1500, 500), Interactor));
    submit(
        world,
        None,
        SimCommand::content(&Mount::new(d, v).as_role(SeatRole::Driver)),
    )
    .unwrap();
    run_ticks(world, 1);
    assert_eq!(
        world.get::<MountedIn>(de),
        Some(&MountedIn {
            vehicle: v,
            seat: 0
        })
    );
    assert_eq!(driver_of(world, ve).map(|(id, _)| id), Some(d));
    // No tree here: the hook only records the goal for the driver.
    command_vehicle_to(world, ve, SimPos::new(3500, 500)).unwrap();
    assert_eq!(
        world.get::<DriveGoal>(de),
        Some(&DriveGoal(SimPos::new(3500, 500)))
    );
}
