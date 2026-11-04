//! Egui harness tests: settings screen, bindings and difficulty.
use super::*;

#[test]
fn settings_from_main_menu_save_back_for_fresh_and_existing_profiles() {
    for existing in [false, true] {
        let mut fixture = IsolatedFrontend::new();
        let h = Harness::new();
        let f = &mut fixture.frontend;
        if existing {
            f.game.campaign.choose_start_country("DEU").unwrap();
            f.game.save(&f.paths.campaign()).unwrap();
            f.game = Game::load(&f.paths.campaign()).unwrap();
        }
        f.screen = Screen::Menu;
        h.click(f, View::Menu, "Settings");
        assert!(f.screen == Screen::Settings);
        assert!(f.nav.return_screen == Screen::Menu);
        f.settings.master = 0.23;
        h.click(f, View::Settings, "Save and return to main menu");
        assert!(f.screen == Screen::Menu, "{}", f.status.message);
        assert!(!f.status.message_error);
        assert!(!f.status.save_pending);
        let saved = Game::load(&f.paths.campaign()).unwrap();
        assert_eq!(saved.campaign.world.start.is_some(), existing);
        assert!(saved.global_paused);
        assert_eq!(Settings::load(&f.paths.settings()).master, 0.23);
        h.click(f, View::Menu, "Settings");
        h.key(f, View::Settings, Key::Escape);
        assert!(f.screen == Screen::Menu, "{}", f.status.message);
    }
}

#[test]
fn settings_name_the_origin_and_offer_a_durable_route_to_main_menu() {
    for origin in [Screen::Battle, Screen::Camp] {
        let mut fixture = IsolatedFrontend::new();
        fixture.deploy();
        let h = Harness::new();
        let f = &mut fixture.frontend;
        // A real reload is in progress; navigating must not reset its clock.
        f.game.attempt.as_mut().unwrap().ammo = 3;
        f.game.tick(Input {
            reload: true,
            ..Input::default()
        });
        assert!(f.game.attempt.as_ref().unwrap().reload_left > 0);
        f.screen = origin;
        f.battle.fire_latched = true;
        h.click(f, View::Header, "Settings");
        assert!(f.screen == Screen::Settings);
        assert!(f.nav.return_screen == origin);
        assert!(f.game.global_paused);
        assert!(!f.battle.fire_latched);
        let paused = serde_json::to_value(&f.game).unwrap();
        let output = h.frame(f, View::Settings, vec![]);
        assert!(
            output
                .shapes
                .iter()
                .any(|shape| { shape_has_text(&shape.shape, f.ctx().settings_return_label()) })
        );
        assert!(
            output
                .shapes
                .iter()
                .any(|shape| { shape_has_text(&shape.shape, "Save and return to main menu") })
        );
        h.click(f, View::Settings, "Save and return to main menu");
        assert!(f.screen == Screen::Menu, "{}", f.status.message);
        assert!(f.game.global_paused);
        assert_eq!(serde_json::to_value(&f.game).unwrap(), paused);
        let saved = Game::load(&f.paths.campaign()).unwrap();
        assert_eq!(serde_json::to_value(&saved).unwrap(), paused);
        h.click(f, View::Menu, "Resume journey");
        assert!(f.screen == Screen::Battle);
        assert!(f.game.global_paused);
        assert_eq!(serde_json::to_value(&f.game).unwrap(), paused);
    }
}

#[test]
fn settings_return_to_camp_restores_its_previous_world_pause() {
    for paused in [false, true] {
        let mut fixture = IsolatedFrontend::new();
        let h = Harness::new();
        let f = &mut fixture.frontend;
        f.game.set_paused(paused);
        h.click(f, View::Header, "Settings");
        assert!(f.game.global_paused);
        h.click(f, View::Settings, "Save and return to camp");
        assert!(f.screen == Screen::Camp, "{}", f.status.message);
        // The successful save queues restoration through the runtime.
        f.flush_for_test();
        assert_eq!(f.game.global_paused, paused);
    }
}

#[test]
fn settings_save_failure_is_visible_and_never_claims_a_return_or_success() {
    for fail_settings in [false, true] {
        let mut fixture = IsolatedFrontend::new();
        let h = Harness::new();
        let f = &mut fixture.frontend;
        f.screen = Screen::Menu;
        h.click(f, View::Menu, "Settings");
        // A file cannot be used as a parent directory, on any supported OS.
        std::fs::create_dir_all(f.paths.campaign().parent().unwrap()).unwrap();
        let blocker = fixture.directory.join("blocked");
        std::fs::write(&blocker, b"blocked").unwrap();
        if fail_settings {
            f.paths = ProfilePaths::files(f.paths.campaign(), blocker.join("settings.json"));
        } else {
            f.paths = ProfilePaths::files(blocker.join("campaign.json"), f.paths.settings());
        }
        h.click(f, View::Settings, "Save and return to main menu");
        assert!(f.screen == Screen::Settings);
        assert!(f.status.message_error);
        assert!(!f.status.save_pending);
        assert!(f.status.message.starts_with("Save failed:"));
        let output = h.frame(f, View::Settings, vec![]);
        assert!(
            output
                .shapes
                .iter()
                .any(|shape| { shape_has_text(&shape.shape, &f.status.message) })
        );
        std::fs::remove_file(blocker).unwrap();
        f.paths = ProfilePaths::in_directory(fixture.directory.clone());
        h.click(f, View::Settings, "Save and return to main menu");
        assert!(f.screen == Screen::Menu, "{}", f.status.message);
        assert!(!f.status.message_error);
    }
}

#[test]
fn binding_capture_ignores_its_activation_click_and_swaps_duplicates() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.screen = Screen::Settings;
    h.click(f, View::Settings, "R");
    assert!(f.prefs.rebind == Some(Action::Reload));
    assert_eq!(f.settings.bindings[Action::Reload as usize], "R");
    h.frame(f, View::Settings, vec![]);
    h.key(f, View::Settings, Key::B);
    assert!(f.prefs.rebind.is_none());
    assert_eq!(f.settings.bindings[Action::Reload as usize], "B");
    assert_eq!(f.settings.bindings[Action::Build as usize], "R");
    h.click(f, View::Settings, "Save and return to camp");
    assert!(f.screen == Screen::Camp);
    let saved = Settings::load(&f.paths.settings());
    assert_eq!(saved.bindings[Action::Reload as usize], "B");
    fixture.unlock_trap();
    fixture.deploy();
    let f = &mut fixture.frontend;
    let output = h.frame(f, View::Battle, vec![]);
    assert!(
        output
            .shapes
            .iter()
            .any(|s| shape_has_text(&s.shape, "B reload"))
    );
    assert!(
        output
            .shapes
            .iter()
            .any(|s| shape_has_text(&s.shape, "R build"))
    );
    h.key(f, View::Battle, Key::R);
    assert!(f.battle.construction);
    assert!(!f.game.global_paused);
}

#[test]
fn field_guidance_defaults_on_for_old_settings_and_is_saved_when_changed() {
    let settings: Settings = serde_json::from_str("{}").unwrap();
    assert!(settings.show_guidance);
    let mut fixture = IsolatedFrontend::new();
    let f = &mut fixture.frontend;
    f.settings.show_guidance = false;
    f.act(|c| c.request_save(SaveIntent::Manual));
    f.flush_for_test();
    assert!(!Settings::load(&f.paths.settings()).show_guidance);
}

#[test]
fn reduced_review_override_is_isolated_and_cannot_change_authority_or_saved_preferences() {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let f = &mut fixture.frontend;
    let authority = serde_json::to_value(&f.game).unwrap();
    for (scenario, practice, force, expected) in [
        (false, false, true, false),
        (true, true, true, false),
        (true, false, false, false),
        (true, false, true, true),
    ] {
        f.settings.reduced_effects = false;
        let mut settings = f.settings.clone();
        settings.reduced_effects = expected;
        review::apply_reduced_override(f, scenario, practice, force);
        assert_eq!(
            serde_json::to_value(&f.settings).unwrap(),
            serde_json::to_value(settings).unwrap()
        );
        assert_eq!(serde_json::to_value(&f.game).unwrap(), authority);
    }
    f.settings.reduced_effects = true;
    review::apply_reduced_override(f, false, false, false);
    assert!(
        f.settings.reduced_effects,
        "An ordinary profile keeps its existing accessibility preference"
    );
}

#[test]
fn difficulty_apply_is_a_validated_saved_command_and_keeps_active_geometry_frozen() {
    use horde_core::balance::{BalancePreset, BalanceSettings};
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    let original = f.game.attempt.as_ref().unwrap().balance.clone();
    assert_eq!(
        (
            f.game.attempt.as_ref().unwrap().lane_count,
            f.game.attempt.as_ref().unwrap().width_yards
        ),
        (11, 30.0)
    );
    f.act(|c| c.open_settings());
    f.flush_for_test();
    let active_snapshot = serde_json::to_value(f.game.attempt.as_ref().unwrap()).unwrap();
    h.click(f, View::Settings, "Normal");
    assert_eq!(f.game.campaign.balance_settings, original);
    h.click(f, View::Settings, "Apply to next run");
    assert_eq!(
        f.game.campaign.balance_settings,
        BalanceSettings::for_preset(BalancePreset::Normal)
    );
    let a = f.game.attempt.as_ref().unwrap();
    assert_eq!((a.lane_count, a.width_yards), (11, 30.0));
    assert_eq!(a.balance, original);
    assert!(a.paused);
    assert_eq!(serde_json::to_value(a).unwrap(), active_snapshot);
    h.click(f, View::Settings, "Save and return to defense");
    let restored = Game::load(&f.paths.campaign()).unwrap();
    assert_eq!(
        restored.campaign.balance_settings,
        f.game.campaign.balance_settings
    );
    assert_eq!(restored.attempt.as_ref().unwrap().width_yards, 30.0);
    let before = serde_json::to_value(&f.game).unwrap();
    let invalid = BalanceSettings {
        enemy_speed: f32::NAN,
        ..BalanceSettings::default()
    };
    f.act(|c| c.dispatch(Command::SetBalance(invalid), ""));
    f.flush_for_test();
    assert!(f.status.message_error);
    assert_eq!(serde_json::to_value(&f.game).unwrap(), before);
    f.game.withdraw();
    f.game.deploy_country_stage(0, Mode::Personal).unwrap();
    let a = f.game.attempt.as_ref().unwrap();
    assert_eq!((a.lane_count, a.width_yards), (3, 12.0));
}

#[test]
fn older_autosave_ack_keeps_navigation_busy_and_freezes_raw_settings_binding_capture() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    let gate = f.runtime.gated_storage_for_test();
    f.act(|c| c.dispatch(AppCommand::Save(SaveIntent::Autosave), ""));
    assert!(!f.flush());
    gate.started();
    f.act(|c| c.open_settings());
    f.act(|c| c.request_save(SaveIntent::SettingsBack));
    assert!(!f.flush());
    gate.release();
    assert!(!f.poll_one_save_for_test());
    gate.started();
    assert!(f.screen == Screen::Settings);
    assert!(f.status.save_pending);
    assert!(f.runtime.durable_transition_pending());
    f.prefs.rebind = Some(Action::Fire);
    f.prefs.binding_armed = true;
    let settings = f.settings.clone();
    let mut output = h.ctx.run_ui(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))),
            events: vec![key_event(Key::X, true)],
            ..Default::default()
        },
        |ui| settings_ui(ui, &mut f.ctx()),
    );
    output.textures_delta.clear();
    assert_eq!(f.settings.bindings, settings.bindings);
    assert_eq!(f.settings.scale, settings.scale);
    assert!(f.status.save_pending);
    gate.release();
    assert!(!f.poll_one_save_for_test());
    assert!(f.screen == Screen::Camp);
    assert!(!f.status.save_pending);
}
