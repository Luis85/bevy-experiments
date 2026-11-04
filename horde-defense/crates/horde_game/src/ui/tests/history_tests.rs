//! Egui harness tests: history screen.
use super::*;

#[test]
fn history_is_reachable_from_menu_camp_and_results_without_repaying_settled_runs() {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    for _ in 0..120 {
        f.game.tick(Input::default());
    }
    f.game.withdraw();
    assert_eq!(f.game.campaign.run_history.len(), 1);
    f.game.set_paused(true);
    let before = serde_json::to_value(&f.game).unwrap();
    f.screen = Screen::Menu;
    h.click(f, View::Menu, "History");
    assert!(f.screen == Screen::History);
    for size in [vec2(1440.0, 900.0), vec2(1050.0 / 1.35, 700.0 / 1.35)] {
        let output = h.frame_size(f, View::History, vec![], size);
        assert!(
            output
                .shapes
                .iter()
                .any(|s| shape_has_text(&s.shape, "Run history"))
        );
        assert!(
            output
                .shapes
                .iter()
                .any(|s| shape_has_text(&s.shape, "Withdrawal"))
        );
    }
    h.click(f, View::History, "Back");
    assert!(f.screen == Screen::Menu);
    f.screen = Screen::Camp;
    h.click(f, View::Header, "History");
    assert!(f.screen == Screen::History);
    h.click(f, View::History, "Back");
    assert!(f.screen == Screen::Camp);
    f.screen = Screen::Results;
    h.click(f, View::Results, "Run analysis");
    assert!(f.screen == Screen::History);
    h.click(f, View::History, "Back");
    assert!(f.screen == Screen::Results);
    assert_eq!(serde_json::to_value(&f.game).unwrap(), before);
}
