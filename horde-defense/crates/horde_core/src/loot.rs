//! Drops (design-loot-economy §3, rulings E5 and E11).
//!
//! Every attempt deployed from wave 2 on freezes its drop tier at deploy
//! ([`deploy_drop_tier`]), so `Attempt::drop_tier` None means "deployed
//! before v7": that attempt keeps today's drop path byte-identical (pinned by
//! `V6_DROP_TRACE_FNV`) and never records `drop_ids`. Only a root attempt on
//! the Some path records its drops. A Some-path drop rolls its quality
//! relative to the item's base quality ([`drop_quality`], decision 7); the
//! legacy path keeps the absolute roll.
use crate::content::content;
use crate::quality::{self, Quality};

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
