//! Preventive replacement is a cash-backed household demand, never a free coat.
use bevy_app::App;
use bevy_ecs::prelude::*;
use island_content::village::{self, Role, Villager, Wallet, catalog, economy, living, production};
use npc_framework::crafting::Durability;
use npc_framework::interaction::items;
use npc_framework::prelude::*;

fn actor(app: &App, name: &str) -> Entity {
    let id = app.world().resource::<island_content::IslandIds>().id(name);
    app.world().resource::<SimIndex>().get(id).unwrap()
}
fn give_coat(app: &mut App, person: Entity, durability: u32, equip: bool) -> Entity {
    economy::grant(app.world_mut(), person, Item::new(catalog::WARM_CLOTHES));
    let id = economy::free_items(app.world(), person)
        .into_iter()
        .find(|(_, item)| item.kind == catalog::WARM_CLOTHES)
        .unwrap()
        .0;
    let item = app.world().resource::<SimIndex>().get(id).unwrap();
    app.world_mut().entity_mut(item).insert(Durability {
        current: durability,
        max: catalog::CLOTHING_DURABILITY,
    });
    if equip {
        items::put_in_slot(app.world_mut(), person, item, "body");
    }
    item
}
fn prepared() -> App {
    let mut app = island_content::new_sim_app(7, "living_world");
    let people: Vec<_> = app
        .world()
        .resource::<SimIndex>()
        .map
        .values()
        .copied()
        .filter(|entity| {
            app.world()
                .get::<Villager>(*entity)
                .is_some_and(|person| person.role != Role::Visitor)
        })
        .collect();
    for person in people {
        give_coat(&mut app, person, catalog::CLOTHING_DURABILITY, true);
    }
    // Declared fixture: winter with all original wallets/stocks/needs intact,
    // except finite garments assigned above to isolate one replacement need.
    app.world_mut().resource_mut::<SimClock>().tick = 144_000;
    app
}
fn wear(app: &mut App, person: Entity, remaining: u32) {
    let id = items::equipped_matching(app.world(), person, catalog::WARM_CLOTHES).unwrap();
    let entity = app.world().resource::<SimIndex>().get(id).unwrap();
    app.world_mut()
        .get_mut::<Durability>(entity)
        .unwrap()
        .current = remaining;
}
fn refresh(app: &mut App) {
    // Every settlement at once: the simulation plans one a tick.
    production::refresh_all(app.world_mut());
}
fn coins(app: &App) -> u64 {
    app.world()
        .resource::<SimIndex>()
        .map
        .values()
        .map(|entity| {
            u64::from(
                app.world()
                    .get::<Wallet>(*entity)
                    .map_or(0, |wallet| wallet.coins),
            )
        })
        .sum::<u64>()
        + u64::from(app.world().resource::<village::VillageState>().fund)
}

#[test]
fn paid_carried_spare_is_reserved_once_without_replacing_a_still_usable_coat() {
    let mut app = prepared();
    let ada = actor(&app, "Ada");
    let lina = actor(&app, "Lina");
    wear(&mut app, ada, 40_000);
    let worn = items::equipped_matching(app.world(), ada, catalog::WARM_CLOTHES).unwrap();
    // The physical seller lot exists before planning and is credited once.
    give_coat(&mut app, lina, catalog::CLOTHING_DURABILITY, false);
    refresh(&mut app);
    assert_eq!(
        production::production_demand(app.world(), lina, catalog::WARM_CLOTHES),
        0
    );
    let cash = coins(&app);
    let buyer_cash = app.world().get::<Wallet>(ada).unwrap().coins;
    let seller_pos = *app.world().get::<SimPos>(lina).unwrap();
    app.world_mut().entity_mut(ada).insert(seller_pos);
    let mut buy = village::VillageAct::new(*app.world().get::<SimId>(ada).unwrap(), "buy");
    buy.target = app.world().get::<SimId>(lina).copied();
    buy.kind = catalog::WARM_CLOTHES.into();
    buy.perform(app.world_mut()).unwrap();
    refresh(&mut app);
    assert_eq!(economy::count(app.world(), ada, catalog::WARM_CLOTHES), 1);
    assert_eq!(
        items::equipped_matching(app.world(), ada, catalog::WARM_CLOTHES),
        Some(worn)
    );
    assert_eq!(
        production::wanted_stock(app.world(), ada, catalog::WARM_CLOTHES),
        1
    );
    assert_eq!(
        production::purchase_target(app.world(), ada, catalog::WARM_CLOTHES),
        1
    );
    // The coat is a staple with a planned stock (closing design): the sale
    // took Lina's spare off the shelf, so she gets standing work to restock
    // it, never more than the settlement's shortfall. Ada's carried spare is
    // not bought again (above), and no order is made for it.
    let region = village::frontier::settlements::member(app.world(), lina).expect("settlement");
    let restock =
        village::stock::target(app.world(), region, catalog::WARM_CLOTHES).saturating_sub(
            village::stock::on_offer(app.world(), region, catalog::WARM_CLOTHES),
        );
    assert!(
        production::production_demand(app.world(), lina, catalog::WARM_CLOTHES) <= restock,
        "Lina's coat work exceeds the planned stock's shortfall of {restock}"
    );
    assert!(app.world().get::<Wallet>(ada).unwrap().coins < buyer_cash);
    assert_eq!(coins(&app), cash);
}

#[test]
fn replacement_horizon_counts_calendar_cold_and_only_known_sheltered_rest() {
    let mut app = prepared();
    let ada = actor(&app, "Ada");
    for (tick, expected) in [
        (0, 14_400),
        (24_000, 31_200),
        (48_000, 48_000),
        (216_000, 14_400),
    ] {
        app.world_mut().resource_mut::<SimClock>().tick = tick;
        assert_eq!(living::clothing_lead_exposure(app.world(), ada), expected);
    }
    app.world_mut().resource_mut::<SimClock>().tick = 48_000;
    {
        let mut person = app.world_mut().get_mut::<Villager>(ada).unwrap();
        person.resting_until = 48_300;
        person.rest_rate = village::HOME_REST_RATE;
    }
    assert_eq!(living::clothing_lead_exposure(app.world(), ada), 47_700);
    app.world_mut().resource_mut::<SimClock>().tick = 216_000;
    app.world_mut().resource_mut::<SimConfig>().night_permille = 0;
    wear(&mut app, ada, 1);
    assert_eq!(living::clothing_lead_exposure(app.world(), ada), 0);
    assert_eq!(living::clothing_reserve_target(app.world(), ada), 0);
}

#[test]
fn newly_equipped_coat_does_not_rebuy_against_a_stale_procurement_round() {
    let mut app = prepared();
    let ada = actor(&app, "Ada");
    let lina = actor(&app, "Lina");
    give_coat(&mut app, lina, catalog::CLOTHING_DURABILITY, false);
    let others: Vec<_> = app
        .world()
        .resource::<SimIndex>()
        .map
        .values()
        .copied()
        .filter(|entity| *entity != ada && app.world().get::<Villager>(*entity).is_some())
        .collect();
    for entity in others {
        app.world_mut().entity_mut(entity).remove::<Brain>();
    }
    let id = *app.world().get::<SimId>(ada).unwrap();
    let seller_pos = *app.world().get::<SimPos>(lina).unwrap();
    app.world_mut().entity_mut(ada).insert(seller_pos);
    let mut previous = production::ProductionPlan {
        refreshed_at: Some(144_000),
        ..Default::default()
    };
    previous.procurement.insert(
        id,
        std::collections::BTreeMap::from([(catalog::WARM_CLOTHES.into(), 1)]),
    );
    app.world_mut().insert_resource(previous);
    let cash = app.world().get::<Wallet>(ada).unwrap().coins;
    run_ticks(app.world_mut(), 40);
    assert_eq!(economy::count(app.world(), ada, catalog::WARM_CLOTHES), 0);
    assert_eq!(app.world().get::<Wallet>(ada).unwrap().coins, cash);
}
