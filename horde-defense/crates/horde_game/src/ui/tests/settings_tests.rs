//! Egui harness tests: settings screen, bindings and difficulty.
use super::*;

#[test]
fn settings_from_main_menu_save_back_for_fresh_and_existing_profiles() {
    for existing in [false, true] {
        let mut fixture = IsolatedFrontend::new();
        let h = Harness::new();
        let f = &mut fixture.frontend;
        if existing {
            f.game.campaign_mut().choose_start_country("DEU").unwrap();
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
        assert_eq!(saved.campaign().world.start.is_some(), existing);
        assert!(saved.global_paused());
        assert_eq!(Settings::load(&f.paths.settings()).master, 0.23);
        h.click(f, View::Menu, "Settings");
        // W3-04 rewrite: Escape is routed by draw_screen (ui/escape.rs), so
        // the press goes through the whole frame.
        h.key(f, View::Full, Key::Escape);
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
        f.game.attempt_mut().unwrap().ammo = 3;
        f.game.tick(Input {
            reload: true,
            ..Input::default()
        });
        assert!(f.game.attempt().unwrap().reload_left > 0);
        f.screen = origin;
        f.battle.fire_latched = true;
        h.click(f, View::Header, "Settings");
        assert!(f.screen == Screen::Settings);
        assert!(f.nav.return_screen == origin);
        assert!(f.game.global_paused());
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
        assert!(f.game.global_paused());
        assert_eq!(serde_json::to_value(&f.game).unwrap(), paused);
        let saved = Game::load(&f.paths.campaign()).unwrap();
        assert_eq!(serde_json::to_value(&saved).unwrap(), paused);
        h.click(f, View::Menu, "Resume journey");
        assert!(f.screen == Screen::Battle);
        assert!(f.game.global_paused());
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
        assert!(f.game.global_paused());
        h.click(f, View::Settings, "Save and return to camp");
        assert!(f.screen == Screen::Camp, "{}", f.status.message);
        // The successful save queues restoration through the runtime.
        f.flush_for_test();
        assert_eq!(f.game.global_paused(), paused);
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
    assert!(!f.game.global_paused());
}

#[test]
fn binding_capture_swaps_a_core_key_with_an_extra_key() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.screen = Screen::Settings;
    assert_eq!(f.settings.extra_bindings.world_clock, "P");
    h.click(f, View::Settings, "R");
    assert!(f.prefs.rebind == Some(Action::Reload));
    h.frame(f, View::Settings, vec![]);
    h.key(f, View::Settings, Key::P);
    assert!(f.prefs.rebind.is_none());
    assert_eq!(f.settings.bindings[Action::Reload as usize], "P");
    assert_eq!(f.settings.extra_bindings.world_clock, "R");
}

/// The frontend as startup builds it from the files already in `directory`.
fn frontend_loaded_from(directory: &std::path::Path) -> FrontendParts {
    let paths = ProfilePaths::in_directory(directory.to_path_buf());
    let startup = crate::profile::read_settings(&paths.settings());
    let profile = crate::profile::LoadedProfile::load(&paths, startup);
    FrontendParts::new(profile, &crate::launch::LaunchConfig::default(), &paths)
}

#[test]
fn startup_settings_are_read_once_and_reused() {
    let fixture = IsolatedFrontend::new();
    let directory = fixture.directory.clone();
    std::fs::create_dir_all(&directory).unwrap();
    let config = crate::launch::LaunchConfig {
        save_directory: directory.clone(),
        ..crate::launch::LaunchConfig::default()
    };
    let path = ProfilePaths::from_config(&config).settings();
    let startup_value = Settings {
        master: 0.3,
        low_latency: true,
        ..Settings::default()
    };
    startup_value.save(&path).unwrap();
    // `main`: the one read, before the App, which the render plan also uses.
    let startup = crate::profile::read_settings(&path);
    let plan = crate::render_plan::render_plan(&startup.settings, config.perf());
    assert!(!plan.pipelined);
    // The file changes after that read; startup never reads it again.
    Settings {
        master: 0.9,
        ..Settings::default()
    }
    .save(&path)
    .unwrap();
    let f = FrontendParts::load(&config, startup);
    assert_eq!(f.settings, startup_value);
    // Later saves still go to the same profile file.
    assert_eq!(f.paths.settings(), path);
    std::fs::remove_file(&path).unwrap();
}

/// A manual save in its own frame (a frame enqueues one save of an intent),
/// waited for.
fn save_in_frame(f: &mut FrontendParts, frame: u32) {
    f.runtime.begin_frame(frame);
    f.act(|c| c.request_save(SaveIntent::Manual));
    f.flush_for_test();
}

#[test]
fn settings_copy_failure_at_startup_keeps_the_original_until_a_save_can_copy_it() {
    let mut fixture = IsolatedFrontend::new();
    let directory = fixture.directory.clone();
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("settings.json");
    let original = b"{\"master\": 0.3, truncated by a crash";
    std::fs::write(&path, original).unwrap();
    // A directory where the copy belongs: creating the copy fails.
    let copy = directory.join(crate::profile::UNREADABLE_SETTINGS);
    std::fs::create_dir(&copy).unwrap();
    fixture.frontend = frontend_loaded_from(&directory);
    let f = &mut fixture.frontend;
    assert_eq!(f.settings, Settings::default());
    f.game.campaign_mut().choose_start_country("DEU").unwrap();
    save_in_frame(f, 1);
    assert_eq!(std::fs::read(&path).unwrap(), original);
    // Only the settings write waits for the copy: the campaign is saved.
    let saved = Game::load(&f.paths.campaign()).unwrap();
    assert!(saved.campaign().world.start.is_some());
    assert!(
        f.status
            .message
            .starts_with("Campaign saved; settings not saved:"),
        "{}",
        f.status.message
    );
    assert!(f.status.message_error);
    // Once the copy can be made, the next save makes it before replacing.
    std::fs::remove_dir(&copy).unwrap();
    save_in_frame(f, 2);
    assert_eq!(std::fs::read(&copy).unwrap(), original);
    assert_eq!(Settings::load(&path), Settings::default());
    // Later saves replace only settings.json; the copy keeps the original.
    f.settings.master = 0.4;
    save_in_frame(f, 3);
    assert_eq!(std::fs::read(&copy).unwrap(), original);
    assert_eq!(Settings::load(&path).master, 0.4);
}

#[test]
fn settings_copy_failure_at_startup_does_not_block_quit() {
    let mut fixture = IsolatedFrontend::new();
    let directory = fixture.directory.clone();
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("settings.json");
    let original = b"{\"master\": 0.3, truncated by a crash";
    std::fs::write(&path, original).unwrap();
    let copy = directory.join(crate::profile::UNREADABLE_SETTINGS);
    std::fs::create_dir(&copy).unwrap();
    fixture.frontend = frontend_loaded_from(&directory);
    let f = &mut fixture.frontend;
    f.game.campaign_mut().choose_start_country("DEU").unwrap();
    f.runtime.begin_frame(1);
    f.act(|c| c.request_save(SaveIntent::Quit));
    assert!(f.flush_for_test(), "{}", f.status.message);
    assert!(
        Game::load(&f.paths.campaign())
            .unwrap()
            .campaign()
            .world
            .start
            .is_some()
    );
    assert_eq!(std::fs::read(&path).unwrap(), original);
    std::fs::remove_dir(&copy).unwrap();
}

#[test]
fn settings_read_failure_at_startup_copies_the_original_before_the_first_save() {
    let mut fixture = IsolatedFrontend::new();
    let directory = fixture.directory.clone();
    let path = directory.join("settings.json");
    // A directory cannot be read as a file: the startup read fails with an
    // error other than NotFound, as a sharing violation or a denied read does.
    std::fs::create_dir_all(&path).unwrap();
    fixture.frontend = frontend_loaded_from(&directory);
    // The file becomes readable again, holding the player's real settings.
    std::fs::remove_dir(&path).unwrap();
    let real = Settings {
        master: 0.25,
        ..Settings::default()
    };
    real.save(&path).unwrap();
    let original = std::fs::read(&path).unwrap();
    let f = &mut fixture.frontend;
    assert_eq!(f.settings, Settings::default());
    save_in_frame(f, 1);
    let copy = directory.join(crate::profile::UNREADABLE_SETTINGS);
    assert_eq!(std::fs::read(&copy).ok(), Some(original.clone()));
    // Later saves never copy this session's own writes over the original.
    f.settings.master = 0.4;
    save_in_frame(f, 2);
    assert_eq!(std::fs::read(&copy).unwrap(), original);
    assert_eq!(Settings::load(&path).master, 0.4);
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
    let original = f.game.attempt().unwrap().balance.clone();
    assert_eq!(
        (
            f.game.attempt().unwrap().lane_count,
            f.game.attempt().unwrap().width_yards
        ),
        (11, 30.0)
    );
    f.act(|c| c.open_settings());
    f.flush_for_test();
    let active_snapshot = serde_json::to_value(f.game.attempt().unwrap()).unwrap();
    h.click(f, View::Settings, "Normal");
    assert_eq!(f.game.campaign().balance_settings, original);
    h.click(f, View::Settings, "Apply to next run");
    assert_eq!(
        f.game.campaign().balance_settings,
        BalanceSettings::for_preset(BalancePreset::Normal)
    );
    let a = f.game.attempt().unwrap();
    assert_eq!((a.lane_count, a.width_yards), (11, 30.0));
    assert_eq!(a.balance, original);
    assert!(a.paused);
    assert_eq!(serde_json::to_value(a).unwrap(), active_snapshot);
    h.click(f, View::Settings, "Save and return to defense");
    let restored = Game::load(&f.paths.campaign()).unwrap();
    assert_eq!(
        restored.campaign().balance_settings,
        f.game.campaign().balance_settings
    );
    assert_eq!(restored.attempt().unwrap().width_yards, 30.0);
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
    let a = f.game.attempt().unwrap();
    assert_eq!((a.lane_count, a.width_yards), (3, 12.0));
}

/// Every painted text run with its screen rect.
fn text_rects(output: &FullOutput) -> Vec<(String, Rect)> {
    fn walk(shape: &egui::epaint::Shape, out: &mut Vec<(String, Rect)>) {
        match shape {
            egui::epaint::Shape::Text(text) => out.push((
                text.galley.text().to_owned(),
                text.galley.rect.translate(text.pos.to_vec2()),
            )),
            egui::epaint::Shape::Vec(shapes) => shapes.iter().for_each(|s| walk(s, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    output.shapes.iter().for_each(|c| walk(&c.shape, &mut out));
    out
}

/// Presses and releases the rail of the slider labelled `label`, 30 px left
/// of its value field, as a player clicking the rail would.
fn press_slider_rail(h: &Harness, f: &mut FrontendParts, label: &str) {
    let texts = text_rects(&h.frame(f, View::Settings, vec![]));
    let (_, label_rect) = texts
        .iter()
        .find(|(text, _)| text == label)
        .unwrap_or_else(|| panic!("Slider not found: {label}"));
    // The value field's text runs ("1.45", "×") sit between rail and label.
    let value_left = texts
        .iter()
        .filter(|(_, r)| {
            (r.center().y - label_rect.center().y).abs() < 2.0 && r.max.x < label_rect.min.x
        })
        .map(|(_, r)| r.min.x)
        .min_by(f32::total_cmp)
        .expect("slider value field");
    let position = pos2(value_left - 30.0, label_rect.center().y);
    for pressed in [true, false] {
        h.frame(
            f,
            View::Settings,
            vec![
                Event::PointerMoved(position),
                Event::PointerButton {
                    pos: position,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                },
            ],
        );
    }
}

/// W1-29 review: a named preset edited on the settings screen keeps that
/// preset's extras. The draft keeps the named preset it came from, and the
/// core normalises it to Custom with that preset as the extras basis.
#[test]
fn editing_a_named_preset_on_the_settings_screen_keeps_its_extras() {
    use horde_core::balance::{BalancePreset, BalanceSettings};
    // Committed Normal; pick Hard, drag a slider, apply.
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.game
        .apply(&Command::SetBalance(BalanceSettings::for_preset(
            BalancePreset::Normal,
        )))
        .unwrap();
    f.act(|c| c.open_settings());
    f.flush_for_test();
    h.click(f, View::Settings, "Hard");
    press_slider_rail(&h, f, "Zombie speed");
    let draft = f.prefs.balance_draft.clone().unwrap();
    assert_ne!(
        draft.enemy_speed,
        BalanceSettings::for_preset(BalancePreset::Hard).enemy_speed,
        "the slider did not move"
    );
    h.click(f, View::Settings, "Apply to next run");
    let campaign = f.game.campaign();
    assert_eq!(campaign.balance_settings.enemy_speed, draft.enemy_speed);
    assert_eq!(campaign.balance_settings.preset, BalancePreset::Custom);
    assert_eq!(campaign.balance_extras_preset(), BalancePreset::Hard);
    // Once applied, the same draft is nothing new to apply.
    let output = h.frame(f, View::Settings, vec![]);
    assert_eq!(
        text_count(
            &output,
            "Unapplied draft. Apply before saving and returning."
        ),
        0
    );

    // Committed Custom from Hard; pick Easy, drag a slider, apply.
    h.click(f, View::Settings, "Easy");
    press_slider_rail(&h, f, "Zombie speed");
    assert_ne!(
        f.prefs.balance_draft.as_ref().unwrap().enemy_speed,
        BalanceSettings::for_preset(BalancePreset::Easy).enemy_speed,
        "the slider did not move"
    );
    h.click(f, View::Settings, "Apply to next run");
    let campaign = f.game.campaign();
    assert_eq!(campaign.balance_settings.preset, BalancePreset::Custom);
    assert_eq!(campaign.balance_extras_preset(), BalancePreset::Easy);
}

/// W1 review: clicking 'Custom' after a named preset must not drop the preset
/// the draft came from, or the edit saves the committed preset's extras.
#[test]
fn choosing_custom_after_a_named_preset_keeps_its_extras() {
    use horde_core::balance::{BalancePreset, BalanceSettings};
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.game
        .apply(&Command::SetBalance(BalanceSettings::for_preset(
            BalancePreset::Normal,
        )))
        .unwrap();
    f.act(|c| c.open_settings());
    f.flush_for_test();
    h.click(f, View::Settings, "Hard");
    h.click(f, View::Settings, "Custom");
    press_slider_rail(&h, f, "Zombie speed");
    let draft = f.prefs.balance_draft.clone().unwrap();
    assert_ne!(
        draft.enemy_speed,
        BalanceSettings::for_preset(BalancePreset::Hard).enemy_speed,
        "the slider did not move"
    );
    h.click(f, View::Settings, "Apply to next run");
    let campaign = f.game.campaign();
    assert_eq!(campaign.balance_settings.enemy_speed, draft.enemy_speed);
    assert_eq!(campaign.balance_settings.preset, BalancePreset::Custom);
    assert_eq!(campaign.balance_extras_preset(), BalancePreset::Hard);

    // Choosing Custom over an unedited preset highlights Custom, and picking
    // a named preset afterwards highlights that preset again.
    f.act(|c| c.open_settings());
    f.flush_for_test();
    h.click(f, View::Settings, "Easy");
    h.click(f, View::Settings, "Custom");
    assert!(f.prefs.balance_custom_chosen);
    h.click(f, View::Settings, "Normal");
    assert!(!f.prefs.balance_custom_chosen);
    assert_eq!(
        f.prefs.balance_draft.as_ref().unwrap(),
        &BalanceSettings::for_preset(BalancePreset::Normal)
    );
}

/// R21 (wave-1 close): the Custom chip keeps the basis of the preset it was
/// clicked from, so Hard then Custom without an edit applies Custom on the
/// Hard basis instead of Hard itself, and is then nothing new to apply.
#[test]
fn choosing_custom_over_an_unedited_preset_applies_custom_on_its_basis() {
    use horde_core::balance::{BalancePreset, BalanceSettings};
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.game
        .apply(&Command::SetBalance(BalanceSettings::for_preset(
            BalancePreset::Normal,
        )))
        .unwrap();
    f.act(|c| c.open_settings());
    f.flush_for_test();
    h.click(f, View::Settings, "Hard");
    h.click(f, View::Settings, "Custom");
    h.click(f, View::Settings, "Apply to next run");
    let campaign = f.game.campaign();
    assert_eq!(
        campaign.balance_settings,
        BalanceSettings {
            preset: BalancePreset::Custom,
            ..BalanceSettings::for_preset(BalancePreset::Hard)
        }
    );
    assert_eq!(campaign.balance_extras_preset(), BalancePreset::Hard);
    let output = h.frame(f, View::Settings, vec![]);
    assert_eq!(
        text_count(
            &output,
            "Unapplied draft. Apply before saving and returning."
        ),
        0
    );
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

#[test]
fn escape_cancels_a_rebind_instead_of_binding_escape() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.screen = Screen::Settings;
    h.click(f, View::Settings, "R");
    assert!(f.prefs.rebind == Some(Action::Reload));
    h.frame(f, View::Settings, vec![]);
    h.key(f, View::Settings, Key::Escape);
    assert!(f.prefs.rebind.is_none());
    assert_eq!(f.settings.bindings[Action::Reload as usize], "R");
    assert_eq!(f.settings.bindings[Action::Pause as usize], "Escape");
}

/// The labels of the last frame's modal primaries.
fn modal_primaries(h: &Harness) -> Vec<String> {
    h.primaries()
        .into_iter()
        .filter(|record| record.surface == Surface::Modal)
        .map(|record| record.label)
        .collect()
}

/// Settings opened from the camp on a committed Normal, with the Hard preset
/// chosen and not applied.
fn settings_with_unapplied_hard(h: &Harness, f: &mut FrontendParts) {
    use horde_core::balance::{BalancePreset, BalanceSettings};
    f.game
        .apply(&Command::SetBalance(BalanceSettings::for_preset(
            BalancePreset::Normal,
        )))
        .unwrap();
    f.act(|c| c.open_settings());
    f.flush_for_test();
    assert!(f.screen == Screen::Settings);
    h.click(f, View::Full, "Hard");
    assert_eq!(
        f.prefs.balance_draft,
        Some(BalanceSettings::for_preset(BalancePreset::Hard))
    );
}

/// W3-07 (UX-09, G2.6): Escape on Settings with an unapplied difficulty
/// draft opens the draft warning instead of saving and dropping the draft.
/// Escape again keeps editing; "Discard and return" and "Apply and return"
/// leave by the route the press asked for.
#[test]
fn dirty_draft_escape_opens_warning() {
    use crate::ui::modal::{
        APPLY_AND_RETURN, AppModal, DISCARD_AND_RETURN, DRAFT_WARNING_TITLE, KEEP_EDITING,
        LeaveSettings,
    };
    use horde_core::balance::{BalancePreset, BalanceSettings};
    let normal = BalanceSettings::for_preset(BalancePreset::Normal);
    let hard = BalanceSettings::for_preset(BalancePreset::Hard);
    let warning = Some(AppModal::DraftWarning {
        then: LeaveSettings::Back,
    });
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    settings_with_unapplied_hard(&h, f);

    h.key(f, View::Full, Key::Escape);
    assert_eq!(f.status.modal, warning);
    assert!(f.screen == Screen::Settings);
    assert!(!f.status.save_pending, "nothing is saved yet");
    assert_eq!(f.game.campaign().balance_settings, normal);
    h.frame(f, View::Full, vec![]);
    let output = h.frame(f, View::Full, vec![]);
    assert_eq!(text_count(&output, DRAFT_WARNING_TITLE), 1);
    assert_eq!(text_count(&output, DISCARD_AND_RETURN), 1);
    assert_eq!(text_count(&output, KEEP_EDITING), 1);
    assert_eq!(modal_primaries(&h), [APPLY_AND_RETURN]);

    // Escape keeps editing: the modal closes and the draft is still there.
    h.key(f, View::Full, Key::Escape);
    assert_eq!(f.status.modal, None);
    assert!(f.screen == Screen::Settings);
    assert_eq!(f.prefs.balance_draft, Some(hard.clone()));
    // So does "Keep editing".
    h.key(f, View::Full, Key::Escape);
    h.click(f, View::Full, KEEP_EDITING);
    assert_eq!(f.status.modal, None);
    assert!(f.screen == Screen::Settings);
    assert_eq!(f.prefs.balance_draft, Some(hard.clone()));

    // "Discard and return" drops the draft and saves back to the camp.
    h.key(f, View::Full, Key::Escape);
    assert_eq!(f.status.modal, warning);
    h.click(f, View::Full, DISCARD_AND_RETURN);
    assert_eq!(f.status.modal, None);
    assert!(f.screen == Screen::Camp, "{}", f.status.message);
    assert_eq!(f.game.campaign().balance_settings, normal);
    assert_eq!(f.prefs.balance_draft, Some(normal.clone()));
    let saved = Game::load(&f.paths.campaign()).unwrap();
    assert_eq!(saved.campaign().balance_settings, normal);

    // "Apply and return" applies the draft before the save, then returns.
    settings_with_unapplied_hard(&h, f);
    h.key(f, View::Full, Key::Escape);
    assert_eq!(f.status.modal, warning);
    h.click(f, View::Full, APPLY_AND_RETURN);
    assert_eq!(f.status.modal, None);
    assert!(f.screen == Screen::Camp, "{}", f.status.message);
    assert_eq!(f.game.campaign().balance_settings, hard);
    let saved = Game::load(&f.paths.campaign()).unwrap();
    assert_eq!(saved.campaign().balance_settings, hard);

    // A draft applied on the screen is not unapplied: Escape just leaves.
    f.act(|c| c.open_settings());
    f.flush_for_test();
    h.click(f, View::Full, "Easy");
    h.click(f, View::Full, "Apply to next run");
    h.key(f, View::Full, Key::Escape);
    assert_eq!(f.status.modal, None);
    assert!(f.screen == Screen::Camp, "{}", f.status.message);
    assert_eq!(
        f.game.campaign().balance_settings,
        BalanceSettings::for_preset(BalancePreset::Easy)
    );
}

/// W3-07: the two leave buttons meet the same warning, and it remembers
/// which route to finish.
#[test]
fn dirty_draft_leave_buttons_open_warning_for_their_route() {
    use crate::ui::modal::{APPLY_AND_RETURN, AppModal, KEEP_EDITING, LeaveSettings};
    use horde_core::balance::{BalancePreset, BalanceSettings};
    let hard = BalanceSettings::for_preset(BalancePreset::Hard);
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    settings_with_unapplied_hard(&h, f);

    // The amber primary.
    h.click(f, View::Full, "Save and return to camp");
    assert_eq!(
        f.status.modal,
        Some(AppModal::DraftWarning {
            then: LeaveSettings::Back
        })
    );
    assert!(f.screen == Screen::Settings);
    assert!(!f.status.save_pending);
    h.click(f, View::Full, KEEP_EDITING);
    assert_eq!(f.status.modal, None);

    // "Save and return to main menu".
    h.click(f, View::Full, "Save and return to main menu");
    assert_eq!(
        f.status.modal,
        Some(AppModal::DraftWarning {
            then: LeaveSettings::Menu
        })
    );
    assert!(f.screen == Screen::Settings);
    assert!(!f.status.save_pending);
    h.click(f, View::Full, APPLY_AND_RETURN);
    assert_eq!(f.status.modal, None);
    assert!(f.screen == Screen::Menu, "{}", f.status.message);
    let saved = Game::load(&f.paths.campaign()).unwrap();
    assert_eq!(saved.campaign().balance_settings, hard);
    assert!(saved.global_paused());
}

/// W3-07: a draft that fails validation cannot be applied from the warning;
/// it says why without a hover, and can still be discarded.
#[test]
fn invalid_draft_warning_disables_apply_and_still_discards() {
    use crate::ui::modal::{
        APPLY_AND_RETURN, AppModal, DISCARD_AND_RETURN, FIX_DRAFT_FIRST, LeaveSettings,
    };
    use horde_core::balance::{BalancePreset, BalanceSettings};
    let normal = BalanceSettings::for_preset(BalancePreset::Normal);
    let warning = Some(AppModal::DraftWarning {
        then: LeaveSettings::Back,
    });
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    settings_with_unapplied_hard(&h, f);
    let draft = f.prefs.balance_draft.as_mut().unwrap();
    draft.start_lanes = 17;
    draft.max_lanes = 3;
    assert!(draft.validate().is_err());

    h.key(f, View::Full, Key::Escape);
    assert_eq!(f.status.modal, warning);
    h.frame(f, View::Full, vec![]);
    let output = h.frame(f, View::Full, vec![]);
    assert_eq!(text_count(&output, FIX_DRAFT_FIRST), 1);
    // The disabled primary does nothing.
    h.click(f, View::Full, APPLY_AND_RETURN);
    assert_eq!(f.status.modal, warning);
    assert!(f.screen == Screen::Settings);
    assert!(!f.status.save_pending);
    assert_eq!(f.game.campaign().balance_settings, normal);

    h.click(f, View::Full, DISCARD_AND_RETURN);
    assert_eq!(f.status.modal, None);
    assert!(f.screen == Screen::Camp, "{}", f.status.message);
    assert_eq!(f.game.campaign().balance_settings, normal);
    assert_eq!(f.prefs.balance_draft, Some(normal));
}

/// The left edge and the centre height of the "Interface scale" slider label.
fn scale_label(output: &FullOutput) -> (f32, f32) {
    fn find(shape: &egui::epaint::Shape) -> Option<(f32, f32)> {
        match shape {
            egui::epaint::Shape::Text(text)
                if text.galley.text().starts_with("Interface scale") =>
            {
                Some((
                    text.pos.x + text.galley.rect.left(),
                    text.pos.y + text.galley.rect.center().y,
                ))
            }
            egui::epaint::Shape::Vec(shapes) => shapes.iter().find_map(find),
            _ => None,
        }
    }
    output
        .shapes
        .iter()
        .find_map(|clipped| find(&clipped.shape))
        .expect("the Interface scale slider is drawn")
}

/// W3-08 (UX-10, G2.6): dragging the interface-scale slider edits a draft;
/// `Settings::scale`, which `draw_screen` applies every frame, moves only
/// when the pointer is released. A plain click on the rail commits at once.
#[test]
fn scale_applies_only_on_release() {
    let button = |pos: Pos2, pressed: bool| Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.screen = Screen::Settings;
    let applied = f.settings.scale;
    let output = h.frame(f, View::Settings, vec![]);
    let (label_left, y) = scale_label(&output);
    // The rail starts at the panel's 20-point margin and is 100 points wide;
    // the value box and the label follow it.
    assert!(label_left > 130.0, "label at {label_left}");
    let (low, mid, high) = (pos2(30.0, y), pos2(70.0, y), pos2(112.0, y));

    // Press near the low end and drag: only the draft follows the pointer.
    h.frame(
        f,
        View::Settings,
        vec![Event::PointerMoved(low), button(low, true)],
    );
    h.frame(f, View::Settings, vec![Event::PointerMoved(mid)]);
    assert_eq!(f.settings.scale, applied, "the scale moved mid-drag");
    let first = f.prefs.scale_draft.expect("a held slider has a draft");
    h.frame(f, View::Settings, vec![Event::PointerMoved(high)]);
    let dragged = f.prefs.scale_draft.expect("a held slider has a draft");
    assert!(dragged > first, "{dragged} after {first}");
    assert_ne!(dragged, applied);
    assert_eq!(f.settings.scale, applied, "the scale moved mid-drag");

    let output = h.frame(f, View::Settings, vec![Event::PointerMoved(high)]);
    assert_eq!(text_count(&output, crate::ui::preferences::SCALE_LABEL), 1);

    // Release commits the draft and clears it.
    h.frame(
        f,
        View::Settings,
        vec![Event::PointerMoved(high), button(high, false)],
    );
    assert_eq!(f.settings.scale, dragged);
    assert_eq!(f.prefs.scale_draft, None);
    h.frame(f, View::Settings, vec![]);
    assert_eq!(f.settings.scale, dragged);

    // A click without a drag commits on its release too.
    h.press_at(f, View::Settings, low, h.target.pixels);
    assert!(f.settings.scale < dragged, "{}", f.settings.scale);
    assert_eq!(f.prefs.scale_draft, None);

    // A draft left behind (the screen was left mid-drag) is dropped, never
    // applied: by the next Settings frame, and when Settings opens.
    let committed = f.settings.scale;
    f.prefs.scale_draft = Some(1.3);
    h.frame(f, View::Settings, vec![]);
    assert_eq!(f.prefs.scale_draft, None);
    assert_eq!(f.settings.scale, committed);
    f.prefs.scale_draft = Some(1.3);
    f.act(|c| c.open_settings());
    assert_eq!(f.prefs.scale_draft, None);
    assert_eq!(f.settings.scale, committed);
}
