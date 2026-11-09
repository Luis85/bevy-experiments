//! Exact corner coordinates from the expanded-village 216780..219900 trace.
//! Merchants remain physically separated; the buyer must walk and pay.
use bevy_app::App;
use bevy_ecs::prelude::*;
use npc_framework::prelude::*;
use serde_json::json;
use std::collections::BTreeMap;
use testbed_content::village::{
    self, catalog, economy, production::ProductionPlan, routing::RouteRecovery,
};

fn actor(app: &App, name: &str) -> Entity {
    let id = app
        .world()
        .resource::<testbed_content::TestbedIds>()
        .id(name);
    app.world().resource::<SimIndex>().get(id).unwrap()
}

fn corner_scene() -> App {
    let mut doc = testbed_content::scenario::scenario_doc("living_village").unwrap();
    // Original expanded geometry reproduces the observed corner exactly. The
    // new policy is opt-in without changing the incumbent replay fixtures.
    doc.world["island_ai"] = json!({});
    let mut app = testbed_content::load_scenario(&doc, 7).unwrap().app;
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
fn observed_integer_corner_is_real_and_resident_recovers_to_complete_physical_trade() {
    let mut app = corner_scene();
    let ivo = actor(&app, "Ivo");
    let jun = actor(&app, "Jun");
    let starting_cash = app.world().get::<village::Wallet>(ivo).unwrap().coins;
    let seller_cash = app.world().get::<village::Wallet>(jun).unwrap().coins;
    let seller_stock = economy::count(app.world(), jun, "plank");
    run_ticks(app.world_mut(), 1);
    let here = SimPos::new(4046, -5991);
    let waypoint = SimPos::new(1500, -6500);
    let grid = app.world().resource::<SpatialGrid>();
    let start_cell = grid.cell_of(here);
    assert!(grid.trace_segment(here, waypoint, |cell| cell == start_cell
        || !grid.is_blocking(cell)));
    assert!(
        !grid.trace_segment(here, here.step_towards(waypoint, 70), |cell| cell
            == start_cell
            || !grid.is_blocking(cell)),
        "the rounded first step must reproduce the reported blocked corner"
    );
    let mut recovering = false;
    for _ in 0..14 {
        run_ticks(app.world_mut(), 20);
        recovering |= app.world().get::<RouteRecovery>(ivo).is_some();
    }
    assert!(
        recovering,
        "the stuck actor must take the explicit bounded recovery path"
    );
    assert_eq!(economy::count(app.world(), jun, "plank"), seller_stock - 1);
    assert!(app.world().get::<village::Wallet>(ivo).unwrap().coins < starting_cash);
    assert!(app.world().get::<village::Wallet>(jun).unwrap().coins > seller_cash);
    assert_eq!(
        app.world().get::<village::Wallet>(ivo).unwrap().coins
            + app.world().get::<village::Wallet>(jun).unwrap().coins,
        starting_cash + seller_cash
    );
}

#[test]
fn corner_recovery_resumes_without_a_teleport_or_a_second_purchase() {
    npc_framework::tools::snapshot_check::assert_resume_equivalent(corner_scene, 53, 281, 20);
}
