//! Egui harness tests: main menu and new-game flow.
use super::*;

#[test]
fn continuing_from_the_menu_preserves_a_saved_world_pause() {
    let mut fixture = IsolatedFrontend::new();
    let f = &mut fixture.frontend;
    f.game.campaign_mut().choose_start_country("DEU").unwrap();
    f.game.set_paused(true);
    f.screen = Screen::Menu;
    let before = serde_json::to_value(&f.game).unwrap();
    f.act(|c| c.resume());
    f.flush_for_test();
    assert!(f.screen == Screen::Camp);
    assert!(f.game.global_paused());
    assert_eq!(serde_json::to_value(&f.game).unwrap(), before);
}

#[test]
fn new_game_requires_confirmation_archives_previous_progress_and_restarts_survivor_choice() {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.game.set_paused(true);
    f.game.campaign_mut().coins = 91;
    f.screen = Screen::Menu;
    f.settings.scale = 1.1;
    f.game.save(&f.paths.campaign()).unwrap();
    let before = serde_json::to_value(&f.game).unwrap();
    h.click(f, View::Menu, "New game");
    assert!(f.menu.new_journey_confirm);
    assert_eq!(serde_json::to_value(&f.game).unwrap(), before);
    h.click(f, View::Menu, "Keep current journey");
    assert!(!f.menu.new_journey_confirm);
    assert_eq!(serde_json::to_value(&f.game).unwrap(), before);
    h.click(f, View::Menu, "New game");
    h.click(f, View::Menu, "Start new journey");
    assert!(f.screen == Screen::SurvivorSelection);
    assert!(f.game.campaign().selected_survivor.is_none());
    assert!(f.game.campaign().world.start.is_none());
    assert_eq!(f.game.campaign().coins, 0);
    assert!(!f.game.campaign().journey_active);
    assert_eq!(f.settings.scale, 1.1);
    let archive = std::fs::read_dir(
        f.paths
            .campaign()
            .parent()
            .unwrap()
            .join("journey-archives"),
    )
    .unwrap()
    .next()
    .unwrap()
    .unwrap()
    .path()
    .join("campaign.json");
    assert_eq!(
        serde_json::to_value(Game::load(&archive).unwrap()).unwrap(),
        before
    );
}

#[test]
fn blank_and_unreadable_saved_generations_still_require_new_game_confirmation_without_writes() {
    for unreadable in [false, true] {
        let mut fixture = IsolatedFrontend::new();
        let h = Harness::new();
        let f = &mut fixture.frontend;
        f.game = Game::default();
        f.status.profile_exists = false;
        f.screen = Screen::Menu;
        if unreadable {
            std::fs::create_dir_all(f.paths.campaign().parent().unwrap()).unwrap();
            std::fs::write(f.paths.campaign(), b"unreadable saved generation").unwrap();
        } else {
            f.game.save(&f.paths.campaign()).unwrap();
        }
        let before = std::fs::read(f.paths.campaign()).unwrap();
        let model = serde_json::to_value(&f.game).unwrap();
        h.click(f, View::Menu, "New game");
        assert!(f.menu.new_journey_confirm);
        assert!(f.screen == Screen::Menu);
        assert_eq!(std::fs::read(f.paths.campaign()).unwrap(), before);
        assert_eq!(serde_json::to_value(&f.game).unwrap(), model);
    }
}
