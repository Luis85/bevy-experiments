//! Egui harness tests: survivor selection and character development.
use super::*;

#[test]
fn legacy_body_points_are_spent_only_through_the_unified_upgrade_graph() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.game.campaign_mut().reward(0, 20, Mode::Personal);
    f.game.campaign_mut().choose_start_country("DEU").unwrap();
    let before = f.game.campaign().profile(Mode::Personal).personal.health;
    f.campaign.improvement_focus = Some(horde_core::campaign::RecommendationAction::LearnNode(
        "body_vitality_1".into(),
    ));
    f.act(|c| c.open_preparation(Tab::Skills));
    h.click(f, View::Results, "Learn skill · 1 legacy CP");
    assert_eq!(f.game.campaign().player.points, 0);
    assert_eq!(f.game.campaign().player.legacy_graph_points, 1);
    assert!(f.game.campaign().has_node("body_vitality_1"));
    assert_eq!(
        f.game.campaign().profile(Mode::Personal).personal.health,
        before + 5
    );
    let output = h.frame(f, View::Results, vec![]);
    assert!(
        !output
            .shapes
            .iter()
            .any(|s| shape_has_text(&s.shape, "Improve · 1 CP"))
    );
}

#[test]
fn immersive_skill_choices_and_purchase_remain_visible_in_the_compact_first_view() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.game.campaign_mut().choose_start_country("DEU").unwrap();
    f.game.campaign_mut().coins = 200;
    f.campaign.improvement_focus = Some(horde_core::campaign::RecommendationAction::LearnNode(
        "piercing".into(),
    ));
    f.act(|c| c.open_preparation(Tab::Skills));
    f.flush_for_test();
    settings::configure(&h.ctx, 1.35);
    f.settings.scale = 1.35;
    let size = vec2(1050., 700.);
    let before = serde_json::to_value(&f.game).unwrap();
    h.frame_size(f, View::Results, vec![], size);
    h.frame_size(f, View::Results, vec![], size);
    let output = h.frame_size(f, View::Results, vec![], size);
    let area = h.ctx.content_rect();
    for text in ["Controlled penetration", "Learn skill · 55 coins"] {
        let p = output
            .shapes
            .iter()
            .find_map(|s| text_position(&s.shape, text))
            .unwrap_or_else(|| panic!("Required upgrade detail not visible: {text}"));
        assert!(area.contains(p));
    }
    assert_eq!(serde_json::to_value(&f.game).unwrap(), before);
}

#[test]
fn a_fresh_campaign_previews_then_confirms_survivor_before_selecting_a_country() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.game = Game::default();
    f.screen = Screen::Menu;
    let original = serde_json::to_value(&f.game).unwrap();
    h.click(f, View::Menu, "New game");
    assert!(f.screen == Screen::SurvivorSelection);
    h.click(f, View::SurvivorSelection, "June");
    assert_eq!(f.campaign.preview_survivor, SurvivorId::June);
    assert_eq!(
        serde_json::to_value(&f.game).unwrap(),
        original,
        "Preview cannot spend or change authoritative identity"
    );
    h.click(f, View::SurvivorSelection, "Continue as June");
    assert_eq!(f.game.campaign().selected_survivor, Some(SurvivorId::June));
    assert!(f.screen == Screen::Camp && f.nav.tab == Tab::World);
    assert_eq!(f.game.campaign().coins, 0);
    assert_eq!(f.game.campaign().npc.name, "Mara Vale");
    let saved = Game::load(&f.paths.campaign()).unwrap();
    assert_eq!(saved.campaign().selected_survivor, Some(SurvivorId::June));
    h.click(f, View::Dossier, "Start journey");
    let a = f.game.attempt().unwrap();
    assert_eq!(a.profile.appearance, Appearance::Female);
    assert_eq!(
        a.profile.weapon.class,
        horde_core::weapons::WeaponClass::Pistol
    );
    assert!(!a.paused);
}

#[test]
fn both_survivor_choices_and_confirmation_fit_the_minimum_scaled_view() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.game = Game::default();
    f.screen = Screen::SurvivorSelection;
    let size = vec2(1050.0, 700.0) / 1.35;
    for choice in SurvivorId::ALL {
        f.campaign.preview_survivor = choice;
        let output = h.frame_size(f, View::SurvivorSelection, vec![], size);
        let viewport = h.ctx.content_rect();
        for label in [
            "Rook".to_owned(),
            "June".to_owned(),
            format!("Continue as {}", choice.name()),
            "Back to main menu".to_owned(),
        ] {
            let point = output
                .shapes
                .iter()
                .find_map(|shape| text_position(&shape.shape, &label))
                .unwrap_or_else(|| panic!("Missing first-view survivor action: {label}"));
            assert!(
                viewport.contains(point),
                "{label} must fit the selection viewport"
            );
        }
    }
}

#[test]
fn active_legacy_geometry_stays_frozen_when_character_development_opens_more_lanes() {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    assert_eq!(f.game.attempt().unwrap().lane_count, 11);
    f.game.campaign_mut().player.level = f.game.campaign_mut().player_level_cap();
    f.game.campaign_mut().player.points = f.game.campaign_mut().player.level - 1;
    f.game.campaign_mut().player.xp = 0;
    assert_eq!(f.game.campaign().lane_count(), 17);
    h.frame(f, View::Battle, vec![]);
    let a = f.game.attempt().unwrap();
    assert_eq!(a.lane_count, 11);
    assert_eq!(a.defender_position(), [0.5, 5.5]);
    let projection = art::ArenaProjection::for_attempt(f.battle.arena_rect.unwrap(), a);
    assert_eq!(
        projection.world(0.5, 5.5).y,
        f.battle.arena_rect.unwrap().center().y
    );
}

#[test]
fn character_max_has_an_explicit_full_bar_and_next_deployment_lane_preview() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.game.campaign_mut().player.level = f.game.campaign_mut().player_level_cap();
    f.game.campaign_mut().player.points = f.game.campaign_mut().player.level - 1;
    f.game.campaign_mut().player.xp = 0;
    f.act(|c| c.open_preparation(Tab::Character));
    let output = h.frame(f, View::Camp, vec![]);
    assert!(
        output
            .shapes
            .iter()
            .any(|s| shape_has_text(&s.shape, "MAX · level"))
    );
    assert!(
        output
            .shapes
            .iter()
            .any(|s| shape_has_text(&s.shape, "Next deployment: 17 lanes"))
    );
}
