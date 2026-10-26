//! Wave F4: the agent-io glue of the wave F capabilities (ADR 0011).
//!
//! - command: the `order` action for owned units (groups, formations,
//!   ownership refusals), `execute_orders` for `Direct` units, section
//!   `command`;
//! - vehicles: `mount` / `dismount` / `drive_to` / `unload` affordances with
//!   `why_not`, `Advisor` (the `DriveTo` command) and `Direct` (the
//!   `DriveVehicle` leaf) driving, section `vehicle`;
//! - progression: section `progression`;
//! - crafting: `craft:<recipe>` affordances per workstation with `why_not`,
//!   acting by affordance (`Advisor`) and by verb (`Direct`), section
//!   `crafting`;
//! - the new journal kinds are significant agent events.

#![cfg(all(
    feature = "agent-io",
    feature = "command",
    feature = "vehicles",
    feature = "progression",
    feature = "crafting"
))]

use bevy_app::App;
use bevy_ecs::prelude::*;
use npc_framework::agent_io;
use npc_framework::glue::perception_agent as agent_io_glue;
use npc_framework::interaction::items::put_in_inventory;
use npc_framework::kernel::spatial::GridBounds;
use npc_framework::prelude::*;
use serde_json::{Value, json};

fn hold_start(_: &mut World, _: Entity, _: &ParamValue) -> Status {
    Status::Running
}
fn hold_stop(_: &mut World, _: Entity, _: StopReason) {}

fn app() -> App {
    build_app(false)
}

/// [`app`] with grid pathfinding (`NavPlugin`, for `NavAgent` units).
fn nav_app() -> App {
    build_app(true)
}

fn build_app(nav: bool) -> App {
    let mut app = App::new();
    app.insert_resource(SpatialGrid::new(1000, GridBounds::centered(64, 64)));
    app.add_plugins(
        NpcPlugins::new()
            .seed(5)
            .without_driver()
            .with_command()
            .with_vehicles()
            .with_progression()
            .with_crafting()
            .with_agent_io(),
    );
    if nav {
        app.add_plugins(npc_framework::nav::NavPlugin);
    }
    app.add_plugins(JournalPlugin::default())
        .register_item_kind(ItemKindDef::new("reed").stackable(20))
        .register_item_kind(ItemKindDef::new("basket"))
        .register_item_kind(ItemKindDef::new("mat"))
        // Task 26: weights, hand tools and a recipe that rolls at a default.
        .register_item_kind(ItemKindDef::new("log").weight_g(3000).stackable(10))
        .register_item_kind(ItemKindDef::new("plank").weight_g(800).stackable(20))
        .register_item_kind(
            ItemKindDef::new("axe")
                .weight_g(1500)
                .tag("tool:axe")
                .equip_slot(npc_framework::interaction::MAIN_HAND),
        )
        .register_item_kind(
            ItemKindDef::new("hammer")
                .weight_g(800)
                .tag("tool:hammer")
                .equip_slot(npc_framework::interaction::MAIN_HAND),
        )
        .register_skill(
            SkillDef::new("Carpentry", Stat::Dx, Difficulty::Average).default_stat(Stat::Dx, -5),
        )
        .register_recipe(
            RecipeDef::new("planks")
                .input("log", 1)
                .tool("tool:hammer")
                .skill("Carpentry", 0)
                .duration(10)
                .output("plank", 2),
        )
        .register_skill(
            SkillDef::new("Weaving", Stat::Dx, Difficulty::Easy).default_stat(Stat::Dx, -4),
        )
        .register_recipe(
            RecipeDef::new("basket")
                .input("reed", 2)
                .skill("Weaving", 10)
                .duration(30)
                .output("basket", 1)
                .xp(150),
        )
        .register_recipe(
            RecipeDef::new("mat")
                .input("reed", 1)
                .station("loom")
                .duration(20)
                .output("mat", 1),
        )
        .register_action("Hold", hold_start, hold_stop)
        .register_tree("hold", |b| {
            b.leaf("Hold", "Hold", ());
        })
        .register_tree("drive_or_hold", |b| {
            b.selector("Root", |b| {
                b.guard(
                    "Drive",
                    npc_framework::vehicles::COND_HAS_DRIVE_GOAL,
                    Abort::LowerPriority,
                    |b| {
                        b.leaf("DriveVehicle", npc_framework::vehicles::DRIVE_VEHICLE, ());
                    },
                );
                b.leaf("Hold", "Hold", ());
            });
        });
    app.finish();
    app.cleanup();
    app
}

const P1: PlayerId = PlayerId(1);

fn call(app: &mut App, m: &str, p: Value) -> Value {
    agent_io::api::call(app.world_mut(), m, Some(p)).unwrap_or_else(|e| panic!("{m}: {e:?}"))
}

fn call_err(app: &mut App, m: &str, p: Value) -> String {
    match agent_io::api::call(app.world_mut(), m, Some(p)) {
        Ok(v) if v["accepted"] == json!(false) => v["message"].to_string(),
        Ok(v) => panic!("{m} succeeded: {v}"),
        Err(e) => format!("{e:?}"),
    }
}

fn attach(app: &mut App, unit: SimId, mode: &str) {
    call(
        app,
        "agent/attach",
        json!({"unit": unit.0, "controller": "c", "mode": mode}),
    );
    run_ticks(app.world_mut(), 1);
}

fn observe(app: &mut App, unit: SimId) -> Value {
    call(app, "agent/observe", json!({"unit": unit.0}))
}

fn affordance(obs: &Value, id: &str) -> Value {
    obs["affordances"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == json!(id))
        .unwrap_or_else(|| panic!("no affordance `{id}` in {}", obs["affordances"]))
        .clone()
}

fn with_brain(app: &mut App, bundle: impl Bundle) -> (SimId, Entity) {
    let tree = app
        .world()
        .resource::<AiRegistry>()
        .tree_id("hold")
        .unwrap();
    spawn_sim(app.world_mut(), (bundle, Brain { tree }))
}

fn pos(app: &App, e: Entity) -> SimPos {
    *app.world().get::<SimPos>(e).unwrap()
}

#[test]
fn an_agent_orders_owned_units_in_formation_and_is_refused_others() {
    let mut app = app();
    let (cmd, _) = with_brain(
        &mut app,
        (SimPos::new(0, 0), Commandable::new(200), Owner(P1)),
    );
    let (a, ae) = spawn_sim(
        app.world_mut(),
        (SimPos::new(0, 2000), Commandable::new(200), Owner(P1)),
    );
    let (b, be) = spawn_sim(
        app.world_mut(),
        (SimPos::new(0, 4000), Commandable::new(200), Owner(P1)),
    );
    let (enemy, _) = spawn_sim(
        app.world_mut(),
        (
            SimPos::new(0, 6000),
            Commandable::new(200),
            Owner(PlayerId(2)),
        ),
    );
    attach(&mut app, cmd, "Advisor");
    let obs = observe(&mut app, cmd);
    assert_eq!(obs["sections"]["command"]["player"], json!(1));
    assert_eq!(obs["sections"]["command"]["owned_total"], json!(3));
    assert_eq!(affordance(&obs, "order")["ok"], json!(true));
    assert_eq!(
        affordance(&obs, "execute_orders")["why_not"],
        json!("Advisor units carry out orders through their own tree")
    );
    let r = call(
        &mut app,
        "agent/act",
        json!({"unit": cmd.0, "controller": "c", "verb": "order", "args": {
            "order": {"type": "Move", "pos": [10000, 0]},
            "units": [a.0, b.0],
            "formation": {"kind": "Line", "spacing_mm": 2000},
        }}),
    );
    assert_eq!(r["accepted"], json!(true), "{r}");
    run_ticks(app.world_mut(), 80);
    let (pa, pb) = (pos(&app, ae), pos(&app, be));
    assert!(pa.dist2(SimPos::new(10_000, 0)) <= 2500 * 2500, "{pa:?}");
    assert!(pb.dist2(SimPos::new(10_000, 0)) <= 2500 * 2500, "{pb:?}");
    assert_ne!(pa, pb, "own formation slots");
    let err = call_err(
        &mut app,
        "agent/act",
        json!({"unit": cmd.0, "controller": "c", "verb": "order", "args": {
            "order": {"type": "Stop"}, "units": [enemy.0],
        }}),
    );
    assert!(err.contains("owned by player 2"), "{err}");
    let err = call_err(
        &mut app,
        "agent/act",
        json!({"unit": cmd.0, "controller": "c", "verb": "order", "args": {"order": {"type": "Fly"}}}),
    );
    assert!(err.contains("bad order"), "{err}");
}

#[test]
fn a_direct_unit_carries_out_its_own_orders_when_told() {
    let mut app = app();
    let (u, ue) = with_brain(
        &mut app,
        (SimPos::new(0, 0), Commandable::new(250), Owner(P1)),
    );
    attach(&mut app, u, "Direct");
    let obs = observe(&mut app, u);
    assert_eq!(
        affordance(&obs, "execute_orders")["why_not"],
        json!("no orders")
    );
    call(
        &mut app,
        "agent/act",
        json!({"unit": u.0, "controller": "c", "verb": "order",
               "args": {"order": {"type": "Move", "pos": [5000, 0]}}}),
    );
    run_ticks(app.world_mut(), 5);
    assert_eq!(pos(&app, ue), SimPos::new(0, 0), "the tree is suspended");
    let obs = observe(&mut app, u);
    assert_eq!(affordance(&obs, "execute_orders")["ok"], json!(true));
    call(
        &mut app,
        "agent/act",
        json!({"unit": u.0, "controller": "c", "affordance": "execute_orders"}),
    );
    run_ticks(app.world_mut(), 25);
    assert_eq!(pos(&app, ue), SimPos::new(5000, 0));
    // Observers may look but not act.
    let (o, _) = with_brain(
        &mut app,
        (SimPos::new(0, 0), Commandable::new(250), Owner(P1)),
    );
    attach(&mut app, o, "Observer");
    let obs = observe(&mut app, o);
    assert_eq!(
        affordance(&obs, "order")["why_not"],
        json!("observers cannot act")
    );
}

fn cart(app: &mut App, at: SimPos) -> (SimId, Entity) {
    spawn_sim(
        app.world_mut(),
        (
            at,
            Vehicle::by_crew(100)
                .seat(Seat::driver())
                .seat(Seat::passenger()),
        ),
    )
}

#[test]
fn vehicle_affordances_say_why_not_and_agents_drive() {
    let mut app = app();
    let (v, ve) = cart(&mut app, SimPos::new(500, 500));
    let (far, _) = with_brain(&mut app, (SimPos::new(6500, 500), Interactor));
    attach(&mut app, far, "Advisor");
    let obs = observe(&mut app, far);
    let mount = affordance(&obs, &format!("mount@{}", v.0));
    assert_eq!(mount["ok"], json!(false));
    assert!(
        mount["why_not"].as_str().unwrap().starts_with("too far"),
        "{mount}"
    );
    assert!(obs["sections"].get("vehicle").is_none());

    // A unit next to the cart mounts it as the driver (Advisor: the
    // Interact command) and sends it off (the DriveTo command).
    let (d, de) = with_brain(
        &mut app,
        (
            SimPos::new(1500, 500),
            Interactor,
            SeatPreference(SeatRole::Driver),
        ),
    );
    call(
        &mut app,
        "agent/attach",
        json!({"unit": d.0, "controller": "d", "mode": "Advisor"}),
    );
    run_ticks(app.world_mut(), 1);
    let obs = observe(&mut app, d);
    assert_eq!(
        affordance(&obs, &format!("mount@{}", v.0))["ok"],
        json!(true)
    );
    call(
        &mut app,
        "agent/act",
        json!({"unit": d.0, "controller": "d", "affordance": format!("mount@{}", v.0)}),
    );
    run_ticks(app.world_mut(), 2);
    assert_eq!(app.world().get::<MountedIn>(de).map(|m| m.seat), Some(0));
    let obs = observe(&mut app, d);
    assert_eq!(
        obs["sections"]["vehicle"]["mounted_in"]["role"],
        json!("Driver")
    );
    assert_eq!(
        affordance(&obs, &format!("dismount@{}", v.0))["ok"],
        json!(true)
    );
    assert_eq!(affordance(&obs, "drive_to")["ok"], json!(true));
    assert_eq!(
        affordance(&obs, &format!("unload@{}", v.0))["ok"],
        json!(true)
    );
    let r = call(
        &mut app,
        "agent/act",
        json!({"unit": d.0, "controller": "d", "verb": "drive_to", "args": {"pos": [4500, 500]}}),
    );
    assert_eq!(r["accepted"], json!(true), "{r}");
    run_ticks(app.world_mut(), 1);
    assert_eq!(
        app.world().get::<DriveGoal>(de),
        Some(&DriveGoal(SimPos::new(4500, 500)))
    );

    // A passenger may not drive.
    let (p, _) = with_brain(&mut app, (SimPos::new(500, 1500), Interactor));
    submit(
        app.world_mut(),
        None,
        SimCommand::content(&Mount::new(p, v)),
    )
    .unwrap();
    run_ticks(app.world_mut(), 1);
    call(
        &mut app,
        "agent/attach",
        json!({"unit": p.0, "controller": "p", "mode": "Advisor"}),
    );
    run_ticks(app.world_mut(), 1);
    let obs = observe(&mut app, p);
    assert_eq!(
        affordance(&obs, "drive_to")["why_not"],
        json!(format!("riding #{} but not in its driver seat", v.0))
    );

    // Direct control drives through the DriveVehicle leaf.
    call(
        &mut app,
        "agent/detach",
        json!({"unit": d.0, "controller": "d"}),
    );
    run_ticks(app.world_mut(), 1);
    app.world_mut().entity_mut(de).remove::<DriveGoal>();
    call(
        &mut app,
        "agent/attach",
        json!({"unit": d.0, "controller": "d", "mode": "Direct"}),
    );
    run_ticks(app.world_mut(), 1);
    call(
        &mut app,
        "agent/act",
        json!({"unit": d.0, "controller": "d", "verb": "drive_to", "args": {"pos": [500, 4500]}}),
    );
    run_ticks(app.world_mut(), 60);
    assert_eq!(pos(&app, ve), SimPos::new(500, 4500));
}

/// Ticks until `done` (at most `limit`), asserting the unit is busy for
/// the agent (`agent/events {until_idle}`, `agent/act_and_wait`) on every
/// tick before it; returns the last busy reason.
fn busy_until(app: &mut App, unit: SimId, limit: u32, done: impl Fn(&App) -> bool) -> String {
    let mut reason = String::new();
    for _ in 0..limit {
        if done(app) {
            return reason;
        }
        reason = agent_io::api::busy_reason(app.world(), unit)
            .unwrap_or_else(|| panic!("unit {unit:?} idle while its vehicle is still driving"));
        run_ticks(app.world_mut(), 1);
    }
    panic!("not done after {limit} ticks");
}

/// Ticks until the unit is idle, at most `limit` ticks (the drive's leaf
/// notices the arrival a tick later and stops at the next think).
fn idle_within(app: &mut App, unit: SimId, limit: u32) {
    for _ in 0..limit {
        run_ticks(app.world_mut(), 1);
        if agent_io::api::busy_reason(app.world(), unit).is_none() {
            return;
        }
    }
    panic!(
        "unit {unit:?} still busy {limit} ticks after arriving: {:?}",
        agent_io::api::busy_reason(app.world(), unit)
    );
}

#[test]
fn a_unit_is_busy_until_the_vehicle_it_drives_arrives() {
    let mut app = app();
    let tree = app
        .world()
        .resource::<AiRegistry>()
        .tree_id("drive_or_hold")
        .unwrap();

    // An Advisor driver of a ByCrew cart: the DriveTo command sets the
    // goal, the driver's own tree drives (DriveVehicle).
    let (v, ve) = cart(&mut app, SimPos::new(500, 500));
    let (d, de) = spawn_sim(
        app.world_mut(),
        (
            SimPos::new(1500, 500),
            Interactor,
            SeatPreference(SeatRole::Driver),
            Brain { tree },
        ),
    );
    submit(
        app.world_mut(),
        None,
        SimCommand::content(&Mount::new(d, v)),
    )
    .unwrap();
    run_ticks(app.world_mut(), 2);
    assert_eq!(app.world().get::<MountedIn>(de).map(|m| m.seat), Some(0));
    attach(&mut app, d, "Advisor");
    assert_eq!(agent_io::api::busy_reason(app.world(), d), None);
    let goal = SimPos::new(4500, 500);
    let r = call(
        &mut app,
        "agent/act",
        json!({"unit": d.0, "controller": "c", "verb": "drive_to", "args": {"pos": [4500, 500]}}),
    );
    assert_eq!(r["accepted"], json!(true), "{r}");
    let reason = busy_until(&mut app, d, 200, |app| pos(app, ve) == goal);
    assert_eq!(
        reason,
        format!("driving vehicle #{} to (4.50 m, 0.50 m)", v.0)
    );
    idle_within(&mut app, d, 3);

    // A Direct vehicle is a unit of its own: busy while it moves.
    let (w, we) = with_brain(&mut app, (SimPos::new(500, 2500), Vehicle::direct(100)));
    attach(&mut app, w, "Advisor");
    assert_eq!(agent_io::api::busy_reason(app.world(), w), None);
    let r = call(
        &mut app,
        "agent/act",
        json!({"unit": w.0, "controller": "c", "verb": "drive_to", "args": {"pos": [3500, 2500]}}),
    );
    assert_eq!(r["accepted"], json!(true), "{r}");
    let goal = SimPos::new(3500, 2500);
    let reason = busy_until(&mut app, w, 200, |app| pos(app, we) == goal);
    assert_eq!(reason, "driving to (3.50 m, 2.50 m)");
    idle_within(&mut app, w, 3);
}

#[test]
fn crafting_affordances_say_why_not_and_progression_is_observed() {
    let mut app = app();
    let mut sheet = CharacterSheet::default();
    sheet.skills.insert("Weaving".into(), 4);
    let (u, ue) = with_brain(
        &mut app,
        (
            SimPos::new(0, 0),
            Interactor,
            Inventory::new(4, 100_000),
            sheet,
            Experience::default(),
        ),
    );
    attach(&mut app, u, "Advisor");
    let obs = observe(&mut app, u);
    assert_eq!(obs["sections"]["progression"]["level"], json!(0));
    assert_eq!(obs["sections"]["crafting"]["crafting"], json!(null));
    assert_eq!(
        affordance(&obs, "craft:basket")["why_not"],
        json!("needs 2 reed (has 0)")
    );
    assert_eq!(
        affordance(&obs, "craft:mat")["why_not"],
        json!("needs a workstation: loom")
    );
    let (loom, _) = spawn_sim(
        app.world_mut(),
        (SimPos::new(6000, 0), Workstation::new(["loom"])),
    );
    run_ticks(app.world_mut(), 1);
    let obs = observe(&mut app, u);
    let at = affordance(&obs, &format!("craft:mat@{}", loom.0));
    assert!(
        at["why_not"].as_str().unwrap().starts_with("too far"),
        "{at}"
    );
    let (_, reed) = spawn_sim(
        app.world_mut(),
        (SimPos::new(0, 0), Item::new("reed").stack(4)),
    );
    put_in_inventory(app.world_mut(), ue, reed);
    let obs = observe(&mut app, u);
    assert_eq!(affordance(&obs, "craft:basket")["ok"], json!(true));
    let r = call(
        &mut app,
        "agent/act",
        json!({"unit": u.0, "controller": "c", "affordance": "craft:basket"}),
    );
    assert_eq!(r["accepted"], json!(true), "{r}");
    run_ticks(app.world_mut(), 2);
    let obs = observe(&mut app, u);
    assert_eq!(
        obs["sections"]["crafting"]["crafting"]["recipe"],
        json!("basket")
    );
    assert!(
        affordance(&obs, "craft:basket")["why_not"]
            .as_str()
            .unwrap()
            .starts_with("busy with craft basket")
    );
    run_ticks(app.world_mut(), 30);
    let obs = observe(&mut app, u);
    assert_eq!(obs["sections"]["progression"]["level"], json!(1));
    assert!(obs["sections"]["progression"]["xp"].as_u64().unwrap() >= 150);
    let events = call(&mut app, "agent/events", json!({"unit": u.0}));
    let kinds: Vec<&str> = events["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|e| e["kind"].as_str())
        .collect();
    for k in ["CraftStarted", "CraftDone", "XpGained", "LevelUp"] {
        assert!(kinds.contains(&k), "{k} in {kinds:?}");
    }

    // Direct control crafts by verb through the Craft leaf.
    call(
        &mut app,
        "agent/detach",
        json!({"unit": u.0, "controller": "c"}),
    );
    run_ticks(app.world_mut(), 1);
    attach(&mut app, u, "Direct");
    let r = call(
        &mut app,
        "agent/act",
        json!({"unit": u.0, "controller": "c", "verb": "craft", "args": {"recipe": "basket"}}),
    );
    assert_eq!(r["accepted"], json!(true), "{r}");
    run_ticks(app.world_mut(), 35);
    assert_eq!(
        app.world()
            .get::<Inventory>(ue)
            .unwrap()
            .items
            .iter()
            .filter(|&&id| {
                let e = app.world().resource::<SimIndex>().get(id).unwrap();
                app.world().get::<Item>(e).unwrap().kind == "basket"
            })
            .count(),
        2
    );
}

#[test]
fn wave_f_journal_kinds_are_significant_agent_events() {
    for k in [
        "OrderIssued",
        "OrderStarted",
        "OrderDone",
        "OrderFailed",
        "Mounted",
        "Dismounted",
        "VehicleStalled",
        "XpGained",
        "LevelUp",
        "SkillImproved",
        "CraftStarted",
        "CraftDone",
        "CraftFailed",
    ] {
        assert!(agent_io::events::SIGNIFICANT_KINDS.contains(&k), "{k}");
    }
}

// ------------------------------------------------- Task 25: feedback

const WALL: CellInfo = CellInfo {
    flags: CellFlags {
        blocking: true,
        occluding: true,
        tags: 0,
    },
    extra_cost: 0,
};

/// A brainless, owned `NavAgent` walker at `at` (it carries out its orders
/// itself), attached as `Advisor`.
fn walker(app: &mut App, at: SimPos) -> (SimId, Entity) {
    let (u, ue) = spawn_sim(
        app.world_mut(),
        (
            at,
            Interactor,
            Commandable::new(200),
            Owner(P1),
            npc_framework::nav::NavAgent,
        ),
    );
    attach(app, u, "Advisor");
    (u, ue)
}

fn order_move(app: &mut App, u: SimId, pos: (i32, i32)) -> Value {
    let r = call(
        app,
        "agent/act",
        json!({"unit": u.0, "controller": "c", "verb": "order",
               "args": {"order": {"type": "Move", "pos": [pos.0, pos.1]}}}),
    );
    assert_eq!(r["accepted"], json!(true), "{r}");
    r
}

/// The unit's `OrderFailed` events (their journal JSON).
fn orders_failed(app: &mut App, u: SimId) -> Vec<Value> {
    call(
        app,
        "agent/events",
        json!({"unit": u.0, "kinds": ["OrderFailed"], "limit": 100}),
    )["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["event"].clone())
        .collect()
}

fn text(app: &mut App, u: SimId) -> String {
    call(
        app,
        "agent/observe",
        json!({"unit": u.0, "format": "text", "events": 50}),
    )["text"]
        .as_str()
        .unwrap_or_default()
        .to_owned()
}

#[test]
fn blocked_move_reports_nearest_reachable_and_door() {
    let mut app = nav_app();
    {
        // A wall along x = 5 with a locked door at (5, 0).
        let mut grid = app.world_mut().resource_mut::<SpatialGrid>();
        for z in -32..32 {
            if z != 0 {
                assert!(grid.set_static(CellCoord::new(5, z), WALL));
            }
        }
    }
    let (door, _) = spawn_sim(
        app.world_mut(),
        (SimPos::new(5500, 500), Door::locked("key:gate")),
    );
    let (u, _) = walker(&mut app, SimPos::new(-6500, 500));
    order_move(&mut app, u, (12_500, 3500));
    run_ticks(app.world_mut(), 130);
    let failed = orders_failed(&mut app, u);
    assert_eq!(failed.len(), 1, "{failed:?}");
    let e = &failed[0];
    assert_eq!(e["order"], json!("Move"));
    assert_eq!(e["reason"], json!("blocked"));
    // The cell east-most in front of the wall, level with the goal.
    assert_eq!(e["nearest_reachable"], json!([4500, 3500]), "{e}");
    assert_eq!(
        e["blocker"],
        json!({"kind": "Door", "id": door.0, "locked": true}),
        "{e}"
    );
    let t = text(&mut app, u);
    assert!(
        t.contains(&format!(
            "Move failed: blocked by locked door #{}; nearest reachable (4.50 m, 3.50 m)",
            door.0
        )),
        "{t}"
    );

    // A goal inside the wall names the wall.
    let (w, _) = walker(&mut app, SimPos::new(-6500, 8500));
    order_move(&mut app, w, (5500, 8500));
    run_ticks(app.world_mut(), 130);
    let failed = orders_failed(&mut app, w);
    assert_eq!(
        failed[0]["blocker"],
        json!({"kind": "Wall", "cell": [5, 8], "pos": [5500, 8500]}),
        "{failed:?}"
    );
    assert_eq!(failed[0]["nearest_reachable"], json!([4500, 8500]));
    let t = text(&mut app, w);
    assert!(
        t.contains(
            "Move failed: blocked by a wall at (5.50 m, 8.50 m); nearest reachable (4.50 m, 8.50 m)"
        ),
        "{t}"
    );
}

#[test]
fn blocked_move_out_of_bounds() {
    let mut app = nav_app();
    let (u, _) = walker(&mut app, SimPos::new(-6500, 500));
    // The grid ends at x = 32 m.
    order_move(&mut app, u, (99_000, 500));
    run_ticks(app.world_mut(), 130);
    let failed = orders_failed(&mut app, u);
    assert_eq!(failed.len(), 1, "{failed:?}");
    assert_eq!(failed[0]["reason"], json!("blocked"));
    assert_eq!(failed[0]["blocker"], json!({"kind": "OutOfBounds"}));
    assert_eq!(failed[0]["nearest_reachable"], json!([31_500, 500]));
    let t = text(&mut app, u);
    assert!(
        t.contains(
            "Move failed: blocked by the edge of the world; nearest reachable (31.50 m, 0.50 m)"
        ),
        "{t}"
    );
}

#[test]
fn move_label_in_metres() {
    let mut app = app();
    let (u, _) = spawn_sim(
        app.world_mut(),
        (SimPos::new(0, 0), Commandable::new(200), Owner(P1)),
    );
    attach(&mut app, u, "Advisor");
    // 30 mm south, as the agent sent it by mistake: the label shows it.
    let r = order_move(&mut app, u, (0, -30));
    assert_eq!(
        r["label"],
        json!("Order yourself: move to (0.00 m, -0.03 m)"),
        "{r}"
    );
    let r = call(
        &mut app,
        "agent/act",
        json!({"unit": u.0, "controller": "c", "verb": "order",
               "args": {"order": {"type": "AttackMove", "pos": [12_345, -6500]}}}),
    );
    assert_eq!(
        r["label"],
        json!("Order yourself: attack-move to (12.34 m, -6.50 m)"),
        "{r}"
    );
    run_ticks(app.world_mut(), 1);
    assert_eq!(
        agent_io::api::busy_reason(app.world(), u).as_deref(),
        Some("order: attack-move to (12.34 m, -6.50 m)")
    );
    let t = text(&mut app, u);
    assert!(
        t.contains("Current order: attack-move to (12.34 m, -6.50 m)."),
        "{t}"
    );
}

/// The testbed's armored wagon: a `Direct` vehicle with an optional driver
/// seat and two passenger seats, owned and commandable.
fn wagon(app: &mut App, at: SimPos) -> (SimId, Entity) {
    spawn_sim(
        app.world_mut(),
        (
            at,
            Vehicle::direct(100)
                .seat(Seat::driver().optional())
                .seats(2, Seat::passenger().offset(0, -900)),
            Commandable::new(100),
            Owner(P1),
        ),
    )
}

/// A unit next to `vehicle`, mounted in its driver seat, attached as `mode`.
fn seated_driver(app: &mut App, vehicle: SimId, at: SimPos, mode: &str) -> (SimId, Entity) {
    let (d, de) = with_brain(app, (at, Interactor, SeatPreference(SeatRole::Driver)));
    submit(
        app.world_mut(),
        None,
        SimCommand::content(&Mount::new(d, vehicle)),
    )
    .unwrap();
    run_ticks(app.world_mut(), 2);
    assert_eq!(app.world().get::<MountedIn>(de).map(|m| m.seat), Some(0));
    attach(app, d, mode);
    (d, de)
}

#[test]
fn driver_seat_can_drive_to() {
    let mut app = app();
    let (w, we) = wagon(&mut app, SimPos::new(500, 500));
    let (d, de) = seated_driver(&mut app, w, SimPos::new(1500, 500), "Advisor");
    // The reported case: "Mounted {role: Driver, seat: 0}", then drive_to
    // was refused "riding #30 but not in its driver seat".
    let obs = observe(&mut app, d);
    assert_eq!(
        obs["sections"]["vehicle"]["mounted_in"]["role"],
        json!("Driver")
    );
    let drive = affordance(&obs, "drive_to");
    assert_eq!(drive["ok"], json!(true), "{drive}");
    let t = text(&mut app, d);
    assert!(!t.contains("moving on its own"), "{t}");
    assert!(
        t.contains(&format!("You ride vehicle #{} (self-propelled", w.0)),
        "{t}"
    );
    let r = call(
        &mut app,
        "agent/act",
        json!({"unit": d.0, "controller": "c", "verb": "drive_to", "args": {"pos": [4500, 500]}}),
    );
    assert_eq!(r["accepted"], json!(true), "{r}");
    assert_eq!(r["label"], json!("Drive to (4.50 m, 0.50 m)"), "{r}");
    let goal = SimPos::new(4500, 500);
    let reason = busy_until(&mut app, d, 200, |app| pos(app, we) == goal);
    assert_eq!(
        reason,
        format!("driving vehicle #{} to (4.50 m, 0.50 m)", w.0)
    );

    // Direct control drives it too (the DriveTo command: a Direct vehicle
    // has no DriveVehicle leaf to run).
    call(
        &mut app,
        "agent/detach",
        json!({"unit": d.0, "controller": "c"}),
    );
    run_ticks(app.world_mut(), 1);
    attach(&mut app, d, "Direct");
    let r = call(
        &mut app,
        "agent/act",
        json!({"unit": d.0, "controller": "c", "verb": "drive_to", "args": {"pos": [500, 4500]}}),
    );
    assert_eq!(r["accepted"], json!(true), "{r}");
    run_ticks(app.world_mut(), 60);
    assert_eq!(pos(&app, we), SimPos::new(500, 4500));

    // The driver seat also unloads the wagon (everyone aboard gets off).
    let (p, pe) = with_brain(&mut app, (SimPos::new(500, 5500), Interactor));
    submit(
        app.world_mut(),
        None,
        SimCommand::content(&Mount::new(p, w)),
    )
    .unwrap();
    run_ticks(app.world_mut(), 2);
    assert_eq!(app.world().get::<MountedIn>(pe).map(|m| m.vehicle), Some(w));
    let obs = observe(&mut app, d);
    let unload = affordance(&obs, &format!("unload@{}", w.0));
    assert_eq!(unload["ok"], json!(true), "{unload}");
    assert_eq!(
        unload["label"],
        json!(format!("Get everyone off vehicle #{} (2 aboard)", w.0))
    );
    let r = call(
        &mut app,
        "agent/act",
        json!({"unit": d.0, "controller": "c", "affordance": format!("unload@{}", w.0)}),
    );
    assert_eq!(r["accepted"], json!(true), "{r}");
    assert_eq!(r["label"], json!(format!("Unload vehicle #{}", w.0)));
    run_ticks(app.world_mut(), 3);
    assert_eq!(app.world().get::<MountedIn>(pe), None);
    assert_eq!(app.world().get::<MountedIn>(de), None);
}

#[test]
fn autopilot_reason_matches_state() {
    let mut app = app();
    let (w, we) = wagon(&mut app, SimPos::new(500, 500));
    let tree = app
        .world()
        .resource::<AiRegistry>()
        .tree_id("hold")
        .unwrap();
    app.world_mut().entity_mut(we).insert(Brain { tree });
    let (d, _) = seated_driver(&mut app, w, SimPos::new(1500, 500), "Advisor");

    // Its own behaviour tree drives it.
    let obs = observe(&mut app, d);
    let why = affordance(&obs, "drive_to")["why_not"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    assert!(
        why.starts_with(&format!("vehicle #{} is on autopilot: ", w.0)),
        "{why}"
    );
    assert!(why.contains("its own behaviour drives it"), "{why}");
    assert!(
        why.contains(&format!("attach a controller to #{}", w.0)),
        "{why}"
    );
    assert!(!why.contains("driver seat"), "{why}");
    let t = text(&mut app, d);
    assert!(
        t.contains(&format!("You ride vehicle #{} (on autopilot)", w.0)),
        "{t}"
    );
    let err = call_err(
        &mut app,
        "agent/act",
        json!({"unit": d.0, "controller": "c", "verb": "drive_to", "args": {"pos": [4500, 500]}}),
    );
    assert!(err.contains("is on autopilot"), "{err}");

    // Another controller drives it: the reason names it.
    call(
        &mut app,
        "agent/attach",
        json!({"unit": w.0, "controller": "w", "mode": "Direct"}),
    );
    run_ticks(app.world_mut(), 1);
    let obs = observe(&mut app, d);
    let why = affordance(&obs, "drive_to")["why_not"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    assert!(
        why.starts_with(&format!("vehicle #{} is on autopilot: controller `w`", w.0)),
        "{why}"
    );

    // Neither: the driver seat steers it.
    call(
        &mut app,
        "agent/detach",
        json!({"unit": w.0, "controller": "w"}),
    );
    run_ticks(app.world_mut(), 1);
    app.world_mut().entity_mut(we).remove::<Brain>();
    let obs = observe(&mut app, d);
    assert_eq!(affordance(&obs, "drive_to")["ok"], json!(true));
    let r = call(
        &mut app,
        "agent/act",
        json!({"unit": d.0, "controller": "c", "verb": "drive_to", "args": {"pos": [3500, 500]}}),
    );
    assert_eq!(r["accepted"], json!(true), "{r}");
    run_ticks(app.world_mut(), 40);
    assert_eq!(pos(&app, we), SimPos::new(3500, 500));
}

#[test]
fn blocked_reports_are_capped_per_tick() {
    let mut app = nav_app();
    app.insert_resource(npc_framework::command::BlockedReports::new(2));
    let walkers: Vec<SimId> = (0..3)
        .map(|i| walker(&mut app, SimPos::new(-6500, 500 + 2000 * i)).0)
        .collect();
    // Orders in the same tick, goals past the edge (no search): the three
    // walks fail in the same tick.
    for &u in &walkers {
        order_move(&mut app, u, (99_000, 500));
    }
    run_ticks(app.world_mut(), 130);
    let failed: Vec<Value> = walkers
        .iter()
        .map(|&u| {
            let f = orders_failed(&mut app, u);
            assert_eq!(f.len(), 1, "{f:?}");
            f[0].clone()
        })
        .collect();
    let ticks: Vec<Value> = walkers
        .iter()
        .map(|&u| {
            call(
                &mut app,
                "agent/events",
                json!({"unit": u.0, "kinds": ["OrderFailed"]}),
            )["events"][0]["tick"]
                .clone()
        })
        .collect();
    assert!(ticks.iter().all(|t| *t == ticks[0]), "{ticks:?}");
    // SimId order: the first two get the report, the third only its reason.
    for f in &failed[..2] {
        assert_eq!(f["blocker"], json!({"kind": "OutOfBounds"}), "{f}");
        assert!(f["nearest_reachable"].is_array(), "{f}");
    }
    assert_eq!(
        failed[2],
        json!({"type": "OrderFailed", "order": "Move", "reason": "blocked"})
    );
}

// ------------------------------------------------- Task 26: item feedback

fn carry(app: &mut App, holder: Entity, item: Item) -> (SimId, Entity) {
    let at = pos(app, holder);
    let (id, e) = spawn_sim(app.world_mut(), (at, item));
    put_in_inventory(app.world_mut(), holder, e);
    (id, e)
}

fn wield(app: &mut App, holder: Entity, item: Item) -> (SimId, Entity) {
    let (id, e) = carry(app, holder, item);
    npc_framework::interaction::items::put_in_slot(
        app.world_mut(),
        holder,
        e,
        npc_framework::interaction::MAIN_HAND,
    );
    (id, e)
}

/// A brainless `Advisor` unit at `at` with an 8-slot, 30 kg pack and hands.
fn packer(app: &mut App, at: SimPos, sheet: Option<CharacterSheet>) -> (SimId, Entity) {
    let (u, ue) = spawn_sim(
        app.world_mut(),
        (
            at,
            Interactor,
            Inventory::new(8, 30_000),
            npc_framework::interaction::Equipment::hands(),
        ),
    );
    if let Some(s) = sheet {
        app.world_mut()
            .entity_mut(ue)
            .insert((s, Experience::default()));
    }
    attach(app, u, "Advisor");
    (u, ue)
}

/// Proposal 005: the `use_tool_on` offer shows the pack load after the
/// yield (and that it is too heavy to pick up); the result says where the
/// yield was dropped and why it could not go in the pack.
#[test]
fn offer_shows_load_after_yield() {
    let mut app = app();
    let at = SimPos::new(24_080, 1100);
    let (u, ue) = packer(&mut app, at, None);
    wield(&mut app, ue, Item::new("axe")); // 1500 g
    carry(&mut app, ue, Item::new("log").stack(8)); // 24000 g
    let (tree, _) = spawn_sim(
        app.world_mut(),
        (
            SimPos::new(25_080, 1100),
            ToolTarget::new("tool:axe")
                .yields(ItemYield::new("log", 2))
                .duration(5)
                .uses(3),
        ),
    );
    run_ticks(app.world_mut(), 1);
    let obs = observe(&mut app, u);
    let chop = affordance(&obs, &format!("use_tool_on@{}", tree.0));
    let label = chop["label"].as_str().unwrap();
    assert!(
        label.contains(
            "yields 2 × log at your feet (pack 25500 -> 31500 of 30000 g, too heavy to pick up)"
        ),
        "{label}"
    );
    assert_eq!(chop["ok"], json!(true), "{chop}");
    let r = call(
        &mut app,
        "agent/act",
        json!({"unit": u.0, "controller": "c", "affordance": format!("use_tool_on@{}", tree.0)}),
    );
    assert_eq!(r["accepted"], json!(true), "{r}");
    run_ticks(app.world_mut(), 8);
    let t = text(&mut app, u);
    assert!(
        t.contains(&format!(
            "use_tool_on #{} success, 2 × log dropped at (24.08 m, 1.10 m): \
             pack would carry 31500 of 30000 g",
            tree.0
        )),
        "{t}"
    );
    let events = call(
        &mut app,
        "agent/events",
        json!({"unit": u.0, "kinds": ["Interacted"], "limit": 10}),
    );
    let done = events["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["event"]["outcome"] == json!("success"))
        .unwrap()
        .clone();
    assert_eq!(done["event"]["dropped"][0]["pos"], json!([24_080, 1100]));
    assert_eq!(
        done["event"]["reason"],
        json!("pack would carry 31500 of 30000 g")
    );
}

/// Proposal 002: a craft line shows the skill odds like a tool use, and
/// what it consumes and uses.
#[test]
fn craft_line_shows_odds() {
    let mut app = app();
    let mut sheet = CharacterSheet::default();
    sheet.attributes.dx = 12;
    let (u, ue) = packer(&mut app, SimPos::new(0, 0), Some(sheet));
    let obs = observe(&mut app, u);
    let a = affordance(&obs, "craft:planks");
    assert_eq!(
        a["why_not"],
        json!("needs 1 log (has 0), tool hammer (has 0)")
    );
    assert!(
        a["label"]
            .as_str()
            .unwrap()
            .ends_with("[Carpentry 7 (default DX-5) vs 3d6: ~16%, low odds]"),
        "{a}"
    );
    carry(&mut app, ue, Item::new("log"));
    wield(&mut app, ue, Item::new("hammer"));
    let obs = observe(&mut app, u);
    let a = affordance(&obs, "craft:planks");
    assert_eq!(a["ok"], json!(true), "{a}");
    assert_eq!(
        a["label"],
        json!(
            "Craft planks: consumes 1 log; uses hammer \
             [Carpentry 7 (default DX-5) vs 3d6: ~16%, low odds]"
        )
    );
    assert_eq!(a["check"]["target_number"], json!(7));
    assert_eq!(a["check"]["default"], json!("DX-5"));
    assert_eq!(a["check"]["odds_permille"], json!(162));
}

/// Proposal 008: the pack line says whether hand-held items take a slot,
/// so "You carry" listing one more thing than the slots makes sense, and
/// unequipping into a full pack names what to drop.
#[test]
fn hand_item_slot_rule_is_stated() {
    let mut app = app();
    let (u, ue) = spawn_sim(
        app.world_mut(),
        (
            SimPos::new(0, 0),
            Interactor,
            Inventory::new(2, 30_000),
            npc_framework::interaction::Equipment::hands(),
        ),
    );
    attach(&mut app, u, "Advisor");
    let (axe, _) = wield(&mut app, ue, Item::new("axe"));
    let (plank, _) = carry(&mut app, ue, Item::new("plank"));
    let (log, _) = carry(&mut app, ue, Item::new("log"));
    let obs = observe(&mut app, u);
    let inv = &obs["sections"]["inventory"];
    assert_eq!(inv["used_slots"], json!(2));
    assert_eq!(inv["equipped_use_slots"], json!(false));
    let t = text(&mut app, u);
    assert!(
        t.contains(&format!(
            "Load 5300 of 30000 g, 2 of 2 slots used; equipped items (axe #{} in main_hand) \
             take no slot, but unequipping one needs a free slot.",
            axe.0
        )),
        "{t}"
    );
    let off = affordance(&obs, &format!("unequip@{}", axe.0));
    assert_eq!(
        off["why_not"],
        json!(format!(
            "inventory is full (2 slots): drop 1 of [plank #{}, log #{}] to make room",
            plank.0, log.0
        ))
    );
    let r = call_err(
        &mut app,
        "agent/act",
        json!({"unit": u.0, "controller": "c", "verb": "unequip", "target": axe.0}),
    );
    assert!(r.contains("drop 1 of [plank"), "{r}");
}

/// Proposal 002: a skill used at its default shows on the sheet's Skills
/// line (and in `used_defaults`), with its default, once it was rolled.
#[test]
fn used_default_skill_is_listed() {
    let mut app = app();
    let mut sheet = CharacterSheet::default();
    sheet.attributes.dx = 12;
    sheet.skills.insert("Weaving".into(), 1);
    let (u, ue) = packer(&mut app, SimPos::new(0, 0), Some(sheet));
    let skills_line = |app: &mut App| {
        let t = text(app, u);
        t.lines()
            .find(|l| l.starts_with("Skills:"))
            .unwrap_or_else(|| panic!("{t}"))
            .to_owned()
    };
    let before = skills_line(&mut app);
    assert_eq!(before, "Skills: Weaving 12 (DX).", "not used yet");
    carry(&mut app, ue, Item::new("log").stack(3));
    wield(&mut app, ue, Item::new("hammer"));
    let r = call(
        &mut app,
        "agent/act",
        json!({"unit": u.0, "controller": "c", "affordance": "craft:planks"}),
    );
    assert_eq!(r["accepted"], json!(true), "{r}");
    run_ticks(app.world_mut(), 15);
    let obs = observe(&mut app, u);
    assert_eq!(
        obs["sections"]["sheet"]["used_defaults"],
        json!([{"name": "Carpentry", "level": 7, "default": "DX-5"}])
    );
    assert_eq!(
        skills_line(&mut app),
        "Skills: Weaving 12 (DX), Carpentry 7 (default DX-5)."
    );
}

// ------------------------------------------------- Task 27 (spec F3, 009)

/// [`app`] with perception (and its agent glue) on, every unit scanning
/// every tick.
fn perceiving_app() -> App {
    let mut app = App::new();
    app.insert_resource(SpatialGrid::new(1000, GridBounds::centered(64, 64)))
        .insert_resource(PerceptionConfig {
            stagger: 1,
            ..Default::default()
        });
    app.add_plugins(
        NpcPlugins::new()
            .seed(5)
            .without_driver()
            .with_command()
            .with_vehicles()
            .with_perception()
            .with_agent_io(),
    );
    app.add_plugins(JournalPlugin::default())
        .register_action("Hold", hold_start, hold_stop)
        .register_tree("hold", |b| {
            b.leaf("Hold", "Hold", ());
        });
    app.finish();
    app.cleanup();
    app
}

fn obs_text(app: &mut App, unit: SimId) -> String {
    call(
        app,
        "agent/observe",
        json!({"unit": unit.0, "format": "text"}),
    )["text"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn line_starting<'a>(text: &'a str, start: &str) -> &'a str {
    text.lines()
        .find(|l| l.starts_with(start))
        .unwrap_or_else(|| panic!("no line `{start}` in\n{text}"))
}

/// Proposal 009: a vehicle and a unit seen in passing are listed under
/// "You remember" with their last position, even when the memory holds
/// more than a line's worth of other things (and they have the highest
/// ids: units and vehicles are spawned after the world).
#[test]
fn vehicles_and_units_are_remembered() {
    let mut app = perceiving_app();
    let (eye, eye_e) = with_brain(
        &mut app,
        (
            SimPos::new(500, 500),
            Facing::new(0, -1),
            FixedFacing,
            Sensors::new().vision(15_000, 60),
        ),
    );
    // Ten things south of the eye (low ids), then a wagon and a unit north.
    let mut things = Vec::new();
    for i in 0..10 {
        let (id, _) = spawn_sim(
            app.world_mut(),
            (SimPos::new(i * 500 - 2000, -4500), Perceivable::new(2)),
        );
        things.push(id);
    }
    let (wagon, wagon_e) = wagon(&mut app, SimPos::new(500, 6500));
    app.world_mut()
        .entity_mut(wagon_e)
        .insert(Perceivable::new(4));
    let (unit, _) = with_brain(
        &mut app,
        (
            SimPos::new(2500, 6500),
            Perceivable::new(1),
            Commandable::new(100),
            Owner(PlayerId(2)),
        ),
    );
    attach(&mut app, eye, "Direct");
    run_ticks(app.world_mut(), 2);
    // Turn north: the wagon and the unit are seen, the ten things remembered.
    *app.world_mut().get_mut::<Facing>(eye_e).unwrap() = Facing::new(0, 1);
    run_ticks(app.world_mut(), 2);
    // Turn east, at nothing: everything is remembered now.
    *app.world_mut().get_mut::<Facing>(eye_e).unwrap() = Facing::new(1, 0);
    run_ticks(app.world_mut(), 2);
    let mem = app.world().get::<PerceptionMemory>(eye_e).unwrap().clone();
    assert_eq!(mem.percepts.len(), 12, "{mem:?}");
    assert!(mem.percepts.values().all(|p| !p.sensed));

    let text = obs_text(&mut app, eye);
    let remember = line_starting(&text, "You remember:");
    for (id, at) in [(wagon, "(0.50 m, 6.50 m)"), (unit, "(2.50 m, 6.50 m)")] {
        assert!(
            remember.contains(&format!("#{} last seen at {at}", id.0)),
            "#{} missing: {remember}",
            id.0
        );
    }
    for id in &things {
        assert!(
            remember.contains(&format!("#{} last seen", id.0)),
            "{remember}"
        );
    }
    assert!(!remember.contains("more"), "{remember}");
    // Most recently seen first.
    let w = remember.find(&format!("#{} ", wagon.0)).unwrap();
    let t = remember.find(&format!("#{} ", things[0].0)).unwrap();
    assert!(w < t, "{remember}");
}

/// Proposals 007 and 011: things within reach are listed on their own
/// line, out-of-view ones marked; reach-sensed things are offered.
#[test]
fn within_reach_line_names_what_is_behind() {
    let mut app = perceiving_app();
    let (eye, eye_e) = with_brain(
        &mut app,
        (
            SimPos::new(500, 500),
            Facing::new(0, 1),
            FixedFacing,
            Sensors::new().vision(15_000, 60),
            npc_framework::perception::ReachSense { radius_mm: 1500 },
        ),
    );
    let (behind, _) = spawn_sim(app.world_mut(), (SimPos::new(500, 0), Perceivable::new(2)));
    let (ahead, _) = spawn_sim(
        app.world_mut(),
        (SimPos::new(500, 1500), Perceivable::new(2)),
    );
    let (far, _) = spawn_sim(
        app.world_mut(),
        (SimPos::new(500, 5500), Perceivable::new(2)),
    );
    attach(&mut app, eye, "Direct");
    run_ticks(app.world_mut(), 2);
    let obs = observe(&mut app, eye);
    let p = &obs["sections"]["perception"];
    assert_eq!(p["reach_mm"], json!(1500));
    assert_eq!(
        p["within_reach"],
        json!([behind.0, ahead.0]),
        "nearest first: {p}"
    );
    let text = obs_text(&mut app, eye);
    let reach = line_starting(&text, "Within reach:");
    let name = |id: SimId| agent_io::observe::describe(app.world(), id);
    assert_eq!(
        reach,
        format!(
            "Within reach: {} (behind you), {}.",
            name(behind),
            name(ahead)
        )
    );
    assert!(!reach.contains(&format!("#{}", far.0)));
    assert!(
        line_starting(&text, "You perceive:").contains(&format!(
            "#{} at (0.50 m, 0.00 m), 0.50 m away (reach)",
            behind.0
        )),
        "{text}"
    );
    let _ = eye_e;
}

/// Without a reach sense there is no "Within reach" line.
#[test]
fn no_reach_line_without_reach_sense() {
    let mut app = perceiving_app();
    let (eye, _) = with_brain(
        &mut app,
        (
            SimPos::new(500, 500),
            Facing::new(0, 1),
            FixedFacing,
            Sensors::new().vision(15_000, 60),
        ),
    );
    spawn_sim(
        app.world_mut(),
        (SimPos::new(500, 1500), Perceivable::new(2)),
    );
    attach(&mut app, eye, "Direct");
    run_ticks(app.world_mut(), 2);
    let obs = observe(&mut app, eye);
    assert_eq!(obs["sections"]["perception"]["reach_mm"], Value::Null);
    let text = obs_text(&mut app, eye);
    assert!(!text.contains("Within reach"), "{text}");
}

/// Review fix 1: "You remember" lists the 24 most recent things plus every
/// vehicle and unit, then a per-kind summary; the JSON stays complete. The
/// wagon and the unit are the oldest memories and still listed.
#[test]
fn oldest_vehicle_and_unit_stay_listed_past_the_cap() {
    let mut app = perceiving_app();
    let (eye, eye_e) = with_brain(
        &mut app,
        (
            SimPos::new(500, 500),
            Facing::new(0, 1),
            FixedFacing,
            Sensors::new().vision(15_000, 60),
        ),
    );
    let (wagon, wagon_e) = wagon(&mut app, SimPos::new(500, 6500));
    app.world_mut()
        .entity_mut(wagon_e)
        .insert(Perceivable::new(4));
    let (unit, _) = with_brain(
        &mut app,
        (
            SimPos::new(2500, 6500),
            Perceivable::new(1),
            Owner(PlayerId(2)),
        ),
    );
    // Thirty plain things south of the eye.
    let mut things = Vec::new();
    for i in 0..30 {
        let (id, _) = spawn_sim(
            app.world_mut(),
            (
                SimPos::new((i % 10) * 500 - 2000, -4500 - (i / 10) * 500),
                Perceivable::new(2),
            ),
        );
        things.push(id);
    }
    attach(&mut app, eye, "Direct");
    // North first: the wagon and the unit are the oldest memories.
    run_ticks(app.world_mut(), 2);
    *app.world_mut().get_mut::<Facing>(eye_e).unwrap() = Facing::new(0, -1);
    run_ticks(app.world_mut(), 2);
    *app.world_mut().get_mut::<Facing>(eye_e).unwrap() = Facing::new(1, 0);
    run_ticks(app.world_mut(), 2);
    let mem = app.world().get::<PerceptionMemory>(eye_e).unwrap().clone();
    assert_eq!(mem.percepts.len(), 32, "{mem:?}");
    let oldest = mem.percepts.values().map(|p| p.last_seen_tick).min();
    assert_eq!(mem.get(wagon).map(|p| p.last_seen_tick), oldest);

    let obs = observe(&mut app, eye);
    assert_eq!(
        obs["sections"]["perception"]["remembered"]
            .as_array()
            .unwrap()
            .len(),
        32,
        "the JSON lists everything"
    );
    let text = obs_text(&mut app, eye);
    let remember = line_starting(&text, "You remember:");
    for (id, at) in [(wagon, "(0.50 m, 6.50 m)"), (unit, "(2.50 m, 6.50 m)")] {
        assert!(
            remember.contains(&format!("#{} last seen at {at}", id.0)),
            "#{} missing: {remember}",
            id.0
        );
    }
    let listed = things
        .iter()
        .filter(|id| remember.contains(&format!("#{} last seen", id.0)))
        .count();
    assert_eq!(listed, agent_io_glue::REMEMBER_TEXT_LIMIT, "{remember}");
    let kind = {
        let e = app.world().resource::<SimIndex>().get(things[0]).unwrap();
        npc_framework::kernel::inspect::kind_name(app.world(), e).to_owned()
    };
    assert!(
        remember.ends_with(&format!("; and 6 more (6 {kind}).")),
        "{remember}"
    );
}

/// Review fix 2: "You perceive" lists the 8 nearest plus every vehicle and
/// unit, then a per-kind summary.
#[test]
fn far_sensed_wagon_is_never_hidden() {
    let mut app = perceiving_app();
    let (eye, _) = with_brain(
        &mut app,
        (
            SimPos::new(500, 500),
            Facing::new(0, 1),
            FixedFacing,
            Sensors::new().vision(15_000, 60),
        ),
    );
    let (wagon, wagon_e) = wagon(&mut app, SimPos::new(500, 12_500));
    app.world_mut()
        .entity_mut(wagon_e)
        .insert(Perceivable::new(4));
    let mut things = Vec::new();
    for i in 0..10 {
        let (id, _) = spawn_sim(
            app.world_mut(),
            (SimPos::new(500, 2000 + i * 500), Perceivable::new(2)),
        );
        things.push(id);
    }
    attach(&mut app, eye, "Direct");
    run_ticks(app.world_mut(), 2);
    let text = obs_text(&mut app, eye);
    let perceive = line_starting(&text, "You perceive:");
    assert!(
        perceive.contains(&format!("#{} at (0.50 m, 12.50 m)", wagon.0)),
        "{perceive}"
    );
    // The eight nearest are listed, the two farthest summarized.
    for (i, id) in things.iter().enumerate() {
        assert_eq!(
            perceive.contains(&format!("#{} at", id.0)),
            i < agent_io_glue::PERCEIVE_TEXT_LIMIT,
            "{perceive}"
        );
    }
    assert!(perceive.contains("; and 2 more (2 "), "{perceive}");
}

// ------------------------------------------------- Task 28 (spec F4, 004)

/// An explored record over the test grid with the fine cells
/// `x0..x1 × z0..z1` set where `keep(x, z)` holds.
fn explored_record(
    app: &App,
    x: std::ops::Range<i32>,
    z: std::ops::Range<i32>,
    keep: impl Fn(i32, i32) -> bool,
) -> npc_framework::perception::ExploredCells {
    let bounds = app.world().resource::<SpatialGrid>().bounds();
    let mut rec = npc_framework::perception::ExploredCells::sized(bounds);
    for cx in x {
        for cz in z.clone() {
            if keep(cx, cz) {
                let i = bounds
                    .index(npc_framework::kernel::spatial::CellCoord::new(cx, cz))
                    .unwrap();
                rec.bits[(i / 64) as usize] |= 1 << (i % 64);
            }
        }
    }
    rec
}

fn searched_unit(app: &mut App, rec: Option<npc_framework::perception::ExploredCells>) -> SimId {
    let (id, e) = with_brain(
        app,
        (
            SimPos::new(0, 0),
            Facing::new(0, 1),
            FixedFacing,
            Sensors::new().vision(15_000, 60),
        ),
    );
    if let Some(rec) = rec {
        app.world_mut().entity_mut(e).insert(rec);
    }
    run_ticks(app.world_mut(), 1);
    id
}

fn sector(obs: &Value, dir: &str) -> (u64, u64) {
    let s = obs["sections"]["perception"]["searched"]["sectors"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["dir"] == json!(dir))
        .unwrap_or_else(|| panic!("no sector {dir}"));
    (
        s["explored"].as_u64().unwrap(),
        s["total"].as_u64().unwrap(),
    )
}

/// Proposal 004: the explored cells, coarsened to 10 m, are summarised in
/// the observation.
#[test]
fn searched_summary_counts_explored_cells() {
    let mut app = perceiving_app();
    // Fine cells 0..20 x 0..20: the four 10 m cells north-east of the actor.
    let rec = explored_record(&app, 0..20, 0..20, |_, _| true);
    let unit = searched_unit(&mut app, Some(rec));
    let obs = observe(&mut app, unit);
    let s = &obs["sections"]["perception"]["searched"];
    assert_eq!(s["cell_mm"], json!(10_000));
    assert_eq!(s["radius_mm"], json!(60_000));
    // 4 of the 64 coarse cells of the 64 m grid; 400 of 4096 fine cells.
    assert_eq!(s["within_radius_pct"], json!(6));
    assert_eq!(s["map_pct"], json!(9));
    let sectors = s["sectors"].as_array().unwrap();
    let dirs: Vec<&str> = sectors.iter().map(|s| s["dir"].as_str().unwrap()).collect();
    assert_eq!(dirs, ["N", "NE", "E", "SE", "S", "SW", "W", "NW"]);
    let total: u64 = sectors.iter().map(|s| s["total"].as_u64().unwrap()).sum();
    assert_eq!(total, 64);
    // Centres (5,5) and (15,15) are north-east, (15,5) east, (5,15) north.
    assert_eq!(sector(&obs, "NE").0, 2);
    assert_eq!(sector(&obs, "E").0, 1);
    assert_eq!(sector(&obs, "N").0, 1);
    let others: u64 = ["SE", "S", "SW", "W", "NW"]
        .iter()
        .map(|d| sector(&obs, d).0)
        .sum();
    assert_eq!(others, 0);
    let text = obs_text(&mut app, unit);
    let line = line_starting(&text, "Searched:");
    assert!(
        line.starts_with("Searched: 6% within 60 m, 9% of the map; least searched: "),
        "{line}"
    );
}

/// The least searched sectors come lowest share first, ties in the fixed
/// order N, NE, E, SE, S, SW, W, NW; a fully searched ring lists none.
#[test]
fn least_searched_sectors() {
    let mut app = perceiving_app();
    // Everything explored but the western ground (x < -10).
    let rec = explored_record(&app, -32..32, -32..32, |x, _| x >= -10);
    let unit = searched_unit(&mut app, Some(rec));
    let obs = observe(&mut app, unit);
    assert_eq!(sector(&obs, "W").0, 0);
    assert!(sector(&obs, "W").1 > 0);
    assert_eq!(sector(&obs, "E").0, sector(&obs, "E").1);
    let text = obs_text(&mut app, unit);
    let line = line_starting(&text, "Searched:");
    assert!(
        line.ends_with("; least searched: west, south-west."),
        "{line}"
    );

    // All explored: nothing is least searched.
    let mut app = perceiving_app();
    let rec = explored_record(&app, -32..32, -32..32, |_, _| true);
    let unit = searched_unit(&mut app, Some(rec));
    let text = obs_text(&mut app, unit);
    let line = line_starting(&text, "Searched:");
    assert_eq!(line, "Searched: 100% within 60 m, 100% of the map.");
}

/// Only actors with an explored-cells record get the summary.
#[test]
fn no_summary_without_fog_record() {
    let mut app = perceiving_app();
    let unit = searched_unit(&mut app, None);
    let obs = observe(&mut app, unit);
    assert!(obs["sections"]["perception"].get("searched").is_none());
    let text = obs_text(&mut app, unit);
    assert!(!text.contains("Searched"), "{text}");
}
