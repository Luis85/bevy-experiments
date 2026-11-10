//! `Game::operations_available` (W2-02, design-progression-tree ruling W-2):
//! the Operations page is reachable. The predicate moved verbatim from the
//! camp UI (`ui/context.rs`), which now delegates to it; this pin keeps the
//! core read equal to the old UI predicate.
use horde_core::{sim::Game, skill_catalog::CampaignFeature};

const FIXTURE_V1: &str = include_str!("fixtures/save_v1.json");
const FIXTURE_V7: &str = include_str!("fixtures/save_v7.json");

/// The camp UI predicate as it stood before W2-02, copied verbatim.
fn legacy_ui_predicate(game: &Game) -> bool {
    [
        CampaignFeature::Recruitment,
        CampaignFeature::Parallel,
        CampaignFeature::Operations,
    ]
    .into_iter()
    .any(|feature| game.campaign().has_feature(feature))
        || !game.strategy().roster.is_empty()
        || !game.battles().is_empty()
}

/// Drops every unlocked feature, so only the roster and expedition terms
/// can open the page.
fn without_features(mut game: Game) -> Game {
    let campaign = game.campaign_mut();
    campaign.legacy_features = false;
    campaign.skill_nodes.clear();
    campaign.skills.clear();
    game
}

/// A fresh profile that owns only the catalogue node granting `feature`, so
/// that feature term alone can open the page.
fn node_only(feature: CampaignFeature) -> Game {
    let mut game = without_features(Game::default());
    let unlock = game
        .campaign()
        .feature_unlock(feature)
        .unwrap_or_else(|| panic!("a skill node grants {feature:?}"));
    game.campaign_mut().skill_nodes.insert(unlock.node_id);
    assert!(game.strategy().roster.is_empty() && game.battles().is_empty());
    game
}

/// Eight fixtures, one per way the page opens or stays shut, with the value
/// each must give: one per feature term (through its skill node), the roster
/// and expedition terms, legacy features, a full v7 save and a fresh profile.
fn fixtures() -> Vec<(&'static str, Game, bool)> {
    let v1 = Game::decode_save_generation(FIXTURE_V1.as_bytes()).expect("v1 fixture loads");
    let v7 = Game::decode_save_generation(FIXTURE_V7.as_bytes()).expect("v7 fixture loads");
    let mut roster_only = without_features(v7.clone());
    roster_only.battles_mut().clear();
    let mut battles_only = without_features(v7.clone());
    battles_only.strategy_mut().roster.clear();
    vec![
        ("fresh profile", Game::default(), false),
        ("v1 legacy features", v1, true),
        ("v7 freeze", v7, true),
        ("v7 roster only", roster_only, true),
        ("v7 expeditions only", battles_only, true),
        (
            "Recruitment node only",
            node_only(CampaignFeature::Recruitment),
            true,
        ),
        (
            "Parallel node only",
            node_only(CampaignFeature::Parallel),
            true,
        ),
        (
            "Operations node only",
            node_only(CampaignFeature::Operations),
            true,
        ),
    ]
}

#[test]
fn operations_available_matches_legacy_ui_predicate() {
    for (name, game, expected) in fixtures() {
        assert_eq!(
            game.operations_available(),
            legacy_ui_predicate(&game),
            "{name}: core and the legacy UI predicate disagree"
        );
        assert_eq!(game.operations_available(), expected, "{name}");
    }
}

#[test]
fn operations_available_is_a_pure_read() {
    for (name, game, _) in fixtures() {
        let before = serde_json::to_value(&game).expect("game serialises");
        let _ = game.operations_available();
        assert_eq!(
            serde_json::to_value(&game).expect("game serialises"),
            before,
            "{name}"
        );
    }
}
