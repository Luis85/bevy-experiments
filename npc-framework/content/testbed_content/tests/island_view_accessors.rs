//! The read accessors views use (`village::view`) and the stability of the
//! civic fund.
mod life_support;

use bevy_ecs::prelude::*;
use life_support::*;
use npc_framework::kernel::journal::set_journal;
use npc_framework::prelude::*;
use serde_json::json;
use testbed_content::village::{
    self, Season, VillageState, Villager, Wallet, civic,
    resident::{self, BlockKind, ResidentTraits},
    view,
};

#[test]
fn a_resident_at_home_at_night_is_not_labelled_as_going_home() {
    let mut app = world_with(|world| {
        short_days(world, 2_400);
        only_weather(world, "clear");
    });
    // Night falls at 70% of the day.
    run_ticks(app.world_mut(), 1_900);
    let mut at_home = 0;
    let mut labels = std::collections::BTreeSet::new();
    for _ in 0..6 {
        run_ticks(app.world_mut(), 60);
        for (id, actor) in all_residents(app.world()) {
            let Some(label) = view::activity_label(app.world(), actor) else {
                continue;
            };
            labels.insert(label);
            let person = app.world().get::<Villager>(actor).expect("villager");
            let here = *app.world().get::<SimPos>(actor).expect("position");
            let home = here.dist2(person.home) <= 1_500_i64.pow(2);
            if home {
                assert_ne!(label, "going home", "resident {} is home already", id.0);
            }
            if label == "asleep at home" || label == "at home" {
                assert!(home);
                at_home += 1;
            }
        }
    }
    assert!(at_home > 0, "nobody was at home at night: {labels:?}");
    assert!(labels.contains("asleep at home"), "{labels:?}");
    assert_eq!(view::activity_label(app.world(), Entity::PLACEHOLDER), None);
}

#[test]
fn the_latest_decision_is_structured_with_a_does_not_apply_flag() {
    let mut app = world_with(|_| {});
    let (id, actor) = all_residents(app.world())[4];
    assert_eq!(
        view::latest_decision(app.world(), actor),
        None,
        "nothing runs yet"
    );
    // Without the journal the answer is read from the live tree.
    set_journal(app.world_mut(), false, false);
    run_ticks(app.world_mut(), 60);
    let live = view::latest_decision(app.world(), actor).expect("live decision");
    assert!(!live.recorded && live.tick.is_none());
    assert_eq!(live.tree, "island_life");
    assert_eq!(live.branch[0], "Resident life");
    assert!(live.action.starts_with("life."));
    assert!(!live.considered.is_empty());
    assert!(live.considered.iter().all(|option| option.reason.is_none()));
    // With the journal the record of the moment of choice is kept.
    set_journal(app.world_mut(), true, false);
    let mut recorded = None;
    for _ in 0..40 {
        run_ticks(app.world_mut(), 20);
        if let Some(decision) = view::latest_decision(app.world(), actor)
            && decision.recorded
        {
            recorded = Some(decision);
            break;
        }
    }
    let decision = recorded.expect("a recorded decision");
    assert!(decision.tick.is_some());
    assert_eq!(decision.activity, view::activity_label(app.world(), actor));
    let record = effects(app.world(), "VillageDecision")
        .into_iter()
        .rfind(|record| record["node"] == json!(decision.node))
        .expect("the journal has the same decision");
    assert_eq!(record["action"], json!(decision.action));
    assert_eq!(record["branch"], json!(decision.branch.join(" > ")));
    assert_eq!(
        record["considered"].as_array().map(Vec::len),
        Some(decision.considered.len())
    );
    // A false guard is a flag, not a sentence.
    let skipped: Vec<_> = decision
        .considered
        .iter()
        .filter(|option| option.does_not_apply)
        .collect();
    assert!(!skipped.is_empty());
    for option in &skipped {
        assert!(option.condition.is_some() && option.reason.is_none());
    }
    let first = &decision.considered[0];
    // The tree's first option (ADR 0041: freezing comes first).
    assert_eq!(first.node, "Freezing");
    assert_eq!(first.condition.as_deref(), Some("life.freezing"));
    assert!(first.does_not_apply);
    // An option that applied and could not start says why.
    for option in decision
        .considered
        .iter()
        .filter(|option| !option.does_not_apply)
    {
        assert!(
            option
                .reason
                .as_ref()
                .is_some_and(|reason| !reason.is_empty())
        );
    }
    let data = serde_json::to_value(&decision).expect("plain data");
    assert!(data["considered"][0].get("rejected").is_none());
    assert_eq!(data["considered"][0]["does_not_apply"], true);
    let _ = id;
}

#[test]
fn effective_schedule_blocks_follow_chronotype_and_diligence() {
    let mut app = world_with(|_| {});
    let (_, actor) = all_residents(app.world())[0];
    let set = |app: &mut bevy_app::App, chronotype: i32, diligence: i32| {
        *app.world_mut()
            .get_mut::<ResidentTraits>(actor)
            .expect("traits") = ResidentTraits {
            diligence,
            sociability: 50,
            chronotype,
            thrift: 50,
        };
    };
    set(&mut app, 0, 50);
    let plain = view::schedule_blocks(app.world(), actor);
    assert_eq!(plain.len(), 5);
    let work = plain
        .iter()
        .find(|block| block.kind == BlockKind::Work)
        .expect("work");
    assert_eq!((work.from_permille, work.to_permille), (0, 700));
    assert_eq!((work.from_tick, work.to_tick), (0, 16_800));
    assert!(work.active, "dawn is inside the work block");
    let leisure = plain
        .iter()
        .find(|block| block.kind == BlockKind::Leisure)
        .expect("leisure");
    assert_eq!((leisure.from_permille, leisure.to_permille), (640, 700));
    // An owl's whole day is later; a lark's wraps before dawn.
    set(&mut app, 100, 50);
    let owl = view::schedule_blocks(app.world(), actor);
    let work = owl
        .iter()
        .find(|block| block.kind == BlockKind::Work)
        .expect("work");
    assert_eq!((work.from_permille, work.to_permille), (25, 725));
    assert!(!work.active, "an owl is not at work at dawn");
    set(&mut app, -100, 50);
    let lark = view::schedule_blocks(app.world(), actor);
    let sleep = lark
        .iter()
        .find(|block| block.kind == BlockKind::Sleep)
        .expect("sleep");
    assert_eq!((sleep.from_permille, sleep.to_permille), (675, 975));
    // Diligence delays leisure; at 100 the delay swallows half of it, and a
    // longer delay than the block removes it.
    set(&mut app, 0, 100);
    let diligent = view::schedule_blocks(app.world(), actor);
    let leisure = diligent
        .iter()
        .find(|block| block.kind == BlockKind::Leisure)
        .expect("leisure");
    assert_eq!(leisure.from_permille, 680);
    // Every view agrees with the condition the tree reads, all day long.
    set(&mut app, 37, 81);
    for _ in 0..24 {
        run_ticks(app.world_mut(), 1_000);
        for block in view::schedule_blocks(app.world(), actor) {
            assert_eq!(
                block.active,
                resident::in_block(app.world(), actor, block.kind)
            );
            let at = resident::day_permille(app.world());
            let inside = if block.from_permille <= block.to_permille {
                (block.from_permille..block.to_permille).contains(&at)
            } else {
                at >= block.from_permille || at < block.to_permille
            };
            if inside {
                assert!(block.active, "{block:?} at {at}");
            }
        }
    }
    let delayed_away = world_with(|world| {
        world["living_world"]["behavior"] = json!({"schedule": {"diligence_shift": 200}});
    });
    let (_, somebody) = all_residents(delayed_away.world())
        .into_iter()
        .find(|(_, actor)| resident::traits(delayed_away.world(), *actor).diligence >= 70)
        .expect("a diligent founder");
    assert!(
        view::schedule_blocks(delayed_away.world(), somebody)
            .iter()
            .all(|block| block.kind != BlockKind::Leisure),
        "a delay longer than the block removes it"
    );
}

#[test]
fn season_effects_weather_household_boat_and_fields_are_plain_data() {
    let mut app = world_with(|world| only_weather(world, "clear"));
    run_ticks(app.world_mut(), 2);
    // Late autumn, two days before winter.
    let effects = view::season_effects(app.world());
    assert_eq!(
        (effects.day, effects.day_of_season, effects.year),
        (20, 6, 1)
    );
    assert_eq!(effects.season, Season::Autumn);
    assert_eq!(effects.days_per_season, 7);
    assert_eq!(effects.growth_percent, 100);
    assert!(!effects.fallow && !effects.cold && !effects.night);
    assert_eq!(
        (effects.hearth_logs_per_day, effects.hearth_log_ticks),
        (2, 12_000)
    );
    assert_eq!(effects.warmth_loss_percent, 100);
    assert!(!effects.outdoor_work_halted);
    let weather = view::weather(app.world());
    assert_eq!(weather.today.day, 20);
    assert_eq!(weather.forecast.len(), 3);
    assert_eq!(weather.forecast[1].day, 22);
    assert_eq!(weather.forecast[1].season, Season::Winter);
    let boat = view::boat(app.world());
    assert!(boat.enabled && !boat.docked && boat.dock.is_some());
    assert_eq!(boat.next_visit_day, 22);
    assert_eq!(boat.next_visit_tick, 2 * 24_000 + 3_600);
    let fields = view::fields(app.world());
    assert_eq!(fields.len(), 3);
    assert_eq!(fields[0].plots[0].stage, "fallow");
    let (id, _) = all_residents(app.world())[0];
    let home = view::household(app.world(), id).expect("household");
    assert!(home.house.is_some() && home.door.is_some());
    for value in [
        serde_json::to_value(&effects),
        serde_json::to_value(&weather),
        serde_json::to_value(&boat),
        serde_json::to_value(&fields),
        serde_json::to_value(&home),
        serde_json::to_value(view::census(app.world())),
        serde_json::to_value(view::schedule_blocks(app.world(), entity(app.world(), id))),
    ] {
        assert!(value.is_ok());
    }
    // Winter: three logs a day, fallow fields, cold all day.
    let mut winter = world_with(|world| {
        only_weather(world, "clear");
        economy(world)["tuning"]["founding_day"] = json!(21);
    });
    run_ticks(winter.world_mut(), 2);
    let effects = view::season_effects(winter.world());
    assert_eq!(effects.season, Season::Winter);
    assert!(effects.fallow && effects.cold);
    assert_eq!(
        (effects.growth_percent, effects.hearth_logs_per_day),
        (0, 3)
    );
    assert_eq!(effects.day_of_season, 1);
    // Accessors answer neutrally in a world without the regional profile.
    let legacy: bevy_app::App = testbed_content::new_sim_app(SEED, "island_colony");
    assert!(view::fields(legacy.world()).is_empty());
    assert!(!view::boat(legacy.world()).enabled);
    assert!(view::weather(legacy.world()).forecast.is_empty());
    assert_eq!(view::season_effects(legacy.world()).growth_percent, 100);
    assert_eq!(view::census(legacy.world()).residents, 0);
}

#[test]
fn the_civic_reserve_follows_recent_grants_and_the_dividend_returns_only_the_rest() {
    let mut app = world_with(|world| {
        short_days(world, 1_200);
        only_weather(world, "clear");
        economy(world)["merchant"]["enabled"] = json!(false);
        economy(world)["demography"]["enabled"] = json!(false);
    });
    set_journal(app.world_mut(), true, false);
    let tuning = village::frontier::economy_spec::tuning(app.world())
        .expect("tuning")
        .clone();
    assert_eq!(civic::reserve(app.world()), tuning.civic_reserve);
    // Thirty residents lose everything; the fund holds their coins.
    let people = all_residents(app.world());
    let mut moved = 0;
    for (_, actor) in people.iter().take(30) {
        let mut wallet = app.world_mut().get_mut::<Wallet>(*actor).expect("wallet");
        moved += std::mem::take(&mut wallet.coins);
    }
    app.world_mut().resource_mut::<VillageState>().fund += moved;
    let coins = money(app.world());
    run_ticks(app.world_mut(), 1_201);
    let levy = effects(app.world(), "VillageCivicLevy").remove(0);
    let granted = levy["floor_grants"].as_u64().expect("grants") as u32;
    assert!(
        granted >= 30 * 30,
        "the destitute were topped up: {granted}"
    );
    // One day on record: the grants paid above the levy collected. The
    // reserve is three such days.
    let shortfall = granted - levy["contributed"].as_u64().expect("levy") as u32;
    assert_eq!(
        app.world().resource::<civic::CivicState>().grant_history,
        vec![shortfall]
    );
    let reserve = civic::reserve(app.world());
    assert_eq!(reserve, shortfall * tuning.civic_reserve_grant_days);
    assert!(reserve > tuning.civic_reserve);
    assert_eq!(
        levy["reserve"],
        json!(reserve.max(42 * tuning.fund_reserve_per_resident))
    );
    let fund = app.world().resource::<VillageState>().fund;
    assert_eq!(civic::budget(app.world()), fund.saturating_sub(reserve));
    // The dividend left the reserve in the fund.
    assert!(
        fund >= reserve.min(moved - granted),
        "the fund kept {fund} of a reserve of {reserve}"
    );
    assert_eq!(money(app.world()), coins);
    // Quiet days let the reserve fall back to its floor.
    for _ in 0..8 {
        run_ticks(app.world_mut(), 1_200);
    }
    assert_eq!(
        app.world()
            .resource::<civic::CivicState>()
            .grant_history
            .len() as u32,
        tuning.civic_reserve_window_days
    );
    assert!(civic::reserve(app.world()) < reserve);
    assert_eq!(money(app.world()), coins);

    // A project the fund cannot pay yet is held back from the dividend.
    let harbour = settlement(app.world(), "harbour");
    let held = civic::fund_hold(app.world());
    app.world_mut()
        .resource_mut::<civic::CivicState>()
        .planners
        .get_mut(&harbour)
        .expect("planner")
        .pending_cost = 200;
    assert_eq!(
        civic::fund_hold(app.world()),
        held + 200 * (100 + tuning.civic_budget_slack_percent) / 100
    );
}
