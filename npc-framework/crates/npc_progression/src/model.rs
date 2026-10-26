//! Components, configuration and their state-hash encodings.

use std::collections::BTreeMap;

use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use npc_kernel::hash::{Fnv64, SimHash, SimHashResource};
use serde::{Deserialize, Serialize};

/// A unit's experience. Units progress only with this component (and a
/// `CharacterSheet`); it brings [`Learning`] and [`SpendPolicy`] (default
/// [`SpendPolicy::Ai`]) with it. Level 0 is the starting level. Hashed as
/// `experience`.
#[derive(
    Component, Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect,
)]
#[require(Learning, SpendPolicy)]
#[reflect(Component)]
pub struct Experience {
    /// Total XP ever gained.
    pub xp: u64,
    pub level: u32,
}

/// How a unit's level-granted character points are spent. Hashed as
/// `spend_policy`.
#[derive(
    Component,
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    Reflect,
)]
#[reflect(Component)]
pub enum SpendPolicy {
    /// Spend at once after a level-up, one skill point at a time, each on
    /// the skill with the highest `uses / (points + 1)` (ties: name order):
    /// weighted toward the skills the unit uses, with diminishing returns.
    /// Units that used no skill keep their points banked.
    #[default]
    Ai,
    /// Bank the points; a player spends them with the `npc_rules`
    /// `SpendPoints` command.
    Player,
}

impl SpendPolicy {
    fn tag(self) -> u8 {
        match self {
            SpendPolicy::Ai => 0,
            SpendPolicy::Player => 1,
        }
    }
}

/// Per-skill learning state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
pub struct SkillProgress {
    /// Rolls of this skill (what the AI spend policy weighs).
    pub uses: u32,
    /// Practice ticks toward the next point (capped at one point's worth
    /// while the practice cap blocks it).
    pub practice: u32,
    /// Study ticks toward the next point.
    pub study: u32,
    /// Teaching ticks toward the next point.
    pub teaching: u32,
}

/// Skill name → learning state (only skills the unit used or learned).
/// Comes with [`Experience`]. Hashed as `learning`.
#[derive(
    Component, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect,
)]
#[reflect(Component)]
pub struct Learning {
    pub skills: BTreeMap<String, SkillProgress>,
}

impl Learning {
    /// The state of `skill` (zero if untouched).
    pub fn get(&self, skill: &str) -> SkillProgress {
        self.skills.get(skill).copied().unwrap_or_default()
    }

    /// How often `skill` was rolled.
    pub fn uses(&self, skill: &str) -> u32 {
        self.get(skill).uses
    }
}

/// Total XP needed to reach each level.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum LevelCurve {
    /// Level `L` needs `base * L * (L + 1) / 2` total XP (with `base` 100:
    /// 100, 300, 600, 1000, 1500, ..): each level costs `base` more than the
    /// one before.
    Triangular { base: u64 },
    /// `totals[L - 1]` total XP for level `L`; beyond the table every level
    /// costs `then_every` more (0: the table is the last level).
    Table { totals: Vec<u64>, then_every: u64 },
}

impl Default for LevelCurve {
    fn default() -> Self {
        LevelCurve::Triangular { base: 100 }
    }
}

impl LevelCurve {
    /// Total XP to reach `level` (0 for level 0; `None` when unreachable).
    pub fn total_for(&self, level: u32) -> Option<u64> {
        if level == 0 {
            return Some(0);
        }
        let l = u64::from(level);
        match self {
            LevelCurve::Triangular { base } => {
                base.checked_mul(l)?.checked_mul(l + 1).map(|v| v / 2)
            }
            LevelCurve::Table { totals, then_every } => {
                let i = usize::try_from(level - 1).ok()?;
                if let Some(&t) = totals.get(i) {
                    return Some(t);
                }
                let last = *totals.last()?;
                if *then_every == 0 {
                    return None;
                }
                let beyond = u64::try_from(i + 1 - totals.len()).ok()?;
                last.checked_add(beyond.checked_mul(*then_every)?)
            }
        }
    }

    fn hash(&self, h: &mut Fnv64) {
        match self {
            LevelCurve::Triangular { base } => {
                h.u8(0);
                h.u64(*base);
            }
            LevelCurve::Table { totals, then_every } => {
                h.u8(1);
                h.u32(u32::try_from(totals.len()).expect("level table too long"));
                for t in totals {
                    h.u64(*t);
                }
                h.u64(*then_every);
            }
        }
    }
}

/// The default XP of a roll (source `"roll"`,
/// [`default_roll_xp`](crate::default_roll_xp)): a **successful** roll of a registered skill
/// gives `base + clamp(pivot - target, 0, max_hardness)` (harder rolls, i.e.
/// lower targets, give more) `+ per_difficulty * d` (`d` = 0 Easy, 1 Average,
/// 2 Hard, 3 Very Hard) `+ crit_success_bonus` on a critical success. A
/// failure gives nothing. Target 10 on an Average skill: 5 + 6 + 2 = 13.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RollXp {
    pub base: u32,
    pub pivot: i32,
    pub max_hardness: u32,
    pub per_difficulty: u32,
    pub crit_success_bonus: u32,
}

impl Default for RollXp {
    fn default() -> Self {
        Self {
            base: 5,
            pivot: 16,
            max_hardness: 10,
            per_difficulty: 2,
            crit_success_bonus: 5,
        }
    }
}

/// Progression tuning. Insert (or `ProgressionAppExt::set_progression_config`)
/// before the freeze to change it. Hashed as `progression_config`.
///
/// **Learning.** Practice, study and teaching each accrue *ticks* per skill;
/// every `*_ticks_per_point` ticks convert into one free skill point, unless
/// that point would lift the skill's effective level above the source's
/// cap: practice `stat + practice_cap_above_stat`, study `stat +
/// study_cap_above_stat` (and the book's own maximum), teaching the
/// teacher's level. A capped source keeps one point's worth of ticks and
/// waits (e.g. for the attribute to rise).
#[derive(Resource, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProgressionConfig {
    pub curve: LevelCurve,
    /// Banked character points granted per level gained.
    pub points_per_level: i32,
    pub max_level: u32,
    pub roll_xp: RollXp,
    /// Practice ticks a roll of a registered skill accrues (any outcome).
    pub practice_per_roll: u32,
    pub practice_ticks_per_point: u32,
    /// Practice alone cannot raise a skill above its attribute + this.
    pub practice_cap_above_stat: i32,
    pub study_ticks_per_point: u32,
    pub study_cap_above_stat: i32,
    pub teaching_ticks_per_point: u32,
    /// A teacher's level must be at least the student's + this.
    pub teaching_min_lead: i32,
}

impl Default for ProgressionConfig {
    fn default() -> Self {
        Self {
            curve: LevelCurve::default(),
            points_per_level: 5,
            max_level: 50,
            roll_xp: RollXp::default(),
            practice_per_roll: 30,
            practice_ticks_per_point: 300,
            practice_cap_above_stat: 2,
            study_ticks_per_point: 200,
            study_cap_above_stat: 4,
            teaching_ticks_per_point: 100,
            teaching_min_lead: 1,
        }
    }
}

impl SimHash for Experience {
    const NAME: &'static str = "experience";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u64(self.xp);
        h.u32(self.level);
    }
}

impl SimHash for SpendPolicy {
    const NAME: &'static str = "spend_policy";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u8(self.tag());
    }
}

impl SimHash for Learning {
    const NAME: &'static str = "learning";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u32(u32::try_from(self.skills.len()).expect("too many skills"));
        for (name, p) in &self.skills {
            h.str(name);
            h.u32(p.uses);
            h.u32(p.practice);
            h.u32(p.study);
            h.u32(p.teaching);
        }
    }
}

impl SimHashResource for ProgressionConfig {
    const NAME: &'static str = "progression_config";
    fn sim_hash(&self, h: &mut Fnv64) {
        self.curve.hash(h);
        h.i32(self.points_per_level);
        h.u32(self.max_level);
        let r = &self.roll_xp;
        h.u32(r.base);
        h.i32(r.pivot);
        h.u32(r.max_hardness);
        h.u32(r.per_difficulty);
        h.u32(r.crit_success_bonus);
        h.u32(self.practice_per_roll);
        h.u32(self.practice_ticks_per_point);
        h.i32(self.practice_cap_above_stat);
        h.u32(self.study_ticks_per_point);
        h.i32(self.study_cap_above_stat);
        h.u32(self.teaching_ticks_per_point);
        h.i32(self.teaching_min_lead);
    }
}
