//! Egui harness tests: onboarding callouts and the country dossier (W3-24;
//! UX-16, ruling S4). A callout shows once, under the camp row; the seen-set
//! lives in settings.json; a pre-pass file is seeded at profile load; a New
//! game starts it empty; "Restore defaults" keeps it.
use super::home_tests::settle;
use super::*;
use crate::{
    launch::LaunchConfig,
    profile::{self, LoadedProfile},
    settings::SETTINGS_VERSION,
};
use horde_core::skill_catalog::{CampaignFeature, SkillEffect, skills};
use std::collections::BTreeSet;

const MARA: &str = "New: Mara - find her in Camp home";
const EXPEDITIONS: &str = "New: Expeditions - Camp home › Operations";
const TRAP: &str = "New: Trap blueprint - press B in battle to build it";
const RESTORE: &str = "Restore display, audio & control defaults";

/// Every painted text run that is a callout line.
fn callout_lines(output: &FullOutput) -> Vec<String> {
    fn collect(shape: &egui::epaint::Shape, lines: &mut Vec<String>) {
        match shape {
            egui::epaint::Shape::Text(text) if text.galley.text().starts_with("New: ") => {
                lines.push(text.galley.text().to_owned());
            }
            egui::epaint::Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, lines)),
            _ => {}
        }
    }
    let mut lines = Vec::new();
    for clipped in &output.shapes {
        collect(&clipped.shape, &mut lines);
    }
    lines
}

/// Buys nothing: adds the node that unlocks `feature` to the campaign.
fn unlock(f: &mut FrontendParts, feature: CampaignFeature) {
    let node = skills()
        .nodes
        .iter()
        .find(|node| node.effects.contains(&SkillEffect::FeatureUnlock(feature)))
        .expect("every feature has a node");
    f.game.campaign_mut().skill_nodes.insert(node.id.clone());
    assert!(f.game.campaign().has_feature(feature));
}

/// A journey in Germany on Camp home with nothing unlocked and the seen-set
/// seeded as a profile load seeds it (empty: nothing is unlocked yet).
fn fresh_camp(fixture: &mut IsolatedFrontend) -> &mut FrontendParts {
    let f = &mut fixture.frontend;
    f.game.campaign_mut().legacy_features = false;
    f.game.campaign_mut().choose_start_country("DEU").unwrap();
    f.game.campaign_mut().journey_active = true;
    f.nav.tab = Tab::Home;
    f.screen = Screen::Camp;
    onboarding::seed_seen_callouts(&mut f.settings, &f.game);
    assert_eq!(f.settings.seen_callouts, Some(BTreeSet::new()));
    f
}

fn seen(f: &FrontendParts) -> Vec<&str> {
    f.settings
        .seen_callouts
        .as_ref()
        .expect("seeded")
        .iter()
        .map(String::as_str)
        .collect()
}

#[test]
fn unlock_callout_shows_once_and_survives_restart() {
    for target in Target::BOTH {
        let mut fixture = IsolatedFrontend::new();
        let h = Harness::at(target);
        let f = fresh_camp(&mut fixture);
        let saveable = f.game.clone();
        f.nav.tab = Tab::World;
        assert_eq!(callout_lines(&settle(&h, f)), Vec::<String>::new());
        let before = h.screen_primaries();

        // Mara's node: her callout shows, once, with both buttons, and adds
        // no primary.
        unlock(f, CampaignFeature::Companion);
        let output = settle(&h, f);
        assert_eq!(callout_lines(&output), [MARA], "{target:?}");
        assert_eq!(text_count(&output, "Show me"), 1);
        assert_eq!(text_count(&output, "Got it"), 1);
        assert!(fully_visible(&output, MARA), "{target:?}");
        assert!(fully_visible(&output, "Got it"), "{target:?}");
        assert_eq!(h.screen_primaries(), before, "a callout is not a primary");

        // Upgrades shows it too (W3-03: the camp row moved there with the
        // tree; Results has no row and no callout).
        f.screen = Screen::Upgrades;
        assert_eq!(callout_lines(&settle(&h, f)), [MARA], "{target:?}");
        f.screen = Screen::Camp;

        // Hints off: hidden, not marked.
        f.settings.show_guidance = false;
        assert_eq!(callout_lines(&settle(&h, f)), Vec::<String>::new());
        assert!(seen(f).is_empty());
        f.settings.show_guidance = true;
        assert_eq!(callout_lines(&settle(&h, f)), [MARA]);

        // "Got it" marks it; it never shows again.
        h.click(f, View::Full, "Got it");
        assert_eq!(seen(f), ["feature:companion"]);
        assert_eq!(callout_lines(&settle(&h, f)), Vec::<String>::new());

        // The next unlock shows next. The battlefield cannot be opened from
        // camp, so it has no "Show me".
        f.game.campaign_mut().blueprints[StructureKind::Trap] = true;
        let output = settle(&h, f);
        assert_eq!(callout_lines(&output), [TRAP]);
        assert_eq!(text_count(&output, "Show me"), 0);

        // Restart: the seen-set is read back from settings.json, so Mara's
        // callout stays away and the unseen one is still due.
        assert_eq!(seen(f), ["feature:companion"], "shown, not marked");
        // `unlock` and the blueprint flag skip the purchase rules, so core
        // would refuse to save this journey. The settings write is what is
        // under test: save the journey as it was before the staging.
        let staged = std::mem::replace(&mut f.game, saveable);
        f.act(|c| c.request_save(SaveIntent::Manual));
        f.flush_for_test();
        assert!(!f.status.message_error, "{}", f.status.message);
        f.game = staged;
        let read = profile::read_settings(&f.paths.settings());
        assert!(read.warning.is_none(), "{:?}", read.warning);
        let restarted = LoadedProfile {
            game: f.game.clone(),
            settings: read.settings,
            warning: String::new(),
            load_error: None,
            settings_warning: None,
            settings_at_risk: false,
        };
        let paths = f.paths.clone();
        let mut again = FrontendParts::new(restarted, &LaunchConfig::default(), &paths);
        assert_eq!(seen(&again), ["feature:companion"]);
        again.screen = Screen::Camp;
        again.nav.tab = Tab::World;
        assert_eq!(callout_lines(&settle(&h, &mut again)), [TRAP]);
        again.runtime.finish_storage_for_test();
    }
}

#[test]
fn show_me_opens_the_place_and_arriving_marks_seen() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = fresh_camp(&mut fixture);
    unlock(f, CampaignFeature::Companion);
    unlock(f, CampaignFeature::Parallel);
    f.game.campaign_mut().blueprints[StructureKind::Trap] = true;
    f.nav.tab = Tab::World;
    assert_eq!(callout_lines(&settle(&h, f)), [MARA]);

    // "Show me" opens Mara's page and marks her callout.
    h.click(f, View::Full, "Show me");
    assert!(f.screen == Screen::Camp && f.nav.tab == Tab::Survivor);
    assert_eq!(seen(f), ["feature:companion"]);
    assert_eq!(callout_lines(&settle(&h, f)), [EXPEDITIONS]);

    // Arriving at Operations by any route marks its callout without a click,
    // hints on or off.
    f.settings.show_guidance = false;
    f.act(|c| c.open_preparation(Tab::Operations));
    settle(&h, f);
    f.settings.show_guidance = true;
    assert_eq!(seen(f), ["feature:companion", "feature:parallel"]);

    // A battle never shows a callout; being in one marks the battle ones.
    f.act(|c| c.open_preparation(Tab::World));
    assert_eq!(callout_lines(&settle(&h, f)), [TRAP]);
    f.game.deploy_country_stage(0, Mode::Personal).unwrap();
    f.screen = Screen::Battle;
    assert_eq!(callout_lines(&settle(&h, f)), Vec::<String>::new());
    assert_eq!(
        seen(f),
        ["blueprint:trap", "feature:companion", "feature:parallel"]
    );
}

#[test]
fn none_is_seeded_with_every_unlocked_callout_at_profile_load() {
    // The table: unique ids, and one "feature:<id>" callout per feature.
    let ids: BTreeSet<&str> = onboarding::CALLOUTS.iter().map(|c| c.id).collect();
    assert_eq!(ids.len(), onboarding::CALLOUTS.len(), "callout ids repeat");
    for feature in CampaignFeature::ALL {
        let id = format!("feature:{}", feature.id());
        assert!(ids.contains(id.as_str()), "{id}");
    }

    // A pre-pass profile: a journey with every feature and the Trap
    // blueprint, and a settings file without the key.
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    fixture.unlock_trap();
    let mut game = fixture.frontend.game.clone();
    game.campaign_mut().choose_start_country("DEU").unwrap();
    game.campaign_mut().journey_active = true;
    assert!(game.campaign().legacy_features && game.campaign().attempts > 0);
    assert!(game.campaign().blueprints[StructureKind::Trap]);
    let old: Settings = serde_json::from_str("{}").unwrap();
    assert_eq!(old.seen_callouts, None);
    let load = |game: &Game, settings: Settings, load_error: Option<String>| {
        FrontendParts::new(
            LoadedProfile {
                game: game.clone(),
                settings,
                warning: String::new(),
                load_error,
                settings_warning: None,
                settings_at_risk: false,
            },
            &LaunchConfig::default(),
            &fixture.frontend.paths,
        )
    };
    let mut f = load(&game, old.clone(), None);
    let mut expected: Vec<String> = CampaignFeature::ALL
        .iter()
        .map(|feature| format!("feature:{}", feature.id()))
        .collect();
    expected.push("blueprint:trap".into());
    expected.sort();
    assert_eq!(seen(&f), expected);
    // Nothing the journey already had is announced, on any camp page.
    f.screen = Screen::Camp;
    for tab in [Tab::Home, Tab::World, Tab::Briefing] {
        f.nav.tab = tab;
        assert_eq!(callout_lines(&settle(&h, &mut f)), Vec::<String>::new());
    }
    // What is unlocked after the load is announced.
    f.game.campaign_mut().blueprints[StructureKind::Wall] = true;
    assert_eq!(
        callout_lines(&settle(&h, &mut f)),
        ["New: Wall blueprint - press B in battle to build it"]
    );

    // A seeded file is left alone, even an empty one.
    let seeded = Settings {
        seen_callouts: Some(BTreeSet::new()),
        ..old.clone()
    };
    assert!(seen(&load(&game, seeded, None)).is_empty());
    // A save that could not be read is not the journey: seeding waits.
    let unread = load(&Game::default(), old, Some("unreadable".into()));
    assert_eq!(unread.settings.seen_callouts, None);

    // A New game starts the seen-set empty, so the new journey is taught.
    // (The Wall flag above skipped its purchase, and the old journey must
    // be saveable to be archived.)
    f.game = game.clone();
    f.screen = Screen::Menu;
    f.act(|c| c.dispatch(AppCommand::NewJourney, ""));
    f.flush_for_test();
    assert!(!f.status.message_error, "{}", f.status.message);
    assert!(f.game.campaign().world.start.is_none());
    assert_eq!(f.settings.seen_callouts, Some(BTreeSet::new()));
    f.runtime.finish_storage_for_test();
}

#[test]
fn restore_defaults_keeps_seen_callouts_and_writes_current_version() {
    let mut fixture = IsolatedFrontend::new();
    let f = &mut fixture.frontend;
    let seen_before: BTreeSet<String> = ["feature:companion".to_owned()].into();
    f.settings.seen_callouts = Some(seen_before.clone());
    f.settings.master = 0.1;
    f.settings.show_guidance = false;
    f.settings.bindings[Action::Build as usize] = "G".into();
    // As an older file reads before its load migration.
    f.settings.version = 0;
    f.screen = Screen::Settings;
    f.nav.return_screen = Screen::Menu;
    let h = Harness::new();
    // Tall enough that the Profile section at the page's end is painted.
    let tall = vec2(1440.0, 4000.0);
    for _ in 0..3 {
        h.frame_size(f, View::Settings, vec![], tall);
    }
    h.click_size(f, View::Settings, RESTORE, tall);
    assert_eq!(
        f.settings,
        Settings {
            seen_callouts: Some(seen_before.clone()),
            ..Settings::default()
        }
    );
    assert_eq!(f.settings.version, SETTINGS_VERSION);

    // The written file carries the current version and the kept set.
    f.act(|c| c.request_save(SaveIntent::Manual));
    f.flush_for_test();
    let written: serde_json::Value =
        serde_json::from_slice(&std::fs::read(f.paths.settings()).unwrap()).unwrap();
    assert_eq!(written["version"], SETTINGS_VERSION);
    assert_eq!(
        written["seen_callouts"],
        serde_json::json!(["feature:companion"])
    );
    assert_eq!(
        Settings::load(&f.paths.settings()).seen_callouts,
        Some(seen_before)
    );
}

/// Closing: a callout whose key a rebind left Unbound (Pause on B, or Cycle
/// attack cleared) never reads "press Unbound": it names the action as
/// unbound and where to bind it, as the battle hints do. Bound keys keep the
/// old wording.
#[test]
fn callouts_name_an_unbound_key_and_where_to_bind_it() {
    use crate::settings::{Action, BindingId, ExtraAction, Settings};
    let line = |settings: &Settings, id: &str| {
        let callout = onboarding::CALLOUTS
            .iter()
            .find(|callout| callout.id == id)
            .expect("callout");
        onboarding::text(callout, settings)
    };
    let mut settings = Settings::default();
    settings.set_binding(BindingId::Core(Action::Pause), "B".into());
    settings.set_binding(BindingId::Extra(ExtraAction::CycleAttack), String::new());
    assert_eq!(settings.binding(BindingId::Core(Action::Build)), "");
    assert_eq!(
        settings.binding(BindingId::Extra(ExtraAction::CycleAttack)),
        ""
    );
    for callout in onboarding::CALLOUTS {
        let text = onboarding::text(callout, &settings);
        assert!(
            !text.contains(crate::settings::UNBOUND),
            "{}: {text}",
            callout.id
        );
    }
    assert_eq!(
        line(&settings, "feature:construction"),
        "New: Build defenses - Construction is unbound; bind it in Settings › Controls"
    );
    assert_eq!(
        line(&settings, "blueprint:trap"),
        "New: Trap blueprint - Construction is unbound; bind it in Settings › Controls to build it"
    );
    assert_eq!(
        line(&settings, "ordnance:grenade"),
        "New: Grenades - Cycle attack is unbound; bind it in Settings › Controls to switch attacks"
    );
    // The bound wording is unchanged.
    let settings = Settings::default();
    assert_eq!(line(&settings, "blueprint:trap"), TRAP);
    assert_eq!(
        line(&settings, "feature:construction"),
        "New: Build defenses - press B in battle"
    );
    let cycle = settings.binding(BindingId::Extra(ExtraAction::CycleAttack));
    assert_eq!(
        line(&settings, "ordnance:grenade"),
        format!("New: Grenades - switch attacks with {cycle} in battle")
    );
}

#[test]
fn country_dossier_names_defense_count_and_mara_timing() {
    let germany = world().country("DEU").unwrap();
    let count = format!("{} defenses", germany.level_count());
    let mara = format!(
        "Liberate {} to unlock Mara in Journey upgrades.",
        germany.name
    );
    let has = |output: &FullOutput, text: &str| {
        output
            .shapes
            .iter()
            .any(|clipped| shape_has_text(&clipped.shape, text))
    };
    for target in Target::BOTH {
        let h = Harness::at(target);
        // A fresh journey choosing its start: Mara is met through the first
        // liberation, so the dossier says when.
        let mut fixture = IsolatedFrontend::new();
        let f = &mut fixture.frontend;
        f.game.campaign_mut().legacy_features = false;
        let preview = f.game.country_choice_preview("DEU").unwrap();
        assert!(preview.companion_after_liberation);
        assert_eq!(preview.stage_count, germany.level_count());
        let output = h.frame(f, View::Dossier, vec![]);
        assert!(has(&output, &count), "{target:?}");
        assert_eq!(text_count(&output, &mara), 1, "{target:?}");
        assert!(fully_visible(&output, &mara), "{target:?}");

        // Once Mara is in camp the line is gone; the count stays.
        f.game.campaign_mut().legacy_features = true;
        f.game.campaign_mut().attempts = 1;
        assert!(
            !f.game
                .country_choice_preview("DEU")
                .unwrap()
                .companion_after_liberation
        );
        let output = h.frame(f, View::Dossier, vec![]);
        assert!(has(&output, &count), "{target:?}");
        assert_eq!(text_count(&output, &mara), 0, "{target:?}");
    }
}
