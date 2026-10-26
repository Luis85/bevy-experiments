//! Plan Task P3 (Review Focus 2): plugin order and content registration
//! order never change ids, the content hash or the state hash.

use bevy_app::App;
use npc_framework::kernel::hash::{SimHashRegistry, state_hash};
use npc_framework::kernel::registry::{ContentHash, KernelAppExt, content_hash};
use npc_framework::prelude::{AiRegistry, NeedRegistry, NpcPlugins, run_ticks};
use village_content::{VillagePlugin, finish_content, spawn_village};

/// The usual order: framework first, then the content plugin.
fn framework_first(seed: u64) -> App {
    let mut app = App::new();
    app.add_plugins((NpcPlugins::new().seed(seed).without_driver(), VillagePlugin));
    app.finish();
    app.cleanup();
    spawn_village(app.world_mut());
    app
}

/// Content registered piece by piece in reverse order, *before* the
/// framework plugins exist.
fn content_first_reversed(seed: u64) -> App {
    let mut app = App::new();
    village_content::add_systems(&mut app);
    village_content::register_commands(&mut app);
    village_content::register_components(&mut app);
    village_content::register_tree(&mut app);
    village_content::villager::register_actions_and_conds(&mut app);
    village_content::register_needs(&mut app);
    app.register_kind_name("village", village_content::world::kind_name)
        .set_inspector(village_content::inspect::inspector());
    app.add_plugins(NpcPlugins::new().seed(seed).without_driver());
    app.finish();
    app.cleanup();
    finish_content(app.world_mut());
    spawn_village(app.world_mut());
    app
}

/// Every id-bearing registry as `(kind, name)` in id order.
fn ids(app: &App) -> Vec<(String, String)> {
    let w = app.world();
    let ai = w.resource::<AiRegistry>();
    let mut out: Vec<(String, String)> = Vec::new();
    out.extend(ai.actions.iter().map(|a| ("action".into(), a.name.into())));
    out.extend(ai.conds.iter().map(|c| ("cond".into(), c.0.into())));
    out.extend(ai.trees.iter().map(|t| ("tree".into(), t.name.clone())));
    out.extend(
        w.resource::<NeedRegistry>()
            .defs
            .iter()
            .map(|d| ("need".into(), d.name.into())),
    );
    let hashed = w.resource::<SimHashRegistry>();
    out.extend(
        hashed
            .components
            .iter()
            .map(|c| ("component".into(), c.name.into())),
    );
    out.extend(
        hashed
            .resources
            .iter()
            .map(|r| ("resource".into(), r.name.into())),
    );
    out
}

#[test]
fn registration_order_independent() {
    let mut a = framework_first(3);
    let mut b = content_first_reversed(3);
    assert_eq!(ids(&a), ids(&b), "ids (registry order) differ");
    assert_eq!(
        a.world().resource::<AiRegistry>().trees,
        b.world().resource::<AiRegistry>().trees,
        "resolved trees differ"
    );
    assert_eq!(
        a.world().resource::<ContentHash>(),
        b.world().resource::<ContentHash>()
    );
    assert_eq!(content_hash(a.world_mut()), content_hash(b.world_mut()));
    for _ in 0..5 {
        run_ticks(a.world_mut(), 1000);
        run_ticks(b.world_mut(), 1000);
        assert_eq!(state_hash(a.world_mut()), state_hash(b.world_mut()));
    }
    // Ids really are name-sorted.
    let names: Vec<&str> = a
        .world()
        .resource::<AiRegistry>()
        .actions
        .iter()
        .map(|x| x.name)
        .collect();
    let mut sorted = names.clone();
    sorted.sort_unstable();
    assert_eq!(names, sorted);
}

#[test]
fn content_hash_covers_names() {
    let mut a = framework_first(1);
    let manifest = a.world().resource::<ContentHash>().manifest.clone();
    for want in [
        ("action", "EatFood"),
        ("cond", "has_order"),
        ("tree", "villager"),
        ("need", "Hunger"),
        ("sim_component", "needs"),
        ("sim_resource", "reservations"),
        ("command", "SpawnVillager"),
    ] {
        assert!(
            manifest.iter().any(|(k, n)| k == want.0 && n == want.1),
            "{want:?} missing from the content manifest"
        );
    }
    assert!(manifest.iter().any(|(k, _)| k == "tree_hash"));
    // Same content, same hash, whatever the seed.
    assert_eq!(
        content_hash(a.world_mut()),
        content_hash(framework_first(9).world_mut())
    );
}
