//! Guidance projects real current evidence; it grants nothing and invents no targets.
use island_content::village::{
    self,
    colony::{JoinIsland, SocialLife},
    player_journey,
};
use npc_framework::agent_io::observe;
use npc_framework::kernel::hash::state_hash;
use npc_framework::prelude::*;
use serde_json::json;

#[test]
fn guidance_and_social_observations_are_pure_and_withhold_unknown_neighbors() {
    let mut app = island_content::new_sim_app(7, "living_world");
    let me = app.world().resource::<village::VillageState>().played;
    JoinIsland { unit: me.0 }.perform(app.world_mut()).unwrap();
    let actor = app.world().resource::<SimIndex>().get(me).unwrap();
    let ada = app
        .world()
        .resource::<island_content::TestbedIds>()
        .id("Ada");
    app.world_mut()
        .get_mut::<SocialLife>(actor)
        .unwrap()
        .affinity
        .insert(ada, 25_000);
    let before = state_hash(app.world_mut());
    let obs = observe::observe(app.world(), me, 0).unwrap().0;
    let _ = player_journey::project(&obs);
    assert_eq!(
        obs["sections"]["village"]["social"]["relationships"],
        json!([])
    );
    assert_eq!(state_hash(app.world_mut()), before);
    let pos = *app
        .world()
        .get::<SimPos>(app.world().resource::<SimIndex>().get(ada).unwrap())
        .unwrap();
    app.world_mut().entity_mut(actor).insert(pos);
    run_ticks(app.world_mut(), 8);
    let before = state_hash(app.world_mut());
    let obs = observe::observe(app.world(), me, 0).unwrap().0;
    let rows = obs["sections"]["village"]["social"]["relationships"]
        .as_array()
        .unwrap();
    let row = rows.iter().find(|row| row["id"] == json!(ada)).unwrap();
    assert_eq!(row["name"], "Ada");
    assert_eq!(row["affinity"], 25_000);
    for hidden in ["pos", "position", "health", "coins", "inventory"] {
        assert!(row.get(hidden).is_none());
    }
    assert_eq!(state_hash(app.world_mut()), before);
}
