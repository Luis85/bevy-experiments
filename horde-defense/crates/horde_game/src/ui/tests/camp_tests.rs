//! Egui harness tests: camp and world map.
use super::*;

#[test]
fn first_country_selection_starts_and_saves_a_personal_defense_waiting_to_begin() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    h.click(f, View::Dossier, "Start journey");
    assert_eq!(f.game.campaign().world.start.as_deref(), Some("DEU"));
    assert!(f.screen == Screen::Battle);
    assert!(!f.battle.construction);
    // The first defense waits for "Begin defense" so the controls can be read.
    let a = f.game.attempt().unwrap();
    assert!(a.paused);
    assert_eq!(a.tick, 0);
    assert_eq!(a.country_id.as_deref(), Some("DEU"));
    assert_eq!(a.mode, Mode::Personal);
    let restored = Game::load(&f.paths.campaign()).unwrap();
    assert_eq!(restored.attempt().unwrap().tick, 0);
    assert!(restored.attempt().unwrap().paused);
    h.click(f, View::Battle, "Begin defense");
    assert!(!f.game.attempt().unwrap().paused);
}

#[test]
fn camp_pause_key_and_footer_control_pause_and_resume_all_parallel_work() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.game.campaign_mut().choose_start_country("DEU").unwrap();
    f.game.campaign_mut().attempts = 1;
    f.game.campaign_mut().coins = 500;
    f.game.campaign_mut().first_clears = horde_core::content::content()
        .levels
        .iter()
        .map(|level| level.id.clone())
        .collect();
    for id in ["DEU", "FRA"] {
        f.game.campaign_mut().world.cleared.push(id.into());
        f.game
            .campaign_mut()
            .world
            .country_levels
            .insert(id.into(), world().country(id).unwrap().level_count());
    }
    let recruit = f
        .game
        .with_strategy(|strategy, campaign| strategy.recruit(campaign, "DEU"))
        .unwrap();
    f.game.deploy_survivor(recruit, "POL", 0).unwrap();
    // W3-02 rewrite: Camp home owns the clock and its key (decision 6).
    // W3-04 rewrite: Escape on Camp home now goes back (to Results or the
    // return-to-menu confirmation, ui/escape.rs).
    // W3-10 rewrite: the clock key is the `world_clock` extra binding (P),
    // not the battle's Pause binding.
    f.nav.tab = Tab::Home;
    assert_eq!(f.settings.bindings[Action::Pause as usize], "Escape");
    h.key(f, View::Full, Key::P);
    assert!(f.game.global_paused());
    let frozen = serde_json::to_value(&f.game).unwrap();
    for _ in 0..60 {
        f.game.tick(Input::default());
    }
    assert_eq!(serde_json::to_value(&f.game).unwrap(), frozen);
    h.click(f, View::Camp, "Resume world clock");
    assert!(!f.game.global_paused());
    f.game.tick(Input::default());
    assert!(f.game.battles_mut()[0].game().attempt().unwrap().tick > 0);
}

#[test]
fn camp_pause_binding_does_not_steal_text_from_country_search() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    // Both the battle's Pause and the world clock sit on P.
    f.settings.bindings[Action::Pause as usize] = "P".into();
    assert_eq!(f.settings.extra_bindings.world_clock, "P");
    h.frame(f, View::Camp, vec![]);
    h.ctx
        .memory_mut(|memory| memory.request_focus(egui::Id::new("country_search")));
    h.frame(
        f,
        View::Camp,
        vec![key_event(Key::P, true), Event::Text("p".into())],
    );
    assert_eq!(f.campaign.search, "p");
    assert!(!f.game.global_paused());
}

#[test]
fn starting_country_action_is_above_the_fold_at_the_minimum_scaled_window() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    settings::configure(&h.ctx, 1.35);
    let f = &mut fixture.frontend;
    f.settings.scale = 1.35;
    let output = h.frame_size(f, View::Camp, vec![], vec2(1050.0, 700.0));
    let start = output
        .shapes
        .iter()
        .find_map(|shape| text_position(&shape.shape, "Start journey"))
        .expect("The first-session primary action must be painted");
    let footer = output
        .shapes
        .iter()
        .find_map(|shape| text_position(&shape.shape, "Back to survivor"))
        .unwrap();
    assert!(h.ctx.content_rect().contains(start));
    assert!(start.y < footer.y - 20.0);
}

#[test]
fn camp_observation_speed_never_offers_a_hidden_speedup_to_personal_combat() {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.screen = Screen::Camp;
    // W3-02 rewrite: the clock's speed controls live on Camp home only.
    f.nav.tab = Tab::Home;
    f.game.set_paused(true);
    f.game.strategy_mut().peak_freed = 4;
    let before = serde_json::to_value(&f.game).unwrap();
    let output = h.frame(f, View::Camp, vec![]);
    assert!(
        output
            .shapes
            .iter()
            .any(|shape| shape_has_text(&shape.shape, "Personal defense · 1×"))
    );
    assert!(
        output
            .shapes
            .iter()
            .all(|shape| text_position(&shape.shape, "4×").is_none())
    );
    assert_eq!(serde_json::to_value(&f.game).unwrap(), before);
}

#[test]
fn compact_country_selection_keeps_a_useful_map_visible_before_the_fixed_journey_footer() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    settings::configure(&h.ctx, 1.35);
    let f = &mut fixture.frontend;
    f.settings.scale = 1.35;
    f.status.practice_profile = true;
    f.status.message = "Practice scenario - progress is isolated from your campaign".into();
    for search in ["", "deu", "a"] {
        f.campaign.search = search.into();
        let output = h.frame_size(f, View::WorldWithChrome, vec![], vec2(1050.0, 700.0));
        let map = output
            .shapes
            .iter()
            .find_map(|clipped| match &clipped.shape {
                egui::Shape::Rect(shape) if shape.fill == Color32::from_rgb(22, 39, 42) => {
                    Some(shape.rect.intersect(clipped.clip_rect))
                }
                _ => None,
            })
            .expect("Map background must be painted");
        assert!(
            map.height() >= 180.0,
            "Search {search:?}: map needs180logical pixels, got {map:?}"
        );
        assert!(map.width() > 500.0);
        let start = output
            .shapes
            .iter()
            .find_map(|shape| text_position(&shape.shape, "Start journey"))
            .unwrap();
        assert!(start.y >= map.bottom());
        assert!(h.ctx.content_rect().contains(start));
        assert!(
            output
                .shapes
                .iter()
                .any(|shape| shape_has_text(&shape.shape, "Find country"))
        );
    }
}

#[test]
fn reselecting_the_current_camp_screen_keeps_strategy_ticks_running() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.game.set_paused(false);
    assert!(f.game.attempt().is_none() && !f.game.global_paused());
    let mut output = h.ctx.run_ui(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))),
            ..RawInput::default()
        },
        |ui| {
            let mut c = f.ctx();
            c.go(Screen::Camp);
            assert!(!c.leaving(), "same-screen request is not a transition");
            draw_screen(ui.ctx(), &mut c, 0.0);
        },
    );
    output.textures_delta.clear();
    let before = f.runtime.simulated_ticks;
    f.step();
    assert_eq!(f.runtime.simulated_ticks, before + 1);
    assert!(f.screen == Screen::Camp);
}

/// W3-18 (CD6): for a non-first country the World footer chooses the country
/// through `SelectCountry` and opens its Briefing. It never deploys.
#[test]
fn world_footer_never_creates_attempt() {
    for (liberated, label) in [(false, "Plan defense"), (true, "Manage country")] {
        let mut fixture = IsolatedFrontend::new();
        let h = Harness::new();
        let f = &mut fixture.frontend;
        f.game.campaign_mut().choose_start_country("DEU").unwrap();
        f.game.campaign_mut().attempts = 1;
        f.game.campaign_mut().world.unlocked.push("FRA".into());
        let levels = world().country("FRA").unwrap().level_count();
        if liberated {
            f.game.campaign_mut().world.cleared.push("FRA".into());
            f.game
                .campaign_mut()
                .world
                .country_levels
                .insert("FRA".into(), levels);
        }
        f.screen = Screen::Camp;
        f.nav.tab = Tab::World;
        f.campaign.selected_country = "FRA".into();
        f.campaign.stage = 3;
        let ids = f.game.campaign().next_attempt_id;
        let output = h.frame(f, View::Dossier, vec![]);
        assert_eq!(text_count(&output, label), 1, "{label}");
        assert_eq!(text_count(&output, "Start journey"), 0, "{label}");
        h.click(f, View::Dossier, label);
        assert!(f.game.attempt().is_none(), "{label}: nothing deploys");
        assert_eq!(f.game.campaign().next_attempt_id, ids, "{label}");
        assert_eq!(f.game.campaign().world.active.as_deref(), Some("FRA"));
        assert!(
            f.screen == Screen::Camp && f.nav.tab == Tab::Briefing,
            "{label}: the Briefing opens"
        );
        assert_eq!(f.campaign.selected_country, "FRA", "{label}");
        assert_eq!(
            f.campaign.stage,
            if liberated { levels - 1 } else { 0 },
            "{label}: the planned stage is the country's next stage"
        );
        assert!(!f.status.message_error, "{label}");
    }
}

/// CF-3 (closing): the camp footer is as tall as what it draws. W3-05 moved
/// Save to the header and the world clock row is Camp home's alone, so a
/// Prepare page's footer holds one primary row; the old 118 pt floor left
/// about 50 pt empty under it and took that from the page at 1050x700@1.35.
#[test]
fn camp_footer_fits_its_one_row_on_prepare_pages() {
    for target in Target::BOTH {
        for (tab, label) in [
            (Tab::Armory, "Go to briefing"),
            (Tab::Character, "Go to briefing"),
            (Tab::Briefing, "Deploy personally"),
        ] {
            let mut fixture = IsolatedFrontend::new();
            let h = Harness::at(target);
            let f = &mut fixture.frontend;
            f.game.campaign_mut().choose_start_country("DEU").unwrap();
            f.game.campaign_mut().journey_active = true;
            f.act(|c| c.open_preparation(tab));
            // Two frames: a panel sizes from its previous frame.
            h.frame(f, View::Camp, vec![]);
            let output = h.frame(f, View::Camp, vec![]);
            let screen = Rect::from_min_size(Pos2::ZERO, target.points());
            let panel = painted_rects(&output)
                .into_iter()
                .filter(|shape| shape.fill == art::PANEL)
                .map(|shape| shape.rect)
                .find(|rect| {
                    (rect.bottom() - screen.bottom()).abs() < 1.0
                        && (rect.width() - screen.width()).abs() < 1.0
                })
                .unwrap_or_else(|| panic!("{target:?} {tab:?}: no footer panel"));
            let button = button_frame(&output, label)
                .unwrap_or_else(|| panic!("{target:?} {tab:?}: {label} has no frame"));
            assert!(panel.contains_rect(button), "{target:?} {tab:?}");
            // The frame's 10 pt margins and the row's 5 pt trailing space,
            // with a few points of item spacing: nothing like a second row.
            let below = panel.bottom() - button.bottom();
            assert!(
                below <= 24.0,
                "{target:?} {tab:?}: {below} pt under {label} in the footer {panel:?}"
            );
        }
    }
}
