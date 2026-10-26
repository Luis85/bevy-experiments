//! Plan Task C1: the opt-in `village_nav` scenario. Villagers with `NavAgent`
//! path-find around a wall per plot; the existing scenarios are untouched
//! (their corpus and goldens are checked by `corpus.rs` / `golden.rs`).

use bevy_ecs::prelude::With;
use npc_framework::kernel::hash::state_hash;
use npc_framework::kernel::journal::{JournalKind, set_journal};
use npc_framework::kernel::registry::content_hash;
use npc_framework::nav::{NavAgent, Path};
use npc_framework::prelude::*;
use village_content::replay::{record, verify};
use village_content::{Villager, new_sim_app};

const PLOTS: u32 = 4;
const TICKS: u64 = 2400;

#[test]
fn villagers_path_find_around_the_wall() {
    let mut app = new_sim_app(1, "village_nav", PLOTS);
    set_journal(app.world_mut(), true, false);
    let mut detoured = false;
    for _ in 0..TICKS {
        run_ticks(app.world_mut(), 1);
        let world = app.world_mut();
        let mut q =
            world.query_filtered::<(&SimPos, Option<&Path>), (With<Villager>, With<NavAgent>)>();
        let rows: Vec<(SimPos, Option<Path>)> =
            q.iter(world).map(|(p, path)| (*p, path.cloned())).collect();
        assert_eq!(rows.len(), PLOTS as usize);
        let grid = world.resource::<SpatialGrid>();
        for (pos, path) in rows {
            assert!(
                !grid.is_blocking(grid.cell_of(pos)),
                "villager inside the wall at {pos:?}"
            );
            detoured |= path.is_some_and(|p| p.waypoints.len() > 1);
        }
    }
    assert!(detoured, "some path had to go around the wall");
    let journal = app.world().resource::<Journal>();
    let found = journal
        .entries
        .iter()
        .filter(|e| matches!(e.kind, JournalKind::PathFound { .. }))
        .count();
    let failed = journal
        .entries
        .iter()
        .filter(|e| matches!(e.kind, JournalKind::PathFailed { .. }))
        .count();
    assert!(found >= PLOTS as usize, "PathFound entries: {found}");
    assert_eq!(failed, 0, "every place is reachable around the wall");
    // They still reach the stockpile and the field behind the wall.
    let effects: Vec<String> = journal
        .entries
        .iter()
        .filter_map(|e| match &e.kind {
            JournalKind::Effect { effect, .. } => serde_json::to_string(effect).ok(),
            _ => None,
        })
        .collect();
    assert!(
        effects.iter().any(|e| e.contains("AddFood")),
        "worked the field"
    );
    assert!(
        effects.iter().any(|e| e.contains("TakeFood")),
        "ate at the stockpile"
    );
}

#[test]
fn village_nav_is_deterministic_and_replayable() {
    let run = || {
        let mut app = new_sim_app(7, "village_nav", PLOTS);
        run_ticks(app.world_mut(), 1200);
        state_hash(app.world_mut())
    };
    assert_eq!(run(), run());

    let mut app = new_sim_app(7, "village_nav", PLOTS);
    let replay = record(&mut app, 600, 100);
    let report = verify(&replay);
    assert!(report.ok, "{report:?}");
}

#[test]
fn nav_is_opt_in_per_scenario() {
    // The nav plugin's registrations exist only in `village_nav`.
    let mut plain = new_sim_app(1, "village_n", PLOTS);
    let mut nav = new_sim_app(1, "village_nav", PLOTS);
    assert_ne!(
        content_hash(plain.world_mut()),
        content_hash(nav.world_mut())
    );
    let world = plain.world_mut();
    assert!(!world.contains_resource::<npc_framework::nav::NavQueue>());
    let mut q = world.query_filtered::<(), With<NavAgent>>();
    assert_eq!(q.iter(world).count(), 0);
}
