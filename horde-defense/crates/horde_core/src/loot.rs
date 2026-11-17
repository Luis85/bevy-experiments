//! Drops (design-loot-economy §3, rulings E5 and E11).
//!
//! Every attempt deployed from wave 2 on freezes its drop tier at deploy
//! ([`deploy_drop_tier`]), so `Attempt::drop_tier` None means "deployed
//! before v7": that attempt keeps today's drop path byte-identical (pinned by
//! `V6_DROP_TRACE_FNV`) and never records `drop_ids`. Only a root attempt on
//! the Some path records its drops. A Some-path drop rolls its quality
//! relative to the item's base quality ([`drop_quality`], decision 7); the
//! legacy path keeps the absolute roll. On the Some path an elite always
//! drops (ruling E8): an elite-only drop picks with the keyed ELITE_PICK
//! hash instead of the combat RNG, and its quality is never below
//! [`ELITE_QUALITY_FLOOR`]. A Some-path pick is weighted ([`DropTable`]):
//! the attempt's own drop tier weighs most and a trained weapon class
//! three times as much, still in one draw.
use crate::content::{Mode, Slot, content};
use crate::quality::{self, Quality};
use crate::sim::Attempt;
use crate::weapons::{WeaponClass, WeaponSpec};
use std::collections::BTreeSet;

/// The highest tier the drop pool holds (4 with today's catalogue): a saved
/// `Attempt::drop_tier` lies in `1..=max_drop_tier()`.
pub fn max_drop_tier() -> u8 {
    content().max_drop_tier()
}

/// The drop tier an attempt freezes at deploy: one above its encounter
/// template (`level`, which wraps every five country stages) or the
/// campaign's unlocked stage count, whichever is higher, clamped to
/// `1..=max_drop_tier()`. Frozen, so a worker's `first_clear` mid-run never
/// changes it.
pub fn deploy_drop_tier(level: usize, unlocked: usize) -> u8 {
    let tier = level.saturating_add(1).max(unlocked);
    u8::try_from(tier)
        .unwrap_or(u8::MAX)
        .clamp(1, max_drop_tier().max(1))
}

/// The quality floor of an elite's drop (ruling E8): tiers whose outcome
/// lies below it weigh 0. Bosses get no floor.
pub const ELITE_QUALITY_FLOOR: Quality = Quality::Uncommon;

/// The quality of a Some-path drop: the keyed relative roll
/// ([`quality::QualityCatalog::roll_relative`]) on the item's `base`, with
/// [`ELITE_QUALITY_FLOOR`] for an elite. It reads no combat RNG.
pub fn drop_quality(
    base: Quality,
    seed: u64,
    attempt: u64,
    enemy: u64,
    rank: u32,
    elite: bool,
) -> Quality {
    quality::catalog().roll_relative(
        base,
        elite.then_some(ELITE_QUALITY_FLOOR),
        seed,
        attempt,
        enemy,
        rank,
    )
}

/// The weight of an item `drop_tier - item.tier` steps below the attempt's
/// drop tier (index 0 is the drop tier itself); tiers above it weigh 0.
/// A step past the table takes its last entry.
pub const DROP_TIER_WEIGHTS: [u64; 4] = [6, 3, 1, 1];
/// The extra weight of a Ranged item whose weapon class the defender is
/// trained in (Personal: the skill classes; NPC: its own weapon's class).
pub const TRAINED_CLASS_WEIGHT: u64 = 3;
/// Per-slot weights in [`Slot::index`] order: a wave-4 tuning lever, neutral
/// now.
pub const SLOT_WEIGHTS: [u64; 4] = [1, 1, 1, 1];

/// The weighted single-draw drop table of an attempt on the `drop_tier`
/// Some path (design-loot-economy §3, plan LE-5): the pool is
/// [`crate::content::Content::drop_pool`] at the frozen drop tier, each
/// entry weighing `DROP_TIER_WEIGHTS[drop_tier - tier] * SLOT_WEIGHTS[slot]`
/// times [`TRAINED_CLASS_WEIGHT`] for a trained Ranged class, walked
/// cumulatively in catalogue index order. A normal drop picks with exactly
/// one `a.rng.below(total)` (one draw, as the uniform pick took); an
/// elite-only drop picks with the keyed ELITE_PICK hash modulo the total.
/// The legacy `drop_tier` None path never reads it (ruling E11).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DropTable {
    /// `(catalogue index, cumulative weight through this entry)`, ascending
    /// in both; zero-weight entries are left out.
    cumulative: Vec<(usize, u64)>,
    total: u64,
}

impl DropTable {
    /// The table of `a`'s frozen drop tier and profile. The trained classes
    /// are the Personal skill classes, or the NPC weapon's own class. An
    /// attempt without a drop tier (deployed before v7, which keeps the
    /// legacy drop) is read at its template level + 1.
    pub fn for_attempt(a: &Attempt) -> Self {
        let drop_tier = a.drop_tier.unwrap_or_else(|| deploy_drop_tier(a.level, 0));
        let trained = match a.mode {
            Mode::Personal => a.profile.modifiers.weapon_classes.clone(),
            Mode::Npc => BTreeSet::from([a.profile.weapon.class]),
        };
        Self::new(usize::from(drop_tier), &trained)
    }

    /// The table at `drop_tier` with the `trained` weapon classes.
    pub fn new(drop_tier: usize, trained: &BTreeSet<WeaponClass>) -> Self {
        let items = &content().items;
        let mut total = 0u64;
        let mut cumulative = Vec::new();
        for definition in content().drop_pool(drop_tier) {
            let item = &items[definition];
            let step = drop_tier.saturating_sub(item.tier);
            let tier = DROP_TIER_WEIGHTS
                .get(step)
                .or(DROP_TIER_WEIGHTS.last())
                .copied()
                .unwrap_or(0);
            let slot = SLOT_WEIGHTS[item.slot.index()];
            let class = if item.slot == Slot::Ranged
                && trained.contains(&WeaponSpec::from_item(item).class)
            {
                TRAINED_CLASS_WEIGHT
            } else {
                1
            };
            let weight = tier.saturating_mul(slot).saturating_mul(class);
            if weight > 0 {
                total = total.saturating_add(weight);
                cumulative.push((definition, total));
            }
        }
        Self { cumulative, total }
    }

    /// The sum of every weight; 0 means nothing can drop.
    pub fn total(&self) -> u64 {
        self.total
    }

    /// The catalogue index `roll` (in `0..total()`) lands on: the first
    /// entry whose cumulative weight exceeds it. Panics on an empty table
    /// or a roll at or above the total.
    pub fn pick(&self, roll: u64) -> usize {
        assert!(
            roll < self.total,
            "drop roll {roll} >= total {}",
            self.total
        );
        let at = self.cumulative.partition_point(|&(_, end)| end <= roll);
        self.cumulative[at].0
    }

    /// The weight of catalogue entry `definition` (0 outside the pool).
    pub fn weight(&self, definition: usize) -> u64 {
        let at = self.cumulative.partition_point(|&(n, _)| n < definition);
        match self.cumulative.get(at) {
            Some(&(n, end)) if n == definition => {
                end - at.checked_sub(1).map_or(0, |prev| self.cumulative[prev].1)
            }
            _ => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deploy_drop_tier_takes_the_higher_of_template_and_unlocked() {
        assert_eq!(max_drop_tier(), 4);
        assert_eq!(deploy_drop_tier(0, 1), 1);
        assert_eq!(deploy_drop_tier(0, 0), 1, "never below 1");
        assert_eq!(deploy_drop_tier(1, 1), 2);
        assert_eq!(deploy_drop_tier(0, 3), 3, "a wrapped late stage");
        assert_eq!(deploy_drop_tier(4, 5), 4, "clamped to the pool");
        assert_eq!(deploy_drop_tier(usize::MAX, 0), 4);
    }
}
