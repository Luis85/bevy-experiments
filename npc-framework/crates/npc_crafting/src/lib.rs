//! `npc_crafting`: opt-in crafting on top of `npc_interaction` (items,
//! inventories) and `npc_rules` (skill rolls) (actor-capabilities spec
//! §4.8, plan wave F3; ADR 0012 lets it depend on those two and the
//! foundations, nothing else).
//!
//! Add [`CraftingPlugin`] next to `InteractionPlugin` and `RulesPlugin`;
//! register recipes ([`CraftingAppExt::register_recipe`]) and put
//! [`Workstation`] on the smart objects that host them. Without the plugin
//! nothing here is registered, hashed or run.
//!
//! ## Recipes
//!
//! A [`RecipeDef`] (by stable name, validated at freeze against the item
//! kinds and skills: [`validate_recipe`]) has item inputs (kind name or tag
//! and a count, taken from the crafter's **inventory**), tools (kind or tag,
//! **carried**: in the inventory or equipped, equipped preferred), the
//! workstation kinds that host it, a skill + difficulty modifier, a duration
//! in ticks, outputs and an XP value.
//!
//! ## Running
//!
//! One path ([`run`]): the `Craft` BT action ([`leaf`], recipe by name, or
//! the next order on the workstation), the `Craft` content command
//! ([`command`]) and order picking all call [`begin_craft`], which claims
//! the workstation (capacity 1, so two crafters contend for it) and the
//! input and tool items in `Reservations`. When the duration is over the
//! skill is rolled (`npc_rules::roll_skill`, purpose `craft.<recipe>`,
//! modifier + the tools' quality; no sheet or no skill: a margin-0
//! success):
//! - **success**: inputs consumed, outputs (quality from the margin,
//!   [`QualityRule`]) go to the inventory (or the ground when full);
//! - **critical success**: + `crit_success_bonus` quality;
//! - **failure**: time spent, nothing consumed;
//! - **critical failure**: inputs consumed, no output, tools damaged.
//!
//! Tools wear on every attempt ([`CraftingConfig`], [`Durability`]).
//! Refusals are [`CraftError`]s whose `Display` is a usable `why_not`.
//!
//! ## Colony and hooks
//!
//! Workstations carry posted work as [`CraftingOrders`] ([`post_order`],
//! the `PostCraftOrder` command); [`crafting_jobs`] is the job board and
//! [`next_order_for`] picks an order an agent can do. Every completed
//! attempt calls the [`CraftingHooks`] (by name) with a [`CraftCompleted`]:
//! this crate cannot depend on `npc_progression`, so the facade's
//! crafting × progression glue registers a hook that awards XP and practice.
//!
//! ## Determinism, hashing, tooling
//!
//! Integer-only; crafters in `SimId` order, items per key in `SimId` order.
//! The plugin registers `workstation`, `crafting_orders`, `durability`,
//! `crafting`, `craft_leaf` and `crafting_config` for the state hash, and
//! its recipes, hooks, the `Craft` action and both commands in the content
//! hash. Journal: `CraftStarted`, `CraftDone`, `CraftFailed` (plus `Roll`,
//! `Reserve`/`Release`, `Spawn`). Completion runs in
//! [`NpcCraftingSet::Complete`] (after the interaction completions, before
//! `NpcSet::Effects`), inside the `crafting` perf mechanic span.

#![deny(
    clippy::float_arithmetic,
    clippy::float_cmp,
    clippy::iter_over_hash_type,
    clippy::disallowed_types,
    clippy::disallowed_methods
)]

pub mod command;
pub mod defs;
pub mod hash;
pub mod leaf;
pub mod run;
pub mod station;

use bevy_app::prelude::*;
use bevy_ecs::prelude::*;
use npc_interaction::{InteractionPlugin, InteractionRegistry, NpcInteractionSet};
use npc_jobs::Reservations;
use npc_kernel::registry::{KernelAppExt, is_frozen};
use npc_kernel::{NpcSet, NpcSimTick, SimId, SimIndex};
use npc_rules::{RulesPlugin, RulesRegistry};
use serde_json::{Value, json};

pub use command::{Craft, PostCraftOrder};
pub use defs::{
    CraftCompleted, CraftHookFn, CraftResult, CraftingConfig, CraftingHooks, QualityRule,
    RecipeDef, RecipeInput, RecipeOutput, RecipeRegistry, validate_recipe,
};
pub use hash::register_crafting_hash;
pub use leaf::{CRAFT_ACTION, CraftLeaf};
pub use run::{
    CraftError, CraftNeed, CraftPlan, CraftSource, Crafting, begin_craft, busy_with, can_craft,
    cancel_craft, check_claimed, complete_craft, crafting_complete_system, tool_name,
};
pub use station::{
    CraftJob, CraftOrder, CraftingOrders, Durability, MAX_CRAFT_ORDERS, Workstation, check_order,
    crafting_jobs, next_order_for, post_order,
};

/// The crafting systems' sets in `NpcSimTick`.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NpcCraftingSet {
    /// After `NpcInteractionSet::Complete`, before `NpcSet::Effects`.
    Complete,
}

fn assert_open(app: &App, what: &str) {
    assert!(
        !is_frozen(app.world()),
        "cannot register {what}: content is already frozen (register in Plugin::build)"
    );
}

/// Registration on `App` (any order, before the freeze).
pub trait CraftingAppExt {
    /// Registers a recipe (panics on a duplicate name; checked at freeze).
    fn register_recipe(&mut self, def: RecipeDef) -> &mut Self;
    /// Adds completion hook `name` (called in name order).
    fn add_craft_hook(&mut self, name: &'static str, f: CraftHookFn) -> &mut Self;
}

impl CraftingAppExt for App {
    fn register_recipe(&mut self, def: RecipeDef) -> &mut Self {
        assert_open(self, &format!("recipe `{}`", def.name));
        let mut r = self.world_mut().get_resource_or_init::<RecipeRegistry>();
        let name = def.name.clone();
        assert!(
            r.recipes.insert(name.clone(), def).is_none(),
            "recipe `{name}` is registered twice"
        );
        self
    }

    fn add_craft_hook(&mut self, name: &'static str, f: CraftHookFn) -> &mut Self {
        assert_open(self, &format!("craft hook `{name}`"));
        self.world_mut()
            .get_resource_or_init::<CraftingHooks>()
            .add(name, f);
        self
    }
}

/// Validates every recipe (panics with the reason) and lists recipes and
/// hooks for the content hash.
fn freeze_crafting(world: &mut World) -> Vec<(String, String)> {
    let recipes = world.get_resource_or_init::<RecipeRegistry>().clone();
    let items = world
        .get_resource::<InteractionRegistry>()
        .cloned()
        .unwrap_or_default();
    let rules = world
        .get_resource::<RulesRegistry>()
        .cloned()
        .unwrap_or_default();
    let mut out = Vec::new();
    for def in recipes.recipes.values() {
        if let Err(e) = validate_recipe(def, &items, &rules) {
            panic!("{e}");
        }
        out.push(("recipe".to_owned(), def.name.clone()));
    }
    for (name, _) in &world.get_resource_or_init::<CraftingHooks>().on_complete {
        out.push(("craft_hook".to_owned(), (*name).to_owned()));
    }
    out
}

fn kind_name(world: &World, entity: Entity) -> Option<&'static str> {
    world
        .get::<Workstation>(entity)
        .is_some()
        .then_some("workstation")
}

/// The crafting section of `entity` (`None` when it has nothing of it):
/// the running craft, a workstation's kinds and orders, an item's
/// durability.
pub fn crafting_json(world: &World, entity: Entity) -> Option<Value> {
    let mut map = serde_json::Map::new();
    if let Some(c) = world.get::<Crafting>(entity) {
        map.insert("crafting".into(), json!(c));
    }
    if let Some(w) = world.get::<Workstation>(entity) {
        map.insert("workstation".into(), json!(w.kinds));
    }
    if let Some(o) = world.get::<CraftingOrders>(entity) {
        map.insert("orders".into(), json!(o.orders));
    }
    if let Some(d) = world.get::<Durability>(entity) {
        map.insert("durability".into(), json!(d));
    }
    (!map.is_empty()).then_some(Value::Object(map))
}

/// [`crafting_json`] by `SimId`.
pub fn crafting_json_of(world: &World, id: SimId) -> Option<Value> {
    let e = world.get_resource::<SimIndex>()?.get(id)?;
    crafting_json(world, e)
}

/// The crafting capability (see the crate docs). Needs `InteractionPlugin`
/// and `RulesPlugin`.
pub struct CraftingPlugin;

impl Plugin for CraftingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CraftingConfig>()
            .init_resource::<RecipeRegistry>()
            .init_resource::<CraftingHooks>()
            .init_resource::<Reservations>();
        register_crafting_hash(app);
        app.register_action(CRAFT_ACTION, leaf::craft_start, leaf::craft_stop)
            .register_command::<Craft>()
            .register_command::<PostCraftOrder>()
            .register_kind_name("~npc_crafting", kind_name)
            .add_freeze_hook("npc_crafting", freeze_crafting)
            .configure_sets(
                NpcSimTick,
                NpcCraftingSet::Complete
                    .after(NpcInteractionSet::Complete)
                    .before(NpcSet::Effects),
            )
            .add_systems(
                NpcSimTick,
                run::crafting_complete_system.in_set(NpcCraftingSet::Complete),
            );
    }

    fn finish(&self, app: &mut App) {
        assert!(
            app.is_plugin_added::<InteractionPlugin>() && app.is_plugin_added::<RulesPlugin>(),
            "CraftingPlugin needs InteractionPlugin and RulesPlugin"
        );
    }
}
