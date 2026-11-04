//! Each profession's public recipe is executable by a registered player client.
use bevy_app::App;
use bevy_ecs::prelude::*;
use npc_framework::agent_io::{api, registration};
use npc_framework::crafting::{RecipeDef, RecipeRegistry, Workstation};
use npc_framework::prelude::*;
use serde_json::json;
use testbed_content::village::{self, Role, Wallet};

fn fixture(verb: &str) -> (App, Entity, SimId, String, RecipeDef) {
    let mut app = testbed_content::new_sim_app(7, "ai_village");
    let recipe = app.world().resource::<RecipeRegistry>().recipes
        [verb.strip_prefix("craft:").unwrap()]
    .clone();
    let ids = app.world().resource::<testbed_content::TestbedIds>();
    let actor_id = ids.id("human");
    let residents: Vec<_> = Role::RESIDENTS
        .iter()
        .map(|role| ids.id(role.person()))
        .collect();
    // Operator fixture supplies actual ingredients and isolates station
    // contention; the client still uses registration, perception and actions.
    for resident in residents {
        let entity = app.world().resource::<SimIndex>().get(resident).unwrap();
        app.world_mut().entity_mut(entity).remove::<Brain>();
    }
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
        .expect("recipe has a real village workstation");
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
        "production-fixture",
        registration::Credential {
            token_digest: registration::token_digest("public-test-token"),
            assigned: actor_id,
            controller: "visitor".into(),
        },
    );
    let session = api::call(
        app.world_mut(),
        "agent/register",
        Some(json!({"agent_id":"production-fixture","token":"public-test-token"})),
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
fn registered_client_can_complete_every_advertised_island_recipe() {
    assert_eq!(village::catalog::RECIPE_VERBS.len(), 6);
    for verb in village::catalog::RECIPE_VERBS {
        let (mut app, actor, station, session, recipe) = fixture(verb);
        let observed = api::call(
            app.world_mut(),
            "agent/observe",
            Some(json!({"session":session})),
        )
        .unwrap();
        let affordance = observed["affordances"]
            .as_array()
            .unwrap()
            .iter()
            .find(|action| action["verb"] == *verb && action["target"] == json!(station))
            .expect("known workstation advertises its supported recipe");
        assert_eq!(affordance["ok"], true, "{verb}: {affordance}");
        let reply = api::call(
            app.world_mut(),
            "agent/act",
            Some(json!({"session":session,"verb":verb,"target":station})),
        )
        .unwrap();
        assert_eq!(reply["accepted"], true, "{verb}: {reply}");
        // Planning has not consumed anything; only the queued simulation action
        // can reserve inputs and complete their physical transformation.
        for input in &recipe.inputs {
            assert_eq!(
                village::economy::count(app.world(), actor, &input.key),
                input.count
            );
        }
        run_ticks(app.world_mut(), u64::from(recipe.duration_ticks) + 80);
        for input in &recipe.inputs {
            assert_eq!(
                village::economy::count(app.world(), actor, &input.key),
                0,
                "{verb}: input {}",
                input.key
            );
        }
        for output in &recipe.outputs {
            assert_eq!(
                village::economy::count(app.world(), actor, &output.kind),
                output.count,
                "{verb}: output {}",
                output.kind
            );
        }
        assert_eq!(
            app.world().get::<Wallet>(actor).unwrap().coins,
            0,
            "crafting must not fabricate currency"
        );
        assert!(
            app.world()
                .get::<npc_framework::crafting::Crafting>(actor)
                .is_none()
        );
    }
}

#[test]
fn actor_crafting_permission_applies_to_canonical_and_named_client_actions() {
    for allowed in [false, true] {
        for named in [false, true] {
            let (mut app, actor, station, session, recipe) = fixture("craft:village_mill");
            let actions = if allowed {
                std::collections::BTreeSet::from(["craft".to_owned()])
            } else {
                std::collections::BTreeSet::new()
            };
            app.world_mut()
                .entity_mut(actor)
                .insert(testbed_content::actors::AllowedActions(actions));
            let verb = if named { "craft:village_mill" } else { "craft" };
            let args = if named {
                json!({})
            } else {
                json!({"recipe":recipe.name})
            };
            let observed = api::call(
                app.world_mut(),
                "agent/observe",
                Some(json!({"session":session})),
            )
            .unwrap();
            let affordance = observed["affordances"]
                .as_array()
                .unwrap()
                .iter()
                .find(|action| {
                    action["verb"] == "craft:village_mill" && action["target"] == json!(station)
                })
                .unwrap();
            assert_eq!(affordance["ok"], allowed, "allowed={allowed}: {affordance}");
            let reply = api::call(
                app.world_mut(),
                "agent/act",
                Some(json!({"session":session,"verb":verb,"target":station,"args":args})),
            )
            .unwrap();
            assert_eq!(
                reply["accepted"], allowed,
                "{verb}, allowed={allowed}: {reply}"
            );
            run_ticks(app.world_mut(), u64::from(recipe.duration_ticks) + 80);
            assert_eq!(
                village::economy::count(app.world(), actor, village::catalog::GRAIN),
                u32::from(!allowed)
            );
            assert_eq!(
                village::economy::count(app.world(), actor, village::catalog::FLOUR),
                u32::from(allowed)
            );
            if !allowed {
                assert!(reply["message"].as_str().unwrap().contains("cannot craft"));
            }
        }
    }
}
