//! Egui harness tests: the camp navigation row, its redirects and the one
//! primary action per page.
use super::*;

fn labels(f: &mut FrontendParts) -> Vec<&'static str> {
    f.act(|c| {
        navigation::camp_entries(c)
            .into_iter()
            .map(navigation::NavEntry::label)
            .collect()
    })
}

/// A journey in progress with no live main attempt.
fn between_runs(fixture: &mut IsolatedFrontend) -> &mut FrontendParts {
    let f = &mut fixture.frontend;
    f.game.campaign_mut().choose_start_country("DEU").unwrap();
    f.game.campaign_mut().journey_active = true;
    f
}

#[test]
fn one_camp_row_lists_the_hub_first_and_unlocked_features_only() {
    let mut fixture = IsolatedFrontend::new();
    let f = between_runs(&mut fixture);
    f.game.campaign_mut().legacy_features = false;
    assert_eq!(
        labels(f),
        [
            "Journey upgrades",
            "Briefing",
            "Equipment",
            "Character",
            "World map"
        ]
    );
    // Legacy campaigns with an attempt behind them unlock Mara and Operations.
    f.game.campaign_mut().legacy_features = true;
    f.game.campaign_mut().attempts = 1;
    assert_eq!(
        labels(f),
        [
            "Journey upgrades",
            "Briefing",
            "Equipment",
            "Character",
            "Mara",
            "World map",
            "Operations"
        ]
    );
    let h = Harness::new();
    f.act(|c| c.open_preparation(Tab::Armory));
    let output = h.frame(f, View::Camp, vec![]);
    for label in labels(f) {
        // The page title may repeat its tab's name.
        assert!(text_count(&output, label) >= 1, "{label} in the row");
    }
    for gone in [
        "Skills",
        "Fortifications",
        "Prepare",
        "Campaign",
        "Back to journey upgrades",
    ] {
        assert_eq!(text_count(&output, gone), 0, "{gone} is no longer a route");
    }
}

#[test]
fn a_live_main_attempt_hides_the_hub_tab() {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let f = &mut fixture.frontend;
    assert!(!labels(f).contains(&"Journey upgrades"));
    assert_eq!(labels(f).first(), Some(&"Briefing"));
}

#[test]
fn the_hub_tab_returns_every_prepare_page_to_journey_upgrades_in_one_click() {
    for tab in [Tab::Briefing, Tab::Armory, Tab::Character, Tab::World] {
        let mut fixture = IsolatedFrontend::new();
        let h = Harness::new();
        let f = between_runs(&mut fixture);
        f.act(|c| c.open_preparation(tab));
        f.act(|c| c.go_tab(tab));
        assert!(f.screen == Screen::Camp);
        h.click(f, View::Camp, "Journey upgrades");
        assert!(f.screen == Screen::Results, "{tab:?}");
    }
}

#[test]
fn old_skills_and_fortification_tabs_redirect_to_the_one_upgrade_home() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = between_runs(&mut fixture);
    f.act(|c| c.open_preparation(Tab::Skills));
    assert!(f.screen == Screen::Results);

    f.act(|c| c.open_preparation(Tab::Armory));
    f.campaign.improvement_focus = None;
    f.act(|c| c.open_preparation(Tab::Engineering));
    assert!(f.screen == Screen::Results);
    let fortification = f
        .game
        .campaign()
        .recommendation_fallback(horde_core::campaign::UpgradeFocus::Fortification)
        .map(|r| r.action);
    assert!(fortification.is_some());
    assert_eq!(f.campaign.improvement_focus, fortification);

    // A remembered old tab never renders a second copy of the tree.
    for tab in [Tab::Skills, Tab::Engineering] {
        f.screen = Screen::Camp;
        f.nav.tab = tab;
        h.frame(f, View::Camp, vec![]);
        assert!(f.screen == Screen::Results, "{tab:?}");
    }

    // During a live defense the redirect lands on the same home, whose
    // primary action returns to the field.
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let f = &mut fixture.frontend;
    f.screen = Screen::Camp;
    f.game.set_paused(true);
    f.act(|c| c.open_preparation(Tab::Skills));
    assert!(f.screen == Screen::Results);
    assert!(f.game.attempt().is_some());
    let output = h.frame(f, View::ResultsWithChrome, vec![]);
    assert_eq!(text_count(&output, "Return to active defense"), 1);
    assert_eq!(text_count(&output, "Leave journey"), 0);
    h.click(f, View::ResultsWithChrome, "Return to active defense");
    assert!(f.screen == Screen::Battle);
}

#[test]
fn prepare_footers_lead_to_the_briefing_never_back_to_the_map() {
    for tab in [Tab::Armory, Tab::Character] {
        let mut fixture = IsolatedFrontend::new();
        let h = Harness::new();
        let f = between_runs(&mut fixture);
        f.act(|c| c.open_preparation(tab));
        let output = h.frame(f, View::Camp, vec![]);
        assert_eq!(text_count(&output, "Go to briefing"), 1, "{tab:?}");
        assert_eq!(text_count(&output, "Back to world map"), 0);
        assert_eq!(text_count(&output, "Back to journey upgrades"), 0);
        h.click(f, View::Camp, "Go to briefing");
        assert!(f.screen == Screen::Camp && f.nav.tab == Tab::Briefing);
        let output = h.frame(f, View::Camp, vec![]);
        assert_eq!(text_count(&output, "Deploy personally"), 1);
        assert_eq!(text_count(&output, "Go to briefing"), 0);
    }
}

#[test]
fn an_active_camp_shows_exactly_one_return_and_an_honest_clock_label() {
    for tab in [Tab::Armory, Tab::Briefing, Tab::World, Tab::Operations] {
        let mut fixture = IsolatedFrontend::new();
        fixture.deploy();
        let h = Harness::new();
        let f = &mut fixture.frontend;
        f.game.set_paused(true);
        f.screen = Screen::Camp;
        f.nav.tab = tab;
        let output = h.frame(f, View::WorldWithChrome, vec![]);
        assert_eq!(
            text_count(&output, "Return to active defense"),
            1,
            "{tab:?}"
        );
        assert_eq!(text_count(&output, "Return and resume defense"), 0);
        assert_eq!(text_count(&output, "Resume defense"), 0);
        assert_eq!(text_count(&output, "Resume world clock"), 1, "{tab:?}");
    }
}

#[test]
fn settings_and_history_return_to_the_page_they_were_opened_from() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = between_runs(&mut fixture);
    f.act(|c| c.open_preparation(Tab::Armory));
    h.click(f, View::WorldWithChrome, "Settings");
    assert!(f.screen == Screen::Settings);
    f.act(|c| c.request_save(SaveIntent::SettingsBack));
    f.flush_for_test();
    assert!(f.screen == Screen::Camp && f.nav.tab == Tab::Armory);

    f.act(|c| c.open_upgrades());
    assert!(f.screen == Screen::Results);
    h.click(f, View::ResultsWithChrome, "History");
    assert!(f.screen == Screen::History);
    f.act(|c| c.close_history());
    assert!(f.screen == Screen::Results);
    h.click(f, View::ResultsWithChrome, "Settings");
    assert!(f.screen == Screen::Settings);
    f.act(|c| c.request_save(SaveIntent::SettingsBack));
    f.flush_for_test();
    assert!(f.screen == Screen::Results);
}

#[test]
fn the_single_row_stays_one_line_inside_the_minimum_window() {
    let mut fixture = IsolatedFrontend::new();
    // Harness::new installs the shipped fonts (Atkinson Hyperlegible body).
    let h = Harness::new();
    settings::configure(&h.ctx, 1.35);
    let f = between_runs(&mut fixture);
    f.settings.scale = 1.35;
    f.game.campaign_mut().legacy_features = true;
    f.game.campaign_mut().attempts = 1;
    f.act(|c| c.open_preparation(Tab::Armory));
    assert_eq!(labels(f).len(), 7, "every destination unlocked");
    let size = vec2(1050.0, 700.0);
    let output = h.frame_size(f, View::Camp, vec![], size);
    let bounds: Vec<Rect> = labels(f)
        .into_iter()
        .map(|label| {
            output
                .shapes
                .iter()
                .find_map(|s| labeled_bounds(&s.shape, label))
                .unwrap_or_else(|| panic!("{label} painted"))
        })
        .collect();
    // The measured frame really ran at the scaled minimum window. (After a
    // pass the context reports its default rect, so the area is derived from
    // the frame's own scale rather than read back.)
    assert!((output.pixels_per_point - 1.35).abs() < 1e-3);
    let area = Rect::from_min_size(Pos2::ZERO, size / output.pixels_per_point);
    // ...and laid the row out in the shipped body face, not egui's default.
    let body = egui::FontId::proportional(16.0);
    let width = |ctx: &egui::Context| {
        ctx.fonts_mut(|fonts| {
            fonts
                .layout_no_wrap("Journey upgrades".into(), body.clone(), Color32::WHITE)
                .size()
                .x
        })
    };
    let fallback = egui::Context::default();
    fallback
        .run_ui(RawInput::default(), |_| {})
        .textures_delta
        .clear();
    let (shipped, default_face) = (width(&h.ctx), width(&fallback));
    assert!(
        (bounds[0].width() - shipped).abs() < 0.5,
        "row measured with the shipped face: {} vs {shipped}",
        bounds[0].width()
    );
    assert!(
        (shipped - default_face).abs() > 1.0,
        "shipped face {shipped} must differ from egui's default {default_face}"
    );
    let top = bounds.iter().map(|r| r.top()).fold(f32::MAX, f32::min);
    let bottom = bounds.iter().map(|r| r.bottom()).fold(f32::MIN, f32::max);
    for rect in &bounds {
        assert!(area.contains_rect(*rect), "{rect:?} inside {area:?}");
    }
    // The old layout needed two rows; all seven destinations share one line.
    assert!(
        bottom - top < bounds[0].height() * 1.5,
        "one line at 1050x700@1.35: {top}..{bottom}"
    );
}
