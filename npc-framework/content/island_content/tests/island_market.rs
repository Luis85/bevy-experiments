//! Physical, finite market behavior shared by clients and resident policy.
use bevy_app::App;
use bevy_ecs::prelude::*;
use island_content::village::{self, VillageAct, Villager, Wallet, market};
use npc_framework::kernel::hash::state_hash;
use npc_framework::prelude::*;

fn scene() -> App {
    island_content::new_sim_app(7, "living_world")
}
fn person(app: &App, name: &str) -> (SimId, Entity) {
    let id = app.world().resource::<island_content::IslandIds>().id(name);
    (id, app.world().resource::<SimIndex>().get(id).unwrap())
}
fn offer(actor: SimId, quantity: u32, minimum: Option<u32>) -> VillageAct {
    let mut a = VillageAct::new(actor, "offer_goods");
    a.kind = "log".into();
    a.quantity = quantity;
    a.limit_price = minimum;
    a
}

#[test]
fn quotes_and_failed_requests_do_not_learn_or_mutate_market_state() {
    let mut app = scene();
    let (actor, seller) = person(&app, "human");
    let (_, buyer) = person(&app, "Tess");
    let before = state_hash(app.world_mut());
    for _ in 0..20 {
        let _ = market::quote(app.world(), buyer, seller, "log", 1);
        assert!(offer(actor, 20, None).perform(app.world_mut()).is_err());
        let _ = market::public_book(app.world());
    }
    assert_eq!(state_hash(app.world_mut()), before);
}

#[test]
fn post_spend_capacity_frees_only_the_holders_actual_consumed_items() {
    let mut app = scene();
    let (_, holder) = spawn_sim(app.world_mut(), (Inventory::new(1, u32::MAX),));
    village::economy::grant(app.world_mut(), holder, Item::new("log"));
    let owned = village::economy::free_items(app.world(), holder)[0].0;
    let (foreign, _) = spawn_sim(app.world_mut(), (Item::new("log"),));
    let incoming = [Item::new("bread")];
    assert!(village::economy::fit(app.world(), holder, &incoming).is_err());
    assert!(village::economy::fit_without(app.world(), holder, &incoming, &[foreign]).is_err());
    village::economy::fit_without(app.world(), holder, &incoming, &[owned]).unwrap();
    assert_eq!(
        village::economy::count(app.world(), holder, "log"),
        1,
        "capacity checks are pure"
    );
}

#[test]
fn empty_wallets_cannot_create_fictitious_funded_buy_pressure() {
    let mut app = scene();
    let people: Vec<_> = app
        .world_mut()
        .query_filtered::<Entity, With<Wallet>>()
        .iter(app.world())
        .collect();
    for actor in people {
        app.world_mut().get_mut::<Wallet>(actor).unwrap().coins = 0;
    }
    run_ticks(app.world_mut(), 1);
    let book = market::public_book(app.world());
    assert!(!book.is_empty());
    assert!(
        book.iter()
            .all(|row| row["funded_demand"].as_u64() == Some(0))
    );
    assert!(
        book.iter()
            .all(|row| row["below_reference_bid_units"].as_u64() == Some(0)
                && row["max_affordable_bid"].is_null()),
        "a zero purse cannot back even a lower-priced bid"
    );
}

#[test]
fn genuine_need_bids_recover_when_scarcity_pushes_reference_above_all_buyers_cash() {
    let mut app = scene();
    let (_, visitor) = person(&app, "human");
    let people: Vec<_> = app
        .world()
        .resource::<SimIndex>()
        .map
        .values()
        .copied()
        .filter(|e| {
            app.world()
                .get::<Villager>(*e)
                .is_some_and(|v| v.role != village::Role::Visitor)
        })
        .collect();
    let cash_before: u64 = people
        .iter()
        .map(|e| u64::from(app.world().get::<Wallet>(*e).unwrap().coins))
        .sum();
    let maximum_purse = people
        .iter()
        .map(|e| app.world().get::<Wallet>(*e).unwrap().coins)
        .max()
        .unwrap();
    // Isolate one genuine clothing customer from the other markets' scarcity:
    // transfer the baker's actual bread stack to the water keeper, who already
    // owns her two-day water ration. Neither goods nor currency are granted.
    let (_, baker) = person(&app, "Mira");
    let (_, clothed_customer) = person(&app, "Nell");
    let bread = village::economy::free_items(app.world(), baker)
        .into_iter()
        .find(|(_, item)| item.kind == "bread")
        .expect("opening baker bread is real stock")
        .0;
    let bread_entity = app.world().resource::<SimIndex>().get(bread).unwrap();
    npc_framework::interaction::items::put_in_inventory(
        app.world_mut(),
        clothed_customer,
        bread_entity,
    );
    village::production::refresh_all(app.world_mut());
    for kind in ["bread", village::catalog::WATER] {
        assert!(
            village::production::purchase_wanted(app.world(), clothed_customer, kind)
                <= village::economy::count(app.world(), clothed_customer, kind),
            "the isolated customer must already own the forecast food and water ration"
        );
    }
    // A round-kernel scarcity fixture: all existing garments are retained by
    // an unlisted visitor. No items or money are created or destroyed, and the
    // physical producers are deliberately not advanced during these rounds.
    // This isolates price discovery from movement and manufacturing throughput.
    let mut garment_count = 0;
    for actor in &people {
        let garments: Vec<_> = village::economy::free_items(app.world(), *actor)
            .into_iter()
            .filter(|(_, item)| item.kind == village::catalog::WARM_CLOTHES)
            .map(|(id, _)| id)
            .collect();
        for id in garments {
            let e = app.world().resource::<SimIndex>().get(id).unwrap();
            garment_count += app.world().get::<Item>(e).unwrap().stack;
            npc_framework::interaction::items::put_in_inventory(app.world_mut(), visitor, e);
        }
    }
    assert!(
        people.iter().all(|actor| village::economy::count(
            app.world(),
            *actor,
            village::catalog::WARM_CLOTHES
        ) == 0),
        "the fixture leaves no physical garment supply with the producers"
    );
    let interval = market::round_ticks(app.world());
    let start = app.world().resource::<SimClock>().tick;
    for round in 0..100 {
        app.world_mut().resource_mut::<SimClock>().tick = start + round * interval;
        village::production::refresh_all(app.world_mut());
        market::tick(app.world_mut());
    }
    let price = market::unit_price(app.world(), village::catalog::WARM_CLOTHES).unwrap();
    let cash_after: u64 = people
        .iter()
        .map(|e| u64::from(app.world().get::<Wallet>(*e).unwrap().coins))
        .sum();
    assert_eq!(
        cash_after, cash_before,
        "price learning cannot invent buyer money"
    );
    assert_eq!(
        village::economy::count(app.world(), visitor, village::catalog::WARM_CLOTHES),
        garment_count
    );
    let row = market::public_book(app.world())
        .into_iter()
        .find(|row| row["kind"] == village::catalog::WARM_CLOTHES)
        .unwrap();
    assert!(
        price <= maximum_purse,
        "an unaffordable stale reference must learn from real lower-priced buyer bids, not freeze all demand forever: maximum_cash={maximum_purse}, book={row}"
    );
    assert!(
        people.iter().any(|e| village::production::desired_stock(
            app.world(),
            *e,
            village::catalog::WARM_CLOTHES
        ) > 0),
        "remaining garment needs must still be able to commission affordable production; book={:?}; plan={:?}",
        market::public_book(app.world()),
        app.world()
            .resource::<village::production::ProductionPlan>()
    );
}

#[test]
fn funded_material_bids_precede_unfunded_future_water_counterbids() {
    let mut app = scene();
    let (id, maker) = person(&app, "Lina");
    let owned_water = village::economy::count(app.world(), maker, village::catalog::WATER);
    let cash = app.world().get::<Wallet>(maker).unwrap().coins;
    assert_eq!(cash, 200);
    // Restore the real stranded 200-coin water reference from the diagnosed
    // recovery state. The planner has financed one actual spinning batch and
    // deliberately deferred the extra, noncritical water stocking purchase.
    market::tick(app.world_mut());
    {
        let mut state = app.world_mut().resource_mut::<market::MarketState>();
        state
            .commodities
            .get_mut(village::catalog::WATER)
            .unwrap()
            .reference_milli = 200_000;
        state.last_round = None;
    }
    {
        let mut plan = app
            .world_mut()
            .resource_mut::<village::production::ProductionPlan>();
        *plan = Default::default();
        plan.wanted.insert(
            id,
            [
                (village::catalog::WATER.into(), owned_water + 1),
                (village::catalog::FIBER.into(), 2),
            ]
            .into(),
        );
        plan.procurement.insert(
            id,
            [
                (village::catalog::WATER.into(), owned_water),
                (village::catalog::FIBER.into(), 2),
            ]
            .into(),
        );
    }
    market::tick(app.world_mut());
    let book = market::public_book(app.world());
    let water = book
        .iter()
        .find(|row| row["kind"] == village::catalog::WATER)
        .unwrap();
    let fiber = book
        .iter()
        .find(|row| row["kind"] == village::catalog::FIBER)
        .unwrap();
    assert_eq!(
        fiber["funded_demand"], 2,
        "the actual working batch keeps its cash allocation"
    );
    assert_eq!(
        water["funded_demand"], 0,
        "the publisher cannot upgrade an unfunded forecast to a paid order"
    );
    assert_eq!(water["below_reference_bid_units"], 1);
    assert_eq!(water["max_affordable_bid"], cash - 2);
    assert_eq!(
        app.world().get::<Wallet>(maker).unwrap().coins,
        cash,
        "published bids are not magical settlement"
    );
}
