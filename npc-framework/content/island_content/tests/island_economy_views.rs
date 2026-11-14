//! Economy views for the interface (review fix S5): price lines, good flow,
//! need alerts, the outlook, the season ledger and the editor switch.
mod life_support;

use island_content::village::{self, Wallet, view};
use life_support::*;
use npc_framework::kernel::commands::ContentCommand;
use npc_framework::prelude::*;

#[test]
fn views_do_not_change_the_hash() {
    let run = |look: bool| {
        let mut app = world_with(|world| only_weather(world, "clear"));
        let mut hashes = Vec::new();
        for _ in 0..30 {
            run_ticks(app.world_mut(), 100);
            if look {
                let world = app.world();
                let _ = view::price_lines(world);
                for (id, _) in village::frontier::settlements::all(world) {
                    let _ = view::outlook(world, id);
                    for good in ["bread", "log", "plank"] {
                        let _ = view::good_flow(world, id, good);
                    }
                }
                for (_, actor) in all_residents(world) {
                    for need in ["Hunger", "Thirst", "Energy", "Warmth"] {
                        let _ = view::need_alert(world, actor, need);
                    }
                }
                for season in view::season_summaries(world) {
                    let _ = view::season_summary(world, season);
                }
                let _ = view::editor_mode(world);
            }
            hashes.push(state_hash(app.world_mut()));
        }
        hashes
    };
    assert_eq!(run(false), run(true));
}

#[test]
fn price_lines_read_the_market_rounds() {
    let mut app = world_with(|world| only_weather(world, "clear"));
    let day = app.world().resource::<SimConfig>().day_length;
    run_ticks(app.world_mut(), day / 2);
    let lines = view::price_lines(app.world());
    assert!(!lines.is_empty());
    let mut keys: Vec<&str> = lines.iter().map(|line| line.good.as_str()).collect();
    let sorted = {
        let mut copy = keys.clone();
        copy.sort_unstable();
        copy
    };
    assert_eq!(keys, sorted, "key order");
    keys.clear();
    let bread = view::price_line(app.world(), "bread").expect("bread is traded");
    assert!(bread.price > 0);
    assert!(bread.history.len() <= 16);
    let last = bread.last_market.clone().expect("a round was held");
    assert_eq!(last.unmet, last.funded_demand.saturating_sub(last.sold));
    assert_eq!(bread.previous.is_some(), bread.history.len() >= 2);
    assert!(view::price_line(app.world(), "no_such_good").is_none());
    // Stable field names for the UI.
    let json = serde_json::to_value(&bread).expect("json");
    for field in [
        "good",
        "display",
        "price",
        "previous",
        "day_ago",
        "change_permille",
        "trend",
        "at_ceiling",
        "last_market",
        "history",
    ] {
        assert!(json.get(field).is_some(), "{field}");
    }
}

#[test]
fn good_flow_names_who_goes_without_and_why() {
    let mut app = world_with(|world| only_weather(world, "clear"));
    let day = app.world().resource::<SimConfig>().day_length;
    run_ticks(app.world_mut(), day / 4);
    let harbour = settlement(app.world(), "harbour");
    // Nobody in the harbour has a coin.
    for (_, actor) in residents(app.world(), harbour) {
        app.world_mut()
            .get_mut::<Wallet>(actor)
            .expect("wallet")
            .coins = 0;
    }
    let world = app.world();
    let mut found = false;
    for good in village::catalog::trade_kinds(world) {
        let flow = view::good_flow(world, harbour, good);
        for line in &flow.going_without {
            assert!(!line.sentence.is_empty());
            if line.cause == view::WithoutCause::NoCoins {
                found = true;
            }
        }
        for line in &flow.held_up {
            assert!(line.missing > 0);
        }
    }
    assert!(found, "a resident without coins goes without something");
}

#[test]
fn outlook_and_need_alerts_answer_for_every_village() {
    let mut app = world_with(|world| only_weather(world, "clear"));
    let day = app.world().resource::<SimConfig>().day_length;
    run_ticks(app.world_mut(), day / 2);
    let world = app.world();
    for (id, _) in village::frontier::settlements::all(world) {
        let outlook = view::outlook(world, id);
        assert!(outlook.population > 0);
        assert!(outlook.food_days.is_some(), "food is eaten");
        if let Some(short) = &outlook.next_shortage {
            assert!(!short.cause_sentence.is_empty());
        }
        let json = serde_json::to_value(&outlook).expect("json");
        for field in [
            "food_days",
            "water_days",
            "fuel_days",
            "beds_free",
            "homeless",
            "idle_hands",
            "open_places",
            "cold_in_days",
            "cold_now",
            "without_coat",
            "next_shortage",
            "day",
        ] {
            assert!(json.get(field).is_some(), "{field}");
        }
    }
    // A need above half has no alert; a low one gives the resident's reason.
    let (_, actor) = all_residents(world)[0];
    let mut app = app;
    let id = app
        .world()
        .resource::<npc_framework::needs::NeedRegistry>()
        .id("Hunger")
        .expect("hunger");
    app.world_mut()
        .get_mut::<npc_framework::needs::Needs>(actor)
        .expect("needs")
        .get_mut(id)
        .value = 10_000;
    let alert = view::need_alert(app.world(), actor, "Hunger").expect("alert");
    assert!(!alert.sentence.is_empty());
    assert_eq!(alert.value_milli, 10_000);
}

#[test]
fn the_season_ledger_opens_seasons_and_adds_up() {
    let mut app = world_with(|world| {
        only_weather(world, "clear");
        short_days(world, 1200);
        economy(world)["tuning"]["days_per_season"] = serde_json::json!(1);
    });
    run_ticks(app.world_mut(), 1200 * 2 + 10);
    let seasons = view::season_summaries(app.world());
    assert_eq!(seasons, vec![0, 1, 2], "one tally per season so far");
    let first = view::season_summary(app.world(), 0).expect("first season");
    let produced: u64 = first.goods.iter().map(|good| good.produced).sum();
    assert!(produced > 0, "the first season made something");
    let used: u64 = first.goods.iter().map(|good| good.eaten_or_used).sum();
    assert!(used > 0, "the first season ate something");
    assert_eq!(
        first.population_end,
        view::season_summary(app.world(), 1)
            .expect("second")
            .population_start
    );
}

#[test]
fn editor_mode_is_a_recorded_switch() {
    let mut app = world_with(|world| only_weather(world, "clear"));
    assert!(!view::editor_mode(app.world()));
    let on = village::frontier::shape::SetEditorMode { on: true };
    assert!(on.validate(app.world()).is_ok());
    on.apply(app.world_mut());
    assert!(view::editor_mode(app.world()));
}
