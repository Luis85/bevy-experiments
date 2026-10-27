//! Registered control is scoped to the assigned actor, not every unit
//! sharing its player. World targets and the actor's vehicle remain usable.
use bevy_app::App;
use npc_framework::agent_io::{api, registration};
use npc_framework::command::{OrderQueue, create_squad};
use npc_framework::prelude::{MountedIn, PlayerId, SimId, SimIndex, SimPos, run_ticks, state_hash};
use serde_json::{Value, json};
use testbed_content::TestbedIds;

fn entity(app: &App, id: SimId) -> bevy_ecs::entity::Entity {
    app.world().resource::<SimIndex>().get(id).unwrap()
}
fn id(app: &App, name: &str) -> SimId {
    app.world().resource::<TestbedIds>().id(name)
}
fn call(app: &mut App, method: &str, p: Value) -> Value {
    api::call(app.world_mut(), method, Some(p)).unwrap_or_else(|e| panic!("{method}: {e:?}"))
}
fn app(locked: bool) -> (App, SimId, String) {
    let mut app = testbed_content::new_sim_app(1, "vehicles");
    let actor = id(&app, "driver");
    let session = if locked {
        app.insert_resource(registration::RegistrationRequired);
        registration::set_credential(
            app.world_mut(),
            "driver-agent",
            registration::Credential {
                token_digest: registration::token_digest("scope-token"),
                assigned: actor,
                controller: "driver-agent".into(),
            },
        );
        let session = call(
            &mut app,
            "agent/register",
            json!({"agent_id":"driver-agent","token":"scope-token"}),
        )["session"]
            .as_str()
            .unwrap()
            .to_owned();
        call(&mut app, "agent/take_control", json!({"session":session}));
        session
    } else {
        call(
            &mut app,
            "agent/attach",
            json!({"unit":actor.0,"controller":"legacy"}),
        );
        String::new()
    };
    run_ticks(app.world_mut(), 1);
    (app, actor, session)
}

#[test]
fn registered_orders_cannot_control_same_player_units_or_squads() {
    let (mut app, actor, session) = app(true);
    let other = id(&app, "passenger");
    let squad = create_squad(app.world_mut(), PlayerId(1), &[actor, other], None);
    for recipient in [other, squad] {
        let entity = entity(&app, other);
        let before_queue = serde_json::to_value(app.world().get::<OrderQueue>(entity)).unwrap();
        let before = state_hash(app.world_mut());
        let commands = api::command_log(app.world()).len();
        let rejected = call(
            &mut app,
            "agent/act",
            json!({"session":session,"verb":"order","args":{"units":[recipient.0],"order":{"type":"Hold"}}}),
        );
        assert_eq!(rejected["accepted"], false);
        assert_eq!(rejected["reason"], "not_yours");
        assert_eq!(state_hash(app.world_mut()), before);
        assert_eq!(api::command_log(app.world()).len(), commands);
        assert_eq!(
            serde_json::to_value(app.world().get::<OrderQueue>(entity)).unwrap(),
            before_queue
        );
    }
    let observed = call(&mut app, "agent/observe", json!({"session":session}));
    assert_eq!(
        observed["sections"]["command"]["owned_units"],
        json!([actor.0])
    );
}

#[test]
fn unlocked_same_player_orders_keep_their_existing_behavior() {
    let (mut app, actor, _) = app(false);
    let other = id(&app, "passenger");
    let accepted = call(
        &mut app,
        "agent/act",
        json!({"unit":actor.0,"controller":"legacy","verb":"order","args":{"units":[other.0],"order":{"type":"Hold"}}}),
    );
    assert_eq!(accepted["accepted"], true, "{accepted}");
}

#[test]
fn registered_actor_can_mount_and_drive_its_vehicle_through_own_orders() {
    let (mut app, actor, session) = app(true);
    let cart = id(&app, "cart");
    let mount = call(
        &mut app,
        "agent/act",
        json!({"session":session,"steps":[
            {"verb":"order","args":{"order":{"type":"Mount","vehicle":cart.0}}},
            {"verb":"execute_orders"}
        ]}),
    );
    assert_eq!(mount["accepted"], true, "{mount}");
    run_ticks(app.world_mut(), 200);
    let e = entity(&app, actor);
    assert_eq!(app.world().get::<MountedIn>(e).unwrap().vehicle, cart);
    let cart_e = entity(&app, cart);
    let old = *app.world().get::<SimPos>(cart_e).unwrap();
    let drive = call(
        &mut app,
        "agent/act",
        json!({"session":session,"steps":[
            {"verb":"order","args":{"order":{"type":"Move","pos":[4500,-1500]}}},
            {"verb":"execute_orders"}
        ]}),
    );
    assert_eq!(drive["accepted"], true, "{drive}");
    run_ticks(app.world_mut(), 100);
    assert_ne!(*app.world().get::<SimPos>(cart_e).unwrap(), old);
    assert_eq!(
        *app.world().get::<SimPos>(cart_e).unwrap(),
        SimPos::new(4500, -1500)
    );
}
