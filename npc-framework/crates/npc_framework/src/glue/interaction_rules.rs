//! Interaction × rules (features `interaction` + `rules`): interaction skill
//! checks roll with `npc_rules`.
//!
//! [`InteractionRulesPlugin`] sets [`rules_skill_check`] as the interaction
//! skill-check hook (name `npc_rules`, which joins the content hash). A
//! verb's check (`InteractionDef::check`, a `ToolTarget`'s check) is then a
//! `roll_skill` with purpose `interaction.<verb>`, journaled as `Roll`; a
//! failed roll spends the interaction's time and applies no effect
//! (`Interacted { outcome: "failed" }`).

#![deny(
    clippy::float_arithmetic,
    clippy::float_cmp,
    clippy::iter_over_hash_type,
    clippy::disallowed_types,
    clippy::disallowed_methods
)]

use bevy_app::prelude::*;
use bevy_ecs::prelude::*;
use npc_kernel::SimId;

use npc_interaction::{InteractionAppExt, SkillCheck};

/// Skill-check hook backed by `npc_rules`: a unit without a
/// `CharacterSheet` passes (no rules for it); with one, it rolls
/// `npc_rules::roll_skill` (journaled `Roll`), and a skill it cannot
/// attempt (no points, no default) fails.
pub fn rules_skill_check(
    world: &mut World,
    actor: Entity,
    check: &SkillCheck,
    purpose: &str,
) -> Option<bool> {
    let id = *world.get::<SimId>(actor)?;
    world.get::<npc_rules::CharacterSheet>(actor)?;
    Some(
        npc_rules::roll_skill(world, id, &check.skill, check.modifier, purpose)
            .is_some_and(|o| o.success),
    )
}

/// Sets [`rules_skill_check`] as the interaction skill-check hook
/// (name `npc_rules`). Add `RulesPlugin` too.
pub struct InteractionRulesPlugin;

impl Plugin for InteractionRulesPlugin {
    fn build(&self, app: &mut App) {
        app.set_skill_check_hook("npc_rules", rules_skill_check);
    }
}
