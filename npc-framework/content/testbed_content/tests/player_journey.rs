//! Guidance projects real current evidence; it grants nothing and invents no targets.
use npc_framework::agent_io::observe;
use npc_framework::kernel::hash::state_hash;
use npc_framework::prelude::*;
use serde_json::{Value, json};
use testbed_content::village::{
    self, catalog,
    colony::{JoinIsland, SocialLife},
    player_journey,
};

fn observation() -> Value {
    json!({"tick":100,"sections":{
        "village":{"alive":true,"needs":{"hunger":100000,"thirst":100000,"energy":100000},
            "home":{"x":500,"z":1500},"activity":{"kind":"idle","busy":false},
            "environment":{"resources":[],"workstations":[]},"nearby_market":[],"economy":{"own_offers":[]}},
        "inventory":{"items":[{"kind":catalog::WATER,"stack":2},{"kind":catalog::BERRIES,"stack":3}]},
        "homestead":{"plots":[],"building":null,
            "cottage":{"inputs":[{"kind":"plank","count":8},{"kind":"log","count":4}],"duration_ticks":600},
            "crop":{"seed_kind":catalog::GRAIN},
            "crops":[{"kind":catalog::GRAIN},{"kind":catalog::BERRIES}]}
    }})
}
fn add_stock(obs: &mut Value, kind: &str, stack: u32) {
    obs["sections"]["inventory"]["items"]
        .as_array_mut()
        .unwrap()
        .push(json!({"kind":kind,"stack":stack}));
}
fn house(built: bool) -> Value {
    json!({"id":300,"own":true,"state_known":true,"use_kind":"housing","owner":126,"cottage_built":built,
        "front_door":{"x":-4000,"z":-16000},"beds":[{"index":0,"position":{"x":-4000,"z":-14600},"stage":"empty","kind":null}]})
}

#[test]
fn unknown_resources_never_turn_into_privileged_targets() {
    let mut obs = observation();
    let journey = player_journey::project(&obs);
    assert_eq!(journey.next.id, "timber");
    assert_eq!(journey.next.progress.unwrap().target, 6);
    assert!(journey.next.action.is_none());
    assert_eq!(player_journey::project(&obs), player_journey::project(&obs));
    obs["sections"]["village"]["environment"]["resources"] =
        json!([{"id":40,"kind":"log","remaining":10}]);
    let next = player_journey::project(&obs).next;
    assert_eq!(next.progress.unwrap().target, 6);
    assert_eq!(
        next.action.unwrap().args["quantity"],
        1,
        "the material goal needs six logs, while each gather request collects one"
    );
}

#[test]
fn preparation_keeps_actual_house_logs_and_reuses_known_recipe_targets() {
    let mut obs = observation();
    add_stock(&mut obs, "plank", 4);
    add_stock(&mut obs, "log", 4);
    obs["sections"]["village"]["environment"]["resources"] =
        json!([{"id":40,"kind":"log","remaining":10}]);
    let next = player_journey::project(&obs).next;
    assert_eq!(next.id, "timber");
    assert_eq!(next.progress.unwrap().target, 5);
    assert_eq!(next.action.unwrap().args["quantity"], 1);
    add_stock(&mut obs, "log", 1);
    obs["sections"]["village"]["environment"]["workstations"] =
        json!([{"id":60,"recipes":[{"name":catalog::SAW}]}]);
    let next = player_journey::project(&obs).next;
    assert_eq!(next.id, "planks");
    assert_eq!(next.action.unwrap().target, Some(SimId(60)));
}

#[test]
fn claiming_and_building_use_observed_doors_and_never_offer_living_foreign_estates() {
    let mut obs = observation();
    add_stock(&mut obs, "log", 4);
    add_stock(&mut obs, "plank", 8);
    let mut plot = house(false);
    plot["own"] = false.into();
    plot["owner_alive"] = true.into();
    obs["sections"]["homestead"]["plots"] = json!([plot]);
    assert!(player_journey::project(&obs).next.action.is_none());
    obs["sections"]["homestead"]["plots"][0]["owner_alive"] = false.into();
    let next = player_journey::project(&obs).next;
    assert_eq!(next.id, "claim");
    assert_eq!(
        next.action.unwrap().position,
        Some(SimPos::new(-4000, -16000))
    );
    obs["sections"]["homestead"]["plots"][0]["own"] = true.into();
    let next = player_journey::project(&obs).next;
    assert_eq!(next.id, "cottage");
    assert_eq!(
        next.action.unwrap().position,
        Some(SimPos::new(-4000, -16000))
    );
}

#[test]
fn completed_home_and_reserved_work_do_not_restart_the_material_tutorial() {
    let mut obs = observation();
    obs["sections"]["homestead"]["plots"] = json!([house(true)]);
    let journey = player_journey::project(&obs);
    assert!(journey.home_built);
    assert_eq!(journey.next.id, "plant");
    assert_eq!(journey.next.action.unwrap().args["kind"], catalog::BERRIES);
    obs["sections"]["homestead"]["plots"][0]["cottage_built"] = false.into();
    obs["sections"]["homestead"]["building"] = json!({"plot":300,"ends":400});
    obs["sections"]["village"]["activity"]["busy"] = true.into();
    let next = player_journey::project(&obs).next;
    assert_eq!(next.id, "building");
    assert_eq!(next.progress.unwrap().current, 300);
    assert!(next.action.is_none());
}

#[test]
fn an_active_garden_is_acknowledged_before_expansion_and_keeps_tending_priorities() {
    let mut obs = observation();
    let mut home = house(true);
    home["beds"] = json!([
        {"index":0,"position":[-4000,-14600],"stage":"growing","kind":catalog::BERRIES},
        {"index":1,"position":[-3000,-14600],"stage":"empty","kind":null}
    ]);
    obs["sections"]["homestead"]["plots"] = json!([home]);
    let next = player_journey::project(&obs).next;
    assert_eq!(next.id, "plant");
    assert_eq!(next.title, "Plant another bed");
    assert_eq!(next.action.unwrap().args["bed"], 1);
    obs["sections"]["homestead"]["plots"][0]["beds"][0]["stage"] = "ready".into();
    let next = player_journey::project(&obs).next;
    assert_eq!(next.id, "harvest");
    assert_eq!(next.action.unwrap().args["bed"], 0);
    obs["sections"]["homestead"]["plots"][0]["beds"][0]["stage"] = "needs_water".into();
    assert_eq!(player_journey::project(&obs).next.id, "water_crop");
    obs["sections"]["village"]["needs"]["thirst"] = 10_000.into();
    assert_eq!(player_journey::project(&obs).next.id, "drink");
    obs["sections"]["village"]["needs"]["thirst"] = 100_000.into();
    obs["sections"]["inventory"]["items"] = json!([{"kind":catalog::BERRIES,"stack":3}]);
    assert_eq!(player_journey::project(&obs).next.id, "water");
}

#[test]
fn seed_guidance_does_not_call_a_growing_legacy_grain_garden_unstarted() {
    let mut obs = observation();
    obs["sections"]["homestead"]
        .as_object_mut()
        .unwrap()
        .remove("crops");
    let mut home = house(true);
    home["beds"] = json!([
        {"index":0,"stage":"growing","kind":catalog::GRAIN},
        {"index":1,"stage":"empty","kind":null}
    ]);
    obs["sections"]["homestead"]["plots"] = json!([home]);
    let next = player_journey::project(&obs).next;
    assert_eq!(next.id, "seed");
    assert_eq!(next.title, "Find seed for another bed");
    assert!(next.action.is_none());
}

#[test]
fn moderate_needs_preserve_current_work_and_critical_needs_request_an_explicit_stop() {
    for need in ["thirst", "hunger"] {
        for building in [false, true] {
            let mut obs = observation();
            obs["sections"]["village"]["activity"]["busy"] = true.into();
            obs["sections"]["village"]["needs"][need] = 20_000.into();
            if building {
                obs["sections"]["homestead"]["building"] = json!({"plot":300,"ends":400});
            }
            let next = player_journey::project(&obs).next;
            assert_eq!(next.id, if building { "building" } else { "working" });
            assert!(next.action.is_none());
            obs["sections"]["village"]["needs"][need] = 14_999.into();
            assert_eq!(
                player_journey::project(&obs).next.action.unwrap().verb,
                "stop_work"
            );
            obs["sections"]["village"]["activity"]["busy"] = false.into();
            obs["sections"]["village"]["needs"][need] = 20_000.into();
            assert_eq!(
                player_journey::project(&obs).next.action.unwrap().verb,
                if need == "thirst" { "drink" } else { "eat" }
            );
            obs["sections"]["inventory"]["items"] = json!([]);
            let kind = if need == "thirst" {
                catalog::WATER
            } else {
                catalog::BERRIES
            };
            obs["sections"]["village"]["environment"]["resources"] =
                json!([{"id":40,"kind":kind,"remaining":10}]);
            let next = player_journey::project(&obs).next;
            assert_eq!(next.action.unwrap().verb, "gather");
        }
    }
}

#[test]
fn urgent_needs_precede_work_and_legacy_crops_stay_grain_only() {
    let mut obs = observation();
    obs["sections"]["village"]["activity"]["busy"] = true.into();
    obs["sections"]["village"]["needs"]["thirst"] = 10_000.into();
    assert_eq!(
        player_journey::project(&obs).next.action.unwrap().verb,
        "stop_work"
    );
    obs["sections"]["village"]["activity"]["busy"] = false.into();
    assert_eq!(
        player_journey::project(&obs).next.action.unwrap().verb,
        "drink"
    );
    obs["sections"]["village"]["needs"]["thirst"] = 100_000.into();
    obs["sections"]["homestead"]
        .as_object_mut()
        .unwrap()
        .remove("crops");
    obs["sections"]["homestead"]["plots"] = json!([house(true)]);
    assert_eq!(player_journey::project(&obs).next.id, "seed");
}

#[test]
fn guidance_and_social_observations_are_pure_and_withhold_unknown_neighbors() {
    let mut app = testbed_content::new_sim_app(7, "island_colony");
    let me = app.world().resource::<village::VillageState>().played;
    JoinIsland { unit: me.0 }.perform(app.world_mut()).unwrap();
    let actor = app.world().resource::<SimIndex>().get(me).unwrap();
    let ada = app
        .world()
        .resource::<testbed_content::TestbedIds>()
        .id("Ada");
    app.world_mut()
        .get_mut::<SocialLife>(actor)
        .unwrap()
        .affinity
        .insert(ada, 25_000);
    let before = state_hash(app.world_mut());
    let obs = observe::observe(app.world(), me, 0).unwrap().0;
    let _ = player_journey::project(&obs);
    assert_eq!(
        obs["sections"]["village"]["social"]["relationships"],
        json!([])
    );
    assert_eq!(state_hash(app.world_mut()), before);
    let pos = *app
        .world()
        .get::<SimPos>(app.world().resource::<SimIndex>().get(ada).unwrap())
        .unwrap();
    app.world_mut().entity_mut(actor).insert(pos);
    run_ticks(app.world_mut(), 8);
    let before = state_hash(app.world_mut());
    let obs = observe::observe(app.world(), me, 0).unwrap().0;
    let rows = obs["sections"]["village"]["social"]["relationships"]
        .as_array()
        .unwrap();
    let row = rows.iter().find(|row| row["id"] == json!(ada)).unwrap();
    assert_eq!(row["name"], "Ada");
    assert_eq!(row["affinity"], 25_000);
    for hidden in ["pos", "position", "health", "coins", "inventory"] {
        assert!(row.get(hidden).is_none());
    }
    assert_eq!(state_hash(app.world_mut()), before);
}
