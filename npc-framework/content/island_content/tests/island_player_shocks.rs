//! Player shocks on the regional island: a food buy-out and coin hoarding.
//!
//! They run two or three ordinary in-game days (several minutes each), so they
//! are explicit. Run:
//! cargo test --locked -p island_content --test island_player_shocks -- --ignored --nocapture
use bevy_app::App;
use bevy_ecs::prelude::*;
use island_content::village::{
    self, CoinPurse, VillageState, Villager, Vitality, Wallet, catalog, economy,
    frontier::labor::ResidentProfile, market,
};
use npc_framework::prelude::*;

const DAY: u64 = 24_000;
const HORIZON: u64 = 2 * DAY;
/// The buy-out lasts one day and bread must be affordable again two days later.
const BUYOUT_HORIZON: u64 = 3 * DAY;
/// Recovered means back within this multiple of the base price.
const RECOVERED_FACTOR: u32 = 2;
const STEP: u64 = 600;
/// `economy.tuning.price_ceiling_percent` of the shipped coastal world.
const PRICE_CEILING_PERCENT: u32 = 500;

fn residents(world: &World) -> Vec<(SimId, Entity)> {
    world
        .resource::<SimIndex>()
        .map
        .iter()
        .filter(|(_, entity)| world.get::<ResidentProfile>(**entity).is_some())
        .map(|(id, entity)| (*id, *entity))
        .collect()
}
fn player(world: &World) -> Entity {
    let id = world.resource::<VillageState>().played;
    world.resource::<SimIndex>().get(id).expect("played actor")
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
        // The boat's purse and coins in escrow close the system.
        + u64::from(village::merchant::purse(world))
        + village::live::escrow(world)
}
fn assert_all_alive(world: &World, label: &str) {
    let now = world.resource::<SimClock>().tick;
    for (id, actor) in residents(world) {
        let vitality = world.get::<Vitality>(actor).expect("vitality");
        assert!(
            vitality.died_at.is_none() && vitality.health > 0,
            "{label}: {} ({}) died by tick {now}",
            world.get::<Villager>(actor).expect("resident").name,
            id.0
        );
    }
}
fn minimum_need(world: &World, need: &str) -> i32 {
    residents(world)
        .into_iter()
        .map(|(_, actor)| economy::need(world, actor, need))
        .min()
        .unwrap_or(0)
}

/// Where the bread is: every farmer, miller and baker with what they hold, are
/// asked to make and are funded to buy along the grain, flour and bread chain,
/// and what they are doing. Printed with every price sample to explain a slow
/// recovery.
fn bread_chain(world: &World) -> String {
    let patches: u32 = world
        .resource::<SimIndex>()
        .map
        .values()
        .filter_map(|entity| world.get::<village::ResourcePatch>(*entity))
        .filter(|patch| patch.kind == catalog::GRAIN)
        .map(|patch| patch.remaining)
        .sum();
    let mut rows = vec![format!("grain_patches={patches}")];
    for (_, actor) in residents(world) {
        let person = world.get::<Villager>(actor).expect("resident");
        let tag = match person.role {
            village::Role::Farmer => "F",
            village::Role::Miller => "M",
            village::Role::Baker => "B",
            _ => continue,
        };
        let goods: Vec<String> = [catalog::GRAIN, catalog::FLOUR, "bread"]
            .into_iter()
            .filter_map(|kind| {
                let have = economy::count(world, actor, kind);
                let make = village::production::production_demand(world, actor, kind);
                let buy = village::production::purchase_target(world, actor, kind);
                (have + make + buy > 0).then(|| {
                    format!(
                        "{}:{have}h{make}m{buy}b",
                        kind.trim_start_matches("village_")
                    )
                })
            })
            .collect();
        rows.push(format!(
            "{tag}[{} {}c {}]",
            goods.join(" "),
            world.get::<Wallet>(actor).map_or(0, |w| w.coins),
            village::behavior::decision(world, actor).unwrap_or("-"),
        ));
    }
    rows.join(" ")
}

/// The player walks up to every seller and buys all bread and cooked meals on
/// offer through the ordinary validated trade, then removes the food from play.
fn buy_out(app: &mut App) -> u32 {
    let buyer = player(app.world());
    let mut bought = 0;
    for (_, seller) in residents(app.world()) {
        for kind in ["bread", catalog::COOKED_MEAL] {
            let offered = market::available_to_sell(app.world(), seller, kind);
            if offered == 0 {
                continue;
            }
            let at = *app.world().get::<SimPos>(seller).expect("seller position");
            app.world_mut().entity_mut(buyer).insert(at);
            for quantity in (1..=offered.min(20)).rev() {
                match economy::trade(app.world_mut(), buyer, seller, kind, quantity) {
                    Ok(()) => {
                        economy::consume(app.world_mut(), buyer, kind, quantity)
                            .expect("bought food is carried");
                        bought += quantity;
                        break;
                    }
                    Err(reason) if quantity == 1 => {
                        println!(
                            "PLAYER_BUYOUT_REFUSED kind={kind} offered={offered} reason={reason}"
                        );
                    }
                    Err(_) => {}
                }
            }
        }
    }
    bought
}

#[test]
#[ignore = "explicit 48000-tick player food buy-out shock (several minutes)"]
fn village_survives_a_player_buying_out_all_bread_and_meals_for_a_day() {
    let mut app = island_content::new_sim_app(7, "living_world");
    let buyer = player(app.world());
    let unit = app.world().resource::<VillageState>().played;
    village::colony::JoinIsland { unit: unit.0 }
        .perform(app.world_mut())
        .expect("the player arrives");
    // A declared outside purse: these coins enter the village as the player pays.
    app.world_mut()
        .get_mut::<Wallet>(buyer)
        .expect("wallet")
        .coins = 5_000;
    let total = money(app.world());
    let mut bought = 0;
    let base = market::unit_price(app.world(), "bread").expect("bread price");
    let ceiling = base * PRICE_CEILING_PERCENT / 100;
    let (mut peak, mut peak_tick, mut after_buyout) = (base, 0, base);
    let mut lowest_after = u32::MAX;
    let mut recovered_at = None;
    let horizon = std::env::var("ISLAND_SHOCK_DAYS")
        .ok()
        .and_then(|days| days.parse::<u64>().ok())
        .map_or(BUYOUT_HORIZON, |days| days.max(3) * DAY);
    // Diagnostics only: stop early to read the chain around the end of the buy-out.
    let stop = std::env::var("ISLAND_SHOCK_STOP_TICK")
        .ok()
        .and_then(|tick| tick.parse::<u64>().ok());
    while app.world().resource::<SimClock>().tick < horizon {
        // The buy-out is sustained through the first day, then the player stops.
        if app.world().resource::<SimClock>().tick < DAY {
            bought += buy_out(&mut app);
        }
        // The script keeps its own buyer fed and warm; this test is about the village.
        for need in ["Hunger", "Thirst", "Energy", "Warmth"] {
            economy::restore(app.world_mut(), buyer, need, 100_000);
        }
        run_ticks(app.world_mut(), STEP);
        assert_all_alive(app.world(), "buy-out");
        assert_eq!(money(app.world()), total, "currency drift during buy-out");
        let now = app.world().resource::<SimClock>().tick;
        let price = market::unit_price(app.world(), "bread").expect("bread price");
        assert!(
            price <= ceiling,
            "bread reached {price} coins at tick {now}, above the {ceiling}-coin ceiling"
        );
        if price > peak {
            (peak, peak_tick) = (price, now);
        }
        if now == DAY {
            after_buyout = price;
        }
        if now > DAY {
            lowest_after = lowest_after.min(price);
            if recovered_at.is_none() && price <= base * RECOVERED_FACTOR {
                recovered_at = Some(now);
            }
        }
        if stop.is_some_and(|stop| now >= stop) {
            println!("PLAYER_BUYOUT_STOPPED tick={now} (diagnostic run, nothing asserted)");
            return;
        }
        if now.is_multiple_of(if stop.is_some() { 1_200 } else { 2_400 }) {
            let sample = app
                .world()
                .resource::<market::MarketState>()
                .commodities
                .get("bread")
                .and_then(|row| row.history.last())
                .map(|sample| (sample.supply, sample.funded_demand, sample.traded_units));
            println!(
                "PLAYER_BUYOUT_PRICE tick={now} bread={price} (supply, funded demand, traded)={sample:?} {}",
                bread_chain(app.world())
            );
        }
    }
    let recovered = market::unit_price(app.world(), "bread").expect("bread price");
    println!(
        "PLAYER_BUYOUT_RECOVERY base={base} ceiling={ceiling} peak={peak} peak_tick={peak_tick} end_of_buyout={after_buyout} recovered_at={recovered_at:?} lowest_after={lowest_after} final={recovered}"
    );
    assert!(
        after_buyout > base,
        "the buy-out never moved the bread price"
    );
    // The shortage ends with the buy-out: bakers bake from the flour they keep
    // and the quote comes down as deliveries answer the open orders. Winter
    // begins one day after the buy-out ends (tick 48000) and slows the grain.
    println!(
        "PLAYER_BUYOUT_RECOVERED before_winter={}",
        recovered_at.is_some_and(|tick| tick < 2 * DAY)
    );
    assert!(
        recovered_at.is_some_and(|tick| tick <= DAY + 2 * DAY),
        "bread was still above {} coins two days after the buy-out ended (recovered at {recovered_at:?})",
        base * RECOVERED_FACTOR
    );
    let state = app.world().resource::<VillageState>();
    println!(
        "PLAYER_BUYOUT bought={bought} player_coins={} min_hunger={} bread_price={:?} meal_price={:?} produced_bread={:?} produced_meals={:?} transactions={}",
        app.world().get::<Wallet>(buyer).map_or(0, |w| w.coins),
        minimum_need(app.world(), "Hunger"),
        market::unit_price(app.world(), "bread"),
        market::unit_price(app.world(), catalog::COOKED_MEAL),
        state.produced.get("bread"),
        state.produced.get(catalog::COOKED_MEAL),
        state.transactions,
    );
    assert!(bought > 0, "the scripted buy-out never bought any food");
    assert!(
        state.produced.get("bread").copied().unwrap_or(0)
            + state
                .produced
                .get(catalog::COOKED_MEAL)
                .copied()
                .unwrap_or(0)
            > 0,
        "no finished food was produced in response"
    );
    village::integrity::validate_colony(app.world()).expect("colony integrity");
}

#[test]
#[ignore = "explicit 48000-tick player coin-hoarding shock (several minutes)"]
fn village_survives_a_player_hoarding_half_of_all_coins() {
    let mut app = island_content::new_sim_app(7, "living_world");
    let hoarder = player(app.world());
    let total = money(app.world());
    // The player has drained half of every purse and never spends it. The hoarder
    // has not begun a life, so the coins cannot return through a death drop.
    let mut hoard = 0;
    for (_, actor) in residents(app.world()) {
        let mut wallet = app.world_mut().get_mut::<Wallet>(actor).expect("wallet");
        let taken = wallet.coins / 2;
        wallet.coins -= taken;
        hoard += taken;
    }
    app.world_mut()
        .get_mut::<Wallet>(hoarder)
        .expect("wallet")
        .coins += hoard;
    assert_eq!(money(app.world()), total);
    assert_eq!(u64::from(hoard) * 2, total);
    let before = app.world().resource::<VillageState>().transactions;
    while app.world().resource::<SimClock>().tick < HORIZON {
        run_ticks(app.world_mut(), STEP);
        assert_all_alive(app.world(), "coin hoard");
        assert_eq!(money(app.world()), total, "currency drift during hoarding");
    }
    let state = app.world().resource::<VillageState>();
    println!(
        "PLAYER_HOARD hoard={hoard} player_coins={} min_hunger={} min_thirst={} fund={} transactions={} produced={:?}",
        app.world().get::<Wallet>(hoarder).map_or(0, |w| w.coins),
        minimum_need(app.world(), "Hunger"),
        minimum_need(app.world(), "Thirst"),
        state.fund,
        state.transactions - before,
        state.produced,
    );
    assert_eq!(
        app.world().get::<Wallet>(hoarder).map_or(0, |w| w.coins),
        hoard,
        "the hoard must stay out of circulation"
    );
    assert!(
        state.transactions > before,
        "trade stopped under coin scarcity"
    );
    village::integrity::validate_colony(app.world()).expect("colony integrity");
}
