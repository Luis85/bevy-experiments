//! Egui harness tests: the liberation recruit block on Results and the
//! Recruit modal (W3-19; design-ux-flow decision 4, §4 "Results" and
//! "Recruit modal"). The block and the modal read `Game::recruit_offer`
//! (price, name, origin trait, the two archetypes) and the modal dispatches
//! `Command::RecruitAs`; a joined recruit leads "Back to camp" with the
//! roster row highlighted.
use super::home_tests::{liberate_germany, settle};
use super::*;
use crate::ui::modal::{AppModal, RecruitPhase};
use horde_core::defenders::RecruitBlocker;

/// The labels of the last frame's modal primaries.
fn modal_primaries(h: &Harness) -> Vec<String> {
    h.primaries()
        .into_iter()
        .filter(|record| record.surface == Surface::Modal)
        .map(|record| record.label)
        .collect()
}

/// Results after the victory that liberated Germany, with `coins` in camp.
fn liberation_results(f: &mut FrontendParts, coins: u64) {
    f.game.campaign_mut().choose_start_country("DEU").unwrap();
    f.game.campaign_mut().journey_active = true;
    liberate_germany(f);
    f.game.campaign_mut().coins = coins;
    f.screen = Screen::Results;
    assert!(f.game.last_result().unwrap().country_freed);
}

fn recruit_from(price: u64) -> String {
    format!("Recruit from Germany · {price} coins")
}

fn choose_modal() -> Option<AppModal> {
    Some(AppModal::Recruit {
        country: "DEU".into(),
        phase: RecruitPhase::Choose,
    })
}

fn paints(output: &FullOutput, part: &str) -> bool {
    output.shapes.iter().any(|s| shape_has_text(&s.shape, part))
}

#[test]
fn liberation_results_offers_recruit_when_node_owned() {
    for target in Target::BOTH {
        let mut fixture = IsolatedFrontend::new();
        let h = Harness::at(target);
        let f = &mut fixture.frontend;
        liberation_results(f, 1_000);
        let offer = f
            .game
            .recruit_offer("DEU")
            .expect("Germany offers a recruit");
        let label = recruit_from(offer.price);
        let output = settle(&h, f);
        assert_eq!(text_count(&output, &label), 1, "{target:?}: {label}");
        assert!(fully_visible(&output, &label), "{target:?}");
        assert_eq!(f.status.modal, None);

        // The block opens the Recruit modal on its Choose phase: the core
        // offer's name, origin trait and two archetypes.
        h.click(f, View::Full, &label);
        assert_eq!(f.status.modal, choose_modal(), "{target:?}");
        let output = h.frame(f, View::Full, vec![]);
        let buttons = offer
            .archetypes
            .map(|c| format!("Recruit as {} · {} coins", c.archetype.label(), offer.price));
        for button in &buttons {
            assert_eq!(text_count(&output, button), 1, "{target:?}: {button}");
            assert!(fully_visible(&output, button), "{target:?}: {button}");
        }
        let who = format!("{} · {}", offer.name, offer.origin_trait.label());
        assert_eq!(text_count(&output, &who), 1, "{target:?}: {who}");
        assert_eq!(modal_primaries(&h), [buttons[0].clone()], "{target:?}");
        assert!(
            f.game.strategy().roster.is_empty(),
            "opening recruits nobody"
        );

        // Escape and "Not now" close it without recruiting.
        h.key(f, View::Full, Key::Escape);
        assert_eq!(f.status.modal, None, "{target:?}: Escape closes");
        h.click(f, View::Full, &label);
        h.click(f, View::Full, "Not now");
        assert_eq!(f.status.modal, None, "{target:?}: Not now closes");
        assert!(f.game.strategy().roster.is_empty());
        assert_eq!(f.game.campaign().coins, 1_000);

        // Recruiting as the second archetype dispatches RecruitAs, charges
        // the core price and moves the modal to Joined.
        h.click(f, View::Full, &label);
        h.click(f, View::Full, &buttons[1]);
        let roster = &f.game.strategy().roster;
        assert_eq!(roster.len(), 1, "{target:?}: {}", f.status.message);
        let recruit = roster[0].id;
        assert_eq!(roster[0].name, offer.name);
        assert_eq!(
            roster[0].npc.creation.archetype,
            offer.archetypes[1].archetype.id()
        );
        assert_eq!(f.game.campaign().coins, 1_000 - offer.price);
        assert_eq!(
            f.status.modal,
            Some(AppModal::Recruit {
                country: "DEU".into(),
                phase: RecruitPhase::Joined {
                    recruit_id: recruit
                },
            }),
            "{target:?}"
        );
        let output = h.frame(f, View::Full, vec![]);
        let joined = format!("{} joined your camp.", offer.name);
        assert_eq!(text_count(&output, &joined), 1, "{target:?}: {joined}");
        assert_eq!(text_count(&output, "Stay here"), 1, "{target:?}");
        assert_eq!(modal_primaries(&h), ["Back to camp"], "{target:?}");
        assert!(f.screen == Screen::Results, "{target:?}: still on Results");

        // "Back to camp" opens Camp home with the new roster row focused.
        h.click(f, View::Full, "Back to camp");
        assert_eq!(f.status.modal, None, "{target:?}");
        assert!(f.screen == Screen::Camp && f.nav.tab == Tab::Home);
        assert_eq!(f.nav.home_focus, Some(HomeFocus::Recruit(recruit)));
        assert_eq!(f.game.strategy().roster.len(), 1, "one recruit, once");
    }
}

/// "Stay here" keeps Results, and Escape closes the Joined phase too.
#[test]
fn joined_recruit_modal_stays_on_results() {
    for stay_by_escape in [false, true] {
        let mut fixture = IsolatedFrontend::new();
        let h = Harness::new();
        let f = &mut fixture.frontend;
        liberation_results(f, 1_000);
        let offer = f.game.recruit_offer("DEU").unwrap();
        h.click(f, View::Full, &recruit_from(offer.price));
        h.click(
            f,
            View::Full,
            &format!(
                "Recruit as {} · {} coins",
                offer.archetypes[0].archetype.label(),
                offer.price
            ),
        );
        assert_eq!(f.game.strategy().roster.len(), 1, "{}", f.status.message);
        if stay_by_escape {
            h.key(f, View::Full, Key::Escape);
        } else {
            h.click(f, View::Full, "Stay here");
        }
        assert_eq!(f.status.modal, None);
        assert!(f.screen == Screen::Results);
        assert_eq!(f.nav.home_focus, None);
    }
}

#[test]
fn liberation_results_names_missing_survivor_network() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    // A journey that has to buy its features: the Recruitment node is not
    // owned.
    f.game.campaign_mut().legacy_features = false;
    liberation_results(f, 1_000);
    let Err(RecruitBlocker::NeedsNode { name, price, .. }) = f.game.recruit_offer("DEU") else {
        panic!("the Recruitment node is missing");
    };
    assert_eq!(name, "Survivor network");
    let line = format!(
        "Learn {name} in Upgrades to recruit from Germany ({} coins)",
        crate::ui::coins::grouped(price)
    );
    let output = settle(&h, f);
    assert_eq!(text_count(&output, &line), 1, "{line}");
    assert!(fully_visible(&output, &line));
    assert_eq!(
        output
            .shapes
            .iter()
            .find_map(|s| text_color(&s.shape, &line)),
        Some(MUTED),
        "the missing node is a muted line, not an action"
    );
    assert!(
        !paints(&output, "Recruit from Germany ·"),
        "no recruit action without the node"
    );

    // With the node owned, any other core blocker is named by its label.
    f.game.campaign_mut().legacy_features = true;
    f.game.campaign_mut().coins = 0;
    let blocker = f.game.recruit_offer("DEU").unwrap_err();
    assert!(
        matches!(blocker, RecruitBlocker::NeedCoins(_)),
        "{blocker:?}"
    );
    let line = format!("Recruit from Germany: {}", blocker.label());
    let output = settle(&h, f);
    assert_eq!(text_count(&output, &line), 1, "{line}");
    assert!(!paints(&output, "Recruit from Germany ·"));

    // A result that liberated nothing shows no recruit block at all.
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let f = &mut fixture.frontend;
    f.game.withdraw();
    f.game.campaign_mut().coins = 1_000;
    f.screen = Screen::Results;
    let output = settle(&h, f);
    assert!(
        !paints(&output, "ecruit from"),
        "a retreat offers no recruit"
    );
}

#[test]
fn liberation_recruit_price_reads_core_at_rank_10() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    liberation_results(f, 1_000);
    f.game.campaign_mut().victories = 10;
    assert_eq!(f.game.campaign().progression_rank(), 10);
    let offer = f.game.recruit_offer("DEU").unwrap();
    assert_eq!(offer.price, 200, "the core price at rank 10");
    let output = settle(&h, f);
    assert_eq!(text_count(&output, &recruit_from(200)), 1);
    assert_eq!(text_count(&output, &recruit_from(100)), 0);
    h.click(f, View::Full, &recruit_from(200));
    let output = h.frame(f, View::Full, vec![]);
    let buttons = offer
        .archetypes
        .map(|c| format!("Recruit as {} · 200 coins", c.archetype.label()));
    for button in &buttons {
        assert_eq!(text_count(&output, button), 1, "{button}");
    }
    assert!(!paints(&output, "· 100 coins"), "no flat 100-coin literal");
    h.click(f, View::Full, &buttons[0]);
    assert_eq!(f.game.strategy().roster.len(), 1, "{}", f.status.message);
    assert_eq!(f.game.campaign().coins, 800, "the command charged 200");

    // The modal follows core: an offer withdrawn while the modal is open
    // reads the blocker and has no recruit button.
    f.status.modal = choose_modal();
    f.game.campaign_mut().coins = 199;
    let blocker = f.game.recruit_offer("DEU").unwrap_err();
    assert_eq!(blocker, RecruitBlocker::NeedCoins(200));
    let output = h.frame(f, View::Full, vec![]);
    assert_eq!(text_count(&output, &blocker.label()), 1);
    assert!(
        !paints(&output, "Recruit as "),
        "a blocked offer has no recruit button"
    );
    assert_eq!(modal_primaries(&h), Vec::<String>::new());
    h.click(f, View::Full, "Close");
    assert_eq!(f.status.modal, None);
}

/// The recruit action is inline content above the action bar, never a
/// fourth action: the bar keeps Upgrade, its continue action and "Leave
/// journey" (W3-03 rewrite: the bar was continue, "More", "Leave journey"
/// above the content; it is now the bottom panel).
#[test]
fn liberation_results_action_bar_still_three_buttons() {
    const BAR: [&str; 3] = ["Upgrade", "Choose next country", "Leave journey"];
    for target in Target::BOTH {
        let mut fixture = IsolatedFrontend::new();
        let h = Harness::at(target);
        let f = &mut fixture.frontend;
        liberation_results(f, 1_000);
        let label = recruit_from(f.game.recruit_offer("DEU").unwrap().price);
        let output = settle(&h, f);
        assert_eq!(
            f.act(|c| super::results::action_bar_labels(c)),
            BAR,
            "{target:?}"
        );
        let bar = BAR.map(|action| {
            assert_eq!(text_count(&output, action), 1, "{target:?}: {action}");
            clipped_bounds(&output, action).unwrap().0
        });
        let bar_top = bar.iter().map(|r| r.top()).fold(f32::MAX, f32::min);
        let (recruit, clip) = clipped_bounds(&output, &label)
            .unwrap_or_else(|| panic!("{target:?}: {label} painted"));
        assert!(
            clip.contains_rect(recruit),
            "{target:?}: {label} is clipped"
        );
        assert!(
            recruit.bottom() <= bar_top,
            "{target:?}: the recruit action sits above the bar ({recruit:?} vs {bar_top})"
        );
        assert_eq!(h.screen_primaries(), ["Choose next country"], "{target:?}");
    }
}

/// Operations' "Recruit survivor" and the Home offer open the same modal;
/// neither recruits by itself any more.
#[test]
fn operations_and_home_offers_open_the_recruit_modal() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    liberation_results(f, 1_000);
    f.game.campaign_mut().attempts = 1;
    let price = f.game.recruit_offer("DEU").unwrap().price;

    f.act(|c| c.open_camp_home(None));
    settle(&h, f);
    assert!(f.screen == Screen::Camp && f.nav.tab == Tab::Home);
    h.click(f, View::Full, "Recruit ›");
    assert_eq!(f.status.modal, choose_modal(), "the Home offer");
    assert!(f.nav.tab == Tab::Home, "Home stays under the modal");
    h.key(f, View::Full, Key::Escape);
    assert_eq!(f.status.modal, None);

    f.campaign.selected_country = "DEU".into();
    f.act(|c| c.go_tab(Tab::Operations));
    settle(&h, f);
    h.click(f, View::Full, &format!("Recruit survivor · {price} coins"));
    assert_eq!(f.status.modal, choose_modal(), "Operations");
    assert!(f.game.strategy().roster.is_empty(), "the modal recruits");
    assert_eq!(f.game.campaign().coins, 1_000);
}
