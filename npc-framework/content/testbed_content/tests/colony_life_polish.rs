//! Shared household/social facts stay truthful, private and side-effect free.
use bevy_app::App;
use bevy_ecs::prelude::*;
use npc_framework::agent_io::observe;
use npc_framework::kernel::hash::state_hash;
use npc_framework::needs::{NeedRegistry, Needs};
use npc_framework::prelude::*;
use npc_framework::snapshot;
use serde_json::json;
use testbed_content::{
    TestbedIds,
    village::{self, colony, economy},
};

fn scene() -> App {
    testbed_content::new_sim_app(7, "island_colony")
}
fn actor(app: &App, name: &str) -> (SimId, Entity) {
    let id = app.world().resource::<TestbedIds>().id(name);
    (id, app.world().resource::<SimIndex>().get(id).unwrap())
}
fn set_need(app: &mut App, actor: Entity, name: &str, value: i32) {
    let id = app.world().resource::<NeedRegistry>().id(name).unwrap();
    app.world_mut().get_mut::<Needs>(actor).unwrap().values[id.0 as usize].value = value;
}

#[test]
fn own_heating_projection_is_private_pure_and_matches_real_hearth_consumption() {
    let mut app = scene();
    let id = app.world().resource::<village::VillageState>().played;
    colony::JoinIsland { unit: id.0 }
        .perform(app.world_mut())
        .unwrap();
    let visitor = app.world().resource::<SimIndex>().get(id).unwrap();
    let house = colony::home_of(app.world(), id).unwrap();
    let household = app.world().resource::<SimIndex>().get(house).unwrap();
    let door = app
        .world()
        .get::<colony::Household>(household)
        .unwrap()
        .door;
    let co_resident = *app
        .world()
        .get::<colony::Household>(household)
        .unwrap()
        .residents
        .iter()
        .find(|resident| **resident != id)
        .unwrap();
    let co_resident = app.world().resource::<SimIndex>().get(co_resident).unwrap();
    // Another member's provisions are not the visitor's usable carried fuel.
    economy::grant(app.world_mut(), co_resident, Item::new("log").stack(4));
    app.world_mut().entity_mut(visitor).insert(door);
    let day = app.world().resource::<SimConfig>().day_length;
    app.world_mut().resource_mut::<SimClock>().tick = day * 9 / 10;
    let before = state_hash(app.world_mut());
    let empty = colony::home_heating(app.world(), visitor).unwrap();
    assert!(empty.cold && empty.within_reach && !empty.heated && !empty.ready_to_light);
    assert_eq!(empty.carried_fuel, 0);
    assert_eq!(empty.period_ticks, day / 2);
    assert_eq!(empty.reach_mm, 1500);
    let observed = observe::observe(app.world(), id, 0).unwrap().0;
    assert_eq!(
        observed["sections"]["village"]["home_heating"],
        json!(empty)
    );
    assert_eq!(state_hash(app.world_mut()), before);

    economy::grant(app.world_mut(), visitor, Item::new("log").stack(2));
    // Keep the other resident away so the test certifies whose log is spent.
    app.world_mut()
        .entity_mut(co_resident)
        .insert(SimPos::new(30_000, 30_000));
    app.world_mut()
        .entity_mut(visitor)
        .insert(SimPos::new(door.x + 1501, door.z));
    let distant = colony::home_heating(app.world(), visitor).unwrap();
    assert_eq!(distant.carried_fuel, 2);
    assert!(!distant.within_reach && !distant.ready_to_light);
    app.world_mut().entity_mut(visitor).insert(door);
    let ready = colony::home_heating(app.world(), visitor).unwrap();
    assert!(ready.ready_to_light);
    colony::tick(app.world_mut());
    let heated = colony::home_heating(app.world(), visitor).unwrap();
    assert!(heated.heated && !heated.ready_to_light);
    assert_eq!(heated.carried_fuel, 1);
    assert_eq!(heated.remaining_ticks, ready.period_ticks);
    colony::tick(app.world_mut());
    assert_eq!(economy::count(app.world(), visitor, "log"), 1);

    let saved = snapshot::save(app.world_mut()).unwrap();
    let mut resumed = scene();
    let report = snapshot::load(resumed.world_mut(), &saved, snapshot::LoadMode::Strict).unwrap();
    assert!(report.defaulted.is_empty() && report.skipped.is_empty());
    let visitor = resumed.world().resource::<SimIndex>().get(id).unwrap();
    assert_eq!(colony::home_heating(resumed.world(), visitor), Some(heated));
}

#[test]
fn heating_projection_handles_warm_weather_absent_household_and_legacy_world() {
    let mut app = scene();
    let (id, ada) = actor(&app, "Ada");
    let house = colony::home_of(app.world(), id).unwrap();
    let household = app.world().resource::<SimIndex>().get(house).unwrap();
    let door = app
        .world()
        .get::<colony::Household>(household)
        .unwrap()
        .door;
    app.world_mut().entity_mut(ada).insert(door);
    economy::grant(app.world_mut(), ada, Item::new("log"));
    let state = colony::home_heating(app.world(), ada).unwrap();
    assert!(!state.cold && !state.ready_to_light);
    assert!(state.carried_fuel > 0);
    app.world_mut()
        .get_mut::<colony::Household>(household)
        .unwrap()
        .residents
        .clear();
    assert_eq!(colony::home_heating(app.world(), ada), None);
    let legacy = testbed_content::new_sim_app(7, "living_village");
    let (id, ada) = actor(&legacy, "Ada");
    assert_eq!(colony::home_heating(legacy.world(), ada), None);
    let observed = observe::observe(legacy.world(), id, 0).unwrap().0;
    assert!(
        observed["sections"]["village"]
            .get("home_heating")
            .is_none()
    );
}

#[test]
fn social_targets_and_settlement_share_availability_without_interrupting_work() {
    for reason in [
        "resting",
        "gathering",
        "work_approach",
        "Hunger",
        "Thirst",
        "Energy",
        "Warmth",
    ] {
        let mut app = scene();
        let (a_id, a) = actor(&app, "Ada");
        let (b_id, b) = actor(&app, "Rowan");
        // Isolate a pair for the domain query; no pathfinding or survival claim.
        app.world_mut()
            .entity_mut(a)
            .insert(SimPos::new(100_000, 100_000));
        app.world_mut()
            .entity_mut(b)
            .insert(SimPos::new(100_100, 100_000));
        app.world_mut().resource_mut::<SimClock>().tick = 600;
        let patch = app
            .world()
            .resource::<SimIndex>()
            .map
            .iter()
            .find(|(_, entity)| {
                app.world()
                    .get::<village::ResourcePatch>(**entity)
                    .is_some()
            })
            .map(|(id, _)| *id)
            .unwrap();
        let workplace = app.world().get::<village::Villager>(b).unwrap().workplace;
        match reason {
            "resting" => {
                app.world_mut()
                    .get_mut::<village::Villager>(b)
                    .unwrap()
                    .resting_until = 900
            }
            "gathering" => {
                app.world_mut()
                    .entity_mut(b)
                    .insert(village::Gathering { patch, ends: 900 });
            }
            "work_approach" => {
                app.world_mut()
                    .entity_mut(b)
                    .insert(village::production::JobIntent {
                        verb: "craft".into(),
                        good: "flour".into(),
                        recipe: village::catalog::MILL.into(),
                        target: Some(workplace),
                    });
            }
            need => {
                let critical = app
                    .world()
                    .resource::<village::policy::IslandAiPolicy>()
                    .critical_need;
                set_need(&mut app, b, need, critical - 1);
            }
        }
        let before = state_hash(app.world_mut());
        assert!(colony::social_available(app.world(), a));
        assert!(!colony::social_available(app.world(), b), "{reason}");
        assert_eq!(colony::social_target(app.world(), a), None, "{reason}");
        assert_eq!(colony::social_target(app.world(), b), None, "{reason}");
        assert_eq!(state_hash(app.world_mut()), before);
        colony::tick(app.world_mut());
        assert!(
            !app.world()
                .get::<colony::SocialLife>(a)
                .unwrap()
                .affinity
                .contains_key(&b_id),
            "{reason}"
        );
        if reason == "gathering" {
            assert!(app.world().get::<village::Gathering>(b).is_some());
        }
        if reason == "work_approach" {
            assert!(
                app.world()
                    .get::<village::production::JobIntent>(b)
                    .is_some()
            );
        }
        app.world_mut()
            .get_mut::<village::Villager>(b)
            .unwrap()
            .resting_until = 0;
        app.world_mut()
            .entity_mut(b)
            .remove::<(village::Gathering, village::production::JobIntent)>();
        for need in ["Hunger", "Thirst", "Energy", "Warmth"] {
            set_need(&mut app, b, need, 100_000);
        }
        assert!(colony::social_available(app.world(), b));
        assert_eq!(
            colony::social_target(app.world(), a).map(|(id, _)| id),
            Some(b_id)
        );
        colony::tick(app.world_mut());
        assert_eq!(
            app.world()
                .get::<colony::SocialLife>(a)
                .unwrap()
                .affinity
                .get(&b_id),
            Some(&250)
        );
        assert_eq!(
            app.world()
                .get::<colony::SocialLife>(b)
                .unwrap()
                .affinity
                .get(&a_id),
            Some(&250)
        );
    }
}

#[test]
fn an_available_social_approach_remains_eligible_until_arrival() {
    let mut app = scene();
    let (_, a) = actor(&app, "Ada");
    let (b_id, b) = actor(&app, "Rowan");
    let target = SimPos::new(100_500, 100_000);
    app.world_mut()
        .entity_mut(a)
        .insert((SimPos::new(100_000, 100_000), MoveIntent::to(target, 70)));
    app.world_mut().entity_mut(b).insert(target);
    app.world_mut().resource_mut::<SimClock>().tick = 600;
    assert!(colony::social_available(app.world(), a));
    assert_eq!(colony::social_target(app.world(), a), Some((b_id, target)));
    colony::tick(app.world_mut());
    assert_eq!(app.world().get::<MoveIntent>(a).unwrap().goal, target);
    assert_eq!(
        app.world()
            .get::<colony::SocialLife>(a)
            .unwrap()
            .affinity
            .get(&b_id),
        Some(&250)
    );
}
