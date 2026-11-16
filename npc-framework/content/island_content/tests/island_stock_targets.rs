//! Prices and oversupply in the living world: reservation prices instead of
//! a flat floor, make-to-stock targets, and the trade volume behind the daily
//! use of goods that are not eaten. Each rule is switched off from data in a
//! control.
mod life_support;

use bevy_app::App;
use bevy_ecs::prelude::*;
use island_content::village::{
    self, Role, Villager, catalog, economy as econ, market, production, stock, view,
};
use life_support::*;
use npc_framework::interaction::Item;
use npc_framework::prelude::*;
use serde_json::json;

fn quiet_world(edit: impl FnOnce(&mut serde_json::Value)) -> App {
    let mut app = world_with(|world| {
        only_weather(world, "clear");
        economy(world)["merchant"]["enabled"] = json!(false);
        edit(world);
    });
    run_ticks(app.world_mut(), 2);
    app
}

fn force_quote(app: &mut App, kind: &str, milli: u32) {
    app.world_mut()
        .resource_mut::<market::MarketState>()
        .commodities
        .get_mut(kind)
        .expect("quoted")
        .reference_milli = milli;
}

fn makers(app: &App, settlement: SimId, role: Role) -> Vec<(SimId, Entity)> {
    residents(app.world(), settlement)
        .into_iter()
        .filter(|(_, actor)| {
            app.world()
                .get::<Villager>(*actor)
                .is_some_and(|person| person.role == role)
        })
        .collect()
}

#[test]
fn a_maker_holds_to_its_input_cost_and_a_gatherer_to_its_raw_floor() {
    let mut app = quiet_world(|_| {});
    let world = app.world();
    let milli = |kind: &str| u64::from(market::reference_milli(world, kind).expect("quoted"));
    // Bake: 2 flour, 1 water, 1 log make 4 bread; margin 10%.
    let cost = (2 * milli(catalog::FLOUR) + milli(catalog::WATER) + milli("log")) * 110 / 100 / 4;
    assert_eq!(
        stock::reservation(world, "bread"),
        Some(cost.div_ceil(1000) as u32)
    );
    // Grain is raw: 25% of its base of 3, never below one coin.
    assert_eq!(stock::reservation(world, catalog::GRAIN), Some(1));
    // With bread forced to one coin, a baker with bread asks its reservation,
    // and its stock is no supply at the forced quote.
    let harbour = settlement(app.world(), "harbour");
    let (_, baker) = makers(&app, harbour, Role::Baker)[0];
    econ::grant(app.world_mut(), baker, Item::new("bread").stack(8));
    force_quote(&mut app, "bread", 1_000);
    let reserve = stock::reservation(app.world(), "bread").expect("reservation");
    assert!(reserve > 1);
    assert_eq!(
        market::asking_price(app.world(), baker, "bread"),
        Some(reserve)
    );
    // Trades happen at the reservation, and the quote climbs towards it by
    // the bounded learning step (a sixteenth a round).
    run_ticks(app.world_mut(), 3_000 * 6);
    let learned = market::reference_milli(app.world(), "bread").expect("quote");
    assert!(learned > 1_000, "the forced quote did not rise: {learned}");
    let reserve = stock::reservation(app.world(), "bread").expect("reservation");
    assert!(
        market::asking_price(app.world(), baker, "bread") >= Some(reserve),
        "a baker never asks below its reservation"
    );
}

#[test]
fn without_reservation_prices_the_forced_quote_stays_low() {
    let mut app = quiet_world(|world| {
        economy(world)["tuning"]["reservation_prices"] = json!(false);
        economy(world)["tuning"]["stock_cover_pricing"] = json!(false);
    });
    assert_eq!(stock::reservation(app.world(), "bread"), None);
    let harbour = settlement(app.world(), "harbour");
    let (_, baker) = makers(&app, harbour, Role::Baker)[0];
    econ::grant(app.world_mut(), baker, Item::new("bread").stack(20));
    force_quote(&mut app, "bread", 1_000);
    assert_eq!(market::asking_price(app.world(), baker, "bread"), Some(1));
}

#[test]
fn makers_start_no_batch_beyond_the_stock_target_and_the_rule_switches_off() {
    let stocked = |days: u32| -> (bool, u32, u32) {
        let mut app = quiet_world(|world| {
            economy(world)["tuning"]["stock_target_days"] = json!(days);
        });
        let harbour = settlement(app.world(), "harbour");
        // The carpenters of the harbour hold far more planks than anybody uses.
        for (_, carpenter) in makers(&app, harbour, Role::Carpenter) {
            econ::grant(app.world_mut(), carpenter, Item::new("plank").stack(20));
        }
        let target = stock::target(app.world(), harbour, "plank");
        let on_offer = stock::on_offer(app.world(), harbour, "plank");
        let over = stock::over_target(app.world(), harbour, "plank");
        // A big order for planks: the plan makes none while stock is over.
        if over {
            // Every settlement at once: the simulation plans one a tick.
            production::refresh_all(app.world_mut());
            for (_, carpenter) in makers(&app, harbour, Role::Carpenter) {
                assert_eq!(
                    production::production_demand(app.world(), carpenter, "plank"),
                    0,
                    "a carpenter was asked to saw planks over the target"
                );
            }
        }
        (over, target, on_offer)
    };
    let (over, target, on_offer) = stocked(2);
    assert!(
        over,
        "{on_offer} planks on offer against a target of {target}"
    );
    assert!(target >= 4, "one batch is always working stock: {target}");
    let (over, target, _) = stocked(0);
    assert!(
        !over && target == 0,
        "the rule is off with stock_target_days 0"
    );
}

#[test]
fn trade_volume_is_a_moving_sum_and_daily_use_covers_every_good() {
    let mut app = quiet_world(|world| {
        economy(world)["tuning"]["trade_window_rounds"] = json!(8);
    });
    let harbour = settlement(app.world(), "harbour");
    // Run a day of rounds; the volume window holds at most eight rounds.
    run_ticks(app.world_mut(), 24_000);
    let volume = app.world().resource::<stock::TradeVolume>();
    assert!(volume.rounds.len() <= 8);
    let sold: u32 = volume
        .rounds
        .iter()
        .filter_map(|round| round.get(&harbour))
        .flat_map(|goods| goods.values())
        .sum();
    // Daily use reports goods beyond food and water once they trade.
    let uses = view::daily_use(app.world(), harbour);
    assert!(uses.iter().any(|row| row.good == catalog::WATER));
    if sold > 0 {
        assert!(
            uses.iter().any(|row| row.good != catalog::WATER
                && catalog::food_restore_in(app.world(), &row.good).is_none()
                && row.per_day > 0),
            "{uses:?}"
        );
    }
    village::integrity::validate_colony(app.world()).expect("valid");
}
