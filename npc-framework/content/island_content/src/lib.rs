//! Island Village content: the regional living island (`village`), its one
//! shipped construction ([`construction`]) and the base it stands on.
//!
//! - [`IslandBasePlugin`] registers the base by stable name: item kinds,
//!   skills, traits, sheet templates and recipes ([`content`]), the played
//!   unit's tree ([`tree`]) and its idle action ([`actions`]), and the unit
//!   and prop components ([`world`]). The island itself registers in
//!   [`village::prepare`].
//! - [`scenario`]: the island scenario (`content: "island"`), the loader for
//!   `npc_agent_io` and the headless constructors ([`try_new_sim_app`]),
//!   which build [`construction::shipped`], the same world the game ships.
//! - [`layout`]: unit and item spawning and [`layout::ascii_map`].
//!
//! ```ignore
//! let mut app = island_content::new_sim_app(7, "living_world");
//! npc_framework::prelude::run_ticks(app.world_mut(), 600);
//! ```
//!
//! The capabilities come from `NpcPlugins::new().with_perception()
//! .with_rules().with_agent_io().with_interaction().with_command()
//! .with_vehicles().with_progression().with_crafting()` (plus
//! `with_per_roll_detection()` where the world asks for it), `npc_nav`'s
//! `NavPlugin`, and their glue; see [`scenario::island_plugins`].

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
pub mod actors;
pub mod agent;
pub mod clipboard;
pub mod construction;
pub mod content;
pub mod layout;
pub mod replay;
pub mod scenario;
pub mod summary;
pub mod tree;
pub mod village;
pub mod world;
pub mod world_runtime;
pub mod worlds;

use bevy_app::prelude::*;
use bevy_ecs::prelude::*;
use npc_framework::kernel::registry::ensure_frozen;
use npc_framework::prelude::*;

pub use agent::IslandAgentPlugin;
pub use scenario::{
    SCENARIO_FILES, load_scenario, new_sim_app, register_scenario_loader, try_new_sim_app,
    try_new_sim_app_with,
};
pub use world::{Behaviour, IslandIds, IslandUnit, Prop, PropKind};

use crate::actions::{ISLAND_IDLE, IslandIdling};

/// Registers the island's base content (see the crate docs). Needs the
/// capability plugins (`scenario::island_plugins`) and `NavPlugin`.
pub struct IslandBasePlugin;

impl Plugin for IslandBasePlugin {
    fn build(&self, app: &mut App) {
        content::register_items(app);
        content::register_rules(app);
        content::register_recipes(app);
        app.register_action(ISLAND_IDLE, actions::idle_start, actions::idle_stop);
        tree::register_trees(app);
        app.register_sim_component::<IslandUnit>()
            .register_sim_component::<Prop>()
            .register_sim_component::<IslandIdling>()
            // Not hashed (a spawn mirror), saved so names survive a resume.
            .register_snapshot_resource_named::<IslandIds>("island_ids")
            // Actor presets (Task 11): snapshot-only, never hashed.
            .register_snapshot_component_named::<actors::AllowedActions>(actors::ALLOWED_ACTIONS)
            .register_snapshot_component_named::<actors::ActorModel>(actors::ACTOR_MODEL)
            .register_verb_gate(actors::ALLOWED_ACTIONS_GATE, actors::verb_gate)
            .register_kind_name("island", world::kind_name)
            .add_systems(NpcSimTick, actions::idle_system.in_set(NpcSet::Actions));
        register_perf_budgets(app);
    }

    fn finish(&self, app: &mut App) {
        finish_content(app.world_mut());
    }
}

/// Freezes content (if the kernel has not yet). Idempotent.
pub fn finish_content(world: &mut World) {
    ensure_frozen(world);
}

/// p95 budgets of the testbed's own action mechanics, `(mechanic, fixed
/// µs, per-agent µs)` (registered by [`IslandBasePlugin`]). `actions.idle` is
/// the same work as the village's (and has the village's row);
/// `actions.move` is the framework's.
pub const ISLAND_BUDGETS: &[(&str, u64, u64)] = &[("actions.idle", 10, 2)];

/// Registers [`ISLAND_BUDGETS`] (`PerfAppExt::register_budget`).
pub fn register_perf_budgets(app: &mut App) {
    use npc_framework::perf::PerfAppExt;
    for &(mechanic, fixed, per_agent) in ISLAND_BUDGETS {
        app.register_budget(mechanic, fixed, per_agent);
    }
}

/// Mechanic tags for the island's base sim systems (`npc_perf`), on top of
/// `MechanicTags::default_npc()`: the idle countdown is `actions.idle`, the
/// island tick `village.economy`.
pub fn perf_tags(tags: &mut npc_framework::perf::MechanicTags) {
    for (prefix, mechanic) in [
        ("island_content::actions::idle_system", "actions.idle"),
        ("island_content::village::tick", "village.economy"),
    ] {
        tags.add(prefix, mechanic);
    }
}

#[cfg(test)]
mod tests {
    use std::any::type_name_of_val;

    use npc_framework::perf::{MechanicTags, UNTAGGED};

    use super::*;

    /// The base system `IslandBasePlugin` schedules in `NpcSimTick` maps to a
    /// tagged mechanic with a budget.
    #[test]
    fn perf_tags_cover_the_sim_systems() {
        let mut tags = MechanicTags::default_npc();
        perf_tags(&mut tags);
        let system = type_name_of_val(&actions::idle_system);
        assert_eq!(tags.mechanic_for(system), "actions.idle", "{system}");
        assert_ne!(tags.mechanic_for(system), UNTAGGED, "{system}");
        assert!(ISLAND_BUDGETS.iter().any(|(m, _, _)| *m == "actions.idle"));
    }
}
