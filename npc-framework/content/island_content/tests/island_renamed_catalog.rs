//! The living world takes every good's and trade's role from its catalog
//! data, not from names in code: a copy of the shipped world with three goods
//! and one trade renamed founds, runs and lives (ADR 0042).
use bevy_ecs::prelude::*;
use island_content::village::{self, Villager, economy as econ, living};
use npc_framework::prelude::*;
use serde_json::{Value, json};

/// Every string in `value` equal to `from` becomes `to`.
fn rename(value: &mut Value, from: &str, to: &str) {
    match value {
        Value::String(text) if text == from => *text = to.to_owned(),
        Value::Array(items) => items.iter_mut().for_each(|item| rename(item, from, to)),
        Value::Object(map) => map.values_mut().for_each(|item| rename(item, from, to)),
        _ => {}
    }
}

/// The shipped living world with log -> timber, bread -> loaf, warm clothes
/// -> cloak, and the tailor's trade called clothier.
fn renamed_world(seed: u64) -> bevy_app::App {
    let mut doc = island_content::scenario::scenario_doc("living_world").expect("scenario");
    let world = &mut doc.world["living_world"];
    for (from, to) in [
        ("log", "timber"),
        ("bread", "loaf"),
        ("village_warm_clothes", "cloak"),
    ] {
        rename(world, from, to);
    }
    let economy = &mut world["economy"];
    // Engine item kinds carry their own item data; renamed goods declare it.
    for good in economy["goods"].as_array_mut().expect("goods") {
        match good["key"].as_str() {
            Some("timber") => good["item"] = json!({"weight_g": 2000, "max_stack": 20}),
            Some("loaf") => good["item"] = json!({"weight_g": 300, "max_stack": 24}),
            _ => {}
        }
    }
    for profession in economy["professions"].as_array_mut().expect("professions") {
        if profession["role"] == "tailor" {
            profession["display"] = json!("clothier");
        }
    }
    island_content::scenario::load_scenario(&doc, seed)
        .map(|loaded| loaded.app)
        .unwrap_or_else(|error| panic!("the renamed world did not load: {error}"))
}

fn residents(app: &mut bevy_app::App) -> Vec<(SimId, Entity)> {
    let mut people: Vec<(SimId, Entity)> = app
        .world_mut()
        .query::<(Entity, &SimId, &Villager)>()
        .iter(app.world())
        .map(|(entity, id, _)| (*id, entity))
        .collect();
    people.sort_unstable_by_key(|(id, _)| *id);
    people
}

fn lives(days: u64) {
    let mut app = renamed_world(7);
    let roles = village::roles::roles(app.world()).clone();
    assert_eq!(roles.fuel, "timber");
    assert_eq!(roles.timber, "timber");
    assert_eq!(roles.baked, "loaf");
    assert_eq!(roles.garment, "cloak");
    assert_eq!(
        village::catalog::good_with_tag(
            app.world(),
            village::frontier::economy_spec::GoodTag::Clothing
        ),
        Some("cloak")
    );
    let day = app.world().resource::<SimConfig>().day_length;
    let before = app
        .world()
        .resource::<village::VillageState>()
        .produced
        .clone();
    run_ticks(app.world_mut(), days * day);
    {
        let state = app.world().resource::<village::VillageState>();
        let made = |kind: &str| {
            state.produced.get(kind).copied().unwrap_or(0) - before.get(kind).copied().unwrap_or(0)
        };
        // They eat, drink, heat their homes, wear a garment and trade.
        assert!(made("loaf") > 0, "no loaf was baked: {:?}", state.produced);
        assert!(made("timber") > 0, "no timber was cut");
        assert!(state.transactions > 0, "nobody traded");
    }
    let burned: u64 = app
        .world_mut()
        .query::<&village::colony::Household>()
        .iter(app.world())
        .map(|home| home.logs_burned)
        .sum();
    assert!(burned > 0, "no hearth burned timber");
    let mut lowest = i32::MAX;
    let mut coated = 0;
    for (_, actor) in residents(&mut app) {
        let world = app.world();
        if !village::living(world, actor) {
            continue;
        }
        for need in ["Hunger", "Thirst", "Warmth"] {
            lowest = lowest.min(econ::need(world, actor, need));
        }
        coated += u32::from(living::has_coat(world, actor));
    }
    assert!(lowest > 0, "somebody went without: lowest need {lowest}");
    assert!(coated > 0, "nobody wears a cloak");
    village::integrity::validate_colony(app.world()).expect("valid");
}

#[test]
fn a_world_with_renamed_goods_and_trade_founds_and_lives_two_days() {
    lives(2);
}

/// The longer proof, for the closing phase.
#[test]
#[ignore = "explicit 14-day run of the renamed catalog; run in release"]
fn a_world_with_renamed_goods_and_trade_lives_fourteen_days() {
    lives(14);
}

/// A living world whose data lacks a role the logic needs is refused at
/// load with a sentence naming the role.
#[test]
fn a_world_without_a_fuel_is_refused_naming_the_role() {
    let mut doc = island_content::scenario::scenario_doc("living_world").expect("scenario");
    for good in doc.world["living_world"]["economy"]["goods"]
        .as_array_mut()
        .expect("goods")
    {
        if let Some(tags) = good.get_mut("tags").and_then(Value::as_array_mut) {
            tags.retain(|tag| tag != "fuel");
        }
    }
    let error = island_content::scenario::load_scenario(&doc, 7)
        .err()
        .expect("refused");
    assert!(error.contains("`fuel`"), "{error}");
    assert!(error.contains("nothing to burn"), "{error}");
}
