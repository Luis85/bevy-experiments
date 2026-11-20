//! The richer traits (review npc-ai#13): curiosity, caution, thrift's stock
//! days and the preferred food, each through an existing decision point, and
//! children who take after their parents.
//!
//! Each behaviour test runs the same world twice from the same seed, with one
//! trait of the subjects changed, so what they do differently is the trait's.
mod life_support;

use bevy_app::App;
use bevy_ecs::prelude::*;
use island_content::village::{
    self, Villager,
    behavior::{LifeDecision, LifeGoal},
    catalog, colony,
    demography::{Demography, Life, LifeStage},
    economy as econ,
    frontier::labor::ResidentProfile,
    production,
    resident::{self, ResidentTraits},
};
use life_support::*;
use npc_framework::interaction::Item;
use npc_framework::kernel::hash::state_hash;
use npc_framework::kernel::journal::set_journal;
use npc_framework::prelude::*;
use npc_framework::snapshot;
use serde_json::{Value, json};
use std::collections::BTreeMap;

const STEP: u64 = 20;

fn set_trait(app: &mut App, id: SimId, edit: impl FnOnce(&mut ResidentTraits)) {
    let actor = entity(app.world(), id);
    edit(
        &mut app
            .world_mut()
            .get_mut::<ResidentTraits>(actor)
            .expect("traits"),
    );
    resident::refresh_schedule(app.world_mut(), actor);
}

fn shipped(seed: u64) -> App {
    island_content::new_sim_app(seed, "living_world")
}

#[test]
fn curiosity_and_caution_are_seeded_per_resident_within_their_ranges() {
    let app = shipped(7);
    let spec = resident::spec(app.world())
        .expect("behaviour")
        .traits
        .clone();
    let traits: Vec<ResidentTraits> = all_residents(app.world())
        .iter()
        .map(|(_, actor)| resident::traits(app.world(), *actor))
        .collect();
    for (pick, range) in [
        (
            (|t: &ResidentTraits| t.curiosity) as fn(&ResidentTraits) -> i32,
            spec.curiosity,
        ),
        (|t: &ResidentTraits| t.caution, spec.caution),
    ] {
        let values: std::collections::BTreeSet<i32> = traits.iter().map(pick).collect();
        assert!(values.len() > 10, "only {} distinct values", values.len());
        assert!(
            values
                .iter()
                .all(|value| (range.min..=range.max).contains(value))
        );
    }
    // A fixed range gives everybody that value.
    let fixed = world_with(|world| {
        world["living_world"]["behavior"] = json!({"traits": {
            "caution": {"min": 90, "max": 90}, "curiosity": {"min": 3, "max": 3}
        }});
    });
    assert!(all_residents(fixed.world()).iter().all(|(_, actor)| {
        let t = resident::traits(fixed.world(), *actor);
        t.caution == 90 && t.curiosity == 3
    }));
}

/// The lowest Thirst of each subject over the run: how far it lets thirst
/// go before it drinks (buying water ahead, on the `Thirst` goal too, happens
/// at any level, so the trough is the measure, not the goal).
fn thirst_troughs(app: &mut App, ids: &[SimId], until: u64) -> BTreeMap<SimId, i32> {
    let mut lowest = BTreeMap::new();
    while app.world().resource::<SimClock>().tick < until {
        run_ticks(app.world_mut(), STEP);
        for id in ids {
            let actor = entity(app.world(), *id);
            let thirst = econ::need(app.world(), actor, "Thirst");
            let low = lowest.entry(*id).or_insert(thirst);
            *low = (*low).min(thirst);
        }
    }
    lowest
}

#[test]
fn a_cautious_resident_drinks_earlier_than_a_carefree_one() {
    let mut carefree = shipped(7);
    let mut cautious = shipped(7);
    let ids: Vec<SimId> = all_residents(carefree.world())
        .iter()
        .map(|(id, _)| *id)
        .take(12)
        .collect();
    for id in &ids {
        set_trait(&mut carefree, *id, |t| t.caution = 0);
        set_trait(&mut cautious, *id, |t| t.caution = 100);
    }
    // The thresholds move by `caution_need_shift` (8000) either way.
    let actor = entity(carefree.world(), ids[0]);
    assert_eq!(
        resident::need_threshold(carefree.world(), actor, 55_000),
        47_000
    );
    assert_eq!(
        resident::need_threshold(cautious.world(), actor, 55_000),
        63_000
    );
    // A day and a half: everybody gets thirsty more than once.
    let low = thirst_troughs(&mut carefree, &ids, 36_000);
    let high = thirst_troughs(&mut cautious, &ids, 36_000);
    println!("CAUTION carefree={low:?} cautious={high:?}");
    let mean = |values: &BTreeMap<SimId, i32>| {
        values.values().map(|value| i64::from(*value)).sum::<i64>() / values.len() as i64
    };
    let (low, high) = (mean(&low), mean(&high));
    println!("CAUTION_MEAN_TROUGH carefree={low} cautious={high}");
    assert!(
        high >= low + 5_000,
        "the cautious let Thirst fall to {high} on average, the carefree to {low}"
    );
}

/// What the plan wants `actor` to hold of water and food.
fn provisions_wanted(world: &World, actor: Entity) -> u32 {
    let mut kinds = production::diet(world, actor);
    kinds.push(catalog::WATER);
    kinds
        .into_iter()
        .map(|kind| production::wanted_stock(world, actor, kind))
        .sum()
}

#[test]
fn a_thrifty_resident_stocks_fewer_days_of_provisions_than_a_spender() {
    let mut spender = shipped(7);
    let mut saver = shipped(7);
    let ids: Vec<SimId> = all_residents(spender.world())
        .iter()
        .map(|(id, _)| *id)
        .take(12)
        .collect();
    for id in &ids {
        set_trait(&mut spender, *id, |t| t.thrift = 0);
        set_trait(&mut saver, *id, |t| t.thrift = 100);
    }
    // Half a day more for thrift 0, half a day less for thrift 100.
    let day = spender.world().resource::<SimConfig>().day_length;
    let actor = entity(spender.world(), ids[0]);
    assert_eq!(
        resident::thrift_stock_shift(spender.world(), actor, day),
        day as i64 / 2
    );
    assert_eq!(
        resident::thrift_stock_shift(saver.world(), actor, day),
        -(day as i64) / 2
    );
    production::refresh_all(spender.world_mut());
    production::refresh_all(saver.world_mut());
    let wanted = |app: &App| -> u32 {
        ids.iter()
            .map(|id| provisions_wanted(app.world(), entity(app.world(), *id)))
            .sum()
    };
    let (more, fewer) = (wanted(&spender), wanted(&saver));
    println!("THRIFT_STOCK spender={more} thrifty={fewer}");
    assert!(
        more * 100 >= fewer * 115,
        "spenders want {more} provisions, the thrifty {fewer}"
    );
}

#[test]
fn a_curious_resident_has_more_curious_days() {
    let mut app = shipped(7);
    let (id, _) = all_residents(app.world())[4];
    let day = app.world().resource::<SimConfig>().day_length;
    let count = |app: &mut App| {
        (0..400_u64)
            .filter(|n| {
                app.world_mut().resource_mut::<SimClock>().tick = n * day + 100;
                colony::curious_today(app.world(), id)
            })
            .count()
    };
    set_trait(&mut app, id, |t| t.curiosity = 0);
    let never = count(&mut app);
    set_trait(&mut app, id, |t| t.curiosity = 50);
    let some = count(&mut app);
    set_trait(&mut app, id, |t| t.curiosity = 100);
    let often = count(&mut app);
    println!("CURIOSITY 0:{never} 50:{some} 100:{often} of 400 days");
    // `schedule.curious_permille` 250, `traits.curious_shift_permille` 250.
    assert_eq!(never, 0);
    assert!((70..=130).contains(&some), "{some} curious days at 50");
    assert!((160..=240).contains(&often), "{often} curious days at 100");
}

#[test]
fn preferences_spread_over_the_foods_and_put_the_liked_food_first() {
    let mut counts: BTreeMap<String, u32> = BTreeMap::new();
    for seed in [7, 11, 23] {
        let app = shipped(seed);
        for (_, actor) in all_residents(app.world()) {
            let food = production::food_preference(app.world(), actor);
            *counts.entry(food.to_owned()).or_default() += 1;
            // The liked food is eaten first.
            assert_eq!(production::diet(app.world(), actor)[0], food);
        }
    }
    println!("FOOD_PREFERENCES {counts:?}");
    for food in [
        catalog::COOKED_MEAL,
        "bread",
        "village_egg",
        "village_turnip",
    ] {
        assert!(
            counts.get(food).copied().unwrap_or(0) >= 10,
            "few residents like {food}: {counts:?}"
        );
    }
}

#[test]
fn a_childs_traits_mix_its_parents_within_the_noise() {
    let spec = resident::TraitSpec::default();
    let a = ResidentTraits {
        diligence: 20,
        sociability: 90,
        chronotype: -100,
        thrift: 0,
        curiosity: 100,
        caution: 10,
    };
    let b = ResidentTraits {
        diligence: 80,
        sociability: 30,
        chronotype: 60,
        thrift: 100,
        curiosity: 40,
        caution: 90,
    };
    let mut seen = std::collections::BTreeSet::new();
    for child in 1..200 {
        let kid = resident::inherit_traits(&spec, 7, child, a, b);
        assert_eq!(kid, resident::inherit_traits(&spec, 7, child, a, b));
        for (value, x, y, range) in [
            (kid.diligence, a.diligence, b.diligence, spec.diligence),
            (
                kid.sociability,
                a.sociability,
                b.sociability,
                spec.sociability,
            ),
            (kid.chronotype, a.chronotype, b.chronotype, spec.chronotype),
            (kid.thrift, a.thrift, b.thrift, spec.thrift),
            (kid.curiosity, a.curiosity, b.curiosity, spec.curiosity),
            (kid.caution, a.caution, b.caution, spec.caution),
        ] {
            let mean = (x + y).div_euclid(2);
            assert!(
                (value - mean).abs() <= spec.inherit_noise,
                "{value} from {x} and {y}"
            );
            assert!((range.min..=range.max).contains(&value));
        }
        seen.insert(kid.caution);
    }
    // The noise is spread, not a constant.
    assert!(seen.len() > 20, "{} distinct inherited values", seen.len());
    // Food: with 1000 permille always a parent's, with 0 always drawn.
    let parents = ["village_egg".to_owned(), "bread".to_owned()];
    let choices = ["village_turnip".to_owned()];
    let always = resident::TraitSpec {
        food_inherit_permille: 1000,
        ..spec.clone()
    };
    let never = resident::TraitSpec {
        food_inherit_permille: 0,
        ..spec
    };
    let picked: std::collections::BTreeSet<String> = (1..50)
        .filter_map(|child| resident::inherit_food(&always, 7, child, &parents, &choices))
        .collect();
    assert_eq!(picked.len(), 2, "both parents' foods are passed on");
    assert!((1..50).all(|child| {
        resident::inherit_food(&never, 7, child, &parents, &choices).as_deref()
            == Some("village_turnip")
    }));
}

fn quick_lives(world: &mut Value) {
    short_days(world, 1200);
    only_weather(world, "clear");
    economy(world)["tuning"]["days_per_season"] = json!(1);
    economy(world)["tuning"]["founding_day"] = json!(0);
    economy(world)["tuning"]["building_levy_percent"] = json!(0);
    economy(world)["merchant"]["enabled"] = json!(false);
    let life = &mut economy(world)["demography"];
    life["courtship_days"] = json!(1);
    life["birth_chance_permille"] = json!(1000);
    life["birth_cooldown_days"] = json!(1_000);
    life["adult_age_seasons"] = json!(2);
    life["birth_food_units"] = json!(2);
    life["founding_couples_percent"] = json!(0);
    world["living_world"]["behavior"] = json!({"traits": {"food_inherit_permille": 1000}});
}

#[test]
fn a_child_born_on_the_island_takes_after_its_parents_and_the_save_keeps_it() {
    let mut app = world_with(quick_lives);
    set_journal(app.world_mut(), true, false);
    let harbour = settlement(app.world(), "harbour");
    let people = residents(app.world(), harbour);
    let ((a_id, a), (b_id, b)) = (people[0], people[1]);
    let today = village::calendar(app.world()).0;
    for (actor, partner, diligence, caution) in [(a, b_id, 90, 95), (b, a_id, 70, 85)] {
        let mut life = app.world_mut().get_mut::<Life>(actor).expect("life");
        life.born_day = today as i64 - 10;
        life.death_day = today + 1_000;
        life.stage = LifeStage::Adult;
        life.partner = Some(partner);
        let mut traits = app
            .world_mut()
            .get_mut::<ResidentTraits>(actor)
            .expect("traits");
        traits.diligence = diligence;
        traits.caution = caution;
        resident::refresh_schedule(app.world_mut(), actor);
        app.world_mut()
            .get_mut::<ResidentProfile>(actor)
            .expect("profile")
            .preferred_food = "village_egg".into();
    }
    let house = colony::home_of(app.world(), a_id).expect("home");
    colony::ColonyAct {
        verb: "assign_home".into(),
        resident: Some(b_id),
        house: Some(house),
        value: 0,
    }
    .perform(app.world_mut())
    .expect("moved in");
    fund(app.world_mut(), 700);
    for actor in [a, b] {
        econ::grant(app.world_mut(), actor, Item::new("bread").stack(4));
    }
    let born = until(&mut app, 3_600, 50, |app| {
        !app.world().resource::<Demography>().children.is_empty()
    });
    assert!(born, "no child was born");
    let child = app.world().resource::<Demography>().children[0].clone();
    let inherited = child.inherited.clone().expect("drawn at birth");
    assert_eq!(inherited.preferred_food, "village_egg");
    let noise = resident::TraitSpec::default().inherit_noise;
    assert!((inherited.traits.diligence - 80).abs() <= noise);
    assert!((inherited.traits.caution - 90).abs() <= noise);

    // A save while the child grows up continues the same way.
    let saved = snapshot::save(app.world_mut()).expect("saved");
    let mut fresh = world_with(quick_lives);
    snapshot::load(fresh.world_mut(), &saved, snapshot::LoadMode::Strict).expect("restores");
    assert_eq!(state_hash(fresh.world_mut()), state_hash(app.world_mut()));
    for app in [&mut app, &mut fresh] {
        let grown = until(app, 2_400, 100, |app| {
            !effects(app.world(), "VillageComingOfAge").is_empty()
                || app.world().resource::<Demography>().came_of_age > 0
        });
        assert!(grown, "the child never came of age");
    }
    assert_eq!(state_hash(fresh.world_mut()), state_hash(app.world_mut()));
    let adult = all_residents(app.world())
        .into_iter()
        .find(|(_, actor)| {
            app.world()
                .get::<Villager>(*actor)
                .is_some_and(|person| person.name == child.name)
        })
        .expect("the grown child")
        .1;
    assert_eq!(resident::traits(app.world(), adult), inherited.traits);
    assert_eq!(
        production::food_preference(app.world(), adult),
        "village_egg"
    );
}

#[test]
fn why_names_the_trait_that_decided() {
    // A cautious resident whose Thirst is between the world's threshold and
    // its own sets out to drink "because they are cautious".
    let mut app = shipped(7);
    let ids: Vec<SimId> = all_residents(app.world())
        .iter()
        .map(|(id, _)| *id)
        .collect();
    for id in &ids {
        set_trait(&mut app, *id, |t| t.caution = 100);
    }
    let mut found = None;
    while found.is_none() && app.world().resource::<SimClock>().tick < 36_000 {
        run_ticks(app.world_mut(), STEP);
        found = ids.iter().copied().find(|id| {
            let actor = entity(app.world(), *id);
            app.world().get::<LifeDecision>(actor).map(|d| d.goal) == Some(LifeGoal::Thirst)
                && (55_000..63_000).contains(&econ::need(app.world(), actor, "Thirst"))
        });
    }
    let id = found.expect("a cautious resident drank before the world's threshold");
    let actor = entity(app.world(), id);
    assert_eq!(
        resident::trait_motive(app.world(), actor),
        Some("they are cautious")
    );
    // The same resident at caution 50 has no trait to blame.
    set_trait(&mut app, id, |t| t.caution = 50);
    assert_eq!(resident::trait_motive(app.world(), actor), None);
    // An unsociable resident's free time is spent alone because of it.
    let mut evening = shipped(7);
    let loners: Vec<SimId> = all_residents(evening.world())
        .iter()
        .map(|(id, _)| *id)
        .collect();
    for id in &loners {
        set_trait(&mut evening, *id, |t| t.sociability = 0);
    }
    let mut alone = None;
    while alone.is_none() && evening.world().resource::<SimClock>().tick < 36_000 {
        run_ticks(evening.world_mut(), STEP);
        alone = loners.iter().copied().find(|id| {
            let actor = entity(evening.world(), *id);
            evening.world().get::<LifeDecision>(actor).map(|d| d.goal) == Some(LifeGoal::Leisure)
        });
    }
    let loner = entity(evening.world(), alone.expect("nobody had free time"));
    assert_eq!(
        resident::trait_motive(evening.world(), loner),
        Some("they keep to themself")
    );
}

#[test]
fn traits_change_the_state_hash_deterministically() {
    let run = |caution: i32| {
        let mut app = shipped(11);
        let (id, _) = all_residents(app.world())[2];
        set_trait(&mut app, id, |t| t.caution = caution);
        run_ticks(app.world_mut(), 2_400);
        state_hash(app.world_mut())
    };
    assert_eq!(run(100), run(100));
    assert_ne!(run(100), run(0));
}
