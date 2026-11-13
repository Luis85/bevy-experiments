//! Real behavior-tree decisions, physical work and control handover in island content.
use bevy_app::App;
use bevy_ecs::prelude::*;
use island_content::village::{self, Gathering, Role};
use npc_framework::kernel::hash::state_hash;
use npc_framework::needs::{NeedRegistry, Needs};
use npc_framework::prelude::*;
use village::behavior::{LifeDecision, LifeGoal};

fn scene() -> App {
    island_content::new_sim_app(7, "living_world")
}
fn body(app: &App, name: &str) -> Entity {
    let id = app
        .world()
        .resource::<island_content::TestbedIds>()
        .id(name);
    app.world().resource::<SimIndex>().get(id).unwrap()
}
fn need(app: &mut App, e: Entity, name: &str, value: i32) {
    let id = app.world().resource::<NeedRegistry>().id(name).unwrap();
    app.world_mut().get_mut::<Needs>(e).unwrap().values[id.0 as usize].value = value;
}

#[test]
fn all_residents_run_the_registered_life_tree_and_visitor_stays_player_driven() {
    let mut app = scene();
    let tree = app
        .world()
        .resource::<AiRegistry>()
        .tree_id(village::behavior::TREE)
        .unwrap();
    for role in Role::RESIDENTS {
        assert_eq!(
            app.world()
                .get::<Brain>(body(&app, role.person()))
                .unwrap()
                .tree,
            tree
        );
    }
    run_ticks(app.world_mut(), 1);
    for role in Role::RESIDENTS {
        let e = body(&app, role.person());
        assert!(app.world().get::<LifeDecision>(e).is_some());
        assert!(
            !app.world()
                .get::<BrainState>(e)
                .unwrap()
                .running_path
                .is_empty()
        );
        assert!(village::behavior::decision(app.world(), e).is_some());
    }
    assert!(
        app.world()
            .get::<LifeDecision>(body(&app, "human"))
            .is_none()
    );
}

#[test]
fn thirst_has_priority_over_hunger_and_work_and_uses_the_shared_consume_action() {
    let mut app = scene();
    let e = body(&app, "Ada");
    need(&mut app, e, "Thirst", 10_000);
    need(&mut app, e, "Hunger", 10_000);
    let water = village::economy::count(app.world(), e, village::catalog::WATER);
    let bread = village::economy::count(app.world(), e, "bread");
    run_ticks(app.world_mut(), 1);
    assert_eq!(
        app.world().get::<LifeDecision>(e).unwrap().goal,
        LifeGoal::Thirst
    );
    assert_eq!(
        village::economy::count(app.world(), e, village::catalog::WATER),
        water - 1
    );
    assert_eq!(village::economy::count(app.world(), e, "bread"), bread);
    assert!(village::economy::need(app.world(), e, "Thirst") > 90_000);
    assert!(app.world().get::<Gathering>(e).is_none());
}

#[test]
fn running_decisions_resume_exactly_and_repeat_deterministically() {
    npc_framework::tools::snapshot_check::assert_resume_equivalent(scene, 77, 800, 20);
    let mut a = scene();
    let mut b = scene();
    run_ticks(a.world_mut(), 800);
    run_ticks(b.world_mut(), 800);
    assert_eq!(state_hash(a.world_mut()), state_hash(b.world_mut()));
}
