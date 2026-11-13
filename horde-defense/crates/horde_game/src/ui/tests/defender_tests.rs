//! Egui harness tests: the defender picker and the takeover labels (W3-16,
//! ruling D16). The picker reads `Game::available_defenders` and dispatches
//! `SetNextDefender`; the Briefing primary dispatches `BeginDefense`; the HUD
//! names whoever holds the defense through `Game::defender_card`; the battle
//! bar's Tactics popup edits the defending recruit with `SetRecruitTactics`.
use super::*;
use horde_core::{
    defenders::{DefenderBlocker, DefenderId},
    strategy::RecruitStatus,
};

/// A journey in Germany between defenses with Mara open and one Ready
/// recruit from each of `origins` (liberated neighbours). Returns the
/// recruits' ids in `origins` order.
fn camp_with_recruits(f: &mut FrontendParts, origins: &[&str]) -> Vec<u64> {
    f.game.campaign_mut().choose_start_country("DEU").unwrap();
    f.game.campaign_mut().journey_active = true;
    f.game.campaign_mut().attempts = 1;
    f.game.campaign_mut().coins = 5_000;
    for id in origins {
        liberate(f, id);
    }
    let ids: Vec<u64> = origins
        .iter()
        .map(|origin| {
            f.game
                .with_strategy(|strategy, campaign| strategy.recruit(campaign, origin))
                .unwrap()
        })
        .collect();
    f.game.set_paused(true);
    for id in &ids {
        let option = option_of(&f.game, DefenderId::Recruit(*id));
        assert_eq!(option.selectable, Ok(()), "a fresh recruit can defend");
    }
    ids
}

/// Marks country `id` liberated.
fn liberate(f: &mut FrontendParts, id: &str) {
    f.game.campaign_mut().world.cleared.push(id.into());
    f.game
        .campaign_mut()
        .world
        .country_levels
        .insert(id.into(), world().country(id).unwrap().level_count());
}

fn option_of(game: &Game, id: DefenderId) -> horde_core::defenders::DefenderOption {
    game.available_defenders()
        .into_iter()
        .find(|option| option.card.id == id)
        .unwrap_or_else(|| panic!("{id:?} is listed"))
}

fn name_of(game: &Game, id: DefenderId) -> String {
    game.defender_card(id).expect("a listed defender").name
}

/// Recruit `id` takes over the main run: chosen with `SetNextDefender`,
/// deployed with `BeginDefense`, shown paused on the battle screen.
fn takeover(f: &mut FrontendParts, id: u64) {
    f.game
        .apply(&Command::SetNextDefender(DefenderId::Recruit(id)))
        .unwrap();
    f.game.apply(&Command::BeginDefense { stage: 0 }).unwrap();
    f.game.set_paused(true);
    f.screen = Screen::Battle;
    let a = f.game.attempt().expect("the takeover deployed");
    assert_eq!(a.defender(), DefenderId::Recruit(id));
    f.battle.aim = [a.width_yards * 0.5, a.defender_position()[1]];
}

#[test]
fn picker_lists_ready_recruit_and_dispatches_set_next_defender() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    let recruit = camp_with_recruits(f, &["FRA"])[0];
    let recruit_name = name_of(&f.game, DefenderId::Recruit(recruit));
    f.act(|c| c.open_briefing());
    assert_eq!(f.game.next_defender(), DefenderId::Player);

    // The picker lists every defender core lists, by its card name, and the
    // primary deploys the player personally.
    let output = h.frame(f, View::Camp, vec![]);
    for id in [
        DefenderId::Player,
        DefenderId::Mara,
        DefenderId::Recruit(recruit),
    ] {
        let name = name_of(&f.game, id);
        assert_eq!(text_count(&output, &name), 1, "{name} is one picker chip");
    }
    assert_eq!(text_count(&output, "Deploy personally"), 1);
    let entries = chrome::picker_entries(&f.game);
    assert_eq!(
        entries.iter().map(|e| e.id).collect::<Vec<_>>(),
        [
            DefenderId::Player,
            DefenderId::Mara,
            DefenderId::Recruit(recruit)
        ]
    );
    assert!(entries.iter().all(|e| e.blocked.is_none()));

    // Choosing the recruit is the SetNextDefender command, nothing else.
    h.click(f, View::Camp, &recruit_name);
    assert_eq!(
        f.game.campaign().next_defender,
        Some(DefenderId::Recruit(recruit))
    );
    assert!(f.game.attempt().is_none(), "choosing never deploys");

    // The briefing now reads the recruit: its name on the primary, and the
    // primary is BeginDefense (the recruit holds the seat, the choice clears).
    let deploy = format!("Deploy {recruit_name}");
    let output = h.frame(f, View::Camp, vec![]);
    assert_eq!(text_count(&output, &deploy), 1);
    assert_eq!(text_count(&output, "Deploy personally"), 0);
    assert_eq!(text_count(&output, "Deploy survivor"), 0);
    h.click(f, View::Camp, &deploy);
    let attempt = f.game.attempt().expect("the briefing primary deployed");
    assert_eq!(attempt.defender(), DefenderId::Recruit(recruit));
    assert_eq!(f.game.campaign().next_defender, None);
    assert_eq!(
        f.game
            .strategy()
            .roster
            .iter()
            .find(|r| r.id == recruit)
            .unwrap()
            .status,
        RecruitStatus::Defending
    );
}

#[test]
fn picker_is_a_combo_box_beyond_three_defenders() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    let recruits = camp_with_recruits(f, &["FRA", "POL"]);
    let second = name_of(&f.game, DefenderId::Recruit(recruits[1]));
    let player = name_of(&f.game, DefenderId::Player);
    f.act(|c| c.open_briefing());
    assert_eq!(chrome::picker_entries(&f.game).len(), 4);
    let output = h.frame(f, View::Camp, vec![]);
    // Closed, the combo box names the next defender only.
    assert_eq!(text_count(&output, &player), 1);
    assert_eq!(text_count(&output, &second), 0);
    h.click(f, View::Camp, &player);
    h.click(f, View::Camp, &second);
    assert_eq!(f.game.next_defender(), DefenderId::Recruit(recruits[1]));
}

#[test]
fn guarding_recruit_disabled_with_core_reason() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    let recruit = camp_with_recruits(f, &["FRA"])[0];
    // Garrisons open with the strategy layer, a few liberations in.
    for id in ["POL", "AUT", "CZE", "CHE"] {
        liberate(f, id);
    }
    f.game
        .apply(&Command::AssignGuard {
            id: recruit,
            country: "FRA".into(),
        })
        .unwrap();
    let blocker = option_of(&f.game, DefenderId::Recruit(recruit))
        .selectable
        .expect_err("a guard cannot defend");
    assert_eq!(
        blocker,
        DefenderBlocker::Guarding {
            country_id: "FRA".into()
        }
    );
    // The entry carries core's reason, verbatim.
    let entry = chrome::picker_entries(&f.game)
        .into_iter()
        .find(|e| e.id == DefenderId::Recruit(recruit))
        .expect("a guard stays listed");
    assert_eq!(entry.blocked, Some(blocker.label()));

    f.act(|c| c.open_briefing());
    let name = name_of(&f.game, DefenderId::Recruit(recruit));
    let shown = format!("{name} · {}", blocker.label());
    let output = h.frame(f, View::Camp, vec![]);
    assert_eq!(text_count(&output, &shown), 1, "the reason is on the chip");
    assert_eq!(text_count(&output, &name), 0);
    // The disabled chip dispatches nothing.
    h.click(f, View::Camp, &shown);
    assert_eq!(f.game.campaign().next_defender, None);
    assert_eq!(f.game.next_defender(), DefenderId::Player);
    assert!(!f.status.message_error, "{}", f.status.message);
}

#[test]
fn results_picker_persists_choice_across_save_load() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    let recruit = camp_with_recruits(f, &["FRA"])[0];
    let name = name_of(&f.game, DefenderId::Recruit(recruit));
    f.game.deploy_country_stage(0, Mode::Personal).unwrap();
    f.game.withdraw();
    f.game.set_paused(true);
    assert!(f.game.attempt().is_none() && f.game.last_result().is_some());
    f.screen = Screen::Results;

    let output = h.frame(f, View::Results, vec![]);
    assert_eq!(text_count(&output, &name), 1, "Results offers the recruit");
    h.click(f, View::Results, &name);
    assert_eq!(f.game.next_defender(), DefenderId::Recruit(recruit));

    f.act(|c| c.request_save(SaveIntent::Manual));
    f.flush_for_test();
    let restored = Game::load(&f.paths.campaign()).unwrap();
    assert_eq!(
        restored.campaign().next_defender,
        Some(DefenderId::Recruit(recruit))
    );
    assert_eq!(restored.next_defender(), DefenderId::Recruit(recruit));
    // The loaded game shows the same choice.
    f.game = restored;
    let selected: Vec<DefenderId> = chrome::picker_entries(&f.game)
        .into_iter()
        .filter(|e| e.selected)
        .map(|e| e.id)
        .collect();
    assert_eq!(selected, [DefenderId::Recruit(recruit)]);
}

#[test]
fn recruit_takeover_hud_names_the_recruit() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    let recruit = camp_with_recruits(f, &["FRA"])[0];
    takeover(f, recruit);
    let card = f.game.active_defender().expect("the defending recruit");
    assert_eq!(card.id, DefenderId::Recruit(recruit));
    assert_ne!(card.hud_name, "MARA");

    let view = hud::view_model(&f.game).unwrap();
    assert_eq!(view.defender, card.hud_name);
    // The portrait follows the defender's own appearance, never a forced one.
    assert_eq!(
        view.female_portrait,
        card.appearance == Appearance::Female,
        "{:?}",
        card.appearance
    );
    assert_eq!(card.appearance, Appearance::Male, "recruit 1 is Alex Rowan");

    let a = f.game.attempt().unwrap();
    let hp = format!("{} · HP {} / {}", card.hud_name, a.hp.max(0), a.max_hp);
    let output = h.frame(f, View::Battle, vec![]);
    assert_eq!(text_count(&output, &hp), 1, "the HUD names the recruit");
    assert!(
        !output
            .shapes
            .iter()
            .any(|s| shape_has_text(&s.shape, "MARA")),
        "Mara is not on this field"
    );
}

#[test]
fn hud_tactics_dispatch_set_recruit_tactics_during_takeover() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    let recruit = camp_with_recruits(f, &["FRA"])[0];
    takeover(f, recruit);
    let name = name_of(&f.game, DefenderId::Recruit(recruit));
    let early_reload = |game: &Game| {
        game.strategy()
            .roster
            .iter()
            .find(|r| r.id == recruit)
            .unwrap()
            .npc
            .early_reload
    };
    let before = early_reload(&f.game);
    let mara = serde_json::to_value(&f.game.campaign().npc).unwrap();

    for _ in 0..2 {
        h.frame(f, View::Battle, vec![]);
    }
    let output = h.frame(f, View::Battle, vec![]);
    assert_eq!(text_count(&output, "Tactics"), 1);
    assert_eq!(text_count(&output, "Survivor"), 0);
    h.click(f, View::Battle, "Tactics");
    // The popup's title names the defender whose tactics it edits.
    let output = h.frame(f, View::Battle, vec![]);
    assert_eq!(text_count(&output, &format!("{name} · tactics")), 1);
    h.click(f, View::Battle, "Reload early when safe");

    assert_eq!(
        early_reload(&f.game),
        !before,
        "the recruit's tactics moved"
    );
    assert_eq!(
        serde_json::to_value(&f.game.campaign().npc).unwrap(),
        mara,
        "Mara's tactics are untouched"
    );
    assert!(!f.status.message_error, "{}", f.status.message);
    assert_eq!(
        f.game.attempt().unwrap().defender(),
        DefenderId::Recruit(recruit)
    );
}

/// Mara keeps her name and portrait: the removal of the hard-coded "MARA"
/// changes nothing for her.
#[test]
fn mara_hud_name_and_portrait_come_from_her_card() {
    let mut fixture = IsolatedFrontend::new();
    let f = &mut fixture.frontend;
    camp_with_recruits(f, &[]);
    f.game.deploy_country_stage(0, Mode::Npc).unwrap();
    let card = f.game.active_defender().unwrap();
    assert_eq!(card.id, DefenderId::Mara);
    let view = hud::view_model(&f.game).unwrap();
    assert_eq!(view.defender, "MARA");
    assert_eq!(view.defender, card.hud_name);
    assert!(view.female_portrait);
}
