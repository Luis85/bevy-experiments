use super::{fixtures::*, *};
use crate::launch::{LaunchConfig, ReviewOptions};
use crate::review_scenes;
use crate::ui::{Screen, Tab};
use bevy_egui::egui;
use horde_core::survivors::SurvivorId;
#[test]
fn loot_replay_is_exclusive_to_explicit_isolated_capture_modes() {
    assert_eq!(
        select_loot_capture_mode(Some("quality-loot"), true, true, false, false),
        LootCaptureMode::Motion
    );
    assert_eq!(
        select_loot_capture_mode(Some("quality-loot"), false, false, true, false),
        LootCaptureMode::Still
    );
    for (scene, motion, sequence, still, practice) in [
        (None, true, true, false, false),
        (Some("battle"), true, true, false, false),
        (Some("quality-loot"), true, true, false, true),
        (Some("quality-loot"), true, false, false, false),
        (Some("quality-loot"), false, true, false, false),
        (Some("quality-loot"), false, false, false, false),
    ] {
        assert_eq!(
            select_loot_capture_mode(scene, motion, sequence, still, practice),
            LootCaptureMode::Disabled
        );
    }
}
fn prepared_frontend() -> FrontendParts {
    let mut f = crate::ui::test_support::test_frontend();
    f.game = Game::default();
    f.game
        .campaign_mut()
        .choose_survivor(SurvivorId::Rook)
        .unwrap();
    f.game.campaign_mut().choose_start_country("DEU").unwrap();
    prepare_progression(&mut f.game);
    f
}
#[test]
fn gesture_controls_require_the_complete_isolated_export_guard() {
    for scene in [Some("gesture-throws"), Some("gesture-melee")] {
        assert!(gesture_fixture(scene, true, true, false).is_some());
        for (motion, sequence, practice) in [
            (false, true, false),
            (true, false, false),
            (true, true, true),
            (false, false, false),
        ] {
            assert_eq!(gesture_fixture(scene, motion, sequence, practice), None);
        }
    }
    for scene in [None, Some("battle"), Some("ordnance"), Some("npc")] {
        assert_eq!(gesture_fixture(scene, true, true, false), None);
    }
}
#[test]
fn scheduled_gesture_inputs_create_real_post_warmup_events_and_recover() {
    use horde_core::sim::EventKind;
    for kind in [GestureFixture::Throws, GestureFixture::Melee] {
        let mut f = prepared_frontend();
        prepare_gesture_battle(&mut f, kind == GestureFixture::Melee);
        f.game.set_paused(false);
        let start = f.game.attempt().unwrap().tick;
        let mut observed_throws = std::collections::BTreeMap::new();
        let mut observed_melee = std::collections::BTreeMap::new();
        for frame in 1..=152 {
            let input = scheduled_gesture_input(
                kind,
                frame,
                f.game.attempt().expect("live isolated gesture battle"),
            )
            .unwrap_or_default();
            if frame <= 60 {
                assert!(!input.melee && input.throw.is_none());
            }
            f.game.tick(input);
            // Combat VFX events legitimately expire. Observe real events
            // while live; never change production TTL to make a fixture pass.
            for event in &f.game.attempt().expect("live gesture battle").events {
                let key = (event.tick, event.actor_id, event.target_id);
                match event.kind {
                    EventKind::Throw => {
                        observed_throws.entry(key).or_insert_with(|| event.clone());
                    }
                    EventKind::Melee => {
                        observed_melee.entry(key).or_insert_with(|| event.clone());
                    }
                    _ => {}
                }
            }
        }
        let a = f
            .game
            .attempt()
            .expect("gesture evidence keeps the real battle active");
        let throws: Vec<_> = observed_throws.values().collect();
        let melee: Vec<_> = observed_melee.values().collect();
        if kind == GestureFixture::Throws {
            assert_eq!(throws.len(), 2);
            assert!(
                throws
                    .iter()
                    .all(|event| event.tick > start + 60 && event.actor_id.is_some())
            );
            assert_eq!(
                a.ordnance
                    .charges(horde_core::ordnance::OrdnanceKind::Grenade),
                a.profile.modifiers.grenade_charges - 1
            );
            assert_eq!(
                a.ordnance
                    .charges(horde_core::ordnance::OrdnanceKind::Molotov),
                a.profile.modifiers.molotov_charges - 1
            );
            assert!(a.tick - throws.last().unwrap().tick >= 24);
            assert!(melee.is_empty());
        } else {
            assert_eq!(melee.len(), 1);
            assert!(melee[0].tick > start + 60 && melee[0].target_id.is_some());
            assert!(melee[0].to[0] <= horde_core::sim::DEFENSE_LINE + 0.01);
            assert_eq!(a.melee_commit, 0);
            assert!(a.tick - melee[0].tick >= 30);
            assert!(throws.is_empty());
        }
    }
}
#[test]
fn short_normal_review_fixtures_keep_live_battles_and_real_throw_cells_valid() {
    for scene in ["battle", "npc", "ordnance", "boss", "results"] {
        let mut f = prepared_frontend();
        let expected = f.game.campaign().arena_geometry();
        prepare_battle(&mut f, scene);
        if scene == "results" {
            assert!(f.game.last_result().is_some());
        } else {
            let a = f.game.attempt().unwrap();
            assert_eq!((a.lane_count, a.width_yards), expected);
            for s in &a.structures {
                assert!(s.lane < a.lane_count && s.x() < a.width_yards * 0.5);
            }
            if scene == "ordnance" {
                assert!(!a.ordnance.projectiles.is_empty());
                assert!(!a.ordnance.fire_zones.is_empty());
                for p in &a.ordnance.projectiles {
                    assert!(horde_core::ordnance::valid_cell_for_arena(
                        p.cell,
                        a.lane_count,
                        a.width_yards
                    ));
                }
            }
            if scene == "boss" {
                assert!(a.enemies.iter().any(|e| e.boss));
            }
        }
    }
}
#[test]
fn history_review_uses_real_settlements_and_damage_instead_of_authored_plot_points() {
    let mut f = prepared_frontend();
    prepare_history(&mut f);
    assert_eq!(f.game.campaign().run_history.len(), 3);
    assert!(
        f.game
            .campaign()
            .run_history
            .iter()
            .any(|r| r.damage_by_source.total() > 0 && r.average_dps > 0.0)
    );
    assert!(f.screen == Screen::History);
    assert!(f.game.attempt().is_none());
}
#[test]
fn new_journey_victory_and_liberation_fixtures_use_one_real_settlement_each() {
    for liberated in [false, true] {
        let mut f = crate::ui::test_support::test_frontend();
        f.game = Game::default();
        f.game
            .campaign_mut()
            .choose_survivor(SurvivorId::Rook)
            .unwrap();
        f.game.campaign_mut().choose_start_country("DEU").unwrap();
        prepare_journey_victory(&mut f, liberated);
        assert_eq!(f.game.campaign().run_history.len(), 1);
        let coins = f.game.campaign().coins;
        let next = f.game.continue_journey().unwrap();
        assert_eq!(
            next,
            if liberated {
                horde_core::sim::JourneyContinuation::CountryLiberated
            } else {
                horde_core::sim::JourneyContinuation::Deployed
            }
        );
        assert_eq!(f.game.campaign().coins, coins);
        assert_eq!(f.game.campaign().run_history.len(), 1);
    }
}

/// The screen (and camp tab, where one is shown) every registered still stages.
/// Kept here so the UI-free registry never names UI types.
const EXPECTED: &[(&str, Screen, Option<Tab>)] = &[
    ("menu", Screen::Menu, None),
    ("menu-female", Screen::Menu, None),
    ("survivor-select", Screen::SurvivorSelection, None),
    ("battle-female", Screen::Battle, None),
    ("progressed-battle", Screen::Battle, None),
    ("leveled-armory", Screen::Camp, Some(Tab::Armory)),
    ("quality-armory", Screen::Camp, Some(Tab::Armory)),
    ("quality-loot", Screen::Battle, None),
    ("reticle-reload", Screen::Battle, None),
    ("damage", Screen::Battle, None),
    ("damage-4x", Screen::Battle, None),
    ("critical-npc", Screen::Battle, None),
    ("progressed-peak", Screen::Battle, None),
    ("world", Screen::Camp, Some(Tab::World)),
    ("campaign", Screen::Camp, Some(Tab::World)),
    ("briefing", Screen::Camp, Some(Tab::Briefing)),
    ("briefing-npc", Screen::Camp, Some(Tab::Briefing)),
    ("intro", Screen::Battle, None),
    ("battle", Screen::Battle, None),
    ("npc", Screen::Battle, None),
    ("npc-expanded", Screen::Battle, None),
    ("boss", Screen::Battle, None),
    ("construction", Screen::Battle, None),
    ("character", Screen::Camp, Some(Tab::Character)),
    ("character-max", Screen::Camp, Some(Tab::Character)),
    ("survivor", Screen::Camp, Some(Tab::Survivor)),
    ("armory", Screen::Camp, Some(Tab::Armory)),
    ("skills", Screen::Results, Some(Tab::Skills)),
    ("skills-survival", Screen::Results, Some(Tab::Skills)),
    ("skills-ordnance", Screen::Results, Some(Tab::Skills)),
    ("ordnance", Screen::Battle, None),
    ("engineering", Screen::Results, None),
    ("results", Screen::Results, None),
    ("operations", Screen::Camp, Some(Tab::Operations)),
    ("settings", Screen::Settings, None),
    ("history", Screen::History, None),
    ("history-empty", Screen::History, None),
    ("arid", Screen::Battle, None),
    ("tropical", Screen::Battle, None),
    ("peak", Screen::Battle, None),
    ("active-camp", Screen::Camp, Some(Tab::World)),
    ("max-lanes", Screen::Battle, None),
    ("journey-victory", Screen::Results, None),
    ("journey-liberated", Screen::Results, None),
    ("journey-upgrades", Screen::Results, None),
    ("skills-search", Screen::Results, None),
];

/// An isolated still review of `scene`, staged exactly as startup stages it.
fn staged(scene: &str) -> FrontendParts {
    let mut f = crate::ui::test_support::test_frontend();
    f.launch = LaunchConfig {
        scenario: Some(scene.into()),
        review: Some(ReviewOptions {
            capture: Some("headless-review-capture.png".into()),
            exit: true,
            ..ReviewOptions::default()
        }),
        ..LaunchConfig::default()
    };
    prepare(&mut f);
    f
}

/// One headless 1440x900 frame of the staged screen, drawn as the App draws it.
fn one_frame(f: &mut FrontendParts) -> egui::FullOutput {
    let ctx = egui::Context::default();
    crate::settings::install_font(&ctx);
    let mut warmup = ctx.run_ui(egui::RawInput::default(), |_| {});
    warmup.textures_delta.clear();
    f.runtime.begin_frame(ctx.cumulative_frame_nr() as u32);
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1440.0, 900.0),
            )),
            ..egui::RawInput::default()
        },
        |ui| crate::ui::draw_screen(ui.ctx(), &mut f.ctx(), 0.0),
    );
    output.textures_delta.clear();
    output
}

/// Every still on `screens` prepares to its EXPECTED screen and tab, draws one
/// headless frame, and shows its registered primary exactly once in amber.
fn stills_prepare_headless(screens: &[Screen]) {
    let mut checked = 0;
    for &(name, screen, tab) in EXPECTED.iter().filter(|(_, s, _)| screens.contains(s)) {
        let scene = review_scenes::REVIEW_SCENES
            .iter()
            .find(|scene| scene.name == name)
            .unwrap_or_else(|| panic!("{name} has an expectation row but no registry row"));
        let mut f = staged(name);
        assert!(f.screen == screen, "{name} staged {:?}", f.screen);
        if let Some(tab) = tab {
            assert_eq!(f.nav.tab, tab, "{name}");
        }
        let output = one_frame(&mut f);
        if let Some(primary) = scene.primary {
            assert_eq!(
                crate::ui::tests::text_count(&output, primary),
                1,
                "{name}: one {primary:?}"
            );
            let color = output
                .shapes
                .iter()
                .find_map(|clipped| crate::ui::tests::text_color(&clipped.shape, primary));
            assert_eq!(
                color,
                Some(crate::art::AMBER),
                "{name}: {primary:?} is amber"
            );
        }
        checked += 1;
    }
    assert!(checked > 0);
}
#[test]
fn stills_menus_and_camp_prepare_headless() {
    stills_prepare_headless(&[
        Screen::Menu,
        Screen::SurvivorSelection,
        Screen::Camp,
        Screen::Settings,
    ]);
}
#[test]
fn stills_battles_prepare_headless() {
    stills_prepare_headless(&[Screen::Battle]);
}
#[test]
fn stills_results_and_history_prepare_headless() {
    stills_prepare_headless(&[Screen::Results, Screen::History]);
}
#[test]
fn every_still_has_an_expectation_row() {
    let rows: Vec<_> = EXPECTED.iter().map(|(name, _, _)| *name).collect();
    for still in review_scenes::stills() {
        assert_eq!(
            rows.iter().filter(|row| **row == still.name).count(),
            1,
            "{} needs exactly one EXPECTED row",
            still.name
        );
    }
    for name in rows {
        assert_eq!(
            review_scenes::lookup(name),
            Some(review_scenes::SceneKind::Still),
            "{name} is not a registered still"
        );
    }
}
#[test]
fn progressed_fixtures_have_a_genuinely_leveled_survivor() {
    for scene in ["progressed-battle", "leveled-armory"] {
        let f = staged(scene);
        let player = &f.game.campaign().player;
        assert!(player.level >= 10, "{scene}: level {}", player.level);
        assert_eq!(player.body.values().sum::<u32>(), 3, "{scene}");
        assert_eq!(player.points, 8, "{scene}: points left after 3 allocations");
    }
}
#[test]
fn non_practice_review_has_no_practice_status() {
    for scene in ["menu", "world", "battle", "armory", "results"] {
        let f = staged(scene);
        assert!(
            !f.status.message.contains("Practice"),
            "{scene}: {:?}",
            f.status.message
        );
    }
    // Synthetic fixtures keep their own disclaimer.
    assert!(
        staged("peak")
            .status
            .message
            .contains("peak-render fixture")
    );
    let mut f = crate::ui::test_support::test_frontend();
    f.launch = LaunchConfig {
        scenario: Some("construction".into()),
        practice: true,
        ..LaunchConfig::default()
    };
    prepare(&mut f);
    assert_eq!(
        f.status.message,
        "Practice scenario - progress is isolated from your campaign"
    );
}
/// Startup as `FrontendParts::load` builds it from these launch variables:
/// the real constructor and the real prepare, with a private temp profile.
fn launched(vars: &[(&str, &str)]) -> FrontendParts {
    let config = LaunchConfig::parse(std::iter::empty::<std::ffi::OsString>(), |name| {
        vars.iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| (*value).into())
    })
    .expect("valid launch");
    let paths = crate::profile::ProfilePaths::in_directory(
        std::env::temp_dir().join(format!("lastlight-launched-{}", std::process::id())),
    );
    let profile = crate::profile::LoadedProfile {
        game: Game::default(),
        settings: crate::settings::Settings::default(),
        warning: String::new(),
        settings_at_risk: false,
    };
    let mut f = FrontendParts::new(profile, &config, &paths);
    prepare(&mut f);
    f
}
#[test]
fn isolated_reviews_are_not_practice_profiles() {
    if cfg!(feature = "review") {
        for scene in ["menu", "armory"] {
            let mut f = launched(&[("LASTLIGHT_REVIEW_SCREEN", scene)]);
            assert_eq!(f.launch.scenario.as_deref(), Some(scene));
            assert!(!f.status.practice_profile, "{scene}");
            let output = one_frame(&mut f);
            for label in [
                "Practice profile",
                "Separate practice saves",
                "Practice · separate campaign",
            ] {
                assert_eq!(
                    crate::ui::tests::text_count(&output, label),
                    0,
                    "{scene}: {label:?}"
                );
            }
        }
    }
    let practice = launched(&[
        ("LASTLIGHT_PRACTICE", "1"),
        ("LASTLIGHT_REVIEW_SCREEN", "construction"),
    ]);
    assert!(practice.launch.practice);
    assert!(practice.status.practice_profile);
}
#[test]
fn review_matrix_lists_stress_screen() {
    let row = review_scenes::REVIEW_SCENES
        .iter()
        .find(|scene| scene.name == "stress")
        .expect("stress registry row");
    assert_eq!(row.kind, review_scenes::SceneKind::MotionOnly);
    assert!(row.motion && row.perf && !row.practice);
    assert_eq!((row.owner, row.since_wave), ("perf-architecture", 1));
    assert!(crate::perf_budget::CPU_GATED_SCENES.contains(&row.name));
    assert!(review_scenes::accepts("stress", true));
    assert!(!review_scenes::accepts("stress", false));
    #[cfg(feature = "review")]
    {
        let mut f = staged("stress");
        assert!(f.screen == Screen::Battle, "{:?}", f.screen);
        assert_eq!(f.campaign.mode, horde_core::content::Mode::Npc);
        let a = f.game.attempt().expect("stress field");
        assert_eq!(a.enemies.len(), crate::perf_budget::ENEMIES_STRESS);
        assert!(a.paused);
        assert_eq!(f.game.battles().len(), 3);
        assert_eq!(
            f.status.message,
            "Synthetic stress fixture · 400 drawn + 3 expeditions · perf headroom, not a gameplay claim"
        );
        one_frame(&mut f);
    }
}
/// An autosave of the stress fixture would fail validation, so the scene never
/// submits one; an explicit quit (how a perf report ends) closes unsaved.
#[cfg(feature = "review")]
#[test]
fn stress_screen_never_requests_a_save_and_shows_no_save_error() {
    use crate::runner::{ControlFrame, SaveIntent};
    let mut f = launched(&[
        ("LASTLIGHT_REVIEW_SCREEN", "stress"),
        ("LASTLIGHT_REVIEW_MOTION", "1"),
    ]);
    f.game.set_paused(false);
    f.runtime.publish(ControlFrame {
        enabled: true,
        ..ControlFrame::default()
    });
    // Past two autosave periods (600 ticks each).
    for _ in 0..1300 {
        f.step();
        f.flush();
    }
    assert!(f.runtime.simulated_ticks >= 1300);
    assert!(
        f.game.attempt().is_some(),
        "the stress battle is still live"
    );
    crate::ui::request_save(&mut f.runtime, &mut f.status, SaveIntent::Quit);
    assert!(f.flush_for_test(), "the quit completes without a save");
    let metrics = f.runtime.persistence_metrics();
    assert_eq!(
        (
            metrics.completed_background_saves,
            metrics.failed_background_saves,
            metrics.pending
        ),
        (0, 0, false)
    );
    assert!(
        !f.status.message_error && !f.status.message.contains("Save failed"),
        "{}",
        f.status.message
    );
    assert!(!f.paths.campaign().exists());
}
#[test]
fn workshop_names_still_reach_the_workshop() {
    for scene in ["model-parity-x", "model-workshop"] {
        let f = staged(scene);
        assert!(f.screen == Screen::Camp, "{scene}");
        assert_eq!(f.nav.tab, Tab::World, "{scene}");
    }
}
