//! A housing commission finances one joined material bundle before future comfort reserves.
use bevy_app::App;
use bevy_ecs::prelude::*;
use island_content::{
    TestbedIds,
    village::{self, catalog, colony, economy, market, production},
};
use npc_framework::needs::{NeedRegistry, Needs};
use npc_framework::prelude::*;

fn fixture() -> (App, SimId, Entity) {
    let mut app = island_content::new_sim_app(7, "living_world");
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
