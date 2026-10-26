//! Stats (attributes and secondary characteristics) and skill difficulty.

use bevy_reflect::Reflect;
use serde::{Deserialize, Serialize};

/// Every value a sheet can be rolled against besides skills.
///
/// The four attributes (`St`, `Dx`, `Iq`, `Ht`) are bought directly; the
/// secondary characteristics start from values the [`RuleSet`](crate::RuleSet)
/// derives from the attributes and can be bought up or down from there.
/// `Speed` is Basic Speed in **quarters** (fixed point: `21` = 5.25);
/// `Move` is Basic Move (whole units).
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Reflect,
)]
pub enum Stat {
    St,
    Dx,
    Iq,
    Ht,
    Hp,
    Will,
    Per,
    Fp,
    Speed,
    Move,
}

impl Stat {
    /// Every stat in declaration (and display) order.
    pub const ALL: [Stat; 10] = [
        Stat::St,
        Stat::Dx,
        Stat::Iq,
        Stat::Ht,
        Stat::Hp,
        Stat::Will,
        Stat::Per,
        Stat::Fp,
        Stat::Speed,
        Stat::Move,
    ];

    /// The four bought attributes.
    pub const ATTRIBUTES: [Stat; 4] = [Stat::St, Stat::Dx, Stat::Iq, Stat::Ht];

    /// Short display name, also the journal `Roll.skill` of a stat roll.
    pub fn name(self) -> &'static str {
        match self {
            Stat::St => "ST",
            Stat::Dx => "DX",
            Stat::Iq => "IQ",
            Stat::Ht => "HT",
            Stat::Hp => "HP",
            Stat::Will => "Will",
            Stat::Per => "Per",
            Stat::Fp => "FP",
            Stat::Speed => "Speed",
            Stat::Move => "Move",
        }
    }

    /// The stat named `name` (case-insensitive, as in [`Stat::name`]).
    pub fn parse(name: &str) -> Option<Stat> {
        Stat::ALL
            .into_iter()
            .find(|s| s.name().eq_ignore_ascii_case(name))
    }

    /// True for `St`, `Dx`, `Iq`, `Ht`.
    pub fn is_attribute(self) -> bool {
        matches!(self, Stat::St | Stat::Dx | Stat::Iq | Stat::Ht)
    }

    /// `u8` hash discriminant (declaration order).
    pub fn index(self) -> u8 {
        match self {
            Stat::St => 0,
            Stat::Dx => 1,
            Stat::Iq => 2,
            Stat::Ht => 3,
            Stat::Hp => 4,
            Stat::Will => 5,
            Stat::Per => 6,
            Stat::Fp => 7,
            Stat::Speed => 8,
            Stat::Move => 9,
        }
    }
}

/// Bought attribute levels (absolute values; the rule set's base, 10 by
/// default, costs nothing).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
pub struct Attributes {
    pub st: i32,
    pub dx: i32,
    pub iq: i32,
    pub ht: i32,
}

impl Default for Attributes {
    fn default() -> Self {
        Self::splat(10)
    }
}

impl Attributes {
    pub const fn new(st: i32, dx: i32, iq: i32, ht: i32) -> Self {
        Self { st, dx, iq, ht }
    }

    pub const fn splat(v: i32) -> Self {
        Self::new(v, v, v, v)
    }

    /// The value of attribute `stat`; `None` for a secondary stat.
    pub fn get(&self, stat: Stat) -> Option<i32> {
        match stat {
            Stat::St => Some(self.st),
            Stat::Dx => Some(self.dx),
            Stat::Iq => Some(self.iq),
            Stat::Ht => Some(self.ht),
            _ => None,
        }
    }

    /// Mutable attribute `stat`; `None` for a secondary stat.
    pub fn get_mut(&mut self, stat: Stat) -> Option<&mut i32> {
        match stat {
            Stat::St => Some(&mut self.st),
            Stat::Dx => Some(&mut self.dx),
            Stat::Iq => Some(&mut self.iq),
            Stat::Ht => Some(&mut self.ht),
            _ => None,
        }
    }
}

/// Bought deltas of the secondary characteristics, on top of the values the
/// rule set derives (`speed_q` in quarters).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
pub struct SecondaryBuys {
    pub hp: i32,
    pub will: i32,
    pub per: i32,
    pub fp: i32,
    pub speed_q: i32,
    pub basic_move: i32,
}

impl SecondaryBuys {
    /// The bought delta of secondary `stat`; `None` for an attribute.
    pub fn get(&self, stat: Stat) -> Option<i32> {
        match stat {
            Stat::Hp => Some(self.hp),
            Stat::Will => Some(self.will),
            Stat::Per => Some(self.per),
            Stat::Fp => Some(self.fp),
            Stat::Speed => Some(self.speed_q),
            Stat::Move => Some(self.basic_move),
            _ => None,
        }
    }

    /// Mutable delta of secondary `stat`; `None` for an attribute.
    pub fn get_mut(&mut self, stat: Stat) -> Option<&mut i32> {
        match stat {
            Stat::Hp => Some(&mut self.hp),
            Stat::Will => Some(&mut self.will),
            Stat::Per => Some(&mut self.per),
            Stat::Fp => Some(&mut self.fp),
            Stat::Speed => Some(&mut self.speed_q),
            Stat::Move => Some(&mut self.basic_move),
            _ => None,
        }
    }
}

/// The secondary values a rule set derives from the effective attributes
/// (before buys and trait modifiers). Basic Move is derived from the final
/// Basic Speed by [`RuleSet::move_from_speed`](crate::RuleSet::move_from_speed).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SecondaryBase {
    pub hp: i32,
    pub will: i32,
    pub per: i32,
    pub fp: i32,
    pub speed_q: i32,
}

/// How hard a skill is to learn: shifts the relative level a given number of
/// points buys ([`DifficultyOffsets`](crate::DifficultyOffsets)).
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Reflect,
)]
pub enum Difficulty {
    Easy,
    Average,
    Hard,
    VeryHard,
}

impl Difficulty {
    pub fn name(self) -> &'static str {
        match self {
            Difficulty::Easy => "Easy",
            Difficulty::Average => "Average",
            Difficulty::Hard => "Hard",
            Difficulty::VeryHard => "VeryHard",
        }
    }
}

/// `"5.25"` for 21 quarters (integer formatting, no floats).
pub fn quarters_label(q: i32) -> String {
    let whole = q.div_euclid(4);
    let frac = q.rem_euclid(4) * 25;
    format!("{whole}.{frac:02}")
}
