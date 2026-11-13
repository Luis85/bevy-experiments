//! Egui harness tests: results, debrief and progression flow.
use super::*;

#[test]
fn results_keep_rewards_banked_and_open_the_actual_recovered_equipment() {
    let mut fixture = IsolatedFrontend::new();
    fixture.frontend.game.campaign_mut().coins = nodes_price(&["steady_hands", "stopping_power"]);
    fixture
        .frontend
        .game
        .campaign_mut()
        .purchase_node("steady_hands")
        .unwrap();
    fixture
        .frontend
        .game
        .campaign_mut()
        .purchase_node("stopping_power")
        .unwrap();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    let mut attempt = f.game.attempt_slot_mut().take().unwrap();
    attempt.boss_spawned = true;
    f.game
        .spawn_enemy(&mut attempt, horde_core::content::EnemyKind::Brute, 5, true);
    let boss = attempt.enemies.last_mut().unwrap();
    boss.x = 8.0;
    boss.hp = 1;
    *f.game.attempt_slot_mut() = Some(attempt);
    f.game.tick(Input {
        aim: [8.0, 5.5],
        fire: true,
        ..Input::default()
    });
    assert!(f.game.attempt().is_none());
    assert!(f.game.last_result().unwrap().loot > 0);
    f.screen = Screen::Results;
    let coins = f.game.campaign().coins;
    for _ in 0..4 {
        let output = h.frame(f, View::Results, vec![]);
        assert!(
            output
                .shapes
                .iter()
                .any(|shape| shape_has_text(&shape.shape, " · banked +"))
        );
    }
    assert_eq!(f.game.campaign().coins, coins);
    h.click(f, View::Results, "Equipment");
    assert!(f.screen == Screen::Camp);
    assert!(f.nav.tab == Tab::Armory);
    assert!(f.armory.selected_item.is_some());
    assert_eq!(f.game.campaign().coins, coins);
}

#[test]
fn victory_stops_at_upgrades_until_continue_deploys_the_next_defense_without_repaying() {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.game.withdraw();
    f.game.last_result_mut().as_mut().unwrap().outcome = Outcome::Victory;
    f.game
        .campaign_mut()
        .world
        .country_levels
        .insert("DEU".into(), 1);
    f.screen = Screen::Results;
    f.game.set_paused(true);
    let before = serde_json::to_value(&f.game).unwrap();
    let coins = f.game.campaign().coins;
    let records = f.game.campaign().run_history.len();
    for _ in 0..3 {
        h.frame(f, View::Results, vec![]);
    }
    assert_eq!(serde_json::to_value(&f.game).unwrap(), before);
    assert!(f.game.attempt().is_none());
    h.click(f, View::Results, "Next defense");
    assert!(f.screen == Screen::Battle);
    let a = f.game.attempt().unwrap();
    assert_eq!(a.stage, 1);
    assert_eq!(a.tick, 0);
    assert!(!a.paused);
    assert_eq!(f.game.campaign().coins, coins);
    assert_eq!(f.game.campaign().run_history.len(), records);
}

#[test]
fn leaving_a_debrief_keeps_strategic_time_paused_until_explicitly_resumed() {
    let mut fixture = IsolatedFrontend::new();
    let f = &mut fixture.frontend;
    f.game.campaign_mut().choose_start_country("DEU").unwrap();
    f.screen = Screen::Results;
    f.game.set_global_paused(false);
    let mut expected = f.game.clone();
    expected.set_paused(true);
    f.act(|c| c.open_campaign());
    f.flush_for_test();
    assert!(f.screen == Screen::Camp && f.nav.tab == Tab::World);
    assert!(f.game.global_paused());
    assert_eq!(
        serde_json::to_value(&f.game).unwrap(),
        serde_json::to_value(&expected).unwrap()
    );
}

#[test]
fn retreat_opens_retained_reward_skill_tree_and_retry_starts_live() {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.game.campaign_mut().reward(25, 0, Mode::Personal);
    h.hold(f, View::Battle, "Retreat & train", 0.9);
    assert!(f.screen == Screen::Results);
    assert!(f.game.attempt().is_none());
    assert_eq!(f.game.campaign().coins, 25);
    h.click(f, View::Results, "Try again");
    assert!(f.screen == Screen::Battle);
    assert!(!f.game.attempt().unwrap().paused);
}

/// W3-02 rewrite: resuming a left journey lands on Camp home (it was the
/// world map); `resume()` falls back to Home (design-ux-flow §4).
#[test]
fn defeat_and_withdraw_each_reach_upgrade_home_and_leave_resume_returns_to_camp_home() {
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
    assert!(f.game.attempt().is_none());
    assert!(f.game.global_paused());
    assert!(f.game.campaign().journey_active);
    assert_eq!(f.game.last_result().unwrap().outcome, Outcome::Defeat);
    assert_eq!(f.runtime.simulated_ticks, 1);
    h.click(f, View::Results, "Try again");
    assert!(f.screen == Screen::Battle);
    assert_eq!(f.game.attempt().unwrap().stage, 0);
    h.hold(f, View::Battle, "Retreat & train", 0.9);
    assert!(f.screen == Screen::Results);
    assert_eq!(f.game.last_result().unwrap().outcome, Outcome::Withdrawal);
    let coins = f.game.campaign().coins;
    let records = f.game.campaign().run_history.len();
    h.click(f, View::Results, "Leave journey");
    assert!(f.screen == Screen::Menu);
    assert!(!f.game.campaign().journey_active);
    f.game = Game::load(&f.paths.campaign()).unwrap();
    h.click(f, View::Menu, "Resume journey");
    assert!(f.screen == Screen::Camp && f.nav.tab == Tab::Home);
    assert_eq!(f.game.campaign().coins, coins);
    assert_eq!(f.game.campaign().run_history.len(), records);
}

#[test]
fn liberated_country_continue_opens_map_and_does_not_autoselect_or_pay_again() {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.game.withdraw();
    f.game.last_result_mut().as_mut().unwrap().outcome = Outcome::Victory;
    let count = world().country("DEU").unwrap().level_count();
    f.game
        .campaign_mut()
        .world
        .country_levels
        .insert("DEU".into(), count);
    f.screen = Screen::Results;
    let coins = f.game.campaign().coins;
    let records = f.game.campaign().run_history.len();
    h.click(f, View::Results, "Choose next country");
    assert!(f.screen == Screen::Camp && f.nav.tab == Tab::World);
    assert!(f.game.attempt().is_none());
    assert!(!f.game.campaign().journey_active);
    assert_eq!(f.game.campaign().world.active.as_deref(), Some("DEU"));
    assert_eq!(f.game.campaign().coins, coins);
    assert_eq!(f.game.campaign().run_history.len(), records);
}

#[test]
fn the_upgrade_home_shares_the_camp_header_and_orders_its_actions() {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.game.withdraw();
    f.game.campaign_mut().coins = 42;
    f.screen = Screen::Results;
    let output = h.frame(f, View::ResultsWithChrome, vec![]);
    for label in [
        "LASTLIGHT",
        "Menu",
        "History",
        "Settings",
        "Journey upgrades",
    ] {
        assert_eq!(text_count(&output, label), 1, "{label} once");
    }
    assert_eq!(text_count(&output, "42 coins"), 1, "one wallet on screen");
    let x = |label: &str| {
        output
            .shapes
            .iter()
            .find_map(|s| labeled_bounds(&s.shape, label))
            .unwrap_or_else(|| panic!("{label} painted"))
            .left()
    };
    assert_eq!(
        text_count(&output, "Run history"),
        0,
        "the header has History"
    );
    let order = ["Try again", "More", "Leave journey"];
    for pair in order.windows(2) {
        assert!(x(pair[0]) < x(pair[1]), "{} before {}", pair[0], pair[1]);
    }
    // The camp row is drawn here too, so its destinations are not repeated
    // as action buttons; "Journey upgrades" is the selected tab, not a title.
    for label in ["Briefing", "Equipment", "Character", "World map"] {
        assert_eq!(text_count(&output, label), 1, "{label} only in the row");
    }
    // "More" keeps only what the row does not list. The manual save moved
    // to the header's "Save" (W3-05), so "Save journey" is gone from it.
    h.click(f, View::ResultsWithChrome, "More");
    let output = h.frame(f, View::ResultsWithChrome, vec![]);
    assert_eq!(text_count(&output, "Plan or replay a defense"), 1);
    assert_eq!(text_count(&output, "Save journey"), 0);
    assert_eq!(text_count(&output, "Save"), 1, "the header saves");
    for label in [
        "World map",
        "Character",
        "Mara",
        "Operations",
        "Fortifications",
    ] {
        assert!(
            text_count(&output, label) <= 1,
            "{label} not repeated in More"
        );
    }
    h.frame(
        f,
        View::ResultsWithChrome,
        vec![key_event(Key::Escape, true)],
    );
    h.click(f, View::ResultsWithChrome, "Menu");
    f.flush_for_test();
    assert!(f.screen == Screen::Menu);
}

#[test]
fn results_outcome_is_one_line_with_one_muted_hint_and_no_second_wallet() {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    for _ in 0..120 {
        f.game.tick(Input::default());
    }
    f.game.withdraw();
    f.screen = Screen::Results;
    f.game.set_paused(true);
    let report = f.game.last_result().unwrap().clone();
    let line = format!(
        "Retreated · {} · Defense {} · banked +{} coins · {} items",
        report.country_name,
        report.stage + 1,
        report.coins,
        report.loot
    );
    let wallet = format!("{} coins", f.game.campaign().coins);
    for (size, scale) in [(vec2(1440.0, 900.0), 1.0), (vec2(1050.0, 700.0), 1.35)] {
        settings::configure(&h.ctx, scale);
        f.settings.scale = scale;
        let output = h.frame_size(f, View::ResultsWithChrome, vec![], size);
        assert_eq!(text_count(&output, &line), 1, "{size:?}: {line}");
        assert!(fully_visible(&output, &line), "{size:?}");
        assert_eq!(text_count(&output, &wallet), 1, "{size:?}: wallet repeated");
        assert!(
            !output
                .shapes
                .iter()
                .any(|s| shape_has_text(&s.shape, "Already banked")),
            "{size:?}"
        );
        // One muted hint under the outcome, above the actions.
        let (outcome, _) = clipped_bounds(&output, &line).unwrap();
        let (actions, _) = clipped_bounds(&output, "Try again").unwrap();
        let between = output
            .shapes
            .iter()
            .filter(|clipped| {
                let mut found = false;
                fn walk(shape: &egui::epaint::Shape, top: f32, bottom: f32, found: &mut bool) {
                    match shape {
                        egui::epaint::Shape::Text(text) => {
                            let y = text.pos.y + text.galley.rect.center().y;
                            *found |= y > top && y < bottom && !text.galley.text().is_empty();
                        }
                        egui::epaint::Shape::Vec(v) => {
                            v.iter().for_each(|s| walk(s, top, bottom, found));
                        }
                        _ => {}
                    }
                }
                walk(
                    &clipped.shape,
                    outcome.bottom(),
                    actions.top() - 6.0,
                    &mut found,
                );
                found
            })
            .count();
        assert_eq!(between, 1, "{size:?}: expected one hint line");
    }
}

#[test]
fn victory_outcome_names_the_win_without_a_retreat_hint() {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.game.withdraw();
    f.game.last_result_mut().as_mut().unwrap().outcome = Outcome::Victory;
    f.screen = Screen::Results;
    let report = f.game.last_result().unwrap().clone();
    let output = h.frame(f, View::ResultsWithChrome, vec![]);
    let line = format!(
        "Victory · {} · Defense {} · banked +{} coins · {} items",
        report.country_name,
        report.stage + 1,
        report.coins,
        report.loot
    );
    assert_eq!(text_count(&output, &line), 1);
    assert!(
        !output
            .shapes
            .iter()
            .any(|s| shape_has_text(&s.shape, "Rest and train"))
    );
}

#[test]
fn a_loss_shows_its_cause_and_the_core_tip_in_full_at_both_sizes() {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.game.withdraw();
    let report = f.game.last_result_mut().as_mut().unwrap();
    report.outcome = Outcome::Defeat;
    report.cause = "Overrun at the line in lane 2".into();
    report.tip = "Aim at the nearest runner first.".into();
    f.screen = Screen::Results;
    f.game.set_paused(true);
    let reason = "Overrun at the line in lane 2. Aim at the nearest runner first.";
    for (size, scale) in [(vec2(1440.0, 900.0), 1.0), (vec2(1050.0, 700.0), 1.35)] {
        settings::configure(&h.ctx, scale);
        f.settings.scale = scale;
        let output = h.frame_size(f, View::ResultsWithChrome, vec![], size);
        assert!(fully_visible(&output, reason), "{size:?}: reason cut");
    }
}

#[test]
fn a_liberation_names_the_freed_country_and_the_fronts_it_opened() {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.game.withdraw();
    let count = horde_core::world::world()
        .country("DEU")
        .unwrap()
        .level_count();
    for stage in 0..count {
        f.game
            .staging()
            .campaign()
            .record_country_stage_clear(stage)
            .unwrap();
    }
    let report = f.game.last_result_mut().as_mut().unwrap();
    report.outcome = Outcome::Victory;
    report.country_freed = true;
    f.screen = Screen::Results;
    f.game.set_paused(true);
    let first = f.game.campaign().world.fronts_opened_by("DEU")[0]
        .name
        .clone();
    for (size, scale) in [(vec2(1440.0, 900.0), 1.0), (vec2(1050.0, 700.0), 1.35)] {
        settings::configure(&h.ctx, scale);
        f.settings.scale = scale;
        let output = h.frame_size(f, View::ResultsWithChrome, vec![], size);
        assert!(
            output.shapes.iter().any(|s| shape_has_text(
                &s.shape,
                &format!("Germany liberated. New fronts: {first}")
            )),
            "{size:?}"
        );
        assert!(
            output
                .shapes
                .iter()
                .any(|s| shape_has_text(&s.shape, "Now open in the tree: Meet Mara")),
            "{size:?}"
        );
        assert_eq!(text_count(&output, "1 liberated"), 1, "{size:?}");
    }
}
