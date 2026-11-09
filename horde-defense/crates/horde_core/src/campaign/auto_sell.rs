//! Saved auto-sell schema (schema v7 §3.1 and §3.4): the per-campaign rule
//! and the per-report receipt of what settlement sold. Both are skipped at
//! their defaults, so a campaign that never enabled the rule and a report
//! that sold nothing save exactly as before.
use crate::content::ItemDefId;
use crate::kind_map::KindMap;
use crate::quality::Quality;
use serde::{Deserialize, Serialize};

/// The most receipt lines a report keeps. Totals stay exact past it; the
/// kept lines are the sales with the highest coins (ties go to the lower
/// item id).
pub const MAX_AUTO_SOLD_LINES: usize = 12;

/// Which of this run's drops settlement sells. Off by default; `slots`
/// follows `Slot::index` (Ranged, Melee, Outfit, Accessory). Drops of a
/// quality strictly below `sell_below` are sold, so `Poor` (which would sell
/// nothing) is rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AutoSellRule {
    pub enabled: bool,
    pub sell_below: Quality,
    pub slots: [bool; 4],
    pub keep_upgrades: bool,
    pub apply_to_npc_runs: bool,
}
impl Default for AutoSellRule {
    fn default() -> Self {
        Self {
            enabled: false,
            sell_below: Quality::Uncommon,
            slots: [true; 4],
            keep_upgrades: true,
            apply_to_npc_runs: true,
        }
    }
}
impl AutoSellRule {
    /// The serde skip predicate: the rule a campaign starts with.
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.sell_below == Quality::Poor {
            return Err("Auto-sell threshold must be above Poor".into());
        }
        Ok(())
    }
}

/// One auto-sold item as Results lists it. It names the definition, never
/// the item id: the item no longer exists.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutoSoldLine {
    pub definition: ItemDefId,
    pub level: u32,
    pub quality: Quality,
    pub coins: u64,
}

/// What settlement auto-sold from one run: exact totals, the per-quality
/// count, at most [`MAX_AUTO_SOLD_LINES`] lines, and the drops kept because
/// selling them would overflow the wallet.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AutoSoldReceipt {
    pub count: u32,
    pub coins: u64,
    pub by_quality: KindMap<Quality, u32>,
    pub lines: Vec<AutoSoldLine>,
    pub kept_on_overflow: u32,
}
impl AutoSoldReceipt {
    /// Nothing was sold or kept; the serde skip predicate.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
    /// Adds a later settlement sale of the same run (the expedition's final
    /// sweep): totals stay exact and the [`MAX_AUTO_SOLD_LINES`] lines with
    /// the highest coins are kept, this receipt's first on a tie.
    pub fn absorb(&mut self, other: AutoSoldReceipt) {
        self.count = self.count.saturating_add(other.count);
        self.coins = self.coins.saturating_add(other.coins);
        for quality in Quality::ALL {
            self.by_quality[quality] =
                self.by_quality[quality].saturating_add(other.by_quality[quality]);
        }
        self.kept_on_overflow = self.kept_on_overflow.saturating_add(other.kept_on_overflow);
        self.lines.extend(other.lines);
        self.lines.sort_by_key(|line| std::cmp::Reverse(line.coins));
        self.lines.truncate(MAX_AUTO_SOLD_LINES);
    }
    /// Bounds against the run's `loot` (schema v7 §3.4).
    pub fn validate(&self, loot: u32) -> Result<(), String> {
        let quality_sum: u64 = self.by_quality.values().map(|n| u64::from(*n)).sum();
        let line_coins = self
            .lines
            .iter()
            .try_fold(0u64, |total, line| total.checked_add(line.coins));
        if u64::from(self.count) + u64::from(self.kept_on_overflow) > u64::from(loot)
            || self.lines.len() > MAX_AUTO_SOLD_LINES.min(self.count as usize)
            || quality_sum != u64::from(self.count)
            || line_coins.is_none_or(|coins| coins > self.coins)
            || (self.count == 0 && self.coins != 0)
            || self
                .lines
                .iter()
                .any(|line| line.definition.index().is_none())
        {
            return Err("Invalid auto-sold receipt".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(coins: u64) -> AutoSoldLine {
        AutoSoldLine {
            definition: ItemDefId::at(5),
            level: 1,
            quality: Quality::Poor,
            coins,
        }
    }

    /// A valid receipt of `count` Poor sales of 2 coins each, all listed up
    /// to the line cap.
    fn receipt(count: u32) -> AutoSoldReceipt {
        let mut by_quality = KindMap::default();
        by_quality[Quality::Poor] = count;
        AutoSoldReceipt {
            count,
            coins: 2 * u64::from(count),
            by_quality,
            lines: (0..count.min(MAX_AUTO_SOLD_LINES as u32))
                .map(|_| line(2))
                .collect(),
            kept_on_overflow: 0,
        }
    }

    #[test]
    fn receipt_validation_rejects_13_lines_and_bad_quality_sum() {
        assert!(AutoSoldReceipt::default().validate(0).is_ok());
        let full = receipt(20);
        assert_eq!(full.lines.len(), MAX_AUTO_SOLD_LINES);
        assert!(full.validate(20).is_ok());
        let mut thirteen = full.clone();
        thirteen.lines.push(line(2));
        assert!(thirteen.validate(20).is_err(), "13 lines");
        // Fewer lines than sales is fine, more lines than sales is not.
        let mut short = receipt(3);
        short.lines.pop();
        assert!(short.validate(3).is_ok());
        let mut extra = receipt(3);
        extra.lines.push(line(0));
        assert!(extra.validate(3).is_err(), "more lines than sales");
        let mut bad_sum = receipt(3);
        bad_sum.by_quality[Quality::Common] = 1;
        assert!(bad_sum.validate(4).is_err(), "quality sum above count");
        bad_sum.by_quality[Quality::Common] = 0;
        bad_sum.by_quality[Quality::Poor] = 2;
        assert!(bad_sum.validate(3).is_err(), "quality sum below count");
        assert!(receipt(3).validate(2).is_err(), "count above loot");
        let mut kept = receipt(3);
        kept.kept_on_overflow = 1;
        assert!(kept.validate(4).is_ok());
        assert!(kept.validate(3).is_err(), "count plus kept above loot");
        let mut coins = receipt(3);
        coins.coins -= 1;
        assert!(coins.validate(3).is_err(), "line coins above the total");
        let mut overflow = receipt(2);
        overflow.lines[0].coins = u64::MAX;
        overflow.coins = u64::MAX;
        assert!(overflow.validate(2).is_err(), "line coins overflow");
        let paid_for_nothing = AutoSoldReceipt {
            coins: 1,
            ..AutoSoldReceipt::default()
        };
        assert!(paid_for_nothing.validate(5).is_err());
        let mut unknown = receipt(1);
        unknown.lines[0].definition = ItemDefId("no_such_item".into());
        assert!(unknown.validate(1).is_err());
    }

    #[test]
    fn absorb_keeps_exact_totals_and_the_dearest_lines() {
        let mut first = receipt(10);
        first.kept_on_overflow = 1;
        let mut swept = receipt(5);
        swept.by_quality[Quality::Poor] = 4;
        swept.by_quality[Quality::Common] = 1;
        swept.lines[0].coins = 9;
        swept.coins += 7;
        first.absorb(swept);
        assert_eq!((first.count, first.coins), (15, 37));
        assert_eq!(first.by_quality[Quality::Poor], 14);
        assert_eq!(first.by_quality[Quality::Common], 1);
        assert_eq!(first.kept_on_overflow, 1);
        assert_eq!(first.lines.len(), MAX_AUTO_SOLD_LINES);
        assert_eq!(first.lines[0].coins, 9, "the dearest line leads");
        assert!(first.validate(16).is_ok());
        let mut empty = AutoSoldReceipt::default();
        empty.absorb(receipt(2));
        assert_eq!(empty, receipt(2));
    }

    #[test]
    fn rule_rejects_a_poor_threshold_and_serializes_every_field() {
        assert!(AutoSellRule::default().validate().is_ok());
        for quality in Quality::ALL {
            let rule = AutoSellRule {
                sell_below: quality,
                ..AutoSellRule::default()
            };
            assert_eq!(rule.validate().is_ok(), quality != Quality::Poor);
        }
        let rule = AutoSellRule {
            enabled: true,
            ..AutoSellRule::default()
        };
        assert!(!rule.is_default());
        let text = serde_json::to_string(&rule).unwrap();
        assert_eq!(
            text,
            r#"{"enabled":true,"sell_below":"Uncommon","slots":[true,true,true,true],"keep_upgrades":true,"apply_to_npc_runs":true}"#
        );
        assert_eq!(serde_json::from_str::<AutoSellRule>(&text).unwrap(), rule);
        // Missing keys take the defaults (container default).
        let partial: AutoSellRule = serde_json::from_str(r#"{"enabled":true}"#).unwrap();
        assert_eq!(partial, rule);
    }
}
