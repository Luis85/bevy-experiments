//! Physical, finite market behavior shared by clients and resident policy.
use bevy_app::App;
use bevy_ecs::prelude::*;
use island_content::village::{self, VillageAct, Villager, Wallet, market};
use npc_framework::agent_io::{api, registration};
use npc_framework::kernel::hash::state_hash;
use npc_framework::prelude::*;
use serde_json::json;

fn scene() -> App {
    island_content::new_sim_app(7, "living_village")
}
fn person(app: &App, name: &str) -> (SimId, Entity) {
    let id = app
        .world()
        .resource::<island_content::TestbedIds>()
        .id(name);
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
fn listed_visitor_goods_sell_physically_for_the_same_quote_and_conserve_cash() {
    let mut app = scene();
    let (seller_id, seller) = person(&app, "human");
    let (buyer_id, buyer) = person(&app, "Tess");
    let pos = *app.world().get::<SimPos>(seller).unwrap();
    app.world_mut().entity_mut(buyer).insert(pos);
    village::economy::grant(app.world_mut(), seller, Item::new("log").stack(5));
    let cash = app.world().get::<Wallet>(seller).unwrap().coins
        + app.world().get::<Wallet>(buyer).unwrap().coins;
    offer(seller_id, 3, Some(7))
        .perform(app.world_mut())
        .unwrap();
    assert!(market::sells(app.world(), seller, "log"));
    assert_eq!(market::available_to_sell(app.world(), seller, "log"), 3);
    let quote = market::quote(app.world(), buyer, seller, "log", 2).unwrap();
    assert_eq!(quote.unit_price, 7);
    let mut buy = VillageAct::new(buyer_id, "buy");
    buy.target = Some(seller_id);
    buy.kind = "log".into();
    buy.quantity = 2;
    buy.limit_price = Some(7);
    buy.perform(app.world_mut()).unwrap();
    assert_eq!(
        app.world().get::<Wallet>(seller).unwrap().coins,
        quote.total
    );
    assert_eq!(
        app.world().get::<Wallet>(seller).unwrap().coins
            + app.world().get::<Wallet>(buyer).unwrap().coins,
        cash
    );
    assert_eq!(village::economy::count(app.world(), seller, "log"), 3);
    assert_eq!(market::available_to_sell(app.world(), seller, "log"), 1);
    assert!(
        buy.perform(app.world_mut()).is_err(),
        "a listing cannot oversell its stated quantity"
    );
}

#[test]
fn offers_cannot_mint_goods_or_sell_reserved_items_and_can_be_withdrawn() {
    let mut app = scene();
    let (actor, body) = person(&app, "human");
    assert!(offer(actor, 1, None).check(app.world()).is_err());
    village::economy::grant(app.world_mut(), body, Item::new("log").stack(2));
    assert!(offer(actor, 3, None).check(app.world()).is_err());
    let stack = village::economy::free_items(app.world(), body)
        .into_iter()
        .find(|(_, i)| i.kind == "log")
        .unwrap()
        .0;
    assert!(
        app.world_mut()
            .resource_mut::<Reservations>()
            .try_reserve(stack, actor, 1)
    );
    assert!(offer(actor, 1, None).check(app.world()).is_err());
    app.world_mut()
        .resource_mut::<Reservations>()
        .release(stack, actor);
    offer(actor, 2, None).perform(app.world_mut()).unwrap();
    let before = village::economy::count(app.world(), body, "log");
    let mut withdraw = VillageAct::new(actor, "withdraw_offer");
    withdraw.kind = "log".into();
    withdraw.perform(app.world_mut()).unwrap();
    assert!(!market::sells(app.world(), body, "log"));
    assert_eq!(village::economy::count(app.world(), body, "log"), before);
}

#[test]
fn price_limits_are_checked_again_at_application_without_partial_trade() {
    let mut app = scene();
    let (seller_id, seller) = person(&app, "human");
    let (buyer_id, buyer) = person(&app, "Tess");
    let pos = *app.world().get::<SimPos>(seller).unwrap();
    app.world_mut().entity_mut(buyer).insert(pos);
    village::economy::grant(app.world_mut(), seller, Item::new("log").stack(2));
    offer(seller_id, 2, Some(3))
        .perform(app.world_mut())
        .unwrap();
    let mut buy = VillageAct::new(buyer_id, "buy");
    buy.target = Some(seller_id);
    buy.kind = "log".into();
    buy.limit_price = Some(3);
    buy.check(app.world()).unwrap();
    offer(seller_id, 2, Some(9))
        .perform(app.world_mut())
        .unwrap();
    let before = state_hash(app.world_mut());
    assert!(buy.perform(app.world_mut()).is_err());
    assert_eq!(state_hash(app.world_mut()), before);
    let mut sale = VillageAct::new(seller_id, "sell");
    sale.target = Some(buyer_id);
    sale.kind = "log".into();
    // Isolate the price condition with an explicit, funded Innkeeper log
    // procurement target; other integration fixtures exercise plan generation.
    let target = village::economy::count(app.world(), buyer, "log") + 1;
    app.world_mut()
        .resource_mut::<village::production::ProductionPlan>()
        .procurement
        .entry(buyer_id)
        .or_default()
        .insert("log".into(), target);
    sale.limit_price = Some(9);
    sale.check(app.world()).unwrap();
    sale.limit_price = Some(10);
    let before_sell = state_hash(app.world_mut());
    assert!(
        sale.perform(app.world_mut())
            .unwrap_err()
            .contains("limit_price")
    );
    assert_eq!(state_hash(app.world_mut()), before_sell);
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
fn listed_goods_and_price_learning_resume_identically() {
    fn factory() -> App {
        let mut app = scene();
        let (id, e) = person(&app, "human");
        village::economy::grant(app.world_mut(), e, Item::new("log").stack(3));
        offer(id, 3, Some(4)).perform(app.world_mut()).unwrap();
        app
    }
    npc_framework::tools::snapshot_check::assert_resume_equivalent(factory, 123, 900, 30);
}

#[test]
fn legacy_village_stays_fixed_price_and_rejects_passive_market_listings() {
    let mut app = island_content::new_sim_app(7, "ai_village");
    let (actor, e) = person(&app, "human");
    assert!(!market::enabled(app.world()));
    assert!(!app.world().contains_resource::<market::MarketState>());
    village::economy::grant(app.world_mut(), e, Item::new("log").stack(3));
    assert!(offer(actor, 3, None).check(app.world()).is_err());
    assert_eq!(market::unit_price(app.world(), "log"), Some(3));
    run_ticks(app.world_mut(), 1000);
    assert_eq!(market::unit_price(app.world(), "log"), Some(3));
    let person = app.world().get::<Villager>(e).unwrap();
    assert_eq!(person.role, village::Role::Visitor);
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
fn registered_client_posts_and_withdraws_bounded_offers_through_recorded_actions() {
    let mut app = scene();
    let (id, actor) = person(&app, "human");
    village::economy::grant(app.world_mut(), actor, Item::new("log").stack(3));
    app.insert_resource(registration::RegistrationRequired);
    registration::set_credential(
        app.world_mut(),
        "market-fixture",
        registration::Credential {
            token_digest: registration::token_digest("public-test-market-token"),
            assigned: id,
            controller: "visitor".into(),
        },
    );
    let session = api::call(
        app.world_mut(),
        "agent/register",
        Some(json!({
            "agent_id":"market-fixture","token":"public-test-market-token",
        })),
    )
    .unwrap()["session"]
        .as_str()
        .unwrap()
        .to_owned();
    for method in ["agent/enter_world", "agent/take_control"] {
        api::call(app.world_mut(), method, Some(json!({"session":session}))).unwrap();
    }
    run_ticks(app.world_mut(), 1);
    let before = state_hash(app.world_mut());
    let invalid = api::call(app.world_mut(), "agent/act", Some(json!({
        "session":session,"verb":"offer_goods","args":{"kind":"log","quantity":2,"limit_price":0},
    }))).unwrap();
    assert_ne!(invalid["accepted"], true, "invalid prices must be rejected");
    assert_eq!(state_hash(app.world_mut()), before);
    let accepted = api::call(app.world_mut(), "agent/act", Some(json!({
        "session":session,"verb":"offer_goods","args":{"kind":"log","quantity":2,"limit_price":4},
    }))).unwrap();
    assert_eq!(accepted["accepted"], true, "{accepted}");
    assert!(
        !market::has_offer(app.world(), actor, "log"),
        "input applies on the next sim tick"
    );
    run_ticks(app.world_mut(), 1);
    assert!(market::has_offer(app.world(), actor, "log"));
    assert_eq!(market::available_to_sell(app.world(), actor, "log"), 2);
    let withdrawn = api::call(
        app.world_mut(),
        "agent/act",
        Some(json!({
            "session":session,"verb":"withdraw_offer","args":{"kind":"log"},
        })),
    )
    .unwrap();
    assert_eq!(withdrawn["accepted"], true, "{withdrawn}");
    run_ticks(app.world_mut(), 1);
    assert!(!market::has_offer(app.world(), actor, "log"));
    assert_eq!(village::economy::count(app.world(), actor, "log"), 3);
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
    village::production::refresh(app.world_mut());
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
        village::production::refresh(app.world_mut());
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

#[test]
fn joint_material_limits_back_all_units_and_a_seller_cannot_override_the_buyers_cap() {
    let mut app = scene();
    let (maker_id, maker) = person(&app, "Lina");
    let (seller_id, seller) = person(&app, "human");
    let surplus = app.world().get::<Wallet>(maker).unwrap().coins - 20;
    app.world_mut().get_mut::<Wallet>(maker).unwrap().coins = 20;
    app.world_mut().resource_mut::<village::VillageState>().fund += surplus;
    market::tick(app.world_mut());
    app.world_mut()
        .resource_mut::<market::MarketState>()
        .last_round = None;
    {
        let mut plan = app
            .world_mut()
            .resource_mut::<village::production::ProductionPlan>();
        *plan = Default::default();
        let goods = [
            (village::catalog::LEATHER.into(), 2),
            (village::catalog::FIBER.into(), 4),
        ];
        plan.wanted.insert(maker_id, goods.clone().into());
        plan.procurement.insert(maker_id, goods.into());
        plan.bid_limits.insert(
            maker_id,
            [
                (village::catalog::LEATHER.into(), 6),
                (village::catalog::FIBER.into(), 2),
            ]
            .into(),
        );
    }
    assert_eq!(
        2 * 6 + 4 * 2,
        app.world().get::<Wallet>(maker).unwrap().coins
    );
    market::tick(app.world_mut());
    let book = market::public_book(app.world());
    let leather = book
        .iter()
        .find(|row| row["kind"] == village::catalog::LEATHER)
        .unwrap();
    let fiber = book
        .iter()
        .find(|row| row["kind"] == village::catalog::FIBER)
        .unwrap();
    assert_eq!(leather["funded_demand"], 0);
    assert_eq!(
        leather["below_reference_bid_units"], 2,
        "the entire joined lot is financed at its stated limit"
    );
    assert_eq!(leather["max_affordable_bid"], 6);
    assert_eq!(
        fiber["funded_demand"], 4,
        "the lower leather bid preserves all complementary fiber cash"
    );
    assert_eq!(app.world().get::<Wallet>(maker).unwrap().coins, 20);

    // Operator-arranged quote test, separate from the unassisted registered
    // production rehearsal: a real offered item cannot force an above-cap buy.
    village::economy::grant(
        app.world_mut(),
        seller,
        Item::new(village::catalog::LEATHER),
    );
    let pos = *app.world().get::<SimPos>(seller).unwrap();
    app.world_mut().entity_mut(maker).insert(pos);
    let mut sale = VillageAct::new(seller_id, "sell");
    sale.target = Some(maker_id);
    sale.kind = village::catalog::LEATHER.into();
    let before = state_hash(app.world_mut());
    assert!(
        sale.perform(app.world_mut())
            .unwrap_err()
            .contains("bid limit")
    );
    assert_eq!(state_hash(app.world_mut()), before);
}
