//! Egui harness tests: results, debrief and progression flow.
use super::*;

#[test]
fn results_keep_rewards_banked_and_open_the_actual_recovered_equipment() {
    let mut fixture = IsolatedFrontend::new();
    fixture.frontend.game.campaign_mut().coins = 75;
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
                .any(|shape| shape_has_text(&shape.shape, "Already banked:"))
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
    h.click(f, View::Battle, "Retreat & train");
    assert!(f.screen == Screen::Results);
    assert!(f.game.attempt().is_none());
    assert_eq!(f.game.campaign().coins, 25);
    h.click(f, View::Results, "Try again");
    assert!(f.screen == Screen::Battle);
    assert!(!f.game.attempt().unwrap().paused);
}

#[test]
fn defeat_and_withdraw_each_reach_upgrade_home_and_leave_resume_returns_to_the_map() {
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
    h.click(f, View::Battle, "Retreat & train");
    assert!(f.screen == Screen::Results);
    assert_eq!(f.game.last_result().unwrap().outcome, Outcome::Withdrawal);
    let coins = f.game.campaign().coins;
    let records = f.game.campaign().run_history.len();
    h.click(f, View::Results, "Leave journey");
    assert!(f.screen == Screen::Menu);
    assert!(!f.game.campaign().journey_active);
    f.game = Game::load(&f.paths.campaign()).unwrap();
    h.click(f, View::Menu, "Resume journey");
    assert!(f.screen == Screen::Camp && f.nav.tab == Tab::World);
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
