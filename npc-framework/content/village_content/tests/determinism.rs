//! Task D: the full village simulation is deterministic.

use bevy_app::App;
use npc_framework::kernel::hash::state_hash;
use npc_framework::prelude::{NpcPlugins, run_ticks};
use village_content::VillagePlugin;
use village_content::scenario::spawn_village;

fn village(seed: u64) -> App {
    let mut app = App::new();
    app.add_plugins((NpcPlugins::new().seed(seed).without_driver(), VillagePlugin));
    spawn_village(app.world_mut());
    app
}

/// Runs `app` to each checkpoint tick in turn and returns the hash at each.
fn hashes_at(app: &mut App, checkpoints: &[u64]) -> Vec<u64> {
    let mut done = 0;
    checkpoints
        .iter()
        .map(|&t| {
            run_ticks(app.world_mut(), t - done);
            done = t;
            state_hash(app.world_mut())
        })
        .collect()
}

#[test]
fn same_seed_same_hash() {
    let checkpoints = [5_000, 10_000, 20_000];
    let a = hashes_at(&mut village(42), &checkpoints);
    let b = hashes_at(&mut village(42), &checkpoints);
    assert_eq!(a, b);
    let c = hashes_at(&mut village(43), &[20_000]);
    assert_ne!(a[2], c[0], "a different seed should give a different state");
}

#[test]
fn hash_independent_of_batching() {
    let mut whole = village(42);
    run_ticks(whole.world_mut(), 20_000);
    let mut batched = village(42);
    for _ in 0..4_000 {
        run_ticks(batched.world_mut(), 5);
    }
    assert_eq!(
        state_hash(whole.world_mut()),
        state_hash(batched.world_mut())
    );
}
