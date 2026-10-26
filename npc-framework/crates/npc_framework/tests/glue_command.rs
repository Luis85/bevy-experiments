//! Wave F4: the command glue (ADR 0011/0012).
//!
//! - command × vehicles: a `ByCrew` cart's order is forwarded to its driver,
//!   who drives (passengers ride along) and then unloads; a cart whose AI
//!   driver is not commandable takes the order itself (the driver's tree
//!   drives through `DriveGoal`); a `Direct` vehicle is ordered like a unit;
//!   `Mount` walks into reach, then mounts; riders cannot be moved;
//! - command × interaction: queued `Interact` orders pick up a key, unlock
//!   and open a door; a hopeless order fails at once with the verb's reason;
//!   the context-order resolvers;
//! - every pair's glue is registered only when both capabilities are on;
//! - determinism: two runs give the same state hash.

#![cfg(all(feature = "command", feature = "vehicles", feature = "interaction"))]

use bevy_app::App;
use bevy_ecs::prelude::*;
use npc_framework::glue::{context_door_order, context_item_order, context_vehicle_order};
use npc_framework::kernel::hash::state_hash;
use npc_framework::kernel::journal::JournalKind;
use npc_framework::kernel::registry::ContentHash;
use npc_framework::kernel::spatial::GridBounds;
use npc_framework::prelude::*;

fn hold_start(_: &mut World, _: Entity, _: &ParamValue) -> Status {
    Status::Running
}
fn hold_stop(_: &mut World, _: Entity, _: StopReason) {}

fn app_with(plugins: NpcPlugins) -> App {
    let mut app = App::new();
    app.insert_resource(SpatialGrid::new(1000, GridBounds::centered(64, 64)));
    app.add_plugins(plugins.seed(7).without_driver())
        .add_plugins(JournalPlugin::default());
    app.finish();
    app.cleanup();
    app
}

fn app() -> App {
    let mut app = App::new();
    app.insert_resource(SpatialGrid::new(1000, GridBounds::centered(64, 64)));
    app.add_plugins(
        NpcPlugins::new()
            .seed(7)
            .without_driver()
            .with_command()
            .with_vehicles(),
    )
    .add_plugins(JournalPlugin::default())
    .register_item_kind(ItemKindDef::new("gate key").weight_g(50).tag("key:gate"))
    .register_action("Hold", hold_start, hold_stop)
    .register_tree("crew", |b| {
        b.selector("Root", |b| {
            b.guard("Driver", "HasDriveGoal", Abort::LowerPriority, |b| {
                b.leaf("Drive", "DriveVehicle", ());
            });
            b.leaf("Hold", "Hold", ());
        });
    });
    app.finish();
    app.cleanup();
    app
}

const P1: PlayerId = PlayerId(1);

fn names(app: &App) -> Vec<String> {
    app.world()
        .resource::<ContentHash>()
        .manifest
        .iter()
        .map(|(_, n)| n.clone())
        .collect()
}

fn journal(app: &App) -> Vec<(Option<SimId>, JournalKind)> {
    app.world()
        .resource::<Journal>()
        .entries
        .iter()
        .map(|e| (e.unit, e.kind.clone()))
        .collect()
}

fn order(app: &mut App, cmd: &IssueOrder) {
    submit_order(app.world_mut(), cmd).unwrap_or_else(|e| panic!("{cmd:?}: {e}"));
}

fn pos(app: &App, e: Entity) -> SimPos {
    *app.world().get::<SimPos>(e).unwrap()
}

fn mount_now(app: &mut App, unit: SimId, vehicle: SimId, role: SeatRole) {
    submit(
        app.world_mut(),
        None,
        SimCommand::content(&Mount::new(unit, vehicle).as_role(role)),
    )
    .unwrap();
    run_ticks(app.world_mut(), 1);
}

/// A cart (commandable, player 1) with a driver seat and two passenger
/// seats, a driver (commandable unless `ai_driver`, then it runs the
/// `crew` tree) and two passengers, all aboard.
fn crewed_cart(app: &mut App, ai_driver: bool) -> (SimId, Entity, SimId, Vec<Entity>) {
    let w = app.world_mut();
    let (cart, ce) = spawn_sim(
        w,
        (
            SimPos::new(500, 500),
            Vehicle::by_crew(100)
                .seat(Seat::driver())
                .seats(2, Seat::passenger().offset(0, 500)),
            Commandable::new(100),
            Owner(P1),
        ),
    );
    let (driver, de) = spawn_sim(w, (SimPos::new(1500, 500), Interactor));
    if ai_driver {
        let tree = w.resource::<AiRegistry>().tree_id("crew").unwrap();
        w.entity_mut(de).insert(Brain { tree });
    } else {
        w.entity_mut(de).insert((Commandable::new(100), Owner(P1)));
    }
    let mut riders = vec![de];
    mount_now(app, driver, cart, SeatRole::Driver);
    for x in [0, 1] {
        let (p, pe) = spawn_sim(
            app.world_mut(),
            (SimPos::new(500 + x * 300, 1500), Interactor),
        );
        mount_now(app, p, cart, SeatRole::Passenger);
        riders.push(pe);
    }
    for &r in &riders {
        assert!(app.world().get::<MountedIn>(r).is_some(), "all aboard");
    }
    (cart, ce, driver, riders)
}

#[test]
fn a_bycrew_cart_is_driven_by_its_driver_then_unloaded() {
    let mut app = app();
    let (cart, ce, driver, riders) = crewed_cart(&mut app, false);
    order(
        &mut app,
        &IssueOrder::new(P1, [cart], UnitOrder::Move { pos: (8500, 500) }),
    );
    order(
        &mut app,
        &IssueOrder::new(P1, [cart], UnitOrder::Unload).queued(true),
    );
    // Forwarded: the driver holds both orders, the cart none.
    let de = riders[0];
    assert_eq!(app.world().get::<OrderQueue>(de).unwrap().len(), 0);
    run_ticks(app.world_mut(), 1);
    assert_eq!(app.world().get::<OrderQueue>(de).unwrap().len(), 2);
    assert!(app.world().get::<OrderQueue>(ce).unwrap().is_empty());
    run_ticks(app.world_mut(), 40);
    let mid = pos(&app, ce);
    assert!(mid.x > 2500 && mid.x < 8500, "on the way: {mid:?}");
    assert_eq!(pos(&app, riders[1]), SimPos::new(mid.x, mid.z + 500));
    run_ticks(app.world_mut(), 60);
    assert_eq!(pos(&app, ce), SimPos::new(8500, 500), "arrived");
    for &r in &riders {
        assert!(app.world().get::<MountedIn>(r).is_none(), "unloaded");
    }
    assert!(app.world().get::<OrderQueue>(de).unwrap().is_empty());
    let done: Vec<String> = journal(&app)
        .into_iter()
        .filter_map(|(u, k)| match k {
            JournalKind::OrderDone { order } if u == Some(driver) => Some(order),
            _ => None,
        })
        .collect();
    assert_eq!(done, ["Move", "Unload"]);
}

#[test]
fn a_cart_with_an_ai_driver_takes_the_order_itself() {
    let mut app = app();
    let (cart, ce, _driver, riders) = crewed_cart(&mut app, true);
    order(
        &mut app,
        &IssueOrder::new(P1, [cart], UnitOrder::Move { pos: (500, 6500) }),
    );
    run_ticks(app.world_mut(), 2);
    assert_eq!(
        app.world().get::<DriveGoal>(riders[0]),
        Some(&DriveGoal(SimPos::new(500, 6500))),
        "the driver's goal"
    );
    run_ticks(app.world_mut(), 80);
    assert_eq!(pos(&app, ce), SimPos::new(500, 6500));
    assert!(app.world().get::<OrderQueue>(ce).unwrap().is_empty());
    assert!(journal(&app).iter().any(|(u, k)| *u == Some(cart)
        && matches!(k, JournalKind::OrderDone { order } if order == "Move")));
}

#[test]
fn riders_cannot_be_moved_and_a_driverless_cart_cannot_drive() {
    let mut app = app();
    let (cart, ce, driver, riders) = crewed_cart(&mut app, false);
    // A commandable passenger is not the driver.
    app.world_mut()
        .entity_mut(riders[1])
        .insert((Commandable::new(100), Owner(P1)));
    let p = *app.world().get::<SimId>(riders[1]).unwrap();
    order(
        &mut app,
        &IssueOrder::new(P1, [p], UnitOrder::Move { pos: (4000, 0) }),
    );
    run_ticks(app.world_mut(), 2);
    let failed = journal(&app).into_iter().find_map(|(u, k)| match k {
        JournalKind::OrderFailed { reason, .. } if u == Some(p) => Some(reason),
        _ => None,
    });
    assert_eq!(
        failed.as_deref(),
        Some(format!("riding #{} (not its driver): dismount first", cart.0).as_str())
    );
    // Without a driver aboard the cart's own order fails.
    submit(
        app.world_mut(),
        None,
        SimCommand::content(&Dismount::new(driver)),
    )
    .unwrap();
    run_ticks(app.world_mut(), 1);
    order(
        &mut app,
        &IssueOrder::new(P1, [cart], UnitOrder::Move { pos: (4000, 500) }),
    );
    run_ticks(app.world_mut(), 2);
    assert!(journal(&app).iter().any(|(u, k)| *u == Some(cart)
        && matches!(k, JournalKind::OrderFailed { reason, .. } if reason == "no driver")));
    assert_eq!(pos(&app, ce), SimPos::new(500, 500));
}

#[test]
fn a_direct_vehicle_is_ordered_like_a_unit() {
    let mut app = app();
    let (tank, te) = spawn_sim(
        app.world_mut(),
        (
            SimPos::new(500, 500),
            Vehicle::direct(250).seat(Seat::driver().optional()),
            Commandable::new(250),
            Owner(P1),
        ),
    );
    order(
        &mut app,
        &IssueOrder::new(P1, [tank], UnitOrder::Move { pos: (5500, 500) }),
    );
    run_ticks(app.world_mut(), 25);
    assert_eq!(pos(&app, te), SimPos::new(5500, 500));
    // Another player may not order it.
    let err = submit_order(
        app.world_mut(),
        &IssueOrder::new(PlayerId(2), [tank], UnitOrder::Stop),
    )
    .unwrap_err();
    assert!(err.to_string().contains("owned by player 1"), "{err}");
}

#[test]
fn a_mount_order_walks_into_reach_then_mounts() {
    let mut app = app();
    let (cart, _) = spawn_sim(
        app.world_mut(),
        (
            SimPos::new(500, 500),
            Vehicle::by_crew(100)
                .seat(Seat::driver())
                .seat(Seat::passenger()),
        ),
    );
    let (u, ue) = spawn_sim(
        app.world_mut(),
        (
            SimPos::new(10_500, 500),
            Interactor,
            Commandable::new(200),
            Owner(P1),
        ),
    );
    order(
        &mut app,
        &IssueOrder::new(P1, [u], UnitOrder::Mount { vehicle: cart }),
    );
    run_ticks(app.world_mut(), 10);
    let x = pos(&app, ue).x;
    assert!(x < 10_500 && x > 2500, "walking: {x}");
    assert!(app.world().get::<MountedIn>(ue).is_none());
    run_ticks(app.world_mut(), 50);
    assert_eq!(
        app.world().get::<MountedIn>(ue).map(|m| m.vehicle),
        Some(cart)
    );
    let j = journal(&app);
    assert!(j.iter().any(|(who, k)| *who == Some(u)
        && matches!(k, JournalKind::OrderDone { order } if order == "Mount")));
    // A unit cannot be told to mount itself.
    let err = submit_order(
        app.world_mut(),
        &IssueOrder::new(P1, [u], UnitOrder::Mount { vehicle: u }),
    )
    .unwrap_err();
    assert!(err.to_string().contains("itself"), "{err}");
}

fn door_scene(app: &mut App) -> (SimId, Entity, SimId, SimId, Entity) {
    let w = app.world_mut();
    let (door, de) = spawn_sim(w, (SimPos::new(500, 500), Door::locked("key:gate")));
    let (key, _) = spawn_sim(w, (SimPos::new(-3500, 2500), Item::new("gate key")));
    let (u, ue) = spawn_sim(
        w,
        (
            SimPos::new(-6500, 500),
            Interactor,
            Inventory::new(4, 10_000),
            Equipment::hands(),
            Commandable::new(200),
            Owner(P1),
        ),
    );
    (door, de, key, u, ue)
}

fn interact(verb: &str, target: SimId) -> UnitOrder {
    UnitOrder::Interact {
        verb: verb.to_owned(),
        target,
    }
}

#[test]
fn queued_interact_orders_fetch_the_key_unlock_and_open_the_door() {
    let mut app = app();
    let (door, de, key, u, ue) = door_scene(&mut app);
    order(
        &mut app,
        &IssueOrder::new(P1, [u], interact("pick_up", key)),
    );
    order(
        &mut app,
        &IssueOrder::new(P1, [u], interact("unlock", door)).queued(true),
    );
    order(
        &mut app,
        &IssueOrder::new(P1, [u], interact("open", door)).queued(true),
    );
    run_ticks(app.world_mut(), 120);
    assert!(
        app.world()
            .get::<Inventory>(ue)
            .unwrap()
            .items
            .contains(&key)
    );
    let d = app.world().get::<Door>(de).unwrap();
    assert!(d.open && !d.is_locked(), "{d:?}");
    let done: Vec<String> = journal(&app)
        .into_iter()
        .filter_map(|(w, k)| match k {
            JournalKind::OrderDone { order } if w == Some(u) => Some(order),
            _ => None,
        })
        .collect();
    assert_eq!(done, ["Interact", "Interact", "Interact"]);
    assert!(app.world().get::<OrderQueue>(ue).unwrap().is_empty());
}

#[test]
fn a_hopeless_interact_order_fails_at_once_with_the_verbs_reason() {
    let mut app = app();
    let (door, _, _, u, ue) = door_scene(&mut app);
    order(&mut app, &IssueOrder::new(P1, [u], interact("open", door)));
    run_ticks(app.world_mut(), 3);
    let failed = journal(&app).into_iter().find_map(|(w, k)| match k {
        JournalKind::OrderFailed { order, reason, .. } if w == Some(u) => Some((order, reason)),
        _ => None,
    });
    assert_eq!(
        failed,
        Some(("Interact".to_owned(), "door is locked".to_owned()))
    );
    assert_eq!(pos(&app, ue), SimPos::new(-6500, 500), "never walked");
    // Unknown verbs are rejected when issued.
    let err = submit_order(
        app.world_mut(),
        &IssueOrder::new(P1, [u], interact("juggle", door)),
    )
    .unwrap_err();
    assert!(err.to_string().contains("unknown verb"), "{err}");
}

#[test]
fn context_orders_pick_up_items_open_doors_and_mount_vehicles() {
    let mut app = app();
    let w = app.world_mut();
    let (key, ke) = spawn_sim(w, (SimPos::new(0, 0), Item::new("gate key")));
    let (door, de) = spawn_sim(w, (SimPos::new(3500, 500), Door::closed()));
    let (gate, ge) = spawn_sim(w, (SimPos::new(5500, 500), Door::locked("key:gate")));
    let (_, oe) = spawn_sim(w, (SimPos::new(7500, 500), Door::opened()));
    let (cart, ve) = spawn_sim(w, (SimPos::new(0, 4000), Vehicle::by_crew(100)));
    let w = app.world();
    assert_eq!(
        context_item_order(w, P1, ke),
        Some(interact("pick_up", key))
    );
    assert_eq!(context_door_order(w, P1, de), Some(interact("open", door)));
    assert_eq!(
        context_door_order(w, P1, ge),
        Some(interact("unlock", gate))
    );
    assert_eq!(context_door_order(w, P1, oe), None);
    assert_eq!(context_item_order(w, P1, de), None);
    assert_eq!(
        context_vehicle_order(w, P1, ve),
        Some(UnitOrder::Mount { vehicle: cart })
    );
}

#[test]
fn glue_is_registered_only_when_both_capabilities_are_on() {
    let has = |app: &App, name: &str| names(app).iter().any(|n| n == name);
    let command = app_with(NpcPlugins::new().with_command());
    assert!(!has(&command, "Interact=npc_interaction.interact"));
    assert!(!has(&command, "Move=npc_vehicles.drive"));
    assert!(!has(&command, "interact_order"));
    let interaction = app_with(NpcPlugins::new().with_interaction());
    assert!(!has(&interaction, "finished=npc_command.interact_order"));
    let both = app_with(NpcPlugins::new().with_command().with_interaction());
    assert!(has(&both, "Interact=npc_interaction.interact"));
    assert!(has(&both, "finished=npc_command.interact_order"));
    assert!(has(&both, "interact_order"));
    assert!(!has(&both, "Move=npc_vehicles.drive"));
    let vehicles = app_with(NpcPlugins::new().with_vehicles());
    assert!(!has(&vehicles, "npc_vehicles.crew"));
    let all = app_with(NpcPlugins::new().with_command().with_vehicles());
    for name in [
        "Move=npc_vehicles.drive",
        "AttackMove=npc_vehicles.drive",
        "Mount=npc_vehicles.mount",
        "Unload=npc_vehicles.unload",
        "npc_vehicles.crew",
        "Interact=npc_interaction.interact",
    ] {
        assert!(has(&all, name), "{name}");
    }
    // Without the glue the data-only orders are rejected when issued.
    let mut command = command;
    let (u, _) = spawn_sim(
        command.world_mut(),
        (SimPos::new(0, 0), Commandable::new(100), Owner(P1)),
    );
    let err = submit_order(
        command.world_mut(),
        &IssueOrder::new(P1, [u], UnitOrder::Unload),
    )
    .unwrap_err();
    assert!(err.to_string().contains("no executor"), "{err}");
}

fn scripted_run() -> u64 {
    let mut app = app();
    let (cart, _, _, _) = crewed_cart(&mut app, false);
    let (door, _, key, u, _) = door_scene(&mut app);
    order(
        &mut app,
        &IssueOrder::new(P1, [cart], UnitOrder::Move { pos: (8500, 4500) }),
    );
    order(
        &mut app,
        &IssueOrder::new(P1, [u], interact("pick_up", key)),
    );
    order(
        &mut app,
        &IssueOrder::new(P1, [u], interact("unlock", door)).queued(true),
    );
    run_ticks(app.world_mut(), 150);
    state_hash(app.world_mut())
}

#[test]
fn command_glue_is_deterministic() {
    assert_eq!(scripted_run(), scripted_run());
}
