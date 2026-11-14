//! Colony policies, physical social contacts and safe replayable terraforming.
use bevy_app::App;
use island_content::{
    IslandIds,
    village::{
        self, VillageState, Vitality,
        colony::{self, AwaitingArrival, ColonyAct, ColonyState, JoinIsland, SocialLife},
    },
};
use npc_framework::prelude::*;

fn scene() -> App {
    island_content::new_sim_app(7, "living_world")
}
fn visitor(app: &App) -> (SimId, bevy_ecs::entity::Entity) {
    let id = app.world().resource::<VillageState>().played;
    (id, app.world().resource::<SimIndex>().get(id).unwrap())
}

#[test]
fn social_relationships_require_real_proximity_and_obey_gathering_policy() {
    let mut app = scene();
    let ids = app.world().resource::<IslandIds>();
    let ada = ids.id("Ada");
    let rowan = ids.id("Rowan");
    let a = app.world().resource::<SimIndex>().get(ada).unwrap();
    let b = app.world().resource::<SimIndex>().get(rowan).unwrap();
    // Fixture relocation isolates proximity; no production/survival claim.
    app.world_mut().entity_mut(a).insert(SimPos::new(100, 100));
    app.world_mut().entity_mut(b).insert(SimPos::new(200, 100));
    // A resident meets again only `schedule.meet_cooldown_ticks` (2400)
    // after its last meeting (closing fix: it was 300).
    app.world_mut().resource_mut::<SimClock>().tick = 2_400;
    colony::tick(app.world_mut());
    // A meeting lasts `schedule.chat_ticks` (200) and pays at its end
    // (review fix S3.3) and pays `schedule.meet_affinity` (1500): advance to its
    // end and settle it.
    app.world_mut().resource_mut::<SimClock>().tick = 2_600;
    colony::tick(app.world_mut());
    assert_eq!(
        app.world()
            .get::<SocialLife>(a)
            .unwrap()
            .affinity
            .get(&rowan),
        Some(&1500)
    );
    assert_eq!(
        app.world().get::<SocialLife>(b).unwrap().affinity.get(&ada),
        Some(&1500)
    );
    let events = app.world().resource::<ColonyState>().social_events;
    ColonyAct {
        verb: "gatherings".into(),
        resident: None,
        house: None,
        value: 0,
    }
    .perform(app.world_mut())
    .unwrap();
    app.world_mut().resource_mut::<SimClock>().tick = 5_100;
    colony::tick(app.world_mut());
    assert_eq!(app.world().resource::<ColonyState>().social_events, events);
    assert!(colony::social_target(app.world(), a).is_none());
}

#[test]
fn registered_actor_can_observe_and_queue_arrival_through_the_shared_resolver() {
    use npc_framework::agent_io::{api, registration};
    use serde_json::json;
    let mut app = scene();
    let (id, actor) = visitor(&app);
    app.insert_resource(registration::RegistrationRequired);
    registration::set_credential(
        app.world_mut(),
        "colony-fixture",
        registration::Credential {
            token_digest: registration::token_digest("public-colony-test-token"),
            assigned: id,
            controller: "visitor".into(),
        },
    );
    let session = api::call(
        app.world_mut(),
        "agent/register",
        Some(json!({
            "agent_id":"colony-fixture", "token":"public-colony-test-token"
        })),
    )
    .unwrap()["session"]
        .as_str()
        .unwrap()
        .to_owned();
    for method in ["agent/enter_world", "agent/take_control"] {
        api::call(app.world_mut(), method, Some(json!({"session":session}))).unwrap();
    }
    run_ticks(app.world_mut(), 1);
    let observed = api::call(
        app.world_mut(),
        "agent/observe",
        Some(json!({"session":session})),
    )
    .unwrap();
    assert!(
        observed["affordances"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["verb"] == "join_island" && a["ok"] == true)
    );
    let result = api::call(
        app.world_mut(),
        "agent/act",
        Some(json!({"session":session,"verb":"join_island"})),
    )
    .unwrap();
    assert_eq!(result["accepted"], true, "{result}");
    assert!(
        app.world().get::<Vitality>(actor).is_none(),
        "arrival must remain queued until a simulation tick"
    );
    run_ticks(app.world_mut(), 1);
    assert!(village::living(app.world(), actor));
    assert!(app.world().get::<AwaitingArrival>(actor).is_none());
}

#[test]
fn successor_life_joins_social_and_housing_state_without_reviving_previous_life() {
    let mut app = scene();
    let (old, actor) = visitor(&app);
    JoinIsland { unit: old.0 }.perform(app.world_mut()).unwrap();
    app.world_mut().get_mut::<Vitality>(actor).unwrap().died_at = Some(0);
    village::VillageAct::new(old, "new_life")
        .perform(app.world_mut())
        .unwrap();
    let (new, successor) = visitor(&app);
    assert_ne!(old, new);
    assert!(app.world().get::<SocialLife>(successor).is_some());
    assert!(colony::home_of(app.world(), new).is_some());
    assert_eq!(app.world().get::<Vitality>(actor).unwrap().died_at, Some(0));
    assert_eq!(
        app.world().get::<village::Wallet>(successor).unwrap().coins,
        0
    );
}
