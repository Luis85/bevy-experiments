//! Actor Testbed content (actor-capabilities spec §4.9): a compact walled
//! compound with **every** opt-in capability on, as both a showcase and the
//! integration test.
//!
//! - [`TestbedPlugin`] registers the content by stable name: item kinds,
//!   skills, traits, sheet templates and recipes ([`content`]), the unit
//!   trees ([`tree`]) and their actions/conditions ([`actions`]), the sim
//!   components ([`world`]) and the `TestbedDrive` command ([`commands`]).
//! - [`scenario`]: scenario files (`content: "testbed"`), the loader for
//!   `npc_agent_io` and the headless constructors ([`try_new_sim_app`]).
//! - [`layout`]: the compound (walls, doors, furnishing, actors) and
//!   [`layout::ascii_map`].
//! - [`agent`] / [`walls`]: the testbed's observation sections for external
//!   agents (who the unit is; the walls and openings around it).
//!
//! ```ignore
//! let mut app = testbed_content::new_sim_app(1, "testbed");
//! npc_framework::prelude::run_ticks(app.world_mut(), 600);
//! ```
//!
//! The capabilities come from `NpcPlugins::new().with_perception()
//! .with_rules().with_agent_io().with_interaction().with_command()
//! .with_vehicles().with_progression().with_crafting()` (plus
//! `with_per_roll_detection()`), `npc_nav`'s `NavPlugin`, and their glue;
//! see [`scenario::testbed_plugins`]. Nothing here lives in a core crate.

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
pub mod commands;
pub mod content;
pub mod explorer;
pub mod layout;
pub mod replay;
pub mod scenario;
pub mod summary;
pub mod tree;
pub mod walls;
pub mod world;

use bevy_app::prelude::*;
use bevy_ecs::prelude::*;
use npc_framework::kernel::registry::ensure_frozen;
use npc_framework::perception::PerceptionAppExt;
use npc_framework::prelude::*;

pub use agent::TestbedAgentPlugin;
pub use commands::TestbedDrive;
pub use scenario::{
    SCENARIO_FILES, load_scenario, new_sim_app, register_scenario_loader, try_new_sim_app,
    try_new_sim_app_with,
};
pub use world::{Behaviour, Prop, PropKind, TestbedIds, TestbedUnit};

use crate::actions::{
    COND_HAS_TOOL_WORK, TB_FIND_TOOL_WORK, TB_IDLE, TB_PICK_WANDER, TB_WALK, TbIdling, TbWalking,
};

/// Registers the testbed content (see the crate docs). Needs the
/// capability plugins (`scenario::testbed_plugins`) and `NavPlugin`.
pub struct TestbedPlugin;

impl Plugin for TestbedPlugin {
    fn build(&self, app: &mut App) {
        content::register_items(app);
        content::register_rules(app);
        content::register_recipes(app);
        app.register_action(TB_IDLE, actions::idle_start, actions::idle_stop)
            .register_action(TB_WALK, actions::walk_start, actions::walk_stop)
            .register_action(
                TB_PICK_WANDER,
                actions::pick_wander_start,
                actions::instant_stop,
            )
            .register_action(
                TB_FIND_TOOL_WORK,
                actions::find_tool_work_start,
                actions::instant_stop,
            )
            .register_cond(COND_HAS_TOOL_WORK, actions::cond_has_tool_work);
        tree::register_trees(app);
        app.register_sim_component::<TestbedUnit>()
            .register_sim_component::<Prop>()
            .register_sim_component::<TbIdling>()
            .register_sim_component::<TbWalking>()
            .register_command::<TestbedDrive>()
            // Not hashed (a spawn mirror), saved so names survive a resume.
            .register_snapshot_resource_named::<TestbedIds>("testbed_ids")
            // Actor presets (Task 11): snapshot-only, never hashed.
            .register_snapshot_component_named::<actors::AllowedActions>(actors::ALLOWED_ACTIONS)
            .register_snapshot_component_named::<actors::ActorModel>(actors::ACTOR_MODEL)
            .register_verb_gate(actors::ALLOWED_ACTIONS_GATE, actors::verb_gate)
            .register_kind_name("testbed", world::kind_name)
            // Not hashed: only shapes the fog record of actors that reveal
            // fog (the explorer's, `explorer::host`).
            .set_fog_statics(world::fog_statics())
            // Not hashed either: only shapes robots' LIDAR readings.
            .set_lidar_obstacles(world::lidar_obstacles())
            .add_systems(
                NpcSimTick,
                (
                    (actions::walk_cancel_system, actions::idle_system)
                        .chain()
                        .in_set(NpcSet::Actions),
                    actions::walk_arrival_system
                        .in_set(NpcSet::Locomotion)
                        .after(NpcMoverSet),
                ),
            );
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
/// µs, per-agent µs)` (registered by [`TestbedPlugin`]). `actions.idle` is
/// the same work as the village's (and has the village's row);
/// `actions.move` is the framework's.
pub const TESTBED_BUDGETS: &[(&str, u64, u64)] = &[("actions.idle", 10, 2)];

/// Registers [`TESTBED_BUDGETS`] (`PerfAppExt::register_budget`).
pub fn register_perf_budgets(app: &mut App) {
    use npc_framework::perf::PerfAppExt;
    for &(mechanic, fixed, per_agent) in TESTBED_BUDGETS {
        app.register_budget(mechanic, fixed, per_agent);
    }
}

/// Mechanic tags for the testbed's sim systems (`npc_perf`), on top of
/// `MechanicTags::default_npc()`, named like the village's
/// (`village_content::perf_tags`): the walk systems are `actions.move`, the
/// idle countdown `actions.idle`.
pub fn perf_tags(tags: &mut npc_framework::perf::MechanicTags) {
    for (prefix, mechanic) in [
        (
            "testbed_content::actions::walk_cancel_system",
            "actions.move",
        ),
        (
            "testbed_content::actions::walk_arrival_system",
            "actions.move",
        ),
        ("testbed_content::actions::idle_system", "actions.idle"),
    ] {
        tags.add(prefix, mechanic);
    }
}

#[cfg(test)]
mod tests {
    use std::any::type_name_of_val;

    use npc_framework::perf::{MechanicTags, UNTAGGED};

    use super::*;

    /// Every system `TestbedPlugin` schedules in `NpcSimTick` (Bevy names a
    /// function system by its type path) maps to a tagged mechanic.
    #[test]
    fn perf_tags_cover_the_sim_systems() {
        let mut tags = MechanicTags::default_npc();
        perf_tags(&mut tags);
        let cases = [
            (
                type_name_of_val(&actions::walk_cancel_system),
                "actions.move",
            ),
            (
                type_name_of_val(&actions::walk_arrival_system),
                "actions.move",
            ),
            (type_name_of_val(&actions::idle_system), "actions.idle"),
        ];
        for (system, mechanic) in cases {
            assert_eq!(tags.mechanic_for(system), mechanic, "{system}");
            assert_ne!(tags.mechanic_for(system), UNTAGGED, "{system}");
        }
        // Each tagged mechanic has a budget: the framework's or ours.
        for mechanic in ["actions.move", "actions.idle"] {
            let framework = npc_framework::perf::NPC_BUDGET_TABLE
                .iter()
                .any(|(m, _, _)| *m == mechanic);
            let ours = TESTBED_BUDGETS.iter().any(|(m, _, _)| *m == mechanic);
            assert!(framework || ours, "{mechanic} has no budget");
        }
    }
}
