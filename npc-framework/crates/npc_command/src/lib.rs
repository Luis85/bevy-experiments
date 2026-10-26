//! `npc_command`: opt-in RTS click orders for units and groups
//! (actor-capabilities spec §4.4, plan wave F1).
//!
//! Add [`CommandPlugin`] next to `NpcPlugins` (or `NpcPlugins::new()
//! .with_command()`), put [`Commandable`] (speed, arrival tolerance) and an
//! [`Owner`] on units and give orders with the recorded [`IssueOrder`]
//! command. Without the plugin nothing here is registered, hashed or run.
//!
//! ## Orders
//!
//! [`UnitOrder`]: `Move`, `AttackMove` (a tagged move until combat
//! exists), `Stop`, `Hold`, `Follow { target }`, `Patrol { points }`, and
//! the data-only `Interact { verb, target }`, `Mount { vehicle }`,
//! `Unload`, which need an [`OrderExecutor`] registered for their kind
//! (capability glue: command × interaction, command × vehicles); without
//! one they are rejected when issued. A game adds its own order kinds as
//! `UnitOrder::Custom { name, args }` with an executor registered by name
//! ([`CommandAppExt::register_custom_order`]), and its own `Interact` verbs
//! with per-verb executors ([`CommandAppExt::register_interact_verb_executor`],
//! consulted before the glue's generic `Interact` executor).
//!
//! Every order is a `SimCommand` ([`IssueOrder`]) carrying the
//! [`PlayerId`] that gives it, validated against every unit's [`Owner`]
//! when submitted and again when it applies: deterministic, recorded in
//! replays, ready for lock-step multiplayer. [`submit_order`] is the one
//! call a UI, agent or script needs; [`enqueue_order`] / [`issue`] are the
//! unrecorded building blocks for replayed sim code (glue commands).
//!
//! ## Queue
//!
//! Each unit has an [`OrderQueue`]: the current [`ActiveOrder`] (with its
//! progress) and the shift-queue behind it. `queue: false` replaces
//! everything (the running order is journaled `OrderFailed { reason:
//! "replaced" }`), `queue: true` appends. Journal: `OrderIssued`,
//! `OrderStarted`, `OrderDone`, `OrderFailed { reason }`.
//!
//! ## Execution
//!
//! [`exec::command_engine_system`] in [`NpcCommandSet::Execute`] (after
//! `NpcSet::Sense`, before `NpcSet::Think`), units in `SimId` order. Moves
//! go through the locomotion seam (`MoveIntent` / `MoveStatus`), so units
//! with a `NavAgent` path-find and the rest walk straight.
//!
//! - Units without a `Brain` execute orders directly.
//! - Units with a `Brain` execute them through their tree: guard a branch
//!   with the condition [`HAS_UNIT_ORDER`] and run the leaf
//!   [`EXECUTE_ORDER_ACTION`] ([`leaf`]). An aborted leaf pauses the order.
//!
//! Executors registered by name per order kind
//! ([`CommandAppExt::register_order_executor`]) run before the built-ins;
//! redirects ([`CommandAppExt::register_order_redirect`]) send an order
//! given to one unit to another (a `ByCrew` vehicle to its driver). The
//! executor lifecycle (`validate` per unit, `cancel` after `Done`/`Failed`
//! too) is on [`OrderExecutor`]; [`approach_step`] / [`approach_then`]
//! ([`approach`]) are the shared walk-into-reach step for executors.
//!
//! ## Groups and squads
//!
//! A `Move` / `AttackMove` to two or more units gets integer formation
//! slots ([`formation`]: `Line`, `Box`, `Wedge`, spacing in mm) around the
//! target, facing from the group's centroid, re-validated against the
//! `SpatialGrid` (a blocked slot moves to the nearest free cell). One unit
//! leads: followers with a `NavAgent` reuse the leader's path shifted by
//! their slot offset instead of searching their own. A [`Squad`] entity
//! (members as a sorted `SimId` list, [`MemberOf`] on the members) is
//! ordered like a unit: [`IssueOrder`] expands it to its members.
//!
//! ## Relation to the kernel's `Order`
//!
//! The kernel's `SimCommand::Order` / `CurrentOrder` (`GoTo`, `Clear`) and
//! the village's `has_order` / `FollowOrder` branch stay exactly as they
//! are: `npc_command` is a **parallel, richer system** that never reads or
//! writes `CurrentOrder`, with its own condition and leaf names. Wrapping
//! the kernel order would have changed the village's state and content
//! hashes (and so its oracles); keeping them apart costs one `From<Order>
//! for UnitOrder` conversion. A game picks one per unit type.
//!
//! ## Determinism and hashing
//!
//! Integer-only; units, squads and slots in `SimId` order. The plugin
//! registers its components, `command_config`, plus the locomotion and
//! spatial state it depends on for the state hash
//! ([`register_command_hash`]), and its commands, leaf, condition,
//! executors and redirects in the content hash. That changes the content
//! hash of any app that adds it, which is why it is opt-in.
//!
//! ## Debugging
//!
//! Journal kinds `OrderIssued`, `OrderStarted`, `OrderDone`, `OrderFailed {
//! reason }` (`npcctl journal --kind OrderFailed`); inspect section
//! [`command_json`]; perf mechanic `command` (the engine system). The view
//! side (selection, control groups, context orders, order gizmos) is
//! `npc_view_helpers::command_ui` (feature `command`).

#![deny(
    clippy::float_arithmetic,
    clippy::float_cmp,
    clippy::iter_over_hash_type,
    clippy::disallowed_types,
    clippy::disallowed_methods
)]

pub mod approach;
pub mod command;
pub mod exec;
pub mod formation;
pub mod hash;
pub mod inspect;
pub mod leaf;
pub mod types;

use bevy_app::prelude::*;
use bevy_ecs::prelude::*;
use npc_kernel::registry::{KernelAppExt, is_frozen};
use npc_kernel::{NpcSet, NpcSimTick};

pub use approach::{ApproachSpec, ApproachStep, Reach, approach_step, approach_then};
pub use command::{
    CreateSquad, DisbandSquad, IssueOrder, IssueReceipt, SkippedUnit, check_owner, create_squad,
    expand_units, issue, issue_to, owner_of, submit_order, submit_order_report,
};
pub use exec::{
    BLOCKED, BLOCKED_REPORTS_PER_TICK, BlockedReports, OrderExecutor, OrderExecutors,
    OrderRedirects, OrderStep, RedirectFn, cancel_orders, command_engine_system, enqueue_order,
    pause_orders, resolve_unit, step_unit,
};
pub use formation::{Formation, FormationKind, GroupPlan, plan_group};
pub use hash::register_command_hash;
pub use inspect::{command_json, command_json_of};
pub use leaf::{EXECUTE_ORDER_ACTION, HAS_UNIT_ORDER, cond_has_unit_order};
pub use types::{
    ActiveOrder, Commandable, ExecutingOrder, GroupLink, MemberOf, OrderProgress, OrderQueue,
    Owner, PlayerId, QueuedOrder, Squad, UnitOrder,
};

/// Tuning of the command capability. Insert it before adding the plugin
/// to change it (the plugin uses `init_resource`); it is sim state, hashed
/// as `command_config` (so a change changes the state hash).
///
/// ```ignore
/// app.insert_resource(CommandConfig { block_fail_ticks: 50, ..default() })
///     .add_plugins(NpcPlugins::new().with_command());
/// ```
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommandConfig {
    /// Formation of group moves (`Move` / `AttackMove` to two or more
    /// units) whose `IssueOrder` names none and that are not a squad's
    /// order (default `Box`, 1500 mm spacing).
    pub default_formation: Formation,
    /// `Follow` keeps within this distance of its target, mm (default 1500).
    pub follow_distance_mm: i32,
    /// `Follow` re-targets when the target moved this far from the goal,
    /// mm (default 1000).
    pub follow_retarget_mm: i32,
    /// A walk fails (`OrderFailed { reason: "blocked" }`) once the mover
    /// reported `MoveStatus::Blocked` for this many ticks in a row: `Move`,
    /// `AttackMove`, `Patrol`, the glue's `Interact` / `Mount` approach and
    /// every [`approach_step`] (default 100).
    pub block_fail_ticks: u32,
    /// Most orders a unit holds (current + queued); a shift-queued order
    /// beyond it is rejected (or skipped with `skip_invalid`) (default 16).
    pub max_queue: u32,
    /// Most units one order may address after expanding squads (default
    /// 1024).
    pub max_group: u32,
    /// Most points of a `Patrol` order (at least 1; default 32).
    pub max_patrol_points: u32,
    /// Rings of grid cells searched for a free cell when a formation slot
    /// (or a follower's shifted waypoint) is blocked (default 8).
    pub relocate_radius_cells: u32,
    /// How long a group follower with a `NavAgent` waits for its leader's
    /// path before searching its own, ticks (default 40).
    pub leader_wait_ticks: u32,
}

impl Default for CommandConfig {
    fn default() -> Self {
        Self {
            default_formation: Formation::default(),
            follow_distance_mm: 1500,
            follow_retarget_mm: 1000,
            block_fail_ticks: 100,
            max_queue: 16,
            max_group: 1024,
            max_patrol_points: 32,
            relocate_radius_cells: 8,
            leader_wait_ticks: 40,
        }
    }
}

/// The command systems' set in `NpcSimTick`.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NpcCommandSet {
    /// After `NpcSet::Sense`, before `NpcSet::Think`: the command engine.
    Execute,
}

fn assert_open(app: &App, what: &str) {
    assert!(
        !is_frozen(app.world()),
        "cannot register {what}: content is already frozen (register in Plugin::build)"
    );
}

/// Registration on `App`. Any order, before or after the plugin, before
/// the freeze.
pub trait CommandAppExt {
    /// Registers executor `name` for order kind `kind` (one of
    /// [`UnitOrder::KINDS`]; panics on an unknown kind or a second executor
    /// for the same kind). `kind=name` joins the content hash.
    fn register_order_executor(
        &mut self,
        kind: &'static str,
        name: &'static str,
        exec: OrderExecutor,
    ) -> &mut Self;
    /// Registers (or replaces) redirect `name` (joins the content hash).
    fn register_order_redirect(&mut self, name: &'static str, f: RedirectFn) -> &mut Self;
    /// Registers the executor of the custom order kind `name`
    /// (`UnitOrder::Custom { name, args }`). Panics on an empty name, a
    /// built-in kind name ([`UnitOrder::KINDS`], `"Custom"`) or a second
    /// executor for `name`. `name` joins the content hash (`order_custom`).
    fn register_custom_order(&mut self, name: &'static str, exec: OrderExecutor) -> &mut Self;
    /// Registers executor `name` for `Interact { verb, .. }` orders with
    /// this `verb`: consulted before the `"Interact"` kind's executor (the
    /// command × interaction glue's), so a game verb (`"repair"`) and the
    /// interaction verbs coexist. Panics on a second executor for `verb`.
    /// `verb=name` joins the content hash (`order_verb`).
    fn register_interact_verb_executor(
        &mut self,
        verb: &'static str,
        name: &'static str,
        exec: OrderExecutor,
    ) -> &mut Self;
}

impl CommandAppExt for App {
    fn register_order_executor(
        &mut self,
        kind: &'static str,
        name: &'static str,
        exec: OrderExecutor,
    ) -> &mut Self {
        assert_open(self, &format!("order executor `{name}`"));
        assert!(
            UnitOrder::KINDS.contains(&kind),
            "unknown order kind `{kind}` (known: {})",
            UnitOrder::KINDS.join(", ")
        );
        let prev = self
            .world_mut()
            .get_resource_or_init::<OrderExecutors>()
            .by_kind
            .insert(kind, (name, exec));
        assert!(
            prev.is_none(),
            "order kind `{kind}` already has an executor"
        );
        self
    }

    fn register_order_redirect(&mut self, name: &'static str, f: RedirectFn) -> &mut Self {
        assert_open(self, &format!("order redirect `{name}`"));
        self.world_mut()
            .get_resource_or_init::<OrderRedirects>()
            .add(name, f);
        self
    }

    fn register_custom_order(&mut self, name: &'static str, exec: OrderExecutor) -> &mut Self {
        assert_open(self, &format!("custom order `{name}`"));
        assert!(!name.is_empty(), "a custom order needs a name");
        assert!(
            name != UnitOrder::CUSTOM && !UnitOrder::KINDS.contains(&name),
            "custom order `{name}` would shadow a built-in order kind"
        );
        let prev = self
            .world_mut()
            .get_resource_or_init::<OrderExecutors>()
            .custom
            .insert(name, exec);
        assert!(
            prev.is_none(),
            "custom order `{name}` already has an executor"
        );
        self
    }

    fn register_interact_verb_executor(
        &mut self,
        verb: &'static str,
        name: &'static str,
        exec: OrderExecutor,
    ) -> &mut Self {
        assert_open(self, &format!("Interact verb executor `{name}`"));
        let prev = self
            .world_mut()
            .get_resource_or_init::<OrderExecutors>()
            .verbs
            .insert(verb, (name, exec));
        assert!(
            prev.is_none(),
            "Interact verb `{verb}` already has an executor"
        );
        self
    }
}

fn freeze_command(world: &mut World) -> Vec<(String, String)> {
    let execs = world.get_resource_or_init::<OrderExecutors>();
    let mut out: Vec<(String, String)> = execs
        .by_kind
        .iter()
        .map(|(kind, (name, _))| ("order_executor".to_owned(), format!("{kind}={name}")))
        .collect();
    // Only present when registered: apps without them hash as before.
    out.extend(
        execs
            .custom
            .keys()
            .map(|name| ("order_custom".to_owned(), (*name).to_owned())),
    );
    out.extend(
        execs
            .verbs
            .iter()
            .map(|(verb, (name, _))| ("order_verb".to_owned(), format!("{verb}={name}"))),
    );
    out.extend(
        world
            .get_resource_or_init::<OrderRedirects>()
            .redirects
            .iter()
            .map(|(name, _)| ("order_redirect".to_owned(), (*name).to_owned())),
    );
    out
}

/// Kind namer, registered under a name that sorts after content namers.
fn kind_name(world: &World, entity: Entity) -> Option<&'static str> {
    world.get::<Squad>(entity).map(|_| "squad")
}

/// The command capability (see the crate docs).
pub struct CommandPlugin;

impl Plugin for CommandPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CommandConfig>()
            .init_resource::<BlockedReports>()
            .init_resource::<OrderExecutors>()
            .init_resource::<OrderRedirects>();
        register_command_hash(app);
        app.register_action(
            EXECUTE_ORDER_ACTION,
            leaf::execute_order_start,
            leaf::execute_order_stop,
        )
        .register_cond(HAS_UNIT_ORDER, cond_has_unit_order)
        .register_command::<IssueOrder>()
        .register_command::<CreateSquad>()
        .register_command::<DisbandSquad>()
        .register_kind_name("~npc_command", kind_name)
        .add_freeze_hook("npc_command", freeze_command)
        .configure_sets(
            NpcSimTick,
            NpcCommandSet::Execute
                .after(NpcSet::Sense)
                .before(NpcSet::Think),
        )
        .add_systems(
            NpcSimTick,
            command_engine_system.in_set(NpcCommandSet::Execute),
        );
    }
}
