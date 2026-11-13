//! Egui harness tests: operations screen and NPC resolution.
use super::*;

#[test]
fn a_personal_field_remains_returnable_from_operations_without_reset_or_resume() {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.game.tick(Input::default());
    f.game.set_paused(true);
    f.screen = Screen::Camp;
    f.nav.tab = Tab::Operations;
    let before = serde_json::to_value(&f.game).unwrap();
    h.click(f, View::Camp, "Return to active defense");
    assert!(f.screen == Screen::Battle);
    assert!(f.game.attempt().unwrap().paused);
    assert_eq!(serde_json::to_value(&f.game).unwrap(), before);
}

#[test]
fn npc_resolution_popup_and_observation_controls_fit_at_both_sizes() {
    for (size, scale) in [(vec2(1440.0, 900.0), 1.0), (vec2(1050.0, 700.0), 1.35)] {
        let mut fixture = IsolatedFrontend::new();
        let h = Harness::new();
        let f = &mut fixture.frontend;
        settings::configure(&h.ctx, scale);
        f.settings.scale = scale;
        f.game.campaign_mut().choose_start_country("DEU").unwrap();
        f.game.campaign_mut().attempts = 1;
        f.game.deploy_country_stage(0, Mode::Npc).unwrap();
        f.game.set_paused(false);
        input_enemy(f, 5, 8.0);
        for _ in 0..600 {
            f.game.tick(Input::default());
            if f.game
                .attempt()
                .unwrap()
                .log
                .iter()
                .any(|s| s.contains("3d6"))
            {
                break;
            }
        }
        assert!(
            f.game
                .attempt()
                .unwrap()
                .log
                .iter()
                .any(|s| s.contains("3d6"))
        );
        f.game.set_paused(true);
        f.screen = Screen::Battle;
        let before = serde_json::to_value(&f.game).unwrap();
        h.frame_size(f, View::Battle, vec![], size);
        h.frame_size(f, View::Battle, vec![], size);
        let base = h.frame_size(f, View::Battle, vec![], size);
        for label in ["Observe", "4×", "Tactics"] {
            let r = base
                .shapes
                .iter()
                .find_map(|s| labeled_bounds(&s.shape, label))
                .unwrap_or_else(|| panic!("Missing visible {label}"));
            assert!(
                h.ctx.content_rect().contains_rect(r),
                "{label} escaped viewport {r:?}"
            );
        }
        h.click_size(f, View::Battle, "Tactics", size);
        for expanded in [false, true] {
            if expanded {
                h.click_size(f, View::Battle, "Latest resolution", size);
            }
            let output = h.frame_size(f, View::Battle, vec![], size);
            let area = h.ctx.content_rect();
            for label in ["Current survivor decision", "Latest resolution"] {
                let r = output
                    .shapes
                    .iter()
                    .find_map(|s| labeled_bounds(&s.shape, label))
                    .unwrap_or_else(|| panic!("Missing {label}, expanded {expanded}"));
                assert!(
                    area.contains_rect(r),
                    "{label} escaped popup viewport {r:?}"
                );
            }
        }
        assert_eq!(serde_json::to_value(&f.game).unwrap(), before);
    }
}

#[test]
fn failed_outbreak_front_change_keeps_operations_and_the_requested_country_context() {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.game.set_paused(true);
    f.screen = Screen::Camp;
    f.nav.tab = Tab::Operations;
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
    let before = serde_json::to_value(&f.game).unwrap();
    h.click(f, View::Camp, "Defend now");
    assert!(f.screen == Screen::Camp && f.nav.tab == Tab::Operations);
    assert_eq!(f.campaign.selected_country, "FRA");
    assert_eq!(f.game.attempt().unwrap().country_id.as_deref(), Some("DEU"));
    assert!(f.status.message_error);
    assert_eq!(serde_json::to_value(&f.game).unwrap(), before);
}

#[test]
fn npc_critical_state_stays_explicit_at_one_and_four_times_observation_speed() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.game.campaign_mut().choose_start_country("DEU").unwrap();
    f.game.campaign_mut().attempts = 1;
    f.game.deploy_country_stage(0, Mode::Npc).unwrap();
    input_enemy(f, 5, 8.0);
    let a = f.game.attempt_mut().unwrap();
    let enemy = a.enemies.last_mut().unwrap();
    enemy.hp = -2;
    enemy.injury.hp = -2;
    assert_eq!(
        enemy.condition(Mode::Npc),
        horde_core::sim::EnemyCondition::Critical
    );
    for speed in [1, 4] {
        f.battle.speed = speed;
        let before = serde_json::to_value(&f.game).unwrap();
        let output = h.frame(f, View::Battle, vec![]);
        assert!(
            output
                .shapes
                .iter()
                .any(|s| shape_has_text(&s.shape, "CRIT · FIGHTING"))
        );
        let mut dossier = h.ctx.run_ui(RawInput::default(), |ui| {
            crate::inspector::dossier(ui, f.game.attempt().unwrap(), [8.0, 5.5])
        });
        assert!(
            dossier
                .shapes
                .iter()
                .any(|s| shape_has_text(&s.shape, "HP -2"))
        );
        assert!(
            dossier
                .shapes
                .iter()
                .any(|s| shape_has_text(&s.shape, "CRITICAL · still fighting"))
        );
        dossier.textures_delta.clear();
        assert_eq!(serde_json::to_value(&f.game).unwrap(), before);
    }
}
