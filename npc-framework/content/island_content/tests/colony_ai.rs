//! Authored priorities, real GOAP preconditions, observer purity and resumed life.
use bevy_app::App;
use bevy_ecs::prelude::*;
use island_content::village::{self, catalog, economy, goap, production};
use npc_framework::kernel::hash::state_hash;
use npc_framework::prelude::*;
use std::collections::BTreeMap;

fn scene() -> App {
    island_content::new_sim_app(7, "living_world")
}
fn actor(app: &App, name: &str) -> Entity {
    let id = app.world().resource::<island_content::IslandIds>().id(name);
    app.world().resource::<SimIndex>().get(id).unwrap()
}

#[test]
fn planner_procures_before_crafting_and_inspection_changes_nothing() {
    let mut app = scene();
    let rowan = actor(&app, "Rowan");
    let id = *app.world().get::<SimId>(rowan).unwrap();
    let grain = economy::count(app.world(), rowan, catalog::GRAIN);
    if grain > 0 {
        economy::consume(app.world_mut(), rowan, catalog::GRAIN, grain).unwrap();
    }
    let mut ledger = production::ProductionPlan::default();
    ledger
        .production
        .insert(id, BTreeMap::from([(catalog::FLOUR.into(), 1)]));
    ledger
        .procurement
        .insert(id, BTreeMap::from([(catalog::GRAIN.into(), 2)]));
    ledger
        .bid_limits
        .insert(id, BTreeMap::from([(catalog::GRAIN.into(), 20)]));
    app.world_mut().insert_resource(ledger);
    let before = state_hash(app.world_mut());
    let plan = goap::plan(app.world(), rowan).unwrap();
    assert_eq!(plan.goal, catalog::FLOUR);
    assert!(
        matches!(plan.steps.first(), Some(goap::PlanStep::Buy { kind, .. }) if kind == catalog::GRAIN)
    );
    assert!(
        matches!(plan.steps.last(), Some(goap::PlanStep::Craft { recipe, .. }) if recipe == catalog::MILL)
    );
    assert!(goap::explain(app.world(), rowan).contains("Goal:"));
    assert_eq!(state_hash(app.world_mut()), before);
    let purse = app.world().get::<village::Wallet>(rowan).unwrap().coins;
    app.world_mut()
        .get_mut::<village::Wallet>(rowan)
        .unwrap()
        .coins = 0;
    assert!(
        goap::plan(app.world(), rowan).is_none(),
        "a plan cannot create purchasing power"
    );
    app.world_mut()
        .get_mut::<village::Wallet>(rowan)
        .unwrap()
        .coins = purse;
}

#[test]
fn planner_explanations_distinguish_demand_supply_and_budget_without_writes() {
    let mut app = scene();
    let rowan = actor(&app, "Rowan");
    let id = *app.world().get::<SimId>(rowan).unwrap();
    app.world_mut()
        .insert_resource(production::ProductionPlan::default());
    let before = state_hash(app.world_mut());
    assert!(goap::explain(app.world(), rowan).contains("No funded production goal"));
    assert_eq!(state_hash(app.world_mut()), before);
    let grain = economy::count(app.world(), rowan, catalog::GRAIN);
    if grain > 0 {
        economy::consume(app.world_mut(), rowan, catalog::GRAIN, grain).unwrap();
    }
    let mut ledger = production::ProductionPlan::default();
    ledger
        .production
        .insert(id, BTreeMap::from([(catalog::FLOUR.into(), 1)]));
    ledger
        .procurement
        .insert(id, BTreeMap::from([(catalog::GRAIN.into(), 2)]));
    ledger
        .bid_limits
        .insert(id, BTreeMap::from([(catalog::GRAIN.into(), 20)]));
    app.world_mut().insert_resource(ledger);
    app.world_mut()
        .resource_mut::<village::policy::IslandAiPolicy>()
        .goap
        .max_steps = 1;
    let before = state_hash(app.world_mut());
    assert!(goap::explain(app.world(), rowan).contains("search limit"));
    assert!(goap::plan(app.world(), rowan).is_none());
    assert_eq!(state_hash(app.world_mut()), before);
    app.world_mut()
        .get_mut::<village::Wallet>(rowan)
        .unwrap()
        .coins = 0;
    let before = state_hash(app.world_mut());
    assert!(goap::explain(app.world(), rowan).contains("current supplies, purchasing power"));
    assert!(goap::plan(app.world(), rowan).is_none());
    assert_eq!(state_hash(app.world_mut()), before);
}

#[test]
fn configured_colony_decisions_resume_and_reproduce_exactly() {
    npc_framework::tools::snapshot_check::assert_resume_equivalent(scene, 91, 1200, 80);
    let mut a = scene();
    let mut b = scene();
    run_ticks(a.world_mut(), 1200);
    run_ticks(b.world_mut(), 1200);
    assert_eq!(state_hash(a.world_mut()), state_hash(b.world_mut()));
}
