//! Egui harness tests: Camp home (W3-02; design-ux-flow decision 5 and §4
//! "Camp/Home"). Home is the default camp tab; the row has six entries;
//! Mara and Operations are drill-downs under Home; Home owns the world clock;
//! the roster reads core labels and power; the Home primary follows
//! `home_primary`.
use super::home::{HomePrimary, home_primary};
use super::*;
use horde_core::{content::EnemyKind, defenders::DefenderId};

fn labels(f: &mut FrontendParts) -> Vec<&'static str> {
    f.act(|c| {
        navigation::camp_entries(c)
            .into_iter()
            .map(navigation::NavEntry::label)
            .collect()
    })
}

/// A journey in Germany with no live attempt and every camp feature open:
/// legacy features with an attempt behind them unlock Mara and Operations.
fn unlocked_home(fixture: &mut IsolatedFrontend) -> &mut FrontendParts {
    let f = &mut fixture.frontend;
    f.game.campaign_mut().choose_start_country("DEU").unwrap();
    f.game.campaign_mut().journey_active = true;
    f.game.campaign_mut().legacy_features = true;
    f.game.campaign_mut().attempts = 1;
    f.nav.tab = Tab::Home;
    f
}

/// Three whole App frames, then the fourth's output.
fn settle(h: &Harness, f: &mut FrontendParts) -> FullOutput {
    for _ in 0..3 {
        h.frame(f, View::Full, vec![]);
    }
    h.frame(f, View::Full, vec![])
}

/// Liberates Germany: every earlier stage cleared, then its final stage won
/// on one tick (the boss already spawned, the last enemy down), so the
/// settled report is a liberation.
fn liberate_germany(f: &mut FrontendParts) {
    let count = world().country("DEU").unwrap().level_count();
    for stage in 0..count - 1 {
        f.game
            .campaign_mut()
            .record_country_stage_clear(stage)
            .unwrap();
    }
    f.game
        .deploy_country_stage(count - 1, Mode::Personal)
        .unwrap();
    let mut a = f.game.attempt_slot_mut().take().unwrap();
    a.boss_spawned = true;
    let lane = a.lane_count / 2;
    f.game.spawn_enemy(&mut a, EnemyKind::Shambler, lane, false);
    let last = a.enemies.last_mut().unwrap();
    last.x = 20.0;
    last.injury.unconscious = true;
    *f.game.attempt_slot_mut() = Some(a);
    f.game.set_paused(false);
    f.game.tick(Input::default());
    assert!(f.game.attempt().is_none(), "the final stage settled");
    let report = f.game.last_result().unwrap();
    assert_eq!(report.outcome, Outcome::Victory);
    assert_eq!(
        f.game.campaign().world.completed("DEU"),
        count,
        "Germany liberated"
    );
    f.game.set_paused(true);
}

#[test]
fn camp_default_tab_is_home() {
    assert_eq!(Tab::default(), Tab::Home);
    assert_eq!(Navigation::default().tab, Tab::Home);
    assert_eq!(test_support::test_frontend().nav.tab, Tab::Home);

    for target in Target::BOTH {
        let mut fixture = IsolatedFrontend::new();
        let h = Harness::at(target);
        let f = unlocked_home(&mut fixture);
        f.nav.tab = Tab::default();
        let output = settle(&h, f);
        assert!(f.screen == Screen::Camp && f.nav.tab == Tab::Home);
        assert_eq!(text_count(&output, "Camp home"), 1, "{target:?}: title");
        // Six entries, Camp first; Mara and Operations are drill-downs.
        assert_eq!(
            labels(f),
            [
                "Camp",
                "Journey upgrades",
                "Briefing",
                "Equipment",
                "Character",
                "World map"
            ],
            "{target:?}"
        );
        for gone in ["Mara", "Operations"] {
            assert_eq!(text_count(&output, gone), 0, "{target:?}: {gone}");
        }
        assert_eq!(h.screen_primaries(), ["Go to briefing"], "{target:?}");
    }

    // The drill-downs open from Home, keep the row (Camp selected) and lead
    // back through their breadcrumb.
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::large();
    let f = unlocked_home(&mut fixture);
    for (link, tab, name) in [
        ("Training ›", Tab::Survivor, "· Mara"),
        ("Open operations ›", Tab::Operations, "· Operations"),
    ] {
        settle(&h, f);
        h.click(f, View::Full, link);
        let output = settle(&h, f);
        assert!(f.screen == Screen::Camp && f.nav.tab == tab, "{link}");
        assert_eq!(text_count(&output, "‹ Camp home"), 1, "{link}");
        assert_eq!(text_count(&output, name), 1, "{link}");
        assert_eq!(labels(f).len(), 6, "{link}: the row is unchanged");
        assert_eq!(text_count(&output, "Camp"), 1, "{link}: Camp in the row");
        h.click(f, View::Full, "‹ Camp home");
        settle(&h, f);
        assert!(f.screen == Screen::Camp && f.nav.tab == Tab::Home, "{link}");
    }
    // "Camp" in the row returns to Home from a Prepare page.
    f.act(|c| c.open_preparation(Tab::Armory));
    settle(&h, f);
    assert!(f.nav.tab == Tab::Armory);
    h.click(f, View::Full, "Camp");
    settle(&h, f);
    assert!(f.screen == Screen::Camp && f.nav.tab == Tab::Home);

    // Before the first start, a Home request lands on the world map.
    let mut fixture = IsolatedFrontend::new();
    let f = &mut fixture.frontend;
    f.nav.tab = Tab::Home;
    settle(&h, f);
    assert!(f.screen == Screen::Camp && f.nav.tab == Tab::World);
    assert_eq!(h.screen_primaries(), ["Start journey"]);

    // Resume falls back to Home when no journey is active.
    let mut fixture = IsolatedFrontend::new();
    let f = &mut fixture.frontend;
    f.game.campaign_mut().choose_start_country("DEU").unwrap();
    f.game.campaign_mut().journey_active = false;
    f.screen = Screen::Menu;
    f.nav.tab = Tab::Armory;
    f.act(|c| c.resume());
    f.flush_for_test();
    assert!(f.screen == Screen::Camp && f.nav.tab == Tab::Home);
}

#[test]
fn clock_controls_only_on_home() {
    let clocks =
        |o: &FullOutput| text_count(o, "Pause world clock") + text_count(o, "Resume world clock");
    for tab in [
        Tab::Home,
        Tab::Briefing,
        Tab::Armory,
        Tab::Character,
        Tab::World,
        Tab::Survivor,
        Tab::Operations,
    ] {
        let mut fixture = IsolatedFrontend::new();
        let h = Harness::large();
        let f = unlocked_home(&mut fixture);
        assert!(f.game.operations_available());
        f.nav.tab = tab;
        let output = settle(&h, f);
        assert!(f.nav.tab == tab, "{tab:?}");
        assert_eq!(clocks(&output), usize::from(tab == Tab::Home), "{tab:?}");
        // The clock key (the `world_clock` extra binding, W3-10) works where
        // the clock is shown, and only there.
        assert_eq!(f.settings.extra_bindings.world_clock, "P");
        let before = f.game.global_paused();
        h.key(f, View::Full, Key::P);
        assert_eq!(
            f.game.global_paused() != before,
            tab == Tab::Home,
            "{tab:?}: the clock key"
        );
    }

    // A live main defense: Home keeps the clock beside its one return.
    for tab in [Tab::Home, Tab::Armory, Tab::Briefing, Tab::World] {
        let mut fixture = IsolatedFrontend::new();
        fixture.deploy();
        let h = Harness::large();
        let f = &mut fixture.frontend;
        f.game.set_paused(true);
        f.screen = Screen::Camp;
        f.nav.tab = tab;
        let output = settle(&h, f);
        assert_eq!(
            h.screen_primaries(),
            ["Return to active defense"],
            "{tab:?}"
        );
        assert_eq!(text_count(&output, "Return to active defense"), 1);
        assert_eq!(
            text_count(&output, "Resume world clock"),
            usize::from(tab == Tab::Home),
            "{tab:?}"
        );
    }
}

/// W3-10: the world clock has its own key. The battle's Pause binding never
/// drives it, and an unbound clock key never fires.
#[test]
fn unbound_extra_action_never_fires() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::large();
    let f = unlocked_home(&mut fixture);
    settle(&h, f);
    assert!(!f.game.global_paused());
    // Unbound: no key drives the clock, not even the Pause binding, and not
    // the key the action had before.
    f.settings.extra_bindings.world_clock.clear();
    f.settings.bindings[Action::Pause as usize] = "K".into();
    for key in [Key::K, Key::P] {
        h.key(f, View::Full, key);
        assert!(!f.game.global_paused(), "{key:?} with the clock unbound");
    }
    let output = settle(&h, f);
    assert_eq!(text_count(&output, "Pause world clock"), 1);
    // The button still works; only the key is unbound.
    h.click(f, View::Full, "Pause world clock");
    assert!(f.game.global_paused());
    // Bound again, the clock key drives it and Pause still does not.
    f.settings.extra_bindings.world_clock = "J".into();
    h.key(f, View::Full, Key::K);
    assert!(f.game.global_paused(), "Pause is the battle's key");
    h.key(f, View::Full, Key::J);
    assert!(!f.game.global_paused());
    h.key(f, View::Full, Key::J);
    assert!(f.game.global_paused());
}

/// The power text a roster row must show, built from core's numbers.
fn power_text(option: &horde_core::defenders::DefenderOption) -> String {
    format!(
        "Power {} · threat {} · {}",
        option.power.rating,
        option.power.threat,
        option.power.band.label()
    )
}

#[test]
fn roster_shows_core_power_and_status_labels() {
    let mut fixture = IsolatedFrontend::new();
    let f = unlocked_home(&mut fixture);
    f.game.campaign_mut().coins = 500;
    for id in ["DEU", "FRA"] {
        f.game.campaign_mut().world.cleared.push(id.into());
        f.game
            .campaign_mut()
            .world
            .country_levels
            .insert(id.into(), world().country(id).unwrap().level_count());
    }
    let ready = f
        .game
        .with_strategy(|strategy, campaign| strategy.recruit(campaign, "DEU"))
        .unwrap();
    let away = f
        .game
        .with_strategy(|strategy, campaign| strategy.recruit(campaign, "FRA"))
        .unwrap();
    f.game.deploy_survivor(away, "POL", 0).unwrap();
    f.game.set_paused(true);

    let options = f.game.available_defenders();
    let ids: Vec<DefenderId> = options.iter().map(|o| o.card.id).collect();
    assert_eq!(
        ids,
        [
            DefenderId::Player,
            DefenderId::Mara,
            DefenderId::Recruit(ready),
            DefenderId::Recruit(away)
        ]
    );
    let status = |id: DefenderId| match id {
        DefenderId::Recruit(r) => f
            .game
            .strategy()
            .roster
            .iter()
            .find(|x| x.id == r)
            .unwrap()
            .status
            .label(),
        _ => "Ready",
    };
    let expected_ready = ids.iter().filter(|id| status(**id) == "Ready").count();
    assert_eq!(expected_ready, 3);
    assert_eq!(status(DefenderId::Recruit(away)), "On expedition");
    for target in Target::BOTH {
        let h = Harness::at(target);
        let output = settle(&h, f);
        assert!(f.nav.tab == Tab::Home);
        for option in &options {
            let name = &option.card.name;
            assert!(text_count(&output, name) >= 1, "{target:?}: {name}");
            let power = option.power;
            let line = power_text(option);
            // Equal powers (the player and Mara can match) share a line.
            let rows = options.iter().filter(|o| power_text(o) == line).count();
            assert_eq!(
                text_count(&output, &line),
                rows,
                "{target:?}: {name} {line}"
            );
            let max_hp = f
                .game
                .deployment_inputs(option.card.id, power.threat)
                .unwrap()
                .max_hp;
            let readiness = format!("HP {max_hp} · Lv {}", option.card.level);
            assert!(
                text_count(&output, &readiness) >= 1,
                "{target:?}: {name} {readiness}"
            );
        }
        // Status reads the core labels, never a Debug name.
        assert_eq!(text_count(&output, "Ready"), expected_ready, "{target:?}");
        assert_eq!(text_count(&output, "On expedition"), 1, "{target:?}");
        assert_eq!(text_count(&output, "Deployed"), 0, "{target:?}");
        // The next defender is marked; the others offer "Defend next".
        assert_eq!(text_count(&output, "Defends next"), 1, "{target:?}");
    }
    // "Defend next" dispatches SetNextDefender through core.
    let h = Harness::large();
    settle(&h, f);
    let before = f.game.next_defender();
    assert_eq!(before, DefenderId::Player);
    f.act(|c| super::home::defend_next(c, DefenderId::Recruit(ready)));
    f.flush_for_test();
    assert_eq!(f.game.next_defender(), DefenderId::Recruit(ready));

    // A route that names a recruit (the Recruit modal's "Back to camp")
    // opens Home with that row highlighted, once.
    f.act(|c| c.open_camp_home(Some(HomeFocus::Recruit(ready))));
    let output = settle(&h, f);
    assert!(f.screen == Screen::Camp && f.nav.tab == Tab::Home);
    let name = f
        .game
        .defender_card(DefenderId::Recruit(ready))
        .unwrap()
        .name;
    let color = |o: &FullOutput| o.shapes.iter().find_map(|s| text_color(&s.shape, &name));
    assert_eq!(color(&output), Some(crate::art::AMBER));
    f.act(|c| c.open_preparation(Tab::Armory));
    assert_eq!(f.nav.home_focus, None);
    f.act(|c| c.open_camp_home(None));
    let output = settle(&h, f);
    assert_ne!(color(&output), Some(crate::art::AMBER));
}

#[test]
fn home_primary_after_liberation_is_choose_next_country() {
    // The pure rule.
    let mut fixture = IsolatedFrontend::new();
    let f = &mut fixture.frontend;
    assert_eq!(home_primary(&f.game), HomePrimary::ChooseCountry);
    f.game.campaign_mut().choose_start_country("DEU").unwrap();
    assert_eq!(home_primary(&f.game), HomePrimary::GoToBriefing);
    let mut live = IsolatedFrontend::new();
    live.deploy();
    assert_eq!(
        home_primary(&live.frontend.game),
        HomePrimary::ReturnToDefense
    );
    // A won stage short of liberation still goes to the briefing.
    let mut won = IsolatedFrontend::new();
    let g = &mut won.frontend;
    g.game.campaign_mut().choose_start_country("DEU").unwrap();
    g.game.deploy_country_stage(0, Mode::Personal).unwrap();
    g.game.withdraw();
    assert!(g.game.campaign().journey_active);
    assert_eq!(home_primary(&g.game), HomePrimary::GoToBriefing);

    let mut fixture = IsolatedFrontend::new();
    let h = Harness::large();
    let f = unlocked_home(&mut fixture);
    liberate_germany(f);
    assert!(f.game.campaign().journey_active);
    assert_eq!(home_primary(&f.game), HomePrimary::ChooseNextCountry);
    assert_eq!(
        HomePrimary::ChooseNextCountry.label(),
        "Choose next country"
    );
    // Home and the other camp pages share the rule.
    for tab in [Tab::Home, Tab::Armory, Tab::Character] {
        f.screen = Screen::Camp;
        f.nav.tab = tab;
        settle(&h, f);
        assert_eq!(h.screen_primaries(), ["Choose next country"], "{tab:?}");
    }
    // It continues the journey: the liberation opens the world map.
    f.nav.tab = Tab::Home;
    settle(&h, f);
    h.click(f, View::Full, "Choose next country");
    settle(&h, f);
    assert!(!f.game.campaign().journey_active);
    assert!(f.game.attempt().is_none());
    assert!(f.screen == Screen::Camp && f.nav.tab == Tab::World);
}

#[test]
fn operations_service_record_says_since_records_began_for_untracked_runs() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::large();
    let f = unlocked_home(&mut fixture);
    f.game.campaign_mut().coins = 500;
    for id in ["DEU", "FRA"] {
        f.game.campaign_mut().world.cleared.push(id.into());
        f.game
            .campaign_mut()
            .world
            .country_levels
            .insert(id.into(), world().country(id).unwrap().level_count());
    }
    let id = f
        .game
        .with_strategy(|strategy, campaign| strategy.recruit(campaign, "DEU"))
        .unwrap();
    f.nav.tab = Tab::Operations;
    let output = settle(&h, f);
    let record = f.game.service_record(id).unwrap();
    assert!(!record.since_records_began());
    let fresh = "Service record · 0 deployments · 0 victories · 0 kills";
    assert_eq!(text_count(&output, fresh), 1);
    assert_eq!(text_count(&output, "(since records began)"), 0);

    // Two attempts the history never named.
    f.game.with_strategy(|strategy, _| {
        strategy
            .roster
            .iter_mut()
            .find(|r| r.id == id)
            .unwrap()
            .npc
            .attempts = 2;
    });
    assert!(f.game.service_record(id).unwrap().since_records_began());
    let output = settle(&h, f);
    assert_eq!(text_count(&output, fresh), 1);
    assert_eq!(text_count(&output, "(since records began)"), 1);
}

#[test]
fn home_alert_plan_response_opens_the_briefing_without_deploying() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::large();
    let f = unlocked_home(&mut fixture);
    f.game.campaign_mut().world.cleared.push("FRA".into());
    f.game
        .campaign_mut()
        .world
        .country_levels
        .insert("FRA".into(), world().country("FRA").unwrap().level_count());
    f.game.strategy_mut().peak_freed = 1;
    f.game.strategy_mut().next_outbreak_id = 2;
    f.game
        .strategy_mut()
        .outbreaks
        .push(horde_core::strategy::Outbreak {
            id: 1,
            country_id: "FRA".into(),
            started_tick: 0,
            deadline_tick: 9999,
            strength: 2,
            enemy: horde_core::rules::InjuryState::new(9, 10),
            outcome: horde_core::strategy::OutbreakOutcome::Active,
            ready_turns: 0,
        });
    let output = settle(&h, f);
    assert_eq!(text_count(&output, "Alerts"), 1);
    assert_eq!(text_count(&output, "Plan response"), 1);
    h.click(f, View::Full, "Plan response");
    settle(&h, f);
    assert!(f.game.attempt().is_none(), "nothing deploys from Home");
    assert!(f.screen == Screen::Camp && f.nav.tab == Tab::Briefing);
    assert_eq!(f.campaign.selected_country, "FRA");
}
