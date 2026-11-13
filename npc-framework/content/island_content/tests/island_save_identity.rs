//! An island save says which version of the island made it, and this build
//! answers in types: current, older, newer, unreadable; only current loads.
use island_content::village::island_save::{self, IslandSaveCheck};
use npc_framework::prelude::*;

fn island() -> bevy_app::App {
    island_content::new_sim_app(7, "living_world")
}

#[test]
fn a_save_of_this_build_is_current_and_loads() {
    let mut app = island();
    run_ticks(app.world_mut(), 20);
    let snap = island_save::save_island(app.world_mut()).expect("saved");
    assert_eq!(
        island_save::saved_version(&snap),
        Some(island_save::ISLAND_SAVE_VERSION)
    );
    let mut fresh = island();
    assert_eq!(
        island_save::check_island_save(fresh.world_mut(), &snap),
        IslandSaveCheck::Current
    );
    island_save::load_island_save(fresh.world_mut(), &snap).expect("loads as saved");
}

#[test]
fn an_older_or_newer_save_is_named_and_refused() {
    let mut app = island();
    run_ticks(app.world_mut(), 5);
    let mut snap = island_save::save_island(app.world_mut()).expect("saved");
    // As a later build sees this save: its content hash differs.
    snap.content_hash ^= 1;
    let current = island_save::ISLAND_SAVE_VERSION + 1;
    let mut fresh = island();
    let older = island_save::check_against(fresh.world_mut(), &snap, current);
    assert_eq!(
        older,
        IslandSaveCheck::OlderIsland {
            saved: Some(island_save::ISLAND_SAVE_VERSION),
            current,
        }
    );
    assert!(older.sentence().contains("older version of the island"));
    // A save from before the stamp, of another build: older, refused.
    snap.header.remove(island_save::HEADER_VERSION);
    assert_eq!(
        island_save::check_island_save(fresh.world_mut(), &snap),
        IslandSaveCheck::OlderIsland {
            saved: None,
            current: island_save::ISLAND_SAVE_VERSION,
        }
    );
    assert!(island_save::load_island_save(fresh.world_mut(), &snap).is_err());
    // A save stamped by a newer island.
    snap.header.insert(
        island_save::HEADER_VERSION.into(),
        serde_json::Value::from(island_save::ISLAND_SAVE_VERSION + 3),
    );
    assert!(matches!(
        island_save::check_island_save(fresh.world_mut(), &snap),
        IslandSaveCheck::NewerIsland { .. }
    ));
}
