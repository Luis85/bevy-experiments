//! Cold recovery uses existing physical fuel, navigation and rest, even with full energy.
use bevy_app::App;
use bevy_ecs::prelude::*;
use npc_framework::needs::{NeedRegistry, Needs};
use npc_framework::prelude::*;
use testbed_content::{
    TestbedIds,
    village::{self, behavior::LifeGoal, colony, economy},
};

fn fixture(scenario: &str) -> (App, SimId, Entity) {
    let mut app = testbed_content::new_sim_app(7, scenario);
    let id = app.world().resource::<TestbedIds>().id("Jun");
    let actor = app.world().resource::<SimIndex>().get(id).unwrap();
    // A policy/action fixture: other brains are frozen so no trader or gatherer
    // can supply the tested actor, change the patch, or light his home for him.
    let others: Vec<_> = app
        .world()
        .resource::<SimIndex>()
        .map
        .values()
        .copied()
        .filter(|entity| *entity != actor)
        .collect();
    for entity in others {
        app.world_mut().entity_mut(entity).remove::<Brain>();
    }
    let day = app.world().resource::<SimConfig>().day_length;
    app.world_mut().resource_mut::<SimClock>().tick = day * 2 + day * 9 / 10;
    for (name, value) in [
        ("Hunger", 100_000),
        ("Thirst", 100_000),
        ("Energy", 100_000),
        ("Warmth", 10_000),
    ] {
        let need = app.world().resource::<NeedRegistry>().id(name).unwrap();
        app.world_mut().get_mut::<Needs>(actor).unwrap().values[need.0 as usize].value = value;
    }
    let logs = economy::count(app.world(), actor, "log");
    if logs > 0 {
        economy::consume(app.world_mut(), actor, "log", logs).unwrap();
    }
    app.world_mut()
        .get_mut::<village::Wallet>(actor)
        .unwrap()
        .coins = 0;
    assert!(!village::living::has_coat(app.world(), actor));
    (app, id, actor)
}
fn timber_patch(app: &App) -> (SimId, Entity, SimPos) {
    app.world()
        .resource::<SimIndex>()
        .map
        .iter()
        .find_map(|(id, entity)| {
            app.world()
                .get::<village::ResourcePatch>(*entity)
                .filter(|patch| patch.kind == "log")
                .map(|_| (*id, *entity, *app.world().get::<SimPos>(*entity).unwrap()))
        })
        .unwrap()
}

#[test]
fn high_energy_cold_resident_uses_actual_home_heat_and_finishes_recovery() {
    let (mut app, _, actor) = fixture("island_colony");
    let home = app.world().get::<village::Villager>(actor).unwrap().home;
    app.world_mut().entity_mut(actor).insert(home);
    economy::grant(app.world_mut(), actor, Item::new("log"));
    run_ticks(app.world_mut(), 1);
    assert_eq!(
        app.world()
            .get::<village::behavior::LifeDecision>(actor)
            .unwrap()
            .goal,
        LifeGoal::Recovery
    );
    assert!(
        app.world()
            .get::<village::Villager>(actor)
            .unwrap()
            .resting_until
            > app.world().resource::<SimClock>().tick
    );
    assert!(colony::home_heated(app.world(), actor));
    assert_eq!(economy::count(app.world(), actor, "log"), 0);
    run_ticks(app.world_mut(), 360);
    assert!(economy::need(app.world(), actor, "Warmth") >= 30_000);
    assert!(
        app.world()
            .get::<village::Villager>(actor)
            .unwrap()
            .resting_until
            <= app.world().resource::<SimClock>().tick
    );
    assert_ne!(
        app.world()
            .get::<village::behavior::LifeDecision>(actor)
            .unwrap()
            .goal,
        LifeGoal::Recovery
    );
}

#[test]
fn a_cold_cashless_resident_acquires_real_fuel_at_night_before_returning_to_heat() {
    let (mut app, _, actor) = fixture("island_colony");
    let (patch_id, patch, position) = timber_patch(&app);
    app.world_mut().entity_mut(actor).insert(position);
    run_ticks(app.world_mut(), 1);
    assert_eq!(
        app.world()
            .get::<village::behavior::LifeDecision>(actor)
            .unwrap()
            .goal,
        LifeGoal::Work
    );
    assert_eq!(
        app.world().get::<village::Gathering>(actor).unwrap().patch,
        patch_id
    );
    let available = app
        .world()
        .get::<village::ResourcePatch>(patch)
        .unwrap()
        .remaining;
    let start = app.world().resource::<SimClock>().tick;
    while app.world().resource::<SimClock>().tick < start + 3600 {
        if colony::home_heated(app.world(), actor)
            && economy::need(app.world(), actor, "Warmth") >= 30_000
        {
            break;
        }
        run_ticks(app.world_mut(), 20);
    }
    assert!(
        colony::home_heated(app.world(), actor),
        "fuel procurement must lead back to physical home recovery"
    );
    assert!(economy::need(app.world(), actor, "Warmth") >= 30_000);
    assert_eq!(
        app.world()
            .get::<village::ResourcePatch>(patch)
            .unwrap()
            .remaining,
        available - 1
    );
    assert_eq!(
        economy::count(app.world(), actor, "log"),
        0,
        "the gathered log must be spent by the hearth"
    );
    assert_eq!(app.world().get::<village::Wallet>(actor).unwrap().coins, 0);
}

#[test]
fn warmth_recovery_preserves_committed_work_and_legacy_energy_only_arbitration() {
    let (mut app, id, actor) = fixture("island_colony");
    let (patch, _, position) = timber_patch(&app);
    app.world_mut().entity_mut(actor).insert(position);
    economy::grant(app.world_mut(), actor, Item::new("log"));
    let mut gather = village::VillageAct::new(id, "gather");
    gather.target = Some(patch);
    gather.perform(app.world_mut()).unwrap();
    let ends = app.world().get::<village::Gathering>(actor).unwrap().ends;
    run_ticks(app.world_mut(), 1);
    assert_eq!(
        app.world().get::<village::Gathering>(actor).unwrap().ends,
        ends
    );
    assert_eq!(
        app.world()
            .get::<village::Villager>(actor)
            .unwrap()
            .resting_until,
        0
    );
    run_ticks(app.world_mut(), 260);
    assert_eq!(
        economy::count(app.world(), actor, "log"),
        2,
        "warming must not cancel the committed gathering action"
    );

    let (mut legacy, _, actor) = fixture("living_village");
    let home = legacy.world().get::<village::Villager>(actor).unwrap().home;
    legacy.world_mut().entity_mut(actor).insert(home);
    run_ticks(legacy.world_mut(), 1);
    assert_eq!(
        legacy
            .world()
            .get::<village::behavior::LifeDecision>(actor)
            .unwrap()
            .goal,
        LifeGoal::Home
    );
    assert_eq!(
        legacy
            .world()
            .get::<village::Villager>(actor)
            .unwrap()
            .resting_until,
        0
    );
}
