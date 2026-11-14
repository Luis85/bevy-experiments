//! Egui harness tests: the Journey upgrades skill tree (selection, purchase,
//! search, keyboard and compact layout).
use super::*;
use horde_core::campaign::NodePayment;
use horde_core::skill_catalog::{search_nodes, skills};

fn upgrade_home() -> IsolatedFrontend {
    let mut fixture = IsolatedFrontend::new();
    let f = &mut fixture.frontend;
    f.game.campaign_mut().choose_start_country("DEU").unwrap();
    f.screen = Screen::Results;
    f.campaign.improvement_focus = None;
    fixture
}
fn selected(h: &Harness) -> String {
    progression::selected_node_for_test(&h.ctx).expect("tree drawn")
}
fn learn_label(f: &FrontendParts, id: &str) -> String {
    match f.game.campaign().node_payment(id).unwrap() {
        NodePayment::Coins(price) => format!("Learn skill · {price} coins"),
        NodePayment::LegacyBodyPoints(points) => format!("Learn skill · {points} legacy CP"),
    }
}
fn position(id: &str) -> Vec2 {
    let node = skills().node(id).unwrap();
    vec2(node.position[0], node.position[1])
}

#[test]
fn the_tree_opens_on_the_recommended_node_not_the_owned_root() {
    let mut fixture = upgrade_home();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.game.campaign_mut().coins = 60;
    let output = h.frame(f, View::Results, vec![]);
    let root = skills().nodes.iter().find(|n| n.price == 0).unwrap();
    let chosen = selected(&h);
    assert_ne!(chosen, root.id);
    assert_eq!(
        Some(chosen),
        f.game.campaign().recommended_node(f.campaign.mode)
    );
    // Catalogue stepping is gone; one jump to the recommendation replaces it.
    for gone in ["Previous", "Next", "Focus", "1 / 169"] {
        assert_eq!(text_count(&output, gone), 0, "{gone} removed");
    }
    assert_eq!(text_count(&output, "Recommended"), 1);
    assert_eq!(text_count(&output, "Path to…"), 1);
}

#[test]
fn learn_dispatches_the_purchase_keeps_the_selection_and_shows_the_main_delta() {
    let mut fixture = upgrade_home();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.game.campaign_mut().coins = 500;
    h.frame(f, View::Results, vec![]);
    let id = selected(&h);
    assert!(f.game.campaign().can_purchase_node(&id).is_ok());
    let node = skills().node(&id).unwrap();
    let line = progression::learned_line(f.game.campaign(), node);
    assert!(line.starts_with(&format!("Learned {}", node.name)));
    let coins = f.game.campaign().coins;
    let label = learn_label(f, &id);
    f.cues.clear();
    h.click(f, View::Results, &label);
    assert!(f.game.campaign().has_node(&id), "BuySkillNode applied");
    assert!(
        f.cues.contains(&crate::audio::Cue::Confirm),
        "purchase cue from the existing bank: {:?}",
        f.cues
    );
    assert_eq!(coins - f.game.campaign().coins, node.price);
    let output = h.frame(f, View::Results, vec![]);
    assert_eq!(selected(&h), id, "selection stays after learning");
    assert_eq!(text_count(&output, &line), 1, "status line with the delta");
    assert_eq!(text_count(&output, "Owned"), 1, "state chip now owned");
    assert_eq!(text_count(&output, &label), 0, "no second purchase offered");
    // Home moves on to the next recommendation.
    h.key(f, View::Results, Key::Home);
    h.frame(f, View::Results, vec![]);
    assert_eq!(
        Some(selected(&h)),
        f.game.campaign().recommended_node(f.campaign.mode)
    );
}

#[test]
fn a_locked_selection_shows_the_core_reason_under_a_disabled_learn_button() {
    let mut fixture = upgrade_home();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.game.campaign_mut().coins = 0;
    f.game.campaign_mut().player.points = 0;
    h.frame(f, View::Results, vec![]);
    let output = h.frame(f, View::Results, vec![]);
    let id = selected(&h);
    let reason = f
        .game
        .campaign()
        .can_purchase_node(&id)
        .expect_err("nothing is affordable with no coins");
    assert_eq!(
        text_count(&output, &reason),
        1,
        "reason under the button: {reason}"
    );
    let label = learn_label(f, &id);
    let before = serde_json::to_value(&f.game).unwrap();
    h.click(f, View::Results, &label);
    assert_eq!(serde_json::to_value(&f.game).unwrap(), before);
}

#[test]
fn search_selects_a_match_and_dims_everything_else() {
    let mut fixture = upgrade_home();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    h.frame(f, View::Results, vec![]);
    h.click(f, View::Results, "Search upgrades");
    h.frame(f, View::Results, vec![Event::Text("grenade".into())]);
    let output = h.frame(f, View::Results, vec![]);
    let matches: Vec<_> = search_nodes("grenade")
        .iter()
        .map(|n| n.id.clone())
        .collect();
    assert!(matches.len() > 1);
    assert_eq!(
        text_count(&output, &format!("{} matches", matches.len())),
        1
    );
    let chosen = selected(&h);
    assert!(matches.contains(&chosen), "selection jumps to a match");
    let alpha = |name: &str| {
        output
            .shapes
            .iter()
            .find_map(|s| text_color(&s.shape, name))
            .map(|c| c.a())
    };
    let lit = alpha(&skills().node(&chosen).unwrap().name).expect("match painted");
    assert!(lit > 200, "matches stay bright ({lit})");
    let dimmed: Vec<u8> = skills()
        .nodes
        .iter()
        .filter(|n| !matches.contains(&n.id))
        .filter_map(|n| alpha(&n.name))
        .collect();
    assert!(!dimmed.is_empty(), "some non-matching node is on screen");
    assert!(
        dimmed.iter().all(|a| *a < 90),
        "non-matches dim: {dimmed:?}"
    );
}

#[test]
fn arrow_keys_move_to_the_nearest_node_in_that_direction() {
    let mut fixture = upgrade_home();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.game.campaign_mut().coins = 60;
    h.frame(f, View::Results, vec![]);
    let start = selected(&h);
    for (key, check) in [
        (Key::ArrowRight, (|d: Vec2| d.x > 1.0) as fn(Vec2) -> bool),
        (Key::ArrowDown, |d: Vec2| d.y > 1.0),
        (Key::ArrowLeft, |d: Vec2| d.x < -1.0),
        (Key::ArrowUp, |d: Vec2| d.y < -1.0),
    ] {
        let from = selected(&h);
        let reachable = skills()
            .nodes
            .iter()
            .any(|n| check(vec2(n.position[0], n.position[1]) - position(&from)));
        h.key(f, View::Results, key);
        h.frame(f, View::Results, vec![]);
        let to = selected(&h);
        if reachable {
            assert_ne!(to, from, "{key:?} moves");
            assert!(check(position(&to) - position(&from)), "{key:?} direction");
        } else {
            assert_eq!(to, from);
        }
    }
    h.key(f, View::Results, Key::Home);
    h.frame(f, View::Results, vec![]);
    assert_eq!(selected(&h), start, "Home returns to the recommendation");
}

#[test]
fn an_explicit_fortification_focus_selects_that_branch() {
    let mut fixture = upgrade_home();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    let first = skills()
        .nodes
        .iter()
        .find(|n| n.branch == "Fortification")
        .unwrap();
    f.campaign.improvement_focus = Some(horde_core::campaign::RecommendationAction::LearnNode(
        first.id.clone(),
    ));
    h.frame(f, View::Results, vec![]);
    let chosen = selected(&h);
    assert_eq!(skills().node(&chosen).unwrap().branch, "Fortification");
    assert!(!f.game.campaign().has_node(&chosen));
}

#[test]
fn the_compact_layout_keeps_at_least_half_the_tree_for_the_canvas() {
    let mut fixture = upgrade_home();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.game.campaign_mut().coins = 200;
    let wide = {
        h.frame(f, View::Results, vec![]);
        progression::layout_for_test(&h.ctx).unwrap()
    };
    assert!(!wide.compact);
    settings::configure(&h.ctx, 1.35);
    f.settings.scale = 1.35;
    // The harness screen is in points: 1050×700 physical pixels at 1.35.
    let size = vec2(1050.0, 700.0) / 1.35;
    h.frame_size(f, View::Results, vec![], size);
    let output = h.frame_size(f, View::Results, vec![], size);
    let layout = progression::layout_for_test(&h.ctx).unwrap();
    assert!(layout.compact);
    assert!(
        layout.canvas.height() >= layout.region.height() * 0.5,
        "canvas {} of {}",
        layout.canvas.height(),
        layout.region.height()
    );
    assert!((layout.canvas.width() - layout.region.width()).abs() < 1.0);
    // The detail panel and its one primary action sit below the canvas.
    let id = selected(&h);
    let learn = output
        .shapes
        .iter()
        .find_map(|s| labeled_bounds(&s.shape, &learn_label(f, &id)))
        .expect("Learn button painted");
    assert!(learn.top() > layout.canvas.bottom());
    assert!(h.ctx.content_rect().contains_rect(learn));
}

/// W3-22: a node authored against enemy archetypes names them, by their
/// catalogue names, in its canvas tooltip and in the detail panel; a node
/// with no counters says nothing. The perk's class stats are previewed.
#[test]
fn counter_node_tooltip_names_its_archetypes() {
    use horde_core::enemy_catalog::enemy_catalog;
    let mut fixture = upgrade_home();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    let names = |node: &horde_core::skill_catalog::SkillNode| -> Vec<String> {
        node.counters
            .iter()
            .map(|id| {
                enemy_catalog()
                    .resolve(id)
                    .expect("catalogue archetype")
                    .name
            })
            .collect()
    };
    let counter_nodes: Vec<_> = skills()
        .nodes
        .iter()
        .filter(|n| !n.counters.is_empty())
        .collect();
    assert!(counter_nodes.len() >= 6, "the decision-19 counter nodes");
    for node in &counter_nodes {
        let tooltip = progression::tooltip_for_test(f.game.campaign(), &node.id);
        let line = format!("Counters: {}", names(node).join(", "));
        assert_eq!(
            tooltip.lines().nth(1),
            Some(line.as_str()),
            "{}: {tooltip}",
            node.id
        );
        assert_eq!(tooltip.lines().count(), 3, "{}", node.id);
    }
    let plain = progression::tooltip_for_test(f.game.campaign(), "steady_hands");
    assert!(!plain.contains("Counters"), "{plain}");
    assert_eq!(plain.lines().count(), 2);

    // The detail panel of a selected counter node carries the same line and
    // the preview rows of its perk.
    let node = skills().node("shotgun_pump_drill").unwrap();
    f.campaign.improvement_focus = Some(horde_core::campaign::RecommendationAction::LearnNode(
        node.id.clone(),
    ));
    h.frame(f, View::Results, vec![]);
    let output = h.frame(f, View::Results, vec![]);
    assert_eq!(selected(&h), node.id);
    let line = format!("Counters: {}", names(node).join(", "));
    assert_eq!(text_count(&output, &line), 1, "{line}");
    let preview = f.game.campaign().node_stat_preview(&node.id);
    assert_eq!(preview.len(), 2, "cadence and reload");
    for delta in &preview {
        assert_eq!(
            text_count(&output, &delta.stat.label()),
            1,
            "{}",
            delta.stat.label()
        );
    }
}
