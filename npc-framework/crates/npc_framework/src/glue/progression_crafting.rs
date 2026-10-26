//! Progression × crafting (features `progression` + `crafting`): crafting
//! teaches.
//!
//! [`ProgressionCraftingPlugin`] adds the craft completion hook
//! `npc_progression` ([`craft_progress`]): for every craft attempt that ran
//! its full duration,
//!
//! - a success (or critical success) awards the recipe's `xp` from XP
//!   source `craft` (`npc_progression::XP_CRAFT`, journal `XpGained`, then
//!   `LevelUp` / points / the AI spend as usual);
//! - any outcome accrues the recipe's duration as practice on its skill
//!   (`grant_practice`, capped at attribute + `practice_cap_above_stat`).
//!
//! The roll itself already feeds progression (roll XP, a use, roll
//! practice) through the rules roll feed. Crafters without `Experience`
//! (or without a sheet) are skipped.

#![deny(
    clippy::float_arithmetic,
    clippy::float_cmp,
    clippy::iter_over_hash_type,
    clippy::disallowed_types,
    clippy::disallowed_methods
)]

use bevy_app::{App, Plugin};
use bevy_ecs::prelude::*;
use npc_crafting::{CraftCompleted, CraftingAppExt};
use npc_progression::{XP_CRAFT, award_xp, grant_practice};

/// Name of the craft hook.
pub const CRAFT_PROGRESS_HOOK: &str = "npc_progression";

/// The craft completion hook (see the module docs).
pub fn craft_progress(world: &mut World, done: &CraftCompleted) {
    if done.result.succeeded() && done.xp > 0 {
        // Not progressing (no Experience / sheet): nothing to award.
        let _ = award_xp(world, done.unit, done.xp, XP_CRAFT);
    }
    if let Some(skill) = &done.skill
        && done.duration_ticks > 0
    {
        let _ = grant_practice(world, done.unit, skill, done.duration_ticks);
    }
}

/// Progression × crafting glue (see the module docs). Add it with
/// `ProgressionPlugin` and `CraftingPlugin`.
pub struct ProgressionCraftingPlugin;

impl Plugin for ProgressionCraftingPlugin {
    fn build(&self, app: &mut App) {
        app.add_craft_hook(CRAFT_PROGRESS_HOOK, craft_progress);
    }
}
