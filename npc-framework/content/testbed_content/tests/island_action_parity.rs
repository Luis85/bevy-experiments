//! NPC policy and external clients use the same checked island actions.
use bevy_app::App;
use bevy_ecs::prelude::*;
use npc_framework::agent_io::{api, registration};
use npc_framework::interaction::items;
use npc_framework::kernel::hash::state_hash;
use npc_framework::prelude::*;
use serde_json::json;
use testbed_content::village::{
    self, CoinPurse, DroppedDiary, Gathering, VillageAct, Villager, Vitality, Wallet,
};

fn scene() -> App {
    testbed_content::new_sim_app(7, "ai_village")
}

fn person(app: &App, name: &str) -> (SimId, Entity) {
    let id = app
        .world()
        .resource::<testbed_content::TestbedIds>()
        .id(name);
    (id, app.world().resource::<SimIndex>().get(id).unwrap())
}

#[test]
fn visitor_and_resident_can_rest_at_their_own_home_under_identical_rules() {
    for name in ["human", "Ada"] {
        let mut app = scene();
        let (id, actor) = person(&app, name);
        let home = app.world().get::<Villager>(actor).unwrap().home;
        let coins = app.world().get::<Wallet>(actor).unwrap().coins;
        let rest = VillageAct::new(id, "rest_home");
        app.world_mut()
            .entity_mut(actor)
            .insert(SimPos::new(home.x + 801, home.z));
        let before = state_hash(app.world_mut());
        assert!(rest.perform(app.world_mut()).unwrap_err().contains("home"));
        assert_eq!(state_hash(app.world_mut()), before, "failed rest is atomic");
        app.world_mut()
            .entity_mut(actor)
            .insert(SimPos::new(home.x + 800, home.z));
        rest.perform(app.world_mut()).unwrap();
        let v = app.world().get::<Villager>(actor).unwrap();
        assert_eq!(v.resting_until, 300);
        assert_eq!(v.rest_rate, 400);
        assert_eq!(app.world().get::<Wallet>(actor).unwrap().coins, coins);
        assert!(
            rest.check(app.world()).is_err(),
            "rest cannot extend itself for free"
        );
    }
}

#[test]
fn ground_rest_is_slow_cancels_work_and_can_be_stopped_for_every_actor() {
    for name in ["human", "Ada"] {
        let mut app = scene();
        let (id, actor) = person(&app, name);
        app.world_mut().entity_mut(actor).insert((
            MoveIntent::to(SimPos::new(5000, 5000), 70),
            Gathering {
                patch: SimId(0),
                ends: 999,
            },
        ));
        VillageAct::new(id, "rest_ground")
            .perform(app.world_mut())
            .unwrap();
        let v = app.world().get::<Villager>(actor).unwrap();
        assert_eq!((v.resting_until, v.rest_rate), (300, 40));
        assert!(app.world().get::<Gathering>(actor).is_none());
        assert!(app.world().get::<MoveIntent>(actor).is_none());
        assert!(
            VillageAct::new(id, "rest_ground")
                .check(app.world())
                .is_err()
        );
        VillageAct::new(id, "stop_work")
            .perform(app.world_mut())
            .unwrap();
        assert_eq!(app.world().get::<Villager>(actor).unwrap().resting_until, 0);
        app.world_mut().get_mut::<Vitality>(actor).unwrap().died_at = Some(0);
        assert!(
            VillageAct::new(id, "rest_ground")
                .check(app.world())
                .is_err()
        );
        assert!(VillageAct::new(id, "rest_home").check(app.world()).is_err());
    }
}

fn dropped_diary(app: &mut App, pos: SimPos) -> (SimId, Entity) {
    spawn_sim(
        app.world_mut(),
        (
            pos,
            Item::new(village::catalog::DIARY),
            DroppedDiary {
                author: SimId(1),
                author_name: "A prior traveller".into(),
                life: 1,
                died_at: 0,
                pages: Vec::new(),
                sealed: true,
            },
        ),
    )
}

#[test]
fn visitor_and_resident_recover_diaries_with_the_same_reach_and_ownership_checks() {
    for name in ["human", "Ada"] {
        let mut app = scene();
        let (id, actor) = person(&app, name);
        let pos = *app.world().get::<SimPos>(actor).unwrap();
        let (diary, item) = dropped_diary(&mut app, SimPos::new(pos.x + 1501, pos.z));
        let mut recover = VillageAct::new(id, "recover_diary");
        recover.target = Some(diary);
        assert!(recover.perform(app.world_mut()).is_err());
        app.world_mut()
            .entity_mut(item)
            .insert(SimPos::new(pos.x + 1500, pos.z));
        recover.perform(app.world_mut()).unwrap();
        assert_eq!(app.world().get::<CarriedBy>(item).unwrap().holder, id);
        assert!(app.world().get::<SimPos>(item).is_none());
        assert!(
            recover.check(app.world()).is_err(),
            "already carried is not a dropped diary"
        );
        let other = person(&app, if name == "human" { "Ada" } else { "human" }).0;
        let mut stolen = VillageAct::new(other, "recover_diary");
        stolen.target = Some(diary);
        assert!(
            stolen.check(app.world()).is_err(),
            "recovery cannot steal carried diaries"
        );
    }
}

#[test]
fn diary_recovery_respects_inventory_capacity_without_moving_the_item() {
    let mut app = scene();
    let (id, actor) = person(&app, "human");
    let pos = *app.world().get::<SimPos>(actor).unwrap();
    let (diary, item) = dropped_diary(&mut app, pos);
    app.world_mut()
        .entity_mut(actor)
        .insert(Inventory::new(0, 0));
    let mut recover = VillageAct::new(id, "recover_diary");
    recover.target = Some(diary);
    let before = state_hash(app.world_mut());
    assert!(recover.perform(app.world_mut()).is_err());
    assert_eq!(state_hash(app.world_mut()), before);
    assert!(app.world().get::<CarriedBy>(item).is_none());
    assert_eq!(app.world().get::<SimPos>(item), Some(&pos));
}

#[test]
fn visitor_and_resident_redeem_carried_coins_without_ghost_inventory_slots() {
    for name in ["human", "Ada"] {
        let mut app = scene();
        let (id, actor) = person(&app, name);
        let coins = app.world().get::<Wallet>(actor).unwrap().coins;
        let (purse, item) = spawn_sim(
            app.world_mut(),
            (Item::new(village::catalog::COINS), CoinPurse { coins: 7 }),
        );
        items::put_in_inventory(app.world_mut(), actor, item);
        let mut collect = VillageAct::new(id, "collect_coins");
        collect.target = Some(purse);
        collect.perform(app.world_mut()).unwrap();
        assert_eq!(app.world().get::<Wallet>(actor).unwrap().coins, coins + 7);
        assert!(!app.world().get::<Inventory>(actor).unwrap().contains(purse));
        assert!(!app.world().entities().contains(item));
    }
}

#[test]
fn external_client_can_submit_every_new_shared_action_as_recorded_input() {
    for verb in ["rest_home", "rest_ground", "recover_diary"] {
        let mut app = scene();
        let (id, actor) = person(&app, "human");
        let home = app.world().get::<Villager>(actor).unwrap().home;
        app.world_mut().entity_mut(actor).insert(home);
        let target = if verb == "recover_diary" {
            Some(dropped_diary(&mut app, home).0)
        } else {
            None
        };
        app.insert_resource(registration::RegistrationRequired);
        registration::set_credential(
            app.world_mut(),
            "parity-fixture",
            registration::Credential {
                token_digest: registration::token_digest("public-test-token"),
                assigned: id,
                controller: "visitor".into(),
            },
        );
        let session = api::call(
            app.world_mut(),
            "agent/register",
            Some(json!({"agent_id":"parity-fixture", "token":"public-test-token"})),
        )
        .unwrap()["session"]
            .as_str()
            .unwrap()
            .to_owned();
        for method in ["agent/enter_world", "agent/take_control"] {
            api::call(app.world_mut(), method, Some(json!({"session":session}))).unwrap();
        }
        run_ticks(app.world_mut(), 1);
        let reply = api::call(
            app.world_mut(),
            "agent/act",
            Some(json!({
                "session":session, "verb":verb, "target":target,
            })),
        )
        .unwrap();
        assert_eq!(reply["accepted"], true, "{verb}: {reply}");
        // Planning does not mutate the actor; a recorded command applies on the sim tick.
        assert_eq!(app.world().get::<Villager>(actor).unwrap().resting_until, 0);
        run_ticks(app.world_mut(), 1);
        if let Some(target) = target {
            let item = app.world().resource::<SimIndex>().get(target).unwrap();
            assert_eq!(app.world().get::<CarriedBy>(item).unwrap().holder, id);
        } else {
            assert!(app.world().get::<Villager>(actor).unwrap().resting_until >= 300);
        }
    }
}
