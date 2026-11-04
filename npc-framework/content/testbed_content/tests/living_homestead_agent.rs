//! Public homestead rules, recorded registered actions and scoped plot knowledge.
use bevy_app::App;
use npc_framework::agent_io::{api, observe, registration};
use npc_framework::kernel::hash::state_hash;
use npc_framework::prelude::*;
use serde_json::{Value, json};
use testbed_content::village::{
    self, catalog, economy,
    homestead::{self, HomesteadAct, HomesteadPlot},
};

fn fixture() -> (App, SimId, bevy_ecs::entity::Entity) {
    let app = testbed_content::new_sim_app(7, "living_village");
    let id = app.world().resource::<village::VillageState>().played;
    let actor = app.world().resource::<SimIndex>().get(id).unwrap();
    (app, id, actor)
}
fn look(app: &App, id: SimId) -> Value {
    observe::observe(app.world(), id, 20).unwrap().0
}
fn row(observation: &Value, id: SimId) -> Value {
    observation["sections"]["homestead"]["plots"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == json!(id))
        .unwrap()
        .clone()
}
fn visit(app: &mut App, actor: bevy_ecs::entity::Entity, plot: bevy_ecs::entity::Entity) {
    let pos = *app.world().get::<SimPos>(plot).unwrap();
    app.world_mut()
        .entity_mut(actor)
        .insert(pos)
        .remove::<MoveIntent>();
    run_ticks(app.world_mut(), 40);
}

#[test]
fn static_rules_are_machine_readable_without_revealing_unknown_plots() {
    let (mut app, id, _) = fixture();
    let before = state_hash(app.world_mut());
    let observed = look(&app, id);
    assert_eq!(state_hash(app.world_mut()), before);
    let section = &observed["sections"]["homestead"];
    assert_eq!(section["plots"], json!([]));
    assert_eq!(
        section["cottage"],
        json!({"inputs":[{"kind":"plank","count":8},{"kind":"log","count":4}],"duration_ticks":600})
    );
    assert_eq!(
        section["crop"],
        json!({"seed_kind":catalog::GRAIN,"seed_count":1,"water_kind":catalog::WATER,"water_count":1,
        "growth_ticks":6000,"yield_kind":catalog::GRAIN,"yield_count":3})
    );
    assert_eq!(section["claim_limits"], json!({"housing":1,"garden":1}));
}

#[test]
fn registered_claim_is_known_scoped_and_queued_before_it_mutates_ownership() {
    let (mut app, id, actor) = fixture();
    let (plot_id, plot) = homestead::plots(app.world())[0];
    app.insert_resource(registration::RegistrationRequired);
    registration::set_credential(
        app.world_mut(),
        "homestead-fixture",
        registration::Credential {
            token_digest: registration::token_digest("public-test-token"),
            assigned: id,
            controller: "visitor".into(),
        },
    );
    let session = api::call(
        app.world_mut(),
        "agent/register",
        Some(json!({"agent_id":"homestead-fixture","token":"public-test-token"})),
    )
    .unwrap()["session"]
        .as_str()
        .unwrap()
        .to_owned();
    for method in ["agent/enter_world", "agent/take_control"] {
        api::call(app.world_mut(), method, Some(json!({"session":session}))).unwrap();
    }
    run_ticks(app.world_mut(), 1);
    let unseen = api::call(
        app.world_mut(),
        "agent/act",
        Some(json!({"session":session,"verb":"claim_plot","target":plot_id})),
    )
    .unwrap();
    assert_eq!(unseen["accepted"], false);
    visit(&mut app, actor, plot);
    let observed = api::call(
        app.world_mut(),
        "agent/observe",
        Some(json!({"session":session})),
    )
    .unwrap();
    assert_eq!(
        row(&observed, plot_id)["front_door"],
        json!(*app.world().get::<SimPos>(plot).unwrap())
    );
    let accepted = api::call(
        app.world_mut(),
        "agent/act",
        Some(json!({"session":session,"verb":"claim_plot","target":plot_id})),
    )
    .unwrap();
    assert_eq!(accepted["accepted"], true, "{accepted}");
    assert_eq!(app.world().get::<HomesteadPlot>(plot).unwrap().owner, None);
    run_ticks(app.world_mut(), 1);
    assert_eq!(
        app.world().get::<HomesteadPlot>(plot).unwrap().owner,
        Some(id)
    );
}

#[test]
fn remembered_foreign_estates_hide_remote_ownership_and_crop_changes_but_owned_plots_remain_known()
{
    let (mut app, id, actor) = fixture();
    let all = homestead::plots(app.world());
    let (own_id, own_plot) = all[0];
    let (foreign_id, foreign_plot) = all[1];
    visit(&mut app, actor, own_plot);
    HomesteadAct::new(id, "claim_plot", own_id)
        .perform(app.world_mut())
        .unwrap();
    visit(&mut app, actor, foreign_plot);
    app.world_mut()
        .entity_mut(actor)
        .insert(SimPos::new(-100_000, -100_000));
    run_ticks(app.world_mut(), 40);
    let before = row(&look(&app, id), foreign_id);
    assert_eq!(before["state_known"], false);
    assert!(before["owner"].is_null());
    assert!(before["cottage_built"].is_null());
    let ada_id = app
        .world()
        .resource::<testbed_content::TestbedIds>()
        .id("Ada");
    let ada = app.world().resource::<SimIndex>().get(ada_id).unwrap();
    app.world_mut().entity_mut(ada).remove::<Brain>();
    let position = *app.world().get::<SimPos>(foreign_plot).unwrap();
    app.world_mut()
        .entity_mut(ada)
        .insert(position)
        .remove::<MoveIntent>();
    HomesteadAct::new(ada_id, "claim_plot", foreign_id)
        .perform(app.world_mut())
        .unwrap();
    let bed = homestead::bed_position(app.world(), foreign_plot, 0).unwrap();
    app.world_mut().entity_mut(ada).insert(bed);
    economy::grant(app.world_mut(), ada, Item::new(catalog::GRAIN));
    economy::grant(app.world_mut(), ada, Item::new(catalog::WATER));
    HomesteadAct::new(ada_id, "plant", foreign_id)
        .perform(app.world_mut())
        .unwrap();
    HomesteadAct::new(ada_id, "water_crop", foreign_id)
        .perform(app.world_mut())
        .unwrap();
    assert_eq!(
        row(&look(&app, id), foreign_id),
        before,
        "stale foreign knowledge cannot track the new owner or plants"
    );
    app.world_mut()
        .get_mut::<PerceptionMemory>(actor)
        .unwrap()
        .percepts
        .clear();
    let observed = look(&app, id);
    let own = row(&observed, own_id);
    assert_eq!(own["own"], true);
    assert_eq!(own["state_known"], true);
    assert_eq!(own["owner"], json!(id));
    assert!(
        observed["sections"]["homestead"]["plots"]
            .as_array()
            .unwrap()
            .iter()
            .all(|plot| plot["id"] != json!(foreign_id))
    );
}

#[test]
fn bed_indices_and_approach_positions_are_explicit_public_state() {
    let (mut app, id, actor) = fixture();
    let (plot_id, plot) = homestead::plots(app.world())
        .into_iter()
        .find(|(_, plot)| homestead::definition(app.world(), *plot).unwrap().use_kind == "garden")
        .unwrap();
    visit(&mut app, actor, plot);
    HomesteadAct::new(id, "claim_plot", plot_id)
        .perform(app.world_mut())
        .unwrap();
    let observed = look(&app, id);
    let state = row(&observed, plot_id);
    let beds = state["beds"].as_array().unwrap();
    assert_eq!(beds.len(), 6);
    for (index, bed) in beds.iter().enumerate() {
        assert_eq!(bed["index"], index);
        assert_eq!(
            bed["position"],
            json!(homestead::bed_position(app.world(), plot, index as u32).unwrap())
        );
        assert_eq!(bed["stage"], "empty");
    }
}
