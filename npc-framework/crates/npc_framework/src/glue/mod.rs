//! Integration glue between the opt-in capability crates (ADR 0011).
//!
//! The capability crates never depend on each other beyond ADR 0012's
//! whitelist (progression → rules, crafting → interaction + rules,
//! vehicles → interaction): each one exposes hooks by stable name
//! (detection checks, observation/perception/affordance providers, action
//! handlers, content commands). The pieces that need **two** capabilities
//! live here, each in a module compiled only when both facade features are
//! on, and each behind its own plugin, so nothing changes until a host adds
//! it (or asks [`NpcPlugins`](crate::NpcPlugins) for both capabilities):
//!
//! | Module | Features | Plugin | Adds |
//! |---|---|---|---|
//! | [`perception_rules`] | `perception` + `rules` | [`PerceptionRulesPlugin`] | detection check `"per_roll"` (a Per roll, journaled with purpose `perception.detect`), [`Stealthy`], [`PerRollConfig`], target modifier hooks |
//! | [`perception_agent`] | `perception` + `agent-io` | [`PerceptionAgentPlugin`] | perception provider `perception`: `agent/observe` shows only what the unit perceives (+ memory), `omniscient: false` |
//! | [`fog_agent`] | `perception` + `agent-io` | (none: used by `perception_agent`) | the `searched` summary of the perception section: the share of 10 m cells explored within 60 m, per compass sector (proposal 004) |
//! | [`rules_agent`] | `rules` + `agent-io` | [`RulesAgentPlugin`] | section `sheet`, affordance/handler `spend_points`, the recorded command `SpendPoints` |
//! | [`interaction_rules`] | `interaction` + `rules` | [`InteractionRulesPlugin`] | interaction skill checks roll with `npc_rules` (skill-check hook `npc_rules`) |
//! | [`interaction_perception`] | `interaction` + `perception` | [`InteractionPerceptionPlugin`] | interaction noises (doors, tools) become `Noise` stimuli (noise hook `npc_perception`) |
//! | [`interaction_agent`] | `interaction` + `agent-io` | [`InteractionAgentPlugin`] | affordances (with `why_not`) and handlers for every verb, section `inventory` |
//! | [`command_interaction`] | `command` + `interaction` | [`CommandInteractionPlugin`] | the `Interact` order (walk into reach, then the verb; fails with its reason), [`InteractOrder`]; with `view-helpers` the item/door context orders |
//! | [`command_vehicles`] | `command` + `vehicles` | [`CommandVehiclesPlugin`] | `ByCrew` orders forwarded to the driver, drivers drive (`Move`/`AttackMove`), `Mount` (walk, then mount) and `Unload` orders; with `view-helpers` vehicle → `Mount` |
//! | [`crafting_interaction`] | `crafting` (+ `interaction`) | [`CraftingInteractionPlugin`] | interaction busy predicate `npc_crafting`: no interaction starts while the unit crafts |
//! | [`progression_crafting`] | `progression` + `crafting` | [`ProgressionCraftingPlugin`] | craft hook `npc_progression`: XP (`craft`) and practice per completed craft |
//! | [`progression_interaction`] | `progression` + `interaction` | [`ProgressionInteractionPlugin`] | verbs `study` ([`Teaches`] books) and `teach` ([`Teacher`] units) |
//! | [`command_agent`] | `command` + `agent-io` | [`CommandAgentPlugin`] | section `command`, actions `order` (any owned units, formations) and `execute_orders` |
//! | [`vehicles_agent`] | `vehicles` + `agent-io` | [`VehiclesAgentPlugin`] | `mount`/`dismount`/`drive_to`/`unload` affordances (with `why_not`), the `DriveTo` command, section `vehicle` |
//! | [`progression_agent`] | `progression` + `agent-io` | [`ProgressionAgentPlugin`] | section `progression` (XP, level, learning) |
//! | [`crafting_agent`] | `crafting` + `agent-io` | [`CraftingAgentPlugin`] | `craft:<recipe>` affordances per nearby workstation (with `why_not`), handlers, section `crafting` |
//!
//! The BRP method `npc/sheet {unit}` is `npc_tools`' (features `remote` +
//! `rules`). Every module is sim code: integers only, `SimId` order, no
//! clocks (the rules in `.claude/rules/sim.md` apply).

#[cfg(all(feature = "command", feature = "agent-io"))]
pub mod command_agent;
#[cfg(all(feature = "command", feature = "interaction"))]
pub mod command_interaction;
#[cfg(all(feature = "command", feature = "vehicles"))]
pub mod command_vehicles;
#[cfg(all(feature = "crafting", feature = "agent-io"))]
pub mod crafting_agent;
#[cfg(feature = "crafting")]
pub mod crafting_interaction;
#[cfg(all(feature = "perception", feature = "agent-io"))]
pub mod fog_agent;
#[cfg(all(feature = "interaction", feature = "agent-io"))]
pub mod interaction_agent;
#[cfg(all(feature = "interaction", feature = "perception"))]
pub mod interaction_perception;
#[cfg(all(feature = "interaction", feature = "rules"))]
pub mod interaction_rules;
#[cfg(all(feature = "perception", feature = "agent-io"))]
pub mod perception_agent;
#[cfg(all(feature = "perception", feature = "rules"))]
pub mod perception_rules;
#[cfg(all(feature = "progression", feature = "agent-io"))]
pub mod progression_agent;
#[cfg(all(feature = "progression", feature = "crafting"))]
pub mod progression_crafting;
#[cfg(all(feature = "progression", feature = "interaction"))]
pub mod progression_interaction;
#[cfg(all(feature = "rules", feature = "agent-io"))]
pub mod rules_agent;
#[cfg(all(feature = "vehicles", feature = "agent-io"))]
pub mod vehicles_agent;

#[cfg(all(feature = "command", feature = "agent-io"))]
pub use command_agent::{
    COMMAND_SECTION, CommandAgentPlugin, EXECUTE_ORDERS, ORDER, command_affordances,
    command_section, order_handler,
};
#[cfg(all(feature = "command", feature = "interaction"))]
pub use command_interaction::{
    CommandInteractionPlugin, INTERACT_EXECUTOR, InteractOrder, InteractPhase, context_door_order,
    context_item_order, interact_executor,
};
#[cfg(all(feature = "command", feature = "vehicles"))]
pub use command_vehicles::{
    CREW_REDIRECT, CommandVehiclesPlugin, DRIVE_EXECUTOR, MOUNT_EXECUTOR, UNLOAD_EXECUTOR,
    context_vehicle_order, crew_redirect,
};
#[cfg(all(feature = "crafting", feature = "agent-io"))]
pub use crafting_agent::{CRAFT, CRAFTING_SECTION, CraftingAgentPlugin, crafting_affordances};
#[cfg(feature = "crafting")]
pub use crafting_interaction::{CRAFTING_BUSY, CraftingInteractionPlugin, crafting_busy};
#[cfg(all(feature = "progression", feature = "agent-io"))]
pub use progression_agent::{PROGRESSION_SECTION, ProgressionAgentPlugin, progression_section};
#[cfg(all(feature = "progression", feature = "crafting"))]
pub use progression_crafting::{CRAFT_PROGRESS_HOOK, ProgressionCraftingPlugin, craft_progress};
#[cfg(all(feature = "progression", feature = "interaction"))]
pub use progression_interaction::{
    ProgressionInteractionPlugin, STUDY, TEACH, Teacher, Teaches, study_verb, teach_verb,
};
#[cfg(all(feature = "vehicles", feature = "agent-io"))]
pub use vehicles_agent::{
    DRIVE_TO, DriveTo, UNLOAD, VEHICLE_SECTION, VehiclesAgentPlugin, vehicle_affordances,
};

#[cfg(all(feature = "interaction", feature = "agent-io"))]
pub use interaction_agent::{
    INTERACTION_AFFORDANCES, INVENTORY_SECTION, InteractionAgentPlugin, interaction_affordances,
    interaction_handler, inventory_section,
};
#[cfg(all(feature = "interaction", feature = "perception"))]
pub use interaction_perception::{InteractionPerceptionPlugin, perception_noise};
#[cfg(all(feature = "interaction", feature = "rules"))]
pub use interaction_rules::{InteractionRulesPlugin, rules_skill_check};
#[cfg(all(feature = "perception", feature = "agent-io"))]
pub use perception_agent::{PERCEPTION_SECTION, PerceptionAgentPlugin, perception_section};
#[cfg(all(feature = "perception", feature = "rules"))]
pub use perception_rules::{
    DETECT_PURPOSE, PER_ROLL, PerRollAppExt, PerRollConfig, PerRollModifiers,
    PerceptionRulesPlugin, Stealthy, TargetModifierFn, per_roll_check, per_roll_modifier,
};
#[cfg(all(feature = "rules", feature = "agent-io"))]
pub use rules_agent::{
    RulesAgentPlugin, SHEET_SECTION, SPEND_POINTS, sheet_section, spend_points_affordances,
    spend_points_handler,
};
