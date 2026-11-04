//! Egui harness tests: camp and world map.
use super::*;

#[test]
fn first_country_selection_immediately_starts_and_saves_a_live_personal_defense() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    h.click(f, View::Dossier, "Start journey");
    assert_eq!(f.game.campaign.world.start.as_deref(), Some("DEU"));
    assert!(f.screen == Screen::Battle);
    assert!(!f.battle.construction);
    let a = f.game.attempt.as_ref().unwrap();
    assert!(!a.paused);
    assert_eq!(a.tick, 0);
    assert_eq!(a.country_id.as_deref(), Some("DEU"));
    assert_eq!(a.mode, Mode::Personal);
    let restored = Game::load(&f.paths.campaign()).unwrap();
    assert_eq!(restored.attempt.as_ref().unwrap().tick, 0);
    assert!(!restored.attempt.as_ref().unwrap().paused);
}

#[test]
fn camp_escape_and_footer_control_pause_and_resume_all_parallel_work() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.game.campaign.choose_start_country("DEU").unwrap();
    f.game.campaign.attempts = 1;
    f.game.campaign.coins = 500;
    f.game.campaign.first_clears = [true; 5];
    f.game.campaign.unlocked = 5;
    for id in ["DEU", "FRA"] {
        f.game.campaign.world.cleared.push(id.into());
        f.game
            .campaign
            .world
            .country_levels
            .insert(id.into(), world().country(id).unwrap().level_count());
    }
    let recruit = f
        .game
        .strategy
        .recruit(&mut f.game.campaign, "DEU")
        .unwrap();
    f.game.deploy_survivor(recruit, "POL", 0).unwrap();
    h.key(f, View::Camp, Key::Escape);
    assert!(f.game.global_paused);
    let frozen = serde_json::to_value(&f.game).unwrap();
    for _ in 0..60 {
        f.game.tick(Input::default());
    }
    assert_eq!(serde_json::to_value(&f.game).unwrap(), frozen);
    h.click(f, View::Camp, "Resume world clock");
    assert!(!f.game.global_paused);
    f.game.tick(Input::default());
    assert!(f.game.battles[0].game.attempt.as_ref().unwrap().tick > 0);
}

#[test]
fn camp_pause_binding_does_not_steal_text_from_country_search() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.settings.bindings[Action::Pause as usize] = "P".into();
    h.frame(f, View::Camp, vec![]);
    h.ctx
        .memory_mut(|memory| memory.request_focus(egui::Id::new("country_search")));
    h.frame(
        f,
        View::Camp,
        vec![key_event(Key::P, true), Event::Text("p".into())],
    );
    assert_eq!(f.campaign.search, "p");
    assert!(!f.game.global_paused);
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
    f.game.set_paused(true);
    f.game.strategy.peak_freed = 4;
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
    assert!(f.game.attempt.is_none() && !f.game.global_paused);
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
