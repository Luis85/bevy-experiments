//! Several civic projects at once (economy review #7): a settlement keeps
//! as many going as it can staff and supply, builders and haulers serve the
//! sites that can progress first, and the overseer's plots never wait behind
//! a held-up civic site.
use bevy_app::App;
use bevy_ecs::prelude::*;
use island_content::IslandIds;
use island_content::village::{
    CoinPurse, VillageState, Wallet, civic,
    frontier::{
        labor::{self, ResidentProfile},
        oversee::{PriorityLevel, PriorityTopic, SetVillagePriority},
    },
};
use npc_framework::prelude::*;
use npc_framework::snapshot;
use serde_json::{Value, json};

const SEED: u64 = 7;

/// The shipped world with its `living_world` block edited as JSON; no
/// founding fields, boat, births or arrivals (as in `island_buildings.rs`).
fn world_with(edit: impl FnOnce(&mut Value)) -> App {
    let mut doc = island_content::scenario::scenario_doc("living_world").expect("scenario");
    let world = &mut doc.world["construction"]["world"]["preset"]["living_world"];
    edit(world);
    world["economy"]["merchant"]["enabled"] = json!(false);
    world["economy"]["demography"]["enabled"] = json!(false);
    world["economy"]["tuning"]["planner_crop_shortage_units"] = json!(100_000);
    if let Some(buildings) = world["economy"]["buildings"].as_array_mut() {
        for def in buildings {
            if def.get("field").is_some() {
                def["field"]["founding"] = json!(false);
            }
        }
    }
    island_content::scenario::load_scenario(&doc, SEED)
        .map(|loaded| loaded.app)
        .unwrap_or_else(|error| panic!("{error}"))
}

/// Small buildings placed anywhere: a free pad and a woodpile of four logs.
fn test_buildings(world: &mut Value) {
    let anywhere = json!({"surfaces": ["grass", "soil", "stone"], "max_slope_mm": 3000,
        "clearance": 0});
    let list = world["economy"]["buildings"]
        .as_array_mut()
        .expect("buildings");
    list.push(json!({"id": "pad", "display": "pad", "work_ticks": 20,
        "footprint": {"width": 1, "depth": 1}, "kind": {"type": "storage", "capacity": 40},
        "solid": false, "civic": false, "placement": anywhere}));
    list.push(json!({"id": "woodpile", "display": "Woodpile",
        "cost": [{"good": "log", "count": 4}], "work_ticks": 20,
        "footprint": {"width": 2, "depth": 2}, "kind": {"type": "storage", "capacity": 40},
        "solid": false, "civic": false}));
}

fn settlement(world: &World, key: &str) -> SimId {
    world
        .resource::<IslandIds>()
        .id(&format!("settlement_{key}"))
}

fn entity(world: &World, id: SimId) -> Entity {
    world.resource::<SimIndex>().get(id).expect("entity")
}

fn money(world: &World) -> u64 {
    world
        .resource::<SimIndex>()
        .map
        .values()
        .map(|entity| {
            u64::from(world.get::<Wallet>(*entity).map_or(0, |w| w.coins))
                + u64::from(world.get::<CoinPurse>(*entity).map_or(0, |p| p.coins))
        })
        .sum::<u64>()
        + u64::from(world.resource::<VillageState>().fund)
}

/// Moves residents' coins into the fund: the reserve plus `coins`.
fn fund(world: &mut World, coins: u32) {
    let reserve = island_content::village::frontier::economy_spec::tuning(world)
        .expect("tuning")
        .civic_reserve;
    let mut left = coins + reserve;
    let people: Vec<Entity> = world.resource::<SimIndex>().map.values().copied().collect();
    for actor in people {
        if left == 0 {
            break;
        }
        if world.get::<ResidentProfile>(actor).is_none() {
            continue;
        }
        let part = world
            .get::<Wallet>(actor)
            .map_or(0, |wallet| wallet.coins.saturating_sub(100))
            .min(left)
            .min(20);
        world.get_mut::<Wallet>(actor).expect("wallet").coins -= part;
        world.resource_mut::<VillageState>().fund += part;
        left -= part;
    }
    assert_eq!(left, 0, "residents could not fund {coins} coins");
}

/// The overseer's recorded placement at the nearest free spot.
fn place(app: &mut App, building: &str, settlement: SimId) -> SimId {
    let def = civic::definition(app.world(), building)
        .expect("definition")
        .clone();
    let (x, z, rotation) =
        civic::find_site(app.world(), &def, settlement, 36).expect("a buildable site");
    civic::PlaceBuilding {
        building: building.into(),
        x,
        z,
        rotation,
    }
    .perform(app.world_mut())
    .expect("placed")
}

/// Turns a placed site into a planner's site of `building` with `cost`
/// (setup only: what the planner itself opens is tested live below).
fn as_civic(app: &mut App, site: SimId, building: &str, cost: &[(&str, u32)]) {
    let target = entity(app.world(), site);
    let mut state = app
        .world_mut()
        .get_mut::<civic::ConstructionSite>(target)
        .expect("site");
    state.civic = true;
    state.building = building.into();
    state.cost = cost.iter().map(|(kind, n)| ((*kind).into(), *n)).collect();
}

fn order(app: &App, settlement: SimId) -> Vec<SimId> {
    civic::open_sites(app.world(), settlement)
        .into_iter()
        .map(|(id, _)| id)
        .collect()
}

/// Moves the clock on by `ticks` without running the world: a site placed
/// before has then gone that long without a delivery.
fn age(app: &mut App, ticks: u64) {
    app.world_mut().resource_mut::<SimClock>().tick += ticks;
}

/// Harbour with two residents nobody can house (one bed a cottage) and
/// unstored surplus: the planner wants a home and a store at once.
fn two_needs() -> App {
    let mut app = world_with(|world| {
        for settlement in world["settlements"].as_array_mut().expect("settlements") {
            for home in settlement["homes"].as_array_mut().expect("homes") {
                home["capacity"] = json!(1);
            }
        }
        let tuning = &mut world["economy"]["tuning"];
        tuning["planner_review_ticks"] = json!(200);
        tuning["planner_sustain_reviews"] = json!(2);
        tuning["planner_storage_overflow_units"] = json!(1);
        tuning["planner_demand_units"] = json!(100_000);
        tuning["founding_day"] = json!(0);
    });
    let harbour = settlement(app.world(), "harbour");
    fund(app.world_mut(), 500);
    for name in ["Wanderer", "Drifter"] {
        labor::settle_newcomer(app.world_mut(), harbour, name).expect("newcomer");
    }
    app
}

/// The harbour's open civic sites: `(site, building, delivered units, work)`.
fn civic_sites(app: &App, harbour: SimId) -> Vec<(SimId, String, u32, u32)> {
    civic::sites(app.world())
        .into_iter()
        .filter(|site| site.settlement == harbour && site.civic)
        .map(|site| {
            (
                site.id,
                site.building.clone(),
                site.delivered.values().sum(),
                site.work_done,
            )
        })
        .collect()
}

/// Runs in 200-tick steps until two civic projects of different needs are
/// open in the harbour; returns them.
fn until_two_open(app: &mut App, limit: u64, coins: u64) -> Vec<(SimId, String, u32, u32)> {
    let harbour = settlement(app.world(), "harbour");
    let end = app.world().resource::<SimClock>().tick + limit;
    while app.world().resource::<SimClock>().tick < end {
        run_ticks(app.world_mut(), 200);
        assert_eq!(money(app.world()), coins, "coins are conserved");
        let open = civic_sites(app, harbour);
        if open.len() >= 2 {
            return open;
        }
    }
    panic!(
        "no second civic project opened; planner: {:?}; sites: {:?}",
        civic::planner(app.world(), harbour),
        civic::sites(app.world())
    );
}

#[test]
fn two_civic_projects_open_and_progress_together_in_one_settlement() {
    let mut app = two_needs();
    let harbour = settlement(app.world(), "harbour");
    let coins = money(app.world());
    let open = until_two_open(&mut app, 12_000, coins);
    let buildings: Vec<&str> = open.iter().map(|(_, b, _, _)| b.as_str()).collect();
    assert!(
        buildings.contains(&"cottage") && buildings.contains(&"stockpile"),
        "a home and a store at once: {open:?}"
    );
    let planner = civic::planner(app.world(), harbour).expect("planner");
    assert_eq!(planner.projects.len(), 2, "{planner:?}");
    let pair: Vec<SimId> = open.iter().map(|(id, ..)| *id).collect();
    // Both progress while both are open: materials arrive at each.
    let end = app.world().resource::<SimClock>().tick + 12_000;
    let mut progressed = false;
    while app.world().resource::<SimClock>().tick < end {
        run_ticks(app.world_mut(), 200);
        assert_eq!(money(app.world()), coins, "coins are conserved");
        let now = civic_sites(&app, harbour);
        let both: Vec<_> = now.iter().filter(|(id, ..)| pair.contains(id)).collect();
        if both.len() < 2 {
            break;
        }
        if both
            .iter()
            .all(|(_, _, delivered, work)| *delivered > 0 || *work > 0)
        {
            progressed = true;
            break;
        }
    }
    assert!(
        progressed,
        "both projects received materials while both were open: {:?}",
        civic::sites(app.world())
    );
    village_integrity(&app);
}

fn village_integrity(app: &App) {
    island_content::village::integrity::validate_colony(app.world()).expect("colony integrity");
}

#[test]
fn several_open_projects_survive_save_and_continue_bit_for_bit() {
    let mut app = two_needs();
    let coins = money(app.world());
    let _ = until_two_open(&mut app, 12_000, coins);
    let saved = snapshot::save(app.world_mut()).expect("saved");
    let mut resumed = two_needs();
    snapshot::load(resumed.world_mut(), &saved, snapshot::LoadMode::Strict).expect("loaded");
    assert_eq!(state_hash(app.world_mut()), state_hash(resumed.world_mut()));
    let harbour = settlement(app.world(), "harbour");
    assert_eq!(
        civic::planner(resumed.world(), harbour).map(|p| p.projects.clone()),
        civic::planner(app.world(), harbour).map(|p| p.projects.clone())
    );
    run_ticks(app.world_mut(), 1_200);
    run_ticks(resumed.world_mut(), 1_200);
    assert_eq!(
        state_hash(app.world_mut()),
        state_hash(resumed.world_mut()),
        "a loaded world with several projects continues bit for bit"
    );
}

/// The order builders and haulers serve open sites: progressing before
/// held up; a home, then the overseer's plot, then the planner's projects
/// by the overseer's level and the planner's order; then age and identity.
#[test]
fn the_order_of_open_sites_is_explicit_and_deterministic() {
    let scene = || {
        let mut app = world_with(|world| {
            test_buildings(world);
            world["economy"]["tuning"]["max_open_sites"] = json!(6);
        });
        let harbour = settlement(app.world(), "harbour");
        let stall = civic::stall_ticks(app.world());
        // Placed first, then a day without a delivery: held up.
        let held = place(&mut app, "pad", harbour);
        as_civic(&mut app, held, "stockpile", &[("log", 4)]);
        age(&mut app, stall);
        let workplace = place(&mut app, "pad", harbour);
        as_civic(&mut app, workplace, "carpenter_workshop", &[("log", 4)]);
        let store = place(&mut app, "pad", harbour);
        as_civic(&mut app, store, "stockpile", &[("log", 4)]);
        let plot = place(&mut app, "woodpile", harbour);
        let home = place(&mut app, "pad", harbour);
        {
            let target = entity(app.world(), home);
            let mut site = app
                .world_mut()
                .get_mut::<civic::ConstructionSite>(target)
                .expect("site");
            site.building = "cottage".into();
            site.cost = [("log".to_owned(), 4)].into_iter().collect();
        }
        (app, harbour, [held, workplace, store, plot, home])
    };
    let (mut app, harbour, [held, workplace, store, plot, home]) = scene();
    assert_eq!(
        order(&app, harbour),
        vec![home, plot, store, workplace, held],
        "a home, the overseer's plot, the planner's store before its workplace, the held-up site last"
    );
    // The overseer raises the carpenters' workplaces: that project moves up.
    SetVillagePriority {
        settlement: harbour,
        topic: PriorityTopic::Workplace,
        profession: Some(island_content::village::Role::Carpenter),
        level: PriorityLevel::High,
    }
    .perform(app.world_mut())
    .expect("priority");
    assert_eq!(
        order(&app, harbour),
        vec![home, plot, workplace, store, held]
    );
    // A delivery ends the hold: the site takes its place by urgency again.
    {
        let target = entity(app.world(), held);
        let now = app.world().resource::<SimClock>().tick;
        let mut site = app
            .world_mut()
            .get_mut::<civic::ConstructionSite>(target)
            .expect("site");
        site.last_delivery_at = Some(now);
    }
    assert_eq!(
        order(&app, harbour),
        vec![home, plot, workplace, held, store],
        "two equal stores go by age"
    );
    // A day later every site without a delivery is held up. The overseer's
    // plot, supplied again, goes first; the held-up home stays ahead of the
    // planner's other projects.
    let stall = civic::stall_ticks(app.world());
    age(&mut app, stall);
    {
        let target = entity(app.world(), plot);
        let now = app.world().resource::<SimClock>().tick;
        app.world_mut()
            .get_mut::<civic::ConstructionSite>(target)
            .expect("site")
            .last_delivery_at = Some(now);
    }
    assert_eq!(
        order(&app, harbour),
        vec![plot, home, workplace, held, store]
    );
    // The same world gives the same order.
    let (twin, twin_harbour, twin_ids) = scene();
    assert_eq!(twin_ids, [held, workplace, store, plot, home]);
    assert_eq!(
        order(&twin, twin_harbour),
        vec![home, plot, store, workplace, held]
    );
}

/// A civic site that cannot get its materials holds nobody up: the
/// overseer's plot and another civic site placed after it are supplied and
/// built while it waits.
#[test]
fn a_held_up_civic_site_blocks_neither_another_site_nor_a_player_plot() {
    let mut app = world_with(|world| {
        test_buildings(world);
        world["economy"]["tuning"]["founding_day"] = json!(0);
    });
    let harbour = settlement(app.world(), "harbour");
    fund(app.world_mut(), 300);
    let coins = money(app.world());
    let stall = civic::stall_ticks(app.world());
    // A planner's site waiting for a good nobody has: held up after a day.
    let held = place(&mut app, "pad", harbour);
    as_civic(&mut app, held, "pad", &[("village_nothing", 1)]);
    age(&mut app, stall);
    let other = place(&mut app, "woodpile", harbour);
    as_civic(&mut app, other, "woodpile", &[("log", 4)]);
    let plot = place(&mut app, "woodpile", harbour);
    assert_eq!(
        order(&app, harbour),
        vec![plot, other, held],
        "the held-up site goes last"
    );
    let end = app.world().resource::<SimClock>().tick + 12_000;
    let open = |app: &App, id: SimId| civic::sites(app.world()).iter().any(|site| site.id == id);
    while app.world().resource::<SimClock>().tick < end && (open(&app, plot) || open(&app, other)) {
        run_ticks(app.world_mut(), 200);
        assert_eq!(money(app.world()), coins, "coins are conserved");
    }
    assert!(
        !open(&app, plot) && !open(&app, other),
        "the plot and the other civic site were built: {:?}",
        civic::sites(app.world())
    );
    assert!(open(&app, held), "the held-up site still waits");
    village_integrity(&app);
}
