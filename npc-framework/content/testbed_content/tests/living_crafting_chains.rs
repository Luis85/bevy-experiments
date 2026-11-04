//! Living economy recipe DAG and real registered-client production contracts.
use bevy_app::App;
use bevy_ecs::prelude::*;
use npc_framework::agent_io::{api, registration};
use npc_framework::crafting::{Durability, RecipeDef, RecipeRegistry, Workstation};
use npc_framework::prelude::*;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use testbed_content::village::{self, Role, Villager, catalog};

#[test]
fn living_goods_and_recipes_are_opt_in_and_legacy_trade_tables_are_unchanged() {
    let legacy = testbed_content::new_sim_app(7, "ai_village");
    let living = testbed_content::new_sim_app(7, "living_village");
    for verb in catalog::LIVING_RECIPE_VERBS {
        let name = verb.strip_prefix("craft:").unwrap();
        assert!(
            !legacy
                .world()
                .resource::<RecipeRegistry>()
                .recipes
                .contains_key(name)
        );
        assert!(
            living
                .world()
                .resource::<RecipeRegistry>()
                .recipes
                .contains_key(name)
        );
    }
    assert_eq!(catalog::trade_kinds(legacy.world()), catalog::TRADE_KINDS);
    assert_eq!(catalog::outputs(Role::Baker), &["bread"]);
    assert_eq!(catalog::outputs(Role::Smith), &["ingot", "hammer"]);
    assert_eq!(
        catalog::outputs_for(legacy.world(), Role::Hunter),
        &[] as &[&str]
    );
    for kind in [
        catalog::RAW_MEAT,
        catalog::HIDE,
        catalog::LEATHER,
        catalog::FIBER,
        catalog::THREAD,
        catalog::COOKED_MEAL,
        catalog::WARM_CLOTHES,
        catalog::SPEAR,
        catalog::BERRIES,
    ] {
        assert!(!catalog::trade_kinds(legacy.world()).contains(&kind));
        assert!(catalog::trade_kinds(living.world()).contains(&kind));
        assert!(catalog::price(kind).is_some_and(|price| price > 0));
        assert!(catalog::stock_limit(kind) > 0);
    }
    assert_eq!(catalog::food_restore(catalog::RAW_MEAT), None);
    assert_eq!(catalog::food_restore(catalog::COOKED_MEAL), Some(90_000));
    assert_eq!(catalog::food_restore(catalog::BERRIES), Some(20_000));
}

fn roots(
    kind: &str,
    graph: &BTreeMap<String, Vec<String>>,
    visiting: &mut BTreeSet<String>,
) -> BTreeSet<String> {
    assert!(
        visiting.insert(kind.into()),
        "production graph contains a cycle at {kind}"
    );
    let mut result = BTreeSet::new();
    match graph.get(kind) {
        Some(inputs) => {
            for input in inputs {
                result.extend(roots(input, graph, visiting));
            }
        }
        None => {
            result.insert(kind.into());
        }
    }
    visiting.remove(kind);
    result
}

#[test]
fn finished_goods_join_real_hunting_farming_water_and_forestry_inputs_without_cycles() {
    let app = testbed_content::new_sim_app(7, "living_village");
    let recipes = &app.world().resource::<RecipeRegistry>().recipes;
    let mut graph = BTreeMap::new();
    for verb in catalog::RECIPE_VERBS
        .iter()
        .chain(catalog::LIVING_RECIPE_VERBS)
    {
        let recipe = &recipes[verb.strip_prefix("craft:").unwrap()];
        for output in &recipe.outputs {
            graph.insert(
                output.kind.clone(),
                recipe
                    .inputs
                    .iter()
                    .map(|input| input.key.clone())
                    .collect(),
            );
        }
    }
    for output in graph.keys() {
        roots(output, &graph, &mut BTreeSet::new());
    }
    assert_eq!(
        roots(catalog::WARM_CLOTHES, &graph, &mut BTreeSet::new()),
        BTreeSet::from([
            catalog::HIDE.into(),
            catalog::FIBER.into(),
            catalog::WATER.into(),
            "log".into()
        ])
    );
    assert_eq!(
        roots(catalog::COOKED_MEAL, &graph, &mut BTreeSet::new()),
        BTreeSet::from([
            catalog::RAW_MEAT.into(),
            catalog::WATER.into(),
            "log".into()
        ])
    );
    assert_eq!(
        roots(catalog::SPEAR, &graph, &mut BTreeSet::new()),
        BTreeSet::from(["ore".into(), "log".into()])
    );
}

fn fixture(verb: &str) -> (App, Entity, SimId, String, RecipeDef) {
    let mut app = testbed_content::new_sim_app(7, "living_village");
    let recipe = app.world().resource::<RecipeRegistry>().recipes
        [verb.strip_prefix("craft:").unwrap()]
    .clone();
    let people: Vec<_> = app
        .world_mut()
        .query::<(Entity, &Villager)>()
        .iter(app.world())
        .filter(|(_, person)| person.role != Role::Visitor)
        .map(|(entity, _)| entity)
        .collect();
    // Operator setup isolates workstations and supplies inputs. No game action
    // is allowed to create these materials or advance the operator clock.
    for person in people {
        app.world_mut().entity_mut(person).remove::<Brain>();
    }
    let actor_id = app.world().resource::<village::VillageState>().played;
    let actor = app.world().resource::<SimIndex>().get(actor_id).unwrap();
    let (station_id, station) = app
        .world()
        .resource::<SimIndex>()
        .map
        .iter()
        .find(|(_, entity)| {
            app.world()
                .get::<Workstation>(**entity)
                .is_some_and(|station| recipe.hosted_by(&station.kinds))
        })
        .map(|(id, entity)| (*id, *entity))
        .expect("physical recipe workstation");
    let pos = *app.world().get::<SimPos>(station).unwrap();
    app.world_mut().entity_mut(actor).insert(pos);
    for input in &recipe.inputs {
        village::economy::grant(
            app.world_mut(),
            actor,
            Item::new(&input.key).stack(input.count),
        );
    }
    app.insert_resource(registration::RegistrationRequired);
    registration::set_credential(
        app.world_mut(),
        "living-production-fixture",
        registration::Credential {
            token_digest: registration::token_digest("public-test-token"),
            assigned: actor_id,
            controller: "visitor".into(),
        },
    );
    let session = api::call(
        app.world_mut(),
        "agent/register",
        Some(json!({"agent_id":"living-production-fixture","token":"public-test-token"})),
    )
    .unwrap()["session"]
        .as_str()
        .unwrap()
        .to_owned();
    for method in ["agent/enter_world", "agent/take_control"] {
        api::call(app.world_mut(), method, Some(json!({"session":session}))).unwrap();
    }
    run_ticks(app.world_mut(), 40);
    (app, actor, station_id, session, recipe)
}

#[test]
fn a_registered_client_physically_completes_every_new_recipe_and_clothes_get_real_wear_state() {
    for verb in catalog::LIVING_RECIPE_VERBS {
        let (mut app, actor, station, session, recipe) = fixture(verb);
        let observed = api::call(
            app.world_mut(),
            "agent/observe",
            Some(json!({"session":session})),
        )
        .unwrap();
        let offered = observed["affordances"]
            .as_array()
            .unwrap()
            .iter()
            .find(|action| action["verb"] == *verb && action["target"] == json!(station))
            .expect("new recipe advertised at its real station");
        assert_eq!(offered["ok"], true, "{verb}: {offered}");
        let reply = api::call(
            app.world_mut(),
            "agent/act",
            Some(json!({"session":session,"verb":verb,"target":station})),
        )
        .unwrap();
        assert_eq!(reply["accepted"], true, "{verb}: {reply}");
        run_ticks(app.world_mut(), u64::from(recipe.duration_ticks) + 80);
        for input in &recipe.inputs {
            assert_eq!(
                village::economy::count(app.world(), actor, &input.key),
                0,
                "{verb} input {}",
                input.key
            );
        }
        for output in &recipe.outputs {
            assert_eq!(
                village::economy::count(app.world(), actor, &output.kind),
                output.count,
                "{verb} output {}",
                output.kind
            );
            if output.kind == catalog::WARM_CLOTHES {
                let id = npc_framework::interaction::items::carried_matching(
                    app.world(),
                    actor,
                    &output.kind,
                )
                .unwrap();
                let entity = app.world().resource::<SimIndex>().get(id).unwrap();
                assert_eq!(
                    app.world().get::<Durability>(entity),
                    Some(&Durability::new(catalog::CLOTHING_DURABILITY))
                );
            }
        }
    }
}

#[test]
fn shared_eating_consumes_finished_meals_and_berries_but_rejects_raw_meat() {
    for (kind, restored) in [(catalog::COOKED_MEAL, 90_000), (catalog::BERRIES, 20_000)] {
        let mut app = testbed_content::new_sim_app(7, "living_village");
        let actor_id = app.world().resource::<village::VillageState>().played;
        let actor = app.world().resource::<SimIndex>().get(actor_id).unwrap();
        let hunger = app
            .world()
            .resource::<npc_framework::needs::NeedRegistry>()
            .id("Hunger")
            .unwrap();
        app.world_mut()
            .get_mut::<npc_framework::needs::Needs>(actor)
            .unwrap()
            .get_mut(hunger)
            .value = 1000;
        village::economy::grant(app.world_mut(), actor, Item::new(kind));
        let mut eat = village::VillageAct::new(actor_id, "eat");
        eat.kind = kind.into();
        eat.perform(app.world_mut()).unwrap();
        assert_eq!(
            village::economy::need(app.world(), actor, "Hunger"),
            1000 + restored
        );
        assert_eq!(village::economy::count(app.world(), actor, kind), 0);
        village::economy::grant(app.world_mut(), actor, Item::new(catalog::RAW_MEAT));
        eat.kind = catalog::RAW_MEAT.into();
        assert!(eat.perform(app.world_mut()).is_err());
        assert_eq!(
            village::economy::count(app.world(), actor, catalog::RAW_MEAT),
            1
        );
    }
}

#[test]
fn default_garment_lifetime_can_be_replaced_by_the_finite_daily_hunting_chain() {
    let app = testbed_content::new_sim_app(7, "living_village");
    let world = app.world();
    let actors = world
        .resource::<SimIndex>()
        .map
        .values()
        .filter(|entity| world.get::<Villager>(**entity).is_some())
        .count() as u64;
    assert_eq!(
        actors, 13,
        "include the visiting hero as well as all twelve residents"
    );
    let day_ticks = world.resource::<SimConfig>().day_length;
    assert_eq!(day_ticks, 24_000);
    let habitat_id = world
        .resource::<testbed_content::TestbedIds>()
        .id("wildlife_habitat");
    let habitat = world.resource::<SimIndex>().get(habitat_id).unwrap();
    let prey_per_day = u64::from(
        world
            .get::<village::hunting::WildlifeHabitat>(habitat)
            .unwrap()
            .capacity
            .min(village::hunting::MAX_WILDLIFE),
    );
    let recipes = &world.resource::<RecipeRegistry>().recipes;
    let tanning = &recipes[catalog::TAN];
    let tailoring = &recipes[catalog::TAILOR];
    let hide_per_tan = u64::from(
        tanning
            .inputs
            .iter()
            .find(|input| input.key == catalog::HIDE)
            .unwrap()
            .count,
    );
    let leather_per_tan = u64::from(
        tanning
            .outputs
            .iter()
            .find(|output| output.kind == catalog::LEATHER)
            .unwrap()
            .count,
    );
    let leather_per_tailor = u64::from(
        tailoring
            .inputs
            .iter()
            .find(|input| input.key == catalog::LEATHER)
            .unwrap()
            .count,
    );
    let garments_per_tailor = u64::from(
        tailoring
            .outputs
            .iter()
            .find(|output| output.kind == catalog::WARM_CLOTHES)
            .unwrap()
            .count,
    );
    // The physical-hunting contract verifies exactly one hide per prey. An
    // equipped garment loses one durability per exposed cold tick. Assume all
    // actors remain outside for an entire cold day: shelter only reduces demand.
    // Compare rational daily replacement demand and the actual recipe yield by
    // cross multiplication, without rounding down demand or using floats.
    let replacement_demand = actors * day_ticks * hide_per_tan * leather_per_tailor;
    let sustainable_supply = u64::from(catalog::CLOTHING_DURABILITY)
        * prey_per_day
        * leather_per_tan
        * garments_per_tailor;
    assert!(
        replacement_demand <= sustainable_supply,
        "the garment lifespan creates structurally unmet replacement demand: {replacement_demand} > {sustainable_supply}; actors={actors}, day_ticks={day_ticks}, durability={}, daily_prey={prey_per_day}",
        catalog::CLOTHING_DURABILITY
    );
}
