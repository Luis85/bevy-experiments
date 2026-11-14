use super::model::{CANVAS, Emphasis, Icon, Node, Tone, fade_fill};
use super::navigation::nearest_in_direction;
use super::pictogram::skill_icon;
use super::viewport::{
    Graph, MAX_SCALE, MIN_SCALE, TITLE_ROWS, TITLE_SCALE, View, bounded_pan, caption_job,
    default_scale, minimap_rect, skill_bounds,
};
use super::wires::{
    LONG_EDGE, STUB_LENGTH, WireCache, route_length, skill_obstacle, skill_wire, stub_label,
    wire_segment_clear, wire_stub,
};
use crate::art::{AMBER, TEXT};
use bevy_egui::egui::{self, Align, Rect, pos2, vec2};
use horde_core::content::StructureKind;

#[test]
fn world_nodes_draw_survivors_and_fortification_nodes_draw_their_structure() {
    let catalog = horde_core::skill_catalog::skills();
    for node in &catalog.nodes {
        let icon = skill_icon(&node.icon);
        if node.branch == "Companions and world" {
            assert!(matches!(icon, Icon::Companion), "{}", node.id);
        }
        for (prefix, kind) in [
            ("trap_", StructureKind::Trap),
            ("wall_", StructureKind::Wall),
            ("turret_", StructureKind::Turret),
        ] {
            if node.id.starts_with(prefix) {
                assert!(
                    matches!(icon, Icon::Defense(k, _) if k == kind),
                    "{}",
                    node.id
                );
            }
        }
    }
}
fn catalogue_nodes() -> Vec<Node> {
    let catalog = horde_core::skill_catalog::skills();
    catalog
        .nodes
        .iter()
        .map(|n| Node {
            position: vec2(n.position[0], n.position[1]),
            title: n.name.clone(),
            badge: "Locked".into(),
            tone: Tone::Prerequisite,
            icon: skill_icon(&n.icon),
            prerequisites: n
                .prerequisites
                .iter()
                .map(|id| catalog.nodes.iter().position(|p| p.id == *id).unwrap())
                .collect(),
            counters: String::new(),
        })
        .collect()
}
fn authored_branch(branch: &str) -> Vec<Node> {
    let authored: Vec<_> = horde_core::skill_catalog::skills()
        .nodes
        .iter()
        .filter(|n| n.branch == branch)
        .collect();
    authored
        .iter()
        .map(|n| Node {
            position: vec2(n.position[0], n.position[1]),
            title: n.name.clone(),
            badge: "Locked".into(),
            tone: Tone::Prerequisite,
            icon: skill_icon(&n.icon),
            prerequisites: n
                .prerequisites
                .iter()
                .filter_map(|p| authored.iter().position(|n| &n.id == p))
                .collect(),
            counters: String::new(),
        })
        .collect()
}
#[test]
fn authored_prerequisite_wires_clear_every_unrelated_icon_and_caption() {
    let catalog = horde_core::skill_catalog::skills();
    let branches: std::collections::BTreeSet<_> =
        catalog.nodes.iter().map(|n| n.branch.as_str()).collect();
    for branch in branches {
        let nodes = authored_branch(branch);
        for (child, node) in nodes.iter().enumerate() {
            for &parent in &node.prerequisites {
                let wire = skill_wire(&nodes, parent, child);
                assert!(
                    wire.len() >= 2,
                    "Missing real prerequisite wire: {} -> {}",
                    nodes[parent].title,
                    node.title
                );
                let obstacles: Vec<_> = nodes
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| *i != parent && *i != child)
                    .map(|(_, n)| skill_obstacle(n))
                    .collect();
                for segment in wire.windows(2) {
                    assert!(
                        wire_segment_clear(segment[0], segment[1], &obstacles),
                        "{} -> {} crosses another skill/caption",
                        nodes[parent].title,
                        node.title
                    );
                }
                assert!((wire[0] - nodes[parent].position).length() < 35.0);
                assert!((*wire.last().unwrap() - node.position).length() < 35.0);
            }
        }
    }
}
#[test]
fn every_node_can_be_focused_fully_in_a_compact_canvas_at_the_default_scale() {
    let viewport = vec2(748.0, 160.0);
    let nodes = catalogue_nodes();
    let (bounds, origin) = skill_bounds(&nodes);
    let scale = default_scale(viewport.x);
    assert!(64.0 * scale >= 38.0, "compact icons stay a usable size");
    assert!(scale >= TITLE_SCALE, "compact canvases keep node names");
    let window = Rect::from_min_size(egui::Pos2::ZERO, viewport);
    for node in &nodes {
        let pan = bounded_pan(-(node.position - origin) * scale, viewport, bounds, scale);
        let center = viewport * 0.5 + pan + (node.position - origin) * scale;
        let icon = Rect::from_center_size(pos2(center.x, center.y), vec2(64.0, 64.0) * scale);
        assert!(window.contains_rect(icon), "{} out of view", node.title);
    }
}
#[test]
fn graph_canvas_takes_exactly_the_size_it_is_given_in_a_horizontal_parent() {
    egui::__run_test_ui(|ui| {
        ui.allocate_ui_with_layout(
            vec2(650.0, 360.0),
            egui::Layout::left_to_right(Align::Min),
            |ui| {
                let origin = ui.cursor().min;
                let id = ui.make_persistent_id("horizontal_graph_regression");
                let rect = Graph::skills(&[]).show(
                    ui,
                    id,
                    &mut View::default(),
                    &Emphasis::default(),
                    vec2(640.0, 280.0),
                );
                assert!((rect.left_top() - origin).length() < 1.0);
                assert_eq!(rect.size(), vec2(640.0, 280.0));
            },
        );
    });
}
#[test]
fn zoom_is_bounded_and_keeps_the_point_under_the_cursor() {
    let mut view = View::default();
    view.zoom_by(100.0);
    assert_eq!(view.scale(), MAX_SCALE);
    view.zoom_by(0.0001);
    assert_eq!(view.scale(), MIN_SCALE);
    let mut view = View {
        scale: 1.0,
        pan: vec2(30.0, -10.0),
        ..View::default()
    };
    let anchor = vec2(120.0, 40.0);
    // World point under the anchor, relative to the layout origin.
    let before = (anchor - view.pan) / view.scale;
    view.zoom_at(1.5, anchor);
    let after = (anchor - view.pan) / view.scale;
    assert!((before - after).length() < 1e-3);
    assert_eq!(view.scale(), 1.5);
}
#[test]
fn arrows_pick_the_nearest_node_in_that_direction() {
    let at = |x: f32, y: f32| Node {
        position: vec2(x, y),
        title: String::new(),
        badge: String::new(),
        tone: Tone::Buyable,
        icon: Icon::Firearm,
        prerequisites: vec![],
        counters: String::new(),
    };
    let nodes = vec![
        at(0.0, 0.0),
        at(280.0, 0.0),
        at(280.0, 200.0),
        at(-280.0, 10.0),
        at(0.0, 200.0),
        at(600.0, 0.0),
    ];
    assert_eq!(nearest_in_direction(&nodes, 0, egui::Vec2::RIGHT), Some(1));
    assert_eq!(nearest_in_direction(&nodes, 0, egui::Vec2::LEFT), Some(3));
    assert_eq!(nearest_in_direction(&nodes, 0, egui::Vec2::DOWN), Some(4));
    assert_eq!(nearest_in_direction(&nodes, 0, egui::Vec2::UP), None);
    assert_eq!(nearest_in_direction(&nodes, 1, egui::Vec2::RIGHT), Some(5));
    assert_eq!(nearest_in_direction(&nodes, 4, egui::Vec2::RIGHT), Some(2));
}
#[test]
fn long_cross_graph_edges_become_short_stubs_at_the_child() {
    let nodes = catalogue_nodes();
    let mut cache = WireCache::default();
    let routes = cache.resolve(&nodes);
    let root = horde_core::skill_catalog::skills()
        .nodes
        .iter()
        .position(|n| n.price == 0)
        .unwrap();
    let long: Vec<_> = routes
        .iter()
        .filter(|(_, route)| route_length(route) > LONG_EDGE)
        .collect();
    assert!(
        long.iter().any(|((parent, _), _)| *parent == root),
        "the root's horizon wires are long edges"
    );
    for (_, route) in long {
        let stub = wire_stub(route, STUB_LENGTH);
        assert!((route_length(&stub) - STUB_LENGTH).abs() < 0.5);
        assert_eq!(stub.last(), route.last());
    }
    let short = vec![vec2(0.0, 0.0), vec2(30.0, 0.0)];
    assert_eq!(wire_stub(&short, STUB_LENGTH), short);
    assert_eq!(
        stub_label("Survivor foundations"),
        "from Survivor foundations"
    );
}
#[test]
fn whole_catalogue_routes_every_real_prerequisite_and_reuses_geometry_across_purchase_states() {
    let catalog = horde_core::skill_catalog::skills();
    let mut nodes = catalogue_nodes();
    let mut cache = WireCache::default();
    let first = cache.resolve(&nodes);
    let expected: usize = nodes.iter().map(|n| n.prerequisites.len()).sum();
    assert_eq!(first.len(), expected);
    for ((parent, child), route) in first.iter() {
        assert!(
            route.len() >= 2,
            "Missing authored edge {} to {}",
            catalog.nodes[*parent].id,
            catalog.nodes[*child].id
        );
        let obstacles: Vec<_> = nodes
            .iter()
            .enumerate()
            .filter(|(i, _)| i != parent && i != child)
            .map(|(_, n)| skill_obstacle(n))
            .collect();
        assert!(
            route
                .windows(2)
                .all(|w| wire_segment_clear(w[0], w[1], &obstacles))
        );
    }
    nodes[1].tone = Tone::Owned;
    nodes[2].tone = Tone::Buyable;
    assert!(std::sync::Arc::ptr_eq(&first, &cache.resolve(&nodes)));
    nodes[0].position.x += 5.0;
    assert!(!std::sync::Arc::ptr_eq(&first, &cache.resolve(&nodes)));
}
#[test]
fn default_scale_keeps_a_short_title_and_state_on_distinct_readable_rows() {
    let nodes = vec![Node {
        position: egui::Vec2::ZERO,
        title: "Controlled penetration training".into(),
        badge: "Locked".into(),
        tone: Tone::Prerequisite,
        icon: Icon::Firearm,
        prerequisites: vec![],
        counters: String::new(),
    }];
    let ctx = egui::Context::default();
    crate::settings::install_font(&ctx);
    let mut view = View::default();
    let mut canvas = Rect::NOTHING;
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(360.0, 260.0))),
            ..Default::default()
        },
        |ui| {
            canvas = Graph::skills(&nodes).show(
                ui,
                egui::Id::new("caption-proof"),
                &mut view,
                &Emphasis::default(),
                vec2(340.0, 160.0),
            );
        },
    );
    let mut title = None;
    let mut state = None;
    for clipped in &output.shapes {
        if let egui::Shape::Text(text) = &clipped.shape {
            let rect = text.galley.rect.translate(text.pos.to_vec2());
            if text.galley.text().starts_with("Controlled") {
                assert!(text.galley.job.sections[0].format.font_id.size >= 12.0);
                assert!(rect.width() <= 120.1);
                assert!(canvas.contains_rect(rect));
                title = Some(rect);
            }
            if text.galley.text() == "Locked" {
                assert!(canvas.contains_rect(rect));
                state = Some(rect);
            }
        }
    }
    let title = title.expect("Node name must be painted at the default scale");
    let state = state.expect("Node state must remain separate");
    assert!(title.bottom() + 2.0 <= state.top());
    output.textures_delta.clear();
}
#[test]
fn every_authored_node_name_wraps_to_two_centred_rows_without_eliding() {
    let ctx = egui::Context::default();
    crate::settings::install_font(&ctx);
    let catalog = horde_core::skill_catalog::skills();
    let mut checked = 0;
    let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
        for scale in [TITLE_SCALE, 1.0, MAX_SCALE] {
            for node in &catalog.nodes {
                let job = caption_job(&node.name, scale, TEXT);
                let width = job.wrap.max_width;
                let galley = ui.painter().layout_job(job);
                assert!(!galley.elided, "{} elided at {scale}", node.name);
                assert!(galley.rows.len() <= TITLE_ROWS, "{}", node.name);
                assert!(galley.rect.width() <= width + 0.1, "{}", node.name);
                assert!(
                    galley.rect.center().x.abs() <= 1.0,
                    "{} not centred at {scale}: {:?}",
                    node.name,
                    galley.rect
                );
                checked += 1;
            }
        }
    });
    output.textures_delta.clear();
    // egui may run a pass more than once.
    assert!(checked > 0 && checked % (catalog.nodes.len() * 3) == 0);
}
#[test]
fn faded_fills_stay_opaque_and_blend_toward_the_canvas() {
    assert_eq!(fade_fill(AMBER, 1.0), AMBER);
    assert_eq!(fade_fill(AMBER, 0.0), CANVAS);
    assert_eq!(fade_fill(AMBER, 0.5).a(), 255);
    assert!(
        minimap_rect(
            Rect::from_min_size(pos2(0.0, 0.0), vec2(100.0, 50.0)),
            vec2(6000.0, 4000.0)
        )
        .is_none()
    );
    let mini = minimap_rect(
        Rect::from_min_size(pos2(0.0, 0.0), vec2(1000.0, 600.0)),
        vec2(6640.0, 4730.0),
    )
    .unwrap();
    assert!(mini.width() <= 190.0 && mini.right() < 1000.0 && mini.bottom() < 600.0);
}

#[test]
fn the_minimap_keeps_most_of_the_canvas_corner_free() {
    let bounds = vec2(6640.0, 4730.0);
    // Desktop canvas and the compact canvas at 1050x700 @ 1.35.
    for canvas in [vec2(1040.0, 600.0), vec2(740.0, 300.0)] {
        let canvas = Rect::from_min_size(pos2(0.0, 0.0), canvas);
        let mini = minimap_rect(canvas, bounds).unwrap();
        let share = mini.area() / canvas.area();
        assert!(share <= 0.04, "{canvas:?}: minimap covers {share:.3}");
        assert!(canvas.contains_rect(mini));
    }
}

#[test]
fn every_body_attribute_draws_its_own_pictogram() {
    use std::collections::BTreeMap;
    let catalog = horde_core::skill_catalog::skills();
    let mut by_attribute: BTreeMap<String, std::collections::BTreeSet<String>> = BTreeMap::new();
    for node in catalog
        .nodes
        .iter()
        .filter(|n| n.branch == "Body development")
    {
        let attribute = node.name.rsplit_once(' ').map_or("", |(name, _)| name);
        by_attribute
            .entry(attribute.to_owned())
            .or_default()
            .insert(node.icon.clone());
    }
    assert_eq!(by_attribute.len(), 5, "{by_attribute:?}");
    let mut seen = std::collections::BTreeSet::new();
    for (attribute, icons) in &by_attribute {
        assert_eq!(
            icons.len(),
            1,
            "{attribute} ranks share one icon: {icons:?}"
        );
        let icon = icons.first().unwrap();
        assert!(
            !matches!(skill_icon(icon), Icon::Firearm),
            "{attribute} draws the default pistol ({icon})"
        );
        assert!(seen.insert(icon.clone()), "{attribute} repeats {icon}");
    }
}
/// CF-2-02 (and the label half of CF-2-01): at every zoom that draws stub
/// labels, from the 1050x700@1.35 compact default (0.6) through the
/// 1440x900 default to the maximum, each "from ..." label of a cross-graph
/// stub meets no node's icon and caption envelope and no other drawn wire.
/// Before, the stub into Reload drill ran "from Survivor foundations" under
/// the Steady hands icon.
#[test]
fn every_stub_label_clears_every_node_envelope_and_other_wire_at_every_zoom() {
    let nodes = catalogue_nodes();
    let (_, origin) = skill_bounds(&nodes);
    let routes = WireCache::default().resolve(&nodes);
    let ctx = egui::Context::default();
    crate::settings::install_font(&ctx);
    for scale in [TITLE_SCALE, 0.6, 0.72, 0.84, 1.0, 1.25, MAX_SCALE] {
        let mut view = View {
            scale,
            ..View::default()
        };
        let mut canvas = Rect::NOTHING;
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1500.0, 900.0))),
                ..Default::default()
            },
            |ui| {
                canvas = Graph::skills(&nodes).show(
                    ui,
                    egui::Id::new("stub-label-proof"),
                    &mut view,
                    &Emphasis::default(),
                    vec2(1400.0, 800.0),
                );
            },
        );
        output.textures_delta.clear();
        let center = canvas.center() + view.pan;
        let point = |v: egui::Vec2| center + (v - origin) * scale;
        let envelopes: Vec<_> = nodes
            .iter()
            .map(|n| {
                let world = skill_obstacle(n);
                (
                    n,
                    Rect::from_two_pos(point(world.min.to_vec2()), point(world.max.to_vec2())),
                )
            })
            .collect();
        // Every drawn wire as screen segments, by its (parent, child).
        let wires: Vec<_> = routes
            .iter()
            .filter(|(_, route)| route.len() >= 2)
            .map(|(&edge, route)| {
                let world = if route_length(route) > LONG_EDGE {
                    wire_stub(route, STUB_LENGTH)
                } else {
                    route.clone()
                };
                (edge, world.into_iter().map(point).collect::<Vec<_>>())
            })
            .collect();
        let crosses = |rect: Rect, a: egui::Pos2, b: egui::Pos2| {
            let span = Rect::from_two_pos(a, b);
            span.min.x < rect.max.x
                && rect.min.x < span.max.x
                && span.min.y < rect.max.y
                && rect.min.y < span.max.y
        };
        let mut labels = 0;
        for clipped in &output.shapes {
            let egui::Shape::Text(text) = &clipped.shape else {
                continue;
            };
            let Some(parent_title) = text.galley.text().strip_prefix("from ") else {
                continue;
            };
            labels += 1;
            let label = text.galley.rect.translate(text.pos.to_vec2());
            for (node, envelope) in &envelopes {
                assert!(
                    !label.intersects(*envelope),
                    "scale {scale}: the label {:?} at {label:?} meets {}'s envelope {envelope:?}",
                    text.galley.text(),
                    node.title
                );
            }
            // Its own stub is the nearest stub from that parent, within 40
            // points; every other wire stays clear.
            let gap = |points: &[egui::Pos2]| {
                points
                    .windows(2)
                    .map(|pair| {
                        let span = Rect::from_two_pos(pair[0], pair[1]);
                        let dx = (span.min.x - label.max.x).max(label.min.x - span.max.x);
                        let dy = (span.min.y - label.max.y).max(label.min.y - span.max.y);
                        dx.max(dy).max(0.0)
                    })
                    .fold(f32::INFINITY, f32::min)
            };
            let own = wires
                .iter()
                .filter(|(edge, _)| {
                    route_length(&routes[edge]) > LONG_EDGE
                        && nodes[edge.0]
                            .title
                            .starts_with(parent_title.trim_end_matches('\u{2026}'))
                })
                .map(|(edge, points)| (*edge, gap(points)))
                .min_by(|a, b| a.1.total_cmp(&b.1));
            let own = match own {
                Some((edge, gap)) if gap <= 40.0 => edge,
                _ => panic!(
                    "scale {scale}: the label {:?} at {label:?} lies by no stub of its parent",
                    text.galley.text()
                ),
            };
            for (edge, points) in &wires {
                if *edge == own {
                    continue;
                }
                for pair in points.windows(2) {
                    assert!(
                        !crosses(label, pair[0], pair[1]),
                        "scale {scale}: the label {:?} at {label:?} crosses the wire {} -> {}",
                        text.galley.text(),
                        nodes[edge.0].title,
                        nodes[edge.1].title
                    );
                }
            }
        }
        assert!(labels > 0, "scale {scale}: stub labels are drawn");
    }
}
