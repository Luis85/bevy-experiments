//! Shared household/social facts stay truthful, private and side-effect free.
use bevy_app::App;
use island_content::village::{self, colony, economy};
use npc_framework::agent_io::observe;
use npc_framework::kernel::hash::state_hash;
use npc_framework::prelude::*;
use npc_framework::snapshot;
use serde_json::json;

fn scene() -> App {
    island_content::new_sim_app(7, "living_world")
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
