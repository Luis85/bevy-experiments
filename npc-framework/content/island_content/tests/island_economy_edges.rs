//! The economy reacts at its edges (review fix S2): tools wear, the boat's
//! prices drift and its order follows the island, food spoils, the stall
//! takes what a player made, and new foods and fuels need only data.
mod life_support;

use island_content::village::{
    self, civic, economy as econ, frontier::economy_spec, ledger, merchant, production,
};
use life_support::*;
use npc_framework::interaction::Item;
use npc_framework::prelude::*;
use serde_json::{Value, json};

fn quick_boat(world: &mut Value, drift: Option<Value>) {
    only_weather(world, "clear");
    short_days(world, 1200);
    let boat = &mut economy(world)["merchant"];
    boat["first_visit_day"] = json!(0);
    boat["interval_days"] = json!(1);
    boat["stay_ticks"] = json!(600);
    if let Some(drift) = drift {
        boat["drift"] = drift;
    } else if let Some(map) = boat.as_object_mut() {
        map.remove("drift");
    }
}

fn factors(seed: u64, drift: Option<Value>) -> Vec<Vec<u32>> {
    let mut app = world_for(seed, |world| quick_boat(world, drift));
    let mut seen = Vec::new();
    for _ in 0..10 {
        run_ticks(app.world_mut(), 1200);
        seen.push(
            merchant::outlook(app.world())
                .into_iter()
                .map(|line| line.factor_permille)
                .collect(),
        );
    }
    seen
}

#[test]
fn boat_prices_drift_within_bounds_and_are_seeded() {
    let shipped = economy_spec::EconomySpec::default().merchant.drift;
    let drift = serde_json::to_value(&shipped).expect("drift");
    let a = factors(7, Some(drift.clone()));
    assert!(
        a.iter().flatten().all(
            |factor| (shipped.price_min_permille..=shipped.price_max_permille).contains(factor)
        ),
        "{a:?}"
    );
    assert!(
        a.iter().flatten().any(|factor| *factor != 1000),
        "prices moved: {a:?}"
    );
    assert_eq!(a, factors(7, Some(drift.clone())), "same seed, same drift");
    assert_ne!(a, factors(11, Some(drift)), "another seed, another drift");
    // No drift block: a fixed boat.
    let fixed = factors(7, None);
    assert!(
        fixed.iter().flatten().all(|factor| *factor == 1000),
        "{fixed:?}"
    );
}

#[test]
fn a_work_tool_without_durability_is_refused() {
    let mut spec = economy_spec::EconomySpec::default();
    for good in &mut spec.goods {
        if good.key == "hammer" {
            good.durability = None;
        }
    }
    let error = spec.validate().expect_err("refused");
    assert!(
        error.contains("`hammer` is a work tool but has no `durability`"),
        "{error}"
    );
}

#[test]
fn building_without_a_hammer_is_slower() {
    let mut app = world_with(|world| only_weather(world, "clear"));
    run_ticks(app.world_mut(), 2);
    let (_, actor) = all_residents(app.world())[0];
    econ::grant(app.world_mut(), actor, Item::new("hammer"));
    assert_eq!(civic::build_tool_percent(app.world(), actor), 100);
    while econ::count(app.world(), actor, "hammer") > 0 {
        econ::consume(app.world_mut(), actor, "hammer", 1).expect("hammer");
    }
    assert_eq!(civic::build_tool_percent(app.world(), actor), 150);
}

#[test]
fn spoilage_reaches_the_ledger() {
    let mut app = world_with(|world| {
        only_weather(world, "clear");
        short_days(world, 1200);
    });
    run_ticks(app.world_mut(), 1200 * 3 + 2);
    let ledger = app.world().resource::<ledger::SeasonLedger>();
    let spoiled: u64 = ledger
        .seasons
        .iter()
        .flat_map(|tally| tally.spoiled.values())
        .sum();
    assert!(spoiled > 0, "food spoiled in three days");
    assert!(
        ledger
            .seasons
            .iter()
            .flat_map(|tally| tally.spoiled.keys())
            .all(
                |good| ["bread", "village_cooked_meal", "village_berries"].contains(&good.as_str())
            ),
        "only food with a shelf life spoils"
    );
}

#[test]
fn a_new_food_good_in_data_is_eaten() {
    let mut app = world_with(|world| {
        only_weather(world, "clear");
        economy(world)["goods"]
            .as_array_mut()
            .expect("goods")
            .push(json!({
                "key": "smoked_fish", "display": "smoked fish", "base_price": 5,
                "food_value": 60000, "tags": ["food"],
                "item": {"weight_g": 400, "max_stack": 20}
            }));
    });
    run_ticks(app.world_mut(), 2);
    let (_, actor) = all_residents(app.world())[0];
    for food in production::diet(app.world(), actor) {
        let held = econ::count(app.world(), actor, food);
        if held > 0 {
            econ::consume(app.world_mut(), actor, food, held).expect("emptied");
        }
    }
    econ::grant(app.world_mut(), actor, Item::new("smoked_fish"));
    assert!(production::diet(app.world(), actor).contains(&"smoked_fish"));
    assert_eq!(
        production::food_choice(app.world(), actor),
        Some("smoked_fish")
    );
}

#[test]
fn a_second_fuel_in_data_is_a_fuel() {
    let app = world_with(|world| {
        only_weather(world, "clear");
        economy(world)["goods"]
            .as_array_mut()
            .expect("goods")
            .push(json!({
                "key": "peat", "display": "peat", "base_price": 2, "tags": ["fuel"],
                "item": {"weight_g": 1000, "max_stack": 20}
            }));
    });
    let roles = village::roles::roles(app.world());
    assert!(roles.fuels.contains(&"peat"), "{:?}", roles.fuels);
    assert!(roles.fuels.contains(&"log"));
}

#[test]
fn the_stall_takes_what_a_player_made() {
    let mut app = world_with(|world| only_weather(world, "clear"));
    run_ticks(app.world_mut(), 2);
    let harbour = settlement(app.world(), "harbour");
    let stall = civic::depots(app.world(), harbour)
        .into_iter()
        .find(|(_, entity)| {
            app.world()
                .get::<civic::Depot>(*entity)
                .is_some_and(|depot| depot.public)
        })
        .map(|(_, entity)| entity)
        .expect("a market stall");
    let (owner_id, owner) = all_residents(app.world())[0];
    let door = *app.world().get::<SimPos>(stall).expect("stall");
    teleport(app.world_mut(), owner, door);
    // Tools are welcome on a stall (not in a store).
    econ::grant(app.world_mut(), owner, Item::new("hammer"));
    civic::depot_check_deposit(app.world(), owner, stall, "hammer", 1).expect("a stall takes it");
    civic::depot_deposit(app.world_mut(), owner, stall, "hammer", 1).expect("laid down");
    let lots = village::view::consigned(app.world(), owner_id);
    assert!(
        lots.iter()
            .any(|lot| lot.good == "hammer" && lot.units == 1)
    );
    let error = civic::depot_check_deposit(app.world(), owner, stall, "no_such_good", 1)
        .expect_err("untraded");
    assert!(error.starts_with("Nobody on this island trades"), "{error}");
}

/// Every founder's tool carries the durability of its data, the endowment
/// stacks included (closing fix: a fisher's rod, a carpenter's rods or a
/// hunter's spare spear came without one and never wore out), and wears
/// out with use.
#[test]
fn founders_tools_carry_their_durability_and_wear_out() {
    use npc_framework::crafting::Durability;
    use npc_framework::interaction::Inventory;
    let mut app = world_with(|world| only_weather(world, "clear"));
    // The founders receive their goods on the first tick.
    run_ticks(app.world_mut(), 2);
    let mut tools = 0;
    let mut sample = None;
    for (_, actor) in all_residents(app.world()) {
        let items = app
            .world()
            .get::<Inventory>(actor)
            .map(|inventory| inventory.items.clone())
            .unwrap_or_default();
        for id in items {
            let Some(entity) = app.world().resource::<SimIndex>().get(id) else {
                continue;
            };
            let kind = app.world().get::<Item>(entity).expect("item").kind.clone();
            let Some(full) = village::catalog::durability_in(app.world(), &kind) else {
                continue;
            };
            let durability = app.world().get::<Durability>(entity);
            assert!(
                durability.is_some_and(|durability| (1..=full).contains(&durability.current)),
                "{kind} of a founder has no durability"
            );
            tools += 1;
            if kind == village::catalog::FISHING_ROD {
                sample = Some((actor, id, kind));
            }
        }
    }
    // Tools, rods and spears; the founders' coats are worn, not carried.
    assert!(tools >= 30, "only {tools} durable founder items");
    // A rod handed out in an endowment wears out after its uses.
    let (actor, id, kind) = sample.expect("somebody holds a rod");
    let full = village::catalog::durability_in(app.world(), &kind).expect("rod durability");
    let holder = app.world().get::<SimId>(actor).copied().expect("id");
    let rods = econ::count(app.world(), actor, &kind);
    for _ in 0..full * rods {
        village::living::wear_tool(app.world_mut(), actor, holder, &kind);
    }
    assert_eq!(econ::count(app.world(), actor, &kind), 0);
    assert!(app.world().resource::<SimIndex>().get(id).is_none());
}

/// The ledger closes a season on the calendar's own boundary (closing fix
/// C3): from founding day 5 with three-day seasons, the founding season
/// ends with day 5, at tick one day in, not three days in.
#[test]
fn the_ledger_turns_with_the_calendar() {
    let mut app = world_with(|world| {
        only_weather(world, "clear");
        short_days(world, 1200);
        economy(world)["tuning"]["days_per_season"] = json!(3);
        economy(world)["tuning"]["founding_day"] = json!(5);
    });
    run_ticks(app.world_mut(), 1_199);
    assert_eq!(
        app.world().resource::<ledger::SeasonLedger>().seasons.len(),
        1
    );
    run_ticks(app.world_mut(), 2);
    let seasons = &app.world().resource::<ledger::SeasonLedger>().seasons;
    assert_eq!(seasons.len(), 2, "the season turned on calendar day 6");
    let closed = &seasons[0];
    let end = closed.end.as_ref().expect("closed");
    // The next season opens with what the last one closed with.
    assert_eq!(seasons[1].start.fund, end.fund);
}
