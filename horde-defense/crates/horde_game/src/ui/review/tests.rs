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
    // Wave 1 (design-qa-captures 1.8).
    ("results-defeat", Screen::Results, None),
    ("save-error", Screen::Camp, Some(Tab::World)),
    ("night-checkpoint", Screen::Battle, None),
    ("weapon-classes-pistol", Screen::Battle, None),
    ("weapon-classes-smg", Screen::Battle, None),
    ("weapon-classes-shotgun", Screen::Battle, None),
    ("weapon-classes-rifle", Screen::Battle, None),
    ("weapon-classes-marksman", Screen::Battle, None),
    ("weapon-classes-machine-gun", Screen::Battle, None),
    ("weapon-classes-turret", Screen::Battle, None),
    ("fx-stress", Screen::Battle, None),
    ("battle-mid", Screen::Battle, None),
    ("operations-observer", Screen::Camp, Some(Tab::Operations)),
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
        // The stress fixture behind fx-stress exists only in review builds.
        if name == "fx-stress" && !cfg!(feature = "review") {
            continue;
        }
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
/// No intent on the stress screen is dropped: each one finishes without a
/// save, so the working indicator clears, Back from Settings returns to the
/// battle and "Save and return to menu" reaches the menu.
#[cfg(feature = "review")]
#[test]
fn stress_screen_finishes_every_intent_without_a_save() {
    use crate::runner::SaveIntent;
    let mut f = launched(&[
        ("LASTLIGHT_REVIEW_SCREEN", "stress"),
        ("LASTLIGHT_REVIEW_MOTION", "1"),
    ]);
    assert!(f.screen == Screen::Battle, "{:?}", f.screen);
    f.runtime.begin_frame(1);
    f.act(|c| c.open_settings());
    assert!(f.screen == Screen::Settings, "{:?}", f.screen);
    f.runtime.begin_frame(2);
    f.act(|c| c.request_save(SaveIntent::SettingsBack));
    assert!(!f.flush_for_test(), "Back does not close the review");
    assert!(f.screen == Screen::Battle, "{:?}", f.screen);
    assert!(!f.status.save_pending, "{}", f.status.message);
    assert_eq!(f.status.message, "Stress review: nothing is saved");
    f.runtime.begin_frame(3);
    f.act(|c| c.request_save(SaveIntent::Manual));
    assert!(!f.flush_for_test());
    assert!(!f.status.save_pending, "{}", f.status.message);
    f.runtime.begin_frame(4);
    f.act(|c| c.request_save(SaveIntent::Menu));
    assert!(!f.flush_for_test());
    assert!(f.screen == Screen::Menu, "{:?}", f.screen);
    assert!(!f.status.save_pending, "{}", f.status.message);
    assert!(!f.status.message_error && !f.status.profile_exists);
    let metrics = f.runtime.persistence_metrics();
    assert_eq!(
        (
            metrics.completed_background_saves,
            metrics.failed_background_saves,
            metrics.pending
        ),
        (0, 0, false)
    );
    assert!(!f.paths.campaign().exists() && !f.paths.settings().exists());
}
/// The real startup (`FrontendParts::load`) of a motion stress review keeps
/// the fixture's disclaimer rather than the generic live-motion note.
#[cfg(feature = "review")]
#[test]
fn stress_disclaimer_survives_the_motion_launch() {
    let vars = [
        ("LASTLIGHT_REVIEW_SCREEN", "stress"),
        ("LASTLIGHT_REVIEW_MOTION", "1"),
    ];
    let config = LaunchConfig::parse(std::iter::empty::<std::ffi::OsString>(), |name| {
        vars.iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| (*value).into())
    })
    .expect("valid launch");
    let f = FrontendParts::load(&config, crate::profile::SettingsLoad::default());
    assert!(f.screen == Screen::Battle, "{:?}", f.screen);
    assert!(!f.game.global_paused(), "motion review runs live");
    assert_eq!(
        f.status.message,
        "Synthetic stress fixture · 400 drawn + 3 expeditions · perf headroom, not a gameplay claim"
    );
}
/// Every synthetic or copied perf fixture keeps its claim disclaimer under
/// motion, as stress does; the peak fixture, live here, drops its "Paused".
/// An ordinary scene still gets the generic live-motion note.
#[cfg(feature = "review")]
#[test]
fn synthetic_disclaimers_survive_the_motion_launch() {
    let maximum = "Synthetic maximum-rank fixture · authored population cap plus boss · no gameplay or FPS claim";
    for (scene, message) in [
        ("peak", "Live peak-render fixture · no simulation FPS claim"),
        ("progressed-peak", maximum),
        ("max-lanes", maximum),
        (
            "battle",
            "Live motion review · progress is isolated from your campaign",
        ),
    ] {
        let vars = [
            ("LASTLIGHT_REVIEW_SCREEN", scene),
            ("LASTLIGHT_REVIEW_MOTION", "1"),
        ];
        let config = LaunchConfig::parse(std::iter::empty::<std::ffi::OsString>(), |name| {
            vars.iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| (*value).into())
        })
        .expect("valid launch");
        let f = FrontendParts::load(&config, crate::profile::SettingsLoad::default());
        assert!(!f.game.global_paused(), "{scene}: motion review runs live");
        assert_eq!(f.status.message, message, "{scene}");
    }
    // npc-stutter stages from a copied save; its staged note is what counts.
    let mut f = crate::ui::test_support::test_frontend();
    let config = LaunchConfig {
        scenario: Some("npc-stutter".into()),
        review: Some(ReviewOptions {
            motion: true,
            ..ReviewOptions::default()
        }),
        ..LaunchConfig::default()
    };
    let staged_note = "Copied checkpoint/resumed NPC battle · isolated performance review";
    f.status.message = staged_note.into();
    apply_motion_override(&mut f, &config);
    assert_eq!(f.status.message, staged_note);
}
#[test]
fn workshop_names_still_reach_the_workshop() {
    for scene in ["model-parity-x", "model-workshop"] {
        let f = staged(scene);
        assert!(f.screen == Screen::Camp, "{scene}");
        assert_eq!(f.nav.tab, Tab::World, "{scene}");
    }
}

/// The staged operations review with its live expedition observed, drawn as
/// the App draws it at 1050x700 pixels and scale 1.35 (777.8x518.5 points;
/// egui's raw screen rect is in points) with the
/// attempt set to `geometry` (lanes, width). Returns the window's recorded
/// field and the observer box: the clip of the battlefield painter, whose
/// height is the window's `(content height - 200).clamp(180, 360)` allocation.
fn observed_operations(f: &mut FrontendParts, geometry: (u8, f32)) -> (egui::Rect, egui::Rect) {
    let (field, observer_box, _) = observed_operations_output(f, geometry);
    (field, observer_box)
}
/// [`observed_operations`] plus the settled frame's output.
fn observed_operations_output(
    f: &mut FrontendParts,
    geometry: (u8, f32),
) -> (egui::Rect, egui::Rect, egui::FullOutput) {
    let id = f
        .game
        .battles()
        .iter()
        .find(|b| !b.settled && b.game().attempt().is_some())
        .map(|b| b.id)
        .expect("the operations review keeps a live expedition");
    // Presentation only: the window reads the geometry, nothing is ticked.
    let battle = f
        .game
        .battles_mut()
        .iter_mut()
        .find(|b| b.id == id)
        .unwrap();
    let a = battle.game_mut().attempt_mut().unwrap();
    (a.lane_count, a.width_yards) = geometry;
    f.campaign.observed = Some(id);
    let ctx = operations_context();
    let mut last = None;
    // Windows settle their size over the first passes; read a settled frame.
    for _ in 0..3 {
        last = Some(operations_frame(&ctx, f));
    }
    let output = last.unwrap();
    let content = ctx.content_rect();
    let field = f
        .battle
        .observer_arena_rect
        .unwrap_or_else(|| panic!("no observer field at {content:?}"));
    let box_height = (content.height() - 200.0).clamp(180.0, 360.0);
    let mut boxes: Vec<egui::Rect> = output
        .shapes
        .iter()
        .map(|s| s.clip_rect)
        .filter(|clip| clip.height() == box_height && clip.contains_rect(field))
        .collect();
    boxes.dedup();
    assert_eq!(boxes.len(), 1, "observer boxes {boxes:?} for {field:?}");
    (field, boxes[0], output)
}

/// A context configured as `observed_operations` draws: scale 1.35, warmed up.
fn operations_context() -> egui::Context {
    let ctx = egui::Context::default();
    crate::settings::install_font(&ctx);
    crate::settings::configure(&ctx, 1.35);
    let mut warmup = ctx.run_ui(egui::RawInput::default(), |_| {});
    warmup.textures_delta.clear();
    ctx
}
/// One 1050x700-pixel frame of the staged screen at scale 1.35.
fn operations_frame(ctx: &egui::Context, f: &mut FrontendParts) -> egui::FullOutput {
    f.runtime.begin_frame(ctx.cumulative_frame_nr() as u32);
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1050.0, 700.0) / 1.35,
            )),
            ..egui::RawInput::default()
        },
        |ui| crate::ui::draw_screen(ui.ctx(), &mut f.ctx(), 0.0),
    );
    output.textures_delta.clear();
    output
}

#[test]
fn observer_uses_fit_field() {
    let mut f = staged("operations");
    // The review's live expedition at a fresh 3x12 opening.
    let (lanes, width) = (3, 12.0);
    let (field, observer_box) = observed_operations(&mut f, (lanes, width));
    let fit = crate::art::fit_field(observer_box, observer_box, lanes, width);
    assert_eq!(
        field,
        fit.rect,
        "{lanes}x{width} in {observer_box:?}: field width is {:.2}x the box",
        field.width() / observer_box.width()
    );
    assert_eq!(field.width(), observer_box.width());
    assert!(fit.zoom > 1.0, "the 3x12 field zooms in: {}", fit.zoom);
}

/// The capture sidecar reads the observer's fitted field from
/// `observer_arena_rect`; the camp screen reports its tab and no battle field.
#[cfg(feature = "review")]
#[test]
fn review_frame_facts_report_the_observer_field() {
    let mut f = staged("operations");
    let (field, _) = observed_operations(&mut f, (3, 12.0));
    let facts = ReviewFrameFacts::collect(
        &f.game,
        &f.launch,
        &f.settings,
        f.screen,
        f.nav.tab,
        &f.battle,
    );
    let corners = [field.left(), field.top(), field.right(), field.bottom()];
    assert_eq!(facts.scene, "operations");
    assert_eq!(facts.observer_arena_rect, Some(corners));
    assert_eq!(facts.overlays, ["observer"]);
    assert_eq!(
        (facts.screen.as_str(), facts.tab.as_deref()),
        ("Camp", Some("Operations"))
    );
    assert_eq!((facts.arena_rect, facts.lanes), (None, None));
}

/// The observer's field is a fact of the operations tab only: a still taken
/// on another tab or screen never reports the rect the window last recorded.
#[cfg(feature = "review")]
#[test]
fn review_frame_facts_drop_the_observer_off_its_tab() {
    let mut f = staged("operations");
    observed_operations(&mut f, (3, 12.0));
    assert!(f.battle.observer_arena_rect.is_some());
    for (screen, tab) in [
        (Screen::Camp, Tab::World),
        (Screen::Battle, Tab::Operations),
        (Screen::Results, Tab::Operations),
    ] {
        let facts =
            ReviewFrameFacts::collect(&f.game, &f.launch, &f.settings, screen, tab, &f.battle);
        assert_eq!(facts.observer_arena_rect, None, "{screen:?} {tab:?}");
        assert!(!facts.overlays.contains(&"observer"), "{screen:?} {tab:?}");
    }
}

/// The sidecar's `ui_scale` is the zoom the still was drawn at: an
/// out-of-range `LASTLIGHT_REVIEW_SCALE` is clamped as `configure` clamps it.
#[cfg(feature = "review")]
#[test]
fn review_frame_facts_report_the_applied_ui_scale() {
    for (requested, applied) in [(1.5, 1.35), (0.5, 0.75), (1.2, 1.2)] {
        let mut f = crate::ui::test_support::test_frontend();
        f.launch = LaunchConfig {
            scenario: Some("menu".into()),
            review: Some(ReviewOptions {
                capture: Some("headless-review-capture.png".into()),
                scale: Some(requested),
                ..ReviewOptions::default()
            }),
            ..LaunchConfig::default()
        };
        prepare(&mut f);
        assert_eq!(f.settings.scale, requested, "the fixture keeps the request");
        let facts = ReviewFrameFacts::collect(
            &f.game,
            &f.launch,
            &f.settings,
            f.screen,
            f.nav.tab,
            &f.battle,
        );
        assert_eq!(facts.ui_scale, applied, "requested {requested}");
    }
}

/// A collapsed observer window draws no field, so it records none: the rect
/// of the frame before the collapse is not kept.
#[test]
fn a_collapsed_observer_records_no_field() {
    let mut f = staged("operations");
    observed_operations(&mut f, (3, 12.0));
    assert!(f.battle.observer_arena_rect.is_some());
    let id = f.campaign.observed.expect("observed");
    // egui 0.36 keys a window's area by `Id::new(title.text())`, an Option.
    let title = format!("Live battlefield #{id}");
    let window = egui::Id::new(Some(title.as_str()));
    let ctx = operations_context();
    let mut collapsing = egui::collapsing_header::CollapsingState::load_with_default_open(
        &ctx,
        window.with("collapsing"),
        true,
    );
    collapsing.set_open(false);
    collapsing.store(&ctx);
    for _ in 0..3 {
        operations_frame(&ctx, &mut f);
    }
    assert!(
        ctx.memory(|m| m.area_rect(window)).is_some(),
        "the collapsed window is still drawn"
    );
    assert_eq!(f.battle.observer_arena_rect, None);
}

/// An 11x30 field already fills the observer box, so the uniform fit leaves
/// it as the box itself. Perturbation proof (W1 review): giving the observer
/// 12 points of headroom (`fit_field(r, r.shrink2(vec2(0.0, 12.0)), ..)` in
/// `ui/operations.rs`) fails here with
/// `left: [[50.6 159.0] - [727.2 453.5]] right: [[23.0 147.0] - [754.8 465.5]]`.
#[test]
fn observer_filling_field_is_unchanged() {
    let mut f = staged("operations");
    let (field, observer_box) = observed_operations(&mut f, (11, 30.0));
    assert_eq!(field, observer_box, "an 11x30 field fills its box");
}

/// The observer caps its forest dressing at a lane of the reference clearing
/// in its box (`box height / REFERENCE_LANES`), as the main battle does: a
/// zoomed 3x12 field, whose lanes are far taller, never grows the props.
#[test]
fn observer_dressing_is_capped_at_the_reference_lane() {
    let props = egui::TextureId::User(2);
    let mut f = staged("operations");
    f.textures.props = Some(props);
    let (field, observer_box, output) = observed_operations_output(&mut f, (3, 12.0));
    let cap = observer_box.height() / crate::art::camera::REFERENCE_LANES;
    assert!(
        field.height() / 3.0 > 2.0 * cap,
        "the 3x12 lane must exceed the cap"
    );
    // Backdrop props are clipped to their peripheral strips, off the field.
    let sizes: Vec<egui::Vec2> = output
        .shapes
        .iter()
        .filter(|shape| !shape.clip_rect.intersect(field).is_positive())
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Mesh(mesh) if mesh.texture_id == props => Some(mesh.calc_bounds().size()),
            _ => None,
        })
        .collect();
    assert!(!sizes.is_empty(), "no dressing painted around {field:?}");
    // A tree is at most 1.8 units wide and 1.85 units tall.
    for size in sizes {
        assert!(
            size.x <= 1.8 * cap + 0.01 && size.y <= 1.85 * cap + 0.01,
            "a {size:?} prop exceeds the {cap} cap"
        );
    }
}

// ----- Wave-1 review scenes (design-qa-captures 1.8, W1-54) -----

#[test]
fn results_defeat_fixture_settles_a_real_defeat() {
    let f = staged("results-defeat");
    assert!(f.screen == Screen::Results, "{:?}", f.screen);
    assert!(f.game.attempt().is_none(), "the defense settled");
    let report = f.game.last_result().expect("a settled report");
    assert_eq!(report.outcome, horde_core::sim::Outcome::Defeat);
    assert!(!report.cause.is_empty(), "a defeat names its cause");
    assert!(!report.tip.is_empty(), "a defeat carries a tip");
    assert_eq!(
        (report.country_id.as_deref(), report.stage),
        (Some("DEU"), 0)
    );
    assert_eq!(f.game.campaign().run_history.len(), 1);
    assert!(f.game.global_paused());
}

#[test]
fn save_error_fixture_surfaces_the_failure_without_io() {
    let f = staged("save-error");
    assert!(f.screen == Screen::Camp, "{:?}", f.screen);
    assert_eq!(f.nav.tab, Tab::World);
    assert!(
        f.status.message.contains("Save failed"),
        "{:?}",
        f.status.message
    );
    assert!(f.status.message.contains("Access is denied. (os error 5)"));
    assert!(f.status.message_error);
    assert!(!f.paths.campaign().exists(), "no campaign.json was written");
    assert!(!f.paths.settings().exists(), "no settings.json was written");
    #[cfg(feature = "review")]
    {
        let metrics = f.runtime.persistence_metrics();
        assert_eq!(
            (
                metrics.completed_background_saves,
                metrics.failed_background_saves,
                metrics.pending
            ),
            (0, 0, false),
            "the failure is a notice, not a real save"
        );
    }
}

/// The first (country, stage) in world file order whose theme is the night
/// checkpoint: an oracle independent of the fixture's search helper.
fn first_night_checkpoint() -> (String, usize) {
    horde_core::world::world()
        .countries
        .iter()
        .find_map(|country| {
            (0..country.level_count())
                .find(|&stage| {
                    let theme = country.stage_theme(stage);
                    theme.lighting == "night" && theme.id == "checkpoint"
                })
                .map(|stage| (country.id.clone(), stage))
        })
        .expect("world.json has a night checkpoint stage")
}

#[test]
fn night_checkpoint_fixture_is_a_night_stage() {
    let f = staged("night-checkpoint");
    assert!(f.screen == Screen::Battle, "{:?}", f.screen);
    let a = f.game.attempt().expect("a deployed night stage");
    assert!(a.paused && f.game.global_paused(), "deployed paused");
    let (country, stage) = first_night_checkpoint();
    assert_eq!((a.country_id.as_deref(), a.stage), (Some(&*country), stage));
    let theme = horde_core::world::world()
        .country(&country)
        .unwrap()
        .stage_theme(a.stage);
    assert_eq!(
        (theme.id.as_str(), theme.lighting.as_str()),
        ("checkpoint", "night")
    );
    assert_eq!(f.campaign.selected_country, country);
    assert_eq!(f.campaign.mode, horde_core::content::Mode::Personal);
}

#[test]
fn weapon_classes_fixtures_equip_and_fire_their_class() {
    use horde_core::{
        content::{Mode, Slot, StructureKind},
        sim::EventKind,
        weapons::{WeaponClass, WeaponSpec},
    };
    for (scene, class) in [
        ("weapon-classes-pistol", WeaponClass::Pistol),
        ("weapon-classes-smg", WeaponClass::Smg),
        ("weapon-classes-shotgun", WeaponClass::Shotgun),
        ("weapon-classes-rifle", WeaponClass::Rifle),
        ("weapon-classes-marksman", WeaponClass::Marksman),
        ("weapon-classes-machine-gun", WeaponClass::MachineGun),
    ] {
        let f = staged(scene);
        assert!(f.screen == Screen::Battle, "{scene}: {:?}", f.screen);
        let equipped = f.game.campaign().equipped(Mode::Personal, Slot::Ranged);
        assert_eq!(equipped.tier, 1, "{scene}: {}", equipped.name);
        assert_eq!(WeaponSpec::from_item(equipped).class, class, "{scene}");
        let a = f.game.attempt().expect("a live battle");
        assert_eq!(a.mode, Mode::Personal, "{scene}");
        assert!(a.paused, "{scene}: paused on the shot");
        assert!(
            a.events
                .iter()
                .any(|event| event.kind == EventKind::Shot && event.actor_id.is_none()),
            "{scene}: the player fired"
        );
        assert!(f.battle.fire_latched, "{scene}: motion keeps firing");
    }
    let f = staged("weapon-classes-turret");
    assert!(f.screen == Screen::Battle, "{:?}", f.screen);
    let a = f.game.attempt().expect("a live battle");
    assert!(a.paused);
    let turrets: Vec<_> = a
        .structures
        .iter()
        .filter(|s| s.kind == StructureKind::Turret)
        .map(|s| s.id)
        .collect();
    assert!(!turrets.is_empty(), "a placed turret");
    assert!(
        a.events.iter().any(|event| event.kind == EventKind::Shot
            && event.actor_id.is_some_and(|id| turrets.contains(&id))),
        "the turret fired"
    );
}

#[test]
fn battle_mid_fixture_is_7x20() {
    // 7 lanes x 20 yd is not a geometry balance.json reaches at any level
    // (lanes and width grow together), so the scene records the nearest
    // reachable pair with the same lane count, 7 x 16 (design-qa-captures 1.8).
    let settings = horde_core::balance::BalanceSettings::default();
    let reachable: Vec<_> = (1..=horde_core::campaign::Player::level_cap())
        .map(|level| settings.geometry(level))
        .collect();
    assert!(!reachable.contains(&(7, 20.0)), "7x20 became reachable");
    assert!(reachable.contains(&(7, 16.0)));
    let f = staged("battle-mid");
    assert!(f.screen == Screen::Battle, "{:?}", f.screen);
    let a = f.game.attempt().expect("a live battle");
    assert_eq!((a.lane_count, a.width_yards), (7, 16.0));
    assert_eq!(f.game.campaign().arena_geometry(), (7, 16.0));
    assert!(a.paused);
    // Twenty unanswered seconds: a crowd of at least one enemy a lane is on
    // the field and the defender still stands. Two a lane is only reached
    // near defeat (about 14 enemies at a third of the defender's health).
    assert_eq!(
        a.tick,
        fixtures::production::BATTLE_MID_TICKS,
        "the fixture ran its full span"
    );
    assert!(
        a.enemies.len() >= usize::from(a.lane_count),
        "a mid-battle crowd: {} enemies on {} lanes",
        a.enemies.len(),
        a.lane_count
    );
    assert!(
        a.hp * 2 > a.max_hp,
        "the defender stands: {}/{}",
        a.hp,
        a.max_hp
    );
}

#[test]
fn operations_observer_fixture_opens_the_observer() {
    let mut f = staged("operations-observer");
    assert!(f.screen == Screen::Camp, "{:?}", f.screen);
    assert_eq!(f.nav.tab, Tab::Operations);
    let observed = f.campaign.observed.expect("the observer is open");
    assert!(
        f.game
            .battles()
            .iter()
            .any(|b| b.id == observed && !b.settled && b.game().attempt().is_some()),
        "it observes a live expedition"
    );
    one_frame(&mut f);
    assert!(
        f.battle.observer_arena_rect.is_some(),
        "the observer window drew its field"
    );
}

#[cfg(feature = "review")]
#[test]
fn fx_stress_aliases_the_stress_fixture() {
    let row = review_scenes::REVIEW_SCENES
        .iter()
        .find(|scene| scene.name == "fx-stress")
        .expect("fx-stress registry row");
    assert_eq!(row.kind, review_scenes::SceneKind::Still);
    assert!(!row.practice && !review_scenes::accepts("fx-stress", false));
    let stress = staged("stress");
    let fx = staged("fx-stress");
    assert!(fx.screen == stress.screen && fx.screen == Screen::Battle);
    assert_eq!(fx.campaign.mode, stress.campaign.mode);
    assert_eq!(
        fx.campaign.selected_country,
        stress.campaign.selected_country
    );
    assert_eq!(fx.status.message, stress.status.message);
    let (a, b) = (fx.game.attempt().unwrap(), stress.game.attempt().unwrap());
    assert!(a.paused);
    assert_eq!(a.enemies.len(), b.enemies.len());
    assert_eq!(
        (a.tick, a.lane_count, a.width_yards, a.stage),
        (b.tick, b.lane_count, b.width_yards, b.stage)
    );
    assert_eq!(fx.game.battles().len(), stress.game.battles().len());
}

// ----- Wave-2 staging helpers (design-qa-captures Wave 2, W2-46) -----

/// A frontend as the matrix leaves it before a scene's own staging: a fresh
/// game with its survivor chosen.
fn unstaged_frontend() -> FrontendParts {
    let mut f = crate::ui::test_support::test_frontend();
    f.game = Game::default();
    f.game
        .campaign_mut()
        .choose_survivor(SurvivorId::Rook)
        .unwrap();
    f
}

#[test]
fn equipment_autosell_rule_counts_only_below_uncommon_in_core() {
    use horde_core::{campaign::AutoSellRule, quality::Quality, sim::AutoSellScope};
    let mut f = unstaged_frontend();
    let staged = fixtures::production::stage_equipment_autosell(&mut f);
    let qualities: Vec<Quality> = staged.iter().map(|(quality, _)| *quality).collect();
    assert_eq!(qualities, Quality::ALL, "one drop of every quality");
    let rule = f.game.campaign().auto_sell;
    assert_eq!(
        rule,
        AutoSellRule {
            enabled: true,
            sell_below: Quality::Uncommon,
            ..AutoSellRule::default()
        },
        "the rule was stored through SetAutoSell"
    );
    for (quality, id) in &staged {
        let item = f
            .game
            .campaign()
            .item(*id)
            .expect("the staged drop is owned");
        assert_eq!(item.quality, Some(*quality));
    }
    // Decision 6: strictly below Uncommon, so the Poor and the Common drop
    // of the six. (design-qa-captures reads "1 of 6"; the core rule sells 2.)
    let below: Vec<u64> = staged
        .iter()
        .filter(|(quality, _)| *quality < Quality::Uncommon)
        .map(|(_, id)| *id)
        .collect();
    assert_eq!(below.len(), 2, "Poor and Common");
    let preview = f.game.auto_sell_preview(&rule, AutoSellScope::Stash);
    assert_eq!(preview.ids, below, "{preview:?}");
    assert_eq!(preview.count, 2);
    for quality in Quality::ALL {
        let expected = u32::from(quality < Quality::Uncommon);
        assert_eq!(preview.by_quality[quality], expected, "{quality:?}");
    }
    assert_eq!((preview.kept_upgrades, preview.blocked), (0, 0));
    assert!(f.game.attempt().is_none(), "staged in camp");
}

#[test]
fn briefing_recruit_staging_sets_next_defender_through_command() {
    use horde_core::{defenders::DefenderId, strategy::RecruitStatus};
    let mut f = unstaged_frontend();
    let id = fixtures::production::stage_briefing_recruit(&mut f);
    let recruit = f
        .game
        .strategy()
        .roster
        .iter()
        .find(|r| r.id == id)
        .expect("the staged recruit is on the roster");
    assert_eq!(recruit.status, RecruitStatus::Ready);
    assert_eq!(
        f.game.campaign().next_defender,
        Some(DefenderId::Recruit(id))
    );
    assert_eq!(f.game.next_defender(), DefenderId::Recruit(id));
    assert!(f.game.attempt().is_none(), "chosen between defenses");
    let options = f.game.available_defenders();
    let selected: Vec<DefenderId> = options
        .iter()
        .filter(|option| option.selected)
        .map(|option| option.card.id)
        .collect();
    assert_eq!(selected, [DefenderId::Recruit(id)]);
    assert!(
        options
            .iter()
            .find(|option| option.card.id == DefenderId::Recruit(id))
            .is_some_and(|option| option.selectable.is_ok()),
        "the recruit can defend next"
    );
}
