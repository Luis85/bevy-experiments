//! Named recipe verbs must obey the same world policy as NPC production.
use island_content::village::{self, Role};
use npc_framework::agent_io::{api, registration};
use npc_framework::prelude::*;
use serde_json::json;

#[test]
fn advertised_named_recipe_action_obeys_the_world_crafting_switch() {
    for enabled in [false, true] {
        let mut app = island_content::new_sim_app(7, "ai_village");
        let ids = app.world().resource::<island_content::TestbedIds>();
        let actor_id = ids.id("human");
        let mill_id = ids.id(&format!("village_{}", Role::Miller.tag()));
        let residents: Vec<_> = Role::RESIDENTS
            .iter()
            .map(|role| ids.id(role.person()))
            .collect();
        // Operator fixture: isolate the public policy/action boundary from
        // unrelated station contention. No provider or private files are used.
        for resident in residents {
            let entity = app.world().resource::<SimIndex>().get(resident).unwrap();
            app.world_mut().entity_mut(entity).remove::<Brain>();
        }
        let actor = app.world().resource::<SimIndex>().get(actor_id).unwrap();
        let mill = app.world().resource::<SimIndex>().get(mill_id).unwrap();
        let pos = *app.world().get::<SimPos>(mill).unwrap();
        app.world_mut().entity_mut(actor).insert(pos);
        village::economy::grant(app.world_mut(), actor, Item::new(village::catalog::GRAIN));
        app.world_mut()
            .resource_mut::<island_content::world_runtime::ActiveWorld>()
            .0
            .systems
            .crafting = enabled;
        app.insert_resource(registration::RegistrationRequired);
        registration::set_credential(
            app.world_mut(),
            "craft-policy-fixture",
            registration::Credential {
                token_digest: registration::token_digest("public-test-token"),
                assigned: actor_id,
                controller: "visitor".into(),
            },
        );
        let session = api::call(
            app.world_mut(),
            "agent/register",
            Some(json!({"agent_id":"craft-policy-fixture","token":"public-test-token"})),
        )
        .unwrap()["session"]
            .as_str()
            .unwrap()
            .to_owned();
        for method in ["agent/enter_world", "agent/take_control"] {
            api::call(app.world_mut(), method, Some(json!({"session":session}))).unwrap();
        }
        run_ticks(app.world_mut(), 40);
        let verb = format!("craft:{}", village::catalog::MILL);
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
            .find(|action| action["verb"] == verb && action["target"] == json!(mill_id))
            .expect("known mill advertises its named recipe");
        let reply = api::call(
            app.world_mut(),
            "agent/act",
            Some(json!({"session":session,"verb":verb,"target":mill_id})),
        )
        .unwrap();
        assert_eq!(reply["accepted"], enabled, "enabled={enabled}: {reply}");
        assert_eq!(affordance["ok"], enabled, "enabled={enabled}: {affordance}");
        if enabled {
            run_ticks(app.world_mut(), 200);
            assert_eq!(
                village::economy::count(app.world(), actor, village::catalog::GRAIN),
                0
            );
            assert_eq!(
                village::economy::count(app.world(), actor, village::catalog::FLOUR),
                1
            );
        } else {
            assert!(reply["message"].as_str().unwrap().contains("disabled"));
            run_ticks(app.world_mut(), 200);
            assert_eq!(
                village::economy::count(app.world(), actor, village::catalog::GRAIN),
                1
            );
            assert_eq!(
                village::economy::count(app.world(), actor, village::catalog::FLOUR),
                0
            );
        }
    }
}
