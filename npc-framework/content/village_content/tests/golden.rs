//! Task P2: insta golden snapshots (JSON) of the seed-1 village.
//!
//! Any behaviour change shows up as a reviewable snapshot diff. Snapshots
//! live in `tests/snapshots/` and are oracles: never edit or re-accept them
//! to make a test pass; re-baselining needs explicit approval. CI runs with
//! `INSTA_UPDATE=no`. Re-baselining: `docs/rebaselining.md`.

use bevy_app::App;
use npc_framework::kernel::journal::set_journal;
use npc_framework::prelude::{Journal, run_ticks};
use village_content::inspect;
use village_content::replay::new_sim_app;
use village_content::scenario::VillageIds;

/// The seed-1 village with the journal on (as `npcctl inspect` runs it, so
/// `recent_journal` is filled).
fn village() -> App {
    let mut app = new_sim_app(1, "village", 1);
    set_journal(app.world_mut(), true, false);
    app
}

/// Villager 5 (the scenario's villager, looked up in `VillageIds`) at
/// `tick`, against the snapshot `inspect_unit5_t{tick}`. One test per
/// snapshot, so a failure at one tick does not hide (or keep insta from
/// staging) the others.
fn inspect_villager_at(tick: u64) {
    let mut app = village();
    run_ticks(app.world_mut(), tick);
    let villager = app.world().resource::<VillageIds>().villager;
    let unit = inspect::unit(app.world(), villager).expect("the villager exists");
    insta::assert_json_snapshot!(format!("inspect_unit5_t{tick}"), unit);
}

#[test]
fn golden_inspect_unit5_t600() {
    inspect_villager_at(600);
}

#[test]
fn golden_inspect_unit5_t1800() {
    inspect_villager_at(1800);
}

#[test]
fn golden_inspect_unit5_t4000() {
    inspect_villager_at(4000);
}

#[test]
fn golden_journal_seed1_first_200() {
    let mut app = village();
    while app.world().resource::<Journal>().entries.len() < 200 {
        run_ticks(app.world_mut(), 1);
    }
    let first: Vec<_> = app
        .world()
        .resource::<Journal>()
        .entries
        .iter()
        .take(200)
        .cloned()
        .collect();
    insta::assert_json_snapshot!("journal_seed1_first_200", first);
}
