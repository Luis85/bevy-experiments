//! `npc_interaction`: opt-in interaction (actor-capabilities spec §4.3,
//! plan wave E1): items, inventory and equipment, doors, tools and verbs
//! registered by name, all behind one `Interact` action and command.
//!
//! Add [`InteractionPlugin`] next to `NpcPlugins`; put [`Interactor`] (plus
//! [`Inventory`] / [`Equipment`]) on units that act, [`Item`] on items,
//! [`Door`] on doors, [`ToolTarget`] on things worked with tools and,
//! optionally, [`Interactable`] on targets to restrict or advertise verbs.
//! Without the plugin nothing here is registered, hashed or run.
//!
//! ## Verbs
//!
//! An [`InteractionDef`] (registered by stable name,
//! [`InteractionAppExt::register_interaction`]) has an `offer` filter,
//! preconditions, a duration in ticks, an optional skill check, effects, a
//! reservation of the target and an optional noise. Built-ins
//! ([`builtin`]): `pick_up`, `drop`, `give`, `open`, `close`, `lock`,
//! `unlock`, `equip`, `unequip`, `use_tool_on`; `mount`/`dismount` are
//! reserved for `npc_vehicles`.
//!
//! ## One path
//!
//! Every entry point calls [`run::begin`]: the `Interact` BT leaf
//! ([`leaf`]; also what a `Direct` agent's queue runs), the `Interact`
//! content command ([`command`]; orders, `Advisor` agents, scripts) and the
//! door-on-path behaviour ([`door::auto_door_system`]).
//! [`run::complete_interactions_system`] applies the effects when the
//! duration is over. So every interaction is journaled the same way
//! (`Interacted { verb, target, item, outcome, reason }`, `ItemMoved`,
//! `DoorChanged`, `Reserve`/`Release`) and replays bit for bit.
//!
//! ## Schedule
//!
//! - `NpcSet::Inputs` (after the spatial update): [`door::door_sync_system`].
//! - [`NpcInteractionSet::Begin`] (after `Think`, before `Actions`): doors on
//!   nav paths.
//! - [`NpcInteractionSet::Complete`] (after `Locomotion`, before `Effects`):
//!   completions in `SimId` order (perf mechanic `interaction`), then
//!   [`door::key_replan_system`] (a failed path retries once its unit holds
//!   the key of a locked door).
//!
//! ## Hooks
//!
//! [`InteractionHooks`]: a skill-check hook (without one, checks succeed)
//! and a noise hook (without one, interactions are silent), set by name with
//! [`InteractionAppExt`]. [`InteractionListeners`]: busy predicates (what
//! else keeps a unit from starting an interaction, e.g. crafting) and
//! finished hooks (called when an interaction ends, e.g. to finish an RTS
//! `Interact` order), added by name; none by default. This crate depends on no other capability crate
//! (ADR 0011): the glue lives in the facade (`npc_framework::glue`:
//! `interaction_rules` rolls checks with `npc_rules`, `interaction_perception`
//! turns noises into `npc_perception` stimuli, `interaction_agent` registers
//! affordances for every verb with `why_not`, their action handlers and an
//! `inventory` observation section for `npc_agent_io`).
//!
//! ## Doors and navigation
//!
//! Door cells carry a tag bit in the grid's static layer; a closed door
//! blocks and occludes. The plugin registers a nav pass-cost function for
//! the bit ([`door::door_pass_cost`]): closed doors the agent may open are
//! passable at extra cost with a path marker, locked ones without the key
//! block. A `NavAgent` + [`Interactor`] reaching a marked door opens it
//! (unlocking first with the key) and walks through.
//!
//! ## Determinism and hashing
//!
//! Integer-only; actors and doors in `SimId` order; carried items as sorted
//! `SimId` lists. When a holder leaves the sim its items drop where it
//! stood; when a carried item leaves, its holder's lists drop its id
//! ([`items::on_sim_id_leave`]). The plugin registers its components and
//! `interaction_config` for the state hash, plus the grid's static layer
//! ([`hash::register_interaction_hash`]), and its verbs, item kinds and
//! hooks in the content hash. That changes the content hash of any app that
//! adds it, which is why it is opt-in.

#![deny(
    clippy::float_arithmetic,
    clippy::float_cmp,
    clippy::iter_over_hash_type,
    clippy::disallowed_types,
    clippy::disallowed_methods
)]

pub mod builtin;
pub mod command;
pub mod defs;
pub mod door;
pub mod hash;
pub mod inspect;
pub mod items;
pub mod leaf;
pub mod run;
pub mod targets;

use bevy_app::prelude::*;
use bevy_ecs::message::Messages;
use bevy_ecs::prelude::*;
use npc_kernel::registry::{KernelAppExt, is_frozen};
use npc_kernel::snapshot::SnapshotAppExt;
use npc_kernel::spatial::{GridChanged, SpatialGrid, spatial_update_system};
use npc_kernel::{NpcSet, NpcSimTick};
use npc_nav::NavAppExt;

pub use command::Interact;
pub use defs::{
    BusyFn, Ctx, FinishedFn, InteractionDef, InteractionHooks, InteractionListeners,
    InteractionRegistry, ItemArg, NoiseFn, NoiseSpec, SkillCheck, SkillCheckFn,
};
pub use door::{Door, DoorCells};
pub use hash::register_interaction_hash;
pub use inspect::{interaction_json, interaction_json_of};
pub use items::{
    CarriedBy, Equipment, Interactor, Inventory, Item, ItemKindDef, MAIN_HAND, OFF_HAND,
};
pub use leaf::{INTERACT_ACTION, InteractLeaf};
pub use run::{Interacting, Source, begin, can_interact, cancel};
pub use targets::{Interactable, ItemYield, ToolTarget};

/// Tuning. Insert before adding the plugin to change it (the plugin uses
/// `init_resource`). Hashed as `interaction_config`.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct InteractionConfig {
    /// Default reach of verbs that need one, in mm.
    pub reach_mm: i32,
    /// Blackboard slot the `Interact` leaf reads its target from.
    pub target_slot: u8,
    /// Blackboard slot the `Interact` leaf reads its item from.
    pub item_slot: u8,
    /// Grid cell tag bit (0..8) marking door cells.
    pub door_tag_bit: u8,
    /// Extra nav cost of passing a closed door (doubled when locked).
    pub door_pass_cost: u16,
    /// Whether nav agents open doors on their path.
    pub auto_open_doors: bool,
    /// Agent affordances list targets within this radius, in mm.
    pub affordance_radius_mm: i32,
}

impl Default for InteractionConfig {
    fn default() -> Self {
        Self {
            reach_mm: 1500,
            target_slot: 7,
            item_slot: 6,
            door_tag_bit: 0,
            door_pass_cost: 20,
            auto_open_doors: true,
            affordance_radius_mm: 10_000,
        }
    }
}

/// The interaction systems' sets in `NpcSimTick`.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NpcInteractionSet {
    /// After `NpcSet::Think`, before `NpcSet::Actions`.
    Begin,
    /// After `NpcSet::Locomotion`, before `NpcSet::Effects`.
    Complete,
}

fn assert_open(app: &App, what: &str) {
    assert!(
        !is_frozen(app.world()),
        "cannot register {what}: content is already frozen (register in Plugin::build)"
    );
}

/// Registration on `App`. Any order, before or after the plugin, before
/// the freeze.
pub trait InteractionAppExt {
    /// Registers an item kind (panics on a duplicate name).
    fn register_item_kind(&mut self, def: ItemKindDef) -> &mut Self;
    /// Registers a verb (panics on a duplicate name).
    fn register_interaction(&mut self, def: InteractionDef) -> &mut Self;
    /// Replaces (or adds) a verb, e.g. a built-in with another duration.
    fn override_interaction(&mut self, def: InteractionDef) -> &mut Self;
    /// Sets the skill-check hook (`name` joins the content hash).
    fn set_skill_check_hook(&mut self, name: &'static str, f: SkillCheckFn) -> &mut Self;
    /// Sets the noise hook (`name` joins the content hash).
    fn set_noise_hook(&mut self, name: &'static str, f: NoiseFn) -> &mut Self;
    /// Adds (or replaces) busy predicate `name` (joins the content hash):
    /// [`run::busy_with`] (so `begin`, the `Interact` command and
    /// affordances) also refuses while it answers `Some`.
    fn add_busy_check(&mut self, name: &'static str, f: BusyFn) -> &mut Self;
    /// Adds (or replaces) finished hook `name` (joins the content hash),
    /// called whenever an interaction ends ([`FinishedFn`]).
    fn add_finished_hook(&mut self, name: &'static str, f: FinishedFn) -> &mut Self;
}

impl InteractionAppExt for App {
    fn register_item_kind(&mut self, def: ItemKindDef) -> &mut Self {
        assert_open(self, &format!("item kind `{}`", def.name));
        let mut r = self
            .world_mut()
            .get_resource_or_init::<InteractionRegistry>();
        let name = def.name.clone();
        assert!(
            r.kinds.insert(name.clone(), def).is_none(),
            "item kind `{name}` is registered twice"
        );
        self
    }

    fn register_interaction(&mut self, def: InteractionDef) -> &mut Self {
        assert_open(self, &format!("interaction `{}`", def.name));
        let mut r = self
            .world_mut()
            .get_resource_or_init::<InteractionRegistry>();
        assert!(
            r.verbs.insert(def.name, def).is_none(),
            "interaction `{}` is registered twice",
            def.name
        );
        self
    }

    fn override_interaction(&mut self, def: InteractionDef) -> &mut Self {
        assert_open(self, &format!("interaction `{}`", def.name));
        self.world_mut()
            .get_resource_or_init::<InteractionRegistry>()
            .verbs
            .insert(def.name, def);
        self
    }

    fn set_skill_check_hook(&mut self, name: &'static str, f: SkillCheckFn) -> &mut Self {
        assert_open(self, &format!("skill-check hook `{name}`"));
        self.world_mut()
            .get_resource_or_init::<InteractionHooks>()
            .skill_check = Some((name, f));
        self
    }

    fn set_noise_hook(&mut self, name: &'static str, f: NoiseFn) -> &mut Self {
        assert_open(self, &format!("noise hook `{name}`"));
        self.world_mut()
            .get_resource_or_init::<InteractionHooks>()
            .noise = Some((name, f));
        self
    }

    fn add_busy_check(&mut self, name: &'static str, f: BusyFn) -> &mut Self {
        assert_open(self, &format!("busy check `{name}`"));
        self.world_mut()
            .get_resource_or_init::<InteractionListeners>()
            .add_busy(name, f);
        self
    }

    fn add_finished_hook(&mut self, name: &'static str, f: FinishedFn) -> &mut Self {
        assert_open(self, &format!("finished hook `{name}`"));
        self.world_mut()
            .get_resource_or_init::<InteractionListeners>()
            .add_finished(name, f);
        self
    }
}

fn freeze_interaction(world: &mut World) -> Vec<(String, String)> {
    let mut out = world
        .get_resource_or_init::<InteractionRegistry>()
        .manifest();
    let hooks = *world.get_resource_or_init::<InteractionHooks>();
    if let Some((name, _)) = hooks.skill_check {
        out.push(("interaction_hook".into(), format!("skill_check={name}")));
    }
    if let Some((name, _)) = hooks.noise {
        out.push(("interaction_hook".into(), format!("noise={name}")));
    }
    let listeners = world.get_resource_or_init::<InteractionListeners>().clone();
    for (name, _) in &listeners.busy {
        out.push(("interaction_hook".into(), format!("busy={name}")));
    }
    for (name, _) in &listeners.finished {
        out.push(("interaction_hook".into(), format!("finished={name}")));
    }
    out
}

/// Kind namer, registered under a name that sorts after content namers (so
/// content naming its own items or doors wins).
fn kind_name(world: &World, entity: Entity) -> Option<&'static str> {
    if world.get::<Item>(entity).is_some() {
        Some("item")
    } else if world.get::<Door>(entity).is_some() {
        Some("door")
    } else {
        None
    }
}

/// The interaction capability (see the crate docs).
pub struct InteractionPlugin {
    /// Register the built-in verbs (default `true`).
    pub builtins: bool,
}

impl Default for InteractionPlugin {
    fn default() -> Self {
        Self { builtins: true }
    }
}

impl Plugin for InteractionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SpatialGrid>()
            .init_resource::<Messages<GridChanged>>()
            .init_resource::<InteractionConfig>()
            .init_resource::<InteractionRegistry>()
            .init_resource::<InteractionHooks>()
            .init_resource::<InteractionListeners>()
            .init_resource::<DoorCells>()
            .init_resource::<npc_jobs::Reservations>()
            .register_snapshot_resource_with(
                "door_cells",
                door::save_door_cells,
                door::load_door_cells,
            )
            .add_snapshot_rebuild("door_cells", door::rebuild_door_cells)
            .register_snapshot_resource_with(
                "key_replan_pending",
                door::save_key_replan_pending,
                door::load_key_replan_pending,
            )
            .add_snapshot_rebuild("key_replan_pending", door::rebuild_key_replan_pending);
        if self.builtins {
            for def in builtin::builtin_verbs() {
                app.register_interaction(def);
            }
        }
        register_interaction_hash(app);
        let bit = app.world().resource::<InteractionConfig>().door_tag_bit;
        app.register_pass_cost(bit, door::door_pass_cost)
            .register_cell_blocker(bit, door::door_blocker)
            .register_action(INTERACT_ACTION, leaf::interact_start, leaf::interact_stop)
            .register_command::<Interact>()
            .register_kind_name("~npc_interaction", kind_name)
            .on_sim_id_leave("npc_interaction.items", items::on_sim_id_leave)
            .add_freeze_hook("npc_interaction", freeze_interaction)
            .configure_sets(
                NpcSimTick,
                (
                    NpcInteractionSet::Begin
                        .after(NpcSet::Think)
                        .before(NpcSet::Actions),
                    NpcInteractionSet::Complete
                        .after(NpcSet::Locomotion)
                        .before(NpcSet::Effects),
                ),
            )
            .add_systems(
                NpcSimTick,
                (
                    door::door_sync_system
                        .in_set(NpcSet::Inputs)
                        .after(spatial_update_system),
                    door::auto_door_system.in_set(NpcInteractionSet::Begin),
                    (run::complete_interactions_system, door::key_replan_system)
                        .chain()
                        .in_set(NpcInteractionSet::Complete),
                ),
            );
    }
}
