//! NPC policy and external clients use the same checked island actions.
use bevy_app::App;
use bevy_ecs::prelude::*;
use island_content::village::{self, DroppedDiary, VillageAct};
use npc_framework::kernel::hash::state_hash;
use npc_framework::prelude::*;

fn scene() -> App {
    island_content::new_sim_app(7, "living_world")
}

fn person(app: &App, name: &str) -> (SimId, Entity) {
    let id = app.world().resource::<island_content::IslandIds>().id(name);
    (id, app.world().resource::<SimIndex>().get(id).unwrap())
}

fn dropped_diary(app: &mut App, pos: SimPos) -> (SimId, Entity) {
    spawn_sim(
        app.world_mut(),
        (
            pos,
            Item::new(village::catalog::DIARY),
            DroppedDiary {
                author: SimId(1),
                author_name: "A prior traveller".into(),
                life: 1,
                died_at: 0,
                pages: Vec::new(),
                sealed: true,
            },
        ),
    )
}

#[test]
fn diary_recovery_respects_inventory_capacity_without_moving_the_item() {
    let mut app = scene();
    let (id, actor) = person(&app, "human");
    let pos = *app.world().get::<SimPos>(actor).unwrap();
    let (diary, item) = dropped_diary(&mut app, pos);
    app.world_mut()
        .entity_mut(actor)
        .insert(Inventory::new(0, 0));
    let mut recover = VillageAct::new(id, "recover_diary");
    recover.target = Some(diary);
    let before = state_hash(app.world_mut());
    assert!(recover.perform(app.world_mut()).is_err());
    assert_eq!(state_hash(app.world_mut()), before);
    assert!(app.world().get::<CarriedBy>(item).is_none());
    assert_eq!(app.world().get::<SimPos>(item), Some(&pos));
}
