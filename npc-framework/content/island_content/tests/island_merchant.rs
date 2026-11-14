//! The merchant boat: a periodic visitor with a finite purse and finite
//! stock, trading through the civic trading post; settlers who come with it.
mod life_support;

use island_content::village::{
    self, VillageState, Wallet, catalog, civic, colony, demography, economy as econ, merchant, view,
};
use life_support::*;
use npc_framework::interaction::Item;
use npc_framework::kernel::journal::set_journal;
use npc_framework::prelude::*;
use serde_json::{Value, json};

/// A boat that lands at once, stays a short while and comes back daily.
fn quick_boat(world: &mut Value) {
    short_days(world, 2400);
    only_weather(world, "clear");
    let boat = &mut economy(world)["merchant"];
    boat["first_visit_day"] = json!(0);
    boat["arrive_permille"] = json!(20);
    boat["stay_ticks"] = json!(600);
    boat["interval_days"] = json!(1);
    economy(world)["demography"]["arrivals_per_visit"] = json!(0);
}

#[test]
fn the_boat_calls_on_schedule_and_trades_through_the_trading_post() {
    // The boat buys only planks here, six at most, so what it takes does not
    // depend on what the residents carry to the post for it.
    let mut app = world_with(|world| {
        quick_boat(world);
        economy(world)["merchant"]["buys"] = json!([
            {"good": "plank", "max_units": 6, "price_percent": 80, "keep_units": 8}
        ]);
        economy(world)["tuning"]["stock_target_days"] = json!(0);
    });
    set_journal(app.world_mut(), true, false);
    run_ticks(app.world_mut(), 2);
    let boat = view::boat(app.world());
    assert!(boat.enabled && !boat.docked);
    assert_eq!(boat.purse, 6000);
    assert_eq!(boat.next_visit_day, village::calendar(app.world()).0 + 1);
    let harbour = settlement(app.world(), "harbour");
    assert_eq!(boat.settlement, Some(harbour));
    let post_id = boat.post.expect("the trading post was founded");
    let post = entity(app.world(), post_id);
    assert_eq!(boat.dock, app.world().get::<SimPos>(post).copied());
    assert!(civic::is_depot(app.world(), post));
    assert!(
        boat.buys
            .iter()
            .any(|(kind, price)| kind == "plank" && *price > 0)
    );

    // A carpenter consigns planks; the settlement has coins to import with.
    let seller = residents(app.world(), harbour)[0].1;
    econ::grant(app.world_mut(), seller, Item::new("plank").stack(14));
    let door = *app.world().get::<SimPos>(post).expect("post");
    teleport(app.world_mut(), seller, door);
    civic::depot_deposit(app.world_mut(), seller, post, "plank", 14).expect("consigned");
    fund(app.world_mut(), 900);
    let coins = money(app.world());
    let planks = units(app.world_mut(), "plank");
    let wallet = app.world().get::<Wallet>(seller).expect("wallet").coins;

    // Planks on the post's shelf the moment before the boat lands: residents
    // may buy some in the meantime.
    let mut on_shelf = 14;
    let landed = until(&mut app, 200, 1, |app| {
        let docked = view::boat(app.world()).docked;
        if !docked {
            on_shelf = civic::depot_available(app.world(), post, "plank", None);
        }
        docked
    });
    assert!(landed, "the boat did not land in the morning");
    let expected = on_shelf.saturating_sub(8).min(6);
    assert!(expected > 0, "the planks were bought before the boat came");
    let docked = view::boat(app.world());
    assert_eq!(docked.visits, 1);
    assert!(docked.leaves_at.is_some());
    assert_eq!(effects(app.world(), "VillageMerchantArrived").len(), 1);
    // It buys the planks above the kept level of eight (and above the
    // island's stock target, none here), at 80% of the base price, and pays
    // the owner at once.
    let visit = docked.this_visit.expect("visit totals");
    assert_eq!(visit.bought.get("plank"), Some(&expected));
    let unit = docked
        .buys
        .iter()
        .find(|(kind, _)| kind == "plank")
        .expect("plank price")
        .1;
    assert_eq!(
        app.world().get::<Wallet>(seller).expect("wallet").coins,
        wallet + expected * unit
    );
    assert_eq!(units(app.world_mut(), "plank"), planks - expected);
    let sale = effects(app.world(), "VillageStallSale")
        .into_iter()
        .find(|record| record["buyer"] == "merchant boat")
        .expect("the owner's sale is journaled");
    assert_eq!(sale["quantity"], expected);
    // It sells the fund what the port lacks, into the harbour's market stall
    // near the centre as civic stock.
    assert!(visit.sold_units > 0, "nothing was imported: {visit:?}");
    assert!(visit.sold.contains_key("ingot") || visit.sold.contains_key(catalog::GRAIN));
    let shelf = civic::depot_views(app.world())
        .into_iter()
        .find(|store| Some(store.id) == docked.market)
        .expect("market shelf");
    for (kind, count) in &visit.sold {
        assert!(
            shelf
                .stock
                .iter()
                .any(|(stocked, units)| stocked == kind && units >= count),
            "{kind} is not on the market's shelf"
        );
    }
    // No coin was made or lost: the purse is part of the sum.
    assert_eq!(money(app.world()), coins);
    assert_eq!(
        u64::from(docked.purse),
        6000 - u64::from(visit.bought_coins) + u64::from(visit.sold_coins)
    );

    let left = until(&mut app, 800, 10, |app| !view::boat(app.world()).docked);
    assert!(left, "the boat never sailed");
    let visits = effects(app.world(), "VillageMerchantVisit");
    assert_eq!(visits.len(), 1);
    assert_eq!(visits[0]["bought_units"], expected);
    assert_eq!(visits[0]["bought"]["plank"], expected);
    assert_eq!(visits[0]["purse"], json!(view::boat(app.world()).purse));
    let after = view::boat(app.world());
    assert!(after.aboard.is_empty() && after.this_visit.is_none());
    assert_eq!(
        after.last_visit.as_ref().map(|last| last.bought_units),
        Some(expected)
    );
    assert_eq!(after.next_visit_day, village::calendar(app.world()).0 + 2);
    assert!(after.next_visit_tick > app.world().resource::<SimClock>().tick);
    let state = app.world().resource::<merchant::Merchant>();
    assert_eq!(state.exported.get("plank"), Some(&u64::from(expected)));
    assert_eq!(
        state.imported.values().sum::<u64>(),
        u64::from(after.last_visit.as_ref().expect("last").sold_units)
    );
    assert_eq!(money(app.world()), coins);
    village::integrity::validate_colony(app.world()).expect("valid after a visit");

    // It comes back the next day.
    let again = until(&mut app, 2_400, 20, |app| {
        view::boat(app.world()).visits == 2
    });
    assert!(again, "the boat did not return");
    assert_eq!(money(app.world()), coins);
}

#[test]
fn the_purse_is_finite_and_a_storm_keeps_the_boat_away() {
    // A poor merchant buys only what the purse pays for.
    let mut poor = world_with(|world| {
        quick_boat(world);
        economy(world)["merchant"]["purse"] = json!(7);
        economy(world)["merchant"]["sells"] = json!([]);
    });
    run_ticks(poor.world_mut(), 2);
    let harbour = settlement(poor.world(), "harbour");
    let post = entity(poor.world(), view::boat(poor.world()).post.expect("post"));
    let seller = residents(poor.world(), harbour)[0].1;
    econ::grant(poor.world_mut(), seller, Item::new("plank").stack(20));
    let door = *poor.world().get::<SimPos>(post).expect("post");
    teleport(poor.world_mut(), seller, door);
    civic::depot_deposit(poor.world_mut(), seller, post, "plank", 20).expect("consigned");
    let coins = money(poor.world());
    assert!(until(&mut poor, 200, 10, |app| view::boat(app.world()).docked));
    let boat = view::boat(poor.world());
    let unit = boat
        .buys
        .iter()
        .find(|(kind, _)| kind == "plank")
        .expect("plank")
        .1;
    let bought = boat.this_visit.expect("visit").bought_units;
    assert_eq!(bought, 7 / unit, "the purse caps the purchase");
    assert_eq!(boat.purse, 7 - bought * unit);
    assert_eq!(money(poor.world()), coins);

    // In a storm the boat does not land; the call is put off a day at a time.
    let mut stormy = world_with(|world| {
        quick_boat(world);
        only_weather(world, "storm");
    });
    set_journal(stormy.world_mut(), true, false);
    run_ticks(stormy.world_mut(), 2_600);
    let boat = view::boat(stormy.world());
    assert!(!boat.docked);
    assert_eq!(boat.visits, 0);
    let delays = effects(stormy.world(), "VillageMerchantDelayed");
    assert!(!delays.is_empty(), "the delay is journaled");
    assert_eq!(delays[0]["reason"], "storm");
    assert_eq!(boat.next_visit_day, village::calendar(stormy.world()).0 + 2);
}

#[test]
fn settlers_arrive_by_boat_when_housing_and_food_allow() {
    let settlers = |world: &mut Value| {
        quick_boat(world);
        economy(world)["demography"]["arrivals_per_visit"] = json!(2);
    };
    let mut app = world_with(settlers);
    set_journal(app.world_mut(), true, false);
    run_ticks(app.world_mut(), 2);
    fund(app.world_mut(), 500);
    let coins = money(app.world());
    let before = demography::census(app.world());
    assert_eq!((before.residents, before.arrivals), (42, 0));
    assert!(until(&mut app, 200, 10, |app| view::boat(app.world()).docked));
    let census = demography::census(app.world());
    assert_eq!((census.residents, census.arrivals), (44, 2));
    let arrivals = effects(app.world(), "VillageArrival");
    assert_eq!(arrivals.len(), 2);
    for arrival in &arrivals {
        assert_eq!(arrival["by"], "boat");
        assert_eq!(arrival["grant_coins"], 60);
        let id = SimId(arrival["resident"].as_u64().expect("id") as u32);
        let actor = entity(app.world(), id);
        assert!(
            colony::home_of(app.world(), id).is_some(),
            "a settler is housed"
        );
        let home = view::household(app.world(), id).expect("household");
        assert_eq!(home.person.stage, "adult");
        assert!(home.person.age_seasons >= 8);
        assert_eq!(app.world().get::<Wallet>(actor).expect("wallet").coins, 60);
        assert_eq!(econ::count(app.world(), actor, "bread"), 2);
    }
    assert_eq!(
        view::boat(app.world()).this_visit.expect("visit").settlers,
        2
    );
    // The grant came from the fund and the rations from outside the island.
    assert_eq!(money(app.world()), coins);
    assert_eq!(
        app.world()
            .resource::<merchant::Merchant>()
            .imported
            .get("bread"),
        Some(&4)
    );
    village::integrity::validate_colony(app.world()).expect("settlers are valid residents");

    // No free bed, or too little food per head: nobody lands.
    let mut full = world_with(|world| {
        settlers(world);
        for settlement in world["living_world"]["settlements"]
            .as_array_mut()
            .expect("settlements")
        {
            for home in settlement["homes"].as_array_mut().expect("homes") {
                home["capacity"] = json!(1);
            }
        }
    });
    let mut hungry = world_with(|world| {
        settlers(world);
        economy(world)["demography"]["arrival_food_per_head"] = json!(40);
    });
    for (app, reason) in [
        (&mut full, "no home has a free bed"),
        (&mut hungry, "too little food per head"),
    ] {
        run_ticks(app.world_mut(), 2);
        let harbour = settlement(app.world(), "harbour");
        assert_eq!(demography::can_settle(app.world(), harbour), Err(reason));
        assert!(until(app, 200, 10, |app| view::boat(app.world()).docked));
        assert_eq!(demography::census(app.world()).residents, 42);
        assert_eq!(
            app.world().resource::<VillageState>().fund,
            0,
            "no grant without a settler"
        );
    }
}

/// The trading post stands at the water beside the berth the boat ties up
/// at, and while the boat is in, residents of every settlement carry the
/// island's surplus of what it buys to the post, where it buys it.
#[test]
fn the_trading_post_is_at_the_berth_and_the_island_sells_its_surplus_there() {
    let mut app = world_with(|world| {
        quick_boat(world);
        // The boat stays two short days: the post is at the water, a long
        // walk from the other settlements.
        economy(world)["merchant"]["stay_ticks"] = json!(4_800);
        // No stock target: every log on offer is surplus the boat may buy.
        economy(world)["tuning"]["stock_target_days"] = json!(0);
        // In summer, so carrying goods to the water is not an errand in the
        // cold for anybody without a coat.
        economy(world)["tuning"]["founding_day"] = json!(8);
    });
    set_journal(app.world_mut(), true, false);
    run_ticks(app.world_mut(), 2);
    let boat = view::boat(app.world());
    let berth = boat.berth.expect("a berth");
    let harbour = settlement(app.world(), "harbour");
    // The berth is open sea, the nearest to the harbour's centre.
    assert!(civic::sea(
        app.world(),
        berth.x.div_euclid(1000),
        berth.z.div_euclid(1000)
    ));
    assert_eq!(merchant::berth_of(app.world(), harbour), Some(berth));
    // The post stands at the waterfront, a few metres from the berth.
    let dock = boat.dock.expect("the post's entrance");
    let metres = dock.dist2(berth).isqrt() / 1000;
    assert!(
        metres <= 20,
        "the trading post is {metres} m from the berth"
    );
    // Logs, gathered everywhere, are the island's surplus: the boat buys them.
    let cutters: Vec<_> = all_residents(app.world())
        .into_iter()
        .filter(|(_, actor)| {
            app.world()
                .get::<village::Villager>(*actor)
                .is_some_and(|person| person.role.name() == "woodcutter")
        })
        .collect();
    assert!(!cutters.is_empty());
    for (_, cutter) in &cutters {
        econ::grant(app.world_mut(), *cutter, Item::new("log").stack(12));
    }
    let coins = money(app.world());
    let bought = until(&mut app, 4_800, 20, |app| {
        app.world()
            .resource::<merchant::Merchant>()
            .exported
            .get("log")
            .is_some_and(|units| *units > 0)
    });
    assert!(
        bought,
        "the boat bought no logs: wanted {} cutters {:?}",
        merchant::wanted_surplus(app.world(), "log"),
        cutters
            .iter()
            .map(|(id, cutter)| (
                id.0,
                econ::count(app.world(), *cutter, "log"),
                civic::boat_trip(app.world(), *cutter),
                view::activity_label(app.world(), *cutter),
                econ::need(app.world(), *cutter, "Warmth")
            ))
            .collect::<Vec<_>>()
    );
    let stored = effects(app.world(), "VillageGoodsStored")
        .into_iter()
        .filter(|effect| effect["store"] == json!(boat.post.expect("post")))
        .count();
    assert!(stored > 0, "nobody carried surplus to the trading post");
    assert_eq!(money(app.world()), coins);
}

/// Runs until the boat has left `calls` more times; false if it does not.
fn until_departures(app: &mut bevy_app::App, calls: u32, limit: u64) -> bool {
    let mut left = 0;
    let mut docked = view::boat(app.world()).docked;
    until(app, limit, 1, |app| {
        let now = view::boat(app.world()).docked;
        if docked && !now {
            left += 1;
        }
        docked = now;
        left >= calls
    })
}

fn closed_orders(app: &bevy_app::App) -> Vec<Value> {
    effects(app.world(), "VillageBoatOrderClosed")
}

/// As it leaves, the boat announces what it will buy at its next call, at a
/// price above the island's; the island makes it and sells it there.
#[test]
fn an_announced_order_is_made_and_sold_at_the_next_call() {
    let mut app = world_with(|world| {
        quick_boat(world);
        let boat = &mut economy(world)["merchant"];
        boat["order_goods"] = json!(1);
        boat["order_units"] = json!(4);
        boat["stay_ticks"] = json!(1_200);
        // A 7,200-tick day (the shipped day is 24,000): the post is at the
        // water, 90 to 190 m from the woodcutters, 1,500 to 3,000 ticks of
        // walking, and a carrier goes home at night. In the 2,400-tick day of
        // the other boat tests nobody reaches the post before nightfall, so
        // the order could never be delivered. The island lays the order at
        // the post ahead of the call (review fix), so one day is enough.
        short_days(world, 7_200);
    });
    set_journal(app.world_mut(), true, false);
    assert!(until_departures(&mut app, 1, 4_800), "the boat never left");
    let order = view::boat(app.world()).next_order;
    assert_eq!(order.len(), 1, "{order:?}");
    let line = order[0].clone();
    assert_ne!(line.good, catalog::FIBER);
    assert_ne!(line.good, catalog::THREAD);
    assert_eq!((line.units, line.delivered), (4, 0));
    let quote = village::market::unit_price(app.world(), &line.good).expect("quote");
    assert!(line.unit_price > quote, "{line:?} at a quote of {quote}");
    let coins = money(app.world());
    assert!(
        until_departures(&mut app, 1, 9_600),
        "the boat did not call again"
    );
    let closed = closed_orders(&app);
    let filled = closed
        .iter()
        .find(|order| order["good"] == json!(line.good))
        .expect("the order was closed at the call");
    assert!(
        filled["delivered"].as_u64() > Some(0),
        "nothing of the order was sold: {closed:?}"
    );
    assert_eq!(money(app.world()), coins, "coins are conserved");
    // A new order is announced for the call after.
    assert!(!view::boat(app.world()).next_order.is_empty());
}

/// An order the island cannot fill expires when the boat leaves: nothing is
/// held for it, no coins move for what was not delivered, and it does not
/// stay as demand.
#[test]
fn an_order_nobody_can_fill_expires_without_leftovers() {
    let mut app = world_with(|world| {
        quick_boat(world);
        economy(world)["merchant"]["order_goods"] = json!(1);
    });
    set_journal(app.world_mut(), true, false);
    assert!(until_departures(&mut app, 1, 4_800), "the boat never left");
    app.world_mut().resource_mut::<merchant::Merchant>().order = vec![merchant::BoatOrder {
        good: "ingot".into(),
        units: 900,
        unit_price: 40,
        delivered: 0,
    }];
    let coins = money(app.world());
    assert!(
        until_departures(&mut app, 1, 4_800),
        "the boat did not call again"
    );
    let closed = closed_orders(&app);
    let ingot = closed
        .iter()
        .rev()
        .find(|order| order["good"] == json!("ingot") && order["units"] == json!(900))
        .expect("the impossible order was closed");
    let delivered = ingot["delivered"].as_u64().expect("delivered");
    assert!(delivered < 900, "{ingot}");
    let state = app.world().resource::<merchant::Merchant>();
    assert!(
        state
            .order
            .iter()
            .all(|order| !(order.good == "ingot" && order.units == 900)),
        "the expired order is still open: {:?}",
        state.order
    );
    assert_eq!(money(app.world()), coins, "coins are conserved");
    // Nothing on any shelf is held for the boat.
    for store in civic::depot_views(app.world()) {
        let depot = app
            .world()
            .get::<civic::Depot>(entity(app.world(), store.id))
            .expect("depot");
        assert!(
            depot.holds.values().all(|hold| hold.kind != "ingot"),
            "{store:?}"
        );
    }
}
