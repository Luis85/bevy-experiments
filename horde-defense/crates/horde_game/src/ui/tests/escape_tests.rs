//! The Escape router in whole frames (G2.5, D18): what a press does on each
//! screen once `draw_screen` routes it, and that the router consumes it in
//! one place.
use super::*;
use crate::ui::modal::AppModal;

/// The labels of the last frame's modal primaries.
fn modal_primaries(h: &Harness) -> Vec<String> {
    h.primaries()
        .into_iter()
        .filter(|record| record.surface == Surface::Modal)
        .map(|record| record.label)
        .collect()
}

#[test]
fn escape_on_menu_opens_quit_confirmation() {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.game.set_paused(true);
    f.screen = Screen::Menu;
    h.key(f, View::Full, Key::Escape);
    assert_eq!(f.status.modal, Some(AppModal::Quit));
    assert!(f.screen == Screen::Menu);
    let output = h.frame(f, View::Full, vec![]);
    assert_eq!(text_count(&output, "Quit Last Light?"), 1);
    assert_eq!(text_count(&output, "Stay"), 1);
    assert_eq!(modal_primaries(&h), ["Save and quit"]);
    // Escape closes the confirmation, and the press that closes it does not
    // open it again.
    h.key(f, View::Full, Key::Escape);
    assert_eq!(f.status.modal, None);
    let output = h.frame(f, View::Full, vec![]);
    assert_eq!(text_count(&output, "Quit Last Light?"), 0);
    // "Stay" closes it too.
    h.key(f, View::Full, Key::Escape);
    h.click(f, View::Full, "Stay");
    assert_eq!(f.status.modal, None);
    assert!(f.screen == Screen::Menu);
    assert!(!f.status.save_pending);
    // "Save and quit" saves the journey with the quit intent.
    h.key(f, View::Full, Key::Escape);
    let model = serde_json::to_value(&f.game).unwrap();
    h.click(f, View::Full, "Save and quit");
    assert_eq!(f.status.modal, None);
    assert!(!f.status.save_pending, "{}", f.status.message);
    let saved = Game::load(&f.paths.campaign()).unwrap();
    assert_eq!(serde_json::to_value(&saved).unwrap(), model);
}

#[test]
fn escape_in_construction_closes_without_pausing() {
    let mut fixture = IsolatedFrontend::new();
    fixture.unlock_trap();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    h.key(f, View::Full, Key::B);
    assert!(f.battle.construction);
    assert!(!f.game.global_paused());
    h.key(f, View::Full, Key::Escape);
    assert!(!f.battle.construction, "Escape closes construction");
    assert!(!f.game.global_paused(), "and does not pause");
    assert!(!f.game.attempt().unwrap().paused);
    // A paused field stays paused: closing construction never toggles Pause.
    h.key(f, View::Full, Key::Escape);
    assert!(f.game.global_paused());
    h.key(f, View::Full, Key::B);
    assert!(f.battle.construction);
    h.key(f, View::Full, Key::Escape);
    assert!(!f.battle.construction);
    assert!(f.game.global_paused(), "still paused");
    assert!(f.screen == Screen::Battle);
}

#[test]
fn pause_bound_to_escape_still_pauses_after_router_consumes() {
    let mut fixture = IsolatedFrontend::new();
    fixture.unlock_trap();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    assert_eq!(f.settings.bindings[Action::Pause as usize], "Escape");
    // The router consumes the press that closes construction, so Pause never
    // sees it; the next press is left to Pause.
    h.key(f, View::Full, Key::B);
    h.key(f, View::Full, Key::Escape);
    assert!(!f.battle.construction);
    assert!(!f.game.global_paused());
    h.key(f, View::Full, Key::Escape);
    assert!(f.game.global_paused(), "Escape reaches the Pause binding");
    h.key(f, View::Full, Key::Escape);
    assert!(!f.game.global_paused(), "and resumes");
    // The details window is topmost: Escape closes it and does not pause.
    f.status.feedback.details_open = true;
    h.key(f, View::Full, Key::Escape);
    assert!(!f.status.feedback.details_open);
    assert!(!f.game.global_paused());
    // With Pause on P, Escape no longer pauses and P does.
    f.settings.bindings[Action::Pause as usize] = "P".into();
    h.key(f, View::Full, Key::Escape);
    assert!(!f.game.global_paused(), "Escape is ignored");
    assert!(f.screen == Screen::Battle);
    h.key(f, View::Full, Key::P);
    assert!(f.game.global_paused());
}

#[test]
fn escape_walks_a_camp_tab_back_to_camp_home_then_the_return_modal() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.game.campaign_mut().choose_start_country("DEU").unwrap();
    f.nav.tab = Tab::Briefing;
    h.key(f, View::Full, Key::Escape);
    assert!(f.screen == Screen::Camp);
    assert_eq!(f.nav.tab, Tab::Home);
    // No report yet: Camp home asks before returning to the menu.
    h.key(f, View::Full, Key::Escape);
    assert_eq!(f.status.modal, Some(AppModal::ReturnToMenu));
    h.frame(f, View::Full, vec![]);
    assert_eq!(modal_primaries(&h), ["Save and return to main menu"]);
    h.click(f, View::Full, "Save and return to main menu");
    assert_eq!(f.status.modal, None);
    assert!(f.screen == Screen::Menu, "{}", f.status.message);
    assert!(Game::load(&f.paths.campaign()).unwrap().global_paused());
}

#[test]
fn escape_on_camp_home_after_a_defense_opens_results_then_the_return_modal() {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.game.withdraw();
    assert!(f.game.last_result().is_some() && f.game.campaign().journey_active);
    f.screen = Screen::Camp;
    f.nav.tab = Tab::Home;
    h.key(f, View::Full, Key::Escape);
    assert!(f.screen == Screen::Results);
    assert_eq!(f.status.modal, None);
    h.key(f, View::Full, Key::Escape);
    assert!(f.screen == Screen::Results);
    assert_eq!(f.status.modal, Some(AppModal::ReturnToMenu));
    h.click(f, View::Full, "Stay");
    assert_eq!(f.status.modal, None);
    assert!(f.screen == Screen::Results);
}

#[test]
fn escape_closes_the_observer_and_armory_confirmation_before_the_tab() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.game.campaign_mut().choose_start_country("DEU").unwrap();
    f.nav.tab = Tab::Armory;
    f.armory.sell_confirm = true;
    h.key(f, View::Full, Key::Escape);
    assert!(!f.armory.sell_confirm);
    assert_eq!(f.nav.tab, Tab::Armory, "the confirmation closes first");
    h.key(f, View::Full, Key::Escape);
    assert_eq!(f.nav.tab, Tab::Home);
}

#[test]
fn escape_from_survivor_selection_and_history_goes_back() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.screen = Screen::SurvivorSelection;
    h.key(f, View::Full, Key::Escape);
    assert!(f.screen == Screen::Menu);
    assert_eq!(f.status.modal, None, "one press is one step");
    f.act(|c| c.open_history());
    f.flush_for_test();
    assert!(f.screen == Screen::History);
    h.key(f, View::Full, Key::Escape);
    assert!(f.screen == Screen::Menu);
}

#[test]
fn a_discarded_pass_routes_escape_once() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.screen = Screen::Menu;
    f.runtime.begin_frame(h.ctx.cumulative_frame_nr() as u32);
    let mut passes = 0;
    let mut output = h.ctx.run_ui(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))),
            events: vec![key_event(Key::Escape, true)],
            ..RawInput::default()
        },
        |ui| {
            passes += 1;
            if passes == 1 {
                ui.ctx().request_discard("test: a discarded first pass");
            }
            draw_screen(ui.ctx(), &mut f.ctx(), 0.0);
        },
    );
    output.textures_delta.clear();
    assert_eq!(passes, 2);
    // The second pass does not read the same press as "close the modal".
    assert_eq!(f.status.modal, Some(AppModal::Quit));
}
