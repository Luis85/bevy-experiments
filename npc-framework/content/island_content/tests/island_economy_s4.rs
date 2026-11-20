//! S4 economy round: the bread target read at dawn and at midday, the
//! staple advance for a broke maker, and an overseer's target of 0.
mod life_support;

use island_content::village::{
    self, Role, VillageState, Villager, Wallet, economy as econ,
    frontier::{
        oversee::{self, SetProfessionTarget},
        settlements,
    },
    production, stock,
};
use life_support::*;
use npc_framework::prelude::*;

/// The bread target follows three days of meals, so it reads the same at
/// dawn, after the night's sleep, as at midday. Before the S4 fix the window
/// was 0.3 day: at dawn it held only the night, with no meal in it, and the
/// target swung between a few loaves and dozens.
#[test]
fn the_bread_target_is_stable_between_dawn_and_midday() {
    let mut app = world_with(|world| only_weather(world, "clear"));
    let day = app.world().resource::<SimConfig>().day_length;
    // Dawn of day 3: day one's few meals are too small a sample with mixed food preferences.
    run_ticks(app.world_mut(), 2 * day);
    let read = |app: &bevy_app::App| -> Vec<(u32, u32)> {
        settlements::all(app.world())
            .into_iter()
            .map(|(settlement, _)| {
                (
                    stock::daily_use(app.world(), settlement, "bread"),
                    stock::target(app.world(), settlement, "bread"),
                )
            })
            .collect()
    };
    let dawn = read(&app);
    run_ticks(app.world_mut(), day / 2);
    let midday = read(&app);
    for ((use_dawn, target_dawn), (use_midday, target_midday)) in dawn.iter().zip(&midday) {
        assert!(*use_dawn > 0, "bread is used at dawn: {dawn:?}");
        // Half a day more of meals in a window of a day and a half at most:
        // within a third either way.
        let near = |a: u32, b: u32| a.abs_diff(b) * 3 <= a.max(b);
        assert!(
            near(*use_dawn, *use_midday) && near(*target_dawn, *target_midday),
            "dawn {dawn:?}, midday {midday:?}"
        );
    }
}

/// A maker whose purse cannot pay for a batch's inputs while a staple it
/// supplies is short gets an advance from the fund. Before the S4 fix the
/// gate looked for a staple entry in the production plan, which a maker
/// gets only once it can pay for the inputs, so no advance was ever made.
#[test]
fn a_broke_maker_of_a_short_staple_gets_an_advance() {
    let mut app = world_with(|world| only_weather(world, "clear"));
    run_ticks(app.world_mut(), 2);
    let (id, baker) = all_residents(app.world())
        .into_iter()
        .find(|(_, actor)| {
            app.world()
                .get::<Villager>(*actor)
                .is_some_and(|person| person.role == Role::Baker)
        })
        .expect("a baker");
    let home = production::economic_settlement(app.world(), baker).expect("settlement");
    // No bread on offer in the baker's settlement, nothing in the baker's
    // pack, no coins.
    for (_, actor) in residents(app.world(), home) {
        let kinds: &[&str] = if actor == baker {
            &["bread", "village_flour", "village_water", "log"]
        } else {
            &["bread"]
        };
        for kind in kinds {
            let held = econ::count(app.world(), actor, kind);
            if held > 0 {
                econ::consume(app.world_mut(), actor, kind, held).expect("emptied");
            }
        }
    }
    app.world_mut().get_mut::<Wallet>(baker).unwrap().coins = 0;
    assert!(
        stock::on_offer(app.world(), home, "bread") < stock::target(app.world(), home, "bread"),
        "bread is short"
    );
    // The advance buys inputs that are for sale: the settlement offers what
    // one batch of bread takes (review #9: a recipe whose inputs nobody sells
    // is no batch the maker could buy).
    for (kind, count) in [("village_flour", 2), ("village_water", 1), ("log", 1)] {
        assert!(
            stock::on_offer(app.world(), home, kind) >= count,
            "{kind} is on offer"
        );
    }
    fund(app.world_mut(), 200);
    let money = life_support::money(app.world());
    let fund_before = app.world().resource::<VillageState>().fund;
    production::advance_staple_makers(app.world_mut(), &[(id, baker)]);
    let coins = app.world().get::<Wallet>(baker).unwrap().coins;
    let owed = app
        .world()
        .resource::<production::StapleAdvances>()
        .owed
        .get(&id)
        .copied()
        .unwrap_or(0);
    assert!(coins > 0 && owed == coins, "advanced {coins}, owed {owed}");
    assert_eq!(
        app.world().resource::<VillageState>().fund,
        fund_before - coins
    );
    assert_eq!(life_support::money(app.world()), money, "coins conserved");
    // A maker who can pay gets nothing.
    production::advance_staple_makers(app.world_mut(), &[(id, baker)]);
    let again = app.world().get::<Wallet>(baker).unwrap().coins;
    assert!(again <= 30, "capped by staple_advance_per_maker: {again}");
}

/// A target of 0 empties the trade, the last worker of an essential trade
/// included, and nobody takes it up again. Before the S4 fix the
/// essential-coverage floor kept the last miller of every village, and a
/// worker over its target needed a shortage elsewhere to move at all.
#[test]
fn a_target_of_zero_empties_the_trade() {
    let mut app = world_with(|world| only_weather(world, "clear"));
    let day = app.world().resource::<SimConfig>().day_length;
    run_ticks(app.world_mut(), day / 4);
    let villages: Vec<SimId> = settlements::all(app.world())
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    let millers = |app: &bevy_app::App| -> u32 {
        villages
            .iter()
            .map(|village| oversee::headcount(app.world(), *village, Role::Miller))
            .sum()
    };
    assert!(millers(&app) > 0, "the island starts with millers");
    for village in &villages {
        SetProfessionTarget {
            settlement: *village,
            profession: Role::Miller,
            wanted: Some(0),
        }
        .perform(app.world_mut())
        .expect("target set");
    }
    let emptied = until(&mut app, 3 * day, day / 16, |app| millers(app) == 0);
    assert!(emptied, "{} millers left after three days", millers(&app));
    run_ticks(app.world_mut(), day / 2);
    assert_eq!(millers(&app), 0, "nobody took the trade up again");
    village::integrity::validate_colony(app.world()).expect("valid");
}

/// An advance buys inputs that are for sale: with none of a batch's inputs
/// held or on offer in the settlement, a broke maker of a short staple gets
/// nothing (review #9: the rule took the cheapest recipe at quoted prices,
/// whose inputs nobody might sell).
#[test]
fn no_advance_when_no_batch_inputs_are_on_offer() {
    let mut app = world_with(|world| only_weather(world, "clear"));
    run_ticks(app.world_mut(), 2);
    let (id, baker) = all_residents(app.world())
        .into_iter()
        .find(|(_, actor)| {
            app.world()
                .get::<Villager>(*actor)
                .is_some_and(|person| person.role == Role::Baker)
        })
        .expect("a baker");
    let home = production::economic_settlement(app.world(), baker).expect("settlement");
    for (_, actor) in residents(app.world(), home) {
        for kind in ["bread", "village_flour", "village_raw_meat", "village_fish"] {
            let held = econ::count(app.world(), actor, kind);
            if held > 0 {
                econ::consume(app.world_mut(), actor, kind, held).expect("emptied");
            }
        }
    }
    for kind in ["village_flour", "village_raw_meat", "village_fish"] {
        assert_eq!(stock::on_offer(app.world(), home, kind), 0, "{kind}");
    }
    app.world_mut().get_mut::<Wallet>(baker).unwrap().coins = 0;
    fund(app.world_mut(), 200);
    production::advance_staple_makers(app.world_mut(), &[(id, baker)]);
    assert_eq!(app.world().get::<Wallet>(baker).unwrap().coins, 0);
    assert_eq!(
        app.world()
            .resource::<production::StapleAdvances>()
            .owed
            .get(&id)
            .copied()
            .unwrap_or(0),
        0
    );
}
