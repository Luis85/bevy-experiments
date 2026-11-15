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

#[test]
fn escape_closes_the_new_journey_confirmation_without_a_reset() {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.game.set_paused(true);
    f.screen = Screen::Menu;
    f.game.save(&f.paths.campaign()).unwrap();
    let before = serde_json::to_value(&f.game).unwrap();
    h.click(f, View::Menu, "New game");
    assert!(f.menu.new_journey_confirm);
    // W3-04 rewrite: the Escape router closes the confirmation, so the press
    // goes through the whole frame.
    h.key(f, View::Full, Key::Escape);
    assert!(!f.menu.new_journey_confirm);
    assert_eq!(serde_json::to_value(&f.game).unwrap(), before);
}

/// CF-3 (closing): the menu's secondary buttons share the primary's 190 pt
/// column. Settings and History split it between them (with the item gap),
/// so the pair ends where "Save and quit" ends; on the journey menu "New
/// game" and "Change survivor" are column-wide too, at both target sizes.
#[test]
fn menu_buttons_share_the_primary_column() {
    for target in Target::BOTH {
        for journey in [false, true] {
            let mut fixture = if journey {
                IsolatedFrontend::new()
            } else {
                IsolatedFrontend::first_launch()
            };
            let h = Harness::at(target);
            let f = &mut fixture.frontend;
            f.screen = Screen::Menu;
            let output = h.frame(f, View::Menu, vec![]);
            let frame = |label: &str| {
                button_frame(&output, label)
                    .unwrap_or_else(|| panic!("{target:?}: {label} has no frame"))
            };
            let column = frame("Save and quit");
            assert!(
                (column.width() - 190.0).abs() < 0.5,
                "{target:?}: {column:?}"
            );
            let (settings, history) = (frame("Settings"), frame("History"));
            assert!(
                (settings.left() - column.left()).abs() < 0.5
                    && (history.right() - column.right()).abs() < 0.5,
                "{target:?}: Settings {settings:?} and History {history:?} leave the column {column:?}"
            );
            assert!(
                settings.right() < history.left(),
                "{target:?}: the pair overlaps"
            );
            if journey {
                for label in ["New game", "Change survivor"] {
                    let button = frame(label);
                    assert!(
                        (button.left() - column.left()).abs() < 0.5
                            && (button.right() - column.right()).abs() < 0.5,
                        "{target:?}: {label} {button:?} is not the column {column:?}"
                    );
                }
            }
        }
    }
}
