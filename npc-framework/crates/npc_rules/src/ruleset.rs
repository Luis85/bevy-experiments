//! The pluggable [`RuleSet`]: point costs, skill progression, crit rules,
//! default-chain resolution, secondary derivation and roll resolution.
//!
//! The defaults are this framework's own numbers. They follow the familiar
//! point-buy / 3d6 roll-under *shape* (attributes cost points per level,
//! skill points buy relative levels on a doubling-then-linear curve,
//! difficulty shifts the curve down, extreme rolls are critical) without
//! reproducing any published rulebook text or table. Every number is a
//! public field: a game replaces the resource (`set_rule_set` or
//! `insert_resource` before the plugin) and every caller keeps working. A
//! different dice mechanic (e.g. one d20, roll-over) only needs other
//! [`DiceSpec`] and [`RuleSet::resolve`] values.

use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};

use crate::stats::{Attributes, Difficulty, SecondaryBase, Stat};
pub use npc_kernel::journal::Critical;

/// The dice of one roll: `count` dice with `sides` faces (default 3d6).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DiceSpec {
    pub count: u8,
    pub sides: u8,
}

impl DiceSpec {
    pub const D6X3: DiceSpec = DiceSpec { count: 3, sides: 6 };

    /// Lowest and highest total.
    pub fn range(self) -> (i32, i32) {
        (
            i32::from(self.count),
            i32::from(self.count) * i32::from(self.sides),
        )
    }
}

/// Points per level of each stat (attributes above/below
/// [`RuleSet::attribute_base`], secondary buys above/below the derived value;
/// `speed_quarter` per quarter of Basic Speed).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CostTable {
    pub st: i32,
    pub dx: i32,
    pub iq: i32,
    pub ht: i32,
    pub hp: i32,
    pub will: i32,
    pub per: i32,
    pub fp: i32,
    pub speed_quarter: i32,
    pub basic_move: i32,
}

impl Default for CostTable {
    /// Our defaults: strength and health are cheaper than agility and
    /// intellect, secondary buys are cheap.
    fn default() -> Self {
        Self {
            st: 10,
            dx: 20,
            iq: 20,
            ht: 10,
            hp: 2,
            will: 5,
            per: 5,
            fp: 3,
            speed_quarter: 5,
            basic_move: 5,
        }
    }
}

impl CostTable {
    pub fn per_level(&self, stat: Stat) -> i32 {
        match stat {
            Stat::St => self.st,
            Stat::Dx => self.dx,
            Stat::Iq => self.iq,
            Stat::Ht => self.ht,
            Stat::Hp => self.hp,
            Stat::Will => self.will,
            Stat::Per => self.per,
            Stat::Fp => self.fp,
            Stat::Speed => self.speed_quarter,
            Stat::Move => self.basic_move,
        }
    }
}

/// How skill points turn into a bonus over the difficulty offset.
///
/// `steps[i]` is the total points needed for bonus `+i`; beyond the last
/// step every further `then_every` points add `+1`. Fewer points than
/// `steps[0]` buy nothing. Default `[1, 2, 4]`, then every 4 (a doubling
/// then linear curve).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SkillProgression {
    pub steps: Vec<i32>,
    pub then_every: i32,
}

impl Default for SkillProgression {
    fn default() -> Self {
        Self {
            steps: vec![1, 2, 4],
            then_every: 4,
        }
    }
}

impl SkillProgression {
    /// The bonus `points` buy, `None` below the first step.
    pub fn bonus(&self, points: i32) -> Option<i32> {
        let first = *self.steps.first()?;
        if points < first {
            return None;
        }
        let i = self.steps.iter().rposition(|&s| s <= points)?;
        let last = self.steps.len() - 1;
        let base = i32::try_from(i).ok()?;
        if i < last || self.then_every <= 0 {
            return Some(base);
        }
        Some(base + (points - self.steps[last]) / self.then_every)
    }

    /// Total points needed for bonus `bonus` (`None` for a negative bonus,
    /// or one beyond the last step when `then_every <= 0`).
    pub fn points_for(&self, bonus: i32) -> Option<i32> {
        let b = usize::try_from(bonus).ok()?;
        if let Some(&p) = self.steps.get(b) {
            return Some(p);
        }
        if self.then_every <= 0 {
            return None;
        }
        let last = self.steps.len().checked_sub(1)?;
        let extra = i32::try_from(b - last).ok()?;
        self.steps[last].checked_add(extra.checked_mul(self.then_every)?)
    }
}

/// Relative level the first step of points buys, per difficulty
/// (default Easy 0, Average −1, Hard −2, Very Hard −3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DifficultyOffsets {
    pub easy: i32,
    pub average: i32,
    pub hard: i32,
    pub very_hard: i32,
}

impl Default for DifficultyOffsets {
    fn default() -> Self {
        Self {
            easy: 0,
            average: -1,
            hard: -2,
            very_hard: -3,
        }
    }
}

impl DifficultyOffsets {
    pub fn get(&self, d: Difficulty) -> i32 {
        match d {
            Difficulty::Easy => self.easy,
            Difficulty::Average => self.average,
            Difficulty::Hard => self.hard,
            Difficulty::VeryHard => self.very_hard,
        }
    }
}

/// Critical thresholds of the default roll-under resolution.
///
/// - Success: `roll <= auto_success_max` always succeeds,
///   `roll >= auto_failure_min` always fails, otherwise `roll <= target`.
/// - Critical success (only on a success): `roll <= crit_success_max`, or
///   `roll <= min(crit_success_cap, target - crit_success_margin)`.
/// - Critical failure (only on a failure): `roll >= crit_failure_min`, or
///   `roll >= target + crit_failure_margin`.
///
/// Defaults: 4 / 17 / 4, margin 10, cap 6 / 18, margin 10. So 3–4 is always
/// a critical success, 5 also at target 15+, 6 also at 16+; 18 is always a
/// critical failure, and any roll 10 or more above the target is one too.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CritRule {
    pub auto_success_max: i32,
    pub auto_failure_min: i32,
    pub crit_success_max: i32,
    pub crit_success_margin: i32,
    pub crit_success_cap: i32,
    pub crit_failure_min: i32,
    pub crit_failure_margin: i32,
}

impl Default for CritRule {
    fn default() -> Self {
        Self {
            auto_success_max: 4,
            auto_failure_min: 17,
            crit_success_max: 4,
            crit_success_margin: 10,
            crit_success_cap: 6,
            crit_failure_min: 18,
            crit_failure_margin: 10,
        }
    }
}

/// How skill defaults resolve.
///
/// - `max_depth`: how many default-of-default hops a skill-to-skill default
///   may take. `0` (the default): a skill defaults from another skill only
///   through that skill's **bought** level, never through the other skill's
///   own defaults. Cycles are impossible (every hop decrements the depth).
/// - `best_of_bought_and_default`: with points in a skill, use the better of
///   the bought level and the best default (default true).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DefaultChain {
    pub max_depth: u8,
    pub best_of_bought_and_default: bool,
}

impl Default for DefaultChain {
    fn default() -> Self {
        Self {
            max_depth: 0,
            best_of_bought_and_default: true,
        }
    }
}

/// Purchase limits checked by [`spend_points`](crate::spend_points).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Limits {
    /// Highest bought attribute level.
    pub attribute_max: i32,
    /// Highest bought delta of a secondary characteristic.
    pub secondary_buy_max: i32,
    /// Most points in one skill.
    pub skill_points_max: i32,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            attribute_max: 30,
            secondary_buy_max: 20,
            skill_points_max: 1000,
        }
    }
}

/// One resolved roll.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RollOutcome {
    /// The dice total.
    pub roll: i32,
    /// Effective level plus modifiers.
    pub target: i32,
    /// `target - roll` (positive: succeeded by that much).
    pub margin: i32,
    /// Whether it succeeded (auto-success/failure rules can disagree with
    /// the sign of `margin`).
    pub success: bool,
    pub critical: Critical,
}

/// Winner of a quick contest.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ContestWinner {
    A,
    B,
    Tie,
}

/// The default secondary derivation: HP from ST, Will and Per from IQ, FP
/// from HT, Basic Speed (in quarters) = HT + DX, i.e. (HT + DX) / 4.
pub fn derive_secondary_default(a: &Attributes) -> SecondaryBase {
    SecondaryBase {
        hp: a.st,
        will: a.iq,
        per: a.iq,
        fp: a.ht,
        speed_q: a.ht + a.dx,
    }
}

/// Default Basic Move: whole units of Basic Speed (`floor(q / 4)`).
pub fn move_from_speed_default(speed_q: i32) -> i32 {
    speed_q.div_euclid(4)
}

/// The default resolution: roll-under with [`RuleSet::crit`].
pub fn resolve_roll_under(rules: &RuleSet, roll: i32, target: i32) -> RollOutcome {
    let c = &rules.crit;
    let success = if roll <= c.auto_success_max {
        true
    } else if roll >= c.auto_failure_min {
        false
    } else {
        roll <= target
    };
    let critical = if success {
        let by_margin = c
            .crit_success_cap
            .min(target.saturating_sub(c.crit_success_margin));
        if roll <= c.crit_success_max.max(by_margin) {
            Critical::Success
        } else {
            Critical::None
        }
    } else if roll >= c.crit_failure_min || roll >= target.saturating_add(c.crit_failure_margin) {
        Critical::Failure
    } else {
        Critical::None
    };
    RollOutcome {
        roll,
        target,
        margin: target.saturating_sub(roll),
        success,
        critical,
    }
}

/// The default quick contest: if exactly one side succeeds it wins;
/// otherwise the larger margin wins; equal margins tie.
pub fn quick_contest_default(a: &RollOutcome, b: &RollOutcome) -> ContestWinner {
    match (a.success, b.success) {
        (true, false) => ContestWinner::A,
        (false, true) => ContestWinner::B,
        _ => match a.margin.cmp(&b.margin) {
            std::cmp::Ordering::Greater => ContestWinner::A,
            std::cmp::Ordering::Less => ContestWinner::B,
            std::cmp::Ordering::Equal => ContestWinner::Tie,
        },
    }
}

/// The rules every sheet computation and roll goes through. A resource:
/// replace it to change the mechanics without touching callers.
#[derive(Resource, Clone, Debug)]
pub struct RuleSet {
    /// Stable name (part of the content hash).
    pub name: &'static str,
    pub dice: DiceSpec,
    /// Attribute level that costs nothing.
    pub attribute_base: i32,
    pub costs: CostTable,
    pub progression: SkillProgression,
    pub difficulty: DifficultyOffsets,
    pub crit: CritRule,
    pub defaults: DefaultChain,
    pub limits: Limits,
    /// Secondary values from the effective attributes.
    pub derive_secondary: fn(&Attributes) -> SecondaryBase,
    /// Basic Move from the final Basic Speed (quarters).
    pub move_from_speed: fn(i32) -> i32,
    /// `(rules, dice total, target) -> outcome`.
    pub resolve: fn(&RuleSet, i32, i32) -> RollOutcome,
    /// Quick-contest winner from two outcomes.
    pub contest: fn(&RollOutcome, &RollOutcome) -> ContestWinner,
}

impl Default for RuleSet {
    fn default() -> Self {
        Self {
            name: "npc_rules.default",
            dice: DiceSpec::D6X3,
            attribute_base: 10,
            costs: CostTable::default(),
            progression: SkillProgression::default(),
            difficulty: DifficultyOffsets::default(),
            crit: CritRule::default(),
            defaults: DefaultChain::default(),
            limits: Limits::default(),
            derive_secondary: derive_secondary_default,
            move_from_speed: move_from_speed_default,
            resolve: resolve_roll_under,
            contest: quick_contest_default,
        }
    }
}

impl RuleSet {
    /// Resolves a dice total against `target` ([`RuleSet::resolve`]).
    pub fn resolve_roll(&self, roll: i32, target: i32) -> RollOutcome {
        (self.resolve)(self, roll, target)
    }

    /// The quick-contest winner ([`RuleSet::contest`]).
    pub fn contest_winner(&self, a: &RollOutcome, b: &RollOutcome) -> ContestWinner {
        (self.contest)(a, b)
    }

    /// Relative level (to the controlling stat) that `points` buy in a skill
    /// of difficulty `d`; `None` when they buy nothing.
    pub fn relative_level(&self, d: Difficulty, points: i32) -> Option<i32> {
        Some(self.difficulty.get(d) + self.progression.bonus(points)?)
    }

    /// Total points a skill of difficulty `d` needs for relative level
    /// `relative`; `None` when unreachable (below the first step).
    pub fn points_for_relative(&self, d: Difficulty, relative: i32) -> Option<i32> {
        self.progression
            .points_for(relative - self.difficulty.get(d))
    }

    /// Points a stat costs at bought value `bought` (attributes relative to
    /// `attribute_base`, secondary deltas from 0). Negative below the base.
    pub fn stat_cost(&self, stat: Stat, bought: i32) -> i32 {
        let from = if stat.is_attribute() {
            self.attribute_base
        } else {
            0
        };
        (bought - from).saturating_mul(self.costs.per_level(stat))
    }
}
