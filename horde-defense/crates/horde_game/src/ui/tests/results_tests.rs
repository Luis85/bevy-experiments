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
    // W3-03 rewrite: the camp row left Results; Equipment is one step on,
    // in the row of the Upgrades screen.
    h.click(f, View::Results, "Upgrade");
    assert!(f.screen == Screen::Upgrades);
    assert_eq!(f.game.campaign().coins, coins);
    h.click(f, View::Upgrades, "Equipment");
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

/// W3-03 rewrite: this was one test of the shared upgrade home (header,
/// camp row, the continue / "More" / "Leave journey" row). Results now has
/// the header and the three-action bar and no camp row; the row is on
/// Upgrades.
#[test]
fn results_and_upgrades_share_the_camp_header_and_only_upgrades_has_the_row() {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.game.withdraw();
    f.game.campaign_mut().coins = 42;
    let x = |output: &FullOutput, label: &str| {
        output
            .shapes
            .iter()
            .find_map(|s| labeled_bounds(&s.shape, label))
            .unwrap_or_else(|| panic!("{label} painted"))
            .left()
    };
    const ROW: [&str; 6] = [
        "Camp",
        "Journey upgrades",
        "Briefing",
        "Equipment",
        "Character",
        "World map",
    ];

    f.screen = Screen::Results;
    let output = h.frame(f, View::ResultsWithChrome, vec![]);
    for label in ["LASTLIGHT", "Menu", "History", "Settings", "Save"] {
        assert_eq!(text_count(&output, label), 1, "{label} once");
    }
    assert_eq!(text_count(&output, "42 coins"), 1, "one wallet on screen");
    assert_eq!(
        text_count(&output, "Run history"),
        0,
        "the header has History"
    );
    let order = ["Upgrade", "Try again", "Leave journey"];
    for pair in order.windows(2) {
        assert!(
            x(&output, pair[0]) < x(&output, pair[1]),
            "{} before {}",
            pair[0],
            pair[1]
        );
    }
    // No camp row, no "More" menu, no replay route and no second save.
    for gone in ROW
        .into_iter()
        .chain(["More", "Plan or replay a defense", "Save journey"])
    {
        assert_eq!(text_count(&output, gone), 0, "{gone} is not on Results");
    }

    f.screen = Screen::Upgrades;
    let output = h.frame(f, View::UpgradesWithChrome, vec![]);
    for label in ["LASTLIGHT", "Menu", "History", "Settings", "Save"] {
        assert_eq!(text_count(&output, label), 1, "{label} once");
    }
    assert_eq!(text_count(&output, "42 coins"), 1, "one wallet on screen");
    // The camp row is drawn here, so its destinations are not repeated as
    // action buttons; "Journey upgrades" is the selected entry, not a title.
    for label in ROW {
        assert_eq!(text_count(&output, label), 1, "{label} only in the row");
    }
    assert!(x(&output, "Back to battle") < x(&output, "Back to camp"));
    for gone in ["Upgrade", "Leave journey", "More"] {
        assert_eq!(text_count(&output, gone), 0, "{gone} is not on Upgrades");
    }

    f.screen = Screen::Results;
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
        // One hint under the outcome. W3-03 rewrite: the actions were the
        // next row, so exactly one line sat between; they are the bottom
        // bar now and the inline content (here the defender picker) sits
        // between. The hint is still the first line under the outcome, it
        // is painted once, and nothing below repeats the banked rewards.
        let (outcome, _) = clipped_bounds(&output, &line).unwrap();
        let (actions, _) = clipped_bounds(&output, "Try again").unwrap();
        let mut between: Vec<(String, Rect)> = painted_texts(&output)
            .into_iter()
            .filter(|(text, rect)| {
                let y = rect.center().y;
                y > outcome.bottom() && y < actions.top() - 6.0 && !text.is_empty()
            })
            .collect();
        between.sort_by(|a, b| a.1.top().total_cmp(&b.1.top()));
        let hint = &between.first().expect("a hint under the outcome").0;
        let cause = report.cause.trim().trim_end_matches('.');
        assert!(
            !cause.is_empty() && hint.starts_with(cause),
            "{size:?}: the first line is the reason: {hint}"
        );
        assert_eq!(text_count(&output, hint), 1, "{size:?}: one hint");
        assert!(
            between[1..]
                .iter()
                .all(|(text, _)| !text.contains("banked")),
            "{size:?}: {between:?}"
        );
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

/// The top of the first painted text run that contains `label`.
fn text_top_containing(output: &FullOutput, label: &str) -> Option<f32> {
    fn find(shape: &egui::epaint::Shape, label: &str) -> Option<f32> {
        match shape {
            egui::epaint::Shape::Text(text) if text.galley.text().contains(label) => {
                Some(text.pos.y)
            }
            egui::epaint::Shape::Vec(shapes) => shapes.iter().find_map(|s| find(s, label)),
            _ => None,
        }
    }
    output.shapes.iter().find_map(|c| find(&c.shape, label))
}

/// W3-22: the level-up grant the settlement paid with this report is one
/// line read from the receipt; a report that paid none shows no such line.
#[test]
fn results_shows_level_grant_line() {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.game.withdraw();
    f.screen = Screen::Results;
    f.game.set_paused(true);
    let output = h.frame(f, View::ResultsWithChrome, vec![]);
    assert!(
        text_top_containing(&output, "reached · +").is_none(),
        "no grant, no line"
    );
    let report = f.game.last_result_mut().as_mut().unwrap();
    report.progression_rewards.level_from = 1;
    report.progression_rewards.level_to = 6;
    report.progression_rewards.level_coins = 1_077;
    let before = serde_json::to_value(&f.game).unwrap();
    for (size, scale) in [(vec2(1440.0, 900.0), 1.0), (vec2(1050.0, 700.0), 1.35)] {
        settings::configure(&h.ctx, scale);
        f.settings.scale = scale;
        let output = h.frame_size(f, View::ResultsWithChrome, vec![], size);
        assert!(
            fully_visible(&output, "Level 6 reached · +1,077 coins"),
            "{size:?}: the grant line names the level reached and the coins paid"
        );
    }
    assert_eq!(serde_json::to_value(&f.game).unwrap(), before);
}

/// W3-22 (design-progression-tree §7): a paid rank milestone is a headline
/// above "Now open", from the receipt: rank, coins and the granted item.
#[test]
fn results_shows_rank_10_milestone_headline() {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.game.withdraw();
    // Ten victories open the rank-10 node, so "Now open" is on screen too.
    f.game.campaign_mut().victories = 10;
    let definition = horde_core::content::ItemDefId::at(0);
    let item = horde_core::content::content()
        .item(&definition)
        .unwrap()
        .name
        .clone();
    let report = f.game.last_result_mut().as_mut().unwrap();
    report.outcome = Outcome::Victory;
    report.progression_rewards.milestones = vec![
        horde_core::sim::MilestoneReward {
            rank: 10,
            coins: 1_000,
            item_id: Some(1),
            item_definition: Some(definition),
        },
        // An item that could not be stored: coins and rank only.
        horde_core::sim::MilestoneReward {
            rank: 15,
            coins: 1_500,
            item_id: None,
            item_definition: None,
        },
    ];
    f.screen = Screen::Results;
    f.game.set_paused(true);
    let headline = format!("Rank 10 milestone · +1,000 coins · {item}");
    for (size, scale) in [(vec2(1440.0, 900.0), 1.0), (vec2(1050.0, 700.0), 1.35)] {
        settings::configure(&h.ctx, scale);
        f.settings.scale = scale;
        let output = h.frame_size(f, View::ResultsWithChrome, vec![], size);
        assert!(fully_visible(&output, &headline), "{size:?}: {headline}");
        assert!(
            fully_visible(&output, "Rank 15 milestone · +1,500 coins"),
            "{size:?}: a milestone without an item names rank and coins"
        );
        let top = text_top_containing(&output, &headline).unwrap();
        let open = text_top_containing(&output, "Now open in the tree")
            .unwrap_or_else(|| panic!("{size:?}: Now open line"));
        assert!(top < open, "{size:?}: the headline sits above Now open");
    }
}

/// W3-23: Results names the run's elite kills and the boss time-to-kill
/// from the report's run record (`Game::results_run`); a run with neither
/// shows no such line.
#[test]
fn results_lists_elite_kills_and_boss_ttk() {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.game.withdraw();
    f.screen = Screen::Results;
    f.game.set_paused(true);
    let output = h.frame(f, View::ResultsWithChrome, vec![]);
    assert!(
        text_top_containing(&output, "oss defeated in").is_none()
            && text_top_containing(&output, "Elite kills").is_none(),
        "no elites, no boss: no line"
    );
    {
        let history = std::sync::Arc::make_mut(&mut f.game.campaign_mut().run_history);
        let run = history.last_mut().unwrap();
        run.kills = 40;
        run.elite_kills = 3;
        run.duration_ticks = 16_000;
        run.boss_ttk_ticks = Some(1_530);
        run.boss_rank = horde_core::progression::BossRank::Capstone;
    }
    let before = serde_json::to_value(&f.game).unwrap();
    for (size, scale) in [(vec2(1440.0, 900.0), 1.0), (vec2(1050.0, 700.0), 1.35)] {
        settings::configure(&h.ctx, scale);
        f.settings.scale = scale;
        let output = h.frame_size(f, View::ResultsWithChrome, vec![], size);
        assert!(
            fully_visible(&output, "Elite kills 3 · Capstone boss defeated in 25.5 s"),
            "{size:?}: the combat line names elites and the boss time-to-kill"
        );
    }
    assert_eq!(serde_json::to_value(&f.game).unwrap(), before);
    // A Sector boss carries no rank word; without elites it stands alone.
    {
        let history = std::sync::Arc::make_mut(&mut f.game.campaign_mut().run_history);
        let run = history.last_mut().unwrap();
        run.elite_kills = 0;
        run.boss_rank = horde_core::progression::BossRank::Sector;
    }
    let output = h.frame(f, View::ResultsWithChrome, vec![]);
    assert!(text_top_containing(&output, "Boss defeated in 25.5 s").is_some());
    assert!(text_top_containing(&output, "Elite kills").is_none());
}

/// Every painted text run with its bounds.
fn painted_texts(output: &FullOutput) -> Vec<(String, Rect)> {
    fn collect(shape: &egui::epaint::Shape, into: &mut Vec<(String, Rect)>) {
        match shape {
            egui::epaint::Shape::Text(t) => into.push((
                t.galley.text().to_owned(),
                t.galley.rect.translate(t.pos.to_vec2()),
            )),
            egui::epaint::Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, into)),
            _ => {}
        }
    }
    let mut texts = Vec::new();
    for clipped in &output.shapes {
        collect(&clipped.shape, &mut texts);
    }
    texts
}

/// The text runs painted in the row of the Results action bar (the row of
/// "Leave journey"), left to right.
fn action_bar_row(output: &FullOutput) -> Vec<String> {
    let texts = painted_texts(output);
    let anchor = texts
        .iter()
        .find(|(text, _)| text == "Leave journey")
        .map(|(_, rect)| *rect)
        .expect("Leave journey painted");
    let mut row: Vec<&(String, Rect)> = texts
        .iter()
        .filter(|(_, rect)| (rect.center().y - anchor.center().y).abs() < 14.0)
        .collect();
    row.sort_by(|a, b| a.1.left().total_cmp(&b.1.left()));
    row.into_iter().map(|(text, _)| text.clone()).collect()
}

type ResultsStage = fn(&mut IsolatedFrontend);
/// Settled rounds on Germany: a withdrawal, a victory with defenses left
/// and the victory that liberates it.
const SETTLED_ROUNDS: [(&str, ResultsStage, &str); 3] = [
    (
        "withdrawal",
        |x| {
            x.deploy();
            x.frontend.game.withdraw();
        },
        "Try again",
    ),
    (
        "victory",
        |x| {
            x.deploy();
            let game = &mut x.frontend.game;
            game.withdraw();
            game.last_result_mut().as_mut().unwrap().outcome = Outcome::Victory;
            game.campaign_mut()
                .world
                .country_levels
                .insert("DEU".into(), 1);
        },
        "Next defense",
    ),
    (
        "liberation",
        |x| {
            x.deploy();
            let game = &mut x.frontend.game;
            game.withdraw();
            game.last_result_mut().as_mut().unwrap().outcome = Outcome::Victory;
            let count = world().country("DEU").unwrap().level_count();
            game.campaign_mut()
                .world
                .country_levels
                .insert("DEU".into(), count);
        },
        "Choose next country",
    ),
];

/// UX-02 (W3-03): the Results action bar is exactly [Upgrade] [continue]
/// [Leave journey], in that order, at both target sizes and for every
/// outcome. The camp row, "More", the replay route and a second save are
/// not on Results.
#[test]
fn results_action_bar_has_exactly_three_buttons() {
    for target in Target::BOTH {
        for (name, stage, continuation) in SETTLED_ROUNDS {
            let mut fixture = IsolatedFrontend::new();
            stage(&mut fixture);
            let h = Harness::at(target);
            let f = &mut fixture.frontend;
            f.game.set_paused(true);
            f.screen = Screen::Results;
            h.frame(f, View::Results, vec![]);
            let output = h.frame(f, View::Results, vec![]);
            assert_eq!(
                action_bar_row(&output),
                ["Upgrade", continuation, "Leave journey"],
                "{target:?} {name}"
            );
            for gone in [
                "More",
                "Plan or replay a defense",
                "Save journey",
                "Journey upgrades",
                "Briefing",
                "Equipment",
                "Character",
                "World map",
            ] {
                assert_eq!(text_count(&output, gone), 0, "{target:?} {name}: {gone}");
            }
        }
    }
}

/// Decision 3 (W3-03): the pure Results primary. Upgrade is the primary
/// only after a defeat or a withdrawal whose recommended node can be bought
/// right now; a win, a live defense and every other node status continue.
#[test]
fn results_primary_table() {
    use super::results::{ResultsAction as R, results_primary};
    use horde_core::campaign::NodeStatus;
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let game = &mut fixture.frontend.game;
    game.withdraw();
    game.campaign_mut().coins = 1_000_000;
    let campaign = game.campaign();
    let available = horde_core::skill_catalog::skills()
        .nodes
        .iter()
        .map(|node| campaign.node_status(&node.id))
        .find(|status| matches!(status, NodeStatus::Available(_)))
        .expect("a rich campaign can buy some node");
    let owned = NodeStatus::Owned;
    let locked = NodeStatus::Locked {
        reason: "Liberate a country first".into(),
    };
    let blocked = NodeStatus::Blocked {
        payment: None,
        reason: "Not enough coins".into(),
    };
    let report = |outcome| {
        let mut report = game.last_result().unwrap().clone();
        report.outcome = outcome;
        report
    };
    use Outcome::{Defeat, Victory, Withdrawal};
    let rows: [(Outcome, Option<&NodeStatus>, bool, R); 14] = [
        (Defeat, Some(&available), false, R::Upgrade),
        (Withdrawal, Some(&available), false, R::Upgrade),
        // A live defense always continues.
        (Defeat, Some(&available), true, R::Continue),
        (Withdrawal, Some(&available), true, R::Continue),
        // A win continues, whatever can be bought.
        (Victory, Some(&available), false, R::Continue),
        (Victory, Some(&available), true, R::Continue),
        (Victory, None, false, R::Continue),
        // A recommendation that cannot be bought now never takes the amber.
        (Defeat, Some(&blocked), false, R::Continue),
        (Defeat, Some(&locked), false, R::Continue),
        (Defeat, Some(&owned), false, R::Continue),
        (Defeat, None, false, R::Continue),
        (Withdrawal, Some(&blocked), false, R::Continue),
        (Withdrawal, Some(&locked), false, R::Continue),
        (Withdrawal, None, false, R::Continue),
    ];
    for (outcome, status, live, expected) in rows {
        assert_eq!(
            results_primary(&report(outcome), status, live),
            expected,
            "{outcome:?} {status:?} live {live}"
        );
    }
}

/// The painted half of decision 3: the bar's amber primary is the
/// `results_primary` choice, the other two buttons stay where they are,
/// and exactly one primary is registered.
#[test]
fn results_primary_is_upgrade_after_a_loss_with_an_affordable_recommendation() {
    for target in Target::BOTH {
        for (name, stage, continuation) in SETTLED_ROUNDS {
            for coins in [0, 1_000_000] {
                let mut fixture = IsolatedFrontend::new();
                stage(&mut fixture);
                let h = Harness::at(target);
                let f = &mut fixture.frontend;
                f.game.campaign_mut().coins = coins;
                f.game.set_paused(true);
                f.screen = Screen::Results;
                h.frame(f, View::Full, vec![]);
                let output = h.frame(f, View::Full, vec![]);
                let expected = if name == "withdrawal" && coins > 0 {
                    "Upgrade"
                } else {
                    continuation
                };
                assert_eq!(
                    h.screen_primaries(),
                    [expected],
                    "{target:?} {name} with {coins} coins"
                );
                assert_eq!(
                    action_bar_row(&output),
                    ["Upgrade", continuation, "Leave journey"],
                    "{target:?} {name} with {coins} coins: fixed positions"
                );
            }
        }
    }
}

/// Results is the debrief of a settled round: drawn with no report it
/// hands over to Camp home and registers no primary.
#[test]
fn results_without_a_report_redirects_to_camp_home() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.game.campaign_mut().choose_start_country("DEU").unwrap();
    assert!(f.game.last_result().is_none());
    f.screen = Screen::Results;
    h.frame(f, View::Full, vec![]);
    assert!(f.screen == Screen::Camp && f.nav.tab == Tab::Home);
}
