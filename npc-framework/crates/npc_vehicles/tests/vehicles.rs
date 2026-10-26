//! Wave F2: `npc_vehicles` on toy content (seats, mounting, crew following,
//! ByCrew driving, Direct vehicles, unload/dismount placement, determinism).

use bevy_app::App;
use bevy_ecs::prelude::*;
use npc_ai::{
    Abort, AiRegistry, BbKey, BbValue, Blackboard, Brain, ParamValue, Status, StopReason,
};
use npc_interaction::{Interact, InteractionPlugin, Interactor};
use npc_jobs::NpcJobsPlugin;
use npc_kernel::hash::state_hash;
use npc_kernel::journal::{JournalEntry, JournalKind};
use npc_kernel::registry::{ContentHash, KernelAppExt};
use npc_kernel::spatial::{CellCoord, CellFlags, CellInfo, GridBounds, SpatialGrid, SpatialPlugin};
use npc_kernel::{
    CommandError, ContentCommand, CurrentOrder, Journal, JournalPlugin, NpcKernelPlugin, Order,
    SimCommand, SimId, SimPos, run_ticks, spawn_sim, submit,
};
use npc_locomotion::{MoveIntent, MoveStatus, StraightLineMover, set_move_intent};
use npc_nav::{NavAgent, NavPlugin};
use npc_vehicles::{
    Dismount, Mount, MountedIn, Occupants, Seat, SeatPreference, SeatRole, SeatSlot, Unload,
    Vehicle, VehicleConfig, VehiclesPlugin, can_move, command_vehicle_to, set_drive_goal,
    vehicle_json,
};

const WALL: CellInfo = CellInfo {
    flags: CellFlags {
        blocking: true,
        occluding: true,
        tags: 0,
    },
    extra_cost: 0,
};

fn hold_start(_: &mut World, _: Entity, _: &ParamValue) -> Status {
    Status::Running
}
fn hold_stop(_: &mut World, _: Entity, _: StopReason) {}

fn base_app(configure: impl FnOnce(&mut App)) -> App {
    let mut app = App::new();
    app.insert_resource(SpatialGrid::new(1000, GridBounds::centered(32, 32)));
    configure(&mut app);
    app.add_plugins((
        NpcKernelPlugin::default(),
        SpatialPlugin,
        NpcJobsPlugin,
        StraightLineMover,
        NavPlugin,
        InteractionPlugin::default(),
        VehiclesPlugin,
        JournalPlugin::default(),
    ))
    .register_action("Hold", hold_start, hold_stop)
    .register_tree("crew", |b| {
        b.selector("Root", |b| {
            b.guard("Driver", "HasDriveGoal", Abort::LowerPriority, |b| {
                b.leaf("Drive", "DriveVehicle", ());
            });
            b.leaf("Hold", "Hold", ());
        });
    })
    .register_tree("board", |b| {
        b.sequence("Root", |b| {
            b.leaf("Board", "Interact", "mount");
            b.leaf("Hold", "Hold", ());
        });
    });
    app.finish();
    app.cleanup();
    app
}

fn app() -> App {
    base_app(|_| {})
}

fn actor(app: &mut App, pos: SimPos) -> (SimId, Entity) {
    spawn_sim(app.world_mut(), (pos, Interactor))
}

fn vehicle(app: &mut App, pos: SimPos, v: Vehicle) -> (SimId, Entity) {
    spawn_sim(app.world_mut(), (pos, v))
}

fn cart() -> Vehicle {
    Vehicle::by_crew(100)
        .seat(Seat::driver())
        .seat(Seat::passenger().offset(0, 500))
}

fn send<C: ContentCommand>(app: &mut App, cmd: &C) -> Result<(), CommandError> {
    submit(app.world_mut(), None, SimCommand::content(cmd)).map(|_| ())
}

fn run(app: &mut App, n: u64) {
    run_ticks(app.world_mut(), n);
}

fn pos(app: &App, e: Entity) -> SimPos {
    *app.world().get::<SimPos>(e).unwrap()
}

fn mounted(app: &App, e: Entity) -> Option<MountedIn> {
    app.world().get::<MountedIn>(e).copied()
}

fn journal(app: &App) -> Vec<JournalEntry> {
    app.world()
        .resource::<Journal>()
        .entries
        .iter()
        .cloned()
        .collect()
}

fn reason(err: CommandError) -> String {
    match err {
        CommandError::Unavailable { reason } => reason,
        other => panic!("expected Unavailable, got {other:?}"),
    }
}

fn wall(app: &mut App, cells: &[(i32, i32)]) {
    let mut grid = app.world_mut().resource_mut::<SpatialGrid>();
    for &(x, z) in cells {
        grid.set_static(CellCoord::new(x, z), WALL);
    }
}

fn brain(app: &App, tree: &str) -> Brain {
    Brain {
        tree: app.world().resource::<AiRegistry>().tree_id(tree).unwrap(),
    }
}

fn cell_of(app: &App, p: SimPos) -> CellCoord {
    app.world().resource::<SpatialGrid>().cell_of(p)
}

#[test]
fn mount_and_dismount_with_seat_roles_and_capacity() {
    let mut app = app();
    let (v, ve) = vehicle(&mut app, SimPos::new(500, 500), cart());
    let (a, ae) = actor(&mut app, SimPos::new(1500, 500));
    let (b, be) = actor(&mut app, SimPos::new(500, 1500));
    let (c, _) = actor(&mut app, SimPos::new(-500, 500));
    let (far, _) = actor(&mut app, SimPos::new(9500, 500));

    assert!(reason(send(&mut app, &Mount::new(far, v)).unwrap_err()).starts_with("too far"));
    assert_eq!(
        reason(send(&mut app, &Mount::new(a, v).as_role(SeatRole::Gunner)).unwrap_err()),
        "no Gunner seat"
    );
    send(&mut app, &Mount::new(a, v).as_role(SeatRole::Driver)).unwrap();
    run(&mut app, 1);
    assert_eq!(
        mounted(&app, ae),
        Some(MountedIn {
            vehicle: v,
            seat: 0
        })
    );
    assert_eq!(pos(&app, ae), SimPos::new(500, 500), "on the driver seat");

    assert_eq!(
        reason(send(&mut app, &Mount::new(c, v).as_role(SeatRole::Driver)).unwrap_err()),
        "no free Driver seat"
    );
    send(&mut app, &Mount::new(b, v)).unwrap();
    run(&mut app, 1);
    assert_eq!(
        mounted(&app, be),
        Some(MountedIn {
            vehicle: v,
            seat: 1
        })
    );
    assert_eq!(pos(&app, be), SimPos::new(500, 1000), "seat offset");
    assert_eq!(
        app.world().get::<Occupants>(ve).unwrap().seats,
        vec![SeatSlot::Taken(a), SeatSlot::Taken(b)]
    );
    assert_eq!(
        reason(send(&mut app, &Mount::new(c, v)).unwrap_err()),
        "no free seat",
        "capacity"
    );
    // A mounted unit cannot mount again; a vehicle cannot be mounted by the
    // generic Interact command either once full.
    assert!(
        reason(send(&mut app, &Interact::new(c, "mount", v)).unwrap_err()).contains("no free seat")
    );

    send(&mut app, &Dismount::new(a)).unwrap();
    run(&mut app, 1);
    assert_eq!(mounted(&app, ae), None);
    let at = pos(&app, ae);
    let grid = app.world().resource::<SpatialGrid>();
    let cell = grid.cell_of(at);
    assert_eq!(at, grid.center(cell), "on a cell centre");
    assert_ne!(cell, CellCoord::new(0, 0), "next to the vehicle, not on it");
    assert!(
        (cell.x).abs() <= 1 && (cell.z).abs() <= 1,
        "adjacent: {cell:?}"
    );
    assert_eq!(
        app.world().get::<Occupants>(ve).unwrap().slot(0),
        SeatSlot::Free
    );
    assert!(
        reason(send(&mut app, &Dismount::new(a)).unwrap_err()).contains("not mounted"),
        "only mounted units dismount"
    );

    let kinds: Vec<(SimId, String)> = journal(&app)
        .into_iter()
        .filter_map(|e| match e.kind {
            JournalKind::Mounted { role, .. } => Some((e.unit.unwrap(), format!("mount {role}"))),
            JournalKind::Dismounted { seat, .. } => {
                Some((e.unit.unwrap(), format!("dismount {seat}")))
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        kinds,
        vec![
            (a, "mount Driver".to_owned()),
            (b, "mount Passenger".to_owned()),
            (a, "dismount 0".to_owned()),
        ]
    );
}

#[test]
fn seat_preference_picks_its_role_or_falls_back() {
    let mut app = app();
    let (v, ve) = vehicle(
        &mut app,
        SimPos::new(500, 500),
        Vehicle::direct(50)
            .seat(Seat::passenger())
            .seat(Seat::gunner())
            .seat(Seat::passenger()),
    );
    let (g, ge) = actor(&mut app, SimPos::new(1500, 500));
    app.world_mut()
        .entity_mut(ge)
        .insert(SeatPreference(SeatRole::Gunner));
    let (h, he) = actor(&mut app, SimPos::new(1500, 500));
    app.world_mut()
        .entity_mut(he)
        .insert(SeatPreference(SeatRole::Gunner));
    send(&mut app, &Interact::new(g, "mount", v)).unwrap();
    run(&mut app, 1);
    send(&mut app, &Interact::new(h, "mount", v)).unwrap();
    run(&mut app, 1);
    assert_eq!(mounted(&app, ge).map(|m| m.seat), Some(1), "its role");
    assert_eq!(mounted(&app, he).map(|m| m.seat), Some(0), "fallback");
    assert_eq!(
        app.world().get::<Occupants>(ve).unwrap().units(),
        vec![g, h]
    );
}

/// Two units start mounting the last seat in the same tick: the claim goes
/// by `SimId`, not by command order; the loser's interaction is cancelled.
#[test]
fn seat_contention_is_won_by_the_lower_sim_id() {
    let race = |via_tree: bool| {
        let mut app = base_app(|app| {
            app.insert_resource(VehicleConfig {
                mount_ticks: 3,
                ..Default::default()
            });
        });
        let (v, ve) = vehicle(
            &mut app,
            SimPos::new(500, 500),
            Vehicle::direct(50).seat(Seat::passenger()),
        );
        let (x, xe) = actor(&mut app, SimPos::new(1500, 500));
        let (y, ye) = actor(&mut app, SimPos::new(-500, 500));
        if via_tree {
            // Brains inserted higher SimId first: thinking goes by SimId anyway.
            for e in [ye, xe] {
                let b = brain(&app, "board");
                app.world_mut().entity_mut(e).insert(b);
                app.world_mut()
                    .get_mut::<Blackboard>(e)
                    .unwrap()
                    .set(BbKey(7), BbValue::U32(v.0));
            }
        } else {
            // Higher SimId's command first.
            send(&mut app, &Interact::new(y, "mount", v)).unwrap();
            send(&mut app, &Interact::new(x, "mount", v)).unwrap();
        }
        run(&mut app, 1);
        assert_eq!(
            app.world().get::<Occupants>(ve).unwrap().seats,
            vec![SeatSlot::Claimed(x)],
            "claimed by the lower SimId"
        );
        let cancelled = journal(&app).into_iter().any(|e| {
            e.unit == Some(y)
                && matches!(&e.kind, JournalKind::Interacted { verb, outcome, reason, .. }
                    if verb == "mount" && outcome == "cancelled"
                        && reason.as_deref() == Some("no free seat"))
        });
        assert!(cancelled, "the loser is cancelled with the reason");
        if via_tree {
            assert_eq!(
                app.world().get::<npc_ai::ActiveLeaf>(ye).map(|l| l.status),
                Some(Status::Failure),
                "the loser's Interact leaf fails"
            );
        }
        run(&mut app, 3);
        assert_eq!(mounted(&app, xe).map(|m| m.vehicle), Some(v));
        assert_eq!(mounted(&app, ye), None);
        state_hash(app.world_mut())
    };
    race(false);
    race(true);
}

#[test]
fn mounted_actor_follows_the_vehicle_exactly() {
    let mut app = app();
    let (_, ve) = vehicle(
        &mut app,
        SimPos::new(500, 500),
        Vehicle::direct(70).seat(Seat::passenger().offset(300, -200)),
    );
    let v = *app.world().get::<SimId>(ve).unwrap();
    let (a, ae) = actor(&mut app, SimPos::new(1500, 500));
    send(&mut app, &Mount::new(a, v)).unwrap();
    run(&mut app, 1);
    command_vehicle_to(app.world_mut(), ve, SimPos::new(8500, 3500)).unwrap();
    for tick in 0..200 {
        if tick == 10 {
            // Its own locomotion is suspended: this intent is dropped.
            set_move_intent(
                app.world_mut(),
                ae,
                MoveIntent::to(SimPos::new(-5000, -5000), 200),
            );
        }
        run(&mut app, 1);
        let vp = pos(&app, ve);
        assert_eq!(
            pos(&app, ae),
            SimPos::new(vp.x + 300, vp.z - 200),
            "tick {tick}"
        );
        assert!(app.world().get::<MoveIntent>(ae).is_none());
    }
    assert_eq!(pos(&app, ve), SimPos::new(8500, 3500));
    assert_eq!(
        app.world().get::<MoveStatus>(ve),
        Some(&MoveStatus::Arrived)
    );
}

#[test]
fn by_crew_vehicle_without_a_driver_cannot_move() {
    let mut app = app();
    let (v, ve) = vehicle(&mut app, SimPos::new(500, 500), cart());
    let (p, pe) = actor(&mut app, SimPos::new(1500, 500));
    send(&mut app, &Mount::new(p, v).as_role(SeatRole::Passenger)).unwrap();
    run(&mut app, 1);
    assert_eq!(can_move(app.world(), ve), Err("no driver".to_owned()));
    assert_eq!(
        command_vehicle_to(app.world_mut(), ve, SimPos::new(5500, 500)),
        Err("no driver".to_owned())
    );
    // Someone writes the intent anyway: it is dropped, the vehicle stays.
    set_move_intent(
        app.world_mut(),
        ve,
        MoveIntent::to(SimPos::new(5500, 500), 100),
    );
    run(&mut app, 5);
    assert_eq!(pos(&app, ve), SimPos::new(500, 500));
    assert_eq!(pos(&app, pe), SimPos::new(500, 1000));
    assert!(app.world().get::<MoveIntent>(ve).is_none());
    assert_eq!(
        app.world().get::<MoveStatus>(ve),
        Some(&MoveStatus::Blocked { ticks: 1 })
    );
    let stalls: Vec<(SimId, String)> = journal(&app)
        .into_iter()
        .filter_map(|e| match e.kind {
            JournalKind::VehicleStalled { reason } => Some((e.unit.unwrap(), reason)),
            _ => None,
        })
        .collect();
    assert_eq!(stalls, vec![(v, "no driver".to_owned())]);
}

/// The cart, a driver running the `crew` tree and a passenger.
fn crewed_cart(app: &mut App) -> (SimId, Entity, Entity, Entity) {
    let (v, ve) = vehicle(app, SimPos::new(500, 500), cart());
    let (d, de) = actor(app, SimPos::new(1500, 500));
    let (p, pe) = actor(app, SimPos::new(-500, 500));
    let b = brain(app, "crew");
    app.world_mut()
        .entity_mut(de)
        .insert((b, CurrentOrder::default()));
    send(app, &Mount::new(d, v).as_role(SeatRole::Driver)).unwrap();
    send(app, &Mount::new(p, v)).unwrap();
    run(app, 1);
    (v, ve, de, pe)
}

/// Runs until the vehicle has moved and stopped again (the driver may need
/// a tick to pick its next goal up).
fn drive_until_parked(app: &mut App, ve: Entity, pe: Entity, max: u32) {
    let mut moving = false;
    for _ in 0..max {
        run(app, 1);
        let vp = pos(app, ve);
        assert_eq!(
            pos(app, pe),
            SimPos::new(vp.x, vp.z + 500),
            "passenger rides"
        );
        let idle = app
            .world()
            .get::<MoveStatus>(ve)
            .is_none_or(|s| *s == MoveStatus::Idle);
        moving |= app.world().get::<MoveIntent>(ve).is_some();
        if moving && app.world().get::<MoveIntent>(ve).is_none() && idle {
            return;
        }
    }
    panic!("the vehicle did not park");
}

#[test]
fn by_crew_vehicle_is_driven_by_orders_given_to_its_driver() {
    let mut app = app();
    let (_, ve, de, pe) = crewed_cart(&mut app);
    let driver = *app.world().get::<SimId>(de).unwrap();
    // The kernel's standing order, given to the driver.
    submit(
        app.world_mut(),
        None,
        SimCommand::Order {
            unit: driver.0,
            order: Order::GoTo { pos: (6500, 500) },
        },
    )
    .unwrap();
    drive_until_parked(&mut app, ve, pe, 200);
    assert_eq!(pos(&app, ve), SimPos::new(6500, 500));
    assert_eq!(
        pos(&app, de),
        SimPos::new(6500, 500),
        "the driver rides too"
    );
    assert_eq!(
        app.world().get::<CurrentOrder>(de),
        Some(&CurrentOrder(None)),
        "the order is done"
    );

    // The vehicle-order hook: `command_vehicle_to` gives the driver a goal.
    command_vehicle_to(app.world_mut(), ve, SimPos::new(6500, 4500)).unwrap();
    drive_until_parked(&mut app, ve, pe, 200);
    assert_eq!(pos(&app, ve), SimPos::new(6500, 4500));
    assert!(app.world().get::<npc_vehicles::DriveGoal>(de).is_none());

    // The blackboard slot.
    let slot = VehicleConfig::default().goal_slot;
    app.world_mut()
        .get_mut::<Blackboard>(de)
        .unwrap()
        .set(BbKey(slot), BbValue::Pos(2500, 4500));
    drive_until_parked(&mut app, ve, pe, 200);
    assert_eq!(pos(&app, ve), SimPos::new(2500, 4500));
    assert_eq!(
        app.world().get::<Blackboard>(de).unwrap().get(BbKey(slot)),
        BbValue::None
    );

    let stalls = journal(&app)
        .iter()
        .filter(|e| matches!(e.kind, JournalKind::VehicleStalled { .. }))
        .count();
    assert_eq!(stalls, 0);
    let json = vehicle_json(app.world(), ve).unwrap();
    assert_eq!(json["vehicle"]["control"], "ByCrew");
    assert_eq!(json["vehicle"]["driver"], driver.0);
    assert_eq!(json["vehicle"]["can_move"], true);
}

#[test]
fn driver_leaving_stops_the_vehicle() {
    let mut app = app();
    let (_, ve, de, pe) = crewed_cart(&mut app);
    set_drive_goal(app.world_mut(), de, SimPos::new(9500, 500));
    run(&mut app, 10);
    let at = pos(&app, ve);
    assert!(at.x > 500, "moving");
    let driver = *app.world().get::<SimId>(de).unwrap();
    send(&mut app, &Dismount::new(driver)).unwrap();
    run(&mut app, 10);
    assert_eq!(mounted(&app, de), None);
    let stopped = pos(&app, ve);
    run(&mut app, 10);
    assert_eq!(pos(&app, ve), stopped, "no driver: no movement");
    assert_eq!(pos(&app, pe), SimPos::new(stopped.x, stopped.z + 500));
}

#[test]
fn direct_vehicle_moves_through_nav_with_its_crew() {
    let mut app = app();
    // A wall across the straight line; nav goes around it.
    let cells: Vec<(i32, i32)> = (-4..=4).map(|z| (4, z)).collect();
    wall(&mut app, &cells);
    let (v, ve) = vehicle(
        &mut app,
        SimPos::new(500, 500),
        Vehicle::direct(120)
            .seat(Seat::driver().optional())
            .seat(Seat::passenger().offset(0, 400)),
    );
    app.world_mut().entity_mut(ve).insert(NavAgent);
    let (a, ae) = actor(&mut app, SimPos::new(1500, 500));
    let (b, be) = actor(&mut app, SimPos::new(-500, 500));
    send(&mut app, &Mount::new(a, v)).unwrap();
    send(&mut app, &Mount::new(b, v)).unwrap();
    run(&mut app, 1);
    command_vehicle_to(app.world_mut(), ve, SimPos::new(8500, 500)).unwrap();
    let mut max_dz = 0;
    for _ in 0..400 {
        run(&mut app, 1);
        let vp = pos(&app, ve);
        max_dz = max_dz.max((vp.z - 500).abs());
        assert!(
            !app.world()
                .resource::<SpatialGrid>()
                .is_blocking(cell_of(&app, vp))
        );
        assert_eq!(pos(&app, ae), vp);
        assert_eq!(pos(&app, be), SimPos::new(vp.x, vp.z + 400));
        if app.world().get::<MoveStatus>(ve) == Some(&MoveStatus::Arrived) {
            break;
        }
    }
    assert_eq!(pos(&app, ve), SimPos::new(8500, 500));
    assert!(max_dz >= 4000, "went around the wall ({max_dz})");
}

#[test]
fn unload_places_every_occupant_on_its_own_free_cell() {
    let mut app = app();
    // Walls on three sides: only the cell to the north is open nearby.
    wall(&mut app, &[(1, 0), (0, 1), (-1, 0)]);
    let (v, ve) = vehicle(
        &mut app,
        SimPos::new(500, 500),
        Vehicle::direct(50).seats(4, Seat::passenger()),
    );
    let mut riders = Vec::new();
    for _ in 0..4 {
        let (id, e) = actor(&mut app, SimPos::new(500, -500));
        send(&mut app, &Mount::new(id, v)).unwrap();
        riders.push((id, e));
    }
    run(&mut app, 1);
    assert_eq!(app.world().get::<Occupants>(ve).unwrap().units().len(), 4);
    send(&mut app, &Unload::new(v)).unwrap();
    run(&mut app, 1);
    let grid = app.world().resource::<SpatialGrid>();
    let mut cells = Vec::new();
    for &(_, e) in &riders {
        assert_eq!(mounted(&app, e), None);
        let p = pos(&app, e);
        let c = grid.cell_of(p);
        assert_eq!(p, grid.center(c));
        assert!(!grid.is_blocking(c), "never on a wall: {c:?}");
        assert_ne!(c, CellCoord::new(0, 0));
        cells.push(c);
    }
    assert_eq!(
        cells[0],
        CellCoord::new(0, -1),
        "the nearest open cell first"
    );
    let mut unique = cells.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(unique.len(), 4, "one cell each: {cells:?}");
    assert!(app.world().get::<Occupants>(ve).unwrap().is_empty());
    assert!(
        reason(send(&mut app, &Unload::new(v)).unwrap_err()).contains("empty"),
        "nothing left to unload"
    );
}

#[test]
fn dismount_next_to_walls_never_goes_through_them() {
    let mut app = app();
    // Boxed in on all four sides: the diagonals are only reachable through
    // wall corners, so there is no way out.
    wall(&mut app, &[(1, 0), (0, 1), (-1, 0), (0, -1)]);
    let (v, _) = vehicle(
        &mut app,
        SimPos::new(500, 500),
        Vehicle::by_crew(50).seat(Seat::driver()),
    );
    let (a, ae) = actor(&mut app, SimPos::new(500, 500));
    send(&mut app, &Mount::new(a, v)).unwrap();
    run(&mut app, 1);
    assert!(mounted(&app, ae).is_some());
    assert_eq!(
        reason(send(&mut app, &Dismount::new(a)).unwrap_err()),
        "no free cell to dismount"
    );
    // Open the west side: that is the way out.
    app.world_mut()
        .resource_mut::<SpatialGrid>()
        .set_static(CellCoord::new(-1, 0), CellInfo::default());
    send(&mut app, &Dismount::new(a)).unwrap();
    run(&mut app, 1);
    assert_eq!(mounted(&app, ae), None);
    assert_eq!(pos(&app, ae), SimPos::new(-500, 500));
}

/// A full scenario: mounting, driving by order, a stall, unloading.
fn scenario() -> (u64, String) {
    let mut app = app();
    wall(&mut app, &[(3, 1), (3, 2)]);
    let (v, ve, de, pe) = crewed_cart(&mut app);
    let driver = *app.world().get::<SimId>(de).unwrap();
    submit(
        app.world_mut(),
        None,
        SimCommand::Order {
            unit: driver.0,
            order: Order::GoTo { pos: (5500, 500) },
        },
    )
    .unwrap();
    drive_until_parked(&mut app, ve, pe, 200);
    send(&mut app, &Dismount::new(driver)).unwrap();
    run(&mut app, 1);
    set_move_intent(app.world_mut(), ve, MoveIntent::to(SimPos::new(0, 0), 100));
    run(&mut app, 2);
    send(&mut app, &Unload::new(v)).unwrap();
    run(&mut app, 3);
    let journal: Vec<String> = journal(&app)
        .iter()
        .map(|e| serde_json::to_string(e).unwrap())
        .collect();
    (state_hash(app.world_mut()), journal.join("\n"))
}

#[test]
fn vehicles_are_deterministic() {
    let (h1, j1) = scenario();
    let (h2, j2) = scenario();
    assert_eq!(h1, h2);
    assert_eq!(j1, j2);
    for kind in ["Mounted", "Dismounted", "VehicleStalled"] {
        assert!(j1.contains(kind), "{kind} journaled");
    }
}

#[test]
fn without_the_plugin_nothing_is_registered() {
    let manifest = |with: bool| {
        let mut app = App::new();
        app.add_plugins((
            NpcKernelPlugin::default(),
            SpatialPlugin,
            NpcJobsPlugin,
            StraightLineMover,
            InteractionPlugin::default(),
        ));
        if with {
            app.add_plugins(VehiclesPlugin);
        }
        app.finish();
        app.cleanup();
        app.world().resource::<ContentHash>().clone()
    };
    let off = manifest(false);
    let on = manifest(true);
    let names =
        |h: &ContentHash| -> Vec<String> { h.manifest.iter().map(|(_, n)| n.clone()).collect() };
    for name in [
        "mount",
        "dismount",
        "Mount",
        "Unload",
        "vehicle",
        "DriveVehicle",
    ] {
        assert!(!names(&off).contains(&name.to_owned()), "{name} off");
        assert!(names(&on).contains(&name.to_owned()), "{name} on");
    }
    assert_ne!(off.hash, on.hash);
}

#[test]
#[should_panic(expected = "needs InteractionPlugin")]
fn the_plugin_needs_interaction() {
    let mut app = App::new();
    app.add_plugins((NpcKernelPlugin::default(), VehiclesPlugin));
    app.finish();
}
