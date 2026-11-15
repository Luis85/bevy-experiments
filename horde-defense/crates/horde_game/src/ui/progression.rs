//! One connected personal progression catalogue. Every rule (state, path,
//! preview, recommendation, search) is a core query; this screen only renders
//! them, and purchases go through `Command::BuySkillNode`.
use super::{UiCtx, coins::grouped, muted};
use crate::art::{AMBER, GREEN, INK, MUTED, TEXT};
use bevy_egui::egui::{self, Align, Key, Layout, Modifiers, Rect, RichText, Sense, Stroke, vec2};
use horde_core::campaign::{
    Campaign, NodePayment, NodeState, PreviewStat, StatDelta, counter_names,
};
use horde_core::command::Command;
use horde_core::skill_catalog::{SkillEffect, SkillNode, search_nodes, skills, upgrade_goals};
use std::collections::BTreeSet;
mod upgrade_graph;
use upgrade_graph::{Emphasis, Graph, MUTED_RED, Node, Tone, View, nearest_in_direction};

/// Fixed so tests and redirects find the one tree's view state.
const TREE_ID: &str = "journey_complete_skill_graph";
/// Seconds an amber pulse marks a node just learned.
const PULSE_SECONDS: f64 = 0.7;

/// View state of the tree: zoom, pan, selection, search text and purchase
/// feedback. UI only; ownership lives in `Campaign`.
#[derive(Clone, Default)]
struct TreeState {
    view: View,
    search: String,
    /// The explicit improvement focus last applied, so it selects once.
    focus_key: Option<String>,
    last_frame: Option<u64>,
    pending: Option<Pending>,
    pulse: Option<(usize, f64)>,
    learned: Option<(usize, String)>,
}
/// A purchase sent this session that has not yet shown up as owned.
#[derive(Clone)]
struct Pending {
    index: usize,
    line: String,
    frame: u64,
}

/// Where the canvas landed inside the tree region, for layout checks.
#[derive(Clone, Copy, Debug)]
#[cfg_attr(not(test), allow(dead_code))]
pub(super) struct TreeLayout {
    pub canvas: Rect,
    pub region: Rect,
    pub compact: bool,
}

#[cfg(test)]
pub(super) fn selected_node_for_test(ctx: &egui::Context) -> Option<String> {
    ctx.data(|d| d.get_temp::<TreeState>(egui::Id::new(TREE_ID)))
        .map(|state| skills().nodes[state.view.selected].id.clone())
}
/// The canvas hover text of node `id` in `campaign`.
#[cfg(test)]
pub(super) fn tooltip_for_test(campaign: &Campaign, id: &str) -> String {
    catalogue_nodes_for(campaign)[index_of(id).expect("catalogue node")].tooltip()
}
#[cfg(test)]
pub(super) fn layout_for_test(ctx: &egui::Context) -> Option<TreeLayout> {
    ctx.data(|d| d.get_temp::<TreeLayout>(egui::Id::new(TREE_ID).with("layout")))
}

fn short_price(payment: NodePayment) -> String {
    match payment {
        NodePayment::LegacyBodyPoints(points) => format!("{points} CP"),
        NodePayment::Coins(price) => format!("{} c", grouped(price)),
    }
}
/// The canvas caption and tone for a core node state.
fn badge(state: &NodeState) -> (String, Tone) {
    match state {
        NodeState::Owned => ("OWNED".into(), Tone::Owned),
        NodeState::Buyable(payment) => (short_price(*payment), Tone::Buyable),
        NodeState::NeedsCoins { payment, short } => (
            format!("{} · need {}", short_price(*payment), grouped(*short)),
            Tone::NeedsCoins,
        ),
        NodeState::NeedsPrerequisites { .. } => ("Locked".into(), Tone::Prerequisite),
        NodeState::NeedsMilestone {
            victories,
            liberated,
        } => (
            match (liberated, victories) {
                (Some((_, need)), _) => format!("Free {need}"),
                (None, Some((_, need))) => format!("Win {need}"),
                (None, None) => "Locked".into(),
            },
            Tone::Milestone,
        ),
        NodeState::Unavailable { .. } => ("Unavailable".into(), Tone::Unavailable),
    }
}
fn node_name(id: &str) -> &str {
    skills().node(id).map_or(id, |n| n.name.as_str())
}
/// The detail panel's state chip: the same states as the canvas, in words.
fn state_chip(state: &NodeState) -> (String, egui::Color32) {
    match state {
        NodeState::Owned => ("Owned".into(), AMBER),
        NodeState::Buyable(_) => ("Ready to learn".into(), TEXT),
        NodeState::NeedsCoins { short, .. } => {
            (format!("Short {} coins", grouped(*short)), MUTED_RED)
        }
        NodeState::NeedsPrerequisites { missing } => (
            match missing.as_slice() {
                [] => "Locked".into(),
                [one] => format!("Locked · learn {} first", node_name(one)),
                [one, rest @ ..] => {
                    format!("Locked · learn {} and {} more", node_name(one), rest.len())
                }
            },
            MUTED,
        ),
        NodeState::NeedsMilestone {
            victories,
            liberated,
        } => (
            match (liberated, victories) {
                (Some((have, need)), _) => {
                    format!("Locked · free {need} countries ({have}/{need})")
                }
                (None, Some((have, need))) => {
                    format!("Locked · win {need} defenses ({have}/{need})")
                }
                (None, None) => "Locked".into(),
            },
            MUTED,
        ),
        NodeState::Unavailable { reason } => (reason.clone(), MUTED),
    }
}

fn catalogue_nodes_for(campaign: &Campaign) -> Vec<Node> {
    let catalog = skills();
    catalog
        .nodes
        .iter()
        .map(|node| {
            let (badge, tone) = badge(&campaign.node_state(&node.id));
            Node {
                position: vec2(node.position[0], node.position[1]),
                title: node.name.clone(),
                badge,
                tone,
                icon: upgrade_graph::skill_icon(&node.icon),
                prerequisites: node
                    .prerequisites
                    .iter()
                    .filter_map(|id| catalog.nodes.iter().position(|p| p.id == *id))
                    .collect(),
                counters: counters_line(node).unwrap_or_default(),
            }
        })
        .collect()
}
/// "Counters: Shield-bearer, Juggernaut": the enemy archetypes the node is
/// authored against, by their core catalogue names. None without counters.
fn counters_line(node: &SkillNode) -> Option<String> {
    let names = counter_names(node);
    (!names.is_empty()).then(|| format!("Counters: {}", names.join(", ")))
}
fn index_of(id: &str) -> Option<usize> {
    skills().nodes.iter().position(|n| n.id == id)
}
/// Branches in catalogue order of first appearance.
fn branches() -> Vec<&'static str> {
    let mut seen = Vec::new();
    for node in &skills().nodes {
        if !seen.contains(&node.branch.as_str()) {
            seen.push(node.branch.as_str());
        }
    }
    seen
}
/// The world-space rectangle spanned by a branch's node centres.
fn branch_rect(branch: &str) -> Rect {
    let mut points = skills()
        .nodes
        .iter()
        .filter(|n| n.branch == branch)
        .map(|n| egui::pos2(n.position[0], n.position[1]));
    let first = points.next().unwrap_or(egui::Pos2::ZERO);
    points.fold(Rect::from_min_max(first, first), |r, p| {
        r.union(Rect::from_min_max(p, p))
    })
}
/// The node an explicit improvement focus names (Fortifications, a
/// recommendation card), owned or not.
fn improvement_node(f: &UiCtx) -> Option<String> {
    match f.campaign.improvement_focus.as_ref()? {
        horde_core::campaign::RecommendationAction::LearnSkill(s) => Some(s.id().to_owned()),
        horde_core::campaign::RecommendationAction::LearnNode(id) => Some(id.clone()),
        _ => None,
    }
}
/// The most useful node of a branch: buyable, then short of coins, then any
/// unowned node. `None` when the branch is complete.
fn branch_pick(branch: &str, nodes: &[Node]) -> Option<usize> {
    let members: Vec<usize> = (0..nodes.len())
        .filter(|&i| skills().nodes[i].branch == branch)
        .collect();
    let pick = |tone: Tone| members.iter().copied().find(|&i| nodes[i].tone == tone);
    pick(Tone::Buyable)
        .or_else(|| pick(Tone::NeedsCoins))
        .or_else(|| {
            members
                .iter()
                .copied()
                .find(|&i| nodes[i].tone != Tone::Owned)
        })
}
/// Where a focus lands: an unowned focus node itself; an owned one stands for
/// its branch; with no focus (or a complete branch), the recommendation.
fn focus_target(f: &UiCtx, focus: Option<&str>, nodes: &[Node]) -> Option<usize> {
    focus
        .and_then(|id| {
            let index = index_of(id)?;
            if nodes[index].tone == Tone::Owned {
                branch_pick(&skills().nodes[index].branch, nodes)
            } else {
                Some(index)
            }
        })
        .or_else(|| recommended_index(f))
}
fn recommended_index(f: &UiCtx) -> Option<usize> {
    f.game
        .campaign()
        .recommended_node(f.campaign.mode)
        .and_then(|id| index_of(&id))
}
/// "Max health now 135 (was 100)" or "Unlocks Shotgun proficiency". The pixel
/// font has no arrow glyph, so the core summary's arrow is spelled out.
fn delta_line(delta: &StatDelta) -> String {
    match &delta.stat {
        PreviewStat::Unlock(name) => format!("Unlocks {name}"),
        stat => format!(
            "{} now {} (was {})",
            stat.label(),
            stat.format(delta.after),
            stat.format(delta.before)
        ),
    }
}
/// "Learned Field fitness · Max health now 135 (was 100)": the main delta.
pub(super) fn learned_line(campaign: &Campaign, node: &SkillNode) -> String {
    match campaign.node_stat_preview(&node.id).first() {
        Some(delta) => format!("Learned {} · {}", node.name, delta_line(delta)),
        None => format!("Learned {}", node.name),
    }
}
/// A small painted arrow for before/after rows.
/// The "What changes" rows, laid out to the detail column's width (CF-3-17).
/// While every row fits, one aligned grid (stat, before, arrow, after). A
/// stat label too long for that (Pump drill's "Shotgun time between shots")
/// grew the grid past the column and cut the after-value off at the window
/// edge, so the rows then stack: the stat on its own wrapping line, and
/// "before → after" indented under it.
fn what_changes(ui: &mut egui::Ui, id: &str, preview: &[StatDelta]) {
    const SPACING: egui::Vec2 = vec2(8.0, 3.0);
    let font = egui::TextStyle::Body.resolve(ui.style());
    let width = |text: &str| {
        ui.painter()
            .layout_no_wrap(text.to_owned(), font.clone(), egui::Color32::PLACEHOLDER)
            .size()
            .x
    };
    // The grid's column widths: stat, before (or the unlocked name), after.
    let mut columns = [0.0f32; 3];
    for delta in preview {
        let cells = match &delta.stat {
            PreviewStat::Unlock(name) => [width("Unlocks"), width(name), 0.0],
            stat => [
                width(&stat.label()),
                width(&stat.format(delta.before)),
                width(&stat.format(delta.after)),
            ],
        };
        for (column, cell) in columns.iter_mut().zip(cells) {
            *column = column.max(cell);
        }
    }
    let row = columns.iter().sum::<f32>() + ARROW_WIDTH + 3.0 * SPACING.x;
    if row <= ui.available_width() {
        egui::Grid::new(("skill-changes", id))
            .num_columns(4)
            .min_col_width(0.0)
            .spacing(SPACING)
            .show(ui, |ui| {
                for delta in preview {
                    match &delta.stat {
                        PreviewStat::Unlock(name) => {
                            ui.label("Unlocks");
                            ui.colored_label(AMBER, name);
                        }
                        stat => {
                            ui.label(stat.label());
                            ui.colored_label(MUTED, stat.format(delta.before));
                            arrow(ui);
                            ui.colored_label(AMBER, stat.format(delta.after));
                        }
                    }
                    ui.end_row();
                }
            });
        return;
    }
    for delta in preview {
        match &delta.stat {
            PreviewStat::Unlock(name) => {
                ui.label("Unlocks");
                ui.horizontal(|ui| {
                    ui.add_space(12.0);
                    ui.add(egui::Label::new(RichText::new(name).color(AMBER)).wrap());
                });
            }
            stat => {
                ui.add(egui::Label::new(stat.label()).wrap());
                ui.horizontal(|ui| {
                    ui.add_space(12.0);
                    ui.colored_label(MUTED, stat.format(delta.before));
                    arrow(ui);
                    ui.colored_label(AMBER, stat.format(delta.after));
                });
            }
        }
        ui.add_space(SPACING.y);
    }
}
/// The drawn width of [`arrow`].
const ARROW_WIDTH: f32 = 16.0;
fn arrow(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(vec2(ARROW_WIDTH, 12.0), Sense::hover());
    let stroke = Stroke::new(1.5, MUTED);
    let (a, b) = (
        rect.left_center() + vec2(2.0, 0.0),
        rect.right_center() - vec2(2.0, 0.0),
    );
    ui.painter().line_segment([a, b], stroke);
    ui.painter().line_segment([b, b + vec2(-4.0, -4.0)], stroke);
    ui.painter().line_segment([b, b + vec2(-4.0, 4.0)], stroke);
}
fn learn(f: &mut UiCtx, state: &mut TreeState, index: usize, frame: u64) {
    let node = &skills().nodes[index];
    let line = learned_line(f.game.campaign(), node);
    f.dispatch(Command::BuySkillNode(node.id.clone()), &line);
    state.pending = Some(Pending { index, line, frame });
}

pub(super) fn skill_tree(ui: &mut egui::Ui, f: &mut UiCtx, height: f32) {
    let catalog = skills();
    let id = egui::Id::new(TREE_ID);
    let mut state = ui
        .ctx()
        .data_mut(|d| d.get_temp::<TreeState>(id).unwrap_or_default());
    if let Some(seed) = f.campaign.tree_search_seed.take() {
        state.search = seed;
    }
    let frame = ui.ctx().cumulative_frame_nr();
    let now = ui.input(|i| i.time);
    // Re-entering the screen (or a new explicit focus) selects the next useful
    // node, never the owned root. Learning keeps the selection where it is.
    let reentered = state.last_frame.is_none_or(|last| frame > last + 1);
    state.last_frame = Some(frame);
    let nodes = catalogue_nodes_for(f.game.campaign());
    let focus = improvement_node(f);
    if reentered || (focus.is_some() && focus != state.focus_key) {
        if let Some(index) = focus_target(f, focus.as_deref(), &nodes) {
            state.view.focus(index);
        }
        state.focus_key = focus;
    }
    if let Some([x, y]) = f.campaign.tree_centre_seed.take() {
        state.view.focus_on_point(vec2(x, y));
    }
    state.view.selected = state.view.selected.min(catalog.nodes.len() - 1);
    if let Some(pending) = state.pending.take() {
        if f.game
            .campaign()
            .node_state(&catalog.nodes[pending.index].id)
            == NodeState::Owned
        {
            state.pulse = Some((pending.index, now));
            state.learned = Some((pending.index, pending.line));
        } else if frame <= pending.frame + 30 {
            state.pending = Some(pending);
        }
    }
    let pulse = state.pulse.and_then(|(index, start)| {
        let phase = ((now - start) / PULSE_SECONDS) as f32;
        (!f.settings.reduced_effects && (0.0..1.0).contains(&phase)).then_some((index, phase))
    });
    if pulse.is_some() {
        ui.ctx().request_repaint();
    }
    let width = ui.available_width();
    let compact = width < 900.0 || height < 430.0;
    let region = Rect::from_min_size(ui.cursor().min, vec2(width, height));

    let search = toolbar(ui, f, &mut state, &nodes, compact);
    let matches: Option<BTreeSet<usize>> = (!state.search.trim().is_empty()).then(|| {
        search_nodes(&state.search)
            .iter()
            .filter_map(|n| index_of(&n.id))
            .collect()
    });
    if let Some(matches) = &matches {
        let selected = state.view.selected;
        let next = if search.cycle {
            matches.range(selected + 1..).next().or(matches.first())
        } else if search.changed && !matches.contains(&selected) {
            matches.first()
        } else {
            None
        };
        if let Some(&next) = next {
            state.view.focus(next);
        }
    }
    keyboard(ui, f, &mut state, &nodes, frame);

    let selected = state.view.selected;
    let emphasis = Emphasis {
        path: f
            .game
            .campaign()
            .path_to_node(&catalog.nodes[selected].id)
            .steps
            .iter()
            .filter_map(|id| index_of(id))
            .collect(),
        matches,
        pulse,
    };
    let graph = Graph::skills(&nodes);
    let body_height = (region.bottom() - ui.cursor().top()).max(150.0);
    let mut action = None;
    let canvas = if compact {
        // The detail goes below so the canvas keeps more than half the height.
        let canvas_height = (body_height * 0.62).max(120.0);
        let canvas = graph.show(
            ui,
            id,
            &mut state.view,
            &emphasis,
            vec2(width, canvas_height),
        );
        ui.add_space(4.0);
        let detail_height = (body_height - canvas_height - 8.0).max(60.0);
        ui.allocate_ui_with_layout(
            vec2(width, detail_height),
            Layout::top_down(Align::Min),
            |ui| {
                action = skill_detail(ui, f, &state, selected, true, detail_height);
            },
        );
        canvas
    } else {
        let detail_width = if width >= 1000.0 { 310.0 } else { 260.0 };
        let graph_width = (width - detail_width - 18.0).max(180.0);
        ui.horizontal_top(|ui| {
            let canvas = graph.show(
                ui,
                id,
                &mut state.view,
                &emphasis,
                vec2(graph_width, body_height),
            );
            ui.separator();
            ui.allocate_ui_with_layout(
                vec2(detail_width, body_height),
                Layout::top_down(Align::Min),
                |ui| {
                    ui.set_width(detail_width);
                    action = skill_detail(ui, f, &state, selected, false, body_height);
                },
            );
            canvas
        })
        .inner
    };
    match action {
        Some(DetailAction::Select(index)) => state.view.focus(index),
        Some(DetailAction::Learn) => learn(f, &mut state, selected, frame),
        None => {}
    }
    ui.ctx().data_mut(|d| {
        d.insert_temp(
            id.with("layout"),
            TreeLayout {
                canvas,
                region,
                compact,
            },
        );
        d.insert_temp(id, state);
    });
}

#[derive(Default)]
struct SearchInput {
    changed: bool,
    cycle: bool,
}

/// Recommended jump, zoom, search, goals and branch chips. Compact layouts
/// keep it to one (horizontally scrolling) row.
fn toolbar(
    ui: &mut egui::Ui,
    f: &mut UiCtx,
    state: &mut TreeState,
    nodes: &[Node],
    compact: bool,
) -> SearchInput {
    let mut search = SearchInput::default();
    let mut controls = |ui: &mut egui::Ui, state: &mut TreeState| {
        if ui
            .button(RichText::new("Recommended").strong())
            .on_hover_text("Jump to the recommended upgrade (Home)")
            .clicked()
            && let Some(index) = recommended_index(f)
        {
            state.view.focus(index);
        }
        if ui.button("-").on_hover_text("Zoom out (wheel)").clicked() {
            state.view.zoom_by(0.8);
        }
        if ui.button("+").on_hover_text("Zoom in (wheel)").clicked() {
            state.view.zoom_by(1.25);
        }
        let response = ui.add(
            egui::TextEdit::singleline(&mut state.search)
                .hint_text("Search upgrades")
                .desired_width(if compact { 130.0 } else { 190.0 }),
        );
        search.changed = response.changed();
        if response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
            search.cycle = true;
            response.request_focus();
        }
        if !state.search.trim().is_empty() {
            let count = search_nodes(&state.search).len();
            muted(
                ui,
                match count {
                    0 => "No match".to_owned(),
                    1 => "1 match".to_owned(),
                    n => format!("{n} matches"),
                },
            );
            if ui.small_button("Clear").clicked() {
                state.search.clear();
            }
        }
        ui.menu_button("Path to…", |ui| {
            for goal in upgrade_goals() {
                let owned = f.game.campaign().node_state(&goal.node_id) == NodeState::Owned;
                let text = if owned {
                    RichText::new(format!("{} · owned", goal.label)).color(MUTED)
                } else {
                    RichText::new(&goal.label)
                };
                if ui.button(text).clicked() {
                    if let Some(index) = index_of(&goal.node_id) {
                        state.view.focus(index);
                    }
                    ui.close();
                }
            }
        })
        .response
        .on_hover_text("Select a goal; its path lights up on the tree");
    };
    if compact {
        ui.scope(|ui| {
            // A solid bar in its own strip under the row (CF-3-16): egui's
            // floating bar was drawn over the lower half of the controls and
            // chips. It also shows the row scrolls while chips are off-screen.
            ui.spacing_mut().scroll = egui::style::ScrollStyle {
                bar_inner_margin: 2.0,
                ..egui::style::ScrollStyle::solid()
            };
            egui::ScrollArea::horizontal()
                .id_salt("skill_tree_toolbar")
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        controls(ui, state);
                        ui.separator();
                        branch_chips(ui, state, nodes);
                    });
                });
        });
    } else {
        ui.horizontal_wrapped(|ui| {
            controls(ui, state);
            muted(ui, "Wheel zooms · drag pans · arrows move · Enter learns");
        });
        ui.horizontal_wrapped(|ui| branch_chips(ui, state, nodes));
    }
    search
}

/// One chip per branch with owned/total; a click frames the branch and
/// selects its most useful node.
fn branch_chips(ui: &mut egui::Ui, state: &mut TreeState, nodes: &[Node]) {
    let catalog = skills();
    let current = catalog.nodes[state.view.selected].branch.as_str();
    for branch in branches() {
        let members: Vec<usize> = (0..catalog.nodes.len())
            .filter(|&i| catalog.nodes[i].branch == branch)
            .collect();
        let owned = members
            .iter()
            .filter(|&&i| nodes[i].tone == Tone::Owned)
            .count();
        let response = ui
            .selectable_label(
                current == branch,
                format!("{branch} {owned}/{}", members.len()),
            )
            .on_hover_text("Frame this branch");
        if response.clicked() {
            if let Some(index) = branch_pick(branch, nodes).or_else(|| members.first().copied()) {
                state.view.selected = index;
            }
            state.view.frame_world(branch_rect(branch));
        }
    }
}

/// Arrows move to the nearest node in that direction, Home jumps to the
/// recommendation and Enter learns the selection. Inactive while a field or
/// button holds keyboard focus.
fn keyboard(ui: &mut egui::Ui, f: &mut UiCtx, state: &mut TreeState, nodes: &[Node], frame: u64) {
    if ui.ctx().egui_wants_keyboard_input() || ui.memory(|m| m.focused()).is_some() {
        return;
    }
    let (direction, home, enter) = ui.input_mut(|i| {
        let mut direction = None;
        for (key, dir) in [
            (Key::ArrowLeft, egui::Vec2::LEFT),
            (Key::ArrowRight, egui::Vec2::RIGHT),
            (Key::ArrowUp, egui::Vec2::UP),
            (Key::ArrowDown, egui::Vec2::DOWN),
        ] {
            if i.consume_key(Modifiers::NONE, key) {
                direction = Some(dir);
            }
        }
        (
            direction,
            i.consume_key(Modifiers::NONE, Key::Home),
            i.consume_key(Modifiers::NONE, Key::Enter),
        )
    });
    if let Some(dir) = direction
        && let Some(index) = nearest_in_direction(nodes, state.view.selected, dir)
    {
        state.view.focus(index);
    }
    if home && let Some(index) = recommended_index(f) {
        state.view.focus(index);
    }
    if enter && nodes[state.view.selected].tone == Tone::Buyable {
        learn(f, state, state.view.selected, frame);
    }
}

enum DetailAction {
    Select(usize),
    Learn,
}

fn price_label(payment: NodePayment) -> String {
    match payment {
        NodePayment::LegacyBodyPoints(points) => format!("{points} legacy CP"),
        NodePayment::Coins(price) => format!("{} coins", grouped(price)),
    }
}
fn chip(ui: &mut egui::Ui, text: &str, color: egui::Color32, filled: bool) {
    egui::Frame::new()
        .fill(if filled {
            color
        } else {
            egui::Color32::TRANSPARENT
        })
        .stroke(Stroke::new(1.0, color))
        .inner_margin(egui::Margin::symmetric(8, 3))
        .show(ui, |ui| {
            ui.label(
                RichText::new(text)
                    .size(13.0)
                    .color(if filled { INK } else { color }),
            );
        });
}

/// Name, state chip, the one Learn action with its disabled reason, then
/// what changes, the requirement checklist and the path summary.
fn skill_detail(
    ui: &mut egui::Ui,
    f: &mut UiCtx,
    state: &TreeState,
    index: usize,
    compact: bool,
    height: f32,
) -> Option<DetailAction> {
    let node = &skills().nodes[index];
    let mut action = None;
    let head = |ui: &mut egui::Ui, action: &mut Option<DetailAction>| {
        let campaign = f.game.campaign();
        let node_state = campaign.node_state(&node.id);
        let (text, color) = state_chip(&node_state);
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(vec2(28.0, 28.0), Sense::hover());
            upgrade_graph::pictogram(
                ui.painter(),
                r,
                upgrade_graph::skill_icon(&node.icon),
                AMBER,
            );
            let name = egui::Label::new(
                RichText::new(&node.name)
                    .size(if compact { 17.0 } else { 19.0 })
                    .strong(),
            );
            ui.add(if compact {
                name.extend()
            } else {
                name.truncate()
            });
            if compact {
                chip(ui, &text, color, node_state == NodeState::Owned);
            }
        });
        if !compact {
            chip(ui, &text, color, node_state == NodeState::Owned);
        }
        if let Some((learned, line)) = &state.learned
            && *learned == index
        {
            ui.label(RichText::new(line).color(AMBER).size(13.0));
        }
        if node_state == NodeState::Owned {
            if !campaign.skill_nodes.contains(&node.id) && is_body_node(node) {
                muted(ui, "Covered by legacy body allocation");
            }
            return;
        }
        let preflight = campaign.can_purchase_node(&node.id);
        let price = campaign
            .node_payment(&node.id)
            .map_or_else(|_| "Unavailable".into(), price_label);
        let mut button =
            egui::Button::new(RichText::new(format!("Learn skill · {price}")).strong())
                .min_size(vec2(ui.available_width().min(320.0), 34.0));
        if preflight.is_ok() {
            button = button
                .fill(egui::Color32::BLACK)
                .stroke(Stroke::new(2.0, AMBER));
        }
        if ui
            .add_enabled(preflight.is_ok(), button)
            .on_hover_text("Enter learns the selected upgrade")
            .clicked()
        {
            *action = Some(DetailAction::Learn);
        }
        if let Err(reason) = &preflight {
            ui.label(RichText::new(reason).color(MUTED).size(12.0));
        }
    };
    let body = |ui: &mut egui::Ui, f: &mut UiCtx, action: &mut Option<DetailAction>| {
        let campaign = f.game.campaign();
        ui.label(&node.description);
        if let Some(counters) = counters_line(node) {
            ui.colored_label(AMBER, counters);
        }
        let preview = campaign.node_stat_preview(&node.id);
        if !preview.is_empty() {
            ui.add_space(6.0);
            ui.strong("What changes");
            what_changes(ui, &node.id, &preview);
        }
        let requirements = campaign.node_requirements(&node.id);
        ui.add_space(6.0);
        ui.strong("Requirements");
        if requirements.is_empty() {
            muted(ui, "None · journey foundation");
        }
        for requirement in &requirements {
            ui.horizontal_wrapped(|ui| {
                if requirement.met {
                    ui.colored_label(GREEN, "✔");
                    ui.label(&requirement.label);
                } else {
                    ui.colored_label(MUTED_RED, "✖");
                    ui.colored_label(MUTED, &requirement.label);
                }
            });
        }
        let path = campaign.path_to_node(&node.id);
        if path.steps.len() > 1 {
            ui.add_space(6.0);
            ui.strong("Path to here");
            ui.label(format!(
                "{} steps · {} coins",
                path.steps.len(),
                grouped(path.total_coins)
            ));
            if let Some(blocking) = &path.blocking {
                muted(ui, format!("Blocked until: {blocking}"));
            }
            muted(
                ui,
                format!(
                    "In order: {}",
                    path.steps
                        .iter()
                        .map(|id| node_name(id))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            );
            if ui.button("Select next step").clicked()
                && let Some(next) = index_of(&path.steps[0])
            {
                *action = Some(DetailAction::Select(next));
            }
        }
        if let Some(note) = campaign.legacy_node_note(&node.id) {
            ui.add_space(6.0);
            muted(ui, note);
        }
        if is_body_node(node)
            && campaign.legacy_features
            && campaign.player.body.iter().any(|n| *n > 0)
            && ui.button("Refund legacy body allocations").clicked()
        {
            f.dispatch(
                Command::RespecPlayer,
                "Legacy body points returned to the tree.",
            );
        }
        if node
            .effects
            .iter()
            .any(|e| matches!(e, SkillEffect::WeaponTraining(_)))
        {
            ui.add_space(6.0);
            muted(
                ui,
                "Training lets you use the weapon class. Buy or loot the weapons in Equipment.",
            );
        }
    };
    if compact {
        ui.horizontal_top(|ui| {
            let left = (ui.available_width() * 0.5).clamp(220.0, 420.0);
            ui.allocate_ui_with_layout(vec2(left, height), Layout::top_down(Align::Min), |ui| {
                ui.set_width(left);
                head(ui, &mut action);
            });
            ui.separator();
            ui.allocate_ui_with_layout(
                vec2(ui.available_width(), height),
                Layout::top_down(Align::Min),
                |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt(("skill-detail", &node.id))
                        .max_height(height)
                        .show(ui, |ui| body(ui, f, &mut action));
                },
            );
        });
    } else {
        let top = ui.cursor().top();
        head(ui, &mut action);
        ui.add_space(4.0);
        let used = ui.cursor().top() - top;
        egui::ScrollArea::vertical()
            .id_salt(("skill-detail", &node.id))
            .max_height((height - used).max(60.0))
            .show(ui, |ui| body(ui, f, &mut action));
    }
    action
}

fn is_body_node(node: &SkillNode) -> bool {
    node.effects
        .iter()
        .any(|e| matches!(e, SkillEffect::BodyDevelopment { .. }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use horde_core::content::StructureKind;
    use horde_core::content::content;
    fn structure_node(kind: StructureKind, rank: u32) -> &'static SkillNode {
        skills()
            .nodes
            .iter()
            .find(|node| {
                node.effects.iter().any(|effect| match effect {
                    SkillEffect::Blueprint(value) => rank == 0 && *value == kind,
                    SkillEffect::StructureRank {
                        kind: value,
                        rank: value_rank,
                    } => rank > 0 && *value == kind && *value_rank == rank,
                    _ => false,
                })
            })
            .unwrap()
    }
    fn actual_node(campaign: &Campaign, id: &str) -> Node {
        catalogue_nodes_for(campaign).remove(index_of(id).unwrap())
    }
    fn acquire_requirements(campaign: &mut Campaign, id: &str) {
        let parents = skills().node(id).unwrap().prerequisites.clone();
        for parent in parents {
            if !campaign.has_node(&parent) {
                acquire_requirements(campaign, &parent);
                campaign.purchase_node(&parent).unwrap();
            }
        }
    }
    #[test]
    fn catalogue_fortification_nodes_render_real_milestone_affordability_and_owned_states() {
        for kind in StructureKind::ALL {
            let mut campaign = Campaign {
                coins: 100_000,
                ..Default::default()
            };
            let blueprint = structure_node(kind, 0);
            assert_ne!(actual_node(&campaign, &blueprint.id).tone, Tone::Buyable);
            assert!(campaign.can_purchase_node(&blueprint.id).is_err());
            campaign.victories = 100;
            acquire_requirements(&mut campaign, &blueprint.id);
            for rank in 0..=3 {
                let node = structure_node(kind, rank);
                acquire_requirements(&mut campaign, &node.id);
                let affordable = actual_node(&campaign, &node.id);
                assert_eq!(affordable.tone, Tone::Buyable);
                assert_eq!(affordable.badge, format!("{} c", grouped(node.price)));
                campaign.coins = node.price - 1;
                let poor = actual_node(&campaign, &node.id);
                assert_eq!(poor.tone, Tone::NeedsCoins);
                assert_eq!(poor.badge, format!("{} c · need 1", grouped(node.price)));
                assert!(campaign.can_purchase_node(&node.id).is_err());
                campaign.coins = 100_000;
                let before = campaign.coins;
                campaign.purchase_node(&node.id).unwrap();
                assert_eq!(before - campaign.coins, node.price);
                let owned = actual_node(&campaign, &node.id);
                assert_eq!(owned.tone, Tone::Owned);
                assert_eq!(owned.badge, "OWNED");
                assert!(campaign.blueprints[kind]);
                assert_eq!(campaign.structure_upgrades[kind], rank);
            }
        }
    }
    #[test]
    fn legacy_fortification_transactions_keep_prices_and_are_reflected_by_production_catalogue_nodes()
     {
        for kind in StructureKind::ALL {
            let mut campaign = Campaign {
                coins: 1000,
                legacy_features: true,
                ..Default::default()
            };
            let clears = content().structure(kind).unlock_clear;
            assert!(!campaign.blueprint_available(kind));
            assert!(campaign.purchase_blueprint(kind).is_err());
            campaign
                .first_clears
                .extend(content().levels[..clears].iter().map(|l| l.id.clone()));
            assert!(campaign.blueprint_available(kind));
            let before = campaign.coins;
            campaign.purchase_blueprint(kind).unwrap();
            assert_eq!(
                before - campaign.coins,
                content().structure(kind).blueprint_price
            );
            assert_eq!(
                actual_node(&campaign, &structure_node(kind, 0).id).tone,
                Tone::Owned
            );
            for rank in 1..=3 {
                let before = campaign.coins;
                campaign.upgrade_structure(kind).unwrap();
                assert_eq!(before - campaign.coins, u64::from(40 * rank));
                assert_eq!(
                    actual_node(&campaign, &structure_node(kind, rank).id).tone,
                    Tone::Owned
                );
            }
            assert!(campaign.upgrade_structure(kind).is_err());
        }
    }
    #[test]
    fn node_states_map_to_canvas_badges_and_detail_chips() {
        let cases = [
            (NodeState::Owned, "OWNED", Tone::Owned, "Owned"),
            (
                NodeState::Buyable(NodePayment::Coins(40)),
                "40 c",
                Tone::Buyable,
                "Ready to learn",
            ),
            (
                NodeState::NeedsCoins {
                    payment: NodePayment::Coins(40),
                    short: 15,
                },
                "40 c · need 15",
                Tone::NeedsCoins,
                "Short 15 coins",
            ),
            (
                NodeState::NeedsPrerequisites {
                    missing: vec!["steady_hands".into()],
                },
                "Locked",
                Tone::Prerequisite,
                "Locked · learn Steady hands first",
            ),
            (
                NodeState::NeedsMilestone {
                    victories: Some((1, 3)),
                    liberated: None,
                },
                "Win 3",
                Tone::Milestone,
                "Locked · win 3 defenses (1/3)",
            ),
            (
                NodeState::NeedsMilestone {
                    victories: Some((0, 3)),
                    liberated: Some((0, 2)),
                },
                "Free 2",
                Tone::Milestone,
                "Locked · free 2 countries (0/2)",
            ),
            (
                NodeState::Buyable(NodePayment::LegacyBodyPoints(1)),
                "1 CP",
                Tone::Buyable,
                "Ready to learn",
            ),
        ];
        for (state, text, tone, chip) in cases {
            assert_eq!(badge(&state), (text.to_owned(), tone));
            assert_eq!(state_chip(&state).0, chip);
        }
        assert_eq!(skills().node("steady_hands").unwrap().name, "Steady hands");
    }
    #[test]
    fn branch_rects_cover_every_member_and_chips_list_each_branch_once() {
        let names = branches();
        assert_eq!(names[0], skills().nodes[0].branch);
        let unique: BTreeSet<_> = names.iter().collect();
        assert_eq!(unique.len(), names.len());
        for node in &skills().nodes {
            assert!(
                branch_rect(&node.branch)
                    .expand(0.5)
                    .contains(egui::pos2(node.position[0], node.position[1]))
            );
        }
    }
}
