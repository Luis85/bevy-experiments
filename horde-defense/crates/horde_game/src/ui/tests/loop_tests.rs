//! The whole loop, headless, at both target sizes (W3-28, UX-19, acceptance
//! G2.12): new game, Rook, country, defeat, Results, Upgrade, buy, Back to
//! battle, victory, Next, liberation, Recruit, Back to camp, the recruit
//! defends.
//!
//! Every move is a click or a key on the whole App frame (`View::Full`), from
//! a first launch with nothing preset. Three things stand in for play the
//! harness cannot do in a test's time, each through the idiom the other UI
//! tests use and each marked where it happens: a defense is lost by the
//! runner settling an attempt at 0 HP, won by the runner settling one whose
//! boss is out and whose last enemy is down, and the wallet is topped up to
//! stand for the coins of the rounds that were not fought.
use super::*;
use crate::ui::modal::{AppModal, RecruitPhase};
use horde_core::{
    campaign::NodePayment,
    content::EnemyKind,
    defenders::DefenderId,
    skill_catalog::{CampaignFeature, SkillEffect, skills},
    strategy::RecruitStatus,
};

/// The labels of the last frame's modal primaries.
fn modal_primaries(h: &Harness) -> Vec<String> {
    h.primaries()
        .into_iter()
        .filter(|record| record.surface == Surface::Modal)
        .map(|record| record.label)
        .collect()
}

/// A loop step's place: the settled App frame is on `screen` and `tab`, draws
/// exactly one screen primary, `primary`, and has no modal over it.
fn at(h: &Harness, f: &mut FrontendParts, step: &str, screen: Screen, tab: Tab, primary: &str) {
    for _ in 0..3 {
        h.frame(f, View::Full, vec![]);
    }
    let target = h.target;
    assert!(
        f.screen == screen && f.nav.tab == tab,
        "{target:?} {step}: on {:?} / {:?}, expected {screen:?} / {tab:?} ({})",
        f.screen,
        f.nav.tab,
        f.status.message
    );
    assert_eq!(
        h.screen_primaries(),
        [primary],
        "{target:?} {step}: exactly one primary"
    );
    assert_eq!(f.status.modal, None, "{target:?} {step}: no modal is open");
}

/// A running battle draws no primary: amber is the paused Begin or Resume.
fn running(h: &Harness, f: &mut FrontendParts, step: &str) {
    h.frame(f, View::Full, vec![]);
    let target = h.target;
    assert!(f.screen == Screen::Battle, "{target:?} {step}");
    assert!(
        !f.game.attempt().expect("a live defense").paused,
        "{target:?} {step}: the defense runs"
    );
    assert_eq!(
        h.screen_primaries(),
        Vec::<String>::new(),
        "{target:?} {step}: a running battle has no primary"
    );
}

/// Runner passes until the live defense settles, the battle bar's controls
/// published as the frame before each pass publishes them.
fn settle_defense(h: &Harness, f: &mut FrontendParts, step: &str) {
    for _ in 0..8 {
        if f.game.attempt().is_none() {
            break;
        }
        h.frame(f, View::Full, vec![]);
        f.step();
    }
    assert!(
        f.game.attempt().is_none(),
        "{:?} {step}: the defense settled",
        h.target
    );
}

/// Stand-in for a lost fight: the wall is down, the runner settles it.
fn lose(h: &Harness, f: &mut FrontendParts, step: &str) {
    f.game.attempt_mut().expect("a live defense").hp = 0;
    settle_defense(h, f, step);
    assert_eq!(
        f.game.last_result().unwrap().outcome,
        Outcome::Defeat,
        "{step}"
    );
}

/// Stand-in for a won fight: the boss is out and the last enemy is down, the
/// runner settles it.
fn win(h: &Harness, f: &mut FrontendParts, step: &str) {
    let mut a = f.game.attempt_slot_mut().take().expect("a live defense");
    a.boss_spawned = true;
    let lane = a.lane_count / 2;
    f.game.spawn_enemy(&mut a, EnemyKind::Shambler, lane, false);
    let last = a.enemies.last_mut().unwrap();
    last.x = a.width_yards * 0.5;
    last.injury.unconscious = true;
    *f.game.attempt_slot_mut() = Some(a);
    settle_defense(h, f, step);
    assert_eq!(
        f.game.last_result().unwrap().outcome,
        Outcome::Victory,
        "{step}"
    );
}

/// The tree's purchase button for node `id`.
fn learn_label(f: &FrontendParts, id: &str) -> String {
    match f.game.campaign().node_payment(id).unwrap() {
        NodePayment::Coins(price) => {
            format!("Learn skill · {} coins", crate::ui::coins::grouped(price))
        }
        NodePayment::LegacyBodyPoints(points) => format!("Learn skill · {points} legacy CP"),
    }
}

/// The id of the node that unlocks `feature`.
fn feature_node(feature: CampaignFeature) -> String {
    skills()
        .nodes
        .iter()
        .find(|node| {
            node.effects
                .iter()
                .any(|effect| matches!(effect, SkillEffect::FeatureUnlock(f) if *f == feature))
        })
        .map(|node| node.id.clone())
        .unwrap_or_else(|| panic!("a {feature:?} node"))
}

/// The unowned nodes on the way to `id`, prerequisites first, `id` last.
fn path_to(f: &FrontendParts, id: &str) -> Vec<String> {
    fn visit(f: &FrontendParts, id: &str, path: &mut Vec<String>) {
        if f.game.campaign().has_node(id) || path.iter().any(|seen| seen == id) {
            return;
        }
        for parent in &skills().node(id).unwrap().prerequisites {
            visit(f, parent, path);
        }
        path.push(id.to_owned());
    }
    let mut path = Vec::new();
    visit(f, id, &mut path);
    path
}

/// Buys node `id` on the Upgrades screen as a player does: its name typed
/// into the tree's search box (Enter steps through the matches), then the
/// purchase button. `searched` is the text the box already holds.
fn buy_in_tree(h: &Harness, f: &mut FrontendParts, id: &str, searched: Option<&str>) {
    let target = h.target;
    let name = skills().node(id).unwrap().name.clone();
    h.click(f, View::Full, searched.unwrap_or("Search upgrades"));
    let select_all = Event::Key {
        key: Key::A,
        physical_key: Some(Key::A),
        pressed: true,
        repeat: false,
        modifiers: Modifiers::COMMAND,
    };
    h.frame(f, View::Full, vec![select_all]);
    h.frame(f, View::Full, vec![Event::Text(name.clone())]);
    h.frame(f, View::Full, vec![]);
    let selected = || progression::selected_node_for_test(&h.ctx);
    for _ in 0..8 {
        if selected().as_deref() == Some(id) {
            break;
        }
        h.key(f, View::Full, Key::Enter);
        h.frame(f, View::Full, vec![]);
    }
    assert_eq!(
        selected().as_deref(),
        Some(id),
        "{target:?}: the search selects {name}"
    );
    let learn = learn_label(f, id);
    h.click(f, View::Full, &learn);
    assert!(
        f.game.campaign().has_node(id),
        "{target:?}: {name} bought ({} / {:?})",
        f.status.message,
        f.game.campaign().can_purchase_node(id)
    );
}

/// Clicks the `label` on the row of `row`: of every text run reading
/// `label`, the one nearest in height to the run reading `row`.
fn click_in_row(h: &Harness, f: &mut FrontendParts, row: &str, label: &str) {
    fn collect(shape: &egui::epaint::Shape, label: &str, found: &mut Vec<Pos2>) {
        match shape {
            egui::epaint::Shape::Text(text) if text.galley.text() == label => {
                found.push(text.pos + text.galley.rect.center().to_vec2());
            }
            egui::epaint::Shape::Vec(shapes) => {
                for shape in shapes {
                    collect(shape, label, found);
                }
            }
            _ => {}
        }
    }
    let output = h.frame(f, View::Full, vec![]);
    let all = |label: &str| {
        let mut found = Vec::new();
        for clipped in &output.shapes {
            collect(&clipped.shape, label, &mut found);
        }
        found
    };
    let rows = all(row);
    assert_eq!(rows.len(), 1, "{:?}: one row reads {row}", h.target);
    let position = all(label)
        .into_iter()
        .min_by(|a, b| (a.y - rows[0].y).abs().total_cmp(&(b.y - rows[0].y).abs()))
        .unwrap_or_else(|| panic!("Rendered action not found: {label}"));
    assert!(
        (position.y - rows[0].y).abs() < 12.0,
        "{:?}: {label} sits on the row of {row}",
        h.target
    );
    h.press_at(f, View::Full, position, h.target.pixels);
}

fn full_loop(target: Target) {
    let mut fixture = IsolatedFrontend::first_launch();
    let h = Harness::at(target);
    let f = &mut fixture.frontend;
    assert!(f.game.campaign().selected_survivor.is_none());
    assert!(f.game.campaign().world.start.is_none());

    // 1. New game: a first launch has no journey to resume.
    at(&h, f, "1 menu", Screen::Menu, Tab::Home, "New game");
    h.click(f, View::Full, "New game");

    // 2. Rook.
    h.frame(f, View::Full, vec![]);
    assert!(f.screen == Screen::SurvivorSelection, "{target:?}");
    h.click(f, View::Full, SurvivorId::Rook.name());
    at(
        &h,
        f,
        "2 survivor",
        Screen::SurvivorSelection,
        Tab::Home,
        "Continue as Rook",
    );
    h.click(f, View::Full, "Continue as Rook");
    assert_eq!(
        f.game.campaign().selected_survivor,
        Some(SurvivorId::Rook),
        "{target:?}"
    );

    // 3. Country: the map opens on the start choice; "Start journey" is the
    // first defense, deployed paused.
    at(
        &h,
        f,
        "3 country",
        Screen::Camp,
        Tab::World,
        "Start journey",
    );
    let country = world().country(&f.campaign.selected_country).unwrap();
    let levels = country.level_count();
    h.click(f, View::Full, "Start journey");
    assert_eq!(
        f.game.campaign().world.start.as_deref(),
        Some(country.id.as_str()),
        "{target:?}: {}",
        f.status.message
    );

    // 4. The first defense waits on "Begin defense", then is lost.
    at(
        &h,
        f,
        "4 first defense",
        Screen::Battle,
        Tab::World,
        "Begin defense",
    );
    assert_eq!(f.game.attempt().unwrap().defender(), DefenderId::Player);
    h.click(f, View::Full, "Begin defense");
    running(&h, f, "4 first defense");
    lose(&h, f, "4 first defense");

    // 5. Results after the defeat. With coins for the recommended upgrade
    // the primary is "Upgrade"; the bar keeps its three actions.
    // Stand-in: the wallet holds what the unfought rounds would have paid.
    f.game.campaign_mut().coins = 100_000;
    at(&h, f, "5 defeat", Screen::Results, Tab::Home, "Upgrade");
    let output = h.frame(f, View::Full, vec![]);
    for label in ["Upgrade", "Try again", "Leave journey"] {
        assert_eq!(text_count(&output, label), 1, "{target:?}: {label}");
    }
    h.click(f, View::Full, "Upgrade");

    // 6. Upgrades: buy the recommended node, then "Back to battle".
    at(
        &h,
        f,
        "6 upgrades",
        Screen::Upgrades,
        Tab::Home,
        "Back to battle",
    );
    let node = progression::selected_node_for_test(&h.ctx).expect("the tree is drawn");
    assert!(
        !f.game.campaign().has_node(&node),
        "{target:?}: the tree opens on an upgrade to buy"
    );
    let learn = learn_label(f, &node);
    h.click(f, View::Full, &learn);
    assert!(
        f.game.campaign().has_node(&node),
        "{target:?}: {node} bought ({})",
        f.status.message
    );
    at(
        &h,
        f,
        "6 upgrades after the purchase",
        Screen::Upgrades,
        Tab::Home,
        "Back to battle",
    );
    h.click(f, View::Full, "Back to battle");

    // 7. Back on the same stage, live from tick 0. Paused after a few ticks
    // it offers "Resume defense"; resumed, it is won.
    running(&h, f, "7 back to battle");
    assert_eq!(f.game.attempt().unwrap().stage, 0, "{target:?}");
    assert_eq!(f.game.attempt().unwrap().tick, 0, "{target:?}");
    for _ in 0..3 {
        h.frame(f, View::Full, vec![]);
        f.step();
    }
    assert!(f.game.attempt().unwrap().tick > 0, "{target:?}: it runs");
    h.click(f, View::Full, "Pause");
    at(
        &h,
        f,
        "7 paused",
        Screen::Battle,
        Tab::Home,
        "Resume defense",
    );
    h.click(f, View::Full, "Resume defense");
    running(&h, f, "7 resumed");
    win(&h, f, "7 victory");

    // 8. Results after the victory: "Next defense", stage by stage, until
    // the country is liberated.
    for stage in 1..levels {
        let step = format!("8 next defense {stage}");
        at(&h, f, &step, Screen::Results, Tab::Home, "Next defense");
        h.click(f, View::Full, "Next defense");
        h.frame(f, View::Full, vec![]);
        assert!(f.screen == Screen::Battle, "{target:?} {step}");
        let attempt = f.game.attempt().expect("the next defense deployed");
        assert_eq!(attempt.stage, stage, "{target:?} {step}");
        running(&h, f, &step);
        win(&h, f, &step);
    }
    at(
        &h,
        f,
        "8 liberation",
        Screen::Results,
        Tab::Home,
        "Choose next country",
    );
    assert!(f.game.last_result().unwrap().country_freed, "{target:?}");
    assert_eq!(
        f.game.campaign().world.completed(&country.id),
        levels,
        "{target:?}"
    );

    // The liberation opens the recruitment node: the tree sells the path to
    // it (found by name in the search box), and Escape leads back to the
    // debrief.
    let network = feature_node(CampaignFeature::Recruitment);
    assert!(!f.game.campaign().has_node(&network), "{target:?}");
    h.click(f, View::Full, "Upgrade");
    at(
        &h,
        f,
        "8 upgrades after the liberation",
        Screen::Upgrades,
        Tab::Home,
        "Choose next country",
    );
    let mut searched: Option<String> = None;
    for id in path_to(f, &network) {
        buy_in_tree(&h, f, &id, searched.as_deref());
        searched = Some(skills().node(&id).unwrap().name.clone());
    }
    assert!(
        f.game.campaign().has_feature(CampaignFeature::Recruitment),
        "{target:?}"
    );
    at(
        &h,
        f,
        "8 upgrades with recruitment open",
        Screen::Upgrades,
        Tab::Home,
        "Choose next country",
    );
    h.key(f, View::Full, Key::Escape);

    // 9. Recruit: the block on Results opens the modal, the survivor joins,
    // "Back to camp" opens Camp home on the new roster row.
    at(
        &h,
        f,
        "9 results with the recruit offer",
        Screen::Results,
        Tab::Home,
        "Choose next country",
    );
    let offer = f
        .game
        .recruit_offer(&country.id)
        .expect("the liberated country offers a recruit");
    let recruit_from = format!("Recruit from {} · {} coins", country.name, offer.price);
    h.click(f, View::Full, &recruit_from);
    assert_eq!(
        f.status.modal,
        Some(AppModal::Recruit {
            country: country.id.clone(),
            phase: RecruitPhase::Choose,
        }),
        "{target:?}"
    );
    let recruit_as = format!(
        "Recruit as {} · {} coins",
        offer.archetypes[0].archetype.label(),
        offer.price
    );
    h.frame(f, View::Full, vec![]);
    assert_eq!(
        modal_primaries(&h),
        std::slice::from_ref(&recruit_as),
        "{target:?}"
    );
    h.click(f, View::Full, &recruit_as);
    let roster = &f.game.strategy().roster;
    assert_eq!(roster.len(), 1, "{target:?}: {}", f.status.message);
    let recruit = roster[0].id;
    let recruit_name = roster[0].name.clone();
    h.frame(f, View::Full, vec![]);
    assert_eq!(modal_primaries(&h), ["Back to camp"], "{target:?}");
    h.click(f, View::Full, "Back to camp");
    assert_eq!(f.status.modal, None, "{target:?}");
    at(
        &h,
        f,
        "9 back to camp",
        Screen::Camp,
        Tab::Home,
        "Choose next country",
    );
    assert_eq!(
        f.nav.home_focus,
        Some(HomeFocus::Recruit(recruit)),
        "{target:?}"
    );

    // 10. The recruit defends: chosen on the roster, the next front planned
    // on the map, deployed from its briefing onto the field, live at once.
    click_in_row(&h, f, &recruit_name, "Defend next");
    assert_eq!(
        f.game.next_defender(),
        DefenderId::Recruit(recruit),
        "{target:?}: {}",
        f.status.message
    );
    h.click(f, View::Full, "Choose next country");
    at(
        &h,
        f,
        "10 next front",
        Screen::Camp,
        Tab::World,
        "Plan defense",
    );
    let front = f.campaign.selected_country.clone();
    assert_ne!(
        front, country.id,
        "{target:?}: a front the liberation opened"
    );
    h.click(f, View::Full, "Plan defense");
    let deploy = format!("Deploy {recruit_name}");
    at(&h, f, "10 briefing", Screen::Camp, Tab::Briefing, &deploy);
    assert!(
        f.game.attempt().is_none(),
        "{target:?}: planning never deploys"
    );
    h.click(f, View::Full, &deploy);
    running(&h, f, "10 the recruit defends");
    let attempt = f.game.attempt().expect("the briefing primary deployed");
    assert_eq!(
        attempt.defender(),
        DefenderId::Recruit(recruit),
        "{target:?}"
    );
    assert_eq!(
        attempt.country_id.as_deref(),
        Some(front.as_str()),
        "{target:?}"
    );
    assert_eq!(
        f.game.strategy().roster[0].status,
        RecruitStatus::Defending,
        "{target:?}"
    );
    for _ in 0..3 {
        h.frame(f, View::Full, vec![]);
        f.step();
    }
    let attempt = f.game.attempt().expect("the recruit holds the field");
    assert!(attempt.tick > 0, "{target:?}: the recruit's defense runs");
    assert_eq!(
        attempt.defender(),
        DefenderId::Recruit(recruit),
        "{target:?}"
    );
    h.click(f, View::Full, "Pause");
    at(
        &h,
        f,
        "10 the recruit's defense, paused",
        Screen::Battle,
        Tab::Briefing,
        "Resume defense",
    );
}

/// G2.12: the loop from a first launch to a recruit defending, with Screen,
/// Tab and exactly one primary asserted at every step, at 1440x900@1.0 and
/// 1050x700@1.35.
#[test]
fn full_loop_new_game_to_recruit_defends() {
    for target in Target::BOTH {
        full_loop(target);
    }
}
