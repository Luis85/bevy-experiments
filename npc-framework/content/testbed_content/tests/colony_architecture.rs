//! Boundaries that must remain true when colony mechanics gain new callers.
use npc_framework::kernel::hash::state_hash;
use npc_framework::prelude::*;
use testbed_content::TestbedIds;
use testbed_content::village::{self, colony, economy, homestead, production};

#[test]
fn construction_queries_cannot_move_workers_or_reserve_their_materials() {
    let mut app = testbed_content::new_sim_app(7, "island_colony");
    let builder = app.world().resource::<TestbedIds>().id("Jun");
    let actor = app.world().resource::<SimIndex>().get(builder).unwrap();
    let (plot, estate) = homestead::plots(app.world())
        .into_iter()
        .find(|(_, entity)| {
            homestead::definition(app.world(), *entity)
                .is_some_and(|definition| definition.use_kind == "housing")
        })
        .unwrap();
    colony::ColonyAct {
        verb: "commission_home".into(),
        resident: Some(builder),
        house: Some(plot),
        value: 0,
    }
    .perform(app.world_mut())
    .unwrap();
    for (kind, required) in [
        ("plank", homestead::PLANK_COST),
        ("log", homestead::LOG_COST),
    ] {
        let missing = required.saturating_sub(economy::count(app.world(), actor, kind));
        if missing > 0 {
            economy::grant(app.world_mut(), actor, Item::new(kind).stack(missing));
        }
    }
    let door = homestead::front_door(*app.world().get::<SimPos>(estate).unwrap());
    let before = state_hash(app.world_mut());
    for _ in 0..10 {
        assert_eq!(
            colony::construction_intent(app.world(), actor),
            Some(colony::ConstructionIntent::Build {
                builder,
                plot,
                door,
                claim_required: true,
            })
        );
    }
    assert_eq!(state_hash(app.world_mut()), before);
    assert_eq!(app.world().resource::<Reservations>().count(plot), 0);
    assert!(app.world().get::<homestead::CottageBuild>(actor).is_none());
}

#[test]
fn civic_demand_invalidates_production_through_the_application_bridge() {
    let mut app = testbed_content::new_sim_app(7, "island_colony");
    production::refresh(app.world_mut());
    assert!(
        app.world()
            .resource::<production::ProductionPlan>()
            .refreshed_at
            .is_some()
    );
    for (verb, value, invalidated) in [("gatherings", 0, false), ("reserve_food", 3, true)] {
        colony::ColonyAct {
            verb: verb.into(),
            resident: None,
            house: None,
            value,
        }
        .perform(app.world_mut())
        .unwrap();
        assert_eq!(
            app.world()
                .resource::<production::ProductionPlan>()
                .refreshed_at
                .is_none(),
            invalidated
        );
    }
    assert_eq!(village::colony::summary(app.world()).residents, 12);
}
