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
/// training price, while the advancement allowance has room. The hover ends
/// with the core `Campaign::allowance_raise_hint`.
pub(super) fn training_point(campaign: &Campaign, npc: &Npc) -> PriceButton {
    let price = campaign.npc_training_price();
    PriceButton {
        label: format!("Buy 1 training CP · {price} coins"),
        enabled: campaign.coins >= price && npc.awarded < campaign.allowance(),
        disabled_hover: format!(
            "Requires {price} coins and room in the campaign advancement allowance. {}",
            campaign.allowance_raise_hint()
        ),
    }
}

/// "Recruit survivor" in `country` at the core recruit price. The disabled
/// hover names each blocker that holds the button, in the order the checks
/// read; an open button keeps the whole requirement list.
pub(super) fn recruit(game: &Game, country: &str) -> PriceButton {
    let campaign = game.campaign();
    let price = campaign.recruit_price();
    let max = world().strategy_config.max_roster;
    let blockers: Vec<String> = [
        (!campaign.world.cleared.iter().any(|id| id == country))
            .then(|| "Choose a liberated country.".to_owned()),
        (game.strategy().roster.len() >= max)
            .then(|| format!("The roster is full ({max} survivors).")),
        (campaign.coins < price)
            .then(|| format!("Requires {price} coins; you have {}.", campaign.coins)),
        campaign
            .reserved
            .is_some()
            .then(|| "Return the active defender to camp first.".to_owned()),
    ]
    .into_iter()
    .flatten()
    .collect();
    PriceButton {
        label: format!("Recruit survivor · {price} coins"),
        enabled: blockers.is_empty(),
        disabled_hover: if blockers.is_empty() {
            format!(
                "Requires a liberated country, room in the roster, {price} coins and the active defender in camp."
            )
        } else {
            blockers.join(" ")
        },
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

    /// The recruit hover names each blocker that holds the button, so coins
    /// alone read as coins alone (W2 review), and the full list stays a
    /// readable sentence.
    #[test]
    fn recruit_hover_names_only_the_blockers() {
        let (game, country) = camp_at_rank_10(199);
        assert_eq!(
            recruit(&game, &country).disabled_hover,
            "Requires 200 coins; you have 199."
        );
        let (mut game, country) = camp_at_rank_10(1_000);
        game.staging().campaign().reserved = Some(horde_core::content::Mode::Personal);
        assert_eq!(
            recruit(&game, &country).disabled_hover,
            "Return the active defender to camp first."
        );
        let (game, _) = camp_at_rank_10(1_000);
        let elsewhere = world()
            .countries
            .iter()
            .find(|c| !game.campaign().world.cleared.contains(&c.id))
            .unwrap()
            .id
            .clone();
        assert_eq!(
            recruit(&game, &elsewhere).disabled_hover,
            "Choose a liberated country."
        );
        // A full roster and short coins: both named, in one sentence each.
        let (mut game, country) = camp_at_rank_10(1_000_000);
        let max = world().strategy_config.max_roster;
        while game.strategy().roster.len() < max {
            game.apply(&horde_core::command::Command::Recruit(country.clone()))
                .unwrap();
        }
        game.staging().campaign().coins = 0;
        let price = game.campaign().recruit_price();
        assert_eq!(
            recruit(&game, &country).disabled_hover,
            format!("The roster is full ({max} survivors). Requires {price} coins; you have 0.")
        );
        // An open button keeps the whole requirement list as its hover.
        let (game, country) = camp_at_rank_10(1_000);
        assert_eq!(
            recruit(&game, &country).disabled_hover,
            "Requires a liberated country, room in the roster, 200 coins and the active defender in camp."
        );
    }

    #[test]
    fn training_hover_names_what_raises_the_allowance() {
        // Two liberations raise the cap past the first clears (W2-10), so
        // the hover reads the core hint rather than "clear new stages".
        let (game, _) = camp_at_rank_10(1_000);
        let campaign = game.campaign();
        let hover = training_point(campaign, &campaign.npc).disabled_hover;
        assert!(hover.ends_with(&campaign.allowance_raise_hint()), "{hover}");
        assert!(hover.contains("Liberate a country"), "{hover}");
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
