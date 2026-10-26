//! The [`CharacterSheet`] component and every computation on it ([`Rules`]):
//! effective stats, skill levels (points, defaults, trait modifiers), point
//! totals, purchases and the serializable [`SheetView`].
//!
//! The sheet stores only what was **bought** (attribute levels, secondary
//! deltas, trait levels, skill points, banked points). Everything derived
//! (secondary values, relative and effective skill levels, point totals) is
//! computed on demand from the [`RuleSet`] and [`RulesRegistry`], so it can
//! never go stale when content or rules change.

use std::collections::BTreeMap;
use std::fmt;

use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use npc_kernel::hash::{Fnv64, SimHash};
use serde::{Deserialize, Serialize};

use crate::defs::{RulesRegistry, SheetTemplate, SkillDefault, TraitModifier};
use crate::ruleset::RuleSet;
use crate::stats::{Attributes, Difficulty, SecondaryBuys, Stat, quarters_label};

/// A unit's character sheet (bought values only; see the module docs).
/// Hashed as `character_sheet` once `RulesPlugin` is added.
#[derive(
    Component, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect,
)]
#[reflect(Component)]
pub struct CharacterSheet {
    pub attributes: Attributes,
    pub secondary: SecondaryBuys,
    /// Trait name → level (≥ 1).
    pub traits: BTreeMap<String, u8>,
    /// Skill name → points spent (≥ 1).
    pub skills: BTreeMap<String, i32>,
    /// Banked, unspent character points.
    pub unspent: i32,
}

impl CharacterSheet {
    /// Builds a sheet from a template (no validation; the registry checked
    /// templates at freeze).
    pub fn from_template(t: &SheetTemplate) -> Self {
        Self {
            attributes: t.attributes,
            secondary: t.secondary,
            traits: t.traits.iter().map(|&(n, l)| (n.to_owned(), l)).collect(),
            skills: t.skills.iter().map(|&(n, p)| (n.to_owned(), p)).collect(),
            unspent: t.unspent,
        }
    }

    /// Points spent in `skill` (0 if none).
    pub fn skill_points(&self, skill: &str) -> i32 {
        self.skills.get(skill).copied().unwrap_or(0)
    }

    /// Level of `name` (0 if the sheet does not have it).
    pub fn trait_level(&self, name: &str) -> u8 {
        self.traits.get(name).copied().unwrap_or(0)
    }

    /// The bought value of `stat`: the attribute level, or the secondary delta.
    pub fn bought(&self, stat: Stat) -> i32 {
        self.attributes
            .get(stat)
            .or_else(|| self.secondary.get(stat))
            .unwrap_or(0)
    }
}

fn len_u32(n: usize) -> u32 {
    u32::try_from(n).expect("character sheet too large")
}

impl SimHash for CharacterSheet {
    const NAME: &'static str = "character_sheet";
    fn sim_hash(&self, h: &mut Fnv64) {
        let a = &self.attributes;
        for v in [a.st, a.dx, a.iq, a.ht] {
            h.i32(v);
        }
        let s = &self.secondary;
        for v in [s.hp, s.will, s.per, s.fp, s.speed_q, s.basic_move] {
            h.i32(v);
        }
        h.u32(len_u32(self.traits.len()));
        for (name, &level) in &self.traits {
            h.str(name);
            h.u8(level);
        }
        h.u32(len_u32(self.skills.len()));
        for (name, &points) in &self.skills {
            h.str(name);
            h.i32(points);
        }
        h.i32(self.unspent);
    }
}

/// Where a skill level comes from.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum LevelSource {
    /// Bought with points.
    Points,
    /// A default: `from` is the stat or skill name, `modifier` its penalty.
    Default { from: String, modifier: i32 },
}

/// A resolved skill level.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SkillLevel {
    /// Effective level (what a roll targets before situational modifiers).
    pub level: i32,
    /// The controlling stat and its effective value.
    pub stat: Stat,
    pub stat_value: i32,
    /// `level - stat_value` (`"DX+1"`).
    pub relative: i32,
    /// Sum of trait modifiers to this skill (included in `level`).
    pub trait_bonus: i32,
    pub source: LevelSource,
}

impl SkillLevel {
    /// `"DX+1"`, `"IQ-2"`, `"HT"`.
    pub fn relative_label(&self) -> String {
        relative_label(self.stat, self.relative)
    }
}

/// `"DX+1"`, `"IQ-2"`, `"HT"`.
pub fn relative_label(stat: Stat, relative: i32) -> String {
    match relative {
        0 => stat.name().to_owned(),
        r if r > 0 => format!("{}+{r}", stat.name()),
        r => format!("{}{r}", stat.name()),
    }
}

/// Points spent, by category.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PointBreakdown {
    pub attributes: i32,
    pub secondary: i32,
    /// Advantages minus disadvantages.
    pub traits: i32,
    pub skills: i32,
    /// Sum of the four above.
    pub spent: i32,
    pub unspent: i32,
    /// `spent + unspent`: the character's total value.
    pub total: i32,
}

/// Something to spend banked points on ([`spend_points`](crate::spend_points)).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Purchase {
    /// Add `points` (≥ 1) to a registered skill.
    Skill { skill: String, points: i32 },
    /// Raise a stat by `levels` (≥ 1; quarters for `Speed`).
    Stat { stat: Stat, levels: i32 },
    /// Add `levels` (≥ 1) of a registered advantage (a trait with a positive
    /// point value; disadvantages are not bought with points).
    Trait { name: String, levels: u8 },
}

/// Why a purchase (or another rules call) was refused. Nothing changes.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SpendError {
    /// No such unit, or it has no `CharacterSheet`.
    NoSheet {
        unit: u32,
    },
    UnknownSkill {
        skill: String,
        known: String,
    },
    UnknownTrait {
        name: String,
        known: String,
    },
    UnknownTemplate {
        name: String,
        known: String,
    },
    /// Amount ≤ 0.
    InvalidAmount {
        amount: i32,
    },
    NotEnoughPoints {
        cost: i32,
        unspent: i32,
    },
    /// The result would exceed a [`Limits`](crate::Limits) value or the
    /// trait's `max_level`.
    AboveMax {
        what: String,
        value: i32,
        max: i32,
    },
    /// A disadvantage (non-positive point value) cannot be bought.
    NotPurchasable {
        name: String,
    },
    Overflow,
}

impl fmt::Display for SpendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SpendError::NoSheet { unit } => write!(f, "unit {unit} has no character sheet"),
            SpendError::UnknownSkill { skill, known } => {
                write!(f, "unknown skill `{skill}` (known: {known})")
            }
            SpendError::UnknownTrait { name, known } => {
                write!(f, "unknown trait `{name}` (known: {known})")
            }
            SpendError::UnknownTemplate { name, known } => {
                write!(f, "unknown sheet template `{name}` (known: {known})")
            }
            SpendError::InvalidAmount { amount } => write!(f, "amount {amount} must be > 0"),
            SpendError::NotEnoughPoints { cost, unspent } => {
                write!(f, "costs {cost} points, only {unspent} unspent")
            }
            SpendError::AboveMax { what, value, max } => {
                write!(f, "{what} would be {value}, above the maximum {max}")
            }
            SpendError::NotPurchasable { name } => {
                write!(f, "trait `{name}` is not bought with points")
            }
            SpendError::Overflow => write!(f, "point arithmetic overflow"),
        }
    }
}

impl std::error::Error for SpendError {}

/// One stat in a [`SheetView`].
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StatView {
    pub stat: Stat,
    pub name: String,
    /// Effective value (`Speed` in quarters).
    pub value: i32,
    /// Bought level / delta.
    pub bought: i32,
    /// Points it costs.
    pub points: i32,
}

/// One trait in a [`SheetView`].
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TraitView {
    pub name: String,
    pub level: u8,
    pub points: i32,
    /// False when the name is not registered (no modifiers, no cost).
    pub known: bool,
}

/// One bought skill in a [`SheetView`].
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SkillView {
    pub name: String,
    pub points: i32,
    /// `None` when the name is not registered.
    pub stat: Option<Stat>,
    pub difficulty: Option<Difficulty>,
    /// Effective level (`None` if unregistered).
    pub level: Option<i32>,
    pub relative: Option<i32>,
    /// `"DX+1"`.
    pub relative_label: Option<String>,
    pub source: Option<LevelSource>,
}

/// The whole sheet with every derived value: what inspect and BRP show
/// (`character_sheet` in the unit JSON). Stable field order.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SheetView {
    pub rule_set: String,
    pub stats: Vec<StatView>,
    /// Basic Speed as a decimal string (`"5.25"`).
    pub basic_speed: String,
    pub traits: Vec<TraitView>,
    pub skills: Vec<SkillView>,
    pub points: PointBreakdown,
}

/// Borrowed rules + registry: every pure computation on a sheet. The world
/// API (`effective_skill`, `roll_skill`, ...) wraps this; systems that hold
/// `Res<RuleSet>` and `Res<RulesRegistry>` can use it directly.
#[derive(Clone, Copy)]
pub struct Rules<'a> {
    pub set: &'a RuleSet,
    pub registry: &'a RulesRegistry,
}

impl<'a> Rules<'a> {
    pub fn new(set: &'a RuleSet, registry: &'a RulesRegistry) -> Self {
        Self { set, registry }
    }

    /// Sum of trait modifiers to `stat`.
    pub fn trait_stat_bonus(&self, sheet: &CharacterSheet, stat: Stat) -> i32 {
        self.trait_bonus(sheet, |m| match *m {
            TraitModifier::Stat { stat: s, per_level } if s == stat => per_level,
            _ => 0,
        })
    }

    /// Sum of trait modifiers to skill `skill`.
    pub fn trait_skill_bonus(&self, sheet: &CharacterSheet, skill: &str) -> i32 {
        self.trait_bonus(sheet, |m| match *m {
            TraitModifier::Skill {
                skill: s,
                per_level,
            } if s == skill => per_level,
            _ => 0,
        })
    }

    fn trait_bonus(
        &self,
        sheet: &CharacterSheet,
        per_level: impl Fn(&TraitModifier) -> i32,
    ) -> i32 {
        let mut total = 0i32;
        for (name, &level) in &sheet.traits {
            if let Some(def) = self.registry.trait_def(name) {
                for m in &def.modifiers {
                    total = total.saturating_add(per_level(m).saturating_mul(i32::from(level)));
                }
            }
        }
        total
    }

    /// The four attributes with trait modifiers applied.
    pub fn effective_attributes(&self, sheet: &CharacterSheet) -> Attributes {
        let a = &sheet.attributes;
        Attributes {
            st: a.st + self.trait_stat_bonus(sheet, Stat::St),
            dx: a.dx + self.trait_stat_bonus(sheet, Stat::Dx),
            iq: a.iq + self.trait_stat_bonus(sheet, Stat::Iq),
            ht: a.ht + self.trait_stat_bonus(sheet, Stat::Ht),
        }
    }

    /// Effective value of `stat` (`Speed` in quarters): attributes are bought
    /// level + trait modifiers; secondary values are derived from the
    /// effective attributes (`RuleSet::derive_secondary`, Move from the final
    /// Speed) + bought delta + trait modifiers.
    pub fn stat(&self, sheet: &CharacterSheet, stat: Stat) -> i32 {
        let attrs = self.effective_attributes(sheet);
        if let Some(v) = attrs.get(stat) {
            return v;
        }
        let base = (self.set.derive_secondary)(&attrs);
        let bonus = |s: Stat| sheet.bought(s) + self.trait_stat_bonus(sheet, s);
        match stat {
            Stat::Hp => base.hp + bonus(Stat::Hp),
            Stat::Will => base.will + bonus(Stat::Will),
            Stat::Per => base.per + bonus(Stat::Per),
            Stat::Fp => base.fp + bonus(Stat::Fp),
            Stat::Speed => base.speed_q + bonus(Stat::Speed),
            Stat::Move => {
                (self.set.move_from_speed)(base.speed_q + bonus(Stat::Speed)) + bonus(Stat::Move)
            }
            Stat::St | Stat::Dx | Stat::Iq | Stat::Ht => unreachable!("attributes handled above"),
        }
    }

    /// The level of `skill` on `sheet`, or `None` when the skill is not
    /// registered, or has no points and no usable default.
    ///
    /// Bought level: controlling stat + difficulty offset + progression
    /// bonus. Defaults: the best of `stat + modifier` and `other skill +
    /// modifier` (see [`DefaultChain`](crate::DefaultChain)). With points,
    /// the better of both when `best_of_bought_and_default`. Trait skill
    /// modifiers are added last.
    pub fn skill_level(&self, sheet: &CharacterSheet, skill: &str) -> Option<SkillLevel> {
        self.skill_level_depth(sheet, skill, self.set.defaults.max_depth, true)
    }

    /// Just the effective level.
    pub fn effective_skill(&self, sheet: &CharacterSheet, skill: &str) -> Option<i32> {
        self.skill_level(sheet, skill).map(|l| l.level)
    }

    fn skill_level_depth(
        &self,
        sheet: &CharacterSheet,
        skill: &str,
        depth: u8,
        allow_defaults: bool,
    ) -> Option<SkillLevel> {
        let def = self.registry.skill(skill)?;
        let stat_value = self.stat(sheet, def.stat);
        let points = sheet.skill_points(skill);
        let bought = self
            .set
            .relative_level(def.difficulty, points)
            .map(|rel| (stat_value + rel, LevelSource::Points));
        let best_default = if allow_defaults {
            self.best_default(sheet, &def.defaults, depth)
        } else {
            None
        };
        let (base, source) = match (bought, best_default) {
            (Some(b), Some(d)) if self.set.defaults.best_of_bought_and_default && d.0 > b.0 => d,
            (Some(b), _) => b,
            (None, Some(d)) => d,
            (None, None) => return None,
        };
        let trait_bonus = self.trait_skill_bonus(sheet, skill);
        let level = base + trait_bonus;
        Some(SkillLevel {
            level,
            stat: def.stat,
            stat_value,
            relative: level - stat_value,
            trait_bonus,
            source,
        })
    }

    /// Best default; ties keep the first listed.
    fn best_default(
        &self,
        sheet: &CharacterSheet,
        defaults: &[SkillDefault],
        depth: u8,
    ) -> Option<(i32, LevelSource)> {
        let mut best: Option<(i32, LevelSource)> = None;
        for d in defaults {
            let candidate = match *d {
                SkillDefault::Stat { stat, modifier } => Some((
                    self.stat(sheet, stat) + modifier,
                    LevelSource::Default {
                        from: stat.name().to_owned(),
                        modifier,
                    },
                )),
                SkillDefault::Skill { skill, modifier } => {
                    let other = if depth > 0 {
                        self.skill_level_depth(sheet, skill, depth - 1, true)
                    } else {
                        // Only the other skill's bought level counts.
                        self.skill_level_depth(sheet, skill, 0, false)
                    };
                    other.map(|l| {
                        (
                            l.level + modifier,
                            LevelSource::Default {
                                from: skill.to_owned(),
                                modifier,
                            },
                        )
                    })
                }
            };
            if let Some(c) = candidate
                && best.as_ref().is_none_or(|b| c.0 > b.0)
            {
                best = Some(c);
            }
        }
        best
    }

    /// Points spent per category and in total.
    pub fn points(&self, sheet: &CharacterSheet) -> PointBreakdown {
        let attributes = Stat::ATTRIBUTES
            .iter()
            .map(|&s| self.set.stat_cost(s, sheet.bought(s)))
            .sum();
        let secondary = Stat::ALL
            .iter()
            .filter(|s| !s.is_attribute())
            .map(|&s| self.set.stat_cost(s, sheet.bought(s)))
            .sum();
        let traits = sheet
            .traits
            .iter()
            .map(|(n, &l)| self.trait_points(n, l))
            .sum();
        let skills: i32 = sheet.skills.values().sum();
        let spent = attributes + secondary + traits + skills;
        PointBreakdown {
            attributes,
            secondary,
            traits,
            skills,
            spent,
            unspent: sheet.unspent,
            total: spent + sheet.unspent,
        }
    }

    fn trait_points(&self, name: &str, level: u8) -> i32 {
        self.registry
            .trait_def(name)
            .map_or(0, |d| d.points_per_level * i32::from(level))
    }

    /// What `purchase` would cost on `sheet` (validated, nothing changes).
    pub fn purchase_cost(
        &self,
        sheet: &CharacterSheet,
        purchase: &Purchase,
    ) -> Result<i32, SpendError> {
        let mut copy = sheet.clone();
        self.apply_purchase_unchecked_points(&mut copy, purchase)
    }

    /// Validates `purchase`, pays it from `sheet.unspent` and applies it.
    /// Returns the cost. On error `sheet` is unchanged.
    pub fn apply_purchase(
        &self,
        sheet: &mut CharacterSheet,
        purchase: &Purchase,
    ) -> Result<i32, SpendError> {
        let mut next = sheet.clone();
        let cost = self.apply_purchase_unchecked_points(&mut next, purchase)?;
        if cost > next.unspent {
            return Err(SpendError::NotEnoughPoints {
                cost,
                unspent: next.unspent,
            });
        }
        next.unspent -= cost;
        *sheet = next;
        Ok(cost)
    }

    /// Applies the purchase's effect (not the payment) and returns its cost.
    fn apply_purchase_unchecked_points(
        &self,
        sheet: &mut CharacterSheet,
        purchase: &Purchase,
    ) -> Result<i32, SpendError> {
        let limits = &self.set.limits;
        match purchase {
            Purchase::Skill { skill, points } => {
                if *points <= 0 {
                    return Err(SpendError::InvalidAmount { amount: *points });
                }
                let Some(def) = self.registry.skill(skill) else {
                    return Err(SpendError::UnknownSkill {
                        skill: skill.clone(),
                        known: self.registry.skill_names(),
                    });
                };
                let next = sheet
                    .skill_points(skill)
                    .checked_add(*points)
                    .ok_or(SpendError::Overflow)?;
                if next > limits.skill_points_max {
                    return Err(SpendError::AboveMax {
                        what: format!("points in `{skill}`"),
                        value: next,
                        max: limits.skill_points_max,
                    });
                }
                sheet.skills.insert(def.name.to_owned(), next);
                Ok(*points)
            }
            Purchase::Stat { stat, levels } => {
                if *levels <= 0 {
                    return Err(SpendError::InvalidAmount { amount: *levels });
                }
                let before = sheet.bought(*stat);
                let after = before.checked_add(*levels).ok_or(SpendError::Overflow)?;
                let max = if stat.is_attribute() {
                    limits.attribute_max
                } else {
                    limits.secondary_buy_max
                };
                if after > max {
                    return Err(SpendError::AboveMax {
                        what: stat.name().to_owned(),
                        value: after,
                        max,
                    });
                }
                let cost = self
                    .set
                    .stat_cost(*stat, after)
                    .checked_sub(self.set.stat_cost(*stat, before))
                    .ok_or(SpendError::Overflow)?;
                let slot = sheet
                    .attributes
                    .get_mut(*stat)
                    .or_else(|| sheet.secondary.get_mut(*stat))
                    .ok_or(SpendError::Overflow)?;
                *slot = after;
                Ok(cost)
            }
            Purchase::Trait { name, levels } => {
                if *levels == 0 {
                    return Err(SpendError::InvalidAmount { amount: 0 });
                }
                let Some(def) = self.registry.trait_def(name) else {
                    return Err(SpendError::UnknownTrait {
                        name: name.clone(),
                        known: self.registry.trait_names(),
                    });
                };
                if def.points_per_level <= 0 {
                    return Err(SpendError::NotPurchasable { name: name.clone() });
                }
                let after = i32::from(sheet.trait_level(name)) + i32::from(*levels);
                if after > i32::from(def.max_level) {
                    return Err(SpendError::AboveMax {
                        what: format!("level of `{name}`"),
                        value: after,
                        max: i32::from(def.max_level),
                    });
                }
                let level = u8::try_from(after).map_err(|_| SpendError::Overflow)?;
                sheet.traits.insert(def.name.to_owned(), level);
                Ok(def.points_per_level * i32::from(*levels))
            }
        }
    }

    /// A fresh sheet from template `name`.
    pub fn template_sheet(&self, name: &str) -> Option<CharacterSheet> {
        self.registry
            .template(name)
            .map(CharacterSheet::from_template)
    }

    /// Every derived value of `sheet`.
    pub fn view(&self, sheet: &CharacterSheet) -> SheetView {
        let stats = Stat::ALL
            .iter()
            .map(|&s| StatView {
                stat: s,
                name: s.name().to_owned(),
                value: self.stat(sheet, s),
                bought: sheet.bought(s),
                points: self.set.stat_cost(s, sheet.bought(s)),
            })
            .collect();
        let traits = sheet
            .traits
            .iter()
            .map(|(n, &l)| TraitView {
                name: n.clone(),
                level: l,
                points: self.trait_points(n, l),
                known: self.registry.trait_def(n).is_some(),
            })
            .collect();
        let skills = sheet
            .skills
            .iter()
            .map(|(n, &p)| {
                let def = self.registry.skill(n);
                let level = self.skill_level(sheet, n);
                SkillView {
                    name: n.clone(),
                    points: p,
                    stat: def.map(|d| d.stat),
                    difficulty: def.map(|d| d.difficulty),
                    level: level.as_ref().map(|l| l.level),
                    relative: level.as_ref().map(|l| l.relative),
                    relative_label: level.as_ref().map(SkillLevel::relative_label),
                    source: level.map(|l| l.source),
                }
            })
            .collect();
        SheetView {
            rule_set: self.set.name.to_owned(),
            stats,
            basic_speed: quarters_label(self.stat(sheet, Stat::Speed)),
            traits,
            skills,
            points: self.points(sheet),
        }
    }
}
