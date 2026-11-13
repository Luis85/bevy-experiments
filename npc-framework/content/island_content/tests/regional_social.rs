//! Leisure opportunities use real nearby walking routes without a second AI.
use bevy_app::App;
use bevy_ecs::prelude::*;
use island_content::{
    TestbedIds,
    village::{self, Villager, colony},
};
use npc_framework::kernel::hash::state_hash;
use npc_framework::prelude::*;

fn fixture(scenario: &str) -> (App, Entity, SimId, SimId) {
    let mut app = island_content::new_sim_app(7, scenario);
    app.world_mut().resource_mut::<SimClock>().tick = 1200;
    let people: Vec<_> = app
        .world()
        .resource::<SimIndex>()
        .map
        .values()
        .copied()
        .filter(|actor| app.world().get::<Villager>(*actor).is_some())
        .collect();
    // Only the named participants are currently free for this isolated meeting.
    for actor in people {
        app.world_mut()
            .get_mut::<Villager>(actor)
            .unwrap()
            .resting_until = 5000;
    }
    let ids = app.world().resource::<TestbedIds>();
    let ada = ids.id("Ada");
    let rowan = ids.id("Rowan");
    let nell = ids.id("Nell");
    for (id, position) in [
        (ada, SimPos::new(0, 0)),
        (rowan, SimPos::new(4000, 0)),
        (nell, SimPos::new(-6000, 0)),
    ] {
        let actor = app.world().resource::<SimIndex>().get(id).unwrap();
        app.world_mut().entity_mut(actor).insert(position);
        app.world_mut()
            .get_mut::<Villager>(actor)
            .unwrap()
            .resting_until = 0;
    }
    // Authored test geometry: the nearest person is across a long pond. Walking
    // around it takes over 15 m, while the slightly farther neighbour is clear.
    let mut grid = app.world_mut().resource_mut::<SpatialGrid>();
    for z in -20..=20 {
        for x in -20..=20 {
            grid.set_static(CellCoord::new(x, z), CellInfo::default());
        }
    }
    for z in -9..=9 {
        for x in 1..=3 {
            grid.set_static(
                CellCoord::new(x, z),
                CellInfo {
                    flags: CellFlags {
                        blocking: true,
                        ..Default::default()
                    },
                    ..Default::default()
                },
            );
        }
    }
    let actor = app.world().resource::<SimIndex>().get(ada).unwrap();
    (app, actor, rowan, nell)
}

#[test]
fn regional_meeting_prefers_reachable_company_over_a_neighbour_across_a_pond() {
    let (mut app, actor, _, reachable) = fixture("living_world");
    let before = state_hash(app.world_mut());
    assert_eq!(
        colony::social_target(app.world(), actor).map(|(id, _)| id),
        Some(reachable)
    );
    assert_eq!(state_hash(app.world_mut()), before);
}

#[test]
fn old_island_keeps_its_original_social_selection_contract() {
    let (app, actor, nearest, _) = fixture("island_colony");
    assert!(!village::frontier::enabled(app.world()));
    assert_eq!(
        colony::social_target(app.world(), actor).map(|(id, _)| id),
        Some(nearest)
    );
}
