//! `npc_rules`: an opt-in character sheet and success-roll system
//! (actor-capabilities spec §4.6, plan wave D2).
//!
//! Point-buy characters with four attributes, derived secondary
//! characteristics, traits and skills, and 3d6 roll-under success rolls.
//! The mechanics are generic and inspired by classic point-buy tabletop
//! systems; names, numbers and text are this framework's own.
//!
//! ```
//! use bevy_app::App;
//! use npc_kernel::{NpcKernelPlugin, spawn_sim};
//! use npc_rules::{
//!     CharacterSheet, Difficulty, RulesAppExt, RulesPlugin, SkillDef, Stat, roll_skill,
//! };
//!
//! let mut app = App::new();
//! app.add_plugins((NpcKernelPlugin::default(), RulesPlugin))
//!     .register_skill(
//!         SkillDef::new("Climbing", Stat::Dx, Difficulty::Average).default_stat(Stat::Dx, -5),
//!     );
//! app.finish();
//! let mut sheet = CharacterSheet::default();
//! sheet.skills.insert("Climbing".into(), 2); // DX+0 = 10
//! let (unit, _) = spawn_sim(app.world_mut(), sheet);
//! let out = roll_skill(app.world_mut(), unit, "Climbing", -2, "climb.wall").unwrap();
//! assert_eq!(out.target, 8);
//! ```
//!
//! ## Pieces
//!
//! - [`CharacterSheet`] (component): bought attribute levels, secondary
//!   deltas, trait levels, skill points and banked points. Derived values
//!   are computed on demand ([`Rules`], [`SheetView`]).
//! - Content registers by stable name ([`RulesAppExt`]): [`SkillDef`]
//!   (controlling stat, [`Difficulty`], defaults), [`TraitDef`] (point value
//!   per level, stat/skill modifiers), [`SheetTemplate`] (starting sheets).
//!   References are checked when the content freezes; the names join the
//!   content hash. The crate itself registers **no** skills or traits.
//! - [`RuleSet`] (resource, pluggable): cost tables, skill progression,
//!   difficulty offsets, crit thresholds, default-chain resolution,
//!   secondary derivation, dice and roll/contest resolution. See
//!   [`ruleset`] for the defaults.
//! - Rolls ([`roll`]): `roll_skill`, `roll_stat`, `roll_against`,
//!   `quick_contest`, `resisted_roll`, with a counter-based dice key and a
//!   journal entry per roll; `roll_with` for systems without `&mut World`;
//!   an opt-in deterministic [`RollFeed`] (in [`RollCounters`]) for mechanics
//!   that react to rolls (`npc_progression`).
//! - Points: `spend_points` (validated [`Purchase`]), `purchase_cost`,
//!   `grant_points`; the recorded command [`SpendPoints`] (not registered by
//!   the plugin: `register_command::<SpendPoints>()` to make spending
//!   replayable).
//!
//! ## Determinism, hashing, tooling
//!
//! Integer-only (Basic Speed in quarters). `RulesPlugin` registers the
//! `character_sheet` sim component (state hash, diffs, inspect JSON as the
//! [`SheetView`]) and a freeze hook; both change the content hash of any app
//! that adds the plugin, which is why it is opt-in: without it nothing of
//! this crate is registered. Rolls run inside the `rules` perf mechanic span.

#![deny(
    clippy::float_arithmetic,
    clippy::float_cmp,
    clippy::iter_over_hash_type,
    clippy::disallowed_types,
    clippy::disallowed_methods
)]

pub mod command;
pub mod defs;
pub mod roll;
pub mod ruleset;
pub mod sheet;
pub mod stats;

use bevy_app::prelude::*;
use bevy_ecs::prelude::*;
use npc_kernel::registry::{KernelAppExt, is_frozen};

pub use command::{SpendPoints, spend_command_error};
pub use defs::{RulesRegistry, SheetTemplate, SkillDef, SkillDefault, TraitDef, TraitModifier};
pub use roll::{
    Contestant, QuickContest, ResistedOutcome, RollCounters, RollFeed, RollRecord, RollSubject,
    apply_template, attribute, dice_total, effective_skill, grant_points, purchase_cost,
    purpose_id, quick_contest, resisted_roll, roll_against, roll_skill, roll_stat, roll_subject,
    roll_with, sheet_json, sheet_view, skill_level, spend_points, template_sheet, with_rules,
};
pub use ruleset::{
    ContestWinner, CostTable, CritRule, Critical, DefaultChain, DiceSpec, DifficultyOffsets,
    Limits, RollOutcome, RuleSet, SkillProgression,
};
pub use sheet::{
    CharacterSheet, LevelSource, PointBreakdown, Purchase, Rules, SheetView, SkillLevel, SkillView,
    SpendError, StatView, TraitView,
};
pub use stats::{Attributes, Difficulty, SecondaryBase, SecondaryBuys, Stat};

fn freeze_rules(world: &mut World) -> Vec<(String, String)> {
    let rule_set = world.get_resource_or_init::<RuleSet>().name;
    let mut registry = world.get_resource_or_init::<RulesRegistry>();
    registry.freeze();
    let mut manifest = registry.manifest();
    manifest.push(("rule_set".to_owned(), rule_set.to_owned()));
    manifest
}

/// Registration of rules content on `App`. Any order, before or after the
/// plugins; references are checked at freeze.
pub trait RulesAppExt {
    fn register_skill(&mut self, def: SkillDef) -> &mut Self;
    fn register_trait(&mut self, def: TraitDef) -> &mut Self;
    fn register_sheet_template(&mut self, template: SheetTemplate) -> &mut Self;
    /// Replaces the [`RuleSet`] (its name joins the content hash).
    fn set_rule_set(&mut self, rules: RuleSet) -> &mut Self;
}

fn assert_open(app: &App, what: &str) {
    assert!(
        !is_frozen(app.world()),
        "cannot register {what}: content is already frozen (register in Plugin::build)"
    );
}

impl RulesAppExt for App {
    fn register_skill(&mut self, def: SkillDef) -> &mut Self {
        assert_open(self, &format!("skill `{}`", def.name));
        self.world_mut()
            .get_resource_or_init::<RulesRegistry>()
            .add_skill(def);
        self
    }

    fn register_trait(&mut self, def: TraitDef) -> &mut Self {
        assert_open(self, &format!("trait `{}`", def.name));
        self.world_mut()
            .get_resource_or_init::<RulesRegistry>()
            .add_trait(def);
        self
    }

    fn register_sheet_template(&mut self, template: SheetTemplate) -> &mut Self {
        assert_open(self, &format!("sheet template `{}`", template.name));
        self.world_mut()
            .get_resource_or_init::<RulesRegistry>()
            .add_template(template);
        self
    }

    fn set_rule_set(&mut self, rules: RuleSet) -> &mut Self {
        assert_open(self, &format!("rule set `{}`", rules.name));
        self.world_mut().insert_resource(rules);
        self
    }
}

/// The rules mechanic: [`RuleSet`] (kept if the host inserted one),
/// [`RulesRegistry`], [`RollCounters`], the hashed `character_sheet`
/// component (JSON as [`SheetView`]) and the `npc_rules` freeze hook. It adds
/// no systems: other mechanics call the roll API.
pub struct RulesPlugin;

impl Plugin for RulesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RuleSet>()
            .init_resource::<RulesRegistry>()
            .init_resource::<RollCounters>()
            .register_sim_component_with::<CharacterSheet>(sheet_json)
            .add_freeze_hook("npc_rules", freeze_rules);
    }
}
