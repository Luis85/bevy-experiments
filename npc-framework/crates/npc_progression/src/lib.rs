//! `npc_progression`: opt-in experience, levels and learning on top of
//! `npc_rules` (actor-capabilities spec §4.7, plan wave F3; ADR 0012 lets
//! it depend on `npc_rules` and the foundations, nothing else).
//!
//! Add [`ProgressionPlugin`] next to `RulesPlugin` and put [`Experience`] on
//! units that have a `CharacterSheet` (it brings [`Learning`] and
//! [`SpendPolicy::Ai`]; insert [`SpendPolicy::Player`] to bank points
//! instead). Without the plugin nothing here is registered, hashed or run.
//!
//! ## XP
//!
//! XP comes from sources registered by stable name ([`XpSourceDef`],
//! [`ProgressionAppExt::register_xp_source`]); the plugin registers `roll`,
//! `craft`, `job` and `discovery` ([`default_sources`]).
//! - **Rolls**: the plugin switches on the rules' deterministic roll feed
//!   (`npc_rules::RollFeed`, never the journal). Every roll of a
//!   *registered skill* by a unit with `Experience` counts a use, accrues
//!   practice and earns the XP of every roll-driven source (default
//!   [`default_roll_xp`]: successes only, scaled by target and difficulty).
//!   Stat rolls (`Per`, `HT`) earn nothing.
//! - **Awards**: [`award_xp`]`(world, unit, amount, source)` for completed
//!   crafts, jobs, discoveries or content sources.
//!
//! ## Levels and points
//!
//! A [`LevelCurve`] (default triangular, base 100: levels at 100, 300, 600,
//! 1000 .. total XP) and `points_per_level` (default 5) banked character
//! points per level, added as `npc_rules::grant_points` does. The
//! [`SpendPolicy`] decides what happens next: `Ai` spends at once, one skill
//! point at a time on the skill with the highest uses per point already in
//! it ([`ai_spend`]); `Player` banks them for the `npc_rules` `SpendPoints`
//! command (register it with `register_command::<SpendPoints>()`, or use the
//! rules × agent-io glue that does).
//!
//! ## Learning
//!
//! Practice (every roll: `practice_per_roll` ticks, or [`grant_practice`]),
//! study ([`grant_study`]) and teaching ([`grant_teaching`]) accrue ticks
//! per skill that convert into free skill points at their own rate, each
//! under a cap: practice alone stops at attribute + 2, study at
//! attribute + 4 (and the book's maximum), teaching at the teacher's level.
//! See [`ProgressionConfig`] for the defaults. This crate registers no verbs:
//! `study` (with a book item) and `teach` need `npc_interaction`, so they
//! live in facade glue, which calls the grant functions.
//!
//! ## Determinism, hashing, tooling
//!
//! Integer-only; units in `SimId` order, each unit's rolls in roll order.
//! The plugin registers `experience` (inspect JSON: [`experience_json`]),
//! `learning`, `spend_policy` and `progression_config` for the state hash,
//! and its XP sources in the content hash. Journal: `XpGained`, `LevelUp`,
//! `SkillImproved`. The system runs in `NpcSet::Cleanup` before the journal
//! flush, inside the `progression` perf mechanic span.

#![deny(
    clippy::float_arithmetic,
    clippy::float_cmp,
    clippy::iter_over_hash_type,
    clippy::disallowed_types,
    clippy::disallowed_methods
)]

pub mod engine;
pub mod model;
pub mod sources;

use bevy_app::prelude::*;
use bevy_ecs::prelude::*;
use npc_kernel::journal::flush_journal_system;
use npc_kernel::registry::{KernelAppExt, is_frozen};
use npc_kernel::{NpcSet, NpcSimTick, SimId, SimIndex};
use npc_rules::{RollCounters, RuleSet, RulesPlugin, RulesRegistry};
use serde_json::{Value, json};

pub use engine::{
    LearnOutcome, ProgressionError, UnitState, Via, XpAward, add_xp, ai_spend, apply_rolls,
    award_xp, grant_practice, grant_study, grant_teaching, learn, progression_system, stat_cap,
    xp_for_level,
};
pub use model::{
    Experience, Learning, LevelCurve, ProgressionConfig, RollXp, SkillProgress, SpendPolicy,
};
pub use sources::{
    RollXpFn, XP_CRAFT, XP_DISCOVERY, XP_JOB, XP_ROLL, XpSourceDef, XpSources, default_roll_xp,
    default_sources,
};

fn assert_open(app: &App, what: &str) {
    assert!(
        !is_frozen(app.world()),
        "cannot register {what}: content is already frozen (register in Plugin::build)"
    );
}

/// Registration on `App` (any order, before the freeze).
pub trait ProgressionAppExt {
    /// Registers an XP source (panics on a duplicate name).
    fn register_xp_source(&mut self, def: XpSourceDef) -> &mut Self;
    /// Replaces (or adds) a source, e.g. `roll` with another formula.
    fn override_xp_source(&mut self, def: XpSourceDef) -> &mut Self;
    /// Replaces the [`ProgressionConfig`].
    fn set_progression_config(&mut self, config: ProgressionConfig) -> &mut Self;
}

impl ProgressionAppExt for App {
    fn register_xp_source(&mut self, def: XpSourceDef) -> &mut Self {
        assert_open(self, &format!("xp source `{}`", def.name));
        let mut s = self.world_mut().get_resource_or_init::<XpSources>();
        assert!(
            s.sources.insert(def.name, def).is_none(),
            "xp source `{}` is registered twice",
            def.name
        );
        self
    }

    fn override_xp_source(&mut self, def: XpSourceDef) -> &mut Self {
        assert_open(self, &format!("xp source `{}`", def.name));
        self.world_mut()
            .get_resource_or_init::<XpSources>()
            .sources
            .insert(def.name, def);
        self
    }

    fn set_progression_config(&mut self, config: ProgressionConfig) -> &mut Self {
        assert_open(self, "the progression config");
        self.world_mut().insert_resource(config);
        self
    }
}

fn freeze_progression(world: &mut World) -> Vec<(String, String)> {
    world
        .get_resource_or_init::<XpSources>()
        .sources
        .values()
        .map(|s| {
            let kind = if s.on_roll.is_some() { "roll" } else { "award" };
            (
                "xp_source".to_owned(),
                format!("{}:{kind}:{}", s.name, s.scale_pct),
            )
        })
        .collect()
}

/// The progression section of `entity` (`None` without [`Experience`]):
/// `{"xp", "level", "next_level_xp", "policy", "skills": {name: {uses,
/// practice, study, teaching}}}`. Also the inspect JSON of `experience`.
pub fn experience_json(world: &World, entity: Entity) -> Option<Value> {
    let exp = world.get::<Experience>(entity)?;
    let config = world
        .get_resource::<ProgressionConfig>()
        .cloned()
        .unwrap_or_default();
    let next = if exp.level < config.max_level {
        config.curve.total_for(exp.level + 1)
    } else {
        None
    };
    Some(json!({
        "xp": exp.xp,
        "level": exp.level,
        "next_level_xp": next,
        "policy": world.get::<SpendPolicy>(entity).copied().unwrap_or_default(),
        "skills": world.get::<Learning>(entity).map(|l| l.skills.clone()).unwrap_or_default(),
    }))
}

/// [`experience_json`] by `SimId`.
pub fn experience_json_of(world: &World, id: SimId) -> Option<Value> {
    let e = world.get_resource::<SimIndex>()?.get(id)?;
    experience_json(world, e)
}

/// The progression capability (see the crate docs). Needs `RulesPlugin`.
pub struct ProgressionPlugin {
    /// Register [`default_sources`] (default `true`).
    pub default_sources: bool,
}

impl Default for ProgressionPlugin {
    fn default() -> Self {
        Self {
            default_sources: true,
        }
    }
}

impl Plugin for ProgressionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ProgressionConfig>()
            .init_resource::<XpSources>()
            .init_resource::<RuleSet>()
            .init_resource::<RulesRegistry>();
        app.world_mut()
            .get_resource_or_init::<RollCounters>()
            .feed
            .enabled = true;
        if self.default_sources {
            for def in default_sources() {
                app.register_xp_source(def);
            }
        }
        app.register_sim_component_with::<Experience>(experience_json)
            .register_sim_component::<Learning>()
            .register_sim_component::<SpendPolicy>()
            .register_sim_resource::<ProgressionConfig>()
            .add_freeze_hook("npc_progression", freeze_progression)
            .add_systems(
                NpcSimTick,
                progression_system
                    .in_set(NpcSet::Cleanup)
                    .before(flush_journal_system),
            );
    }

    fn finish(&self, app: &mut App) {
        assert!(
            app.is_plugin_added::<RulesPlugin>(),
            "ProgressionPlugin needs RulesPlugin (character sheets)"
        );
    }
}
