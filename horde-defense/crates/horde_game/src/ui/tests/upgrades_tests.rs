//! Egui harness tests: the Upgrades screen and its routes (W3-03).
use super::*;
use crate::ui::upgrades::upgrades_continue_label;

/// A withdrawn first defense of Germany with `coins` in the wallet, paused
/// on Results.
fn withdrawn_on_results(fixture: &mut IsolatedFrontend, coins: u64) {
    fixture.deploy();
    let f = &mut fixture.frontend;
    f.game.withdraw();
    f.game.campaign_mut().coins = coins;
    f.game.set_paused(true);
    f.screen = Screen::Results;
}

/// G2.1 (UX-01): Results and Upgrades are two screens. "Upgrade" opens the
/// tree on `Screen::Upgrades` with the camp row, the amber "Back to battle"
/// and "Back to camp"; Escape goes back to Results; "Back to camp" opens
/// Camp home. With no settled round "Back to camp" is the primary.
#[test]
fn upgrade_opens_upgrades_screen_with_back_to_battle_and_back_to_camp() {
    for target in Target::BOTH {
        let mut fixture = IsolatedFrontend::new();
        withdrawn_on_results(&mut fixture, 1_000_000);
        let h = Harness::at(target);
        let f = &mut fixture.frontend;
        h.frame(f, View::Full, vec![]);
        h.frame(f, View::Full, vec![]);
        assert_eq!(h.screen_primaries(), ["Upgrade"], "{target:?}");
        f.campaign.improvement_focus = None;

        h.click(f, View::Full, "Upgrade");
        assert!(f.screen == Screen::Upgrades, "{target:?}: {:?}", f.screen);
        assert!(
            f.campaign.improvement_focus.is_some(),
            "{target:?}: the tree opens on the recommended upgrade"
        );
        assert!(f.game.global_paused(), "{target:?}");
        h.frame(f, View::Full, vec![]);
        let output = h.frame(f, View::Full, vec![]);
        assert_eq!(h.screen_primaries(), ["Back to battle"], "{target:?}");
        for label in ["Back to battle", "Back to camp", "Journey upgrades"] {
            assert_eq!(text_count(&output, label), 1, "{target:?}: {label}");
        }
        // The debrief's own actions stay on Results.
        for label in ["Upgrade", "Leave journey", "Try again"] {
            assert_eq!(text_count(&output, label), 0, "{target:?}: {label}");
        }
        assert_eq!(
            Screen::Upgrades.settings_return_label(),
            "Save and return to upgrades"
        );

        // Escape goes back to the debrief; the tree is not there.
        h.key(f, View::Full, Key::Escape);
        assert!(f.screen == Screen::Results, "{target:?}: {:?}", f.screen);
        let output = h.frame(f, View::Full, vec![]);
        assert_eq!(text_count(&output, "Back to camp"), 0, "{target:?}");

        // "Back to camp" opens Camp home, still paused.
        h.click(f, View::Full, "Upgrade");
        assert!(f.screen == Screen::Upgrades, "{target:?}");
        h.click(f, View::Full, "Back to camp");
        assert!(
            f.screen == Screen::Camp && f.nav.tab == Tab::Home,
            "{target:?}: {:?} {:?}",
            f.screen,
            f.nav.tab
        );
        assert!(f.game.global_paused(), "{target:?}");

        // No settled round: nothing to continue, "Back to camp" is the
        // primary and Escape goes to Camp home.
        let mut fixture = IsolatedFrontend::new();
        let f = &mut fixture.frontend;
        f.game.campaign_mut().choose_start_country("DEU").unwrap();
        f.nav.tab = Tab::Home;
        f.act(|c| c.open_upgrades());
        f.flush_for_test();
        assert!(f.screen == Screen::Upgrades, "{target:?}");
        h.frame(f, View::Full, vec![]);
        let output = h.frame(f, View::Full, vec![]);
        assert_eq!(h.screen_primaries(), ["Back to camp"], "{target:?}");
        assert_eq!(text_count(&output, "Back to battle"), 0, "{target:?}");
        h.key(f, View::Full, Key::Escape);
        assert!(
            f.screen == Screen::Camp && f.nav.tab == Tab::Home,
            "{target:?}"
        );
    }
}

/// "Back to battle" after a loss deploys the same stage again, live and
/// from tick 0, without paying or recording anything; after a win with
/// defenses left it deploys the next one.
#[test]
fn back_to_battle_retries_same_stage() {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.game.attempt_mut().unwrap().hp = 0;
    f.runtime.publish(ControlFrame {
        enabled: true,
        speed: 4,
        ..Default::default()
    });
    f.step();
    assert!(f.screen == Screen::Results);
    let report = f.game.last_result().unwrap();
    assert_eq!((report.outcome, report.stage), (Outcome::Defeat, 0));
    let coins = f.game.campaign().coins;
    let records = f.game.campaign().run_history.len();
    h.click(f, View::Full, "Upgrade");
    assert!(f.screen == Screen::Upgrades);
    assert_eq!(
        f.act(|c| upgrades_continue_label(c)),
        Some("Back to battle")
    );
    h.click(f, View::Full, "Back to battle");
    assert!(f.screen == Screen::Battle, "{}", f.status.message);
    let attempt = f.game.attempt().unwrap();
    assert_eq!(attempt.stage, 0, "the lost stage again");
    assert_eq!(attempt.tick, 0);
    assert!(!attempt.paused);
    assert_eq!(f.game.campaign().coins, coins);
    assert_eq!(f.game.campaign().run_history.len(), records);

    // After a win with defenses left: the next defense.
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let f = &mut fixture.frontend;
    f.game.withdraw();
    f.game.last_result_mut().as_mut().unwrap().outcome = Outcome::Victory;
    f.game
        .campaign_mut()
        .world
        .country_levels
        .insert("DEU".into(), 1);
    f.game.set_paused(true);
    f.screen = Screen::Upgrades;
    h.click(f, View::Full, "Back to battle");
    assert!(f.screen == Screen::Battle, "{}", f.status.message);
    assert_eq!(f.game.attempt().unwrap().stage, 1);
}

/// `upgrades_continue_label` (design-ux-flow §3): a live defense, a settled
/// liberation, a settled round, and nothing to continue.
#[test]
fn upgrades_continue_label_table() {
    // No settled round.
    let mut fixture = IsolatedFrontend::new();
    let f = &mut fixture.frontend;
    f.game.campaign_mut().choose_start_country("DEU").unwrap();
    assert_eq!(f.act(|c| upgrades_continue_label(c)), None);

    // A live defense, with or without an older report.
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let f = &mut fixture.frontend;
    assert_eq!(
        f.act(|c| upgrades_continue_label(c)),
        Some("Return to active defense")
    );

    // A settled loss of a running journey.
    f.game.withdraw();
    assert_eq!(
        f.act(|c| upgrades_continue_label(c)),
        Some("Back to battle")
    );

    // The victory that liberated the country.
    f.game.last_result_mut().as_mut().unwrap().outcome = Outcome::Victory;
    let count = world().country("DEU").unwrap().level_count();
    f.game
        .campaign_mut()
        .world
        .country_levels
        .insert("DEU".into(), count);
    assert_eq!(
        f.act(|c| upgrades_continue_label(c)),
        Some("Choose next country")
    );

    // The journey was left: nothing to continue.
    f.game.apply(&Command::LeaveJourney).unwrap();
    assert!(!f.game.campaign().journey_active);
    assert_eq!(f.act(|c| upgrades_continue_label(c)), None);
}

/// A live defense opened from the battle: Upgrades leads back onto its
/// field, and the tree's screen shows the paused-fields strip.
#[test]
fn upgrades_over_a_live_defense_returns_to_it() {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.act(|c| c.open_upgrades());
    f.flush_for_test();
    assert!(f.screen == Screen::Upgrades);
    assert!(f.game.attempt().unwrap().paused);
    h.frame(f, View::Full, vec![]);
    let output = h.frame(f, View::Full, vec![]);
    assert_eq!(h.screen_primaries(), ["Return to active defense"]);
    assert_eq!(text_count(&output, "Fields paused during upgrades"), 1);
    h.click(f, View::Full, "Return to active defense");
    assert!(f.screen == Screen::Battle);
    assert!(f.game.attempt().is_some());
}
