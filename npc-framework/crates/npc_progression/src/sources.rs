//! XP sources, registered by stable name.

use std::collections::BTreeMap;

use bevy_ecs::prelude::*;
use npc_rules::{Critical, Difficulty, RollRecord};

use crate::model::ProgressionConfig;

/// XP of one roll for a roll-driven source (0: none).
pub type RollXpFn = fn(&ProgressionConfig, &RollRecord, Option<Difficulty>) -> u32;

/// The built-in source of successful rolls ([`default_roll_xp`]).
pub const XP_ROLL: &str = "roll";
/// Completed crafts (awarded by the crafting × progression glue).
pub const XP_CRAFT: &str = "craft";
/// Completed jobs (awarded by content).
pub const XP_JOB: &str = "job";
/// Discoveries (awarded by content).
pub const XP_DISCOVERY: &str = "discovery";

/// An XP source. Every source can be awarded by name
/// ([`award_xp`](crate::award_xp)); `scale_pct` scales what is awarded
/// (100: as is). A source with `on_roll` also turns every roll of a
/// registered skill by a progressing unit into XP.
#[derive(Clone, Copy, Debug)]
pub struct XpSourceDef {
    pub name: &'static str,
    pub scale_pct: u32,
    pub on_roll: Option<RollXpFn>,
}

impl XpSourceDef {
    /// An awarded source at 100 %.
    pub fn new(name: &'static str) -> Self {
        Self {
            name,
            scale_pct: 100,
            on_roll: None,
        }
    }

    pub fn scale_pct(mut self, pct: u32) -> Self {
        self.scale_pct = pct;
        self
    }

    pub fn on_roll(mut self, f: RollXpFn) -> Self {
        self.on_roll = Some(f);
        self
    }

    /// `amount` scaled by `scale_pct` (saturating).
    pub fn scale(&self, amount: u32) -> u32 {
        let v = u64::from(amount) * u64::from(self.scale_pct) / 100;
        u32::try_from(v).unwrap_or(u32::MAX)
    }
}

/// The registered sources, by name (the names join the content hash).
#[derive(Resource, Clone, Debug, Default)]
pub struct XpSources {
    pub sources: BTreeMap<&'static str, XpSourceDef>,
}

impl XpSources {
    pub fn get(&self, name: &str) -> Option<&XpSourceDef> {
        self.sources.get(name)
    }

    /// `"a, b, c"`.
    pub fn names(&self) -> String {
        self.sources.keys().copied().collect::<Vec<_>>().join(", ")
    }

    /// Every roll-driven source, in name order.
    pub fn roll_sources(&self) -> impl Iterator<Item = &XpSourceDef> {
        self.sources.values().filter(|s| s.on_roll.is_some())
    }
}

/// The default roll XP (see [`RollXp`](crate::RollXp)): successful rolls
/// only, more for lower targets, harder skills and critical successes.
pub fn default_roll_xp(
    config: &ProgressionConfig,
    record: &RollRecord,
    difficulty: Option<Difficulty>,
) -> u32 {
    if !record.outcome.success {
        return 0;
    }
    let r = &config.roll_xp;
    let hardness = r.pivot.saturating_sub(record.outcome.target).max(0);
    let hardness = u32::try_from(hardness).unwrap_or(0).min(r.max_hardness);
    let step = match difficulty {
        Some(Difficulty::Easy) | None => 0,
        Some(Difficulty::Average) => 1,
        Some(Difficulty::Hard) => 2,
        Some(Difficulty::VeryHard) => 3,
    };
    let crit = if record.outcome.critical == Critical::Success {
        r.crit_success_bonus
    } else {
        0
    };
    r.base
        .saturating_add(hardness)
        .saturating_add(r.per_difficulty.saturating_mul(step))
        .saturating_add(crit)
}

/// The sources `ProgressionPlugin` registers by default: `roll` (with
/// [`default_roll_xp`]), `craft`, `job`, `discovery`.
pub fn default_sources() -> Vec<XpSourceDef> {
    vec![
        XpSourceDef::new(XP_ROLL).on_roll(default_roll_xp),
        XpSourceDef::new(XP_CRAFT),
        XpSourceDef::new(XP_JOB),
        XpSourceDef::new(XP_DISCOVERY),
    ]
}
