//! Crafting × interaction (feature `crafting`, which turns on
//! `interaction`): a unit that crafts is busy for interactions too.
//!
//! `npc_crafting` already refuses to start a craft while the unit runs an
//! interaction (its `busy_with` checks both). The other direction needs a
//! hook, because `npc_interaction` cannot know about crafting:
//! [`CraftingInteractionPlugin`] adds the interaction busy predicate
//! `npc_crafting` ([`crafting_busy`]), so `npc_interaction::begin` (the
//! `Interact` leaf and command, agent affordances, RTS `Interact` orders,
//! mounting) refuses with `"busy with craft bread"` while a craft runs.

#![deny(
    clippy::float_arithmetic,
    clippy::float_cmp,
    clippy::iter_over_hash_type,
    clippy::disallowed_types,
    clippy::disallowed_methods
)]

use bevy_app::{App, Plugin};
use bevy_ecs::prelude::*;
use npc_crafting::Crafting;
use npc_interaction::InteractionAppExt;

/// Name of the busy predicate.
pub const CRAFTING_BUSY: &str = "npc_crafting";

/// Busy predicate: the unit's running craft, as `"craft <recipe>"`.
pub fn crafting_busy(world: &World, actor: Entity) -> Option<String> {
    world
        .get::<Crafting>(actor)
        .map(|c| format!("craft {}", c.recipe))
}

/// Crafting × interaction glue (see the module docs). Add it with
/// `CraftingPlugin` (and so `InteractionPlugin`).
pub struct CraftingInteractionPlugin;

impl Plugin for CraftingInteractionPlugin {
    fn build(&self, app: &mut App) {
        app.add_busy_check(CRAFTING_BUSY, crafting_busy);
    }
}
