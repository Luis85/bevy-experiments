//! Camp purchase buttons whose labels, enable checks and hover text read the
//! core camp prices (rulings R10 and D15): `Campaign::recruit_price` and
//! `Campaign::npc_training_price`, both threat-scaled.
use horde_core::{
    campaign::{Campaign, Npc},
    sim::Game,
    world::world,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PriceButton {
    pub label: String,
    pub enabled: bool,
    pub disabled_hover: String,
}

/// "Buy 1 training CP" for Mara or a recruit (`npc`): one point at the core
/// training price, while the advancement allowance has room.
pub(super) fn training_point(campaign: &Campaign, npc: &Npc) -> PriceButton {
    let price = campaign.npc_training_price();
    PriceButton {
        label: format!("Buy 1 training CP · {price} coins"),
        enabled: campaign.coins >= price && npc.awarded < campaign.allowance(),
        disabled_hover: format!(
            "Requires {price} coins and room in the campaign advancement allowance. Clear new stages to raise the allowance."
        ),
    }
}

/// "Recruit survivor" in `country` at the core recruit price.
pub(super) fn recruit(game: &Game, country: &str) -> PriceButton {
    let campaign = game.campaign();
    let price = campaign.recruit_price();
    let enabled = campaign.world.cleared.iter().any(|id| id == country)
        && game.strategy().roster.len() < world().strategy_config.max_roster
        && campaign.coins >= price
        && campaign.reserved.is_none();
    PriceButton {
        label: format!("Recruit survivor · {price} coins"),
        enabled,
        disabled_hover: format!(
            "Choose a liberated country, keep space in the roster and {price} coins, and return the active defender to camp."
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two liberated countries, recruitment open, threat rank 10.
    fn camp_at_rank_10(coins: u64) -> (Game, String) {
        let mut c = Campaign {
            legacy_features: true,
            ..Campaign::default()
        };
        let ids: Vec<_> = world()
            .countries
            .iter()
            .take(2)
            .map(|c| c.id.clone())
            .collect();
        c.choose_start_country(&ids[0]).unwrap();
        for id in &ids {
            let levels = world().country(id).unwrap().level_count();
            c.world.country_levels.insert(id.clone(), levels);
            c.world.cleared.push(id.clone());
        }
        c.coins = coins;
        c.victories = 10;
        let mut game = Game::default();
        *game.staging().campaign() = c;
        assert_eq!(game.campaign().progression_rank(), 10);
        (game, ids[0].clone())
    }

    #[test]
    fn camp_price_labels_follow_core_price_at_rank_10() {
        let (game, country) = camp_at_rank_10(1_000);
        let recruit_button = recruit(&game, &country);
        assert_eq!(recruit_button.label, "Recruit survivor · 200 coins");
        assert!(recruit_button.enabled);
        assert!(
            recruit_button.disabled_hover.contains("200 coins"),
            "{}",
            recruit_button.disabled_hover
        );
        let training = training_point(game.campaign(), &game.campaign().npc);
        assert_eq!(training.label, "Buy 1 training CP · 50 coins");
        assert!(training.enabled);
        assert!(
            training.disabled_hover.starts_with("Requires 50 coins"),
            "{}",
            training.disabled_hover
        );
        // The enable checks follow the same core price.
        let (game, country) = camp_at_rank_10(199);
        assert!(!recruit(&game, &country).enabled, "199 < 200");
        let (game, country) = camp_at_rank_10(200);
        assert!(recruit(&game, &country).enabled);
        let (game, _) = camp_at_rank_10(49);
        assert!(!training_point(game.campaign(), &game.campaign().npc).enabled);
        let (game, _) = camp_at_rank_10(50);
        assert!(training_point(game.campaign(), &game.campaign().npc).enabled);
    }

    #[test]
    fn camp_screens_render_core_prices_not_literals() {
        for (path, source) in [
            ("ui/character.rs", include_str!("character.rs")),
            ("ui/operations.rs", include_str!("operations.rs")),
        ] {
            for literal in ["25 coins", ">= 25", "recruit_cost"] {
                assert!(!source.contains(literal), "{path} still has {literal:?}");
            }
        }
    }
}
