//! `npc_framework`: the one crate a game depends on.
//!
//! ```ignore
//! use bevy::prelude::*;
//! use npc_framework::prelude::*;
//!
//! App::new()
//!     .add_plugins(DefaultPlugins)
//!     // 20 sim ticks per second, driven from the host's FixedUpdate (whose
//!     // own timestep is left alone).
//!     .add_plugins(NpcPlugins::new().seed(1).tick_hz(20).run_in(FixedUpdate))
//!     .add_plugins(MyUnitsPlugin) // registers needs, actions, trees, commands
//!     .run();
//! ```
//!
//! - [`NpcPlugins`]: the kernel, needs, jobs and the host-safe driver as one
//!   `PluginGroup` (`.set()`/`.disable()` work as usual).
//! - [`NpcAppExt`]: register content by **stable name**; ids are frozen
//!   sorted by name in the kernel's `Plugin::finish`, so registration and
//!   plugin order never change ids, hashes or replays.
//! - [`prelude`]: the types content and hosts use day to day.
//! - Features: `tools` (headless CLI helpers and perf glue), `remote` (BRP
//!   methods), `perf` (profiler), `locomotion` (default: the `MoveIntent`
//!   seam and the straight-line mover in `NpcPlugins`), `nav` (opt-in grid
//!   pathfinding: add `nav::NavPlugin` and put `nav::NavAgent` on units),
//!   `perception` (opt-in sensors: add `perception::PerceptionPlugin` and put
//!   `perception::Sensors` / `perception::Perceivable` on entities),
//!   `rules` (opt-in character sheet and 3d6 success rolls: add
//!   `rules::RulesPlugin` and put `rules::CharacterSheet` on units),
//!   `agent-io` (opt-in external agent control: add
//!   `agent_io::AgentIoPlugin`; the session host and clients), `interaction`
//!   (opt-in items, inventory, equipment, doors, tools and verbs: add
//!   `interaction::InteractionPlugin`), `vehicles` (opt-in seats, mounting,
//!   ByCrew/Direct vehicles: `NpcPlugins::new().with_vehicles()`), `command`
//!   (opt-in RTS orders, queues,
//!   formations and squads: `NpcPlugins::new().with_command()` or add
//!   `command::CommandPlugin`), `view-helpers` (generic presentation).
//!
//! - Features `progression` (opt-in XP, levels, spend policies and
//!   learning: `NpcPlugins::new().with_progression()`, needs `rules`) and
//!   `crafting` (opt-in recipes, workstations and order queues:
//!   `NpcPlugins::new().with_crafting()`, needs `interaction` + `rules`).
//!
//! - Capability glue ([`glue`], ADR 0011): what needs two capabilities
//!   (the `per_roll` detection check, the `perception` and `sheet`
//!   observation sections, `spend_points`, `Interact`/`Mount`/`Unload`
//!   orders, `ByCrew` forwarding, craft XP, `study`/`teach`, the agent
//!   actions of every capability) lives in facade modules compiled only when
//!   both features are on; `NpcPlugins::new().with_perception()
//!   .with_rules().with_agent_io()` adds the capabilities and their glue
//!   (every `with_*` adds the glue of each pair that is on).
//!
//! The sub-crates are re-exported as modules ([`kernel`], [`ai`], [`needs`],
//! [`jobs`], and with features [`locomotion`], [`nav`], [`perception`], [`perf`], [`rules`],
//! [`tools`], [`view_helpers`]).

use bevy_app::{PluginGroup, PluginGroupBuilder};
use bevy_ecs::intern::Interned;
use bevy_ecs::schedule::ScheduleLabel;

pub mod glue;

#[cfg(feature = "agent-io")]
pub use npc_agent_io as agent_io;
pub use npc_ai as ai;
#[cfg(feature = "command")]
pub use npc_command as command;
#[cfg(feature = "crafting")]
pub use npc_crafting as crafting;
#[cfg(feature = "interaction")]
pub use npc_interaction as interaction;
pub use npc_jobs as jobs;
pub use npc_kernel as kernel;
pub use npc_kernel::snapshot;
#[cfg(feature = "locomotion")]
pub use npc_locomotion as locomotion;
#[cfg(feature = "nav")]
pub use npc_nav as nav;
pub use npc_needs as needs;
#[cfg(feature = "perception")]
pub use npc_perception as perception;
/// The profiler (`npc_perf`), plus with `view-helpers` its windowed-app
/// wiring in two calls: `DefaultPlugins.set(perf::log_plugin())` (Bevy's
/// `LogPlugin` with the profiler layer) and `perf::NpcPerfPlugin` (tags,
/// budgets scaled for the build profile, `PerfHandle`, `end_frame` in
/// `Last`).
#[cfg(feature = "perf")]
pub mod perf {
    pub use npc_perf::*;
    #[cfg(feature = "view-helpers")]
    pub use npc_view_helpers::perf_setup::{
        NpcPerfPlugin, install_log_plugin_now, log_plugin, perf_layer, perf_log_filter,
    };

    /// Content registers p95 budgets for its own mechanics by name, in any
    /// plugin order, without touching `npc_perf`'s core table:
    /// `app.register_budget("actions.eat", 10, 2)` (µs at one agent, plus
    /// µs per agent). The rows live in the app's [`PerfBudgetTable`]
    /// (never hashed, never read by the sim); `npcctl bench`
    /// ([`PerfBudgets::default_npc_for_world`]) and `NpcPerfPlugin` add
    /// them to the core budgets.
    pub trait PerfAppExt {
        fn register_budget(
            &mut self,
            mechanic: impl Into<String>,
            fixed_us: u64,
            per_agent_us: u64,
        ) -> &mut Self;
    }

    impl PerfAppExt for bevy_app::App {
        fn register_budget(
            &mut self,
            mechanic: impl Into<String>,
            fixed_us: u64,
            per_agent_us: u64,
        ) -> &mut Self {
            self.world_mut()
                .get_resource_or_init::<PerfBudgetTable>()
                .add(mechanic, fixed_us, per_agent_us);
            self
        }
    }
}
#[cfg(feature = "progression")]
pub use npc_progression as progression;
#[cfg(feature = "rules")]
pub use npc_rules as rules;
#[cfg(feature = "tools")]
pub use npc_tools as tools;
#[cfg(feature = "vehicles")]
pub use npc_vehicles as vehicles;
#[cfg(feature = "view-helpers")]
pub use npc_view_helpers as view_helpers;

/// Re-exported so content derives (`#[derive(Component)]`) and hosts name
/// the same engine crates the framework uses.
pub use bevy_app;
pub use bevy_ecs;
pub use bevy_reflect;

use npc_kernel::driver::Pacing;
pub use npc_kernel::registry::KernelAppExt;
use npc_kernel::{DeterminismMode, NpcDriverPlugin, NpcKernelPlugin};
pub use npc_needs::NeedsAppExt;
use npc_needs::NpcNeedsPlugin;

/// Content registration by stable name (plan Task P3 interface):
/// `register_action`, `register_cond`, `register_scorer`, `register_tree`,
/// `register_sim_component::<T>()`, `register_command::<C>()` (from
/// [`KernelAppExt`]) and `register_need` (from [`NeedsAppExt`]). Every method
/// may be called in any order, before or after [`NpcPlugins`]; ids are frozen
/// sorted by name in the kernel's `Plugin::finish`.
///
/// It is the union of the per-crate extension traits (so the generic crates
/// can register their own pieces without depending on the facade); the
/// prelude brings all three into scope.
pub trait NpcAppExt: KernelAppExt + NeedsAppExt {}

impl<T: KernelAppExt + NeedsAppExt> NpcAppExt for T {}

/// The framework as one plugin group: [`NpcKernelPlugin`], [`NpcNeedsPlugin`],
/// [`NpcJobsPlugin`](npc_jobs::NpcJobsPlugin), the kernel's
/// [`SpatialPlugin`](npc_kernel::spatial::SpatialPlugin) (unhashed, a no-op
/// without blockers), with feature `locomotion` the
/// [`StraightLineMover`](npc_locomotion::StraightLineMover) (a host with its
/// own mover drops it with
/// `NpcPlugins::new().build().disable::<StraightLineMover>()`: `disable` is
/// a `PluginGroupBuilder` method), and (unless
/// [`NpcPlugins::without_driver`]) [`NpcDriverPlugin`]. Pathfinding is not
/// in the group: add `nav::NavPlugin` (feature `nav`) to opt in.
///
/// The defaults: seed 0, 20 ticks per second, driven from `FixedUpdate`,
/// [`DeterminismMode::Strict`].
///
/// **Capabilities (all opt-in).** `with_perception()`, `with_rules()` and
/// `with_agent_io()` add `PerceptionPlugin`, `RulesPlugin` and
/// `AgentIoPlugin` to the group, plus the [`glue`] plugin of every pair that
/// is on: perception + rules → [`PerceptionRulesPlugin`](glue::PerceptionRulesPlugin)
/// (registers the `"per_roll"` detection check; active only with
/// `with_per_roll_detection()`), perception + agent-io →
/// [`PerceptionAgentPlugin`](glue::PerceptionAgentPlugin), rules + agent-io →
/// [`RulesAgentPlugin`](glue::RulesAgentPlugin). Do not also add those
/// plugins yourself (Bevy rejects duplicates). Equivalent tuple, e.g.
/// `(NpcPlugins::new(), PerceptionPlugin, RulesPlugin, AgentIoPlugin::default(),
/// glue::PerceptionRulesPlugin::default(), glue::PerceptionAgentPlugin,
/// glue::RulesAgentPlugin)`.
///
/// The wave F capabilities (`with_command`, `with_vehicles`,
/// `with_progression`, `with_crafting`) add their glue the same way, one
/// plugin per pair that is on: command + interaction →
/// [`CommandInteractionPlugin`](glue::CommandInteractionPlugin), command +
/// vehicles → [`CommandVehiclesPlugin`](glue::CommandVehiclesPlugin),
/// crafting → [`CraftingInteractionPlugin`](glue::CraftingInteractionPlugin),
/// progression + crafting →
/// [`ProgressionCraftingPlugin`](glue::ProgressionCraftingPlugin),
/// progression + interaction →
/// [`ProgressionInteractionPlugin`](glue::ProgressionInteractionPlugin), and
/// with agent-io [`CommandAgentPlugin`](glue::CommandAgentPlugin),
/// [`VehiclesAgentPlugin`](glue::VehiclesAgentPlugin),
/// [`ProgressionAgentPlugin`](glue::ProgressionAgentPlugin),
/// [`CraftingAgentPlugin`](glue::CraftingAgentPlugin).
#[derive(Clone)]
pub struct NpcPlugins {
    seed: u64,
    tick_hz: u32,
    schedule: Option<Interned<dyn ScheduleLabel>>,
    determinism: DeterminismMode,
    pacing: Pacing,
    capabilities: Capabilities,
}

/// Which opt-in capabilities [`NpcPlugins`] adds.
#[derive(Clone, Copy, Debug, Default)]
// Which fields are read depends on the enabled features.
#[allow(dead_code)]
struct Capabilities {
    perception: bool,
    rules: bool,
    agent_io: bool,
    per_roll: bool,
    interaction: bool,
    progression: bool,
    crafting: bool,
    vehicles: bool,
    command: bool,
}

impl Default for NpcPlugins {
    fn default() -> Self {
        Self::new()
    }
}

impl NpcPlugins {
    pub fn new() -> Self {
        Self {
            seed: 0,
            tick_hz: npc_kernel::clock::DEFAULT_TICK_HZ,
            schedule: Some(bevy_app::FixedUpdate.intern()),
            determinism: DeterminismMode::Strict,
            pacing: Pacing::HostTime,
            capabilities: Capabilities::default(),
        }
    }

    /// Adds `PerceptionPlugin` (feature `perception`).
    #[cfg(feature = "perception")]
    pub fn with_perception(mut self) -> Self {
        self.capabilities.perception = true;
        self
    }

    /// Adds `RulesPlugin` (feature `rules`).
    #[cfg(feature = "rules")]
    pub fn with_rules(mut self) -> Self {
        self.capabilities.rules = true;
        self
    }

    /// Adds `AgentIoPlugin::default()` (feature `agent-io`).
    #[cfg(feature = "agent-io")]
    pub fn with_agent_io(mut self) -> Self {
        self.capabilities.agent_io = true;
        self
    }

    /// Adds `InteractionPlugin::default()` (feature `interaction`), plus its
    /// glue with the other capabilities that are on (`glue::InteractionRulesPlugin`,
    /// `glue::InteractionPerceptionPlugin`, `glue::InteractionAgentPlugin`).
    #[cfg(feature = "interaction")]
    pub fn with_interaction(mut self) -> Self {
        self.capabilities.interaction = true;
        self
    }

    /// Adds `CommandPlugin` (feature `command`): RTS orders (`IssueOrder`),
    /// order queues, formations, squads and the `ExecuteOrder` leaf; plus
    /// its glue with the other capabilities that are on:
    /// `glue::CommandInteractionPlugin` (`Interact` orders),
    /// `glue::CommandVehiclesPlugin` (`ByCrew` forwarding, driving, `Mount`,
    /// `Unload`), `glue::CommandAgentPlugin` (the `order` action).
    #[cfg(feature = "command")]
    pub fn with_command(mut self) -> Self {
        self.capabilities.command = true;
        self
    }

    /// Adds `VehiclesPlugin` (feature `vehicles`) and the interaction
    /// capability it builds on (its `mount`/`dismount` are interaction
    /// verbs), as [`NpcPlugins::with_interaction`] does; with command also
    /// `glue::CommandVehiclesPlugin`, with agent-io
    /// `glue::VehiclesAgentPlugin`.
    #[cfg(feature = "vehicles")]
    pub fn with_vehicles(mut self) -> Self {
        self.capabilities.interaction = true;
        self.capabilities.vehicles = true;
        self
    }

    /// Adds `ProgressionPlugin::default()` (feature `progression`) and the
    /// rules capability it builds on, as [`NpcPlugins::with_rules`] does;
    /// with crafting also `glue::ProgressionCraftingPlugin` (craft XP and
    /// practice), with interaction `glue::ProgressionInteractionPlugin`
    /// (`study` / `teach`), with agent-io `glue::ProgressionAgentPlugin`.
    #[cfg(feature = "progression")]
    pub fn with_progression(mut self) -> Self {
        self.capabilities.rules = true;
        self.capabilities.progression = true;
        self
    }

    /// Adds `CraftingPlugin` (feature `crafting`) and the interaction and
    /// rules capabilities it builds on (so their glue too), plus
    /// `glue::CraftingInteractionPlugin` (no interaction while crafting),
    /// with progression `glue::ProgressionCraftingPlugin`, with agent-io
    /// `glue::CraftingAgentPlugin`.
    #[cfg(feature = "crafting")]
    pub fn with_crafting(mut self) -> Self {
        self.capabilities.interaction = true;
        self.capabilities.rules = true;
        self.capabilities.crafting = true;
        self
    }

    /// Perception + rules with the `"per_roll"` detection check active
    /// (features `perception` + `rules`; see [`glue::perception_rules`]).
    #[cfg(all(feature = "perception", feature = "rules"))]
    pub fn with_per_roll_detection(mut self) -> Self {
        self.capabilities.perception = true;
        self.capabilities.rules = true;
        self.capabilities.per_roll = true;
        self
    }

    /// World seed for the counter RNG.
    pub fn seed(mut self, seed: u64) -> Self {
        self.seed = seed;
        self
    }

    /// Sim ticks per second (the host's own timesteps are not touched).
    pub fn tick_hz(mut self, hz: u32) -> Self {
        assert!(hz > 0, "tick_hz must be > 0");
        self.tick_hz = hz;
        self
    }

    /// The host schedule the driver runs in (`FixedUpdate`, `Update`, ...).
    pub fn run_in(mut self, schedule: impl ScheduleLabel) -> Self {
        self.schedule = Some(schedule.intern());
        self
    }

    /// No driver: the host runs ticks itself (`npc_kernel::run_ticks`), as
    /// headless tools, tests and replays do.
    pub fn without_driver(mut self) -> Self {
        self.schedule = None;
        self
    }

    /// [`Pacing::EveryRun`]: `SimControl.multiplier` ticks per schedule run
    /// instead of real time (headless servers, frame-stepped tests).
    pub fn pacing(mut self, pacing: Pacing) -> Self {
        self.pacing = pacing;
        self
    }

    pub fn determinism(mut self, mode: DeterminismMode) -> Self {
        self.determinism = mode;
        self
    }
}

impl PluginGroup for NpcPlugins {
    fn build(self) -> PluginGroupBuilder {
        let group = PluginGroupBuilder::start::<Self>()
            .add(NpcKernelPlugin {
                seed: self.seed,
                determinism: self.determinism,
            })
            .add(NpcNeedsPlugin)
            .add(npc_jobs::NpcJobsPlugin)
            .add(npc_kernel::spatial::SpatialPlugin);
        #[cfg(feature = "locomotion")]
        let group = group.add(npc_locomotion::StraightLineMover);
        let group = self.capabilities.add_to(group);
        match self.schedule {
            Some(schedule) => group.add(NpcDriverPlugin {
                tick_hz: self.tick_hz,
                schedule,
                max_ticks_per_run: 100,
                pacing: self.pacing,
            }),
            None => group,
        }
    }
}

impl Capabilities {
    /// The capability plugins, then the glue of every pair that is on.
    fn add_to(self, group: PluginGroupBuilder) -> PluginGroupBuilder {
        #[cfg(feature = "perception")]
        let group = if self.perception {
            group.add(npc_perception::PerceptionPlugin)
        } else {
            group
        };
        #[cfg(feature = "rules")]
        let group = if self.rules {
            group.add(npc_rules::RulesPlugin)
        } else {
            group
        };
        #[cfg(feature = "agent-io")]
        let group = if self.agent_io {
            group.add(npc_agent_io::AgentIoPlugin::default())
        } else {
            group
        };
        #[cfg(all(feature = "perception", feature = "rules"))]
        let group = if self.perception && self.rules {
            group.add(glue::PerceptionRulesPlugin {
                activate: self.per_roll,
            })
        } else {
            group
        };
        #[cfg(all(feature = "perception", feature = "agent-io"))]
        let group = if self.perception && self.agent_io {
            group.add(glue::PerceptionAgentPlugin)
        } else {
            group
        };
        #[cfg(all(feature = "rules", feature = "agent-io"))]
        let group = if self.rules && self.agent_io {
            group.add(glue::RulesAgentPlugin)
        } else {
            group
        };
        #[cfg(feature = "interaction")]
        let group = if self.interaction {
            group.add(npc_interaction::InteractionPlugin::default())
        } else {
            group
        };
        #[cfg(feature = "vehicles")]
        let group = if self.vehicles {
            group.add(npc_vehicles::VehiclesPlugin)
        } else {
            group
        };
        #[cfg(all(feature = "interaction", feature = "rules"))]
        let group = if self.interaction && self.rules {
            group.add(glue::InteractionRulesPlugin)
        } else {
            group
        };
        #[cfg(all(feature = "interaction", feature = "perception"))]
        let group = if self.interaction && self.perception {
            group.add(glue::InteractionPerceptionPlugin)
        } else {
            group
        };
        #[cfg(all(feature = "interaction", feature = "agent-io"))]
        let group = if self.interaction && self.agent_io {
            group.add(glue::InteractionAgentPlugin)
        } else {
            group
        };
        #[cfg(feature = "command")]
        let group = if self.command {
            group.add(npc_command::CommandPlugin)
        } else {
            group
        };
        #[cfg(feature = "progression")]
        let group = if self.progression {
            group.add(npc_progression::ProgressionPlugin::default())
        } else {
            group
        };
        #[cfg(feature = "crafting")]
        let group = if self.crafting {
            group.add(npc_crafting::CraftingPlugin)
        } else {
            group
        };
        self.add_wave_f_glue(group)
    }

    /// The glue of the wave F capabilities (command, vehicles, progression,
    /// crafting) with each other and with interaction / agent-io.
    fn add_wave_f_glue(self, group: PluginGroupBuilder) -> PluginGroupBuilder {
        #[cfg(all(feature = "command", feature = "interaction"))]
        let group = if self.command && self.interaction {
            group.add(glue::CommandInteractionPlugin)
        } else {
            group
        };
        #[cfg(all(feature = "command", feature = "vehicles"))]
        let group = if self.command && self.vehicles {
            group.add(glue::CommandVehiclesPlugin)
        } else {
            group
        };
        #[cfg(feature = "crafting")]
        let group = if self.crafting {
            group.add(glue::CraftingInteractionPlugin)
        } else {
            group
        };
        #[cfg(all(feature = "progression", feature = "crafting"))]
        let group = if self.progression && self.crafting {
            group.add(glue::ProgressionCraftingPlugin)
        } else {
            group
        };
        #[cfg(all(feature = "progression", feature = "interaction"))]
        let group = if self.progression && self.interaction {
            group.add(glue::ProgressionInteractionPlugin)
        } else {
            group
        };
        #[cfg(all(feature = "command", feature = "agent-io"))]
        let group = if self.command && self.agent_io {
            group.add(glue::CommandAgentPlugin)
        } else {
            group
        };
        #[cfg(all(feature = "vehicles", feature = "agent-io"))]
        let group = if self.vehicles && self.agent_io {
            group.add(glue::VehiclesAgentPlugin)
        } else {
            group
        };
        #[cfg(all(feature = "progression", feature = "agent-io"))]
        let group = if self.progression && self.agent_io {
            group.add(glue::ProgressionAgentPlugin)
        } else {
            group
        };
        #[cfg(all(feature = "crafting", feature = "agent-io"))]
        let group = if self.crafting && self.agent_io {
            group.add(glue::CraftingAgentPlugin)
        } else {
            group
        };
        group
    }
}

/// What content and hosts use day to day.
pub mod prelude {
    #[cfg(all(feature = "perception", feature = "rules"))]
    pub use crate::glue::{PerRollAppExt, PerceptionRulesPlugin, Stealthy};
    #[cfg(feature = "perf")]
    pub use crate::perf::PerfAppExt;
    pub use crate::{KernelAppExt, NeedsAppExt, NpcAppExt, NpcPlugins};
    #[cfg(feature = "agent-io")]
    pub use npc_agent_io::{AgentIoAppExt, AgentIoPlugin};
    pub use npc_ai::{
        Abort, ActiveLeaf, AiRegistry, BbKey, BbValue, Blackboard, Brain, BrainState, ParamValue,
        Status, StopReason, ThinkRequest, TreeBuilder, TreeId,
    };
    #[cfg(feature = "command")]
    pub use npc_command::{
        ApproachSpec, ApproachStep, CommandAppExt, CommandConfig, CommandPlugin, Commandable,
        CreateSquad, DisbandSquad, EXECUTE_ORDER_ACTION, Formation, FormationKind, HAS_UNIT_ORDER,
        IssueOrder, IssueReceipt, MemberOf, OrderExecutor, OrderQueue, OrderStep, Owner, PlayerId,
        Reach, SkippedUnit, Squad, UnitOrder, approach_step, approach_then, create_squad,
        submit_order, submit_order_report,
    };
    #[cfg(feature = "crafting")]
    pub use npc_crafting::{
        Craft, CraftCompleted, CraftError, CraftResult, CraftingAppExt, CraftingConfig,
        CraftingOrders, CraftingPlugin, Durability, PostCraftOrder, RecipeDef, Workstation,
        can_craft, crafting_jobs, post_order,
    };
    #[cfg(feature = "interaction")]
    pub use npc_interaction::{
        CarriedBy, Door, Equipment, Interact, Interactable, InteractionAppExt, InteractionConfig,
        InteractionDef, InteractionPlugin, Interactor, Inventory, Item, ItemKindDef, ItemYield,
        SkillCheck, ToolTarget,
    };
    pub use npc_jobs::{Approach, Provides, Reservations, free_for, free_slots, try_reserve_slot};
    pub use npc_kernel::commands::CommandError;
    pub use npc_kernel::driver::{NpcDriverSet, Pacing, SimTime};
    pub use npc_kernel::hash::{Fnv64, SimHash, SimHashResource, state_hash};
    pub use npc_kernel::journal::{JournalKind, record_effect, set_journal};
    pub use npc_kernel::json::JsonObject;
    pub use npc_kernel::registry::content_hash;
    pub use npc_kernel::snapshot::SnapshotAppExt;
    pub use npc_kernel::spatial::{
        BlockerHit, CellCoord, CellFlags, CellInfo, Footprint, GridBlocker, GridBounds,
        GridChanged, GridOccluder, SpatialGrid, nearest_blocker_by_edge,
    };
    pub use npc_kernel::{
        ContentCommand, CurrentOrder, DeterminismMode, HostPosition, Journal, JournalPlugin,
        NpcSet, NpcSimTick, Order, OrderKind, PrevSimPos, SimClock, SimCommand, SimConfig,
        SimControl, SimId, SimIndex, SimPos, SimRate, WorldSeed, run_ticks, spawn_sim, submit,
    };
    #[cfg(feature = "locomotion")]
    pub use npc_locomotion::{
        ExternalMover, MoveIntent, MoveStatus, NpcMoverSet, clear_move_intent, set_move_intent,
    };
    #[cfg(feature = "nav")]
    pub use npc_nav::{NavAgent, NavAppExt, NavBudget, NavPlugin, Path};
    pub use npc_needs::{
        NEED_MAX, Need, NeedDef, NeedId, NeedLevel, NeedRegistry, NeedThresholds, Needs,
    };
    #[cfg(feature = "perception")]
    pub use npc_perception::{
        EmitNoise, Facing, FixedFacing, Noise, Perceivable, PerceptionAppExt, PerceptionConfig,
        PerceptionFacts, PerceptionMemory, PerceptionPlugin, ReachSense, SenseKind, Sensors,
        emit_noise,
    };
    #[cfg(feature = "progression")]
    pub use npc_progression::{
        Experience, LevelCurve, ProgressionAppExt, ProgressionConfig, ProgressionPlugin,
        SpendPolicy, XpSourceDef, award_xp, grant_practice, grant_study, grant_teaching,
    };
    #[cfg(feature = "rules")]
    pub use npc_rules::{
        CharacterSheet, Contestant, Critical, Difficulty, Purchase, RollOutcome, RollSubject,
        RuleSet, RulesAppExt, RulesPlugin, SheetTemplate, SkillDef, Stat, TraitDef,
        effective_skill, roll_skill, roll_stat, spend_points,
    };
    #[cfg(feature = "vehicles")]
    pub use npc_vehicles::{
        Dismount, DriveGoal, Mount, MountedIn, NpcVehicleSet, Occupants, Seat, SeatPreference,
        SeatRole, SeatSlot, Unload, Vehicle, VehicleConfig, VehicleControl, VehiclesPlugin,
        command_vehicle_to, driver_of, set_drive_goal,
    };
}
