//! A trade added purely in data (ADR 0042): a copy of the shipped living
//! world plus a clay patch, a pots good with its recipe, a pottery workplace
//! in every settlement and a potter trade. The world founds, a resident takes
//! the trade, digs clay, makes pots and sells them, and the views and the
//! journal name the trade by its display name.
use bevy_ecs::prelude::*;
use island_content::village::{self, Villager, economy as econ};
use npc_framework::kernel::journal::{Journal, JournalKind, set_journal};
use npc_framework::prelude::*;
use serde_json::{Value, json};

/// Where each settlement's pottery stands: the free slot of its workplace
/// grid in the shipped layout.
const POTTERIES: [(&str, i32, i32); 3] = [
    ("harbour", 30_000, -22_000),
    ("alderwood", -77_000, 22_000),
    ("stone_ridge", 45_000, 19_000),
];

pub const POTTER_DISPLAY: &str = "clay potter";

/// The shipped world with the pottery trade added in data only.
pub fn pottery_world(seed: u64) -> bevy_app::App {
    let mut doc = island_content::scenario::scenario_doc("living_world").expect("scenario");
    let world = &mut doc.world["construction"]["world"]["preset"]["living_world"];
    let economy = &mut world["economy"];
    let goods = economy["goods"].as_array_mut().expect("goods");
    goods.push(json!({
        "key": "clay", "display": "clay", "base_price": 2, "stock_limit": 20,
        "item": {"weight_g": 1500, "max_stack": 20}
    }));
    goods.push(json!({
        "key": "pots", "display": "clay pots", "base_price": 7, "stock_limit": 10,
        "item": {"weight_g": 800, "max_stack": 10}
    }));
    economy["recipes"]
        .as_array_mut()
        .expect("recipes")
        .push(json!({
            "key": "throw_pots", "display": "Throw pots", "station": "potter_wheel",
            "inputs": [{"good": "clay", "count": 2}],
            "outputs": [{"good": "pots", "count": 1}],
            "duration": 160
        }));
    economy["patches"]
        .as_array_mut()
        .expect("patches")
        .push(json!({
            "good": "clay", "dx": 0, "dz": 0, "at_site": "potter",
            "capacity": 200, "gather_ticks": 160, "regrowth": 6
        }));
    let professions = economy["professions"].as_array_mut().expect("professions");
    for profession in professions.iter_mut() {
        // Households want pots: the inn keeps a few, and several trades may
        // turn potter.
        match profession["role"].as_str() {
            Some("innkeeper") => {
                push(profession, "wants", json!("pots"));
                push(
                    profession,
                    "keeps",
                    json!({"good": "pots", "count": 3, "reserve_inputs": false}),
                );
            }
            Some("baker" | "miller" | "carpenter" | "smith") => {
                push(profession, "aptitudes", json!("potter"));
                push(profession, "wants", json!("pots"));
                push(
                    profession,
                    "keeps",
                    json!({"good": "pots", "count": 2, "reserve_inputs": false}),
                );
            }
            _ => {}
        }
    }
    professions.push(json!({
        "role": "potter", "display": POTTER_DISPLAY, "station": "potter_wheel",
        "outputs": ["clay", "pots"], "wants": ["clay"], "recipes": ["throw_pots"],
        "aptitudes": ["potter", "carpenter"], "template": "carpenter"
    }));
    economy["buildings"].as_array_mut().expect("buildings").push(json!({
        "id": "pottery", "display": "Pottery",
        "cost": [{"good": "plank", "count": 8}, {"good": "log", "count": 4}],
        "work_ticks": 800, "footprint": {"width": 5, "depth": 5},
        "kind": {"type": "workplace", "profession": "potter", "station": "potter_wheel", "capacity": 2}
    }));
    for settlement in world["settlements"].as_array_mut().expect("settlements") {
        let key = settlement["key"].as_str().expect("key").to_owned();
        let label = settlement["label"].as_str().expect("label").to_owned();
        let (_, x, z) = POTTERIES
            .iter()
            .find(|(name, ..)| *name == key)
            .expect("pottery spot");
        settlement["sites"]
            .as_array_mut()
            .expect("sites")
            .push(json!({
                "key": "work_potter", "label": format!("{label} pottery"), "role": "potter",
                "position": {"x": x, "z": z}, "capacity": 2
            }));
    }
    island_content::scenario::load_scenario(&doc, seed)
        .map(|loaded| loaded.app)
        .unwrap_or_else(|error| panic!("the pottery world did not load: {error}"))
}

fn push(object: &mut Value, field: &str, item: Value) {
    if object.get(field).is_none() {
        object[field] = json!([]);
    }
    object[field].as_array_mut().expect("list").push(item);
}

fn potter() -> village::Role {
    village::Role::from_key("potter")
}

fn potters(app: &mut bevy_app::App) -> Vec<(SimId, Entity)> {
    let mut people: Vec<(SimId, Entity)> = app
        .world_mut()
        .query::<(Entity, &SimId, &Villager)>()
        .iter(app.world())
        .filter(|(_, _, person)| person.role == potter())
        .map(|(entity, id, _)| (*id, entity))
        .collect();
    people.sort_unstable_by_key(|(id, _)| *id);
    people
}

fn lives(days: u64) {
    let mut app = pottery_world(7);
    let world = app.world();
    assert!(
        village::frontier::economy_spec::trades(world).contains(&potter()),
        "the potter is a trade of the catalog"
    );
    assert_eq!(
        village::frontier::economy_spec::trade_label(world, potter()),
        POTTER_DISPLAY
    );
    let catalogue = village::view::catalogue(world);
    let pottery = catalogue
        .iter()
        .find(|entry| entry.building == "pottery")
        .expect("the pottery is in the catalogue");
    assert_eq!(pottery.display, "Pottery");
    let day = app.world().resource::<SimConfig>().day_length;
    let before = app
        .world()
        .resource::<village::VillageState>()
        .produced
        .clone();
    // Occupation changes, gathered while the journal's ring buffer turns.
    set_journal(app.world_mut(), true, false);
    let mut changes: Vec<Value> = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for _ in 0..days * 8 {
        run_ticks(app.world_mut(), day / 8);
        for entry in &app.world().resource::<Journal>().entries {
            if let JournalKind::Effect {
                effect,
                applied: true,
            } = &entry.kind
                && effect.get("type").and_then(Value::as_str) == Some("RegionalOccupationChanged")
            {
                let value = serde_json::to_value(effect).expect("effect JSON");
                if seen.insert((entry.tick, value.to_string())) {
                    changes.push(value);
                }
            }
        }
    }
    let world = app.world();
    let state = world.resource::<village::VillageState>();
    let made = |kind: &str| {
        state.produced.get(kind).copied().unwrap_or(0) - before.get(kind).copied().unwrap_or(0)
    };
    assert!(made("clay") > 0, "no clay was dug: {:?}", state.produced);
    assert!(made("pots") > 0, "no pots were made: {:?}", state.produced);
    let people = potters(&mut app);
    assert!(!people.is_empty(), "nobody took the potter's trade");
    let world = app.world();
    let producers = village::view::producers(world, "pots");
    assert!(
        producers.iter().any(|view| view.role == POTTER_DISPLAY),
        "producers name the trade by its display name: {producers:?}"
    );
    let table = village::frontier::oversee::profession_table(
        world,
        village::frontier::settlements::member(world, people[0].1).expect("settlement"),
    );
    assert!(
        table
            .iter()
            .any(|row| row.key == "potter" && row.display == POTTER_DISPLAY && row.current > 0),
        "the profession table lists the potters: {table:?}"
    );
    assert!(
        changes
            .iter()
            .any(|change| change["to_key"] == "potter" && change["to_trade"] == POTTER_DISPLAY),
        "the journal names the new trade by key and display name: {changes:?}"
    );
    // Somebody who is not a potter holds pots: they were sold.
    let mut bought = 0;
    let mut query = app.world_mut().query::<(Entity, &Villager)>();
    let world = app.world();
    for (actor, person) in query.iter(world) {
        if person.role != potter() {
            bought += econ::count(world, actor, "pots");
        }
    }
    assert!(bought > 0, "nobody bought pots");
    // The potter's readable plan line uses display names only.
    let readable: Vec<String> = people
        .iter()
        .map(|(_, actor)| village::view::plan_lines(world, *actor).readable)
        .collect();
    assert!(
        readable.iter().all(|line| !line.contains("throw_pots")),
        "plan lines are readable: {readable:?}"
    );
    village::integrity::validate_colony(world).expect("valid");
}

#[test]
fn a_trade_added_in_data_founds_staffs_and_sells_within_four_days() {
    lives(4);
}

/// The longer proof, for the closing phase.
#[test]
#[ignore = "explicit 14-day run of the data-only trade; run in release"]
fn a_trade_added_in_data_lives_fourteen_days() {
    lives(14);
}

/// A data trade is saved and named by its key.
#[test]
fn a_data_trade_is_its_key_in_data_and_saves() {
    let role = potter();
    assert!(role.is_data_only());
    assert_eq!(serde_json::to_value(role).expect("json"), json!("potter"));
    let back: village::Role = serde_json::from_value(json!("potter")).expect("role");
    assert_eq!(back, role);
    let shipped: village::Role = serde_json::from_value(json!("water_keeper")).expect("role");
    assert_eq!(shipped, village::Role::WaterKeeper);
    assert!(serde_json::from_value::<village::Role>(json!("Potter!")).is_err());
}
