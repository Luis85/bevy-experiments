//! Three normal days of autonomous resident life, with a separate late-flow gate.
//! No needs, inventories, wallets, professions or clock settings are altered.
//! The idle visiting hero is not an unaided-player-survival fixture.
use bevy_app::App;
use bevy_ecs::prelude::*;
use island_content::village::{
    self, CoinPurse, Role, VillageState, Villager, Vitality, Wallet, catalog,
};
use npc_framework::prelude::*;
use serde_json::json;
use std::collections::BTreeMap;

const DAY_TICKS: u64 = 24_000;
const LATE_START: u64 = 2 * DAY_TICKS;
const END: u64 = 3 * DAY_TICKS;
const SAMPLE_TICKS: u64 = 200;
const RESIDENTS: usize = 12;

fn money(app: &mut App) -> u64 {
    let wallets: u64 = app
        .world_mut()
        .query::<&Wallet>()
        .iter(app.world())
        .map(|wallet| u64::from(wallet.coins))
        .sum();
    let purses: u64 = app
        .world_mut()
        .query::<&CoinPurse>()
        .iter(app.world())
        .map(|purse| u64::from(purse.coins))
        .sum();
    wallets + purses + u64::from(app.world().resource::<VillageState>().fund)
}

fn residents(app: &App) -> Vec<(SimId, Entity)> {
    app.world()
        .resource::<SimIndex>()
        .map
        .iter()
        .filter(|(_, entity)| {
            app.world()
                .get::<Villager>(**entity)
                .is_some_and(|person| person.role != Role::Visitor)
        })
        .map(|(id, entity)| (*id, *entity))
        .collect()
}

fn assert_residents_alive(app: &App, seed: u64) {
    let people = residents(app);
    assert_eq!(
        people.len(),
        RESIDENTS,
        "seed {seed}: resident population changed"
    );
    let failed: Vec<_> = people
        .into_iter()
        .filter_map(|(id, entity)| {
            let person = app.world().get::<Villager>(entity).unwrap();
            let vitality = app.world().get::<Vitality>(entity).unwrap();
            (vitality.health <= 0 || vitality.died_at.is_some()).then_some((
                id,
                person.name.clone(),
                vitality.health,
                vitality.died_at,
            ))
        })
        .collect();
    assert!(
        failed.is_empty(),
        "seed {seed}, tick {}: residents did not survive: {failed:?}",
        app.world().resource::<SimClock>().tick
    );
}

/// Include carried, reserved and dropped goods, so stock dumped on the ground
/// cannot disguise an unbounded unsold backlog as an empty shop inventory.
fn goods(app: &mut App) -> BTreeMap<String, u64> {
    let kinds = catalog::trade_kinds(app.world());
    let mut goods = BTreeMap::new();
    for item in app.world_mut().query::<&Item>().iter(app.world()) {
        if kinds.contains(&item.kind.as_str()) {
            *goods.entry(item.kind.clone()).or_default() += u64::from(item.stack);
        }
    }
    goods
}

fn finished_food_produced(state: &VillageState) -> u64 {
    ["bread", catalog::COOKED_MEAL]
        .into_iter()
        .map(|kind| state.produced.get(kind).copied().unwrap_or(0))
        .sum()
}

#[test]
fn three_seed_living_economies_keep_late_trade_and_food_flow_without_backlog_or_created_money() {
    for seed in [1, 7, 42] {
        let mut app = island_content::new_sim_app(seed, "living_village");
        assert_eq!(app.world().resource::<SimClock>().tick, 0);
        assert_eq!(app.world().resource::<SimConfig>().day_length, DAY_TICKS);
        assert_residents_alive(&app, seed);
        let initial_money = money(&mut app);
        let mut late_start = None;
        let mut food_available_samples = 0_u64;
        let mut unavailable_streak = 0_u64;
        let mut longest_unavailable_streak = 0_u64;
        let mut peak_goods = BTreeMap::<String, u64>::new();
        while app.world().resource::<SimClock>().tick < END {
            run_ticks(app.world_mut(), SAMPLE_TICKS);
            let tick = app.world().resource::<SimClock>().tick;
            assert_eq!(
                money(&mut app),
                initial_money,
                "seed {seed}, tick {tick}: currency was created or lost"
            );
            assert_residents_alive(&app, seed);
            let stock = goods(&mut app);
            for (kind, quantity) in &stock {
                // More than a whole shop's stock per actor (including the idle
                // visitor) is a backlog, not ordinary personal working stock.
                let ceiling = u64::from(catalog::stock_limit(kind)) * (RESIDENTS as u64 + 1);
                assert!(
                    *quantity <= ceiling,
                    "seed {seed}, tick {tick}: unconsumed {kind} backlog {quantity} exceeds {ceiling}"
                );
                let peak = peak_goods.entry(kind.clone()).or_default();
                *peak = (*peak).max(*quantity);
            }
            if tick == LATE_START {
                let state = app.world().resource::<VillageState>();
                late_start = Some((state.transactions, finished_food_produced(state)));
            } else if tick > LATE_START {
                let available: u64 = residents(&app)
                    .into_iter()
                    .map(|(_, actor)| {
                        ["bread", catalog::COOKED_MEAL]
                            .into_iter()
                            .map(|kind| {
                                u64::from(village::economy::count(app.world(), actor, kind))
                            })
                            .sum::<u64>()
                    })
                    .sum();
                if available > 0 {
                    food_available_samples += 1;
                    unavailable_streak = 0;
                } else {
                    unavailable_streak += 1;
                    longest_unavailable_streak = longest_unavailable_streak.max(unavailable_streak);
                }
            }
        }
        let state = app.world().resource::<VillageState>();
        let (transactions_before, food_before) = late_start.expect("late-window checkpoint");
        let late_transactions = state.transactions - transactions_before;
        let late_food = finished_food_produced(state) - food_before;
        println!(
            "LIVING_STABILITY_EVIDENCE {}",
            json!({
                "seed":seed,"tick":END,"late_window":[LATE_START,END],
                "late_transactions":late_transactions,"late_finished_food_produced":late_food,
                "food_available_samples":food_available_samples,"food_sample_interval_ticks":SAMPLE_TICKS,
                "longest_no_finished_food_sample_streak_ticks":longest_unavailable_streak*SAMPLE_TICKS,
                "peak_unconsumed_goods":peak_goods,"currency_total":initial_money,
                "alive_residents":RESIDENTS
            })
        );
        assert!(
            late_transactions > 0,
            "seed {seed}: the last normal day had no physical trade; opening totals cannot satisfy ongoing economic life"
        );
        assert!(
            late_food > 0,
            "seed {seed}: the last normal day produced no finished food despite continuing bodily needs"
        );
        assert!(
            food_available_samples > 0,
            "seed {seed}: no finished food was available at any late-window sample"
        );
        assert_eq!(money(&mut app), initial_money);
    }
}
