//! Exact corner coordinates from the expanded-village 216780..219900 trace.
//! Merchants remain physically separated; the buyer must walk and pay.
use bevy_app::App;
use bevy_ecs::prelude::*;
use island_content::village::{self, catalog, production::ProductionPlan};
use npc_framework::prelude::*;
use serde_json::json;
use std::collections::BTreeMap;

fn actor(app: &App, name: &str) -> Entity {
    let id = app.world().resource::<island_content::IslandIds>().id(name);
    app.world().resource::<SimIndex>().get(id).unwrap()
}

fn corner_scene() -> App {
    let mut doc = island_content::scenario::scenario_doc("living_world").unwrap();
    // Original expanded geometry reproduces the observed corner exactly. The
    // new policy is opt-in without changing the incumbent replay fixtures.
    doc.world["island_ai"] = json!({});
    let mut app = island_content::load_scenario(&doc, 7).unwrap().app;
    let ivo = actor(&app, "Ivo");
    let jun = actor(&app, "Jun");
    let others: Vec<_> = app
        .world()
        .resource::<SimIndex>()
        .map
        .values()
        .copied()
        .filter(|entity| *entity != ivo && app.world().get::<village::Villager>(*entity).is_some())
        .collect();
    for entity in others {
        app.world_mut().entity_mut(entity).remove::<Brain>();
    }
    app.world_mut()
        .entity_mut(ivo)
        .insert(SimPos::new(4046, -5991));
    app.world_mut()
        .entity_mut(jun)
        .insert(SimPos::new(-6030, -5229));
    let id = *app.world().get::<SimId>(ivo).unwrap();
    let mut plan = ProductionPlan {
        refreshed_at: Some(0),
        ..Default::default()
    };
    plan.production
        .insert(id, BTreeMap::from([(catalog::SPEAR.into(), 1)]));
    plan.procurement
        .insert(id, BTreeMap::from([("plank".into(), 1)]));
    plan.wanted
        .insert(id, BTreeMap::from([("plank".into(), 1)]));
    plan.bid_limits
        .insert(id, BTreeMap::from([("plank".into(), 100)]));
    app.world_mut().insert_resource(plan);
    app
}

#[test]
fn corner_recovery_resumes_without_a_teleport_or_a_second_purchase() {
    npc_framework::tools::snapshot_check::assert_resume_equivalent(corner_scene, 53, 281, 20);
}
