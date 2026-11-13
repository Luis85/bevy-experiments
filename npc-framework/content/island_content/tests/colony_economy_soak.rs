//! Three unchanged normal days, multiple seeds, and continuing late production.
//! Currency includes wallets, the communal fund and physical dropped purses.
use bevy_app::App;
use bevy_ecs::prelude::*;
use island_content::village::{
    self, CoinPurse, Role, VillageState, Villager, Vitality, Wallet, catalog, economy,
};
use npc_framework::prelude::*;
use serde_json::json;
use std::collections::BTreeMap;

fn residents(app: &App) -> Vec<Entity> {
    app.world()
        .resource::<SimIndex>()
        .map
        .values()
        .copied()
        .filter(|e| {
            app.world()
                .get::<Villager>(*e)
                .is_some_and(|v| v.role != Role::Visitor)
        })
        .collect()
}

/// Extended audit: intentionally excluded from the ordinary iteration gate.
/// Run explicitly with `--ignored --nocapture`; no state or clock tuning occurs.
#[test]
#[ignore = "extended 336000-tick seasonal economy and navigation audit"]
fn extended_colony_survives_winter_and_replacement_without_long_navigation_stalls() {
    const END: u64 = 336_000;
    const SAMPLE: u64 = 120;
    const WINTER_LATE_START: u64 = 144_000;
    const WINTER_END: u64 = 216_000;
    const FINAL_LATE_START: u64 = 264_000;
    let mut app = island_content::new_sim_app(7, "island_colony");
    assert_eq!(app.world().resource::<SimConfig>().day_length, 24_000);
    let initial_money = money(&app);
    let mut critical = BTreeMap::from([
        ("Hunger", 0_u64),
        ("Thirst", 0),
        ("Energy", 0),
        ("Warmth", 0),
    ]);
    let mut minimum = BTreeMap::from([
        ("Hunger", 100_000),
        ("Thirst", 100_000),
        ("Energy", 100_000),
        ("Warmth", 100_000),
    ]);
    let mut max_blocked = (0_u32, String::new(), 0_u64);
    let mut winter_before = BTreeMap::new();
    let mut winter_after = BTreeMap::new();
    let mut final_before = BTreeMap::new();
    let mut final_transactions = 0;
    let mut max_cold_residents_without_coat = 0;
    let mut cold_actor_samples_without_coat = 0_u64;
    let mut minimum_winter_coats = 12;
    let mut day_evidence = Vec::new();
    while app.world().resource::<SimClock>().tick < END {
        run_ticks(app.world_mut(), SAMPLE);
        let tick = app.world().resource::<SimClock>().tick;
        let people = residents(&app);
        assert_eq!(people.len(), 12);
        assert_eq!(money(&app), initial_money, "currency drift at {tick}");
        let mut coated = 0;
        for person in &people {
            let resident = app.world().get::<Villager>(*person).unwrap();
            let vitality = app.world().get::<Vitality>(*person).unwrap();
            assert!(
                vitality.died_at.is_none() && vitality.health > 0,
                "{} died at {tick}",
                resident.name
            );
            for name in ["Hunger", "Thirst", "Energy", "Warmth"] {
                let value = economy::need(app.world(), *person, name);
                *minimum.get_mut(name).unwrap() = minimum[name].min(value);
                if value < 15_000 {
                    *critical.get_mut(name).unwrap() += 1;
                }
            }
            coated += u32::from(village::living::has_coat(app.world(), *person));
            if app.world().get::<MoveIntent>(*person).is_some()
                && let Some(MoveStatus::Blocked { ticks }) = app.world().get::<MoveStatus>(*person)
                && *ticks > max_blocked.0
            {
                max_blocked = (*ticks, resident.name.clone(), tick);
            }
        }
        if village::living::cold(app.world()) {
            max_cold_residents_without_coat = max_cold_residents_without_coat.max(12 - coated);
            cold_actor_samples_without_coat += u64::from(12 - coated);
        }
        if village::calendar(app.world()).1 == village::Season::Winter {
            minimum_winter_coats = minimum_winter_coats.min(coated);
        }
        let state = app.world().resource::<VillageState>();
        match tick {
            WINTER_LATE_START => winter_before = state.produced.clone(),
            WINTER_END => winter_after = state.produced.clone(),
            FINAL_LATE_START => {
                final_before = state.produced.clone();
                final_transactions = state.transactions;
            }
            _ => {}
        }
        if tick.is_multiple_of(24_000) {
            let evidence = json!({"tick":tick,"season":village::calendar(app.world()).1,
                "coated":coated,"transactions":state.transactions,"produced":state.produced,
                "minimum_needs_so_far":minimum,"max_observed_blocked_ticks_so_far":max_blocked.0});
            println!("COLONY_EXTENDED_DAY {evidence}");
            day_evidence.push(evidence);
        }
    }
    let state = app.world().resource::<VillageState>();
    let delta =
        |before: &BTreeMap<String, u64>, after: &BTreeMap<String, u64>| -> BTreeMap<String, u64> {
            [
                catalog::WARM_CLOTHES,
                catalog::HIDE,
                catalog::LEATHER,
                catalog::SPEAR,
                catalog::THREAD,
                "bread",
                catalog::COOKED_MEAL,
            ]
            .into_iter()
            .map(|kind| {
                (
                    kind.into(),
                    after
                        .get(kind)
                        .copied()
                        .unwrap_or(0)
                        .saturating_sub(before.get(kind).copied().unwrap_or(0)),
                )
            })
            .collect()
        };
    let winter_flow = delta(&winter_before, &winter_after);
    let final_flow = delta(&final_before, &state.produced);
    println!(
        "COLONY_EXTENDED_EVIDENCE {}",
        json!({"seed":7,"ticks":END,"resident_count":12,
        "currency":initial_money,"sample_interval_ticks":SAMPLE,"minimum_needs":minimum,
        "critical_actor_samples":critical,"maximum_observed_consecutive_blocked":{
            "ticks":max_blocked.0,"resident":max_blocked.1,"observed_at":max_blocked.2},
        "winter_window":[WINTER_LATE_START,WINTER_END],"winter_production":winter_flow,
        "final_window":[FINAL_LATE_START,END],"final_production":final_flow,
        "final_transactions":state.transactions-final_transactions,"total_produced":state.produced,
        "minimum_coats_equipped_in_winter":minimum_winter_coats,
        "maximum_cold_residents_without_coat":max_cold_residents_without_coat,
        "cold_actor_samples_without_coat":cold_actor_samples_without_coat,"days":day_evidence})
    );
    assert!(
        state.transactions > final_transactions,
        "physical trading ceased in the final three days"
    );
    assert!(
        final_flow["bread"] + final_flow[catalog::COOKED_MEAL] > 0,
        "finished food stopped after winter"
    );
    for kind in [
        catalog::WARM_CLOTHES,
        catalog::HIDE,
        catalog::LEATHER,
        catalog::SPEAR,
    ] {
        assert!(
            winter_flow[kind] > 0,
            "late-winter {kind} chain did not replenish"
        );
    }
    assert!(
        max_blocked.0 < 1200,
        "long blocked route {:?}; corner recovery must preserve economic circulation",
        max_blocked
    );
}
fn money(app: &App) -> u64 {
    app.world()
        .resource::<SimIndex>()
        .map
        .values()
        .map(|e| {
            u64::from(app.world().get::<Wallet>(*e).map_or(0, |w| w.coins))
                + u64::from(app.world().get::<CoinPurse>(*e).map_or(0, |p| p.coins))
        })
        .sum::<u64>()
        + u64::from(app.world().resource::<VillageState>().fund)
}
fn food(state: &VillageState) -> u64 {
    ["bread", catalog::COOKED_MEAL]
        .iter()
        .map(|kind| state.produced.get(*kind).copied().unwrap_or(0))
        .sum()
}

#[test]
fn configured_islands_keep_three_day_production_trade_and_residents_alive() {
    for seed in [7, 42] {
        let mut app = island_content::new_sim_app(seed, "island_colony");
        let initial_money = money(&app);
        let day = app.world().resource::<SimConfig>().day_length;
        let mut late = None;
        let mut low_hunger_samples = 0_u32;
        let mut low_thirst_samples = 0_u32;
        let mut longest_food_gap = 0_u64;
        let mut food_gap = 0_u64;
        while app.world().resource::<SimClock>().tick < day * 3 {
            run_ticks(app.world_mut(), 300);
            let tick = app.world().resource::<SimClock>().tick;
            let people = residents(&app);
            assert_eq!(people.len(), 12);
            assert_eq!(
                money(&app),
                initial_money,
                "currency drift seed {seed} tick {tick}"
            );
            for person in &people {
                let vitality = app.world().get::<Vitality>(*person).unwrap();
                assert!(
                    vitality.died_at.is_none() && vitality.health > 0,
                    "resident {:?} died seed {seed} tick {tick}",
                    app.world().get::<Villager>(*person).unwrap().name
                );
                low_hunger_samples +=
                    u32::from(economy::need(app.world(), *person, "Hunger") < 15_000);
                low_thirst_samples +=
                    u32::from(economy::need(app.world(), *person, "Thirst") < 15_000);
            }
            let state = app.world().resource::<VillageState>();
            if tick == day * 2 {
                late = Some((state.transactions, food(state)));
            }
            if tick > day * 2 {
                let stocked = people.iter().any(|person| {
                    ["bread", catalog::COOKED_MEAL]
                        .iter()
                        .any(|kind| economy::count(app.world(), *person, kind) > 0)
                });
                food_gap = if stocked { 0 } else { food_gap + 300 };
                longest_food_gap = longest_food_gap.max(food_gap);
            }
        }
        let state = app.world().resource::<VillageState>();
        let (transactions, meals) = late.unwrap();
        assert!(
            state.transactions > transactions,
            "late trade stopped seed {seed}"
        );
        assert!(
            food(state) > meals,
            "late food production stopped seed {seed}"
        );
        assert!(
            longest_food_gap < day,
            "no finished food throughout last day seed {seed}"
        );
        for kind in [catalog::FLOUR, "bread", "plank", "ingot", catalog::WATER] {
            assert!(
                state.produced.get(kind).copied().unwrap_or(0) > 0,
                "chain {kind} never operated seed {seed}"
            );
        }
        println!(
            "COLONY_SOAK_EVIDENCE {}",
            json!({"seed":seed,"ticks":day*3,
            "residents_alive":12,"currency":initial_money,"late_transactions":state.transactions-transactions,
            "late_finished_food":food(state)-meals,"low_hunger_actor_samples":low_hunger_samples,
            "low_thirst_actor_samples":low_thirst_samples,"sample_interval":300,"longest_late_food_gap_ticks":longest_food_gap,
            "produced":state.produced,"colony":village::colony::enabled(app.world())})
        );
    }
}
