//! Actor Testbed glue scenarios (wave G2b): the wave F4 capability glue on
//! the testbed's own content, headless, one embedded scenario each.
//!
//! - command × vehicles: a `ByCrew` cart ordered through its driver carries
//!   its passenger, then `Unload`; `Mount` orders; the `Direct` wagon with
//!   crew aboard;
//! - command × interaction: queued `Interact` orders fetch the key, unlock
//!   and open the store, walk in and pick up the pickaxe;
//! - crafting: mine ore, smelt an ingot at the forge, forge an axe at the
//!   anvil (hammer carried), quality from the Smithing margin;
//! - progression: crafting XP levels up and the AI spend policy improves the
//!   most-used skill; the Player policy banks points for `SpendPoints`;
//!   studying a book (`Teaches`) and teaching by the smith (`Teacher`),
//!   both gated by level;
//! - agent io: a `Direct` unit fetches the key, enters the store, takes the
//!   pickaxe and mines ore through `observe` / `act` only, reading
//!   affordance `why_not`s on the way; an `Advisor` orders a squad in
//!   formation;
//! - faction ownership: orders across players are rejected.

use bevy_app::App;
use bevy_ecs::prelude::*;
use npc_framework::agent_io;
use npc_framework::agent_io::scenario::SessionRecord;
use npc_framework::kernel::journal::{JournalEntry, JournalKind};
use npc_framework::kernel::replay::Recorder;
use npc_framework::prelude::*;
use npc_framework::rules::SpendPoints;
use serde_json::{Value, json};
use testbed_content::{TestbedIds, try_new_sim_app_with};

// ---------------------------------------------------------------- helpers

fn finish(mut app: App) -> App {
    app.finish();
    app.cleanup();
    app
}

fn journal_plugin(app: &mut App) {
    app.add_plugins(JournalPlugin {
        verbose: false,
        capacity: 400_000,
    });
}

/// Embedded scenario `name`, seed 1, journal on, finished.
fn app(name: &str) -> App {
    finish(try_new_sim_app_with(1, name, journal_plugin).unwrap_or_else(|e| panic!("{name}: {e}")))
}

/// An inline testbed scenario (`world` and `actors` as in the files).
fn inline(doc: Value) -> App {
    let doc = agent_io::ScenarioDoc::from_value(doc).unwrap();
    let loaded = testbed_content::scenario::load_with(&doc, 1, journal_plugin).unwrap();
    finish(loaded.app)
}

fn id(app: &App, name: &str) -> SimId {
    app.world().resource::<TestbedIds>().id(name)
}

fn ent(app: &App, id: SimId) -> Entity {
    app.world()
        .resource::<SimIndex>()
        .get(id)
        .expect("in the sim")
}

fn get<T: Component + Clone>(app: &App, id: SimId) -> Option<T> {
    app.world().get::<T>(ent(app, id)).cloned()
}

fn pos(app: &App, id: SimId) -> SimPos {
    get::<SimPos>(app, id).unwrap()
}

fn run(app: &mut App, n: u64) {
    run_ticks(app.world_mut(), n);
}

fn tick(app: &App) -> u64 {
    app.world().resource::<SimClock>().tick
}

fn content<C: ContentCommand>(app: &mut App, cmd: &C) -> Result<(), CommandError> {
    submit(app.world_mut(), None, SimCommand::content(cmd)).map(|_| ())
}

fn reason(err: CommandError) -> String {
    match err {
        CommandError::Unavailable { reason } => reason,
        other => other.to_string(),
    }
}

const P1: PlayerId = PlayerId(1);

fn issue(app: &mut App, player: u32, units: &[SimId], o: UnitOrder, queue: bool) {
    let cmd = IssueOrder::new(PlayerId(player), units.to_vec(), o).queued(queue);
    submit_order(app.world_mut(), &cmd).unwrap_or_else(|e| panic!("{cmd:?}: {e}"));
}

fn interact_order(verb: &str, target: SimId) -> UnitOrder {
    UnitOrder::Interact {
        verb: verb.to_owned(),
        target,
    }
}

fn mv(x: i32, z: i32) -> UnitOrder {
    UnitOrder::Move { pos: (x, z) }
}

fn journal(app: &App) -> Vec<JournalEntry> {
    app.world()
        .resource::<Journal>()
        .entries
        .iter()
        .cloned()
        .collect()
}

fn of(app: &App, unit: SimId) -> Vec<JournalKind> {
    journal(app)
        .into_iter()
        .filter(|e| e.unit == Some(unit))
        .map(|e| e.kind)
        .collect()
}

/// Order kinds `unit` completed, in order.
fn orders_done(app: &App, unit: SimId) -> Vec<String> {
    of(app, unit)
        .into_iter()
        .filter_map(|k| match k {
            JournalKind::OrderDone { order } => Some(order),
            _ => None,
        })
        .collect()
}

fn orders_failed(app: &App, unit: SimId) -> Vec<(String, String)> {
    of(app, unit)
        .into_iter()
        .filter_map(|k| match k {
            JournalKind::OrderFailed { order, reason, .. } => Some((order, reason)),
            _ => None,
        })
        .collect()
}

fn idle(app: &App, unit: SimId) -> bool {
    get::<OrderQueue>(app, unit).is_none_or(|q| q.is_empty())
}

/// Runs until `unit` has no orders left (at most `limit` ticks).
fn run_orders(app: &mut App, unit: SimId, limit: u64) {
    let end = tick(app) + limit;
    run(app, 1);
    while !idle(app, unit) {
        assert!(
            tick(app) < end,
            "#{} still busy at tick {}: {:?}",
            unit.0,
            tick(app),
            get::<OrderQueue>(app, unit)
        );
        run(app, 5);
    }
}

/// `(id, kind, stack, quality)` of what `unit` carries (inventory and hands).
fn carried(app: &App, unit: SimId) -> Vec<(SimId, String, u32, i32)> {
    let mut ids = get::<Inventory>(app, unit).unwrap().items.clone();
    ids.extend(get::<Equipment>(app, unit).unwrap().items());
    ids.sort_unstable();
    ids.dedup();
    ids.into_iter()
        .map(|i| {
            let item = get::<Item>(app, i).unwrap();
            (i, item.kind, item.stack, item.quality)
        })
        .collect()
}

fn count(app: &App, unit: SimId, kind: &str) -> u32 {
    carried(app, unit)
        .iter()
        .filter(|c| c.1 == kind)
        .map(|c| c.2)
        .sum()
}

fn skill(app: &App, unit: SimId, name: &str) -> i32 {
    effective_skill(app.world(), unit, name).unwrap()
}

fn sheet(app: &App, unit: SimId) -> CharacterSheet {
    get::<CharacterSheet>(app, unit).unwrap()
}

fn mounted(app: &App, unit: SimId) -> Option<(SimId, u8)> {
    get::<MountedIn>(app, unit).map(|m| (m.vehicle, m.seat))
}

fn call(app: &mut App, method: &str, params: Value) -> Value {
    agent_io::call(app.world_mut(), method, Some(params))
        .unwrap_or_else(|e| panic!("{method}: {e:?}"))
}

// ---------------------------------------------------------------- vehicles

#[test]
fn mount_orders_board_the_cart_and_its_driver_takes_the_carts_orders() {
    let mut app = app("vehicles");
    let (driver, passenger, cart) = (id(&app, "driver"), id(&app, "passenger"), id(&app, "cart"));
    issue(
        &mut app,
        1,
        &[driver],
        UnitOrder::Mount { vehicle: cart },
        false,
    );
    issue(
        &mut app,
        1,
        &[passenger],
        UnitOrder::Mount { vehicle: cart },
        false,
    );
    run_orders(&mut app, driver, 200);
    run_orders(&mut app, passenger, 200);
    assert_eq!(mounted(&app, driver), Some((cart, 0)), "driver seat first");
    assert_eq!(mounted(&app, passenger), Some((cart, 1)));
    assert_eq!(orders_done(&app, driver), ["Mount"]);
    assert_eq!(orders_done(&app, passenger), ["Mount"]);

    // The cart's orders go to its driver (`npc_vehicles.crew` redirect).
    issue(&mut app, 1, &[cart], mv(4500, -1500), false);
    issue(&mut app, 1, &[cart], UnitOrder::Unload, true);
    run(&mut app, 1);
    assert_eq!(get::<OrderQueue>(&app, driver).unwrap().len(), 2);
    assert!(idle(&app, cart), "forwarded, not queued on the cart");
    run(&mut app, 20);
    let mid = pos(&app, cart);
    assert!(mid.z > -5500 && mid.z < -1500, "under way: {mid:?}");
    assert_eq!(
        pos(&app, passenger),
        SimPos::new(mid.x, mid.z - 700),
        "seat"
    );
    run_orders(&mut app, driver, 300);
    assert_eq!(pos(&app, cart), SimPos::new(4500, -1500));
    assert_eq!(orders_done(&app, driver), ["Mount", "Move", "Unload"]);
    assert_eq!(mounted(&app, driver), None, "unloaded");
    assert_eq!(mounted(&app, passenger), None, "unloaded");
    // Placed on free cells next to the cart.
    for u in [driver, passenger] {
        let d2 = pos(&app, u).dist2(pos(&app, cart));
        assert!(
            d2 > 0 && d2 <= 2500 * 2500,
            "#{} at {:?}",
            u.0,
            pos(&app, u)
        );
    }
    assert!(
        journal(&app)
            .iter()
            .any(|e| e.unit == Some(passenger) && matches!(e.kind, JournalKind::Dismounted { .. }))
    );
}

#[test]
fn the_wagon_takes_orders_with_its_crew_aboard() {
    let mut app = app("vehicles");
    let (driver, passenger, wagon) = (id(&app, "driver"), id(&app, "passenger"), id(&app, "wagon"));
    // Out through the gate (nav opens it), then board the wagon.
    for u in [driver, passenger] {
        issue(
            &mut app,
            1,
            &[u],
            UnitOrder::Mount { vehicle: wagon },
            false,
        );
    }
    run_orders(&mut app, driver, 1500);
    run_orders(&mut app, passenger, 1500);
    assert_eq!(mounted(&app, driver).map(|m| m.0), Some(wagon));
    assert_eq!(mounted(&app, passenger).map(|m| m.0), Some(wagon));
    assert!(get::<Door>(&app, id(&app, "gate")).unwrap().open);
    // A Direct vehicle is a unit: ordered itself, it moves with its crew.
    let goal = SimPos::new(10_500, -14_500);
    issue(&mut app, 1, &[wagon], mv(goal.x, goal.z), false);
    issue(&mut app, 1, &[wagon], UnitOrder::Unload, true);
    run(&mut app, 30);
    let mid = pos(&app, wagon);
    let seat = mounted(&app, passenger).unwrap().1;
    assert!(mid.x > 500, "under way: {mid:?}");
    assert!(seat > 0);
    assert_eq!(pos(&app, passenger), SimPos::new(mid.x, mid.z - 900));
    run_orders(&mut app, wagon, 1000);
    assert_eq!(pos(&app, wagon), goal);
    assert_eq!(orders_done(&app, wagon), ["Move", "Unload"]);
    assert_eq!(mounted(&app, driver), None);
    assert_eq!(mounted(&app, passenger), None);
}

/// Proposal 010's case: in the explorer world an agent mounts the `Direct`
/// wagon (a `NavAgent` + `Commandable` unit of its own, no tree) in its
/// driver seat ("Mounted {role: Driver, seat: 0}") and `drive_to` was
/// refused "riding #30 but not in its driver seat". Now the seat steers it.
#[test]
fn the_wagon_is_driven_from_its_driver_seat() {
    let mut app = inline(
        json!({"version": 1, "content": "testbed", "name": "wagon_seat",
        "world": {"layout": "frontier", "furnish": true, "store_locked": true,
                  "detection": "per_roll"},
        "actors": [{"kind": "unit", "name": "explorer", "template": "scout",
                    "pos": [1500, -14500], "facing": [0, -1]}]}),
    );
    let (explorer, wagon) = (id(&app, "explorer"), id(&app, "wagon"));
    assert_eq!(pos(&app, wagon), SimPos::new(500, -14_500));
    // The explorer's controller, as the explorer scenario attaches it.
    call(
        &mut app,
        "agent/attach",
        json!({"unit": explorer.0, "controller": "explorer", "mode": "Direct", "on_idle": "Idle"}),
    );
    run(&mut app, 1);
    let mount = Mount {
        unit: explorer.0,
        vehicle: wagon.0,
        role: Some(SeatRole::Driver),
    };
    content(&mut app, &mount).unwrap();
    run(&mut app, 2);
    assert_eq!(mounted(&app, explorer), Some((wagon, 0)));
    assert!(of(&app, explorer).iter().any(
        |k| matches!(k, JournalKind::Mounted { vehicle, seat: 0, role } if *vehicle == wagon && role == "Driver")
    ));
    let obs = call(
        &mut app,
        "agent/observe",
        json!({"unit": explorer.0, "format": "text"}),
    );
    let text = obs["text"].as_str().unwrap();
    assert!(
        text.contains(&format!(
            "You ride vehicle #{} (self-propelled, steered from its driver seat)",
            wagon.0
        )),
        "{text}"
    );
    let r = call(
        &mut app,
        "agent/act",
        json!({"unit": explorer.0, "controller": "explorer", "verb": "drive_to",
               "args": {"pos": [0, -24_000]}}),
    );
    assert_eq!(r["accepted"], json!(true), "{r}");
    run(&mut app, 400);
    let at = pos(&app, wagon);
    assert!(
        at.dist2(SimPos::new(0, -24_000)) <= 1000 * 1000,
        "the wagon drove: {at:?}"
    );
    assert_eq!(
        mounted(&app, explorer),
        Some((wagon, 0)),
        "still at the wheel"
    );
}

// ---------------------------------------------------------------- interact orders and crafting

/// Queues the smithy fetch: key → unlock → open → into the store → the
/// pickaxe and the hammer → out to the ore rock.
fn queue_fetch(app: &mut App, smith: SimId) {
    let (key, door, pickaxe, hammer) = (
        id(app, "store_key"),
        id(app, "store_door"),
        id(app, "pickaxe"),
        id(app, "hammer"),
    );
    issue(app, 1, &[smith], interact_order("pick_up", key), false);
    let rest = [
        interact_order("unlock", door),
        interact_order("open", door),
        mv(5500, 6500),
        interact_order("pick_up", pickaxe),
        interact_order("pick_up", hammer),
        mv(ORE_SPOT.0, ORE_SPOT.1),
    ];
    for o in rest {
        issue(app, 1, &[smith], o, true);
    }
}

/// Next to the ore rock (9, −7).
const ORE_SPOT: (i32, i32) = (8500, -6500);

/// Ore lying on the ground, ascending `SimId`.
fn ground_ore(app: &mut App) -> Vec<SimId> {
    let world = app.world_mut();
    let mut v: Vec<SimId> = world
        .query_filtered::<(&SimId, &Item), Without<CarriedBy>>()
        .iter(world)
        .filter(|(_, i)| i.kind == "ore")
        .map(|(&id, _)| id)
        .collect();
    v.sort_unstable();
    v
}

/// Ore units lying on the ground.
fn ground_ore_count(app: &mut App) -> u32 {
    ground_ore(app)
        .into_iter()
        .map(|o| get::<Item>(app, o).unwrap().stack)
        .sum()
}

/// Equips the pickaxe (now carried), mines until two ore lie on the ground
/// (a failed Mining roll yields none), picks them up.
fn mine_two_ore(app: &mut App, smith: SimId) {
    let (pickaxe, ore) = (id(app, "pickaxe"), id(app, "ore_rock"));
    issue(app, 1, &[smith], interact_order("equip", pickaxe), false);
    run_orders(app, smith, 100);
    let mut swings = 0;
    while ground_ore_count(app) < 2 {
        issue(app, 1, &[smith], interact_order("use_tool_on", ore), false);
        run_orders(app, smith, 300);
        swings += 1;
        assert!(swings <= 6, "the rock has 6 uses");
    }
    for o in ground_ore(app) {
        issue(app, 1, &[smith], interact_order("pick_up", o), true);
    }
    run_orders(app, smith, 300);
}

/// Between the forge (−2, 8) and the anvil (−4, 7), in reach of both.
const FORGE_SPOT: (i32, i32) = (-2500, 7500);

#[test]
fn queued_interact_orders_fetch_the_key_open_the_store_and_take_the_pickaxe() {
    let mut app = app("smithy");
    let (smith, door, pickaxe) = (
        id(&app, "smith"),
        id(&app, "store_door"),
        id(&app, "pickaxe"),
    );
    queue_fetch(&mut app, smith);
    run(&mut app, 1);
    assert_eq!(get::<OrderQueue>(&app, smith).unwrap().len(), 7);
    run_orders(&mut app, smith, 3000);
    let d = get::<Door>(&app, door).unwrap();
    assert!(d.open && !d.is_locked(), "{d:?}");
    let done = orders_done(&app, smith);
    assert_eq!(
        &done[..6],
        [
            "Interact", "Interact", "Interact", "Move", "Interact", "Interact"
        ],
        "{:?}",
        orders_failed(&app, smith)
    );
    let items: Vec<String> = carried(&app, smith).into_iter().map(|c| c.1).collect();
    for kind in ["store_key", "pickaxe", "hammer", "plank"] {
        assert!(items.iter().any(|k| k == kind), "{kind} in {items:?}");
    }
    assert!(get::<Equipment>(&app, smith).unwrap().items().is_empty());
    assert_eq!(pos(&app, smith), SimPos::new(ORE_SPOT.0, ORE_SPOT.1));
    assert!(orders_failed(&app, smith).is_empty());
    // The pickaxe can only be equipped once carried, and only then it mines.
    let ore = id(&app, "ore_rock");
    let e = content(&mut app, &Interact::new(smith, "use_tool_on", ore)).unwrap_err();
    assert_eq!(reason(e), "equip tool:pickaxe first");
    mine_two_ore(&mut app, smith);
    assert_eq!(
        get::<Equipment>(&app, smith).unwrap().items(),
        vec![pickaxe]
    );
    assert_eq!(count(&app, smith, "ore"), 2);
    // Every verb was journaled as an interaction.
    let verbs: Vec<String> = of(&app, smith)
        .into_iter()
        .filter_map(|k| match k {
            JournalKind::Interacted { verb, outcome, .. } if outcome == "success" => Some(verb),
            _ => None,
        })
        .collect();
    assert_eq!(
        &verbs[..6],
        ["pick_up", "unlock", "open", "pick_up", "pick_up", "equip"]
    );
    assert!(verbs[6..].contains(&"use_tool_on".to_owned()), "{verbs:?}");
    assert_eq!(verbs.last().map(String::as_str), Some("pick_up"), "the ore");
}

/// Crafts `recipe` at `station` until it succeeds (at most `tries`);
/// returns the `CraftDone` `(margin, quality, outputs)`.
fn craft_until_done(
    app: &mut App,
    unit: SimId,
    recipe: &str,
    station: SimId,
    tries: usize,
) -> (i32, i32, Vec<SimId>) {
    for _ in 0..tries {
        let before = journal(app).len();
        content(app, &Craft::new(unit, recipe).at(station))
            .unwrap_or_else(|e| panic!("craft {recipe}: {}", reason(e)));
        let duration = app
            .world()
            .resource::<npc_framework::crafting::RecipeRegistry>()
            .recipes[recipe]
            .duration_ticks;
        run(app, u64::from(duration) + 2);
        let done = journal(app)[before..].iter().find_map(|e| match &e.kind {
            JournalKind::CraftDone {
                recipe: r,
                margin,
                quality,
                outputs,
                ..
            } if e.unit == Some(unit) && r == recipe => Some((*margin, *quality, outputs.clone())),
            _ => None,
        });
        if let Some(d) = done {
            return d;
        }
    }
    panic!("{recipe} never succeeded in {tries} tries");
}

#[test]
fn mine_ore_smelt_an_ingot_and_forge_an_axe_with_quality_from_smithing() {
    let mut app = app("smithy");
    let (smith, forge, anvil, ore) = (
        id(&app, "smith"),
        id(&app, "forge"),
        id(&app, "anvil"),
        id(&app, "ore_rock"),
    );
    queue_fetch(&mut app, smith);
    run_orders(&mut app, smith, 3000);
    mine_two_ore(&mut app, smith);
    assert_eq!(count(&app, smith, "ore"), 2);
    issue(&mut app, 1, &[smith], mv(FORGE_SPOT.0, FORGE_SPOT.1), false);
    run_orders(&mut app, smith, 3000);
    let _ = ore;
    let mining = of(&app, smith).into_iter().any(|k| {
        matches!(k, JournalKind::Roll { skill, purpose, .. } if skill == "Mining" && purpose.starts_with("interaction"))
    });
    assert!(mining, "mining is a Mining check");

    // The forge smelts two ore into an ingot (Smithing roll).
    let e = content(&mut app, &Craft::new(smith, "forge_axe").at(anvil)).unwrap_err();
    assert_eq!(
        reason(e),
        "needs 1 ingot (has 0), 1 plank (has 1), tool hammer (has 1)"
    );
    craft_until_done(&mut app, smith, "ingot", forge, 6);
    assert_eq!(count(&app, smith, "ingot"), 1);
    // The anvil forges the axe from the ingot and the plank, with the hammer
    // carried; quality follows the Smithing margin.
    let (margin, quality, outputs) = craft_until_done(&mut app, smith, "forge_axe", anvil, 6);
    assert_eq!(count(&app, smith, "ingot"), 0);
    assert_eq!(count(&app, smith, "plank"), 0);
    assert_eq!(outputs.len(), 1);
    let axe = get::<Item>(&app, outputs[0]).unwrap();
    assert_eq!(axe.kind, "axe");
    assert_eq!(axe.quality, quality);
    let base = (margin / 2).clamp(0, 5);
    assert!(
        quality == base || quality == base + 2,
        "quality {quality} from margin {margin}"
    );
    let rolls: Vec<(i32, String)> = of(&app, smith)
        .into_iter()
        .filter_map(|k| match k {
            JournalKind::Roll {
                skill,
                purpose,
                target,
                ..
            } if purpose == "craft.forge_axe" => Some((target, skill)),
            _ => None,
        })
        .collect();
    let smithing = skill(&app, smith, "Smithing");
    assert!(!rolls.is_empty());
    assert!(
        rolls
            .iter()
            .all(|r| r.1 == "Smithing" && r.0 <= smithing - 2),
        "Smithing −2: {rolls:?} (skill {smithing})"
    );
    // Crafting gave XP (`craft` source).
    assert!(of(&app, smith).into_iter().any(
        |k| matches!(k, JournalKind::XpGained { source, amount, .. } if source == "craft" && amount == 30)
    ));
}

// ---------------------------------------------------------------- progression

fn xp_by_source(app: &App, unit: SimId) -> std::collections::BTreeMap<String, u32> {
    let mut m = std::collections::BTreeMap::new();
    for k in of(app, unit) {
        if let JournalKind::XpGained { source, amount, .. } = k {
            *m.entry(source).or_insert(0) += amount;
        }
    }
    m
}

fn level_ups(app: &App, unit: SimId) -> Vec<(u32, i32)> {
    of(app, unit)
        .into_iter()
        .filter_map(|k| match k {
            JournalKind::LevelUp { level, points } => Some((level, points)),
            _ => None,
        })
        .collect()
}

/// Crafts planks at the workbench until `unit` levels up (at most `tries`).
fn plane_until_level_up(app: &mut App, unit: SimId, tries: usize) {
    let bench = id(app, "workbench");
    for _ in 0..tries {
        content(app, &Craft::new(unit, "planks").at(bench)).unwrap();
        run(app, 42);
        if !level_ups(app, unit).is_empty() {
            return;
        }
    }
    panic!(
        "no level up after {tries} crafts: {:?}",
        xp_by_source(app, unit)
    );
}

#[test]
fn crafting_xp_levels_up_and_the_ai_policy_improves_the_most_used_skill() {
    let mut app = app("craft");
    let master = id(&app, "master");
    let before = sheet(&app, master);
    let carpentry = skill(&app, master, "Carpentry");
    assert_eq!(
        get::<SpendPolicy>(&app, master),
        Some(SpendPolicy::Ai),
        "the default policy"
    );
    plane_until_level_up(&mut app, master, 8);
    let xp = xp_by_source(&app, master);
    assert!(xp.get("craft").is_some_and(|&a| a > 0), "{xp:?}");
    assert!(xp.get("roll").is_some_and(|&a| a > 0), "{xp:?}");
    assert_eq!(level_ups(&app, master), [(1, 5)]);
    let exp = get::<Experience>(&app, master).unwrap();
    assert_eq!(exp.level, 1);
    // The AI spends its points at once (the template's banked ones and the
    // level's 5), on Carpentry: the only skill it used (uses / (points + 1)
    // is highest).
    let after = sheet(&app, master);
    assert_eq!(after.unspent, 0, "nothing left banked");
    assert_eq!(
        after.skills.get("Woodcutting"),
        before.skills.get("Woodcutting")
    );
    assert!(skill(&app, master, "Carpentry") > carpentry);
    let spent: i32 = of(&app, master)
        .into_iter()
        .filter_map(|k| match k {
            JournalKind::SkillImproved {
                skill, points, via, ..
            } if via == "spend" => {
                assert_eq!(skill, "Carpentry");
                Some(points)
            }
            _ => None,
        })
        .sum();
    assert!(spent >= 5, "{spent}");
    assert!(after.skills["Carpentry"] >= before.skills["Carpentry"] + spent);
}

#[test]
fn the_player_policy_banks_points_for_spend_points() {
    let mut app = app("smithy");
    let joiner = id(&app, "joiner");
    assert_eq!(get::<SpendPolicy>(&app, joiner), Some(SpendPolicy::Player));
    let before = sheet(&app, joiner);
    plane_until_level_up(&mut app, joiner, 8);
    let banked = sheet(&app, joiner);
    assert_eq!(banked.unspent, before.unspent + 5, "banked, not spent");
    assert!(
        !of(&app, joiner)
            .into_iter()
            .any(|k| matches!(k, JournalKind::SkillImproved { via, .. } if via == "spend")),
        "the player policy never spends"
    );
    // Too dear: nothing changes.
    let dear = SpendPoints {
        unit: joiner.0,
        purchase: Purchase::Skill {
            skill: "Carpentry".into(),
            points: banked.unspent + 1,
        },
    };
    assert!(content(&mut app, &dear).is_err());
    let buy = SpendPoints {
        unit: joiner.0,
        purchase: Purchase::Skill {
            skill: "Carpentry".into(),
            points: 4,
        },
    };
    let level = skill(&app, joiner, "Carpentry");
    content(&mut app, &buy).unwrap();
    run(&mut app, 1);
    let after = sheet(&app, joiner);
    assert_eq!(after.unspent, banked.unspent - 4);
    assert_eq!(after.skills["Carpentry"], banked.skills["Carpentry"] + 4);
    assert!(skill(&app, joiner, "Carpentry") > level);
}

#[test]
fn studying_the_book_raises_carpentry_up_to_its_limit() {
    let mut app = app("smithy");
    let (apprentice, joiner, book) = (id(&app, "apprentice"), id(&app, "joiner"), id(&app, "book"));
    let t = get::<npc_framework::glue::Teaches>(&app, book).expect("the book teaches");
    assert_eq!((t.skill.as_str(), t.max_level), ("Carpentry", 12));
    let scroll = get::<npc_framework::glue::Teaches>(&app, id(&app, "scroll")).unwrap();
    assert_eq!(scroll.skill, "Smithing");
    let before = skill(&app, apprentice, "Carpentry");
    content(&mut app, &Interact::new(apprentice, "study", book)).unwrap();
    run(&mut app, 205);
    let after = skill(&app, apprentice, "Carpentry");
    assert!(after > before, "{before} -> {after}");
    assert!(after <= 12, "capped by the book");
    assert!(
        of(&app, apprentice).into_iter().any(|k| matches!(
            k,
            JournalKind::SkillImproved { skill, via, points: 1, .. }
                if skill == "Carpentry" && via == "study"
        )),
        "one point per session"
    );
    // Gated: the joiner already knows more than the book teaches.
    let e = content(&mut app, &Interact::new(joiner, "study", book)).unwrap_err();
    let level = skill(&app, joiner, "Carpentry");
    assert_eq!(
        reason(e),
        format!("already Carpentry {level} (it teaches up to 12)")
    );
}

#[test]
fn the_smith_teaches_smithing_to_a_weaker_student_only() {
    let mut app = app("smithy");
    let (smith, apprentice) = (id(&app, "smith"), id(&app, "apprentice"));
    let teacher = get::<npc_framework::glue::Teacher>(&app, smith).expect("the smith teaches");
    assert_eq!(teacher.skills, ["Smithing"]);
    assert!(get::<npc_framework::glue::Teacher>(&app, apprentice).is_none());
    // Too far apart for a lesson: the apprentice walks over first.
    let e = content(&mut app, &Interact::new(smith, "teach", apprentice)).unwrap_err();
    assert!(reason(e).starts_with("too far"));
    let at = pos(&app, smith);
    issue(&mut app, 1, &[apprentice], mv(at.x - 1000, at.z), false);
    run_orders(&mut app, apprentice, 1500);
    let before = skill(&app, apprentice, "Smithing");
    content(&mut app, &Interact::new(smith, "teach", apprentice)).unwrap();
    run(&mut app, 105);
    let after = skill(&app, apprentice, "Smithing");
    assert!(
        after > before && after < skill(&app, smith, "Smithing"),
        "{before} -> {after}"
    );
    assert!(of(&app, apprentice).into_iter().any(|k| matches!(
        k,
        JournalKind::SkillImproved { skill, via, points: 1, .. }
            if skill == "Smithing" && via == "teaching"
    )));

    // Two equal smiths: neither can teach the other.
    let mut peers = inline(json!({"version": 1, "content": "testbed",
    "world": {"furnish": false, "detection": "automatic"},
    "actors": [
        {"kind": "unit", "name": "master", "template": "smith", "pos": [500, 500]},
        {"kind": "unit", "name": "peer", "template": "smith", "pos": [1500, 500]}
    ]}));
    let (master, peer) = (id(&peers, "master"), id(&peers, "peer"));
    let level = skill(&peers, master, "Smithing");
    let e = content(&mut peers, &Interact::new(master, "teach", peer)).unwrap_err();
    assert_eq!(
        reason(e),
        format!("teacher level {level} must be at least the student's {level} + 1")
    );
}

// ---------------------------------------------------------------- agent io

fn affordance(obs: &Value, id: &str) -> Option<Value> {
    obs["affordances"]
        .as_array()?
        .iter()
        .find(|a| a["id"] == json!(id))
        .cloned()
}

fn why_not(obs: &Value, id: &str) -> String {
    let a = affordance(obs, id).unwrap_or_else(|| panic!("no `{id}` offered"));
    assert_eq!(a["ok"], json!(false), "{a}");
    a["why_not"].as_str().unwrap().to_owned()
}

fn ok(obs: &Value, id: &str) -> bool {
    affordance(obs, id).is_some_and(|a| a["ok"] == json!(true))
}

fn self_pos(obs: &Value) -> (i64, i64) {
    let p = &obs["sections"]["self"]["pos"];
    (p[0].as_i64().unwrap(), p[1].as_i64().unwrap())
}

fn holds(obs: &Value, item: SimId) -> bool {
    obs["sections"]["inventory"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|i| i["id"] == json!(item.0))
}

fn busy(obs: &Value) -> bool {
    !obs["queue"]["current"].is_null()
        || obs["queue"]["pending"]
            .as_array()
            .is_some_and(|p| !p.is_empty())
        || !obs["sections"]["inventory"]["interacting"].is_null()
        || !obs["sections"]["command"]["current"].is_null()
}

/// One step of the scripted fetch-and-mine controller.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Goal {
    Key,
    Unlock,
    Open,
    Pickaxe,
    Equip,
    Mine,
    /// Pick up the mined ore (it lands on the ground).
    TakeOre(u32),
    Done,
}

/// A scripted controller (like `npc_agent_io::greedy`): it reads only its
/// observation and acts only through `agent/act`. `waypoints` are where it
/// walks for each goal (map knowledge a human player has too).
struct FetchAgent {
    unit: SimId,
    key: SimId,
    door: SimId,
    pickaxe: SimId,
    ore: SimId,
    goal: Goal,
    /// `why_not`s read on the way, in order.
    seen: Vec<String>,
    acts: Vec<String>,
}

impl FetchAgent {
    fn waypoint(&self) -> (i32, i32) {
        match self.goal {
            Goal::Key => (7500, -1500),
            Goal::Unlock | Goal::Open => (6500, 2500),
            Goal::Pickaxe | Goal::Equip => (4500, 6500),
            // Close to the rock (reach 1.5 m) and facing it on arrival: the rock
            // must be in view to be offered.
            Goal::Mine | Goal::TakeOre(_) | Goal::Done => (8800, -6000),
        }
    }

    fn affordance_id(&self) -> String {
        match self.goal {
            Goal::Key => format!("pick_up@{}", self.key.0),
            Goal::Unlock => format!("unlock@{}", self.door.0),
            Goal::Open => format!("open@{}", self.door.0),
            Goal::Pickaxe => format!("pick_up@{}", self.pickaxe.0),
            Goal::Equip => format!("equip@{}", self.pickaxe.0),
            Goal::Mine | Goal::Done => format!("use_tool_on@{}", self.ore.0),
            Goal::TakeOre(id) => format!("pick_up@{id}"),
        }
    }

    fn note(&mut self, obs: &Value, id: &str) {
        if let Some(a) = affordance(obs, id)
            && let Some(w) = a["why_not"].as_str()
            && self.seen.last().is_none_or(|l| l != w)
        {
            self.seen.push(w.to_owned());
        }
    }

    /// Advances the goal from the observation; returns the `agent/act`
    /// params to send, if any.
    fn decide(&mut self, obs: &Value) -> Option<Value> {
        let door_open = affordance(obs, &format!("close@{}", self.door.0))
            .is_some_and(|a| a["ok"] == json!(true));
        let ore_carried = obs["sections"]["inventory"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["kind"] == json!("ore"));
        self.goal = match self.goal {
            Goal::Key if holds(obs, self.key) => Goal::Unlock,
            Goal::Unlock
                if why_not_is(obs, &format!("lock@{}", self.door.0), "door is open")
                    || ok(obs, &format!("lock@{}", self.door.0)) =>
            {
                Goal::Open
            }
            Goal::Open if door_open => Goal::Pickaxe,
            Goal::Pickaxe if holds(obs, self.pickaxe) => Goal::Equip,
            Goal::Equip
                if obs["sections"]["inventory"]["equipment"]["main_hand"]
                    == json!(self.pickaxe.0) =>
            {
                Goal::Mine
            }
            Goal::Mine => mined_ore(obs).map_or(Goal::Mine, Goal::TakeOre),
            Goal::TakeOre(_) if ore_carried => Goal::Done,
            g => g,
        };
        if self.goal == Goal::Done || busy(obs) {
            return None;
        }
        let aff = self.affordance_id();
        self.note(obs, &aff);
        let base = json!({"unit": self.unit.0, "controller": "fetch"});
        let with = |extra: Value| {
            let mut p = base.clone();
            for (k, v) in extra.as_object().unwrap() {
                p[k] = v.clone();
            }
            p
        };
        if ok(obs, &aff) {
            self.acts.push(aff.clone());
            return Some(with(json!({"affordance": aff})));
        }
        let (x, z) = self.waypoint();
        if self_pos(obs) != (i64::from(x), i64::from(z)) {
            if ok(obs, "execute_orders") {
                self.acts.push("execute_orders".into());
                return Some(with(json!({"affordance": "execute_orders"})));
            }
            self.acts.push(format!("order Move {x},{z}"));
            return Some(with(json!({"verb": "order",
                "args": {"order": {"type": "Move", "pos": [x, z]}}})));
        }
        panic!(
            "stuck at {:?} on {:?}: {aff} {:?}",
            self_pos(obs),
            self.goal,
            affordance(obs, &aff)
        );
    }
}

/// The target of a `pick_up` affordance for ore on the ground.
fn mined_ore(obs: &Value) -> Option<u32> {
    obs["affordances"].as_array()?.iter().find_map(|a| {
        let label = a["label"].as_str()?;
        if a["verb"] == json!("pick_up") && label.contains("ore #") {
            a["target"].as_u64().and_then(|t| u32::try_from(t).ok())
        } else {
            None
        }
    })
}

fn why_not_is(obs: &Value, id: &str, text: &str) -> bool {
    affordance(obs, id).is_some_and(|a| a["why_not"] == json!(text))
}

/// Runs the fetch agent on `app` (smithy, the smith under `Direct`
/// control): it observes and maybe acts, then "thinks" for `think_ticks`
/// ticks while the world runs (`on_tick` after each tick). Returns what it
/// did and the tick it was done at.
fn run_fetch_agent(
    app: &mut App,
    think_ticks: u64,
    limit: u64,
    on_tick: &mut dyn FnMut(&mut App),
) -> (FetchAgentReport, u64) {
    let smith = id(app, "smith");
    let mut agent = FetchAgent {
        unit: smith,
        key: id(app, "store_key"),
        door: id(app, "store_door"),
        pickaxe: id(app, "pickaxe"),
        ore: id(app, "ore_rock"),
        goal: Goal::Key,
        seen: Vec::new(),
        acts: Vec::new(),
    };
    let r = call(
        app,
        "agent/attach",
        json!({"unit": smith.0, "controller": "fetch", "mode": "Direct"}),
    );
    assert_eq!(r["controller"], json!("fetch"), "{r}");
    run(app, 1);
    on_tick(app);
    while agent.goal != Goal::Done {
        assert!(
            tick(app) < limit,
            "not done by tick {}: {:?}",
            tick(app),
            agent.goal
        );
        let obs = call(
            app,
            "agent/observe",
            json!({"unit": smith.0, "controller": "fetch"}),
        );
        if let Some(params) = agent.decide(&obs) {
            let r = call(app, "agent/act", params.clone());
            assert_eq!(r["accepted"], json!(true), "{params} -> {r}");
        }
        for _ in 0..think_ticks {
            run(app, 1);
            on_tick(app);
        }
    }
    (
        FetchAgentReport {
            seen: agent.seen,
            acts: agent.acts,
        },
        tick(app),
    )
}

struct FetchAgentReport {
    seen: Vec<String>,
    acts: Vec<String>,
}

#[test]
fn a_direct_agent_fetches_the_key_enters_the_store_and_mines_ore_by_observe_and_act() {
    let mut app = app("smithy");
    let (smith, door, ore) = (
        id(&app, "smith"),
        id(&app, "store_door"),
        id(&app, "ore_rock"),
    );
    // Before anything: the door (in sight) says why not; the ore rock, out of
    // sight, is not offered at all (affordances are perception-gated).
    run(&mut app, 8);
    let obs = call(&mut app, "agent/observe", json!({"unit": smith.0}));
    assert_eq!(
        why_not(&obs, &format!("unlock@{}", door.0)),
        "door is locked: needs key:store"
    );
    assert_eq!(why_not(&obs, &format!("open@{}", door.0)), "door is locked");
    assert!(
        affordance(&obs, &format!("use_tool_on@{}", ore.0)).is_none(),
        "the ore rock is not seen yet: {}",
        obs["affordances"]
    );
    let (report, done_at) = run_fetch_agent(&mut app, 5, 4000, &mut |_| {});
    println!(
        "done at tick {done_at}: {:?}\nsaw {:?}",
        report.acts, report.seen
    );
    assert!(
        report.seen.iter().any(|w| w.starts_with("too far")),
        "{:?}",
        report.seen
    );
    assert!(count(&app, smith, "ore") >= 1);
    assert!(get::<Door>(&app, door).unwrap().open);
    // Only the agent acted: every interaction came from its affordances.
    let mut verbs: Vec<&str> = report
        .acts
        .iter()
        .filter_map(|a| a.split_once('@').map(|v| v.0))
        .collect();
    verbs.dedup();
    assert_eq!(
        verbs,
        [
            "pick_up",
            "unlock",
            "open",
            "pick_up",
            "equip",
            "use_tool_on",
            "pick_up"
        ],
        "{:?}",
        report.acts
    );
    assert!(
        report.acts.iter().any(|a| a == "execute_orders"),
        "it walks by its own orders"
    );
    let ext = journal(&app)
        .iter()
        .filter(|e| e.unit == Some(smith))
        .any(|e| e.kind.name() == "ExternalAction");
    assert!(ext, "agent actions are journaled");
}

/// The fetch agent's session on the smithy, loaded like `npcctl serve` does
/// (the `npc_agent_io` loader, seed 1), recorded with checkpoints every 100
/// ticks up to the first multiple of 100 after the agent is done.
fn record_fetch_session() -> SessionRecord {
    let mut loaders = agent_io::ScenarioLoaders::default();
    testbed_content::register_scenario_loader(&mut loaders);
    let doc = testbed_content::scenario::scenario_doc("smithy").unwrap();
    let mut app = finish(
        agent_io::scenario::load(&loaders, &doc, Some(1))
            .unwrap()
            .app,
    );
    let mut recorder = Recorder::aligned(100, u64::MAX);
    let (_, done_at) = run_fetch_agent(&mut app, 5, 4000, &mut |app| {
        recorder.after_tick(app.world_mut());
    });
    let end = (done_at / 100 + 1) * 100;
    while tick(&app) < end {
        run(&mut app, 1);
        recorder.after_tick(app.world_mut());
    }
    let replay = recorder.finish(app.world_mut()).unwrap();
    SessionRecord {
        version: agent_io::scenario::SESSION_VERSION,
        scenario: doc,
        seed: 1,
        replay,
    }
}

#[test]
fn a_recorded_agent_session_replays_without_the_agent() {
    let record = record_fetch_session();
    assert!(
        record
            .replay
            .commands
            .iter()
            .any(|c| c.2.kind() == "AgentAct"),
        "the agent's actions are recorded commands"
    );
    let mut loaders = agent_io::ScenarioLoaders::default();
    testbed_content::register_scenario_loader(&mut loaders);
    let report = agent_io::scenario::verify_session(&loaders, &record);
    assert!(
        report.ok && report.checked == record.replay.checkpoints.len(),
        "{report:?}"
    );
    // Through JSON, as the corpus stores it.
    let back: SessionRecord =
        serde_json::from_str(&serde_json::to_string(&record).unwrap()).unwrap();
    assert!(agent_io::scenario::verify_session(&loaders, &back).ok);
    // Without one of its actions the session does not replay.
    let mut bad = record.clone();
    let i = bad
        .replay
        .commands
        .iter()
        .position(|c| c.2.kind() == "AgentAct")
        .unwrap();
    bad.replay.commands.remove(i);
    assert!(!agent_io::scenario::verify_session(&loaders, &bad).ok);
}

/// Writes the session record of [`record_fetch_session`] to
/// `$TESTBED_SESSION_OUT` (how `tests/replays/testbed_agent_session.json`
/// was made; never needed to pass the corpus).
#[test]
#[ignore = "writes a corpus file"]
fn write_agent_session_record() {
    let path = std::env::var("TESTBED_SESSION_OUT").expect("set TESTBED_SESSION_OUT");
    let record = record_fetch_session();
    let text = serde_json::to_string_pretty(&record).unwrap();
    std::fs::write(
        &path,
        text + "
",
    )
    .unwrap();
}

#[test]
fn an_advisor_orders_a_squad_in_formation() {
    let mut app = app("formation");
    let (a, b, c, d) = (id(&app, "a"), id(&app, "b"), id(&app, "c"), id(&app, "d"));
    content(
        &mut app,
        &CreateSquad {
            player: P1,
            members: vec![b, c, d],
            formation: Some(Formation {
                kind: FormationKind::Wedge,
                spacing_mm: 1500,
            }),
        },
    )
    .unwrap();
    run(&mut app, 1);
    let squad = {
        let world = app.world_mut();
        let mut q = world.query_filtered::<&SimId, With<Squad>>();
        let mut ids: Vec<SimId> = q.iter(world).copied().collect();
        ids.sort_unstable();
        assert_eq!(ids.len(), 1);
        ids[0]
    };
    assert_eq!(get::<Squad>(&app, squad).unwrap().members, [b, c, d]);
    call(
        &mut app,
        "agent/attach",
        json!({"unit": a.0, "controller": "cmdr", "mode": "Advisor"}),
    );
    run(&mut app, 1);
    let target = SimPos::new(2500, -7500);
    let r = call(
        &mut app,
        "agent/act",
        json!({"unit": a.0, "controller": "cmdr", "verb": "order", "args": {
            "order": {"type": "Move", "pos": [target.x, target.z]},
            "units": [squad.0],
        }}),
    );
    assert_eq!(r["accepted"], json!(true), "{r}");
    run(&mut app, 400);
    let at: Vec<SimPos> = [b, c, d].iter().map(|&u| pos(&app, u)).collect();
    for (i, p) in at.iter().enumerate() {
        assert!(p.dist2(target) <= 3000 * 3000, "near the target: {at:?}");
        for q in &at[i + 1..] {
            assert!(p.dist2(*q) >= 1000 * 1000, "own slots: {at:?}");
        }
    }
    assert_eq!(pos(&app, a), SimPos::new(-6500, -4500), "the advisor stays");
    for u in [b, c, d] {
        assert_eq!(orders_done(&app, u), ["Move"]);
    }
    // An enemy is not the advisor's to order.
    let enemy = id(&app, "enemy");
    let r = call(
        &mut app,
        "agent/act",
        json!({"unit": a.0, "controller": "cmdr", "verb": "order", "args": {
            "order": {"type": "Stop"}, "units": [enemy.0],
        }}),
    );
    assert_eq!(r["accepted"], json!(false), "{r}");
    assert!(
        r["message"].as_str().unwrap().contains("owned by player 2"),
        "{r}"
    );
}

#[test]
fn orders_across_factions_are_rejected() {
    let mut app = app("testbed");
    let (smith, skel, cart, wagon, key) = (
        id(&app, "smith"),
        id(&app, "skel_warrior_a"),
        id(&app, "cart"),
        id(&app, "wagon"),
        id(&app, "store_key"),
    );
    let refuse = |app: &mut App, player: u32, units: &[SimId], o: UnitOrder| -> String {
        let cmd = IssueOrder::new(PlayerId(player), units.to_vec(), o);
        submit_order(app.world_mut(), &cmd)
            .map(|_| ())
            .unwrap_err()
            .to_string()
    };
    let e = refuse(&mut app, 2, &[smith], mv(0, 0));
    assert!(e.contains("owned by player 1"), "{e}");
    let e = refuse(&mut app, 1, &[skel], mv(0, 0));
    assert!(e.contains("owned by player 2"), "{e}");
    for v in [cart, wagon] {
        let e = refuse(&mut app, 2, &[v], mv(0, 0));
        assert!(e.contains("owned by player 1"), "vehicle: {e}");
    }
    let e = refuse(&mut app, 2, &[smith], interact_order("pick_up", key));
    assert!(e.contains("owned by player 1"), "{e}");
    // A group order with one foreign unit is refused as a whole.
    let e = refuse(&mut app, 1, &[smith, skel], mv(0, 0));
    assert!(e.contains("owned by player 2"), "{e}");
    assert!(idle(&app, smith));
    // An agent on a skeleton sees and orders only player 2's units.
    call(
        &mut app,
        "agent/attach",
        json!({"unit": skel.0, "controller": "s", "mode": "Advisor"}),
    );
    run(&mut app, 1);
    let obs = call(&mut app, "agent/observe", json!({"unit": skel.0}));
    let owned: Vec<u64> = obs["sections"]["command"]["owned_units"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_u64().unwrap())
        .collect();
    assert_eq!(obs["sections"]["command"]["player"], json!(2));
    assert_eq!(owned.len(), 4, "{owned:?}");
    assert!(!owned.contains(&u64::from(smith.0)));
    let r = call(
        &mut app,
        "agent/act",
        json!({"unit": skel.0, "controller": "s", "verb": "order", "args": {
            "order": {"type": "Move", "pos": [0, 0]}, "units": [smith.0],
        }}),
    );
    assert_eq!(r["accepted"], json!(false), "{r}");
    assert!(
        r["message"].as_str().unwrap().contains("owned by player 1"),
        "{r}"
    );
    let _ = P1;
}
