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
    ("skills", Screen::Upgrades, Some(Tab::Skills)),
    ("skills-survival", Screen::Upgrades, Some(Tab::Skills)),
    ("skills-ordnance", Screen::Upgrades, Some(Tab::Skills)),
    ("ordnance", Screen::Battle, None),
    ("engineering", Screen::Upgrades, None),
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
    ("skills-search", Screen::Upgrades, None),
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
    // Wave-2 close stills (W2-55).
    ("boss-phase", Screen::Battle, None),
    ("skills-firearms", Screen::Upgrades, Some(Tab::Skills)),
    // W2-57 (CF-2-04).
    ("spitter-windup", Screen::Battle, None),
    // Wave-3 UI scenes (W3-27). `upgrades` follows `open_upgrades`.
    ("upgrades", Screen::Upgrades, None),
    ("camp-home", Screen::Camp, Some(Tab::Home)),
    ("briefing-recruit", Screen::Camp, Some(Tab::Briefing)),
    ("equipment-autosell", Screen::Camp, Some(Tab::Armory)),
    ("results-liberation-recruit", Screen::Results, None),
    ("takeover-battle", Screen::Battle, None),
    // W5-09.
    ("dawn-reference", Screen::Battle, None),
    ("elite-tells", Screen::Battle, None),
    // W5-10.
    ("storm", Screen::Battle, None),
    // Closing phase: the W3-14 boss banner.
    ("boss-warning", Screen::Battle, None),
    // Closing phase: the W3-25 expedition toasts.
    ("camp-reports", Screen::Camp, Some(Tab::Home)),
    // Closing phase: the UX-16 callout row on Camp home and on Upgrades.
    ("callout-camp", Screen::Camp, Some(Tab::Home)),
    ("callout-upgrades", Screen::Upgrades, None),
    // Closing phase: the W3-15 low-HP band, personal and observed.
    ("low-hp", Screen::Battle, None),
    ("low-hp-observed", Screen::Battle, None),
    // Closing phase: W2-41's Firearms column at -1780, rows 0 to 380.
    (
        "skills-firearms-column",
        Screen::Upgrades,
        Some(Tab::Skills),
    ),
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
    one_frame_with_context(f).1
}

/// [`one_frame`], also returning the context it drew in (its primary
/// registry holds the frame's primaries).
fn one_frame_with_context(f: &mut FrontendParts) -> (egui::Context, egui::FullOutput) {
    frames_with_context(f, 1)
}

/// `frames` headless 1440x900 frames on one context, returning the last: a
/// modal paints from its second frame (the first only sizes it).
fn frames_with_context(f: &mut FrontendParts, frames: usize) -> (egui::Context, egui::FullOutput) {
    let ctx = egui::Context::default();
    crate::settings::install_font(&ctx);
    let mut warmup = ctx.run_ui(egui::RawInput::default(), |_| {});
    warmup.textures_delta.clear();
    let mut last = None;
    for _ in 0..frames.max(1) {
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
        last = Some(output);
    }
    (ctx, last.expect("at least one frame"))
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
        let (ctx, output) = one_frame_with_context(&mut f);
        if let Some(primary) = scene.primary {
            assert_eq!(
                crate::ui::tests::text_count(&output, primary),
                1,
                "{name}: one {primary:?}"
            );
            // The amber primary (AMBER fill, INK text) is the one the
            // frame's primary registry recorded on the screen surface.
            let screen: Vec<_> = crate::ui::frame_primaries(&ctx)
                .into_iter()
                .filter(|record| record.surface == crate::ui::Surface::Screen)
                .map(|record| record.label)
                .collect();
            assert_eq!(
                screen,
                [primary],
                "{name}: {primary:?} is the amber primary"
            );
            let color = output
                .shapes
                .iter()
                .find_map(|clipped| crate::ui::tests::text_color(&clipped.shape, primary));
            assert_eq!(
                color,
                Some(crate::art::INK),
                "{name}: {primary:?} is INK on amber"
            );
        }
        checked += 1;
    }
    assert!(checked > 0);
}
/// The screens each `stills_*_prepare_headless` test checks, one group a
/// test; `every_expected_screen_has_a_stills_test` keeps every screen EXPECTED
/// names in one of them.
const STILL_SCREEN_GROUPS: &[&[Screen]] = &[
    &[
        Screen::Menu,
        Screen::SurvivorSelection,
        Screen::Camp,
        Screen::Settings,
    ],
    &[Screen::Battle],
    &[Screen::Results, Screen::History],
    &[Screen::Upgrades],
];
#[test]
fn stills_menus_and_camp_prepare_headless() {
    stills_prepare_headless(STILL_SCREEN_GROUPS[0]);
}
#[test]
fn stills_battles_prepare_headless() {
    stills_prepare_headless(STILL_SCREEN_GROUPS[1]);
}
#[test]
fn stills_results_and_history_prepare_headless() {
    stills_prepare_headless(STILL_SCREEN_GROUPS[2]);
}
#[test]
fn stills_upgrades_prepare_headless() {
    stills_prepare_headless(STILL_SCREEN_GROUPS[3]);
}
#[test]
fn every_expected_screen_has_a_stills_test() {
    for screen in Screen::ALL {
        let named = EXPECTED.iter().any(|(_, s, _)| *s == screen);
        let tested = STILL_SCREEN_GROUPS
            .iter()
            .any(|group| group.contains(&screen));
        assert!(
            !named || tested,
            "{screen:?} has stills but no stills_*_prepare_headless test checks them"
        );
    }
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
        load_error: None,
        settings_warning: None,
        settings_at_risk: false,
    };
    let mut f = FrontendParts::new(profile, &config, &paths);
    prepare(&mut f);
    f
}
/// CF-3-13: a still's footer status line never depends on how long the
/// launch took. The status line's 5 s wall-clock fade decided whether the
/// "Campaign restored…" line of a reused review folder (CF-3-24) and its
/// Dismiss showed, so the normal and reduced briefing-npc stills differed.
/// Startup reads that journey, the scene replaces it, and in both modes,
/// with the capture frame 0.5 s or 8 s after the first, the stills carry one
/// status state: no restored line, and a line staged after the load (here
/// a plan save) shown at full strength.
#[test]
fn briefing_npc_stills_carry_one_status_state_in_both_modes() {
    use crate::ui::tests::text_count;
    const STAGED: &str = "Country plan saved. Next deployment pays its supply costs.";
    let mut states = Vec::new();
    for staged_note in [false, true] {
        for reduced in [false, true] {
            for capture_at in [0.5, 8.0] {
                let config = LaunchConfig {
                    scenario: Some("briefing-npc".into()),
                    review: Some(ReviewOptions {
                        capture: Some("headless-review-capture.png".into()),
                        exit: true,
                        reduced,
                        ..ReviewOptions::default()
                    }),
                    ..LaunchConfig::default()
                };
                let paths = crate::profile::ProfilePaths::in_directory(
                    std::env::temp_dir().join(format!("lastlight-cf313-{}", std::process::id())),
                );
                let profile = crate::profile::LoadedProfile {
                    game: Game::default(),
                    settings: crate::settings::Settings::default(),
                    warning: crate::profile::CAMPAIGN_RESTORED.into(),
                    load_error: None,
                    settings_warning: None,
                    settings_at_risk: false,
                };
                let mut f = FrontendParts::new(profile, &config, &paths);
                prepare(&mut f);
                apply_reduced_override(&mut f, true, false, reduced);
                if staged_note {
                    f.status.message = STAGED.into();
                }
                let ctx = egui::Context::default();
                crate::settings::install_font(&ctx);
                let mut warmup = ctx.run_ui(egui::RawInput::default(), |_| {});
                warmup.textures_delta.clear();
                let mut last = None;
                for time in [0.0, capture_at] {
                    f.runtime.begin_frame(ctx.cumulative_frame_nr() as u32);
                    let mut output = ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                egui::vec2(1440.0, 900.0),
                            )),
                            time: Some(time),
                            ..egui::RawInput::default()
                        },
                        |ui| crate::ui::draw_screen(ui.ctx(), &mut f.ctx(), 0.0),
                    );
                    output.textures_delta.clear();
                    last = Some(output);
                }
                let output = last.expect("a capture frame");
                // The painted strength of the run: its colour's alpha times
                // the opacity of the scope it was painted in.
                fn strength(shape: &egui::epaint::Shape, label: &str) -> Option<f32> {
                    match shape {
                        egui::epaint::Shape::Text(t) if t.galley.text() == label => {
                            let color = t.galley.job.sections.first()?.format.color;
                            Some(f32::from(color.a()) / 255.0 * t.opacity_factor)
                        }
                        egui::epaint::Shape::Vec(v) => v.iter().find_map(|s| strength(s, label)),
                        _ => None,
                    }
                }
                let alpha = |label: &str| {
                    output
                        .shapes
                        .iter()
                        .find_map(|clipped| strength(&clipped.shape, label))
                };
                states.push((
                    staged_note,
                    reduced,
                    capture_at,
                    text_count(&output, crate::profile::CAMPAIGN_RESTORED),
                    text_count(&output, STAGED),
                    alpha(STAGED),
                    text_count(&output, "Dismiss"),
                ));
            }
        }
    }
    for state in &states {
        let (staged_note, _, _, restored, note, alpha, dismiss) = *state;
        assert_eq!(restored, 0, "the replaced journey's line shows: {state:?}");
        assert_eq!(
            (note, alpha.is_some_and(|a| a > 0.999), dismiss),
            if staged_note {
                (1, true, 1)
            } else {
                (0, false, 0)
            },
            "{state:?} of {states:?}"
        );
    }
}
/// CF-3-24: an isolated review (a staged scenario outside practice) starts
/// from a fresh profile. Its folder, `<temp>/lastlight-review-<pid>`, can be
/// left by an earlier process that had the same PID; the campaign and
/// settings there are never read, so the still shows no "Campaign restored"
/// line and the default settings. A practice scenario still resumes its own
/// saved profile.
#[cfg(feature = "review")]
#[test]
fn an_isolated_review_over_a_reused_folder_starts_from_a_fresh_profile() {
    use crate::profile::{ProfilePaths, startup_settings};
    use crate::settings::Settings;
    let parse = |vars: &[(&str, &str)]| {
        LaunchConfig::parse(std::iter::empty::<std::ffi::OsString>(), |name| {
            vars.iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| (*value).into())
        })
        .expect("valid launch")
    };
    let directory = std::env::temp_dir().join(format!("lastlight-cf324-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    let paths = ProfilePaths::in_directory(directory.clone());
    let review = parse(&[("LASTLIGHT_REVIEW_SCREEN", "history-empty")]);
    // What the earlier process with this PID left: its settings, and first
    // an unreadable campaign, then a readable journey.
    let stale_settings = Settings {
        master: 0.25,
        reduced_effects: true,
        ..Settings::default()
    };
    stale_settings.save(&paths.settings()).unwrap();
    std::fs::write(paths.campaign(), b"{\"truncated").unwrap();
    let mut stale = Game::default();
    stale
        .campaign_mut()
        .choose_survivor(SurvivorId::Rook)
        .unwrap();
    for readable in [false, true] {
        if readable {
            stale.save(&paths.campaign()).unwrap();
        }
        let settings = startup_settings(&review, &paths);
        assert_eq!(
            settings.settings,
            Settings::default(),
            "stale settings read"
        );
        assert!(settings.warning.is_none());
        let f = FrontendParts::load_from(&review, &paths, settings);
        assert_eq!(
            f.status.message, "",
            "readable {readable}: the stale folder's status shows"
        );
        assert!(!f.status.message_error, "readable {readable}");
    }

    let practice = parse(&[
        ("LASTLIGHT_PRACTICE", "1"),
        ("LASTLIGHT_REVIEW_SCREEN", "construction"),
    ]);
    let settings = startup_settings(&practice, &paths);
    assert_eq!(
        settings.settings.master, 0.25,
        "practice reads its settings"
    );
    let f = FrontendParts::load_from(&practice, &paths, settings);
    assert_eq!(
        f.game.campaign().selected_survivor,
        Some(SurvivorId::Rook),
        "practice resumes its saved journey"
    );
    std::fs::remove_dir_all(directory).unwrap();
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
/// battle and "Save and return to main menu" reaches the menu.
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
        Some(&f.status),
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
        let facts = ReviewFrameFacts::collect(
            &f.game,
            &f.launch,
            &f.settings,
            Some(&f.status),
            screen,
            tab,
            &f.battle,
        );
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
            Some(&f.status),
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
    let window = crate::ui::operations::observer_window_id(id);
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

/// The meshes drawn inside `clip` from egui-managed textures: in the
/// observer box these are the survivor rig's parts, sampled from the player
/// source sheets (the two models share sheets, so the parts' regions tell
/// them apart). Each `observed_operations_output` call draws on a fresh
/// context, which uploads the sources in catalogue order, so the texture ids
/// compare across calls.
fn managed_meshes(output: &egui::FullOutput, clip: egui::Rect) -> Vec<egui::Mesh> {
    fn collect(shape: &egui::Shape, into: &mut Vec<egui::Mesh>) {
        match shape {
            egui::Shape::Vec(shapes) => shapes.iter().for_each(|shape| collect(shape, into)),
            egui::Shape::Mesh(mesh) => {
                if matches!(mesh.texture_id, egui::TextureId::Managed(_)) {
                    into.push((**mesh).clone());
                }
            }
            _ => {}
        }
    }
    let mut meshes = Vec::new();
    for clipped in output.shapes.iter().filter(|s| s.clip_rect == clip) {
        collect(&clipped.shape, &mut meshes);
    }
    meshes
}

/// The observer window is the expedition recruit's (W3-17, ruling D17): its
/// title names the recruit from `Game::battle_defender_card`, and the field
/// draws the worker attempt's stamped appearance instead of Mara's Female.
#[test]
fn observer_window_draws_the_expedition_recruit_not_mara() {
    use horde_core::survivors::Appearance;
    let mut f = staged("operations");
    let (_, observer_box, output) = observed_operations_output(&mut f, (11, 30.0));
    let id = f.campaign.observed.expect("observed");
    let card = f
        .game
        .battle_defender_card(id)
        .expect("the expedition's recruit is on the roster");
    assert_ne!(card.name, "Mara");
    let title = format!("Live battlefield #{id} · {}", card.name);
    let titles: Vec<String> = output
        .shapes
        .iter()
        .filter_map(|s| match &s.shape {
            egui::Shape::Text(text) => Some(text.galley.text().to_owned()),
            _ => None,
        })
        .filter(|text| text.starts_with("Live battlefield"))
        .collect();
    assert_eq!(titles, [title], "the window title names the recruit");

    // The worker attempt carries the recruit's identity appearance, and the
    // field draws that model: stamping the other appearance changes the rig.
    let worker_appearance = |f: &FrontendParts| {
        let battle = f.game.battles().iter().find(|b| b.id == id).unwrap();
        battle.game().attempt().unwrap().defender_appearance()
    };
    assert_eq!(worker_appearance(&f), card.appearance);
    let own = managed_meshes(&output, observer_box);
    assert!(!own.is_empty(), "the rig draws from its source sheets");
    let other = match card.appearance {
        Appearance::Male => Appearance::Female,
        Appearance::Female => Appearance::Male,
    };
    let battle = f
        .game
        .battles_mut()
        .iter_mut()
        .find(|b| b.id == id)
        .unwrap();
    battle.game_mut().attempt_mut().unwrap().profile.appearance = other;
    let (_, observer_box, output) = observed_operations_output(&mut f, (11, 30.0));
    // `assert!`, not `assert_ne!`: a failure must not dump the meshes.
    assert!(
        managed_meshes(&output, observer_box) != own,
        "the observer draws the worker attempt's appearance, {:?} then {other:?}",
        card.appearance
    );
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
        // Decision 16 (W5-10): the class the weapon effects draw is the one
        // frozen in the deploy's gear snapshot, not a later re-read of the
        // equipped item.
        let ranged: Vec<_> = a
            .gear_snapshot
            .iter()
            .filter(|record| record.slot == Slot::Ranged)
            .map(|record| record.weapon_class)
            .collect();
        assert_eq!(ranged, [Some(class)], "{scene}: the snapshot's class");
        assert_eq!(crate::art::shooter_class(a), class, "{scene}");
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
    // of the six (design-qa-captures expects 2 since the W2 review).
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

/// The briefing reads core's next defender (W3-16), so the Mara briefing
/// scene stages her through `SetNextDefender`.
#[test]
fn briefing_npc_scene_stages_mara_as_the_next_defender() {
    use horde_core::defenders::DefenderId;
    let f = staged("briefing-npc");
    assert!(f.screen == Screen::Camp && f.nav.tab == Tab::Briefing);
    assert_eq!(f.game.campaign().next_defender, Some(DefenderId::Mara));
    assert!(f.game.attempt().is_none());
    let personal = staged("briefing");
    assert_eq!(personal.game.next_defender(), DefenderId::Player);
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

// ----- Wave-2 close stills (W2-55) -----

/// W2-37's close capture needs a boss inside its phase hold: the boss-phase
/// still stages a boss past rank 4 that crossed half its hit points, paused
/// on the tick it entered the hold (CF-2-03: the whole 90 ticks left for a
/// motion export), so the view carries the phase in normal and reduced
/// effects.
#[test]
fn boss_phase_fixture_pauses_a_boss_inside_its_phase_hold() {
    assert_eq!(
        review_scenes::lookup("boss-phase"),
        Some(review_scenes::SceneKind::Still),
        "boss-phase is a registered still"
    );
    let f = staged("boss-phase");
    assert!(f.screen == Screen::Battle, "staged {:?}", f.screen);
    let a = f.game.attempt().expect("a live battle");
    assert!(a.paused, "the still is paused in the hold");
    assert!(
        a.progression.rank > 4,
        "rank {} has no phases",
        a.progression.rank
    );
    let boss = a.enemies.iter().find(|e| e.boss).expect("a boss");
    assert!(boss.boss_held(a.tick), "tick {}: the boss holds", a.tick);
    for reduced in [false, true] {
        let view = crate::battle_view::BattleViewModel::build(
            a,
            crate::animation::SceneAnimation::sample(a, reduced),
            crate::battle_view::SceneBlend::NONE,
            reduced,
        );
        let phase = view
            .boss_phase
            .unwrap_or_else(|| panic!("reduced {reduced}: the view carries the phase"));
        assert_eq!((phase.boss_id, phase.transition), (boss.id, 1));
        assert_eq!(
            phase.hold_left_ticks, 90,
            "reduced {reduced}: the whole hold is left"
        );
    }
}

/// The motion export of `scene` in `reduced` effects, run headless as the App
/// runs it. Per native frame: `RuntimeSet::Apply`, one fixed step (the
/// export's `FixedTimesteps(1)`), Update (`capture_frames`, which records the
/// frame and schedules the release), then the egui pass, which draws the
/// screen on one persistent context and publishes the controls it holds:
/// enabled only while the attempt runs, so a release's first tick waits for
/// the pass after the release applies (CF-3-29). `on_capture` sees every
/// captured frame, by index, after its egui pass. `staged` sees the loaded
/// fixture before the first native frame. Each export saves to a folder of
/// its own: the App's per-process review folder is shared by every test in
/// the process, so a parallel test's fresh profile would fail this one's
/// autosave, and the save-error modal would pause the battle.
#[cfg(feature = "review")]
fn run_motion_export(
    scene: &str,
    reduced: bool,
    staged: impl FnOnce(&FrontendParts),
    mut on_capture: impl FnMut(u32, &mut FrontendParts),
) {
    use crate::capture_sequence::{LAST, frame_index};
    let vars = [
        ("LASTLIGHT_REVIEW_SCREEN", scene),
        ("LASTLIGHT_REVIEW_MOTION", "1"),
        ("LASTLIGHT_CAPTURE_SEQUENCE", "frames"),
    ];
    let config = LaunchConfig::parse(std::iter::empty::<std::ffi::OsString>(), |name| {
        vars.iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| (*value).into())
    })
    .expect("valid export launch");
    let folder = std::env::temp_dir().join(format!(
        "lastlight-motion-test-{}-{scene}-{reduced}",
        std::process::id()
    ));
    let mut f = FrontendParts::load_from(
        &config,
        &crate::profile::ProfilePaths::in_directory(folder.clone()),
        crate::profile::SettingsLoad::default(),
    );
    f.settings.reduced_effects = reduced;
    staged(&f);
    let ctx = egui::Context::default();
    crate::settings::install_font(&ctx);
    let mut warmup = ctx.run_ui(egui::RawInput::default(), |_| {});
    warmup.textures_delta.clear();
    for frame in 1..=LAST {
        f.flush();
        f.step();
        schedule_motion_release(&config, &mut f.runtime, frame);
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
        if let Some(index) = frame_index(frame) {
            on_capture(index, &mut f);
        }
    }
    drop(f);
    let _ = std::fs::remove_dir_all(folder);
}

/// CF-2-03: the boss-phase motion export plays the whole hold. Its warm-up
/// frames hold the staged entry tick, and the export resumes the defense so
/// that frame-0000 draws its first tick (`review::motion_release_frame`), so
/// frames 0-22 draw the phase on ticks 1-89 of the 90-tick hold (normal and
/// reduced) and frame 23 its end ([`run_motion_export`] stands in for the App).
#[cfg(feature = "review")]
#[test]
fn boss_phase_motion_export_plays_the_whole_hold() {
    use crate::capture_sequence::COUNT;
    let mut boss = None;
    let mut frames = Vec::new();
    run_motion_export(
        "boss-phase",
        false,
        |f| {
            assert!(
                f.game.global_paused(),
                "the export's warm-up holds the staged entry tick"
            );
            boss = f
                .game
                .attempt()
                .and_then(|a| a.enemies.iter().find(|e| e.boss))
                .map(|e| e.id);
        },
        |index, f| {
            let a = f.game.attempt().expect("the defense stands");
            let phase = [false, true].map(|reduced| {
                crate::battle_view::BattleViewModel::build(
                    a,
                    crate::animation::SceneAnimation::sample(a, reduced),
                    crate::battle_view::SceneBlend::NONE,
                    reduced,
                )
                .boss_phase
                .map(|p| (p.boss_id, p.transition, p.hold_left_ticks))
            });
            assert_eq!(phase[0], phase[1], "frame {index}: both modes agree");
            frames.push(phase[0]);
        },
    );
    let boss = boss.expect("a boss");
    let expected: Vec<_> = (0..u64::from(COUNT))
        .map(|k| (k < 23).then(|| (boss, 1, 89 - 4 * k)))
        .collect();
    assert_eq!(frames, expected);
}

// ----- Closing-phase stills -----

/// The W3-14 boss banner gets a deterministic capture: the boss-warning
/// still is a paused stage-0 defense on the tick its director warned, with
/// the banner one second into its 2.0 s, so normal and reduced effects both
/// draw "BOSS APPROACHING" at full strength and the sidecar reports its rect.
#[test]
fn boss_warning_still_draws_the_banner_on_the_warning_tick() {
    assert_eq!(
        review_scenes::lookup("boss-warning"),
        Some(review_scenes::SceneKind::Still),
        "boss-warning is a registered still"
    );
    for reduced in [false, true] {
        let mut f = staged("boss-warning");
        f.settings.reduced_effects = reduced;
        assert!(f.screen == Screen::Battle, "staged {:?}", f.screen);
        let a = f.game.attempt().expect("a live battle");
        assert!(a.paused, "the still is paused");
        let due = a.boss_warning_tick().expect("the director warned");
        assert_eq!(a.tick, due, "paused on the warning tick");
        assert!(!a.boss_spawned);
        let output = one_frame(&mut f);
        let banner = f
            .battle
            .boss_banner_rect
            .unwrap_or_else(|| panic!("reduced {reduced}: the banner is drawn"));
        assert!(
            painted_text(&output)
                .iter()
                .any(|text| text == "BOSS APPROACHING"),
            "reduced {reduced}: the banner's title"
        );
        // A second frame of the paused still draws the same banner.
        one_frame(&mut f);
        assert_eq!(f.battle.boss_banner_rect, Some(banner), "reduced {reduced}");
    }
}

/// The capture sidecar reports the boss banner the battle frame painted
/// (`BattleUi::boss_banner_rect`), on the battle screen only.
#[cfg(feature = "review")]
#[test]
fn review_frame_facts_report_the_boss_banner() {
    let mut f = staged("boss-warning");
    one_frame(&mut f);
    let banner = f.battle.boss_banner_rect.expect("the banner is drawn");
    let corners = [banner.left(), banner.top(), banner.right(), banner.bottom()];
    let facts = ReviewFrameFacts::collect(
        &f.game,
        &f.launch,
        &f.settings,
        Some(&f.status),
        f.screen,
        f.nav.tab,
        &f.battle,
    );
    assert_eq!(facts.boss_banner_rect, Some(corners));
    for screen in [Screen::Results, Screen::Camp] {
        let facts = ReviewFrameFacts::collect(
            &f.game,
            &f.launch,
            &f.settings,
            Some(&f.status),
            screen,
            f.nav.tab,
            &f.battle,
        );
        assert_eq!(facts.boss_banner_rect, None, "{screen:?}");
    }
}

/// The capture sidecar reports whether the battle frame drew the low-HP band
/// (`BattleUi::low_hp`, UX-12), on the battle screen only, so a capture
/// review can assert the band on the low-hp stills and its absence elsewhere.
#[cfg(feature = "review")]
#[test]
fn review_frame_facts_report_the_low_hp_band() {
    let facts = |f: &FrontendParts, screen: Screen| {
        ReviewFrameFacts::collect(
            &f.game,
            &f.launch,
            &f.settings,
            Some(&f.status),
            screen,
            f.nav.tab,
            &f.battle,
        )
    };
    for (scene, low) in [
        ("low-hp", true),
        ("low-hp-observed", true),
        ("battle", false),
    ] {
        let mut f = staged(scene);
        one_frame(&mut f);
        assert_eq!(facts(&f, f.screen).low_hp, Some(low), "{scene}");
        for screen in [Screen::Results, Screen::Camp] {
            assert_eq!(facts(&f, screen).low_hp, None, "{scene} on {screen:?}");
        }
    }
}

/// The boss-warning motion export holds the tick before the warning through
/// the warm-up and resumes it so that frame-0000 draws the warning tick (as
/// the boss-phase export does), and every later frame is four ticks on, all
/// inside the banner's 2.0 s. CF-3-29: frame-0000 carries the banner, at
/// full strength in reduced effects and on its first fade step (started,
/// nothing shown yet) in normal effects, whose frame 1 draws it fading in.
#[cfg(feature = "review")]
#[test]
fn boss_warning_motion_export_starts_on_the_warning_tick() {
    use crate::capture_sequence::COUNT;
    for reduced in [false, true] {
        let mut staged = None;
        let mut frames = Vec::new();
        let mut banners = Vec::new();
        run_motion_export(
            "boss-warning",
            reduced,
            |f| {
                assert!(f.game.global_paused(), "the warm-up holds the staged tick");
                let a = f.game.attempt().expect("a live battle");
                assert_eq!(a.boss_warning_tick(), None, "staged before the warning");
                staged = Some(a.tick);
            },
            |_, f| {
                let a = f.game.attempt().expect("the defense stands");
                frames.push((a.tick, a.boss_warning_tick()));
                banners.push((
                    f.battle.boss_banner.map(|b| (b.key.1, b.shown)),
                    f.battle.boss_banner_rect.is_some(),
                ));
            },
        );
        let due = staged.expect("staged") + 1;
        let expected: Vec<_> = (0..u64::from(COUNT))
            .map(|k| (due + 4 * k, Some(due)))
            .collect();
        assert_eq!(frames, expected, "reduced {reduced}");
        assert_eq!(
            banners[0],
            (Some((due, 0.0)), reduced),
            "reduced {reduced}: frame-0000 starts the banner"
        );
        for (index, (banner, drawn)) in banners.iter().enumerate().skip(1) {
            assert!(
                *drawn && banner.is_some_and(|(tick, _)| tick == due),
                "reduced {reduced}: frame {index} draws the banner ({banner:?})"
            );
        }
    }
}

/// W3-25's expedition notices get a deterministic capture: the camp-reports
/// still is Camp home with two settled expeditions the player has not
/// acknowledged and a folded count, so its first frame toasts both outcome
/// lines and the folded summary (three toasts, the whole visible stack).
#[test]
fn camp_reports_still_toasts_two_outcomes_and_the_folded_count() {
    assert_eq!(
        review_scenes::lookup("camp-reports"),
        Some(review_scenes::SceneKind::Still),
        "camp-reports is a registered still"
    );
    let mut f = staged("camp-reports");
    assert!(f.screen == Screen::Camp, "staged {:?}", f.screen);
    assert_eq!(f.nav.tab, Tab::Home);
    assert!(f.game.global_paused(), "the world clock is held");
    let outcomes = f.game.unacknowledged_outcomes();
    assert_eq!(outcomes.notices.len(), 2, "two settled outcomes");
    assert_eq!(outcomes.folded, fixtures::production::CAMP_REPORTS_FOLDED);
    let lines: Vec<String> = outcomes
        .notices
        .iter()
        .map(|notice| {
            crate::ui::expedition_notices::outcome_line(
                &notice.recruit_name,
                &notice.country_name,
                &notice.summary,
            )
        })
        .collect();
    assert_ne!(lines[0], lines[1], "two recruits, two lines");
    let output = one_frame(&mut f);
    let painted = painted_text(&output);
    for line in &lines {
        assert!(painted.contains(line), "{line:?} in {painted:?}");
    }
    let folded = format!(
        "{} more expedition outcomes · Operations",
        fixtures::production::CAMP_REPORTS_FOLDED
    );
    assert!(painted.contains(&folded), "{folded:?} in {painted:?}");
    assert_eq!(f.status.toasts.len(), 3, "the whole visible stack");
}

/// CF-3-26 (closing): the top-right toast stack starts under the camp tab
/// row. At a fixed 52 pt under the screen top the first toast lay on the tab
/// row at 1050x700@1.35 (777 pt wide), over "Character" and "World map".
/// Checked on camp-reports' three toasts at both sizes: every toast line
/// lies below every tab label and meets none.
#[test]
fn camp_toast_stack_starts_under_the_tab_row_at_both_sizes() {
    fn runs(output: &egui::FullOutput) -> Vec<(String, egui::Rect)> {
        fn walk(shape: &egui::epaint::Shape, found: &mut Vec<(String, egui::Rect)>) {
            match shape {
                egui::epaint::Shape::Text(text) => found.push((
                    text.galley.text().replace('\n', " "),
                    text.galley.rect.translate(text.pos.to_vec2()),
                )),
                egui::epaint::Shape::Vec(shapes) => {
                    shapes.iter().for_each(|shape| walk(shape, found));
                }
                _ => {}
            }
        }
        let mut found = Vec::new();
        for clipped in &output.shapes {
            walk(&clipped.shape, &mut found);
        }
        found
    }
    for (size, scale) in [
        (egui::vec2(1440.0, 900.0), 1.0),
        (egui::vec2(1050.0, 700.0) / 1.35, 1.35),
    ] {
        let mut f = staged("camp-reports");
        f.settings.scale = scale;
        let entries: Vec<&str> = crate::ui::navigation::camp_entries(&f.ctx())
            .into_iter()
            .map(|entry| entry.label())
            .collect();
        let ctx = egui::Context::default();
        crate::settings::install_font(&ctx);
        crate::settings::configure(&ctx, scale);
        let mut warmup = ctx.run_ui(egui::RawInput::default(), |_| {});
        warmup.textures_delta.clear();
        let mut last = None;
        for _ in 0..2 {
            f.runtime.begin_frame(ctx.cumulative_frame_nr() as u32);
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    ..egui::RawInput::default()
                },
                |ui| crate::ui::draw_screen(ui.ctx(), &mut f.ctx(), 0.0),
            );
            output.textures_delta.clear();
            last = Some(output);
        }
        let output = last.unwrap();
        let painted = runs(&output);
        let tabs: Vec<egui::Rect> = painted
            .iter()
            .filter(|(text, _)| entries.contains(&text.as_str()))
            .map(|(_, rect)| *rect)
            .collect();
        assert_eq!(
            tabs.len(),
            entries.len(),
            "{size:?}: {entries:?} in {painted:?}"
        );
        let row_bottom = tabs
            .iter()
            .map(|rect| rect.bottom())
            .fold(f32::MIN, f32::max);
        let toasts: Vec<&(String, egui::Rect)> = painted
            .iter()
            .filter(|(text, _)| text.contains("expedition outcome") || text.contains(" · "))
            .filter(|(_, rect)| rect.left() > size.x * 0.5)
            .collect();
        assert!(!toasts.is_empty(), "{size:?}: no toast in {painted:?}");
        for (text, rect) in toasts {
            assert!(
                rect.top() >= row_bottom && tabs.iter().all(|tab| !tab.intersects(*rect)),
                "{size:?}: the toast {text:?} {rect:?} lies on the tab row {tabs:?}"
            );
        }
    }
}

/// UX-16's callout row gets a capture on both places it shows: the
/// callout-camp and callout-upgrades stills leave exactly one unlocked
/// callout unread (every other scene marks them all seen), one whose place
/// is elsewhere, so arriving does not mark it and every frame draws the row
/// with "Show me" and "Got it".
#[test]
fn callout_stills_leave_one_callout_unread_on_camp_and_upgrades() {
    use crate::ui::onboarding;
    for (scene, screen) in [
        ("callout-camp", Screen::Camp),
        ("callout-upgrades", Screen::Upgrades),
    ] {
        assert_eq!(
            review_scenes::lookup(scene),
            Some(review_scenes::SceneKind::Still),
            "{scene} is a registered still"
        );
        let mut f = staged(scene);
        assert!(f.screen == screen, "{scene} staged {:?}", f.screen);
        let callout = onboarding::due(&f.settings, &f.game)
            .unwrap_or_else(|| panic!("{scene}: a callout is due"));
        assert_eq!(callout.id, fixtures::production::UNREAD_CALLOUT, "{scene}");
        let unread: Vec<_> = onboarding::CALLOUTS
            .iter()
            .filter(|c| !f.settings.seen_callouts.as_ref().unwrap().contains(c.id))
            .filter(|c| onboarding::unlocked_ids(&f.game).contains(c.id))
            .map(|c| c.id)
            .collect();
        assert_eq!(unread, [callout.id], "{scene}: one unread callout");
        let line = onboarding::text(callout, &f.settings);
        for _ in 0..2 {
            let painted = painted_text(&one_frame(&mut f));
            for text in [line.as_str(), "Show me", "Got it"] {
                assert!(
                    painted.iter().any(|p| p == text),
                    "{scene}: {text:?} in {painted:?}"
                );
            }
        }
    }
    // Every other staged scene still shows its screen without a callout.
    let f = staged("camp-home");
    assert!(crate::ui::onboarding::due(&f.settings, &f.game).is_none());
}

/// CF-3-30: with the status row and the callout band over the compact tree
/// at 1050x700@1.35, the skill detail's head column (name and chip, Learn,
/// the disabled reason) is drawn whole: the reason line lies inside the
/// screen and inside its clip, as the canvas leaves the head its height.
#[test]
fn compact_skill_detail_draws_the_disabled_reason_whole_under_the_callout() {
    let mut f = staged("callout-upgrades");
    f.settings.scale = 1.35;
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1050.0, 700.0) / 1.35);
    let ctx = egui::Context::default();
    crate::settings::install_font(&ctx);
    crate::settings::configure(&ctx, 1.35);
    let mut warmup = ctx.run_ui(egui::RawInput::default(), |_| {});
    warmup.textures_delta.clear();
    let mut last = None;
    for _ in 0..4 {
        f.runtime.begin_frame(ctx.cumulative_frame_nr() as u32);
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(screen),
                ..egui::RawInput::default()
            },
            |ui| crate::ui::draw_screen(ui.ctx(), &mut f.ctx(), 0.0),
        );
        output.textures_delta.clear();
        last = Some(output);
    }
    let output = last.expect("frames ran");
    fn walk(
        shape: &egui::epaint::Shape,
        clip: egui::Rect,
        out: &mut Vec<(String, egui::Rect, egui::Rect)>,
    ) {
        match shape {
            egui::epaint::Shape::Text(t) => out.push((
                t.galley.text().to_owned(),
                t.galley.rect.translate(t.pos.to_vec2()),
                clip,
            )),
            egui::epaint::Shape::Vec(shapes) => shapes.iter().for_each(|s| walk(s, clip, out)),
            _ => {}
        }
    }
    let mut texts = Vec::new();
    for clipped in &output.shapes {
        walk(&clipped.shape, clipped.clip_rect, &mut texts);
    }
    let callout = crate::ui::onboarding::due(&f.settings, &f.game).expect("a callout is due");
    let line = crate::ui::onboarding::text(callout, &f.settings);
    assert!(
        texts.iter().any(|(text, ..)| *text == line),
        "the callout band shows"
    );
    let (text, rect, clip) = texts
        .iter()
        .find(|(text, ..)| text.starts_with("Need ") && text.ends_with(" more coins"))
        .unwrap_or_else(|| panic!("the disabled reason is painted: {texts:?}"));
    assert!(
        screen.contains_rect(*rect),
        "{text:?} at {rect:?} lies inside the screen {screen:?}"
    );
    assert!(
        clip.contains_rect(*rect),
        "{text:?} at {rect:?} lies inside its clip {clip:?}"
    );
}

/// UX-12's low-HP band gets a capture for both defenders it applies to:
/// the low-hp still is a paused personal defense and low-hp-observed a
/// paused main-run defense Mara holds, each with the defender at or below
/// 30% of maximum HP, so both modes draw the band and "· LOW".
#[test]
fn low_hp_stills_draw_the_band_for_the_personal_and_the_observed_defender() {
    use horde_core::content::Mode;
    for (scene, mode, name) in [
        // Every defender's HUD name is its card's capitals (CF-3-28).
        ("low-hp", Mode::Personal, "ROOK"),
        ("low-hp-observed", Mode::Npc, "MARA"),
    ] {
        assert_eq!(
            review_scenes::lookup(scene),
            Some(review_scenes::SceneKind::Still),
            "{scene} is a registered still"
        );
        for reduced in [false, true] {
            let mut f = staged(scene);
            f.settings.reduced_effects = reduced;
            assert!(f.screen == Screen::Battle, "{scene} staged {:?}", f.screen);
            let a = f.game.attempt().expect("a live defense");
            assert!(a.paused, "{scene}: the still is paused");
            assert_eq!(a.mode, mode, "{scene}");
            assert!(
                a.hp > 0 && i64::from(a.hp) * 100 <= 30 * i64::from(a.max_hp),
                "{scene}: HP {} / {} is in the band",
                a.hp,
                a.max_hp
            );
            let (hp, max_hp) = (a.hp, a.max_hp);
            let painted = painted_text(&one_frame(&mut f));
            assert!(f.battle.low_hp, "{scene}, reduced {reduced}: the band");
            let low = format!("{name} · HP {hp} / {max_hp} · LOW");
            assert!(
                painted.contains(&low),
                "{scene}, reduced {reduced}: {low:?} in {painted:?}"
            );
        }
    }
}

/// W2-28's close capture (CF-2-04) needs a spitter winding up at its reach:
/// the spitter-windup still is a rank-6 or later field, paused with a
/// catalogue spitter stationed four yards beyond the defense line halfway
/// through its windup, so the view draws its aim line at the point its spit
/// lands (the wall centre or the defender) in normal and reduced effects.
#[test]
fn spitter_windup_fixture_pauses_a_spitter_mid_windup_at_its_reach() {
    use horde_core::sim::{DEFENSE_LINE, EnemyPhase};
    assert_eq!(
        review_scenes::lookup("spitter-windup"),
        Some(review_scenes::SceneKind::Still),
        "spitter-windup is a registered still"
    );
    let f = staged("spitter-windup");
    assert!(f.screen == Screen::Battle, "staged {:?}", f.screen);
    let a = f.game.attempt().expect("a live battle");
    assert!(a.paused, "the still is paused mid-windup");
    assert!(
        a.progression.rank >= 6,
        "rank {} fields no spitters",
        a.progression.rank
    );
    let spitter = a
        .enemies
        .iter()
        .find(|e| {
            e.resolved
                .as_ref()
                .is_some_and(|r| r.archetype_id == "spitter")
                && e.attack_phase == EnemyPhase::Windup
        })
        .expect("a catalogue spitter winds up");
    let reach = spitter.resolved.as_ref().map(|r| r.reach).unwrap_or(0.0);
    assert_eq!(reach, 4.0, "the shipped spitter's reach");
    assert!(
        (spitter.x - (DEFENSE_LINE + reach)).abs() < 0.01,
        "the spitter stands at its reach, x {}",
        spitter.x
    );
    let target = a.spit_target(spitter).expect("the windup is a spit");
    assert!(
        spitter.attack_left > 0 && spitter.attack_left < spitter.windup_total,
        "mid-windup: {} of {} ticks left",
        spitter.attack_left,
        spitter.windup_total
    );
    assert!(
        spitter.attack_left.abs_diff(spitter.windup_total / 2) <= 1,
        "halfway: {} of {} ticks left",
        spitter.attack_left,
        spitter.windup_total
    );
    for reduced in [false, true] {
        let view = crate::battle_view::BattleViewModel::build(
            a,
            crate::animation::SceneAnimation::sample(a, reduced),
            crate::battle_view::SceneBlend::NONE,
            reduced,
        );
        let drawn = view
            .enemies
            .iter()
            .find(|e| e.id == spitter.id)
            .unwrap_or_else(|| panic!("reduced {reduced}: the spitter is drawn"));
        assert!(
            drawn.windup.is_some(),
            "reduced {reduced}: its windup shows"
        );
        assert_eq!(
            drawn.aim_target,
            Some(target),
            "reduced {reduced}: the aim line runs to where the spit lands"
        );
    }
}

/// Every painted text run containing `label` (newlines read as spaces), with
/// its painted bounds.
fn text_bounds(output: &egui::FullOutput, label: &str) -> Vec<egui::Rect> {
    fn walk(shape: &egui::epaint::Shape, label: &str, found: &mut Vec<egui::Rect>) {
        match shape {
            egui::epaint::Shape::Text(text)
                if text.galley.text().replace('\n', " ").contains(label) =>
            {
                found.push(text.galley.rect.translate(text.pos.to_vec2()));
            }
            egui::epaint::Shape::Vec(shapes) => {
                shapes.iter().for_each(|shape| walk(shape, label, found));
            }
            _ => {}
        }
    }
    let mut found = Vec::new();
    for clipped in &output.shapes {
        walk(&clipped.shape, label, &mut found);
    }
    found
}

/// W2-41's close capture item: no still opened Journey upgrades on the
/// Firearms column at x = -1780. skills-firearms focuses Pistol double tap
/// (-1780, 0): the tree selects it and paints its title inside the canvas at
/// 1440x900@1.0 and at the compact 1050x700@1.35.
#[test]
fn skills_firearms_fixture_opens_on_pistol_double_tap_at_both_sizes() {
    assert_eq!(
        review_scenes::lookup("skills-firearms"),
        Some(review_scenes::SceneKind::Still),
        "skills-firearms is a registered still"
    );
    let node = horde_core::skill_catalog::skills()
        .node("pistol_double_tap")
        .expect("pistol_double_tap");
    assert_eq!(node.branch, "Firearms");
    assert_eq!(node.position, [-1780.0, 0.0]);
    for (size, scale) in [
        (egui::vec2(1440.0, 900.0), 1.0),
        (egui::vec2(1050.0, 700.0) / 1.35, 1.35),
    ] {
        let mut f = staged("skills-firearms");
        assert!(f.screen == Screen::Upgrades, "staged {:?}", f.screen);
        assert_eq!(f.nav.tab, Tab::Skills);
        f.settings.scale = scale;
        let ctx = egui::Context::default();
        crate::settings::install_font(&ctx);
        crate::settings::configure(&ctx, scale);
        let mut warmup = ctx.run_ui(egui::RawInput::default(), |_| {});
        warmup.textures_delta.clear();
        let mut output = None;
        for _ in 0..2 {
            f.runtime.begin_frame(ctx.cumulative_frame_nr() as u32);
            let mut frame = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    ..egui::RawInput::default()
                },
                |ui| crate::ui::draw_screen(ui.ctx(), &mut f.ctx(), 0.0),
            );
            frame.textures_delta.clear();
            output = Some(frame);
        }
        let output = output.expect("two frames");
        assert_eq!(
            crate::ui::progression::selected_node_for_test(&ctx).as_deref(),
            Some("pistol_double_tap"),
            "scale {scale}: the focus selects Pistol double tap"
        );
        let layout = crate::ui::progression::layout_for_test(&ctx).expect("a tree layout");
        assert_eq!(layout.compact, scale > 1.0, "scale {scale}");
        let titles = text_bounds(&output, &node.name);
        assert!(
            titles.iter().any(|rect| layout.canvas.contains_rect(*rect)),
            "scale {scale}: {:?} painted inside the canvas {:?}: {titles:?}",
            node.name,
            layout.canvas
        );
    }
}

/// Two frames of the staged Upgrades tree at `size` (logical points) and UI
/// `scale`, with the context that drew them.
fn staged_tree_frames(
    scene: &str,
    size: egui::Vec2,
    scale: f32,
) -> (egui::Context, egui::FullOutput) {
    let mut f = staged(scene);
    assert!(
        f.screen == Screen::Upgrades,
        "{scene}: staged {:?}",
        f.screen
    );
    f.settings.scale = scale;
    let ctx = egui::Context::default();
    crate::settings::install_font(&ctx);
    crate::settings::configure(&ctx, scale);
    let mut warmup = ctx.run_ui(egui::RawInput::default(), |_| {});
    warmup.textures_delta.clear();
    let mut output = None;
    for _ in 0..2 {
        f.runtime.begin_frame(ctx.cumulative_frame_nr() as u32);
        let mut frame = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                ..egui::RawInput::default()
            },
            |ui| crate::ui::draw_screen(ui.ctx(), &mut f.ctx(), 0.0),
        );
        frame.textures_delta.clear();
        output = Some(frame);
    }
    (ctx, output.expect("two frames"))
}

/// W2-41's close capture item, which also closes design-progression-tree §3
/// item 8 ("milestone-gated nodes do not collide in position: validator plus
/// capture"): the Firearms column at x = -1780 must be evidenced at its 0,
/// 190 and 380 rows (Double tap, Pump drill, Breath control) with both ends
/// of the rifle_training -> rifle_breath_control route, at 1440x900@1.0 and
/// at 1050x700@1.35. skills-firearms frames the 0 row at both sizes (the test
/// above) but, centred on Double tap, paints Breath control below the canvas
/// even at 1440x900 (title at y 898.7 under the canvas bottom 886). No node
/// focus fits the 190 and 380 rows in the 195-pt compact canvas: centred on
/// Pump drill, Breath control's title lands at y 411 under the bottom 384.6
/// and Rifle proficiency's ends at x 767.8 past the right edge 763.8. So
/// skills-firearms-column selects Pump drill and centres the point between
/// the three nodes, (-1500, 285).
#[test]
fn skills_firearms_column_frames_the_1780_rows_and_the_breath_control_route_at_both_sizes() {
    const SCENE: &str = "skills-firearms-column";
    let catalog = horde_core::skill_catalog::skills();
    let ids = [
        "shotgun_pump_drill",
        "rifle_breath_control",
        "rifle_training",
    ];
    let names: Vec<&str> = ids
        .iter()
        .map(|id| catalog.node(id).expect("catalogue node").name.as_str())
        .collect();
    for (id, y) in [
        ("pistol_double_tap", 0.0),
        ("shotgun_pump_drill", 190.0),
        ("rifle_breath_control", 380.0),
    ] {
        assert_eq!(catalog.node(id).expect(id).position, [-1780.0, y], "{id}");
    }
    assert!(
        catalog
            .node("rifle_breath_control")
            .expect("rifle_breath_control")
            .prerequisites
            .iter()
            .any(|p| p == "rifle_training"),
        "Breath control is reached from Rifle proficiency"
    );
    assert_eq!(
        review_scenes::lookup(SCENE),
        Some(review_scenes::SceneKind::Still),
        "{SCENE} is a registered still"
    );
    for (size, scale) in [
        (egui::vec2(1440.0, 900.0), 1.0),
        (egui::vec2(1050.0, 700.0) / 1.35, 1.35),
    ] {
        let (ctx, output) = staged_tree_frames(SCENE, size, scale);
        assert_eq!(
            crate::ui::progression::selected_node_for_test(&ctx).as_deref(),
            Some("shotgun_pump_drill"),
            "scale {scale}: the focus selects Pump drill"
        );
        let layout = crate::ui::progression::layout_for_test(&ctx).expect("a tree layout");
        assert_eq!(layout.compact, scale > 1.0, "scale {scale}");
        for name in &names {
            let titles = text_bounds(&output, name);
            assert!(
                titles.iter().any(|rect| layout.canvas.contains_rect(*rect)),
                "scale {scale}: {name:?} painted inside the canvas {:?}: {titles:?}",
                layout.canvas
            );
        }
    }
}

// ----- Wave-3 UI scenes (design-qa-captures 1.8 and "Wave 3", W3-27) -----

/// Every painted text run of the frame.
fn painted_text(output: &egui::FullOutput) -> Vec<String> {
    fn walk(shape: &egui::epaint::Shape, out: &mut Vec<String>) {
        match shape {
            egui::epaint::Shape::Text(text) => out.push(text.galley.text().to_owned()),
            egui::epaint::Shape::Vec(shapes) => shapes.iter().for_each(|s| walk(s, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    for clipped in &output.shapes {
        walk(&clipped.shape, &mut out);
    }
    out
}

/// The labels of the last frame's primaries on `surface`, in draw order.
fn primaries_on(ctx: &egui::Context, surface: crate::ui::Surface) -> Vec<String> {
    crate::ui::frame_primaries(ctx)
        .into_iter()
        .filter(|record| record.surface == surface)
        .map(|record| record.label)
        .collect()
}

/// The wave-3 scenes are registered stills with their owners and "before"
/// scenes (design-qa-captures 1.8).
#[test]
fn wave3_scenes_are_registered_stills() {
    for (name, owner, proxy) in [
        ("upgrades", "ux-flow", "journey-upgrades"),
        ("camp-home", "ux-flow", "active-camp"),
        ("briefing-recruit", "campaign-defenders", "briefing-npc"),
        ("equipment-autosell", "loot-economy", "quality-armory"),
        (
            "results-liberation-recruit",
            "campaign-defenders",
            "journey-liberated",
        ),
        ("takeover-battle", "campaign-defenders", "npc"),
    ] {
        let scene = review_scenes::REVIEW_SCENES
            .iter()
            .find(|scene| scene.name == name)
            .unwrap_or_else(|| panic!("{name} is not registered"));
        assert_eq!(scene.kind, review_scenes::SceneKind::Still, "{name}");
        assert_eq!(
            (scene.owner, scene.before_proxy, scene.since_wave),
            (owner, Some(proxy), 3),
            "{name}"
        );
    }
}

/// The upgrades scene opens the upgrades by their one route
/// (`UiCtx::open_upgrades`) after a real settled first round, focused on the
/// recommended upgrade, and shows exactly one amber primary: "Back to
/// battle" (loop map, plan 3.2), with "Back to camp" beside it.
#[test]
fn upgrades_fixture_has_one_amber_primary_back_to_battle() {
    let mut f = staged("upgrades");
    assert!(f.screen == Screen::Upgrades, "staged {:?}", f.screen);
    assert!(f.game.attempt().is_none(), "the round settled");
    assert_eq!(f.game.campaign().run_history.len(), 1, "one real round");
    let report = f.game.last_result().expect("a settled report");
    assert_eq!(
        (report.country_id.as_deref(), report.stage),
        (Some("DEU"), 0)
    );
    assert!(
        f.campaign.improvement_focus.is_some(),
        "the tree opens on the recommended upgrade"
    );
    assert!(f.game.global_paused());
    let (ctx, output) = one_frame_with_context(&mut f);
    assert_eq!(
        primaries_on(&ctx, crate::ui::Surface::Screen),
        ["Back to battle"],
        "one amber primary"
    );
    assert_eq!(
        crate::review_scenes::REVIEW_SCENES
            .iter()
            .find(|scene| scene.name == "upgrades")
            .and_then(|scene| scene.primary),
        Some("Back to battle"),
        "the registered primary is the painted one"
    );
    assert_eq!(
        crate::ui::tests::text_count(&output, "Back to camp"),
        1,
        "the way back to the camp"
    );
}

/// Camp home lists what the scene stages: Mara and two recruits (a guard and
/// one on a live expedition), an outbreak alert, the next front, and the
/// one amber "Go to briefing".
#[test]
fn camp_home_fixture_lists_roster_and_go_to_briefing() {
    use horde_core::{
        defenders::DefenderId,
        skill_catalog::CampaignFeature,
        strategy::{OutbreakOutcome, RecruitStatus},
    };
    let mut f = staged("camp-home");
    assert!(f.screen == Screen::Camp, "staged {:?}", f.screen);
    assert_eq!(f.nav.tab, Tab::Home);
    assert!(f.game.attempt().is_none(), "between defenses");
    assert!(f.game.global_paused());
    assert!(f.game.campaign().has_feature(CampaignFeature::Companion));
    let statuses: Vec<RecruitStatus> = f.game.strategy().roster.iter().map(|r| r.status).collect();
    assert_eq!(
        statuses,
        [RecruitStatus::Guarding, RecruitStatus::Deployed],
        "a guard and an expedition recruit"
    );
    let live = f
        .game
        .battles()
        .iter()
        .filter(|b| !b.settled && b.game().attempt().is_some())
        .count();
    assert_eq!(live, 1, "one live expedition");
    let outbreaks: Vec<&str> = f
        .game
        .strategy()
        .outbreaks
        .iter()
        .filter(|o| o.outcome == OutbreakOutcome::Active)
        .map(|o| o.country_id.as_str())
        .collect();
    assert_eq!(outbreaks, ["FRA"]);
    assert_eq!(
        f.game.campaign().world.active.as_deref(),
        Some("POL"),
        "the next front is chosen"
    );
    assert_eq!(f.game.next_defender(), DefenderId::Player);

    let names: Vec<String> = f
        .game
        .available_defenders()
        .into_iter()
        .map(|option| option.card.name)
        .collect();
    assert_eq!(names.len(), 4, "you, Mara and the two recruits: {names:?}");
    let (ctx, output) = one_frame_with_context(&mut f);
    let text = painted_text(&output);
    for name in &names {
        assert!(
            text.iter().any(|run| run == name),
            "the roster lists {name}: {text:?}"
        );
    }
    let next = format!("Poland · Defense 1 · {}", names[0]);
    assert_eq!(crate::ui::tests::text_count(&output, &next), 1, "{text:?}");
    assert!(
        text.iter()
            .any(|run| run.starts_with("France · ") && run.ends_with("s left")),
        "the outbreak alert names France: {text:?}"
    );
    assert_eq!(
        primaries_on(&ctx, crate::ui::Surface::Screen),
        ["Go to briefing"]
    );
}

/// The briefing of the next front with the staged recruit chosen through
/// `SetNextDefender`: the one amber primary deploys the recruit by name.
#[test]
fn briefing_recruit_fixture_selects_next_defender() {
    use horde_core::defenders::DefenderId;
    let mut f = staged("briefing-recruit");
    assert!(f.screen == Screen::Camp, "staged {:?}", f.screen);
    assert_eq!(f.nav.tab, Tab::Briefing);
    assert!(f.game.attempt().is_none(), "chosen between defenses");
    let DefenderId::Recruit(id) = f.game.next_defender() else {
        panic!("the next defender is {:?}", f.game.next_defender());
    };
    assert_eq!(
        f.game.campaign().next_defender,
        Some(DefenderId::Recruit(id))
    );
    assert_eq!(f.game.campaign().world.active.as_deref(), Some("POL"));
    assert_eq!(f.campaign.stage, 0, "the front's first defense");
    let card = f
        .game
        .defender_card(DefenderId::Recruit(id))
        .expect("the recruit's card");
    let deploy = format!("Deploy {}", card.name);
    let scene = review_scenes::REVIEW_SCENES
        .iter()
        .find(|scene| scene.name == "briefing-recruit")
        .expect("briefing-recruit is registered");
    assert_eq!(scene.primary, Some(deploy.as_str()), "the registry primary");
    let (ctx, output) = one_frame_with_context(&mut f);
    assert_eq!(primaries_on(&ctx, crate::ui::Surface::Screen), [deploy]);
    let line = format!("Poland · Defense 1 · {}", card.name);
    assert_eq!(
        crate::ui::tests::text_count(&output, &line),
        1,
        "{:?}",
        painted_text(&output)
    );
}

/// The Equipment page opens with the Auto-sell panel revealed on the staged
/// rule (sell below Uncommon), and its preview counts only the Poor and the
/// Common drop of the six, with core's coins.
#[test]
fn equipment_autosell_preview_counts_only_below_uncommon() {
    use horde_core::{quality::Quality, sim::AutoSellScope};
    let mut f = staged("equipment-autosell");
    assert!(f.screen == Screen::Camp, "staged {:?}", f.screen);
    assert_eq!(f.nav.tab, Tab::Armory);
    let rule = f.game.campaign().auto_sell;
    assert!(rule.enabled);
    assert_eq!(rule.sell_below, Quality::Uncommon);
    let preview = f.game.auto_sell_preview(&rule, AutoSellScope::Stash);
    assert_eq!(preview.count, 2, "Poor and Common");
    let line = format!("Would sell 2 now · +{}", preview.coins);
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1440.0, 900.0));
    // The first frame opens the panel and asks the page to scroll to it;
    // from the second frame on it is on screen. The reveal holds it there
    // until the player's first input (CF-3-25).
    let ctx = egui::Context::default();
    crate::settings::install_font(&ctx);
    let mut warmup = ctx.run_ui(egui::RawInput::default(), |_| {});
    warmup.textures_delta.clear();
    for frame in 0..30 {
        f.runtime.begin_frame(ctx.cumulative_frame_nr() as u32);
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(screen),
                ..egui::RawInput::default()
            },
            |ui| crate::ui::draw_screen(ui.ctx(), &mut f.ctx(), 0.0),
        );
        output.textures_delta.clear();
        assert!(f.armory.reveal_auto_sell, "no input yet: the reveal holds");
        if frame == 0 {
            continue;
        }
        assert_eq!(
            crate::ui::tests::text_count(&output, &line),
            1,
            "frame {frame}: the open panel shows {line:?}: {:?}",
            painted_text(&output)
        );
        let bounds = text_bounds(&output, &line);
        assert!(
            bounds.iter().all(|rect| screen.contains_rect(*rect)),
            "frame {frame}: the preview is on screen: {bounds:?}"
        );
    }
}

/// CF-3-25 (closing): the reveal keeps the whole opened panel in view at the
/// compact size too. A scroll taken on the first frame only was measured
/// against a page that then changed under it (the first frames of a launch
/// draw before the 1.35 scale applies; the footer and the item art settle),
/// and left "Apply rule to stash now…" cut at the viewport's bottom. Checked
/// at 1050x700@1.35 (and 1440x900) in both modes, with the first frames on
/// the unscaled window: the Apply button's text lies inside its scroll clip,
/// and the player's first press ends the reveal.
#[test]
fn equipment_autosell_reveal_shows_the_apply_button_at_both_sizes() {
    fn texts(output: &egui::FullOutput, label: &str) -> Vec<(egui::Rect, egui::Rect)> {
        fn walk(
            shape: &egui::epaint::Shape,
            clip: egui::Rect,
            label: &str,
            found: &mut Vec<(egui::Rect, egui::Rect)>,
        ) {
            match shape {
                egui::epaint::Shape::Text(text) if text.galley.text().contains(label) => {
                    found.push((text.galley.rect.translate(text.pos.to_vec2()), clip));
                }
                egui::epaint::Shape::Vec(shapes) => {
                    shapes
                        .iter()
                        .for_each(|shape| walk(shape, clip, label, found));
                }
                _ => {}
            }
        }
        let mut found = Vec::new();
        for clipped in &output.shapes {
            walk(&clipped.shape, clipped.clip_rect, label, &mut found);
        }
        found
    }
    for reduced in [false, true] {
        for (size, scale) in [
            (egui::vec2(1050.0, 700.0) / 1.35, 1.35),
            (egui::vec2(1440.0, 900.0), 1.0),
        ] {
            let mut f = staged("equipment-autosell");
            f.settings.scale = scale;
            f.settings.reduced_effects = reduced;
            let ctx = egui::Context::default();
            crate::settings::install_font(&ctx);
            crate::settings::configure(&ctx, scale);
            let mut warmup = ctx.run_ui(egui::RawInput::default(), |_| {});
            warmup.textures_delta.clear();
            let mut last = None;
            // The first frames draw on the unscaled window, as a launch does
            // before its scale applies; then well past the section's open
            // animation at 60 frames a second.
            let frame = |f: &mut FrontendParts, size: egui::Vec2, events| {
                f.runtime.begin_frame(ctx.cumulative_frame_nr() as u32);
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                        events,
                        ..egui::RawInput::default()
                    },
                    |ui| crate::ui::draw_screen(ui.ctx(), &mut f.ctx(), 0.0),
                );
                output.textures_delta.clear();
                output
            };
            for step in 0..30 {
                let shown = if step < 3 { size * scale } else { size };
                last = Some(frame(&mut f, shown, Vec::new()));
            }
            let output = last.unwrap();
            let apply = texts(&output, "Apply rule to stash now");
            assert_eq!(
                apply.len(),
                1,
                "{size:?} reduced {reduced}: {:?}",
                painted_text(&output)
            );
            let (rect, clip) = apply[0];
            assert!(
                clip.contains_rect(rect),
                "{size:?} reduced {reduced}: Apply {rect:?} is cut by its clip {clip:?}"
            );
            assert!(
                f.armory.reveal_auto_sell,
                "{size:?} reduced {reduced}: no input yet, the reveal holds"
            );
            // The player's first press (on the empty corner under the
            // footer's primary) hands the page back.
            let corner = size.to_pos2() - egui::vec2(4.0, 4.0);
            let press = |pressed| egui::Event::PointerButton {
                pos: corner,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            frame(
                &mut f,
                size,
                vec![egui::Event::PointerMoved(corner), press(true), press(false)],
            );
            assert!(
                !f.armory.reveal_auto_sell,
                "{size:?} reduced {reduced}: a press ends the reveal"
            );
        }
    }
}

/// Results after the victory that liberated Germany, with the Recruitment
/// node owned and the auto-sell rule stored before the win: the liberation
/// block offers the recruit at core's price, the loot block lists what the
/// settlement auto-sold, and the amber primary is "Choose next country".
#[test]
fn results_liberation_recruit_offers_recruit_from_germany() {
    use horde_core::{quality::Quality, skill_catalog::CampaignFeature};
    let mut f = staged("results-liberation-recruit");
    assert!(f.screen == Screen::Results, "staged {:?}", f.screen);
    assert!(f.game.attempt().is_none(), "the final stage settled");
    let report = f.game.last_result().expect("a settled report").clone();
    assert_eq!(report.outcome, horde_core::sim::Outcome::Victory);
    assert!(report.country_freed, "Germany is liberated");
    assert_eq!(report.country_id.as_deref(), Some("DEU"));
    assert!(f.game.campaign().has_feature(CampaignFeature::Recruitment));
    assert!(f.game.strategy().roster.is_empty(), "nobody recruited yet");
    let offer = f
        .game
        .recruit_offer("DEU")
        .expect("Germany offers a recruit");
    assert!(
        f.game.campaign().coins >= offer.price,
        "the offer is affordable"
    );
    // Decision 6: strictly below Uncommon, so the Poor and the Common drop.
    assert_eq!(report.auto_sold.count, 2, "{:?}", report.auto_sold);
    for quality in Quality::ALL {
        assert_eq!(
            report.auto_sold.by_quality[quality],
            u32::from(quality < Quality::Uncommon),
            "{quality:?}"
        );
    }

    let (ctx, output) = one_frame_with_context(&mut f);
    let recruit = format!("Recruit from Germany · {} coins", offer.price);
    assert_eq!(
        crate::ui::tests::text_count(&output, &recruit),
        1,
        "{:?}",
        painted_text(&output)
    );
    let auto_sold = format!(
        "Auto-sold {} · +{}",
        report.auto_sold.count, report.auto_sold.coins
    );
    assert_eq!(
        crate::ui::tests::text_count(&output, &auto_sold),
        1,
        "{:?}",
        painted_text(&output)
    );
    assert_eq!(
        primaries_on(&ctx, crate::ui::Surface::Screen),
        ["Choose next country"]
    );
}

/// The takeover battle is the recruit's: the main attempt's defender is the
/// recruit chosen through `SetNextDefender`, the HUD names the recruit, and
/// the field draws the attempt's stamped appearance (the recruit's own, not
/// the Female rig `Mode::Npc` meant before W3-17).
#[test]
fn takeover_battle_draws_the_recruit_not_mara() {
    use horde_core::{content::Mode, defenders::DefenderId, survivors::Appearance};
    let mut f = staged("takeover-battle");
    assert!(f.screen == Screen::Battle, "staged {:?}", f.screen);
    let a = f.game.attempt().expect("a live takeover");
    let DefenderId::Recruit(_) = a.defender() else {
        panic!("the defender is {:?}", a.defender());
    };
    assert_eq!(a.mode, Mode::Npc);
    assert!(a.paused && a.tick == 0, "paused before the first tick");
    assert_eq!(a.country_id.as_deref(), Some("POL"));
    let card = f.game.active_defender().expect("the defending recruit");
    assert_eq!(card.id, a.defender());
    assert_ne!(card.hud_name, "MARA");
    assert_eq!(
        card.appearance,
        Appearance::Male,
        "the staged recruit is not the Female default"
    );
    assert_eq!(a.defender_appearance(), card.appearance);
    let hp = format!("{} · HP {} / {}", card.hud_name, a.hp.max(0), a.max_hp);

    let all_meshes = |output: &egui::FullOutput| -> Vec<egui::Mesh> {
        let mut clips: Vec<egui::Rect> = Vec::new();
        for clipped in &output.shapes {
            if !clips.contains(&clipped.clip_rect) {
                clips.push(clipped.clip_rect);
            }
        }
        clips
            .into_iter()
            .flat_map(|clip| managed_meshes(output, clip))
            .collect()
    };
    let output = one_frame(&mut f);
    assert_eq!(
        crate::ui::tests::text_count(&output, &hp),
        1,
        "the HUD names the recruit: {:?}",
        painted_text(&output)
    );
    assert!(
        !painted_text(&output).iter().any(|run| run.contains("MARA")),
        "Mara is not on this field"
    );
    let own = all_meshes(&output);
    assert!(!own.is_empty(), "the rig draws from its source sheets");
    // Stamping the other appearance changes the drawn rig: the field reads
    // the attempt's stamp, not the mode.
    f.game.attempt_mut().unwrap().profile.appearance = Appearance::Female;
    let output = one_frame(&mut f);
    // `assert!`, not `assert_ne!`: a failure must not dump the meshes.
    assert!(
        all_meshes(&output) != own,
        "the field draws the attempt's stamped appearance"
    );
}

/// The failed manual save opens the storage modal (W3-06): the modal holds
/// the save's intent and the OS error, and its one primary is "Retry".
#[test]
fn save_error_fixture_opens_retry_modal() {
    use crate::runner::SaveIntent;
    use crate::ui::modal::{AppModal, StorageError};
    let mut f = staged("save-error");
    assert_eq!(
        f.status.modal,
        Some(AppModal::Storage(StorageError::Save {
            intent: SaveIntent::Manual,
            error: "Access is denied. (os error 5)".into(),
        }))
    );
    let (ctx, output) = frames_with_context(&mut f, 2);
    assert_eq!(primaries_on(&ctx, crate::ui::Surface::Modal), ["Retry"]);
    assert_eq!(crate::ui::tests::text_count(&output, "Retry"), 1);
    assert!(
        painted_text(&output)
            .iter()
            .any(|run| run.contains("Access is denied. (os error 5)")),
        "the modal shows the OS error"
    );
    assert!(!f.paths.campaign().exists(), "no save was attempted");
    // The capture sidecar names the modal over the camp (design-qa-captures
    // 1.8), and drops it once the modal closes.
    #[cfg(feature = "review")]
    {
        let facts = |f: &FrontendParts| {
            ReviewFrameFacts::collect(
                &f.game,
                &f.launch,
                &f.settings,
                Some(&f.status),
                f.screen,
                f.nav.tab,
                &f.battle,
            )
            .overlays
        };
        assert_eq!(facts(&f), ["save-error-modal"]);
        f.status.modal = None;
        assert!(facts(&f).is_empty());
    }
}

// ----- W5-09: the wave-5 graphics review scenes -----

/// Design-vfx-lighting §12 (night gate): dawn-reference is the dawn stage of
/// the night-checkpoint scene's own country, on the same field geometry,
/// paused at tick 0 with no hostile, so the two captures differ by the stage
/// theme alone.
#[test]
fn dawn_reference_pairs_the_night_checkpoint_field() {
    use crate::art::lighting::Lighting;
    let night = staged("night-checkpoint");
    let dawn = staged("dawn-reference");
    assert!(dawn.screen == Screen::Battle, "{:?}", dawn.screen);
    let (n, d) = (
        night.game.attempt().expect("the night stage"),
        dawn.game.attempt().expect("a deployed dawn stage"),
    );
    assert!(d.paused && dawn.game.global_paused(), "deployed paused");
    assert_eq!((d.tick, d.enemies.len()), (0, 0), "tick 0, no hostile");
    assert_eq!((n.tick, n.enemies.len()), (0, 0));
    assert_eq!(d.country_id, n.country_id, "the same country and biome");
    assert_ne!(d.stage, n.stage);
    assert_eq!(
        (d.lane_count, d.width_yards),
        (n.lane_count, n.width_yards),
        "the same lanes and width"
    );
    let (night_look, dawn_look) = (crate::art::scene_look(n), crate::art::scene_look(d));
    assert!(dawn_look.lighting == Lighting::Dawn, "a dawn stage");
    assert!(night_look.lighting == Lighting::Night, "a night stage");
    assert_eq!(dawn_look.theme_index, night_look.theme_index);
    assert!(!dawn_look.storm && !night_look.storm, "no lightning");
}

/// Design-vfx-lighting §14: elite-tells holds a live elite of every shipped
/// affix on the visible field, paused, and the view gives each its tell.
#[test]
fn elite_tells_fixture_fields_every_shipped_affix() {
    let f = staged("elite-tells");
    assert!(f.screen == Screen::Battle, "{:?}", f.screen);
    let a = f.game.attempt().expect("a live defense");
    assert!(a.paused && f.game.global_paused(), "paused");
    let catalog = horde_core::enemy_catalog::enemy_catalog();
    let affixes: Vec<&str> = catalog.elite_affix_ids().collect();
    assert!(!affixes.is_empty(), "the catalogue ships affixes");
    for affix in &affixes {
        let elite = a.enemies.iter().find(|e| {
            e.resolved
                .as_ref()
                .and_then(|profile| profile.elite.as_ref())
                .is_some_and(|mark| mark.affix_id == *affix)
        });
        let elite = elite.unwrap_or_else(|| panic!("no {affix} elite on the field"));
        assert!(
            !elite.boss && elite.x < a.width_yards,
            "{affix}: a hostile inside the field (x {})",
            elite.x
        );
    }
    let view = fx_view(&f, false);
    let tells = view.enemies.iter().filter(|e| e.elite.is_some()).count();
    assert!(tells >= affixes.len(), "{tells} tells");
    assert!(
        view.enemies.iter().any(|e| e.elite.is_none()),
        "a plain hostile stays for comparison"
    );
}

/// CF-3-27: every elite of the elite-tells still stands apart from every
/// other, so each ring and marker reads as its own hostile's: two lanes
/// apart, or `ELITE_TELLS_SPREAD_YARDS` apart along the field. Under that
/// spread on its 7-lane, 16-yard field two rings (0.88 lane heights, about a
/// yard wide) overlapped, and a marker one lane above its ring sat in the
/// ring of a hostile in the lane above. No elite stands in the top lane,
/// whose marker floats above the field (under the PAUSED tag at 1050x700).
#[test]
fn elite_tells_fixture_spreads_its_elites() {
    let f = staged("elite-tells");
    let a = f.game.attempt().expect("a live defense");
    let elites: Vec<_> = a
        .enemies
        .iter()
        .filter(|e| {
            e.resolved
                .as_ref()
                .is_some_and(|profile| profile.elite.is_some())
        })
        .collect();
    assert!(elites.len() >= 2, "{} elites", elites.len());
    for elite in &elites {
        assert!(
            fixtures::production::elite_tells_marker_in_field(elite),
            "elite {} in lane {} draws its marker above the field",
            elite.id,
            elite.lane
        );
    }
    for (i, one) in elites.iter().enumerate() {
        for other in &elites[i + 1..] {
            assert!(
                fixtures::production::elite_tells_apart(one, other),
                "elites {} (lane {}, x {:.2}) and {} (lane {}, x {:.2}) stand together",
                one.id,
                one.lane,
                one.x,
                other.id,
                other.lane,
                other.x
            );
        }
    }
    let spread = fixtures::production::ELITE_TELLS_SPREAD_YARDS;
    assert!((1.5..=2.0).contains(&spread), "{spread}");
}

fn fx_view(f: &FrontendParts, reduced: bool) -> crate::battle_view::BattleViewModel {
    let a = f.game.attempt().expect("a live field");
    crate::battle_view::BattleViewModel::build(
        a,
        crate::animation::SceneAnimation::sample(a, reduced),
        crate::battle_view::SceneBlend::NONE,
        reduced,
    )
}

#[cfg(feature = "review")]
fn fx_stress_counts(f: &FrontendParts, reduced: bool) -> crate::art::FxCounts {
    let arena = egui::Rect::from_min_size(egui::pos2(24.0, 124.0), egui::vec2(1392.0, 692.0));
    crate::art::fx_counts(
        arena,
        &fx_view(f, reduced),
        f.game.attempt().unwrap(),
        f.settings.blood,
        reduced,
    )
}

/// Design-vfx-lighting §14: fx-stress is the stress field (17 lanes, night, a
/// boss, the machine-gun Npc and the turrets) carrying the effect load: four
/// burning zones, a grenade and a Molotov burst, blood decals on and at least
/// 30% elites. The `stress` perf scene carries none of the extra load.
#[cfg(feature = "review")]
#[test]
fn fx_stress_fixture_carries_the_effect_load() {
    use crate::art::lighting::Lighting;
    use horde_core::{content::StructureKind, ordnance::OrdnanceKind, weapons::WeaponClass};
    let f = staged("fx-stress");
    let a = f.game.attempt().expect("the stress field");
    assert!(a.paused);
    assert_eq!(a.lane_count, 17);
    assert!(
        crate::art::scene_look(a).lighting == Lighting::Night,
        "night"
    );
    assert!(a.enemies.iter().any(|e| e.boss), "a boss");
    assert!(
        a.structures.iter().any(|s| s.kind == StructureKind::Turret),
        "a turret"
    );
    assert!(
        a.gear_snapshot
            .iter()
            .any(|record| record.weapon_class == Some(WeaponClass::MachineGun)),
        "the machine gun"
    );
    assert!(f.settings.blood, "blood decals on");
    assert_eq!(a.ordnance.fire_zones.len(), 4, "four burning zones");
    let view = fx_view(&f, false);
    let blasts: Vec<_> = view.fx.iter().filter_map(|e| e.ordnance).collect();
    assert_eq!(blasts.len(), 2, "two bursts: {blasts:?}");
    assert!(blasts.contains(&OrdnanceKind::Grenade) && blasts.contains(&OrdnanceKind::Molotov));
    // W5-06: a felled boss's kill burst (x1.8) beside the live boss, on the
    // staged tick, so the newest impacts the spark cap keeps include it.
    let boss_kills: Vec<_> = view
        .fx
        .iter()
        .filter(|e| e.kind == horde_core::sim::EventKind::Kill && e.boss)
        .collect();
    assert_eq!(boss_kills.len(), 1, "one boss kill burst");
    assert_eq!(boss_kills[0].age, 0.0, "the burst is fresh");
    assert!(
        a.enemies
            .iter()
            .any(|e| e.boss && Some(e.id) != boss_kills[0].actor_id),
        "the live boss is another body"
    );
    let hostiles = a.enemies.iter().filter(|e| !e.boss).count();
    let elites = a
        .enemies
        .iter()
        .filter(|e| !e.boss && e.resolved.as_ref().is_some_and(|p| p.elite.is_some()))
        .count();
    assert!(
        elites * 10 >= hostiles * 3,
        "{elites} of {hostiles} hostiles are elite"
    );
    // The perf scene keeps the bare fixture.
    let stress = staged("stress");
    let s = stress.game.attempt().unwrap();
    assert_eq!(s.ordnance.fire_zones.len(), 3);
    assert!(s.events.is_empty() && !stress.settings.blood);
}

/// Acceptance G7.5 (design-vfx-lighting §13): every budgeted effect class of
/// the fx-stress frame stays within `perf_budget::FX_BUDGET`, paused and over
/// 96 live ticks, with normal and reduced effects; the paused still reaches
/// the caps the load is built to reach.
#[cfg(feature = "review")]
#[test]
fn fx_stress_shape_counts_within_perf_budget() {
    use horde_core::{command::Command, sim::Input};
    let budget = |name: &str| {
        crate::perf_budget::FX_BUDGET
            .iter()
            .find(|(row, _)| *row == name)
            .unwrap_or_else(|| panic!("no fx budget row {name}"))
            .1
    };
    for reduced in [false, true] {
        let mut f = staged("fx-stress");
        let still = fx_stress_counts(&f, reduced);
        assert_eq!(still.fire_zones, 4, "reduced {reduced}");
        assert_eq!(still.explosions, 2, "reduced {reduced}");
        assert_eq!(
            still.elite_tells,
            budget("elite_tells"),
            "the tell cap is reached"
        );
        assert_eq!(
            still.blood_decals,
            budget("decals"),
            "the decal cap is reached"
        );
        assert_eq!(still.lights == 0, reduced, "pools only with normal effects");
        f.game.apply(&Command::SetPaused(false)).expect("unpause");
        for tick in 0..=96 {
            let counts = fx_stress_counts(&f, reduced);
            for (name, drawn) in counts.rows() {
                assert!(
                    drawn <= budget(name),
                    "tick {tick} reduced {reduced}: {name} {drawn} > {}",
                    budget(name)
                );
            }
            f.game.tick(Input::default());
        }
    }
}

/// The additive colours of one frame's shapes: mesh vertices and the fills
/// and strokes of the plain shapes (egui's additive colours have alpha 0).
#[cfg(feature = "review")]
fn additive_colors(shape: &egui::Shape) -> usize {
    use egui::Shape;
    let additive = |c: egui::Color32| usize::from(c.a() == 0 && c != egui::Color32::TRANSPARENT);
    match shape {
        Shape::Vec(shapes) => shapes.iter().map(additive_colors).sum(),
        Shape::Mesh(mesh) => mesh.vertices.iter().map(|v| additive(v.color)).sum(),
        Shape::Circle(c) => additive(c.fill) + additive(c.stroke.color),
        Shape::Ellipse(e) => additive(e.fill) + additive(e.stroke.color),
        Shape::Rect(r) => additive(r.fill) + additive(r.stroke.color),
        Shape::LineSegment { stroke, .. } => additive(stroke.color),
        _ => 0,
    }
}

/// What one live fx-stress frame draws of the effects reduced effects
/// leave out.
#[cfg(feature = "review")]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct FxStressFrame {
    /// The frame's additive colours, read off the painted shapes: the light
    /// pools (`light::pools_mesh`) and the loot glint are the field's only
    /// additive paint.
    additive: usize,
    /// The frame's muzzle flash polygons and sky-flash alpha
    /// (`art::flash_facts`, the painter's own rules).
    flashes: crate::art::FlashFacts,
    /// The burst keyframe parts drawn (core, shock ring, fireball, bloom,
    /// tongues and the rest; `art::burst_keyframe_parts`), beyond the
    /// reduced static ring.
    burst_keyframes: usize,
}

/// Each of 96 live fx-stress frames, drawn as the App draws them (one tick
/// a frame).
#[cfg(feature = "review")]
fn fx_stress_sequence(reduced: bool) -> Vec<FxStressFrame> {
    use horde_core::{command::Command, sim::Input};
    let mut f = staged("fx-stress");
    f.settings.reduced_effects = reduced;
    f.game.apply(&Command::SetPaused(false)).expect("unpause");
    let arena = egui::Rect::from_min_size(egui::pos2(24.0, 124.0), egui::vec2(1392.0, 692.0));
    (0..96)
        .map(|_| {
            f.game.tick(Input::default());
            let output = one_frame(&mut f);
            let reduced = f.settings.reduced_effects;
            let a = f.game.attempt().expect("the stress field stands");
            FxStressFrame {
                additive: output
                    .shapes
                    .iter()
                    .map(|clipped| additive_colors(&clipped.shape))
                    .sum(),
                flashes: crate::art::flash_facts(a, f.battle.sky_clock(), reduced),
                burst_keyframes: crate::art::burst_keyframe_parts(
                    arena,
                    &fx_view(&f, reduced),
                    a,
                    reduced,
                ),
            }
        })
        .collect()
}

/// Acceptance G7.4 (reduced MOTION gate, the headless half): 96 live frames
/// of fx-stress in reduced effects paint no additive vertex (no light pool
/// or loot glint), no muzzle flash polygon and no burst keyframe part (the
/// bursts are the static solid or dashed ring). The same 96 frames with
/// normal effects light every frame, draw muzzle flashes and play burst
/// keyframes, which proves each detector sees what it rules out.
/// fx-stress is a night stage, so its sky-flash alpha is zero in both
/// modes; the reduced storm export's zero sky flash is
/// `motion_exports_flash_in_normal_mode_and_never_in_reduced`. The pixel half
/// (frame-to-frame luminance) is `scripts/measure_field_luminance.py motion`
/// on a capture_motion export, with `--sidecar` the capture_review sidecar
/// `<mode>/<scene>[-small].json` of the same scene, viewport and mode (the
/// export's manifest.json has no `arena_rect`; docs/VALIDATION.md, "Full
/// verify").
#[cfg(feature = "review")]
#[test]
fn reduced_96_frame_fx_stress_sequence_has_no_additive_vertex() {
    let normal = fx_stress_sequence(false);
    assert_eq!(normal.len(), 96);
    assert!(
        normal.iter().all(|frame| frame.additive > 0),
        "normal effects light every frame: {normal:?}"
    );
    assert!(
        normal
            .iter()
            .any(|frame| frame.flashes.muzzle_flash_count > 0),
        "normal effects draw muzzle flashes: {normal:?}"
    );
    assert!(
        normal.iter().any(|frame| frame.burst_keyframes > 0),
        "normal effects play burst keyframes: {normal:?}"
    );
    let reduced = fx_stress_sequence(true);
    let lit: Vec<_> = reduced
        .iter()
        .enumerate()
        .filter(|(_, frame)| **frame != FxStressFrame::default())
        .collect();
    assert!(
        lit.is_empty(),
        "reduced effects paint no additive vertex, muzzle flash, sky flash or burst keyframe: {lit:?}"
    );
}

// ----- W5-10: the storm scene and the motion manifest's flash counters -----

/// Design-qa-captures 1.8 (ruling W-8): the storm scene is a real storm
/// stage, deployed and played calm up to a tick from which the motion
/// export's 24 frames hold one whole lightning pulse; the still is paused
/// there (a paused field never flashes).
#[cfg(feature = "review")]
#[test]
fn storm_fixture_is_a_storm_tagged_stage() {
    use crate::art::lighting::{self, Lighting};
    use crate::capture_sequence::{FIRST, LAST};
    let f = staged("storm");
    assert!(f.screen == Screen::Battle, "{:?}", f.screen);
    let a = f.game.attempt().expect("a deployed storm stage");
    assert!(a.paused && f.game.global_paused(), "paused");
    let country = horde_core::world::world()
        .country(a.country_id.as_deref().expect("a country stage"))
        .expect("a world country");
    assert!(
        lighting::is_storm(&country.stage_theme(a.stage)),
        "a storm-tagged stage theme"
    );
    let look = crate::art::scene_look(a);
    assert!(look.storm && look.lighting == Lighting::Storm, "{look:?}");
    // The whole pulse (rise and decay) lies inside the export's captured
    // ticks: a tick is 1000/60 ms of the export's lightning clock.
    let ms = |tick: u64| tick * 1000 / 60;
    let window = ms(a.tick + u64::from(FIRST))..ms(a.tick + u64::from(LAST));
    let start = lighting::first_pulse_after(look.seed, window.start);
    assert!(
        window.contains(&start)
            && window.contains(&(start + lighting::SKY_RISE_MS + lighting::SKY_DECAY_MS)),
        "pulse at {start} ms, window {window:?}"
    );
    // The pulse starts on frame 8 of the export's 24.
    assert_eq!(
        (start * 60).div_ceil(1000),
        a.tick + u64::from(FIRST + 8 * crate::capture_sequence::STEP)
    );
    let reduced = crate::art::flash_facts(a, crate::art::SkyClock::of_tick(a.tick, 1), true);
    assert_eq!(reduced, crate::art::FlashFacts::default(), "reduced");
}

/// The flash facts of the 24 frames a motion export of `scene` captures,
/// stepped as the export steps them (a live fixture from startup, one tick a
/// native frame, the lightning clock on the tick).
#[cfg(feature = "review")]
fn motion_flash_facts(scene: &str, reduced: bool) -> Vec<crate::art::FlashFacts> {
    motion_flash_frames(scene, reduced)
        .into_iter()
        .map(|(facts, _)| facts)
        .collect()
}

/// [`motion_flash_facts`], each frame with the sky-flash alpha the lightning
/// schedule gives the frame's own tick: `lighting::sky_flash` of the stage's
/// presentation seed at the tick's 60 Hz wall time, with the attempt's quiet
/// windows, through `lighting::sky_alpha`.
#[cfg(feature = "review")]
fn motion_flash_frames(scene: &str, reduced: bool) -> Vec<(crate::art::FlashFacts, f32)> {
    use crate::art::lighting;
    use crate::capture_sequence::{LAST, frame_index};
    let mut vars = vec![
        ("LASTLIGHT_REVIEW_SCREEN", scene),
        ("LASTLIGHT_REVIEW_MOTION", "1"),
        ("LASTLIGHT_CAPTURE_SEQUENCE", "frames"),
    ];
    if reduced {
        vars.push(("LASTLIGHT_REVIEW_REDUCED", "1"));
    }
    let config = LaunchConfig::parse(std::iter::empty::<std::ffi::OsString>(), |name| {
        vars.iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| (*value).into())
    })
    .expect("valid export launch");
    let mut f = FrontendParts::load(&config, crate::profile::SettingsLoad::default());
    assert_eq!(f.settings.reduced_effects, reduced, "{scene}");
    let mut frames = Vec::new();
    for frame in 1..=LAST {
        f.flush();
        f.step();
        if frame_index(frame).is_some() {
            let a = f.game.attempt().expect("the defense stands");
            assert!(!a.paused, "{scene}: the export runs live");
            let look = crate::art::scene_look(a);
            let scheduled = lighting::sky_alpha(lighting::sky_flash(
                look.seed,
                crate::art::SkyClock::of_tick(a.tick, 1).wall_ms as u64,
                lighting::sky_quiet(a, f.battle.speed),
                a.paused,
                f.settings.reduced_effects,
                look.storm,
            ));
            frames.push((
                crate::art::flash_facts(
                    a,
                    crate::art::SkyClock::of_tick(a.tick, f.battle.speed),
                    f.settings.reduced_effects,
                ),
                scheduled,
            ));
        }
        schedule_motion_release(&config, &mut f.runtime, frame);
        crate::ui::schedule_motion_trigger(&config, &mut f.battle, frame);
        // The battle screen's egui pass publishes the controls it holds.
        one_frame(&mut f);
    }
    frames
}

/// Design-qa-captures 1.9 (W5): a normal storm export shows the pulse (at
/// least one frame with sky_flash > 0, lit in two neighbouring frames so it
/// rises and decays rather than popping, and every frame's sky_flash the
/// schedule's alpha at that frame's own tick, so the facts the manifest
/// writes follow the stage's seed, the tick clock and the quiet windows), a
/// normal machine-gun export captures muzzle flashes (its trigger is pulled
/// on `review::motion_trigger_frame`, so frames land on a flash one tick
/// old) and lights every frame, and the reduced exports of storm, fx-stress
/// and weapon-classes-machine-gun are all zeros. No
/// frame can exceed `SKY_PEAK_ALPHA` (sky_flash is `lighting::sky_alpha` of
/// a clamped pulse), so the peak is not asserted.
/// sky_flash is the over-rect alpha, not a luminance: the storm's ΔL < 0.2
/// (G7.3) is `sky_flash_peak_delta_below_0_2_under_gamma_and_linear_blending` and
/// measure_field_luminance.py storm on the exported PNGs.
#[cfg(feature = "review")]
#[test]
fn motion_exports_flash_in_normal_mode_and_never_in_reduced() {
    use crate::art::FlashFacts;
    let frames = motion_flash_frames("storm", false);
    for (i, (facts, scheduled)) in frames.iter().enumerate() {
        assert!(
            (facts.sky_flash - scheduled).abs() <= f32::EPSILON,
            "frame {i}: sky_flash {} is not the schedule's {scheduled}",
            facts.sky_flash
        );
    }
    let storm: Vec<FlashFacts> = frames.into_iter().map(|(facts, _)| facts).collect();
    assert_eq!(storm.len(), 24);
    assert!(
        storm.iter().any(|facts| facts.sky_flash > 0.0),
        "the pulse is captured: {storm:?}"
    );
    assert!(
        storm
            .windows(2)
            .any(|pair| pair[0].sky_flash > 0.0 && pair[1].sky_flash > 0.0),
        "the pulse is lit in two neighbouring frames: {storm:?}"
    );
    let gun = motion_flash_facts("weapon-classes-machine-gun", false);
    assert!(
        gun.iter().any(|f| f.muzzle_flash_count > 0),
        "a captured frame lands on a muzzle flash: {gun:?}"
    );
    assert!(
        gun.iter().all(|f| f.flash_alpha_max > 0.0),
        "the machine gun lights every frame: {gun:?}"
    );
    assert!(gun.iter().all(|f| f.sky_flash == 0.0), "no storm in DEU 0");
    let still = staged("weapon-classes-machine-gun");
    let a = still.game.attempt().expect("a live battle");
    let clock = crate::art::SkyClock::of_tick(a.tick, 1);
    let shot = crate::art::flash_facts(a, clock, false);
    assert_eq!(shot.muzzle_flash_count, 1, "{shot:?}");
    assert!(
        shot.flash_alpha_max >= 0.9 && shot.sky_flash == 0.0,
        "{shot:?}"
    );
    assert_eq!(
        crate::art::flash_facts(a, clock, true),
        FlashFacts::default()
    );
    for scene in ["storm", "fx-stress", "weapon-classes-machine-gun"] {
        assert_eq!(
            motion_flash_facts(scene, true),
            [FlashFacts::default(); 24],
            "{scene}: reduced effects never flash"
        );
    }
}

/// CF-3-23: a weapon-class still draws its missed shot's one MISS whatever
/// the capture waited. Outside damage and damage-4x the popup clock of a
/// still was the wall clock, so the volley's MISS (CF-1-12) faded or expired
/// before the capture frame: it showed in 1 of 4 shotgun stills and in no
/// rifle still. Staged as the capture stages them, with the capture frame
/// 2 s after the first (past a popup's 0.9 s life), weapon-classes-shotgun
/// and weapon-classes-rifle each paint exactly one MISS at full strength, at
/// 1440x900@1.0 and 1050x700@1.35, at one spot in normal and reduced effects
/// (each label's centre inside the other: normal draws the fresh label at
/// its punch scale).
#[test]
fn weapon_class_stills_draw_one_miss_at_one_spot_in_both_modes() {
    fn misses(output: &egui::FullOutput) -> Vec<(egui::Rect, u8)> {
        fn walk(shape: &egui::epaint::Shape, found: &mut Vec<(egui::Rect, u8)>) {
            match shape {
                egui::epaint::Shape::Text(text) if text.galley.text() == "MISS" => {
                    let alpha = text
                        .galley
                        .job
                        .sections
                        .first()
                        .map_or(0, |section| section.format.color.a());
                    found.push((text.galley.rect.translate(text.pos.to_vec2()), alpha));
                }
                egui::epaint::Shape::Vec(shapes) => shapes.iter().for_each(|s| walk(s, found)),
                _ => {}
            }
        }
        let mut found = Vec::new();
        for clipped in &output.shapes {
            walk(&clipped.shape, &mut found);
        }
        found
    }
    for scene in ["weapon-classes-shotgun", "weapon-classes-rifle"] {
        for (size, scale) in [
            (egui::vec2(1440.0, 900.0), 1.0),
            (egui::vec2(1050.0, 700.0) / 1.35, 1.35),
        ] {
            let mut spots = Vec::new();
            for reduced in [false, true] {
                let mut f = staged(scene);
                assert!(f.screen == Screen::Battle, "{scene}: staged {:?}", f.screen);
                assert_eq!(f.launch.damage_capture_mode(), LootCaptureMode::Still);
                // An interactive review of the scene keeps the live clock.
                let interactive = LaunchConfig {
                    scenario: Some(scene.into()),
                    review: Some(ReviewOptions::default()),
                    ..LaunchConfig::default()
                };
                assert_eq!(interactive.damage_capture_mode(), LootCaptureMode::Disabled);
                f.settings.scale = scale;
                f.settings.reduced_effects = reduced;
                let ctx = egui::Context::default();
                crate::settings::install_font(&ctx);
                crate::settings::configure(&ctx, scale);
                let mut warmup = ctx.run_ui(egui::RawInput::default(), |_| {});
                warmup.textures_delta.clear();
                let mut last = None;
                for time in [0.0, 0.5, 2.0] {
                    f.runtime.begin_frame(ctx.cumulative_frame_nr() as u32);
                    let mut output = ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                            time: Some(time),
                            ..egui::RawInput::default()
                        },
                        |ui| crate::ui::draw_screen(ui.ctx(), &mut f.ctx(), 0.0),
                    );
                    output.textures_delta.clear();
                    last = Some(output);
                }
                let found = misses(&last.expect("a capture frame"));
                let label = format!("{scene} scale {scale} reduced {reduced}");
                // A popup paints its outline as offset copies of its text
                // (up to 2 pt off), then the text: one label is one cluster.
                let mut labels: Vec<Vec<(egui::Rect, u8)>> = Vec::new();
                for run in found {
                    match labels
                        .iter_mut()
                        .find(|label| (label[0].0.center() - run.0.center()).length() < 6.0)
                    {
                        Some(label) => label.push(run),
                        None => labels.push(vec![run]),
                    }
                }
                assert_eq!(labels.len(), 1, "{label}: one MISS, found {labels:?}");
                let alpha = labels[0].iter().map(|run| run.1).max().unwrap_or(0);
                assert!(alpha > 200, "{label}: the MISS at full strength ({alpha})");
                let (rect, _) = *labels[0].last().expect("the text over its outline");
                spots.push(rect);
            }
            // One spot: normal effects draws the fresh label at its punch
            // scale and reduced at 1.0, both clamped to the same field edge,
            // so each label's centre lies inside the other.
            assert!(
                spots[0].contains(spots[1].center()) && spots[1].contains(spots[0].center()),
                "{scene} scale {scale}: normal {:?} and reduced {:?} differ",
                spots[0],
                spots[1]
            );
        }
    }
}

/// Every text run of `output` with its screen rect and alpha, nested shape
/// lists included.
fn text_runs(output: &egui::FullOutput) -> Vec<(String, egui::Rect, u8)> {
    fn walk(shape: &egui::epaint::Shape, found: &mut Vec<(String, egui::Rect, u8)>) {
        match shape {
            egui::epaint::Shape::Text(text) => {
                let alpha = text
                    .override_text_color
                    .or_else(|| {
                        text.galley
                            .job
                            .sections
                            .first()
                            .map(|section| section.format.color)
                    })
                    .map_or(0, |color| color.a());
                found.push((
                    text.galley.text().to_owned(),
                    text.galley.rect.translate(text.pos.to_vec2()),
                    alpha,
                ));
            }
            egui::epaint::Shape::Vec(shapes) => shapes.iter().for_each(|s| walk(s, found)),
            _ => {}
        }
    }
    let mut found = Vec::new();
    for clipped in &output.shapes {
        walk(&clipped.shape, &mut found);
    }
    found
}

/// The outlined pop-ups among `runs`: a pop-up paints eight outline copies
/// of its text (each at most 2 pt off) and then its face, so it is a cluster
/// of nine or more runs of one text. Each is its text, the union of its runs
/// and its brightest alpha.
fn outlined_popups(runs: &[(String, egui::Rect, u8)]) -> Vec<(String, egui::Rect, u8)> {
    let mut clusters: Vec<(String, egui::Rect, u8, usize)> = Vec::new();
    for (text, rect, alpha) in runs {
        match clusters.iter_mut().find(|(other, union, ..)| {
            other == text && (union.center() - rect.center()).length() < 6.0
        }) {
            Some(cluster) => {
                cluster.1 = cluster.1.union(*rect);
                cluster.2 = cluster.2.max(*alpha);
                cluster.3 += 1;
            }
            None => clusters.push((text.clone(), *rect, *alpha, 1)),
        }
    }
    clusters
        .into_iter()
        .filter(|cluster| cluster.3 >= 9)
        .map(|(text, rect, alpha, _)| (text, rect, alpha))
        .collect()
}

/// The frames a still capture of `scene` draws at `size` and `scale`, with
/// effects `reduced`, on egui input clocks `times`: the last frame's output.
fn staged_battle_still(
    scene: &str,
    size: egui::Vec2,
    scale: f32,
    reduced: bool,
    times: &[f64],
) -> egui::FullOutput {
    let mut f = staged(scene);
    assert!(f.screen == Screen::Battle, "{scene}: staged {:?}", f.screen);
    f.settings.scale = scale;
    f.settings.reduced_effects = reduced;
    let ctx = egui::Context::default();
    crate::settings::install_font(&ctx);
    crate::settings::configure(&ctx, scale);
    let mut warmup = ctx.run_ui(egui::RawInput::default(), |_| {});
    warmup.textures_delta.clear();
    let mut last = None;
    for &time in times {
        f.runtime.begin_frame(ctx.cumulative_frame_nr() as u32);
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                time: Some(time),
                ..egui::RawInput::default()
            },
            |ui| crate::ui::draw_screen(ui.ctx(), &mut f.ctx(), 0.0),
        );
        output.textures_delta.clear();
        last = Some(output);
    }
    last.expect("a capture frame")
}

/// Whether `text` is a damage pop-up's text: a number, a weak-point "N!",
/// an incoming "-N" or one of the outcome words.
fn damage_popup_text(text: &str) -> bool {
    let digits = text.trim_start_matches('-').trim_end_matches('!');
    (!digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()))
        || matches!(text, "MISS" | "ARMOR" | "DODGE" | "DEFENDED")
}

/// CF-3-06: a Battle still draws its damage pop-ups whatever the capture
/// waited and whatever the effects mode. Outside damage and damage-4x (and,
/// since CF-3-23, the weapon classes) the popup clock of a still was the
/// wall clock, so a still caught whatever was mid-fade on its capture frame:
/// normal battle-female showed a faded "29" that reduced lacked, reduced
/// progressed-battle a "37" at 30% strength that normal lacked. Staged as
/// the capture stages them, captured once 0.5 s and once 3.5 s after the
/// first frame (past a pop-up's 0.9 s life), each scene paints the same
/// pop-up texts at full strength in normal and reduced effects at both
/// sizes, and some scene paints any.
#[test]
fn battle_stills_draw_the_same_popups_at_full_strength_in_both_modes() {
    let mut drawn = 0;
    for scene in [
        "battle-female",
        "progressed-battle",
        "ordnance",
        "elite-tells",
    ] {
        for (size, scale) in [
            (egui::vec2(1440.0, 900.0), 1.0),
            (egui::vec2(1050.0, 700.0) / 1.35, 1.35),
        ] {
            let mut sets = Vec::new();
            for reduced in [false, true] {
                for times in [[0.0, 0.2, 0.5], [0.0, 1.2, 3.5]] {
                    let label = format!("{scene} scale {scale} reduced {reduced} at {times:?}");
                    let output = staged_battle_still(scene, size, scale, reduced, &times);
                    let mut texts: Vec<String> = outlined_popups(&text_runs(&output))
                        .into_iter()
                        .filter(|(text, ..)| damage_popup_text(text))
                        .map(|(text, _, alpha)| {
                            assert!(alpha > 200, "{label}: {text:?} at alpha {alpha}");
                            text
                        })
                        .collect();
                    texts.sort();
                    sets.push((label, texts));
                }
            }
            for (label, texts) in &sets[1..] {
                assert_eq!(
                    texts, &sets[0].1,
                    "{label} draws other pop-ups than {}",
                    sets[0].0
                );
            }
            drawn += sets[0].1.len();
        }
    }
    assert!(drawn > 0, "no Battle still drew a damage pop-up");
}

/// Three headless frames of the staged `scene` at `size` points and `scale` on
/// a fresh context, every frame given `events`, run through the review
/// capture's pointer filter when `drop` is set; returns the last frame.
fn pointer_frames(
    scene: &str,
    size: egui::Vec2,
    scale: f32,
    events: &[egui::Event],
    drop: bool,
) -> egui::FullOutput {
    let mut f = staged(scene);
    assert!(f.launch.review_capture(), "{scene}: staged as a capture");
    f.settings.scale = scale;
    let ctx = egui::Context::default();
    crate::settings::install_font(&ctx);
    crate::settings::configure(&ctx, scale);
    let mut warmup = ctx.run_ui(egui::RawInput::default(), |_| {});
    warmup.textures_delta.clear();
    let mut output = None;
    for _ in 0..3 {
        f.runtime.begin_frame(ctx.cumulative_frame_nr() as u32);
        let mut input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
            events: events.to_vec(),
            ..egui::RawInput::default()
        };
        if drop {
            drop_os_pointer(&mut input);
        }
        let mut frame = ctx.run_ui(input, |ui| {
            crate::ui::draw_screen(ui.ctx(), &mut f.ctx(), 0.0)
        });
        frame.textures_delta.clear();
        output = Some(frame);
    }
    output.expect("three frames")
}

/// CF-3-18: a review capture's frames never see the OS pointer, so a still
/// paints the same wherever the mouse sat during the run. The closing
/// reduced skills-firearms-column-small still drew the toolbar's floating
/// scroll bar expanded in amber (pixels 38-978 x 440-466 at 1050x700@1.35)
/// because the mouse rested over it; the normal still did not. With the
/// capture's filter, a pointer resting or wheeling anywhere over the scene
/// (that toolbar spot included) paints exactly the frame with no pointer, at
/// both sizes. The perturbation half: unfiltered, the same pointer does
/// change the paint, so the comparison has teeth.
#[test]
fn review_capture_paints_the_same_whatever_the_os_pointer() {
    const SCENE: &str = "skills-firearms-column";
    let wheel = egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::vec2(-40.0, -40.0),
        phase: egui::TouchPhase::Move,
        modifiers: egui::Modifiers::NONE,
    };
    for (size, scale) in [
        (egui::vec2(1440.0, 900.0), 1.0),
        (egui::vec2(1050.0, 700.0) / 1.35, 1.35),
    ] {
        let still = pointer_frames(SCENE, size, scale, &[], false).shapes;
        // The toolbar scroll bar of the closing still, in points.
        let mut points = vec![egui::pos2(500.0, 453.0) / scale];
        for column in 0..6 {
            for row in 0..4 {
                points.push(egui::pos2(
                    size.x * (column as f32 + 0.5) / 6.0,
                    size.y * (row as f32 + 0.5) / 4.0,
                ));
            }
        }
        let mut perturbed = 0;
        for point in points {
            for events in [
                vec![egui::Event::PointerMoved(point)],
                vec![egui::Event::PointerMoved(point), wheel.clone()],
            ] {
                let filtered = pointer_frames(SCENE, size, scale, &events, true).shapes;
                assert!(
                    filtered == still,
                    "scale {scale}: a capture with the pointer at {point:?} ({} events) \
                     painted another frame than with no pointer",
                    events.len()
                );
                if pointer_frames(SCENE, size, scale, &events, false).shapes != still {
                    perturbed += 1;
                }
            }
        }
        assert!(
            perturbed > 0,
            "scale {scale}: the unfiltered pointer never changed the paint"
        );
    }
}

/// CF-3-18: the filter drops every pointer event, reports the pointer gone
/// and keeps the keyboard and text input.
#[test]
fn drop_os_pointer_keeps_only_non_pointer_events() {
    let key = egui::Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    };
    let text = egui::Event::Text("a".into());
    let mut input = egui::RawInput {
        events: vec![
            egui::Event::PointerMoved(egui::pos2(10.0, 10.0)),
            key.clone(),
            egui::Event::MouseMoved(egui::vec2(1.0, 1.0)),
            egui::Event::PointerButton {
                pos: egui::pos2(10.0, 10.0),
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Line,
                delta: egui::vec2(0.0, 1.0),
                phase: egui::TouchPhase::Move,
                modifiers: egui::Modifiers::NONE,
            },
            egui::Event::Zoom(1.1),
            egui::Event::Rotate(0.1),
            egui::Event::Touch {
                device_id: egui::TouchDeviceId(0),
                id: egui::TouchId(0),
                phase: egui::TouchPhase::Start,
                pos: egui::pos2(10.0, 10.0),
                force: None,
            },
            text.clone(),
        ],
        ..egui::RawInput::default()
    };
    drop_os_pointer(&mut input);
    assert_eq!(input.events, vec![key, text, egui::Event::PointerGone]);
}

/// A6-18 (closing): an isolated review scene is muted, except a perf run that
/// asks for its audio (`LASTLIGHT_PERF_AUDIO` with `LASTLIGHT_PERF_REPORT`),
/// so a 60 s live battle can write a cue log with real starts for
/// `replay_cue_log`. The flag alone, without a report, still mutes.
#[cfg(feature = "review")]
#[test]
fn a_perf_audio_review_keeps_the_battle_audible() {
    let default = crate::settings::Settings::default().master;
    assert!(default > 0.0);
    for scene in ["npc", "battle", "peak"] {
        let screen = ("LASTLIGHT_REVIEW_SCREEN", scene);
        let report = ("LASTLIGHT_PERF_REPORT", "perf.json");
        let audio = ("LASTLIGHT_PERF_AUDIO", "1");
        for (vars, master) in [
            (&[screen][..], 0.0),
            (&[screen, report][..], 0.0),
            (&[screen, audio][..], 0.0),
            (&[screen, report, audio][..], default),
        ] {
            assert_eq!(launched(vars).settings.master, master, "{scene} {vars:?}");
        }
    }
}
