//! System name → mechanic mapping.

use bevy_ecs::prelude::Resource;
use serde::{Deserialize, Serialize};

/// Mechanic id for systems no tag prefix matches.
pub const UNTAGGED: &str = "untagged";

/// An ordered list of `(prefix, mechanic)` pairs. A system name maps to the
/// mechanic of the **longest** matching prefix, or [`UNTAGGED`].
///
/// Bevy names a function system by its full type path (`std::any::type_name`
/// of the fn item), e.g. `npc_needs::decay_needs_system`. Closures appear
/// as `path::to::fn::{{closure}}` and generic systems as `path::fn<T>`, so
/// plain prefix matching on module paths works for all of them. Leading `<`
/// and `&` (as in `<T as Trait>::f`) are skipped before matching.
///
/// Note: without Bevy's `debug` feature (`bevy_ecs/debug`, on by default in
/// `bevy`) every system is named `<Enable the debug feature to see the name>`
/// and lands in [`UNTAGGED`].
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Resource)]
pub struct MechanicTags {
    entries: Vec<(String, String)>,
}

impl MechanicTags {
    /// Tags from `(prefix, mechanic)` pairs. A repeated prefix replaces the
    /// earlier entry.
    pub fn new<P, M>(entries: impl IntoIterator<Item = (P, M)>) -> Self
    where
        P: Into<String>,
        M: Into<String>,
    {
        let mut tags = Self::default();
        for (prefix, mechanic) in entries {
            tags.add(prefix, mechanic);
        }
        tags
    }

    /// Adds (or replaces) the mechanic for `prefix`.
    pub fn add(&mut self, prefix: impl Into<String>, mechanic: impl Into<String>) -> &mut Self {
        let (prefix, mechanic) = (prefix.into(), mechanic.into());
        match self.entries.iter_mut().find(|(p, _)| *p == prefix) {
            Some(entry) => entry.1 = mechanic,
            None => self.entries.push((prefix, mechanic)),
        }
        self
    }

    /// Builder form of [`MechanicTags::add`].
    pub fn with(mut self, prefix: impl Into<String>, mechanic: impl Into<String>) -> Self {
        self.add(prefix, mechanic);
        self
    }

    /// The `(prefix, mechanic)` pairs in insertion order.
    pub fn entries(&self) -> &[(String, String)] {
        &self.entries
    }

    /// The mechanic for a system name: longest matching prefix, else
    /// [`UNTAGGED`].
    pub fn mechanic_for(&self, system_name: &str) -> &str {
        let name = system_name.trim_start_matches(['<', '&', ' ']);
        self.entries
            .iter()
            .filter(|(prefix, _)| name.starts_with(prefix.as_str()))
            .max_by_key(|(prefix, _)| prefix.len())
            .map_or(UNTAGGED, |(_, mechanic)| mechanic.as_str())
    }

    /// Tags for the framework's generic systems (content adds its own on
    /// top, e.g. `village_content::perf_tags`):
    ///
    /// | prefix | mechanic |
    /// |---|---|
    /// | `npc_needs` | `needs` |
    /// | `npc_kernel::clock` | `clock` (`rethink_timer_system` → `think`) |
    /// | `npc_kernel::schedule::think_system`, `npc_ai` | `think` |
    /// | `npc_kernel::math` (`copy_prev_pos_system`) | `actions.move` |
    /// | `npc_kernel::spatial` (grid index, blockers) | `spatial` |
    /// | `npc_locomotion`, `npc_nav::mover` (movers) | `locomotion` |
    /// | `npc_nav` (request bookkeeping; the search itself is the `nav.astar` span) | `nav` |
    /// | `npc_perception` (sensing pass, facing update) | `perception` |
    /// | `npc_perception::fog` (`fog_system`: fog of war; also a `fog` span) | `fog` |
    /// | `npc_framework::glue::perception_rules` (journaling `per_roll` rolls) | `perception` |
    /// | `npc_rules` (systems; rolls also run in a `rules` mechanic span) | `rules` |
    /// | `npc_agent_io` (the `ExternalAction` passes) | `agent_io` |
    /// | `npc_interaction` (door sync, doors on paths, completions; also an `interaction` span) | `interaction` |
    /// | `npc_progression` (the roll-feed pass; also a `progression` span) | `progression` |
    /// | `npc_crafting` (craft completions; also a `crafting` span) | `crafting` |
    /// | `npc_command` (the order engine: queues, group follow-ups) | `command` |
    /// | `npc_vehicles` (drivers, crew gate, seat follow and claims; also a `vehicles` span) | `vehicles` |
    /// | `npc_kernel::{commands,orders,driver::apply_host_positions_system}` | `inputs` |
    /// | `npc_kernel::journal` | `journal` |
    /// | `npc_kernel::{replay,inspect,hash}` | `debug` |
    /// | `npc_kernel::driver::drive_sim` | `driver` (inclusive) |
    /// | `bevy_app::main_schedule` (`Main::run_main`, ..) | `frame` (inclusive) |
    /// | `npc_view_helpers::<module>` | `view.<module>` |
    /// | `npc_tools` / `npc_perf` | `remote` / `perf` |
    /// | `bevy_render`, `bevy_pbr`, `bevy_core_pipeline`, … | `render` |
    /// | `bevy_` (everything else) | `engine` |
    ///
    /// `driver` (`drive_sim`) is *inclusive*: it runs the whole
    /// `NpcSimTick` schedule, so its time also contains every sim mechanic;
    /// `frame` (Bevy's `Main::run_main`, which runs every schedule of the
    /// frame) contains everything else, so it is kept out of `engine`.
    /// Other framework systems are deliberately left `untagged` so that a
    /// new system without a tag shows up. `"mechanic"` spans need no tag:
    /// their name is the mechanic (`npc_ai` emits `think.agent`,
    /// `think.cond`, `think.start` and `think.stop`, nested inside `think`).
    pub fn default_npc() -> Self {
        let mut tags = Self::new([
            ("npc_needs", "needs"),
            ("npc_kernel::clock", "clock"),
            ("npc_kernel::clock::rethink_timer_system", "think"),
            ("npc_kernel::schedule::think_system", "think"),
            ("npc_ai", "think"),
            ("npc_kernel::math", "actions.move"),
            ("npc_kernel::spatial", "spatial"),
            ("npc_locomotion", "locomotion"),
            ("npc_nav", "nav"),
            ("npc_nav::mover", "locomotion"),
            ("npc_perception", "perception"),
            ("npc_perception::fog", "fog"),
            ("npc_framework::glue::perception_rules", "perception"),
            ("npc_rules", "rules"),
            ("npc_agent_io", "agent_io"),
            ("npc_interaction", "interaction"),
            ("npc_progression", "progression"),
            ("npc_crafting", "crafting"),
            ("npc_command", "command"),
            ("npc_vehicles", "vehicles"),
            ("npc_kernel::commands", "inputs"),
            ("npc_kernel::orders", "inputs"),
            ("npc_kernel::driver::apply_host_positions_system", "inputs"),
            ("npc_kernel::driver::drive_sim", "driver"),
            ("npc_kernel::journal", "journal"),
            ("npc_kernel::replay", "debug"),
            ("npc_kernel::inspect", "debug"),
            ("npc_kernel::hash", "debug"),
            ("npc_tools", "remote"),
            ("npc_perf", "perf"),
            ("bevy_", "engine"),
            ("bevy_app::main_schedule", "frame"),
        ]);
        for module in [
            "driver_ui",
            "interp",
            "journal_panel",
            "perf_panel",
            "screenshot",
            "select",
            "visuals",
        ] {
            tags.add(
                format!("npc_view_helpers::{module}"),
                format!("view.{module}"),
            );
        }
        tags.add("npc_view_helpers", "view");
        for render in [
            "bevy_render",
            "bevy_pbr",
            "bevy_core_pipeline",
            "bevy_sprite_render",
            "bevy_ui_render",
            "bevy_gizmos_render",
            "bevy_anti_alias",
            "bevy_post_process",
            "bevy_extract",
            "bevy_material",
            "bevy_shader",
        ] {
            tags.add(render, "render");
        }
        tags
    }
}
