//! Pure armory view model: the campaign facts the Equipment screen renders.
use horde_core::{
    campaign::{OfferPreview, RecommendationAction},
    content::{Mode, Slot, content},
    sim::Game,
    weapons::{WeaponClass, WeaponSpec},
};

/// How a ranged offer relates to the selected defender's weapon training.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum OfferTraining {
    NpcIssued,
    Trained,
    TrainingRequired,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct ArmoryOffer {
    pub preview: OfferPreview,
    /// Ranged offers only.
    pub weapon: Option<(WeaponClass, OfferTraining)>,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct ArmoryView {
    pub rank: u32,
    pub shop_level: u32,
    pub next_gear_rank: Option<u32>,
    /// The focused `BuyEquipment` recommendation, when it names a catalogue item.
    pub recommended: Option<OfferPreview>,
    /// The focused `Equip` recommendation, when that item is still owned.
    pub equip_focus: Option<u64>,
    pub offers: Vec<ArmoryOffer>,
}

pub(super) fn view_model(
    game: &Game,
    mode: Mode,
    focus: Option<&RecommendationAction>,
) -> ArmoryView {
    let campaign = &game.campaign;
    let recommended = match focus {
        Some(RecommendationAction::BuyEquipment(index)) => campaign.shop_offer(*index).ok(),
        _ => None,
    };
    let equip_focus = match focus {
        Some(RecommendationAction::Equip(id)) if campaign.item(*id).is_some() => Some(*id),
        _ => None,
    };
    let offers = campaign
        .shop_offers()
        .into_iter()
        .map(|preview| {
            let authored = &content().items[preview.definition];
            let weapon = (authored.slot == Slot::Ranged).then(|| {
                let training = if authored.npc_only {
                    OfferTraining::NpcIssued
                } else if campaign.can_use_weapon(mode, preview.definition) {
                    OfferTraining::Trained
                } else {
                    OfferTraining::TrainingRequired
                };
                (WeaponSpec::from_item(authored).class, training)
            });
            ArmoryOffer { preview, weapon }
        })
        .collect();
    ArmoryView {
        rank: campaign.progression_rank(),
        shop_level: campaign.shop_item_level(),
        next_gear_rank: campaign.next_gear_level_rank(),
        recommended,
        equip_focus,
        offers,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game() -> Game {
        let mut game = Game::default();
        game.campaign.unlocked = 4;
        game.campaign.coins = 500;
        game
    }

    #[test]
    fn view_model_reports_gear_level_and_quartermaster_offers() {
        let game = game();
        let view = view_model(&game, Mode::Personal, None);
        assert_eq!(view.rank, game.campaign.progression_rank());
        assert_eq!(view.shop_level, game.campaign.shop_item_level());
        assert_eq!(view.next_gear_rank, game.campaign.next_gear_level_rank());
        assert_eq!(view.recommended, None);
        assert_eq!(view.equip_focus, None);
        assert_eq!(view.offers.len(), game.campaign.shop_offers().len());
        for offer in &view.offers {
            let authored = &content().items[offer.preview.definition];
            assert_eq!(offer.weapon.is_some(), authored.slot == Slot::Ranged);
            if let Some((_, training)) = offer.weapon {
                assert_eq!(
                    training == OfferTraining::Trained,
                    !authored.npc_only
                        && game
                            .campaign
                            .can_use_weapon(Mode::Personal, offer.preview.definition)
                );
            }
        }
    }

    #[test]
    fn view_model_resolves_buy_and_equip_focus() {
        let game = game();
        let index = game.campaign.shop_offers()[0].definition;
        let view = view_model(
            &game,
            Mode::Personal,
            Some(&RecommendationAction::BuyEquipment(index)),
        );
        assert_eq!(view.recommended, game.campaign.shop_offer(index).ok());
        let owned = game.campaign.items[0].id;
        let view = view_model(
            &game,
            Mode::Personal,
            Some(&RecommendationAction::Equip(owned)),
        );
        assert_eq!(view.equip_focus, Some(owned));
        let view = view_model(
            &game,
            Mode::Personal,
            Some(&RecommendationAction::Equip(u64::MAX)),
        );
        assert_eq!(view.equip_focus, None);
        let view = view_model(
            &game,
            Mode::Personal,
            Some(&RecommendationAction::BuyEquipment(usize::MAX)),
        );
        assert_eq!(view.recommended, None);
    }
}
