//! Authored priorities, real GOAP preconditions, observer purity and resumed life.
use bevy_app::App;
use bevy_ecs::prelude::*;
use npc_framework::kernel::hash::state_hash;
use npc_framework::needs::{NeedRegistry, Needs};
use npc_framework::prelude::*;
use serde_json::json;
use std::collections::BTreeMap;
use testbed_content::village::{self, catalog, economy, goap, production};

fn scene() -> App {
    testbed_content::new_sim_app(7, "island_colony")
}
fn actor(app: &App, name: &str) -> Entity {
    let id = app
        .world()
        .resource::<testbed_content::TestbedIds>()
        .id(name);
    app.world().resource::<SimIndex>().get(id).unwrap()
}
fn need(app: &mut App, person: Entity, name: &str, value: i32) {
    let id = app.world().resource::<NeedRegistry>().id(name).unwrap();
    app.world_mut().get_mut::<Needs>(person).unwrap().values[id.0 as usize].value = value;
}

#[test]
fn authored_routine_changes_noncritical_priority_but_preserves_emergency_guards() {
    let mut doc = testbed_content::scenario::scenario_doc("island_colony").unwrap();
    doc.world["island_ai"] =
        json!({"routine":["hunger","thirst","recovery","home","work"],"decision_ticks":40});
    let mut app = testbed_content::load_scenario(&doc, 7).unwrap().app;
    let ada = actor(&app, "Ada");
    need(&mut app, ada, "Hunger", 40_000);
    need(&mut app, ada, "Thirst", 40_000);
    let water = economy::count(app.world(), ada, catalog::WATER);
    run_ticks(app.world_mut(), 1);
    let decision = app
        .world()
        .get::<village::behavior::LifeDecision>(ada)
        .unwrap();
    assert_eq!(decision.goal, village::behavior::LifeGoal::Hunger);
    assert_eq!(decision.reconsider_at, 40);
    assert_eq!(economy::count(app.world(), ada, catalog::WATER), water);
    assert!(economy::need(app.world(), ada, "Hunger") > 80_000);

    let mut critical = testbed_content::load_scenario(&doc, 7).unwrap().app;
    let ada = actor(&critical, "Ada");
    need(&mut critical, ada, "Hunger", 40_000);
    need(&mut critical, ada, "Thirst", 10_000);
    run_ticks(critical.world_mut(), 1);
    assert_eq!(
        critical
            .world()
            .get::<village::behavior::LifeDecision>(ada)
            .unwrap()
            .goal,
        village::behavior::LifeGoal::Thirst
    );
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

#[test]
fn policy_values_are_hashed_and_saved_and_old_worlds_have_no_policy() {
    let mut a = scene();
    let before = state_hash(a.world_mut());
    a.world_mut()
        .resource_mut::<village::policy::IslandAiPolicy>()
        .goap
        .buy_cost = 2;
    assert_ne!(state_hash(a.world_mut()), before);
    let old = testbed_content::new_sim_app(7, "living_village");
    assert!(village::policy::active(old.world()).is_none());
    assert!(
        serde_json::to_value(testbed_content::worlds::builtin_preset("living_island").unwrap())
            .unwrap()
            .get("island_ai")
            .is_none()
    );
}
