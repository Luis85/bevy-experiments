//! The regional economy is authored data: the `economy` block of the world file.
//!
//! These tests edit only JSON. They prove that the shipped file states the
//! built-in economy, that a file without the block still loads, that a broken
//! block is refused with a readable reason, and that a new good, recipe and
//! profession output added as data are produced and traded by the residents.
use bevy_app::App;
use bevy_ecs::prelude::*;
use island_content::village::{
    self, VillageState, Villager, economy,
    frontier::{LivingWorldSpec, economy_spec::EconomySpec, labor::ResidentProfile},
    market,
};
use npc_framework::prelude::*;
use serde_json::{Value, json};

const CHARCOAL: &str = "village_charcoal";
const BURN: &str = "village_burn_charcoal";

fn shipped_world() -> Value {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("worlds/living_world.json");
    serde_json::from_str(&std::fs::read_to_string(path).expect("shipped world")).expect("JSON")
}

fn app_with(living_world: &Value) -> Result<App, String> {
    let mut doc = island_content::scenario::scenario_doc("living_world").expect("scenario");
    doc.world["living_world"] = living_world.clone();
    island_content::scenario::load_scenario(&doc, 7).map(|loaded| loaded.app)
}

fn entry<'a>(list: &'a mut Value, field: &str, name: &str) -> &'a mut Value {
    list.as_array_mut()
        .expect("list")
        .iter_mut()
        .find(|entry| entry[field] == name)
        .unwrap_or_else(|| panic!("no entry {name}"))
}

/// The whole change is data: one good, one recipe, one extra output and recipe
/// for carpenters, and smiths who keep two units in stock.
fn add_charcoal(economy: &mut Value) {
    economy["goods"].as_array_mut().expect("goods").push(json!({
        "key": CHARCOAL, "display": "charcoal", "base_price": 3, "stock_limit": 20,
        "tags": ["fuel"], "item": {"weight_g": 300, "max_stack": 20}
    }));
    economy["recipes"]
        .as_array_mut()
        .expect("recipes")
        .push(json!({
            "key": BURN, "display": "Burn charcoal", "station": "village_saw",
            "inputs": [{"good": "log", "count": 1}],
            "outputs": [{"good": CHARCOAL, "count": 2}], "duration": 120
        }));
    let carpenter = entry(&mut economy["professions"], "role", "carpenter");
    carpenter["outputs"]
        .as_array_mut()
        .expect("outputs")
        .push(json!(CHARCOAL));
    carpenter["recipes"]
        .as_array_mut()
        .expect("recipes")
        .push(json!(BURN));
    let smith = entry(&mut economy["professions"], "role", "smith");
    smith["wants"]
        .as_array_mut()
        .expect("wants")
        .push(json!(CHARCOAL));
    smith["keeps"] = json!([{"good": CHARCOAL, "count": 2}]);
}

#[test]
fn shipped_world_file_states_the_built_in_economy() {
    let world = shipped_world();
    let block = &world["living_world"]["economy"];
    assert_eq!(block["version"], 1);
    assert_eq!(block["goods"].as_array().map(Vec::len), Some(24));
    assert_eq!(block["recipes"].as_array().map(Vec::len), Some(17));
    assert_eq!(block["professions"].as_array().map(Vec::len), Some(13));
    assert_eq!(block["patches"].as_array().map(Vec::len), Some(7));
    let parsed: EconomySpec = serde_json::from_value(block.clone()).expect("economy block");
    assert_eq!(parsed, EconomySpec::default());
    let preset = island_content::worlds::builtin_preset("living_world").expect("preset");
    assert_eq!(
        preset.living_world.expect("regional spec").economy,
        EconomySpec::default()
    );
}

#[test]
fn a_world_file_without_an_economy_block_gets_the_built_in_economy() {
    let mut world = shipped_world();
    world["living_world"]
        .as_object_mut()
        .expect("living world")
        .remove("economy");
    let spec: LivingWorldSpec =
        serde_json::from_value(world["living_world"].clone()).expect("older file");
    assert_eq!(spec, LivingWorldSpec::default());
    spec.validate().expect("valid");
    // A partial block keeps the built-in sections it does not mention.
    world["living_world"]["economy"] = json!({"tuning": {"levy_percent": 25}});
    let spec: LivingWorldSpec =
        serde_json::from_value(world["living_world"].clone()).expect("partial block");
    assert_eq!(spec.economy.tuning.levy_percent, 25);
    assert_eq!(spec.economy.goods, EconomySpec::default().goods);
    spec.validate().expect("valid");
}

#[test]
fn a_broken_economy_block_is_refused_with_a_reason_naming_the_entry() {
    let mut world = shipped_world();
    let recipes = &mut world["living_world"]["economy"]["recipes"];
    entry(recipes, "key", "village_bake")["inputs"][0]["good"] = json!("village_rye");
    let Err(error) = app_with(&world["living_world"]) else {
        panic!("a recipe with an unknown good must be refused");
    };
    assert!(
        error.contains("recipe `village_bake` references unknown good `village_rye`"),
        "{error}"
    );
    let mut world = shipped_world();
    world["living_world"]["economy"]["tunning"] = json!({});
    assert!(app_with(&world["living_world"]).is_err(), "unknown field");
}

#[test]
fn edited_economy_data_is_part_of_the_hashed_construction() {
    let world = shipped_world();
    let mut plain = app_with(&world["living_world"]).expect("shipped");
    let mut edited = world["living_world"].clone();
    entry(&mut edited["economy"]["goods"], "key", "bread")["base_price"] = json!(7);
    let mut edited = app_with(&edited).expect("edited");
    assert_ne!(
        state_hash(plain.world_mut()),
        state_hash(edited.world_mut()),
        "economy data must be covered by the state hash"
    );
    assert_eq!(market::unit_price(edited.world(), "bread"), Some(7));
    assert_eq!(market::unit_price(plain.world(), "bread"), Some(6));
}

#[test]
fn a_good_recipe_and_profession_output_added_as_data_are_produced_and_traded() {
    let mut world = shipped_world();
    add_charcoal(&mut world["living_world"]["economy"]);
    let mut app = app_with(&world["living_world"]).expect("edited world loads");
    let people: Vec<Entity> = app
        .world()
        .resource::<SimIndex>()
        .map
        .values()
        .copied()
        .filter(|entity| app.world().get::<ResidentProfile>(*entity).is_some())
        .collect();
    let held = |app: &App, role: village::Role| -> u32 {
        people
            .iter()
            .filter(|actor| {
                app.world()
                    .get::<Villager>(**actor)
                    .is_some_and(|person| person.role == role)
            })
            .map(|actor| economy::count(app.world(), *actor, CHARCOAL))
            .sum()
    };
    assert_eq!(market::unit_price(app.world(), CHARCOAL), Some(3));
    assert_eq!(held(&app, village::Role::Smith), 0);
    let mut bought = 0;
    while app.world().resource::<SimClock>().tick < 9_600 && bought == 0 {
        run_ticks(app.world_mut(), 300);
        bought = held(&app, village::Role::Smith);
    }
    let now = app.world().resource::<SimClock>().tick;
    let state = app.world().resource::<VillageState>();
    let produced = state.produced.get(CHARCOAL).copied().unwrap_or(0);
    let sold = app
        .world()
        .resource::<market::MarketState>()
        .commodities
        .get(CHARCOAL)
        .map_or(0, |row| row.total_units);
    println!("ECONOMY_DATA tick={now} produced={produced} sold={sold} smiths_hold={bought}");
    assert!(produced > 0, "no carpenter made the new good by tick {now}");
    assert!(
        sold > 0 && bought > 0,
        "the new good was never traded to a smith by tick {now}"
    );
    village::integrity::validate_colony(app.world()).expect("colony integrity");
}

/// Prints the built-in economy as one line of JSON. Use it to refresh the
/// `economy` block of the shipped world and scenario files after a default
/// changes:
/// cargo test --locked -p island_content --test island_economy_data -- --ignored --nocapture print
#[test]
#[ignore = "prints the built-in economy block for the shipped files"]
fn print_the_built_in_economy_block() {
    println!(
        "ECONOMY_JSON {}",
        serde_json::to_string(&EconomySpec::default()).expect("economy JSON")
    );
}
