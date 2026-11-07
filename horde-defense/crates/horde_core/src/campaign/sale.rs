//! One sale-eligibility rule (LE-1, W1-19). [`Campaign::sale_block`] is the
//! only place that decides whether an owned item may be sold; the quote
//! ([`Campaign::sale_eligibility_for`]), the batch plan
//! ([`Campaign::plan_batch`]) and the batch sale ([`Campaign::execute_plan`])
//! all read it, and `Game::sale_eligibility` is the public facade. Since
//! ruleset 2 (W1-20) `Command::SellItem` is strict: it sells exactly when the
//! rule allows and otherwise answers `CommandError::SaleBlocked(block)`; no
//! sale ever rewrites a loadout.

use super::*;
use crate::command::CommandError;

/// Who is selling. Pure reads never consume RNG either way.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaleOrigin {
    /// A player command or preview: nothing sells while a defender is deployed.
    Player,
    /// A settlement drain (wave-2 auto-sell): skips only the in-battle guard.
    Settlement,
}

/// Why an item cannot be sold. [`Campaign::sale_block`] checks the guards in
/// declaration order and reports the first match.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SaleBlock {
    /// The id is not an owned item.
    Unknown,
    /// A defender is deployed (player origin only).
    InBattle,
    /// Held by a live expedition or a country guard (`reserved_items`).
    Reserved,
    /// Protected starter equipment.
    Protected,
    Locked,
    Favorite,
    /// NPC-issue gear (a legacy unprotected copy).
    NpcOnly,
    /// In the player's loadout.
    EquippedPersonal,
    /// In Mara's loadout.
    EquippedMara,
    /// Part of the named saved presets, in preset order.
    Preset {
        names: Vec<String>,
    },
    /// In an idle recruit's loadout.
    RecruitEquipped {
        name: String,
    },
}
impl SaleBlock {
    /// Plain player-facing text.
    pub fn label(&self) -> String {
        match self {
            Self::Unknown => "Item is no longer owned".into(),
            Self::InBattle => "Return to camp before changing equipment or progression".into(),
            Self::Reserved => "This item is deployed or assigned to a country guard".into(),
            Self::Protected => "Protected starter equipment cannot be sold".into(),
            Self::Locked => "Unlock this item before selling it".into(),
            Self::Favorite => "Remove the favorite mark before selling this item".into(),
            Self::NpcOnly => "NPC issued gear cannot be sold".into(),
            Self::EquippedPersonal => "Equipped by you".into(),
            Self::EquippedMara => "Equipped by Mara".into(),
            Self::Preset { names } => {
                let noun = if names.len() == 1 {
                    "preset"
                } else {
                    "presets"
                };
                format!("Used by the {noun} {}", names.join(", "))
            }
            Self::RecruitEquipped { name } => format!("Equipped by {name}"),
        }
    }
}

/// The shortcut a blocked sale offers. Wired to [`SaleBlock`] with the armory
/// resolution buttons (wave 3); W1-20 shows only `SaleBlock::label()`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaleResolution {
    Unlock,
    Unfavourite,
    EquipStarter { mode: Mode, slot: Slot },
    OpenPreset { index: usize },
    OpenRecruit { recruit: u64 },
    None,
}

/// What a sale would pay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SaleQuote {
    pub coins: u64,
}

/// Item references the campaign cannot see on its own: the idle recruits'
/// loadouts, built once per sale by `StrategyState::sale_refs`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaleRefs {
    recruit_equipped: BTreeMap<u64, String>,
}
impl SaleRefs {
    /// `recruit_equipped` maps an item id to the recruit holding it.
    pub(crate) fn new(recruit_equipped: BTreeMap<u64, String>) -> Self {
        Self { recruit_equipped }
    }
    /// No recruit references: campaign-only tests.
    #[cfg(test)]
    pub(crate) fn empty() -> Self {
        Self::new(BTreeMap::new())
    }
}

/// The items a batch sells and what they pay together. The quote and the
/// sale share one plan, so the quote always equals the coins realised.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct SalePlan {
    pub ids: Vec<u64>,
    pub coins: u64,
}

impl Campaign {
    /// The single sale rule: the first guard `id` fails, or None when it may
    /// be sold. Pure: no mutation and no RNG.
    pub(crate) fn sale_block(
        &self,
        id: u64,
        refs: &SaleRefs,
        origin: SaleOrigin,
    ) -> Option<SaleBlock> {
        let Some(item) = self.item(id) else {
            return Some(SaleBlock::Unknown);
        };
        if origin == SaleOrigin::Player && self.reserved.is_some() {
            return Some(SaleBlock::InBattle);
        }
        if self.reserved_items.contains(&id) {
            return Some(SaleBlock::Reserved);
        }
        if item.protected {
            return Some(SaleBlock::Protected);
        }
        if item.locked {
            return Some(SaleBlock::Locked);
        }
        if item.favorite {
            return Some(SaleBlock::Favorite);
        }
        if item.definition().npc_only {
            return Some(SaleBlock::NpcOnly);
        }
        if self.player.loadout.contains(&id) {
            return Some(SaleBlock::EquippedPersonal);
        }
        if self.npc.loadout.contains(&id) {
            return Some(SaleBlock::EquippedMara);
        }
        let names: Vec<String> = self
            .presets
            .iter()
            .filter(|p| p.loadout.contains(&id))
            .map(|p| p.name.clone())
            .collect();
        if !names.is_empty() {
            return Some(SaleBlock::Preset { names });
        }
        refs.recruit_equipped
            .get(&id)
            .map(|name| SaleBlock::RecruitEquipped { name: name.clone() })
    }
    /// The quote for one item, or why it cannot be sold.
    pub(crate) fn sale_eligibility_for(
        &self,
        id: u64,
        origin: SaleOrigin,
        refs: &SaleRefs,
    ) -> Result<SaleQuote, SaleBlock> {
        match self.sale_block(id, refs, origin) {
            Some(block) => Err(block),
            None => Ok(SaleQuote {
                coins: self.item_sale_value(id).expect("eligible items are owned"),
            }),
        }
    }
    /// The eligible subset of `ids`, in order, and their summed quote.
    pub(crate) fn plan_batch(
        &self,
        refs: &SaleRefs,
        origin: SaleOrigin,
        ids: impl IntoIterator<Item = u64>,
    ) -> SalePlan {
        let mut plan = SalePlan::default();
        for id in ids {
            if let Ok(quote) = self.sale_eligibility_for(id, origin, refs) {
                plan.ids.push(id);
                plan.coins = plan.coins.saturating_add(quote.coins);
            }
        }
        plan
    }
    /// Every owned item id, in inventory order.
    pub(crate) fn owned_item_ids(&self) -> Vec<u64> {
        self.items.iter().map(|i| i.id).collect()
    }
    /// Sells a whole plan or nothing: the wallet limit rejects the batch.
    pub(crate) fn execute_plan(&mut self, plan: &SalePlan) -> Result<u64, CommandError> {
        let wallet = self
            .coins
            .checked_add(plan.coins)
            .ok_or(CommandError::WalletLimit)?;
        let sold: BTreeSet<u64> = plan.ids.iter().copied().collect();
        self.items.retain(|x| !sold.contains(&x.id));
        self.coins = wallet;
        Ok(plan.coins)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settlement_origin_skips_only_the_in_battle_guard() {
        let mut c = Campaign::default();
        let extra = c.add_item(5).unwrap();
        let favorite = c.add_item(6).unwrap();
        c.set_favorite(favorite, true).unwrap();
        let axe = c.add_item(5).unwrap();
        c.equip(Mode::Personal, axe).unwrap();
        c.reserve(Mode::Personal).unwrap();
        let refs = SaleRefs::empty();
        for origin in [SaleOrigin::Player, SaleOrigin::Settlement] {
            assert_eq!(c.sale_block(999, &refs, origin), Some(SaleBlock::Unknown));
        }
        assert_eq!(
            c.sale_block(extra, &refs, SaleOrigin::Player),
            Some(SaleBlock::InBattle)
        );
        assert_eq!(c.sale_block(extra, &refs, SaleOrigin::Settlement), None);
        assert_eq!(
            c.sale_block(favorite, &refs, SaleOrigin::Settlement),
            Some(SaleBlock::Favorite)
        );
        // The main run holds its loadout through `reserved`, not
        // `reserved_items`, so a settlement still meets the equipped guard.
        assert_eq!(
            c.sale_block(axe, &refs, SaleOrigin::Settlement),
            Some(SaleBlock::EquippedPersonal)
        );
        let plan = c.plan_batch(&refs, SaleOrigin::Settlement, c.owned_item_ids());
        assert_eq!(plan.ids, vec![extra]);
        assert_eq!(plan.coins, c.item_sale_value(extra).unwrap());
        assert_eq!(c.sell_quote(&refs), 0);
    }

    #[test]
    fn preset_block_label_picks_the_noun_by_count() {
        let one = SaleBlock::Preset {
            names: vec!["A".into()],
        };
        assert_eq!(one.label(), "Used by the preset A");
        let two = SaleBlock::Preset {
            names: vec!["A".into(), "B".into()],
        };
        assert_eq!(two.label(), "Used by the presets A, B");
    }

    #[test]
    fn batch_wallet_overflow_sells_nothing() {
        let mut c = Campaign {
            coins: u64::MAX - 1,
            ..Campaign::default()
        };
        c.add_item(5).unwrap();
        let refs = SaleRefs::empty();
        let plan = c.plan_batch(&refs, SaleOrigin::Player, c.owned_item_ids());
        assert_eq!(plan.ids.len(), 1);
        let before = c.clone();
        assert_eq!(c.execute_plan(&plan), Err(CommandError::WalletLimit));
        assert_eq!(c, before);
    }
}
