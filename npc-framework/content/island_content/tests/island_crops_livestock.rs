//! The second field crop (turnips) and the hen house (economy review #9):
//! turnip fields are founded beside the grain fields and grown in their own
//! season; the hens of each hen house are fed by the hen keeper's recipes
//! (grain or turnips in, eggs out), lay as many eggs as they are fed, starve
//! down to their floor without a keeper and breed back when fed; both foods
//! are eaten through the diet; a save mid-season continues with the same
//! hash; and the run is deterministic.
mod life_support;

use bevy_ecs::prelude::*;
use island_content::village::{
    self, Role, Season, VillageState, catalog, economy as econ,
    frontier::{economy_spec::EconomySpec, oversee::SetProfessionTarget, settlements},
    integrity, livestock, production, view,
};
use life_support::*;
use npc_framework::kernel::hash::state_hash;
use npc_framework::kernel::journal::set_journal;
use npc_framework::prelude::*;
use npc_framework::snapshot;
use serde_json::{Value, json};

const DAY: u64 = 2_400;

fn herder() -> Role {
    Role::from_key(catalog::HERDER)
}

/// Spring on the first day, clear skies, short days, and turnips that ripen
/// within a test.
fn quick_turnips(world: &mut Value) {
    short_days(world, DAY);
    economy(world)["tuning"]["founding_day"] = json!(0);
    only_weather(world, "clear");
    let mut crops = serde_json::to_value(EconomySpec::default().crops).expect("crops");
    let turnip = crops
        .as_array_mut()
        .expect("crops")
        .iter_mut()
        .find(|crop| crop["key"] == "turnip")
        .expect("the turnip crop");
    turnip["growth_ticks"] = json!(1_500);
    turnip["tend_interval_ticks"] = json!(600);
    for field in ["sow_ticks", "tend_ticks", "harvest_ticks"] {
        turnip[field] = json!(40);
    }
    economy(world)["crops"] = crops;
}

fn produced(world: &World, kind: &str) -> u64 {
    world
        .resource::<VillageState>()
        .produced
        .get(kind)
        .copied()
        .unwrap_or(0)
}

fn feedings(world: &World) -> u64 {
    livestock::views(world)
        .iter()
        .map(|pen| {
            let entity = entity(world, pen.id);
            world
                .get::<livestock::Livestock>(entity)
                .map_or(0, |herd| herd.feedings)
        })
        .sum()
}

#[test]
fn every_settlement_is_founded_with_a_turnip_field_and_a_hen_house() {
    let mut app = world_with(|_| {});
    run_ticks(app.world_mut(), 2);
    let fields = view::fields(app.world());
    let turnips: Vec<_> = fields
        .iter()
        .filter(|field| field.crop == "turnip")
        .collect();
    assert_eq!(turnips.len(), 3, "one turnip field per settlement");
    for field in &turnips {
        assert_eq!(field.plots.len(), 6);
        assert_eq!(field.seed_stock, 6, "seed for every plot");
        let store = entity(app.world(), field.id);
        let only = &app
            .world()
            .get::<village::civic::Depot>(store)
            .expect("a granary")
            .only;
        assert_eq!(
            only,
            &vec![catalog::TURNIP.to_owned()],
            "its granary keeps turnips"
        );
    }
    assert_eq!(
        fields.iter().filter(|field| field.crop == "grain").count(),
        3,
        "the grain fields are still there"
    );
    let pens = livestock::views(app.world());
    assert_eq!(pens.len(), 3, "one hen house per settlement");
    let villages: std::collections::BTreeSet<SimId> =
        pens.iter().map(|pen| pen.settlement).collect();
    assert_eq!(villages.len(), 3);
    for pen in &pens {
        assert_eq!(
            (pen.animals.as_str(), pen.head, pen.max_head),
            ("hens", 4, 8)
        );
    }
    integrity::validate_colony(app.world()).expect("valid with turnip fields and pens");
}

/// A world founded two days before winter (the shipped founding) sows no
/// turnips: a plot is only sown with its growing time left before winter.
/// In spring the farmers sow, tend and harvest them.
#[test]
fn turnips_grow_in_their_season() {
    let mut autumn = world_with(|world| {
        short_days(world, DAY);
        only_weather(world, "clear");
    });
    set_journal(autumn.world_mut(), true, false);
    run_ticks(autumn.world_mut(), 2 * DAY - 10);
    assert_ne!(village::calendar(autumn.world()).1, Season::Winter);
    assert!(
        effects(autumn.world(), "VillageFieldSown")
            .iter()
            .all(|sown| sown["crop"] != "turnip"),
        "no turnip is sown so late in autumn"
    );

    let mut app = world_with(quick_turnips);
    set_journal(app.world_mut(), true, false);
    run_ticks(app.world_mut(), 2);
    let mut sown = false;
    let mut harvested = Vec::new();
    let done = until(&mut app, 6 * DAY, 100, |app| {
        sown |= effects(app.world(), "VillageFieldSown")
            .iter()
            .any(|effect| effect["crop"] == "turnip");
        harvested.extend(
            effects(app.world(), "VillageFieldHarvested")
                .into_iter()
                .filter(|effect| effect["crop"] == "turnip"),
        );
        sown && !harvested.is_empty()
    });
    assert!(done, "turnips sown {sown}, harvests {harvested:?}");
    assert_eq!(harvested[0]["kind"], catalog::TURNIP);
    assert_eq!(harvested[0]["quantity"], 5, "six a plot, one kept as seed");
    assert!(produced(app.world(), catalog::TURNIP) >= 6);
    assert_eq!(village::calendar(app.world()).1, Season::Spring);
}

/// A hen keeper takes the empty essential seat; every finished batch feeds
/// two hens and yields two eggs, so the eggs laid are exactly the hens fed;
/// a batch never starts beyond the hens not yet fed today.
#[test]
fn hens_lay_as_many_eggs_as_they_are_fed() {
    let mut app = world_with(|world| {
        short_days(world, DAY);
        only_weather(world, "clear");
    });
    // Staffing the seat and buying feed take the first two short days; at
    // the 2,400-tick day a batch cannot start in the last part of the work
    // hours either (`resident::fits_before_rest`).
    let kept = until(&mut app, 8 * DAY, 100, |app| {
        produced(app.world(), catalog::EGG) >= 8
    });
    let keepers = all_residents(app.world())
        .into_iter()
        .filter(|(_, actor)| {
            app.world()
                .get::<village::Villager>(*actor)
                .is_some_and(|person| person.role == herder())
        })
        .count();
    assert!(
        kept,
        "{keepers} hen keepers; eggs {}",
        produced(app.world(), catalog::EGG)
    );
    assert!(keepers > 0, "somebody keeps the hens");
    // Eggs come only from feeding: two eggs for two hens fed.
    assert_eq!(produced(app.world(), catalog::EGG), feedings(app.world()));
    for pen in livestock::views(app.world()) {
        assert!(pen.fed_today <= pen.head, "{pen:?}");
    }
}

/// With nobody keeping hens the herds go hungry: one hen is lost every
/// `starve_days` hungry days, down to the floor and no further; a herd fed
/// in full breeds back.
#[test]
fn unfed_hens_starve_to_their_floor_and_fed_hens_breed() {
    let mut app = world_with(|world| {
        short_days(world, DAY);
        only_weather(world, "clear");
        economy(world)["tuning"]["founding_day"] = json!(0);
        for building in economy(world)["buildings"]
            .as_array_mut()
            .expect("buildings")
        {
            if building["id"] == "hen_house" {
                building["livestock"]["starve_days"] = json!(1);
                building["livestock"]["breed_days"] = json!(1);
            }
        }
    });
    run_ticks(app.world_mut(), 2);
    for (village, _) in settlements::all(app.world()) {
        SetProfessionTarget {
            settlement: village,
            profession: herder(),
            wanted: Some(0),
        }
        .perform(app.world_mut())
        .expect("target set");
    }
    run_ticks(app.world_mut(), 5 * DAY);
    let pens = livestock::views(app.world());
    for pen in &pens {
        assert_eq!(pen.head, 2, "starved to the floor of two: {pen:?}");
        assert_eq!(pen.died, 2, "{pen:?}");
    }
    // Feed one herd in full by hand (as finished batches would) for a day.
    let pen = pens[0].id;
    let herd = entity(app.world(), pen);
    let born_before = pens[0].born;
    {
        let mut state = app
            .world_mut()
            .get_mut::<livestock::Livestock>(herd)
            .expect("herd");
        state.fed_today = state.head;
    }
    let now = app.world().resource::<SimClock>().tick;
    run_ticks(app.world_mut(), DAY - now % DAY + 1);
    let after = app
        .world()
        .get::<livestock::Livestock>(herd)
        .expect("herd")
        .clone();
    assert_eq!(after.head, 3, "a fed herd breeds in spring: {after:?}");
    assert_eq!(after.born, born_before + 1);
    village::integrity::validate_colony(app.world()).expect("valid");
}

/// Turnips and eggs are foods of the diet: a resident holding only them eats
/// them, after the foods it prefers and by food value.
#[test]
fn turnips_and_eggs_are_eaten_through_the_diet() {
    let mut app = world_with(|world| short_days(world, DAY));
    run_ticks(app.world_mut(), 2);
    let (_, actor) = all_residents(app.world())[0];
    let diet = production::diet(app.world(), actor);
    let at = |kind: &str| diet.iter().position(|food| *food == kind).expect(kind);
    assert!(at(catalog::EGG) < at(catalog::BERRIES), "{diet:?}");
    assert!(at(catalog::TURNIP) < at(catalog::BERRIES), "{diet:?}");
    assert!(at("bread") < at(catalog::TURNIP), "{diet:?}");
    // Nothing but turnips in the pack: a hungry resident eats one.
    for kind in diet.iter().copied() {
        let held = econ::count(app.world(), actor, kind);
        if held > 0 {
            econ::consume(app.world_mut(), actor, kind, held).expect("emptied");
        }
    }
    econ::grant(
        app.world_mut(),
        actor,
        npc_framework::interaction::Item::new(catalog::TURNIP).stack(3),
    );
    if let Some(mut needs) = app
        .world_mut()
        .get_mut::<npc_framework::needs::Needs>(actor)
    {
        for need in &mut needs.values {
            need.value = need.value.max(60_000);
        }
    }
    let hunger = app
        .world()
        .resource::<npc_framework::needs::NeedRegistry>()
        .id("Hunger")
        .expect("hunger");
    app.world_mut()
        .get_mut::<npc_framework::needs::Needs>(actor)
        .expect("needs")
        .values[hunger.0 as usize]
        .value = 20_000;
    assert_eq!(
        production::food_choice(app.world(), actor),
        Some(catalog::TURNIP)
    );
    let eaten = until(&mut app, DAY, 20, |app| {
        econ::count(app.world(), actor, catalog::TURNIP) < 3
    });
    assert!(eaten, "the turnips were never eaten");
}

/// A save in spring, with turnips growing and hens fed, continues as the
/// title screen does with the same hash, and both run on alike.
#[test]
fn a_save_mid_season_continues_with_the_same_hash() {
    let mut app = world_with(quick_turnips);
    set_journal(app.world_mut(), true, false);
    let growing = until(&mut app, 4 * DAY, 100, |app| {
        view::fields(app.world()).iter().any(|field| {
            field.crop == "turnip" && field.plots.iter().any(|plot| plot.stage != "fallow")
        }) && feedings(app.world()) > 0
    });
    assert!(growing, "no turnip growing or no hen fed");
    let saved = snapshot::save(app.world_mut()).expect("saved");
    let mut fresh = world_with(quick_turnips);
    let expected = snapshot::save(fresh.world_mut()).expect("baseline");
    integrity::validate_snapshot_structure(&expected, &saved).expect("the save continues");
    let report =
        snapshot::load(fresh.world_mut(), &saved, snapshot::LoadMode::Strict).expect("restores");
    assert!(
        report.defaulted.is_empty() && report.skipped.is_empty(),
        "{report:?}"
    );
    integrity::validate_colony(fresh.world()).expect("valid after the restore");
    assert_eq!(
        livestock::views(fresh.world()),
        livestock::views(app.world()),
        "the herds came back as they were"
    );
    assert_eq!(state_hash(fresh.world_mut()), state_hash(app.world_mut()));
    // Across the next dawn, where the herds are settled.
    run_ticks(app.world_mut(), DAY);
    run_ticks(fresh.world_mut(), DAY);
    assert_eq!(
        state_hash(fresh.world_mut()),
        state_hash(app.world_mut()),
        "the continued world runs on as the original"
    );
}

/// Two runs of one seed agree, turnip fields and herds included.
#[test]
fn turnips_and_hens_are_deterministic() {
    let run = || {
        let mut app = world_with(quick_turnips);
        run_ticks(app.world_mut(), 3 * DAY);
        let pens = livestock::views(app.world());
        let fields = view::fields(app.world());
        (state_hash(app.world_mut()), pens, fields)
    };
    let (a, pens_a, fields_a) = run();
    let (b, pens_b, fields_b) = run();
    assert_eq!(pens_a, pens_b);
    assert_eq!(fields_a, fields_b);
    assert_eq!(a, b);
}
