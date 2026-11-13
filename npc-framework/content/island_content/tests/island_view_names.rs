//! View data in words a player reads: workplace names, planner lines and the
//! progress of the current activity (living world).
use bevy_ecs::prelude::*;
use island_content::village::{Villager, view};
use npc_framework::prelude::*;

#[test]
fn workplaces_plans_and_progress_read_without_raw_keys() {
    let mut app = island_content::new_sim_app(7, "living_world");
    run_ticks(app.world_mut(), 2_400);
    let mut people: Vec<(SimId, Entity)> = app
        .world_mut()
        .query::<(Entity, &SimId, &Villager)>()
        .iter(app.world())
        .map(|(entity, id, _)| (*id, entity))
        .collect();
    people.sort_unstable_by_key(|(id, _)| *id);
    let mut progressed = 0;
    for (id, entity) in people {
        let name = view::workplace_name(app.world(), id).expect("a workplace");
        assert!(name.starts_with("the "), "{name}");
        assert!(!name.contains('_'), "{name}");
        let lines = view::plan_lines(app.world(), entity);
        assert!(!lines.readable.contains("village_"), "{}", lines.readable);
        if let Some(permille) = view::activity_progress_permille(app.world(), entity) {
            assert!(permille <= 1000);
            progressed += 1;
        }
    }
    assert!(
        progressed > 0,
        "nobody's activity had a measurable progress"
    );
}
