//! A housing commission finances one joined material bundle before future comfort reserves.
use bevy_app::App;
use bevy_ecs::prelude::*;
use npc_framework::needs::{NeedRegistry, Needs};
use npc_framework::prelude::*;
use testbed_content::{
    TestbedIds,
    village::{self, catalog, colony, economy, market, production},
};

fn fixture() -> (App, SimId, Entity) {
    let mut app = testbed_content::new_sim_app(7, "island_colony");
    let builder = app.world().resource::<TestbedIds>().id("Jun");
    let actor = app.world().resource::<SimIndex>().get(builder).unwrap();
    let plot = village::homestead::plots(app.world())
        .into_iter()
        .find(|(_, entity)| {
            village::homestead::definition(app.world(), *entity)
                .unwrap()
                .use_kind
                == "housing"
        })
        .unwrap()
        .0;
    colony::ColonyAct {
        verb: "commission_home".into(),
        resident: Some(builder),
        house: Some(plot),
        value: 0,
    }
    .perform(app.world_mut())
    .unwrap();
    // Declared fixture reproduces the observed stalled pack and market quote.
    // Production refresh may allocate these goods/cash; it must not create any.
    for kind in ["log", "plank"] {
        let count = economy::count(app.world(), actor, kind);
        if count > 0 {
            economy::consume(app.world_mut(), actor, kind, count).unwrap();
        }
    }
    economy::grant(app.world_mut(), actor, Item::new("log"));
    economy::grant(app.world_mut(), actor, Item::new("plank").stack(6));
    app.world_mut()
        .get_mut::<village::Wallet>(actor)
        .unwrap()
        .coins = 136;
    app.world_mut()
        .resource_mut::<market::MarketState>()
        .commodities
        .insert(
            catalog::WARM_CLOTHES.into(),
            market::Commodity {
                reference_milli: 212_000,
                pending_units: 0,
                pending_turnover: 0,
                total_units: 0,
                total_turnover: 0,
                history: Vec::new(),
            },
        );
    for name in ["Hunger", "Thirst", "Energy", "Warmth"] {
        need(&mut app, actor, name, 100_000);
    }
    (app, builder, actor)
}
fn need(app: &mut App, actor: Entity, name: &str, value: i32) {
    let id = app.world().resource::<NeedRegistry>().id(name).unwrap();
    app.world_mut().get_mut::<Needs>(actor).unwrap().values[id.0 as usize].value = value;
}

#[test]
fn a_forecast_clothing_bid_cannot_strand_a_financeable_cottage_or_double_count_its_log() {
    let (mut app, _, actor) = fixture();
    let items = economy::free_items(app.world(), actor);
    let coins = app.world().get::<village::Wallet>(actor).unwrap().coins;
    production::refresh(app.world_mut());
    // Six planks need one saw batch. Its input log and the four retained house
    // logs are separate requirements, even though the actor owns only one log.
    assert!(production::desired_stock(app.world(), actor, "plank") >= 8);
    assert!(production::desired_stock(app.world(), actor, "log") >= 5);
    assert!(production::production_demand(app.world(), actor, "plank") >= 4);
    assert!(production::internal_demand(app.world(), actor, "plank") >= 2);
    assert!(
        production::purchase_target(app.world(), actor, "plank") <= 6,
        "owned/own-produced planks must not be charged again as finished purchases"
    );
    assert!(
        production::purchase_target(app.world(), actor, "log") > 1,
        "the remaining real timber must retain a funded procurement target"
    );
    assert_eq!(economy::free_items(app.world(), actor), items);
    assert_eq!(
        app.world().get::<village::Wallet>(actor).unwrap().coins,
        coins
    );
}

#[test]
fn urgent_food_water_and_cold_protection_keep_priority_over_the_commission() {
    let (mut app, _, actor) = fixture();
    for kind in [
        "bread",
        catalog::COOKED_MEAL,
        catalog::BERRIES,
        catalog::WATER,
    ] {
        let count = economy::count(app.world(), actor, kind);
        if count > 0 {
            economy::consume(app.world_mut(), actor, kind, count).unwrap();
        }
    }
    for name in ["Hunger", "Thirst", "Warmth"] {
        need(&mut app, actor, name, 10_000);
    }
    let food = production::food_choice(app.world(), actor)
        .unwrap_or_else(|| production::food_preference(app.world(), actor));
    let coins = app.world().get::<village::Wallet>(actor).unwrap().coins;
    let urgent_cost = market::unit_price(app.world(), catalog::WATER).unwrap()
        + market::unit_price(app.world(), food).unwrap();
    let items = economy::free_items(app.world(), actor);
    production::refresh(app.world_mut());
    assert!(production::purchase_target(app.world(), actor, catalog::WATER) >= 1);
    assert!(production::purchase_target(app.world(), actor, food) >= 1);
    assert_eq!(
        production::desired_stock(app.world(), actor, catalog::WARM_CLOTHES),
        1,
        "urgent clothing must not be financed again by the ordinary forecast"
    );
    let coat_limit =
        production::purchase_bid_limit(app.world(), actor, catalog::WARM_CLOTHES).unwrap();
    assert!(coat_limit > 0 && coat_limit <= coins - urgent_cost);
    assert!(
        production::purchase_target(app.world(), actor, "log") <= 1,
        "construction must not spend the urgent clothing bid's reserved money"
    );
    assert_eq!(economy::free_items(app.world(), actor), items);
    assert_eq!(
        app.world().get::<village::Wallet>(actor).unwrap().coins,
        coins
    );
}

#[test]
fn a_cashless_builder_can_use_a_complete_owned_material_bundle_without_refinancing() {
    let (mut app, _, actor) = fixture();
    economy::grant(app.world_mut(), actor, Item::new("log").stack(3));
    economy::grant(app.world_mut(), actor, Item::new("plank").stack(2));
    app.world_mut()
        .get_mut::<village::Wallet>(actor)
        .unwrap()
        .coins = 0;
    let items = economy::free_items(app.world(), actor);
    production::refresh(app.world_mut());
    assert!(matches!(
        colony::construction_intent(app.world(), actor),
        Some(colony::ConstructionIntent::Build { .. })
    ));
    assert!(production::desired_stock(app.world(), actor, "plank") >= 8);
    assert!(production::desired_stock(app.world(), actor, "log") >= 4);
    assert_eq!(production::internal_demand(app.world(), actor, "plank"), 0);
    assert_eq!(economy::free_items(app.world(), actor), items);
    assert_eq!(app.world().get::<village::Wallet>(actor).unwrap().coins, 0);
}

#[test]
fn critical_warmth_funds_available_home_fuel_before_a_forecast_clothing_bid() {
    let (mut app, _, actor) = fixture();
    economy::consume(app.world_mut(), actor, "log", 1).unwrap();
    need(&mut app, actor, "Warmth", 10_000);
    let cash = app.world().get::<village::Wallet>(actor).unwrap().coins;
    let fuel_price = market::unit_price(app.world(), "log").unwrap();
    let items = economy::free_items(app.world(), actor);
    production::refresh(app.world_mut());
    assert_eq!(
        production::purchase_target(app.world(), actor, "log"),
        1,
        "urgent hearth fuel must retain its own paid target before unavailable clothing"
    );
    assert!(
        production::purchase_bid_limit(app.world(), actor, catalog::WARM_CLOTHES).unwrap()
            <= cash - fuel_price
    );
    assert_eq!(economy::free_items(app.world(), actor), items);
    assert_eq!(
        app.world().get::<village::Wallet>(actor).unwrap().coins,
        cash
    );
}
