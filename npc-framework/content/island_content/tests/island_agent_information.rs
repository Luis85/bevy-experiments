//! Registered, perception-scoped onboarding data. Fixture relocation is operator setup only.
use bevy_app::App;
use island_content::village::{self, Role, Wallet};
use npc_framework::agent_io::{api, registration};
use npc_framework::prelude::*;
use serde_json::{Value, json};

fn entity(app: &App, id: SimId) -> bevy_ecs::entity::Entity {
    app.world().resource::<SimIndex>().get(id).unwrap()
}
fn id(app: &App, name: &str) -> SimId {
    app.world()
        .resource::<island_content::TestbedIds>()
        .id(name)
}
fn call(app: &mut App, method: &str, p: Value) -> Value {
    api::call(app.world_mut(), method, Some(p)).unwrap()
}
fn fixture() -> (App, SimId, String) {
    let mut app = island_content::new_sim_app(7, "ai_village");
    let actor = id(&app, "human");
    app.insert_resource(registration::RegistrationRequired);
    registration::set_credential(
        app.world_mut(),
        "onboarding",
        registration::Credential {
            token_digest: registration::token_digest("disposable-test-token"),
            assigned: actor,
            controller: "visitor".into(),
        },
    );
    let session = call(
        &mut app,
        "agent/register",
        json!({"agent_id":"onboarding","token":"disposable-test-token"}),
    )["session"]
        .as_str()
        .unwrap()
        .to_owned();
    call(&mut app, "agent/enter_world", json!({"session":session}));
    call(&mut app, "agent/take_control", json!({"session":session}));
    run_ticks(app.world_mut(), 1); // Operator clock, never an agent request.
    (app, actor, session)
}
fn stand_by(app: &mut App, actor: SimId, merchant: SimId) {
    let body = entity(app, actor);
    // The operator keeps this information fixture alongside a moving trader through
    // a perception scan. This is not economic-route or unaided-movement evidence.
    for _ in 0..40 {
        let pos = *app.world().get::<SimPos>(entity(app, merchant)).unwrap();
        app.world_mut().entity_mut(body).insert(pos);
        run_ticks(app.world_mut(), 1);
    }
    let pos = *app.world().get::<SimPos>(entity(app, merchant)).unwrap();
    assert!(app.world().get::<SimPos>(body).unwrap().dist2(pos) <= 2_250_000);
}

#[test]
fn empty_pack_discovers_public_demand_and_concrete_refusals_without_other_wallets() {
    let (mut app, actor, session) = fixture();
    let miller = id(&app, Role::Miller.person());
    stand_by(&mut app, actor, miller);
    let observed = call(&mut app, "agent/observe", json!({"session":session}));
    let merchant = observed["sections"]["village"]["nearby_market"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] == json!(miller))
        .expect("near perceived miller");
    let grain = merchant["offers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["direction"] == "sell" && o["kind"] == village::catalog::GRAIN)
        .expect("demand discoverable with empty pack");
    assert_eq!(
        grain["unit_price"],
        village::catalog::price(village::catalog::GRAIN).unwrap()
    );
    assert_eq!(grain["available"], false);
    assert!(grain["reason"].as_str().unwrap().contains("unreserved"));
    assert!(grain["demand"].as_u64().unwrap() > 0);
    assert!(merchant.get("coins").is_none());
    let body = entity(&app, miller);
    app.world_mut().get_mut::<Wallet>(body).unwrap().coins = 0;
    let observed = call(&mut app, "agent/observe", json!({"session":session}));
    let market = &observed["sections"]["village"]["nearby_market"];
    assert!(market.to_string().contains("merchant cannot afford"));
    assert!(!market.to_string().contains("buyer has"));
}

#[test]
fn unseen_merchants_and_other_actors_control_are_not_exposed() {
    let (mut app, actor, session) = fixture();
    let observed = call(&mut app, "agent/observe", json!({"session":session}));
    assert!(
        observed["sections"]["village"]["nearby_market"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let miller = id(&app, Role::Miller.person());
    let rejected = call(
        &mut app,
        "agent/act",
        json!({"session":session,"verb":"buy","target":miller,"args":{"kind":village::catalog::FLOUR}}),
    );
    assert_eq!(rejected["accepted"], false);
    let unauthorized = api::call(
        app.world_mut(),
        "agent/observe",
        Some(json!({"session":session,"unit":miller})),
    );
    assert!(unauthorized.is_err());
    assert_ne!(actor, miller);
}

#[test]
fn activity_distinguishes_gathering_and_explains_cancellation() {
    let (mut app, actor, session) = fixture();
    let body = entity(&app, actor);
    app.world_mut().entity_mut(body).insert(village::Gathering {
        patch: actor,
        ends: 1000,
    });
    let observed = call(&mut app, "agent/observe", json!({"session":session}));
    let activity = &observed["sections"]["village"]["activity"];
    assert_eq!(activity["kind"], "gathering");
    assert_eq!(activity["busy"], true);
    assert!(activity["recovery"].as_str().unwrap().contains("stop_work"));
}

#[test]
fn island_handbook_objective_is_specific_without_changing_other_worlds() {
    let island = island_content::worlds::builtin_preset("island_village").unwrap();
    let rules = island_content::explorer::workspace::render_world_rules(&island);
    for text in [
        "earn a living",
        "Permanent death",
        "Diary",
        "operator",
        "never advance ticks",
        "Free gathering",
        "remaining reserves",
        "accepted action is queued",
        "turn ending does not pause",
    ] {
        assert!(rules.contains(text), "missing {text}: {rules}");
    }
    assert!(!rules.contains("your character collapses or shuts down"));
    assert!(!rules.contains("Ada") && !rules.contains("Rowan"));
    let legacy = island_content::worlds::builtin_preset("plain_explorer").unwrap();
    let rules = island_content::explorer::workspace::render_world_rules(&legacy);
    assert!(!rules.contains("earn a living") && !rules.contains("scheduler"));
    assert!(rules.contains("No survival needs are enabled."));
}

#[test]
fn depleted_shop_explains_unavailability_and_duplicate_stacks_do_not_duplicate_demand() {
    let (mut app, actor, session) = fixture();
    let baker = id(&app, Role::Baker.person());
    stand_by(&mut app, actor, baker);
    let merchant = entity(&app, baker);
    let body = entity(&app, actor);
    app.world_mut().get_mut::<Wallet>(body).unwrap().coins = 100;
    let count = village::economy::count(app.world(), merchant, "bread");
    village::economy::consume(app.world_mut(), merchant, "bread", count).unwrap();
    let observed = call(&mut app, "agent/observe", json!({"session":session}));
    let offers = observed["sections"]["village"]["nearby_market"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] == json!(baker))
        .unwrap()["offers"]
        .as_array()
        .unwrap();
    let bread = offers
        .iter()
        .find(|o| o["direction"] == "buy" && o["kind"] == "bread")
        .unwrap();
    assert_eq!(bread["stock"], 0);
    assert_eq!(bread["available"], false);
    assert!(
        bread["reason"]
            .as_str()
            .unwrap()
            .contains("unreserved bread")
    );
    let demand: Vec<_> = offers
        .iter()
        .filter(|o| o["direction"] == "sell")
        .map(|o| o["kind"].as_str().unwrap())
        .collect();
    let unique: std::collections::BTreeSet<_> = demand.iter().copied().collect();
    assert_eq!(demand.len(), unique.len());
}

#[test]
fn remembered_patch_stock_and_depletion_are_private_until_sensed_or_reached() {
    let (mut app, actor, session) = fixture();
    let patch = id(&app, "patch_log");
    stand_by(&mut app, actor, patch);
    let patch_entity = entity(&app, patch);
    app.world_mut()
        .get_mut::<village::ResourcePatch>(patch_entity)
        .unwrap()
        .remaining = 17;
    let observed = call(&mut app, "agent/observe", json!({"session":session}));
    let gather = |observation: &Value| {
        observation["affordances"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["verb"] == "gather" && a["target"] == json!(patch))
            .expect("known patch offers gathering")
            .clone()
    };
    assert!(
        gather(&observed)["label"]
            .as_str()
            .unwrap()
            .contains("17 left")
    );

    // Operator-only relocation and stock changes create an information-isolation
    // fixture, not evidence of unaided movement or economic play.
    let body = entity(&app, actor);
    app.world_mut()
        .entity_mut(body)
        .insert(SimPos::new(-100_000, -100_000));
    run_ticks(app.world_mut(), 40);
    let before = call(&mut app, "agent/observe", json!({"session":session}));
    assert!(
        before["sections"]["perception"]["remembered"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["id"] == json!(patch))
    );
    assert!(
        !before["sections"]["perception"]["sensed"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["id"] == json!(patch))
    );
    let before_affordance = gather(&before);
    assert!(
        before_affordance["label"]
            .as_str()
            .unwrap()
            .contains("approach to check stock")
    );
    assert_eq!(before_affordance["ok"], false);
    let request = json!({"session":session,"verb":"gather","target":patch});
    let stocked_refusal = call(&mut app, "agent/act", request.clone());
    assert_eq!(stocked_refusal["accepted"], false);
    assert!(
        stocked_refusal["message"]
            .as_str()
            .unwrap()
            .contains("move within 1.5 m")
    );

    app.world_mut()
        .get_mut::<village::ResourcePatch>(patch_entity)
        .unwrap()
        .remaining = 0;
    let after = call(&mut app, "agent/observe", json!({"session":session}));
    assert_eq!(gather(&after), before_affordance);
    let depleted_refusal = call(&mut app, "agent/act", request);
    assert_eq!(depleted_refusal["accepted"], false);
    assert_eq!(depleted_refusal["message"], stocked_refusal["message"]);

    stand_by(&mut app, actor, patch);
    let nearby = call(&mut app, "agent/observe", json!({"session":session}));
    assert!(
        gather(&nearby)["label"]
            .as_str()
            .unwrap()
            .contains("0 left")
    );
    let depleted = call(
        &mut app,
        "agent/act",
        json!({"session":session,"verb":"gather","target":patch}),
    );
    assert_eq!(depleted["accepted"], false);
    assert!(depleted["message"].as_str().unwrap().contains("depleted"));
}

#[test]
fn commands_queued_before_renting_a_bed_recheck_rest_at_application() {
    let (mut app, actor, _) = fixture();
    let patch = id(&app, "patch_log");
    let innkeeper = id(&app, Role::Innkeeper.person());
    stand_by(&mut app, actor, patch);
    let body = entity(&app, actor);
    let keeper = entity(&app, innkeeper);
    let pos = *app.world().get::<SimPos>(body).unwrap();
    // Operator fixture puts both valid targets within reach before submission.
    app.world_mut().entity_mut(keeper).insert(pos);
    app.world_mut().get_mut::<Wallet>(body).unwrap().coins = 100;
    let mut rest = village::VillageAct::new(actor, "rest");
    rest.target = Some(innkeeper);
    let mut gather = village::VillageAct::new(actor, "gather");
    gather.target = Some(patch);
    submit(app.world_mut(), None, SimCommand::content(&rest)).unwrap();
    submit(app.world_mut(), None, SimCommand::content(&gather)).unwrap();
    run_ticks(app.world_mut(), 1);
    assert!(app.world().get::<village::Gathering>(body).is_none());
    let skipped = app
        .world()
        .resource::<npc_framework::kernel::commands::SkippedCommands>();
    assert!(skipped.entries.iter().any(|entry| {
        entry.cmd == SimCommand::content(&gather)
            && entry.error.to_string().contains("resting until tick")
    }));
    assert_eq!(
        app.world().get::<Wallet>(body).unwrap().coins,
        100 - village::catalog::BED_PRICE
    );
    let stop = village::VillageAct::new(actor, "stop_work");
    assert!(stop.check(app.world()).is_ok());
}

#[test]
fn renting_a_bed_rejects_wallet_overflow_without_charging_the_actor() {
    let (mut app, actor, _) = fixture();
    let innkeeper = id(&app, Role::Innkeeper.person());
    stand_by(&mut app, actor, innkeeper);
    let body = entity(&app, actor);
    let keeper = entity(&app, innkeeper);
    app.world_mut().get_mut::<Wallet>(body).unwrap().coins = 100;
    app.world_mut().get_mut::<Wallet>(keeper).unwrap().coins = u32::MAX;
    let mut rest = village::VillageAct::new(actor, "rest");
    rest.target = Some(innkeeper);
    assert!(
        rest.perform(app.world_mut())
            .unwrap_err()
            .contains("wallet is full")
    );
    assert_eq!(app.world().get::<Wallet>(body).unwrap().coins, 100);
    assert_eq!(app.world().get::<Wallet>(keeper).unwrap().coins, u32::MAX);
}
