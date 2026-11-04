//! Real behavior-tree decisions, physical work and control handover in island content.
use bevy_app::App;
use bevy_ecs::prelude::*;
use npc_framework::agent_io::control::{
    AttachController, ControlMode, DetachController, IdlePolicy,
};
use npc_framework::kernel::hash::state_hash;
use npc_framework::needs::{NeedRegistry, Needs};
use npc_framework::prelude::*;
use testbed_content::village::{self, Gathering, Role, Villager, Vitality, Wallet};
use village::behavior::{LifeDecision, LifeGoal};

fn scene() -> App {
    testbed_content::new_sim_app(7, "ai_village")
}
fn body(app: &App, name: &str) -> Entity {
    let id = app
        .world()
        .resource::<testbed_content::TestbedIds>()
        .id(name);
    app.world().resource::<SimIndex>().get(id).unwrap()
}
fn need(app: &mut App, e: Entity, name: &str, value: i32) {
    let id = app.world().resource::<NeedRegistry>().id(name).unwrap();
    app.world_mut().get_mut::<Needs>(e).unwrap().values[id.0 as usize].value = value;
}
fn at_patch(app: &mut App, name: &str, patch: &str) -> Entity {
    let e = body(app, name);
    let pos = *app.world().get::<SimPos>(body(app, patch)).unwrap();
    app.world_mut().entity_mut(e).insert(pos);
    e
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
fn urgent_thirst_aborts_owned_work_and_reacts_without_matching_need_threshold() {
    let mut app = scene();
    let e = at_patch(&mut app, "Ada", "patch_village_grain");
    run_ticks(app.world_mut(), 1);
    assert!(app.world().get::<Gathering>(e).is_some());
    need(&mut app, e, "Thirst", 14_999);
    let thirst = app.world().resource::<NeedRegistry>().id("Thirst").unwrap();
    app.world_mut().get_mut::<Needs>(e).unwrap().values[thirst.0 as usize].level =
        npc_framework::needs::NeedLevel::Low;
    // Keep the existing hysteresis level, so no explicit or threshold-change
    // request can trigger the reaction; the staggered timer must discover it.
    for _ in 0..40 {
        run_ticks(app.world_mut(), 1);
        if village::economy::need(app.world(), e, "Thirst") > 90_000 {
            assert!(
                app.world().get::<Gathering>(e).is_none(),
                "owned gathering must cancel before drinking"
            );
            assert_eq!(
                app.world().get::<LifeDecision>(e).unwrap().goal,
                LifeGoal::Thirst
            );
            return;
        }
    }
    panic!("critical thirst was not discovered by the bounded periodic re-think");
}

#[test]
fn empty_or_unusable_food_supply_does_not_block_production() {
    for unusable in ["empty", "full_wallet", "full_pack"] {
        let mut app = scene();
        let e = body(&app, "Ada");
        let baker = body(&app, "Mira");
        village::economy::consume(app.world_mut(), e, "bread", 2).unwrap();
        match unusable {
            "empty" => {
                let count = village::economy::count(app.world(), baker, "bread");
                village::economy::consume(app.world_mut(), baker, "bread", count).unwrap();
            }
            "full_wallet" => app.world_mut().get_mut::<Wallet>(baker).unwrap().coins = u32::MAX,
            "full_pack" => {
                app.world_mut()
                    .get_mut::<Inventory>(e)
                    .unwrap()
                    .max_weight_g = 0;
            }
            _ => unreachable!(),
        }
        need(&mut app, e, "Hunger", 10_000);
        run_ticks(app.world_mut(), 1);
        assert_eq!(
            app.world().get::<LifeDecision>(e).unwrap().goal,
            LifeGoal::Work,
            "{unusable}"
        );
    }
}

#[test]
fn control_takeover_cancels_owned_work_and_detach_restores_the_same_tree() {
    let mut app = scene();
    let e = at_patch(&mut app, "Ada", "patch_village_grain");
    let id = *app.world().get::<SimId>(e).unwrap();
    run_ticks(app.world_mut(), 1);
    assert!(app.world().get::<Gathering>(e).is_some());
    submit(
        app.world_mut(),
        None,
        SimCommand::content(&AttachController {
            unit: id.0,
            controller: "player_fixture".into(),
            mode: ControlMode::Direct,
            on_idle: IdlePolicy::Idle,
            force: false,
        }),
    )
    .unwrap();
    run_ticks(app.world_mut(), 80);
    assert!(app.world().get::<LifeDecision>(e).is_none());
    assert!(app.world().get::<Gathering>(e).is_none());
    assert!(app.world().get::<MoveIntent>(e).is_none());
    submit(
        app.world_mut(),
        None,
        SimCommand::content(&DetachController {
            unit: id.0,
            controller: "player_fixture".into(),
        }),
    )
    .unwrap();
    run_ticks(app.world_mut(), 2);
    assert_eq!(
        app.world().get::<LifeDecision>(e).unwrap().goal,
        LifeGoal::Work
    );
    assert!(app.world().get::<Gathering>(e).is_some());
}

#[test]
fn home_recovery_is_not_self_aborted_by_the_busy_guard() {
    let mut app = scene();
    let e = body(&app, "Ada");
    let home = app.world().get::<Villager>(e).unwrap().home;
    app.world_mut().entity_mut(e).insert(home);
    need(&mut app, e, "Energy", 29_000);
    run_ticks(app.world_mut(), 80);
    assert_eq!(
        app.world().get::<LifeDecision>(e).unwrap().goal,
        LifeGoal::Recovery
    );
    assert!(
        app.world().get::<Villager>(e).unwrap().resting_until
            > app.world().resource::<SimClock>().tick
    );
    assert!(village::economy::need(app.world(), e, "Energy") > 55_000);
}

#[test]
fn death_removes_the_life_decision_and_never_restarts_work() {
    let mut app = scene();
    let e = at_patch(&mut app, "Ada", "patch_village_grain");
    run_ticks(app.world_mut(), 1);
    app.world_mut().get_mut::<Vitality>(e).unwrap().health = 0;
    run_ticks(app.world_mut(), 80);
    assert!(app.world().get::<Vitality>(e).unwrap().died_at.is_some());
    assert!(app.world().get::<LifeDecision>(e).is_none());
    assert!(app.world().get::<Gathering>(e).is_none());
    assert_eq!(village::behavior::decision(app.world(), e), None);
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

#[test]
fn explicit_player_orders_suspend_daily_work_and_use_the_generic_executor() {
    let mut app = scene();
    let e = at_patch(&mut app, "Ada", "patch_village_grain");
    let id = *app.world().get::<SimId>(e).unwrap();
    run_ticks(app.world_mut(), 1);
    let start = *app.world().get::<SimPos>(e).unwrap();
    let destination = SimPos::new(start.x + 2000, start.z);
    npc_framework::command::submit_order(
        app.world_mut(),
        &npc_framework::command::IssueOrder::new(
            PlayerId(1),
            [id],
            UnitOrder::Move {
                pos: (destination.x, destination.z),
            },
        ),
    )
    .unwrap();
    run_ticks(app.world_mut(), 20);
    assert!(app.world().get::<LifeDecision>(e).is_none());
    assert!(app.world().get::<Gathering>(e).is_none());
    assert!(app.world().get::<SimPos>(e).unwrap().dist2(destination) < start.dist2(destination));
}

#[test]
fn disabled_world_crafting_applies_to_npc_production_too() {
    for enabled in [false, true] {
        let mut app = scene();
        let e = at_patch(&mut app, "Rowan", "village_2");
        village::economy::grant(app.world_mut(), e, Item::new(village::catalog::GRAIN));
        app.world_mut()
            .resource_mut::<testbed_content::world_runtime::ActiveWorld>()
            .0
            .systems
            .crafting = enabled;
        run_ticks(app.world_mut(), 1);
        assert_eq!(
            app.world()
                .get::<npc_framework::crafting::Crafting>(e)
                .is_some(),
            enabled
        );
    }
}

#[test]
fn npc_manufacturing_obeys_the_same_actor_crafting_permission_as_a_client() {
    for allowed in [false, true] {
        let mut app = scene();
        let e = at_patch(&mut app, "Rowan", "village_2");
        village::economy::grant(app.world_mut(), e, Item::new(village::catalog::GRAIN));
        app.world_mut()
            .entity_mut(e)
            .insert(testbed_content::actors::AllowedActions(if allowed {
                ["craft".into()].into_iter().collect()
            } else {
                Default::default()
            }));
        run_ticks(app.world_mut(), 1);
        assert_eq!(
            app.world()
                .get::<npc_framework::crafting::Crafting>(e)
                .is_some(),
            allowed
        );
    }
}

#[test]
fn buffered_procurement_falls_back_to_the_quantity_that_physically_fits() {
    let mut app = scene();
    let e = body(&app, "Rowan");
    let farmer = body(&app, "Ada");
    let position = *app.world().get::<SimPos>(farmer).unwrap();
    app.world_mut().entity_mut(e).insert(position);
    let carried = npc_framework::interaction::items::carried_weight(app.world(), e);
    let grain_weight =
        npc_framework::interaction::items::kind_def(app.world(), village::catalog::GRAIN)
            .unwrap()
            .weight_g;
    app.world_mut()
        .get_mut::<Inventory>(e)
        .unwrap()
        .max_weight_g = carried + grain_weight * 2;
    let before = app.world().get::<Wallet>(e).unwrap().coins;
    let stock = village::economy::count(app.world(), farmer, village::catalog::GRAIN);
    run_ticks(app.world_mut(), 1);
    let bought = village::economy::count(app.world(), e, village::catalog::GRAIN);
    assert_eq!(
        bought, 2,
        "a full-sized buffer cannot fit, but a real paid input purchase must still occur"
    );
    assert_eq!(
        village::economy::count(app.world(), farmer, village::catalog::GRAIN),
        stock - bought
    );
    assert_eq!(
        app.world().get::<Wallet>(e).unwrap().coins,
        before - bought * village::catalog::price(village::catalog::GRAIN).unwrap()
    );
}
