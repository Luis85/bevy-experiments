//! Deterministic daily weather and rare seasonal shocks of the coastal world.
mod life_support;

use bevy_ecs::prelude::*;
use life_support::*;
use npc_framework::kernel::journal::set_journal;
use npc_framework::prelude::*;
use serde_json::json;
use testbed_content::village::{
    self, ResourcePatch, Season, catalog, economy as econ, fields,
    frontier::ecology::EcologicalPatch,
    frontier::life_spec::{Shock, Weather, WeatherSpec},
    living, view, weather,
};

#[test]
fn weather_is_a_function_of_the_seed_and_the_day_with_seasonal_odds() {
    let spec = WeatherSpec::default();
    let run = |seed: u64| -> Vec<Weather> {
        (0..560)
            .map(|day| weather::weather_of(&spec, seed, day, 7, 2))
            .collect()
    };
    assert_eq!(run(7), run(7), "the same seed gives the same weather");
    assert_ne!(run(7), run(11), "another seed gives other weather");
    let days = run(7);
    let count = |season: u64, kind: Weather| {
        days.iter()
            .enumerate()
            .filter(|(day, weather)| (*day as u64 / 7) % 4 == season && **weather == kind)
            .count()
    };
    // 140 days of each season: the odds of the data show.
    assert!(count(1, Weather::Snow) == 0, "it never snows in summer");
    assert!(count(3, Weather::Snow) > count(0, Weather::Snow) * 4);
    assert!(count(1, Weather::Clear) > 70);
    assert!(count(0, Weather::Rain) > count(3, Weather::Rain));
    for kind in [Weather::Clear, Weather::Rain, Weather::Storm, Weather::Snow] {
        assert!(
            days.contains(&kind),
            "{kind:?} never happened in twenty years"
        );
    }
    // Shocks are rare, seeded and never fall into the grace after the founding.
    let shocks: Vec<_> = (0..400)
        .filter_map(|season| weather::shock_of(&spec, 7, season, 2).map(|shock| (season, shock)))
        .collect();
    assert!((10..=80).contains(&shocks.len()), "{} shocks", shocks.len());
    assert!(shocks.iter().all(|(season, _)| *season >= 5));
    for (season, shock) in &shocks {
        match shock {
            Shock::Drought => assert_eq!(season % 4, 1),
            Shock::Blight => assert!(season % 4 <= 1),
            Shock::HarshWinter => assert_eq!(season % 4, 3),
        }
    }
    for wanted in [Shock::Drought, Shock::Blight, Shock::HarshWinter] {
        assert!(shocks.iter().any(|(_, shock)| *shock == wanted));
    }
}

#[test]
fn the_forecast_is_exact_and_every_change_is_journaled_once() {
    let mut app = world_with(|world| short_days(world, 1200));
    set_journal(app.world_mut(), true, false);
    run_ticks(app.world_mut(), 1);
    let first = effects(app.world(), "VillageWeather");
    assert_eq!(first.len(), 1, "the first day's weather is journaled");
    let now = view::weather(app.world());
    assert_eq!(first[0]["weather"], now.today.weather.name());
    assert_eq!(now.forecast.len(), 3);
    assert_eq!(now.forecast[0].day, now.today.day + 1);
    let mut yesterday = now.today.weather;
    for expected in &now.forecast {
        run_ticks(app.world_mut(), 1200);
        let today = view::weather(app.world()).today;
        assert_eq!(today.day, expected.day);
        assert_eq!(today.weather, expected.weather, "day {}", today.day);
        // One record per change of weather, none while it stays. The journal
        // is a ring buffer, so each day is checked while its record is there.
        let records: Vec<_> = effects(app.world(), "VillageWeather")
            .into_iter()
            .filter(|record| record["day"] == json!(today.day))
            .collect();
        if today.weather == yesterday {
            assert!(records.is_empty(), "day {}: nothing changed", today.day);
        } else {
            assert_eq!(records.len(), 1, "day {}", today.day);
            assert_eq!(records[0]["weather"], today.weather.name());
            assert_eq!(records[0]["previous"], yesterday.name());
        }
        yesterday = today.weather;
    }
    // The same seed gives the same hash; the weather state is part of it.
    let mut again = world_with(|world| short_days(world, 1200));
    run_ticks(again.world_mut(), 3601);
    assert_eq!(
        state_hash(app.world_mut()),
        state_hash(again.world_mut()),
        "the journal must not change the simulation"
    );
}

fn coldest(world: &World) -> i32 {
    all_residents(world)
        .into_iter()
        .filter(|(_, actor)| !living::has_coat(world, *actor))
        .map(|(_, actor)| econ::need(world, actor, "Warmth"))
        .min()
        .expect("somebody has no coat")
}

#[test]
fn rain_speeds_growth_and_refills_water_and_snow_chills() {
    let spring = |world: &mut serde_json::Value, kind: &str| {
        only_weather(world, kind);
        economy(world)["tuning"]["founding_day"] = json!(0);
    };
    let mut clear = world_with(|world| spring(world, "clear"));
    let mut rain = world_with(|world| spring(world, "rain"));
    let mut snow = world_with(|world| spring(world, "snow"));
    for app in [&mut clear, &mut rain, &mut snow] {
        // Draw the wells dry, in the ecological ledger too.
        let mut wells = app
            .world_mut()
            .query::<(&mut ResourcePatch, &mut EcologicalPatch)>();
        for (mut patch, mut ecology) in wells.iter_mut(app.world_mut()) {
            if patch.kind == catalog::WATER {
                ecology.harvested_units += u64::from(patch.remaining);
                patch.remaining = 0;
            }
        }
        run_ticks(app.world_mut(), 601);
    }
    let water = |app: &mut bevy_app::App| -> u32 {
        let mut wells = app.world_mut().query::<&ResourcePatch>();
        wells
            .iter(app.world())
            .filter(|patch| patch.kind == catalog::WATER)
            .map(|patch| patch.remaining)
            .sum()
    };
    assert_eq!(view::season_effects(clear.world()).growth_percent, 100);
    assert_eq!(view::season_effects(rain.world()).growth_percent, 150);
    assert_eq!(view::season_effects(snow.world()).growth_percent, 0);
    village::integrity::validate_colony(rain.world()).expect("rain keeps the ecology ledger");
    assert!(
        water(&mut rain) > water(&mut clear),
        "rain refills the wells: {} against {}",
        water(&mut rain),
        water(&mut clear)
    );
    // A spring day is mild; under snow it is cold in the open at noon.
    assert!(!view::season_effects(clear.world()).cold);
    let effects = view::season_effects(snow.world());
    assert!(effects.cold && effects.warmth_loss_percent == 150);
    assert!(living::cold(snow.world()));
    assert!(
        coldest(snow.world()) < coldest(clear.world()),
        "snow costs warmth: {} against {}",
        coldest(snow.world()),
        coldest(clear.world())
    );
}

#[test]
fn a_storm_halts_outdoor_work_and_residents_shelter_or_craft() {
    let mut app = world_with(|world| only_weather(world, "storm"));
    set_journal(app.world_mut(), true, false);
    let outdoor = [
        "gathering",
        "working to a plan",
        "hunting",
        "building",
        "carrying building materials",
        "storing surplus goods",
        "restocking the stall",
        "sowing the field",
        "tending the field",
        "harvesting the field",
        "buying work materials",
    ];
    let mut sheltering = 0;
    for _ in 0..8 {
        run_ticks(app.world_mut(), 150);
        assert!(view::season_effects(app.world()).outdoor_work_halted);
        for (id, actor) in all_residents(app.world()) {
            let label = view::activity_label(app.world(), actor).unwrap_or("");
            assert!(
                !outdoor.contains(&label),
                "resident {} is {label} in a storm",
                id.0
            );
            if matches!(label, "going home" | "at home" | "asleep at home") {
                sheltering += 1;
            }
        }
    }
    assert!(sheltering > 0, "nobody sheltered from the storm");
    let branches: Vec<String> = effects(app.world(), "VillageDecision")
        .iter()
        .filter_map(|record| record["branch"].as_str().map(str::to_owned))
        .collect();
    assert!(
        branches.iter().any(|branch| branch.contains("Storm")),
        "no decision record names the storm branch"
    );
    assert!(
        !branches.iter().any(|branch| branch.contains("Work hours")),
        "somebody reached the work branch in a storm"
    );
}

#[test]
fn seasonal_shocks_change_growth_warmth_fuel_and_fields() {
    let always = |world: &mut serde_json::Value, founding_day: i64| {
        short_days(world, 1200);
        let tuning = &mut economy(world)["tuning"];
        tuning["days_per_season"] = json!(1);
        tuning["founding_day"] = json!(founding_day);
        let weather = &mut economy(world)["weather"];
        weather["shock_grace_seasons"] = json!(0);
        weather["drought_permille"] = json!(500);
        weather["blight_permille"] = json!(500);
        weather["harsh_winter_permille"] = json!(1000);
    };
    // A summer drought: no rain, and what grows grows at half speed.
    let mut summer = world_with(|world| {
        always(world, 1);
        let weather = &mut economy(world)["weather"];
        weather["drought_permille"] = json!(1000);
        weather["blight_permille"] = json!(0);
        weather["summer"] = json!({"clear": 0, "rain": 1, "storm": 0, "snow": 0});
    });
    set_journal(summer.world_mut(), true, false);
    run_ticks(summer.world_mut(), 2);
    let effects_now = view::season_effects(summer.world());
    assert_eq!(effects_now.season, Season::Summer);
    assert_eq!(effects_now.shock, Some(Shock::Drought));
    assert_eq!(effects_now.weather, Weather::Clear, "a drought has no rain");
    assert_eq!(effects_now.growth_percent, 50);
    assert_eq!(effects(summer.world(), "VillageSeasonShock").len(), 1);

    // A harsh winter: more warmth lost and one more hearth log a day.
    let mut winter = world_with(|world| {
        always(world, 3);
        economy(world)["weather"]["winter"] = json!({"clear": 1, "rain": 0, "storm": 0, "snow": 0});
    });
    let mut mild = world_with(|world| {
        always(world, 3);
        economy(world)["weather"]["harsh_winter_permille"] = json!(0);
        economy(world)["weather"]["winter"] = json!({"clear": 1, "rain": 0, "storm": 0, "snow": 0});
    });
    run_ticks(winter.world_mut(), 2);
    run_ticks(mild.world_mut(), 2);
    let (harsh, plain) = (
        view::season_effects(winter.world()),
        view::season_effects(mild.world()),
    );
    assert_eq!(harsh.shock, Some(Shock::HarshWinter));
    assert_eq!(plain.shock, None);
    assert!(harsh.warmth_loss_percent >= 150 && plain.warmth_loss_percent == 100);
    assert_eq!(plain.hearth_logs_per_day, 3);
    assert_eq!(harsh.hearth_logs_per_day, 4);
    assert!(harsh.fallow && harsh.growth_percent == 0);

    // A blight at the turn of the season destroys part of what grows.
    let mut blight = world_with(|world| {
        always(world, 0);
        let weather = &mut economy(world)["weather"];
        weather["drought_permille"] = json!(0);
        weather["blight_permille"] = json!(1000);
        // Founded on the first day of spring without one: the test wants the
        // blight of the summer that follows.
        weather["shock_grace_seasons"] = json!(1);
    });
    set_journal(blight.world_mut(), true, false);
    run_ticks(blight.world_mut(), 2);
    assert_eq!(view::season_effects(blight.world()).shock, None);
    let ids: Vec<_> = view::fields(blight.world())
        .into_iter()
        .map(|field| field.id)
        .collect();
    assert_eq!(ids.len(), 3);
    for id in &ids {
        let field = entity(blight.world(), *id);
        let mut state = blight
            .world_mut()
            .get_mut::<fields::Field>(field)
            .expect("field");
        for plot in &mut state.plots {
            plot.stage = fields::PlotStage::Growing;
            plot.growth = 10;
        }
    }
    run_ticks(blight.world_mut(), 1200);
    assert_eq!(
        view::season_effects(blight.world()).shock,
        Some(Shock::Blight)
    );
    let lost: u64 = effects(blight.world(), "VillageFieldFallow")
        .iter()
        .filter(|record| record["reason"] == "blight")
        .map(|record| record["lost_plots"].as_u64().unwrap_or(0))
        .sum();
    assert!(
        (4..=18).contains(&lost),
        "a blight takes about four plots in ten of 24: {lost}"
    );
    let fallow: usize = view::fields(blight.world())
        .iter()
        .flat_map(|field| field.plots.iter())
        .filter(|plot| plot.stage == "fallow")
        .count();
    assert!(fallow as u64 >= lost);
    village::integrity::validate_colony(blight.world()).expect("valid after a blight");
}

/// Makers work to order, so supply never exceeds demand by itself: a price
/// that rose in a storm had nothing to bring it down. A gatherer whose good is
/// dear keeps a few units beyond its orders. The control is the same world
/// with the shelf switched off: there the well keepers are asked for exactly
/// what is ordered. Without the rule both worlds plan the same and this fails.
#[test]
fn a_gatherer_of_a_dear_good_is_asked_for_a_shelf_beyond_its_orders() {
    use testbed_content::village::{Role, Villager, market, production};
    let planned = |shelf: u32, dear: bool| -> (u32, usize) {
        let mut app = world_with(|world| {
            only_weather(world, "clear");
            economy(world)["tuning"]["gatherer_shelf_units"] = json!(shelf);
        });
        run_ticks(app.world_mut(), 2);
        let keepers: Vec<Entity> = all_residents(app.world())
            .into_iter()
            .map(|(_, actor)| actor)
            .filter(|actor| {
                app.world()
                    .get::<Villager>(*actor)
                    .is_some_and(|person| person.role == Role::WaterKeeper)
            })
            .collect();
        assert!(!keepers.is_empty());
        for keeper in &keepers {
            let held = econ::count(app.world(), *keeper, catalog::WATER);
            if held > 0 {
                econ::consume(app.world_mut(), *keeper, catalog::WATER, held).expect("emptied");
            }
        }
        if dear {
            let base = catalog::price_in(app.world(), catalog::WATER).expect("base price");
            app.world_mut()
                .resource_mut::<market::MarketState>()
                .commodities
                .get_mut(catalog::WATER)
                .expect("water is quoted")
                .reference_milli = base * 3_000;
        }
        // To the next plan, read the moment it is made.
        let made = app
            .world()
            .resource::<production::ProductionPlan>()
            .refreshed_at;
        let fresh = until(&mut app, 400, 1, |app| {
            app.world()
                .resource::<production::ProductionPlan>()
                .refreshed_at
                != made
        });
        assert!(fresh, "no new plan");
        (
            keepers
                .iter()
                .map(|keeper| production::production_demand(app.world(), *keeper, catalog::WATER))
                .sum(),
            keepers.len(),
        )
    };
    let (with_shelf, keepers) = planned(4, true);
    let (without, _) = planned(0, true);
    let (cheap, _) = planned(4, false);
    println!(
        "SHELF water asked of {keepers} keepers: dear with shelf {with_shelf}, dear without \
         {without}, at base price {cheap}"
    );
    assert!(
        with_shelf > without,
        "a dear good: {with_shelf} units asked with the shelf, {without} without"
    );
    assert!(
        with_shelf <= without + 4 * keepers as u32,
        "never more than the shelf"
    );
}

/// The outcome over days, with the control. Explicit; run in release:
/// cargo test --locked --release -p testbed_content --test island_weather -- --ignored --nocapture
#[test]
#[ignore = "explicit: two worlds for four days each; run in release"]
fn a_dear_staple_returns_to_its_base_price_with_the_shelf_and_not_without() {
    use testbed_content::village::market;
    // Two days in, when the founding stocks are gone and makers work to
    // order, water is made dear; one more day decides.
    let dear_water = |shelf: u32| -> (Vec<u32>, u32) {
        let mut app = world_with(|world| {
            only_weather(world, "clear");
            economy(world)["tuning"]["gatherer_shelf_units"] = json!(shelf);
        });
        run_ticks(app.world_mut(), 48_000);
        let base = catalog::price_in(app.world(), catalog::WATER).expect("base price");
        app.world_mut()
            .resource_mut::<market::MarketState>()
            .commodities
            .get_mut(catalog::WATER)
            .expect("water is quoted")
            .reference_milli = base * 5_000;
        let mut series = Vec::new();
        for _ in 0..16 {
            run_ticks(app.world_mut(), 3_000);
            series.push(market::unit_price(app.world(), catalog::WATER).expect("price"));
        }
        (series, base)
    };
    let (with_shelf, base) = dear_water(4);
    let (without, _) = dear_water(0);
    println!("SHELF_PROOF base={base} with_shelf={with_shelf:?} without={without:?}");
    let dear = |series: &[u32]| series.iter().filter(|price| **price > base * 2).count();
    assert!(
        dear(&with_shelf) < dear(&without),
        "the shelf did not shorten the time water stays dear"
    );
}
