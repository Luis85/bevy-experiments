//! Example content for `npc_framework`: the villager.
//!
//! A villager has three needs (hunger, energy, thirst), lives in a house with
//! a bed, eats from a stockpile that a field refills, drinks at a well,
//! follows RTS-style orders, and
//! runs one behaviour tree (see [`villager`]). Everything is registered by
//! stable name through the facade's `NpcAppExt` in [`VillagePlugin`]; the
//! framework crates know nothing about villagers.
//!
//! ```ignore
//! app.add_plugins((NpcPlugins::new().seed(1), VillagePlugin));
//! ```
//!
//! Headless: [`new_sim_app`] builds a finished app with a scenario spawned
//! (it is also the village's replay `AppFactory`, see [`replay`]).

// Content is sim code: the same determinism guardrails as the kernel (this
// crate's clippy.toml is a copy of the kernel's).
#![deny(
    clippy::float_arithmetic,
    clippy::float_cmp,
    clippy::iter_over_hash_type,
    clippy::disallowed_types,
    clippy::disallowed_methods
)]

pub mod actions;
pub mod activity;
pub mod agent;
pub mod commands;
pub mod effects;
pub mod inspect;
pub mod needs;
pub mod orders;
pub mod replay;
pub mod scenario;
pub mod villager;
pub mod world;

use bevy_app::prelude::*;
use bevy_ecs::prelude::*;
use npc_framework::kernel::registry::ensure_frozen;
use npc_framework::prelude::*;

pub use activity::Activity;
pub use commands::{SetFood, SpawnVillager};
pub use needs::{ENERGY, HUNGER, THIRST};
pub use scenario::{
    SCENARIOS, VillageIds, new_sim_app, spawn_village, spawn_village_n, spawn_village_nav,
    try_new_sim_app, try_new_sim_app_with,
};
pub use villager::{VILLAGER_TREE, VillagerTree};
pub use world::{Bed, Home, Place, PlaceKind, Stockpile, Villager, Well, WorkSite};

use crate::actions::{
    Drink, Eat, Idle, MoveTo, Sleep, Work, drink_system, eat_system, idle_system,
    move_arrival_system, move_to_system, sleep_system, work_system,
};
use crate::effects::{SimEffect, resolve_effects_system};
use crate::orders::{OrderMove, order_system};

/// Registers the village needs.
pub fn register_needs(app: &mut App) {
    app.register_need(needs::HUNGER_DEF)
        .register_need(needs::ENERGY_DEF)
        .register_need(needs::THIRST_DEF);
}

/// Registers the villager tree (by name).
pub fn register_tree(app: &mut App) {
    app.register_tree(VILLAGER_TREE, villager::villager_tree);
}

/// Registers the village's hashed sim components and reflected types.
pub fn register_components(app: &mut App) {
    app.register_sim_component::<Activity>()
        .register_sim_component::<Stockpile>()
        .register_sim_component::<OrderMove>()
        .register_type::<Villager>()
        .register_type::<Place>();
    app.register_sim_component::<MoveTo>()
        .register_sim_component::<Work>()
        .register_sim_component::<Eat>()
        .register_sim_component::<Drink>()
        .register_sim_component::<Sleep>()
        .register_sim_component::<Idle>();
    register_snapshot_entries(app);
}

/// Registers the village state that is not hashed but must survive a world
/// snapshot (ADR 0013): the places and the villager marker (their identity
/// and links), and `VillageIds` (a spawn mirror, so tools keep finding
/// things after a resume). Not part of the content hash.
pub fn register_snapshot_entries(app: &mut App) {
    app.register_snapshot_component_named::<Villager>("villager")
        .register_snapshot_component_named::<Place>("place")
        .register_snapshot_component_named::<Home>("home")
        .register_snapshot_component_named::<Bed>("bed")
        .register_snapshot_component_named::<WorkSite>("work_site")
        .register_snapshot_component_named::<Well>("well")
        .register_snapshot_resource_named::<VillageIds>("village_ids");
}

/// Registers the village's content commands.
pub fn register_commands(app: &mut App) {
    app.register_command::<SpawnVillager>()
        .register_command::<SetFood>();
}

/// Adds the village's systems: action systems in `NpcSet::Actions` (chained
/// in a fixed order: they all write `ActiveLeaf`), the walk's arrival check
/// in `NpcSet::Locomotion` after the movers (`NpcMoverSet`, so a walk ends
/// in the tick it arrives), and the effect resolver.
///
/// Walking goes through the locomotion seam: `MoveToDest` / `FollowOrder`
/// write a `MoveIntent` that `NpcPlugins`' `StraightLineMover` (or, for
/// `NavAgent`s, `npc_nav`'s `GridPathMover`) follows.
pub fn add_systems(app: &mut App) {
    app.add_message::<SimEffect>().add_systems(
        NpcSimTick,
        (
            (
                order_system,
                move_to_system,
                eat_system,
                drink_system,
                work_system,
                sleep_system,
                idle_system,
            )
                .chain()
                .in_set(NpcSet::Actions),
            move_arrival_system
                .in_set(NpcSet::Locomotion)
                .after(NpcMoverSet),
            resolve_effects_system.in_set(NpcSet::Effects),
        ),
    );
}

/// The villager unit type: needs, actions, conditions, tree, sim components,
/// commands, kind names, the village inspector and systems. Add it with
/// `NpcPlugins` (before or after: registration order never matters).
pub struct VillagePlugin;

impl Plugin for VillagePlugin {
    fn build(&self, app: &mut App) {
        register_needs(app);
        villager::register_actions_and_conds(app);
        register_tree(app);
        register_components(app);
        register_commands(app);
        app.register_kind_name("village", world::kind_name)
            .set_inspector(inspect::inspector());
        add_systems(app);
        #[cfg(feature = "perf")]
        register_perf_budgets(app);
    }

    fn finish(&self, app: &mut App) {
        finish_content(app.world_mut());
    }
}

/// Freezes content (if the kernel has not yet) and inserts [`VillagerTree`].
/// Idempotent.
pub fn finish_content(world: &mut World) {
    if world.contains_resource::<VillagerTree>() {
        return;
    }
    ensure_frozen(world);
    let tree = world
        .resource::<AiRegistry>()
        .tree_id(VILLAGER_TREE)
        .expect("villager tree registered by VillagePlugin");
    world.insert_resource(VillagerTree(tree));
}

/// p95 budgets of the village's own action mechanics, `(mechanic, fixed
/// µs, per-villager µs)` (registered by [`VillagePlugin`] with feature
/// `perf`; `actions.move` is the framework's).
pub const VILLAGE_BUDGETS: &[(&str, u64, u64)] = &[
    ("actions.eat", 10, 2),
    ("actions.drink", 10, 2),
    ("actions.work", 10, 2),
    ("actions.sleep", 10, 2),
    ("actions.idle", 10, 2),
];

/// Registers [`VILLAGE_BUDGETS`] (`PerfAppExt::register_budget`).
#[cfg(feature = "perf")]
pub fn register_perf_budgets(app: &mut App) {
    use npc_framework::perf::PerfAppExt;
    for &(mechanic, fixed, per_agent) in VILLAGE_BUDGETS {
        app.register_budget(mechanic, fixed, per_agent);
    }
}

/// Mechanic tags for the village's systems (`npc_perf`), on top of
/// `MechanicTags::default_npc()`; the mechanic names match the
/// pre-split ones, so bench baselines stay comparable.
#[cfg(feature = "perf")]
pub fn perf_tags(tags: &mut npc_framework::perf::MechanicTags) {
    for (prefix, mechanic) in [
        ("village_content::actions::move_to_system", "actions.move"),
        (
            "village_content::actions::move_arrival_system",
            "actions.move",
        ),
        ("village_content::actions::eat_system", "actions.eat"),
        ("village_content::actions::drink_system", "actions.drink"),
        ("village_content::actions::work_system", "actions.work"),
        ("village_content::actions::sleep_system", "actions.sleep"),
        ("village_content::actions::idle_system", "actions.idle"),
        ("village_content::effects", "effects"),
        ("village_content::orders", "inputs"),
    ] {
        tags.add(prefix, mechanic);
    }
}
