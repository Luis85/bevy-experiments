//! Inspectable progression canvas. Geometry and view state are presentation-only.
use crate::art::{AMBER, INK, MUTED, TEXT};
use crate::ui::muted;
use bevy_egui::egui::{
    self, Align, Align2, Color32, FontId, Layout, Rect, Sense, Stroke, pos2, vec2,
};
use horde_core::{campaign::Skill, content::StructureKind};

#[derive(Clone, Copy)]
pub(super) enum Icon {
    Skill(Skill),
    Firearm,
    Grenade,
    Flame,
    Defense(StructureKind, u32),
}

pub(super) struct Node {
    pub position: egui::Vec2,
    pub title: String,
    pub state: String,
    pub icon: Icon,
    pub acquired: bool,
    pub available: bool,
    pub prerequisites: Vec<usize>,
}

#[derive(Clone)]
pub(super) struct View {
    pub selected: usize,
    pub recommendation: Option<usize>,
    zoom: f32,
    pan: egui::Vec2,
    focus_selected: bool,
    canvas_size: egui::Vec2,
}
impl Default for View {
    fn default() -> Self {
        Self {
            selected: 0,
            recommendation: None,
            zoom: 1.0,
            pan: egui::Vec2::ZERO,
            focus_selected: true,
            canvas_size: egui::Vec2::ZERO,
        }
    }
}
impl View {
    pub(super) fn focus(&mut self, index: usize) {
        self.selected = index;
        self.focus_selected = true;
    }
    pub(super) fn recommend(&mut self, selected: Option<usize>) {
        if selected.is_some() && self.recommendation != selected {
            self.selected = selected.unwrap_or(0);
            self.pan = egui::Vec2::ZERO;
            self.zoom = 1.0;
            self.focus_selected = true;
        }
        self.recommendation = selected;
    }
    fn zoom_by(&mut self, multiplier: f32) {
        self.zoom = (self.zoom * multiplier).clamp(0.6, 2.5);
    }
}

pub(super) struct Graph<'a> {
    nodes: &'a [Node],
    defenses: bool,
}
#[derive(Clone, Default)]
struct WireCache {
    layout: Option<u64>,
    routes: std::sync::Arc<std::collections::BTreeMap<(usize, usize), Vec<egui::Vec2>>>,
}
impl WireCache {
    fn resolve(
        &mut self,
        nodes: &[Node],
    ) -> std::sync::Arc<std::collections::BTreeMap<(usize, usize), Vec<egui::Vec2>>> {
        use std::hash::{Hash, Hasher};
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        for node in nodes {
            node.position.x.to_bits().hash(&mut hash);
            node.position.y.to_bits().hash(&mut hash);
            node.prerequisites.hash(&mut hash);
        }
        let fingerprint = hash.finish();
        if self.layout != Some(fingerprint) {
            let mut routes = std::collections::BTreeMap::new();
            for (child, node) in nodes.iter().enumerate() {
                for parent in &node.prerequisites {
                    if *parent < nodes.len() {
                        routes.insert((*parent, child), skill_wire(nodes, *parent, child));
                    }
                }
            }
            self.routes = std::sync::Arc::new(routes);
            self.layout = Some(fingerprint);
        }
        self.routes.clone()
    }
}

impl<'a> Graph<'a> {
    pub(super) fn skills(nodes: &'a [Node]) -> Self {
        Self {
            nodes,
            defenses: false,
        }
    }

    pub(super) fn show(
        &self,
        ui: &mut egui::Ui,
        id: egui::Id,
        view: &mut View,
        height: f32,
    ) -> Rect {
        // A graph is one vertical surface even when called inside a horizontal split.
        ui.with_layout(Layout::top_down(Align::Min), |ui| {
            self.show_contents(ui, id, view, height)
        })
        .inner
    }

    fn show_contents(&self, ui: &mut egui::Ui, id: egui::Id, view: &mut View, height: f32) -> Rect {
        ui.horizontal_wrapped(|ui| {
            if ui
                .small_button("Zoom -")
                .on_hover_text("Zoom out")
                .clicked()
            {
                view.zoom_by(0.8);
            }
            if ui.small_button("Zoom +").on_hover_text("Zoom in").clicked() {
                view.zoom_by(1.25);
            }
            if ui
                .small_button(if self.defenses { "Fit" } else { "Focus" })
                .on_hover_text(if self.defenses {
                    "Fit all upgrades"
                } else {
                    "Center the selected skill at a readable size"
                })
                .clicked()
            {
                view.zoom = 1.0;
                view.pan = egui::Vec2::ZERO;
                view.focus_selected = true;
            }
            if !self.defenses && !self.nodes.is_empty() {
                if ui.small_button("Previous").clicked() {
                    view.selected = (view.selected + self.nodes.len() - 1) % self.nodes.len();
                    view.focus_selected = true;
                }
                ui.label(format!("{} / {}", view.selected + 1, self.nodes.len()));
                if ui.small_button("Next").clicked() {
                    view.selected = (view.selected + 1) % self.nodes.len();
                    view.focus_selected = true;
                }
            }
            muted(
                ui,
                if self.defenses {
                    "Drag to pan · select to inspect"
                } else {
                    "Scroll or drag to pan"
                },
            );
        });
        let (rect, drag) =
            ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::drag());
        if drag.dragged() {
            view.pan += ui.input(|i| i.pointer.delta());
            view.focus_selected = false;
        }
        if ui.input(|i| i.pointer.hover_pos().is_some_and(|p| rect.contains(p))) {
            let pinch = ui.input(|i| i.zoom_delta());
            if pinch != 1.0 {
                view.zoom_by(pinch);
            }
            if !self.defenses {
                let wheel = ui.input_mut(|i| {
                    let delta = i.smooth_scroll_delta;
                    i.smooth_scroll_delta = egui::Vec2::ZERO;
                    delta
                });
                if wheel != egui::Vec2::ZERO {
                    view.pan += wheel;
                    view.focus_selected = false;
                }
            }
        }
        let compact = height < 310.0;
        let horizontal = self.defenses && (compact || rect.width() / rect.height() > 1.7);
        let (bounds, origin) = if self.defenses {
            (
                if compact {
                    vec2(850.0, 390.0)
                } else if horizontal {
                    vec2(930.0, 480.0)
                } else {
                    vec2(740.0, 680.0)
                },
                egui::Vec2::ZERO,
            )
        } else {
            skill_bounds(self.nodes)
        };
        let base_scale = if self.defenses {
            fit_scale(rect.size(), bounds)
        } else {
            fit_scale(rect.size(), bounds).max(MIN_SKILL_SCALE)
        };
        let scale = if self.defenses {
            base_scale * view.zoom
        } else {
            (base_scale * view.zoom).max(MIN_SKILL_SCALE)
        };
        if !self.defenses {
            view.zoom = scale / base_scale;
            if (view.canvas_size - rect.size()).length() > 1.0 {
                view.focus_selected = true;
            }
            if view.focus_selected
                && let Some(node) = self.nodes.get(view.selected)
            {
                view.pan = -(node.position - origin) * scale - vec2(0.0, 26.0 * scale);
            }
            view.pan = bounded_pan(view.pan, rect.size(), bounds, scale);
        } else {
            view.pan.x = view.pan.x.clamp(-rect.width(), rect.width());
            view.pan.y = view.pan.y.clamp(-rect.height(), rect.height());
        }
        view.focus_selected = false;
        view.canvas_size = rect.size();
        let center = rect.center() + view.pan;
        let point = |v: egui::Vec2| center + (v - origin) * scale;
        let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
        painter.rect_filled(rect, 0.0, Color32::from_rgb(12, 17, 17));
        painter.rect_stroke(
            rect,
            0.0,
            Stroke::new(1.0, MUTED.gamma_multiply(0.45)),
            egui::StrokeKind::Inside,
        );
        let line = Stroke::new(1.2, MUTED.gamma_multiply(0.5));
        if self.defenses {
            for column in 0..3 {
                let x = (column as f32 - 1.0) * 235.0;
                if !horizontal {
                    painter.text(
                        point(vec2(x, -267.0)),
                        Align2::CENTER_CENTER,
                        ["TRAPS", "WALLS", "TURRETS"][column],
                        FontId::proportional((16.0 * scale).max(12.0)),
                        MUTED,
                    );
                }
                for rank in 0..3 {
                    let a = canvas_position(
                        column * 4 + rank as usize,
                        self.defenses,
                        horizontal,
                        compact,
                    );
                    let b = canvas_position(
                        column * 4 + rank as usize + 1,
                        self.defenses,
                        horizontal,
                        compact,
                    );
                    let acquired = self.nodes[column * 4 + rank as usize + 1].acquired;
                    let points = if horizontal {
                        vec![point(a + vec2(34.0, 0.0)), point(b - vec2(34.0, 0.0))]
                    } else {
                        // Route around captions rather than through their costs.
                        vec![
                            point(a + vec2(34.0, 0.0)),
                            point(a + vec2(100.0, 0.0)),
                            point(b + vec2(100.0, 0.0)),
                            point(b + vec2(34.0, 0.0)),
                        ]
                    };
                    painter.add(egui::Shape::line(
                        points,
                        if acquired {
                            Stroke::new(2.0, AMBER)
                        } else {
                            line
                        },
                    ));
                }
            }
        } else {
            let routes = ui.ctx().data_mut(|d| {
                d.get_temp_mut_or_default::<WireCache>(id.with("authored_wire_routes"))
                    .resolve(self.nodes)
            });
            for (index, node) in self.nodes.iter().enumerate() {
                for parent in &node.prerequisites {
                    if *parent >= self.nodes.len() {
                        continue;
                    }
                    let Some(route) = routes.get(&(*parent, index)) else {
                        continue;
                    };
                    if route.len() < 2 {
                        continue;
                    }
                    let color = if node.acquired {
                        AMBER
                    } else {
                        MUTED.gamma_multiply(0.65)
                    };
                    let points: Vec<_> = route.iter().map(|v| point(*v)).collect();
                    let tip = *points.last().expect("nonempty route");
                    let direction = (tip - points[points.len() - 2]).normalized();
                    painter.add(egui::Shape::line(points, Stroke::new(1.5, color)));
                    let wing = egui::vec2(-direction.y, direction.x) * 4.0;
                    painter
                        .line_segment([tip, tip - direction * 8.0 + wing], Stroke::new(1.5, color));
                    painter
                        .line_segment([tip, tip - direction * 8.0 - wing], Stroke::new(1.5, color));
                }
            }
        }
        for (index, node) in self.nodes.iter().enumerate() {
            let position = if self.defenses && (horizontal || compact) {
                canvas_position(index, self.defenses, horizontal, compact)
            } else {
                node.position
            };
            let node_center = point(position);
            let icon_rect = Rect::from_center_size(node_center, vec2(64.0, 64.0) * scale);
            let hit_rect = Rect::from_center_size(
                node_center + vec2(0.0, 24.0) * scale,
                vec2(160.0, 122.0) * scale,
            )
            .intersect(rect);
            if !hit_rect.is_positive() {
                continue;
            }
            let response = ui.interact(hit_rect, id.with(index), Sense::click());
            response.widget_info(|| {
                egui::WidgetInfo::selected(
                    egui::WidgetType::Button,
                    true,
                    view.selected == index,
                    format!("{}: {}. Select to inspect.", node.title, node.state),
                )
            });
            if response.clicked() || response.gained_focus() {
                view.selected = index;
                if response.gained_focus() {
                    view.focus_selected = true;
                }
            }
            let selected = view.selected == index;
            let color = if node.acquired || selected || response.has_focus() {
                AMBER
            } else if node.available || response.hovered() {
                TEXT
            } else {
                MUTED
            };
            painter.rect_filled(
                icon_rect,
                0.0,
                if node.acquired {
                    Color32::from_rgb(65, 51, 26)
                } else {
                    INK
                },
            );
            painter.rect_stroke(
                icon_rect,
                0.0,
                Stroke::new(if selected { 2.0 } else { 1.0 }, color),
                egui::StrokeKind::Inside,
            );
            if selected || response.has_focus() {
                painter.rect_stroke(
                    icon_rect.expand(5.0 * scale),
                    0.0,
                    Stroke::new(1.0, AMBER),
                    egui::StrokeKind::Inside,
                );
            }
            pictogram(&painter, icon_rect.shrink(12.0 * scale), node.icon, color);
            let galley = painter.layout_job(caption_job(&node.title, scale));
            let title_top = icon_rect.bottom() + 7.0;
            let state_y = title_top - node_center.y + galley.size().y + 10.0;
            // Centre-aligned galleys are laid out around x = 0.
            painter.galley(pos2(node_center.x, title_top), galley, TEXT);
            let compact_caption = !self.defenses && (compact || scale < 0.8);
            let badge = if compact_caption {
                short_state(&node.state)
            } else {
                node.state.clone()
            };
            painter.text(
                node_center + vec2(0.0, state_y),
                Align2::CENTER_CENTER,
                &badge,
                FontId::proportional((12.0 * scale).max(11.0)),
                color,
            );
            response.on_hover_text(format!(
                "{} · {}\nSelect for effects and purchase action.",
                node.title, node.state
            ));
        }
        rect
    }
}

const MIN_SKILL_SCALE: f32 = 0.75;
/// Node captions wrap to at most this many centred rows.
const TITLE_ROWS: usize = 2;
fn bounded_pan(
    pan: egui::Vec2,
    viewport: egui::Vec2,
    bounds: egui::Vec2,
    scale: f32,
) -> egui::Vec2 {
    let limit = ((bounds * scale - viewport) * 0.5).max(egui::Vec2::ZERO);
    pan.clamp(-limit, limit)
}
/// A node name wraps onto a second centred line; only a pathological third
/// line is elided. Authored nodes are spaced for two caption rows.
fn caption_job(title: &str, scale: f32) -> egui::text::LayoutJob {
    let width = (160.0 * scale).clamp(120.0, 180.0);
    let mut job = egui::text::LayoutJob::simple(
        title.to_owned(),
        FontId::proportional((15.0 * scale).clamp(12.0, 20.0)),
        TEXT,
        width,
    );
    job.wrap = egui::text::TextWrapping {
        max_width: width,
        max_rows: TITLE_ROWS,
        break_anywhere: false,
        overflow_character: Some('…'),
    };
    job.halign = Align::Center;
    job
}
fn short_state(state: &str) -> String {
    match state {
        "LEARNED" => "Owned".into(),
        "PREREQUISITE" => "Locked".into(),
        other => other.replace(" coins", " c"),
    }
}
fn skill_obstacle(node: &Node) -> Rect {
    Rect::from_min_max(
        pos2(node.position.x - 92.0, node.position.y - 42.0),
        pos2(node.position.x + 92.0, node.position.y + 116.0),
    )
}
fn wire_segment_clear(a: egui::Vec2, b: egui::Vec2, obstacles: &[Rect]) -> bool {
    obstacles.iter().all(|r| {
        if (a.y - b.y).abs() < 0.01 {
            !(a.y > r.top()
                && a.y < r.bottom()
                && a.x.max(b.x) > r.left()
                && a.x.min(b.x) < r.right())
        } else if (a.x - b.x).abs() < 0.01 {
            !(a.x > r.left()
                && a.x < r.right()
                && a.y.max(b.y) > r.top()
                && a.y.min(b.y) < r.bottom())
        } else {
            false
        }
    })
}
/// Orthogonal ports and open channels keep prerequisites out of every unrelated
/// icon/caption envelope. All geometry comes from the authored node positions.
fn skill_wire(nodes: &[Node], parent: usize, child: usize) -> Vec<egui::Vec2> {
    let obstacles: Vec<_> = nodes.iter().map(skill_obstacle).collect();
    let source = nodes[parent].position;
    let target = nodes[child].position;
    let ports = |p: egui::Vec2| {
        [
            p + vec2(0.0, -52.0),
            p + vec2(-102.0, 0.0),
            p + vec2(102.0, 0.0),
        ]
    };
    let mut best = Vec::new();
    let mut best_cost = f32::INFINITY;
    {
        let mut consider = |path: Vec<egui::Vec2>| {
            if !path
                .windows(2)
                .all(|w| wire_segment_clear(w[0], w[1], &obstacles))
            {
                return;
            }
            let cost: f32 = path.windows(2).map(|w| (w[1] - w[0]).length()).sum::<f32>()
                + path.len() as f32 * 8.0;
            if cost < best_cost {
                best_cost = cost;
                best = path;
            }
        };
        for a in ports(source) {
            for b in ports(target) {
                consider(vec![a, vec2(a.x, b.y), b]);
                consider(vec![a, vec2(b.x, a.y), b]);
            }
        }
    }
    if best.is_empty() {
        for a in ports(source) {
            for b in ports(target) {
                for obstacle in &obstacles {
                    let mut consider = |path: Vec<egui::Vec2>| {
                        if !path
                            .windows(2)
                            .all(|w| wire_segment_clear(w[0], w[1], &obstacles))
                        {
                            return;
                        }
                        let cost = path.windows(2).map(|w| (w[1] - w[0]).length()).sum::<f32>()
                            + path.len() as f32 * 8.0;
                        if cost < best_cost {
                            best_cost = cost;
                            best = path;
                        }
                    };
                    for y in [obstacle.top() - 10.0, obstacle.bottom() + 10.0] {
                        consider(vec![a, vec2(a.x, y), vec2(b.x, y), b]);
                    }
                    for x in [obstacle.left() - 10.0, obstacle.right() + 10.0] {
                        consider(vec![a, vec2(x, a.y), vec2(x, b.y), b]);
                    }
                }
            }
        }
    }
    if best.is_empty() {
        return best;
    }
    let first = best[0];
    let last = *best.last().expect("nonempty path");
    best.insert(0, source + (first - source).normalized() * 34.0);
    best.push(target + (last - target).normalized() * 34.0);
    best.dedup_by(|a, b| (*a - *b).length() < 0.01);
    best
}

pub(super) fn skill_icon(name: &str) -> Icon {
    match name {
        "reload" => Icon::Skill(Skill::QuickReload),
        "pierce" => Icon::Skill(Skill::Piercing),
        "sweep" => Icon::Skill(Skill::Sweep),
        "health" => Icon::Skill(Skill::SecondWind),
        "brace" => Icon::Skill(Skill::Brace),
        "engineering" => Icon::Skill(Skill::Engineering),
        "trap" => Icon::Defense(StructureKind::Trap, 0),
        "wall" => Icon::Defense(StructureKind::Wall, 0),
        "turret" => Icon::Defense(StructureKind::Turret, 0),
        "grenade" => Icon::Grenade,
        "flame" => Icon::Flame,
        _ => Icon::Firearm,
    }
}
fn skill_bounds(nodes: &[Node]) -> (egui::Vec2, egui::Vec2) {
    let mut min = nodes.first().map_or(egui::Vec2::ZERO, |node| node.position);
    let mut max = min;
    for node in nodes {
        min = min.min(node.position);
        max = max.max(node.position);
    }
    (
        max - min + vec2(240.0, 200.0),
        (min + max) * 0.5 + vec2(0.0, 31.0),
    )
}

fn fit_scale(available: egui::Vec2, bounds: egui::Vec2) -> f32 {
    ((available.x - 24.0) / bounds.x)
        .min((available.y - 24.0) / bounds.y)
        .clamp(0.05, 1.4)
}

pub(super) fn skill_position(index: usize) -> egui::Vec2 {
    let (column, row) = match index {
        0 => (0, 0),
        1 => (0, 1),
        2 => (1, 0),
        3 => (2, 0),
        4 => (1, 1),
        _ => (2, 1),
    };
    vec2((column as f32 - 1.0) * 235.0, -90.0 + row as f32 * 200.0)
}

pub(super) fn defense_position(column: usize, rank: u32) -> egui::Vec2 {
    vec2((column as f32 - 1.0) * 235.0, -205.0 + rank as f32 * 145.0)
}

fn defense_canvas_position(column: usize, rank: u32, horizontal: bool) -> egui::Vec2 {
    if horizontal {
        vec2(-315.0 + rank as f32 * 210.0, -170.0 + column as f32 * 150.0)
    } else {
        defense_position(column, rank)
    }
}

fn canvas_position(index: usize, defenses: bool, horizontal: bool, compact: bool) -> egui::Vec2 {
    if compact && defenses {
        vec2(
            -300.0 + (index % 4) as f32 * 200.0,
            -125.0 + (index / 4) as f32 * 125.0,
        )
    } else if compact {
        let old = skill_position(index);
        vec2(
            old.x / 235.0 * 200.0,
            if old.y < 0.0 { -65.0 } else { 65.0 },
        )
    } else if defenses {
        defense_canvas_position(index / 4, (index % 4) as u32, horizontal)
    } else {
        skill_position(index)
    }
}

/// The same small, square-ended strokes form every progression pictogram.
pub(super) fn pictogram(p: &egui::Painter, rect: Rect, icon: Icon, color: Color32) {
    let unit = rect.width().min(rect.height()) / 24.0;
    let origin = rect.center() - vec2(12.0, 12.0) * unit;
    let point = |x: f32, y: f32| origin + vec2(x, y) * unit;
    let stroke = Stroke::new((2.0 * unit).max(1.0), color);
    let segment = |a: [f32; 2], b: [f32; 2]| {
        p.line_segment([point(a[0], a[1]), point(b[0], b[1])], stroke);
    };
    let box_outline = |x: f32, y: f32, w: f32, h: f32| {
        p.rect_stroke(
            Rect::from_min_size(point(x, y), vec2(w, h) * unit),
            0.0,
            stroke,
            egui::StrokeKind::Inside,
        );
    };
    match icon {
        Icon::Firearm => {
            box_outline(3.0, 7.0, 17.0, 6.0);
            segment([20.0, 10.0], [23.0, 10.0]);
            segment([8.0, 13.0], [6.0, 20.0]);
            segment([12.0, 13.0], [12.0, 17.0]);
        }
        Icon::Grenade => {
            box_outline(6.0, 8.0, 12.0, 13.0);
            box_outline(9.0, 3.0, 6.0, 5.0);
            segment([15.0, 4.0], [21.0, 7.0]);
            segment([10.0, 11.0], [10.0, 18.0]);
            segment([14.0, 11.0], [14.0, 18.0]);
        }
        Icon::Flame => {
            for (a, b) in [
                ([12.0, 2.0], [6.0, 12.0]),
                ([6.0, 12.0], [5.0, 17.0]),
                ([5.0, 17.0], [12.0, 22.0]),
                ([12.0, 22.0], [19.0, 17.0]),
                ([19.0, 17.0], [17.0, 10.0]),
                ([17.0, 10.0], [13.0, 14.0]),
                ([13.0, 14.0], [12.0, 2.0]),
            ] {
                segment(a, b);
            }
        }

        Icon::Skill(Skill::Piercing) => {
            segment([2.0, 12.0], [22.0, 12.0]);
            segment([17.0, 7.0], [22.0, 12.0]);
            segment([17.0, 17.0], [22.0, 12.0]);
            segment([7.0, 3.0], [7.0, 8.0]);
            segment([7.0, 16.0], [7.0, 21.0]);
            segment([12.0, 3.0], [12.0, 8.0]);
            segment([12.0, 16.0], [12.0, 21.0]);
        }
        Icon::Skill(Skill::QuickReload) => {
            box_outline(8.0, 4.0, 8.0, 16.0);
            segment([11.0, 8.0], [13.0, 8.0]);
            segment([11.0, 12.0], [13.0, 12.0]);
            segment([3.0, 16.0], [3.0, 5.0]);
            segment([3.0, 5.0], [6.0, 8.0]);
            segment([21.0, 8.0], [21.0, 19.0]);
            segment([21.0, 19.0], [18.0, 16.0]);
        }
        Icon::Skill(Skill::Sweep) => {
            segment([4.0, 20.0], [20.0, 4.0]);
            segment([2.0, 15.0], [8.0, 21.0]);
            segment([12.0, 3.0], [19.0, 3.0]);
            segment([21.0, 5.0], [21.0, 12.0]);
            segment([4.0, 4.0], [7.0, 7.0]);
        }
        Icon::Skill(Skill::SecondWind) => {
            box_outline(3.0, 6.0, 18.0, 15.0);
            box_outline(8.0, 2.0, 8.0, 5.0);
            segment([12.0, 9.0], [12.0, 18.0]);
            segment([8.0, 13.5], [16.0, 13.5]);
        }
        Icon::Skill(Skill::Brace) => {
            for (a, b) in [
                ([3.0, 4.0], [21.0, 4.0]),
                ([3.0, 4.0], [3.0, 15.0]),
                ([21.0, 4.0], [21.0, 15.0]),
                ([3.0, 15.0], [12.0, 22.0]),
                ([21.0, 15.0], [12.0, 22.0]),
                ([12.0, 7.0], [12.0, 17.0]),
            ] {
                segment(a, b);
            }
        }
        Icon::Skill(Skill::Engineering) => {
            segment([4.0, 20.0], [17.0, 7.0]);
            segment([14.0, 3.0], [14.0, 8.0]);
            segment([14.0, 8.0], [19.0, 10.0]);
            segment([19.0, 10.0], [22.0, 6.0]);
            segment([22.0, 6.0], [19.0, 3.0]);
            box_outline(2.0, 17.0, 5.0, 5.0);
        }
        Icon::Defense(kind, rank) => {
            match kind {
                StructureKind::Trap => {
                    segment([2.0, 18.0], [22.0, 18.0]);
                    for x in [5.0, 12.0, 19.0] {
                        segment([x - 3.0, 18.0], [x, 7.0]);
                        segment([x, 7.0], [x + 3.0, 18.0]);
                    }
                }
                StructureKind::Wall => {
                    box_outline(2.0, 5.0, 20.0, 15.0);
                    segment([2.0, 12.0], [22.0, 12.0]);
                    segment([9.0, 5.0], [9.0, 12.0]);
                    segment([15.0, 12.0], [15.0, 20.0]);
                }
                StructureKind::Turret => {
                    box_outline(7.0, 6.0, 11.0, 7.0);
                    segment([1.0, 9.0], [7.0, 9.0]);
                    segment([12.0, 13.0], [12.0, 17.0]);
                    segment([12.0, 17.0], [5.0, 22.0]);
                    segment([12.0, 17.0], [19.0, 22.0]);
                }
            }
            for index in 0..rank {
                p.rect_filled(
                    Rect::from_min_size(
                        point(4.0 + index as f32 * 6.0, 0.0),
                        vec2(4.0, 2.0) * unit,
                    ),
                    0.0,
                    color,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
                state: "PREREQUISITE".into(),
                icon: skill_icon(&n.icon),
                acquired: false,
                available: false,
                prerequisites: n
                    .prerequisites
                    .iter()
                    .filter_map(|p| authored.iter().position(|n| &n.id == p))
                    .collect(),
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
    fn compact_skills_keep_readable_icons_and_every_selection_can_be_focused_with_bounded_pan() {
        let viewport = vec2(748.0, 120.0);
        for branch in ["Firearms", "Survival", "Ordnance"] {
            let nodes = authored_branch(branch);
            let (bounds, origin) = skill_bounds(&nodes);
            let scale = fit_scale(viewport, bounds).max(MIN_SKILL_SCALE);
            assert!(
                64.0 * scale >= 48.0,
                "Compact skill icons need a usable minimum size"
            );
            let window = Rect::from_min_size(egui::Pos2::ZERO, viewport);
            for node in &nodes {
                let pan = bounded_pan(-(node.position - origin) * scale, viewport, bounds, scale);
                let center = viewport * 0.5 + pan + (node.position - origin) * scale;
                let icon =
                    Rect::from_center_size(pos2(center.x, center.y), vec2(64.0, 64.0) * scale);
                assert!(
                    window.contains_rect(icon),
                    "{} cannot be brought fully into view",
                    node.title
                );
            }
            let limit = bounded_pan(vec2(1e6, -1e6), viewport, bounds, scale);
            assert_eq!(bounded_pan(limit, viewport, bounds, scale), limit);
            assert_eq!(
                limit.x, 0.0,
                "A branch that fits horizontally must not drift sideways"
            );
        }
        assert_eq!(short_state("PREREQUISITE"), "Locked");
        assert_eq!(short_state("LEARNED"), "Owned");
    }
    #[test]
    fn graph_canvas_stays_below_toolbar_in_horizontal_parent() {
        egui::__run_test_ui(|ui| {
            ui.allocate_ui_with_layout(
                vec2(650.0, 360.0),
                Layout::left_to_right(Align::Min),
                |ui| {
                    let origin = ui.cursor().min;
                    let id = ui.make_persistent_id("horizontal_graph_regression");
                    let rect = Graph::skills(&[]).show(ui, id, &mut View::default(), 280.0);
                    assert!(
                        (rect.left() - origin.x).abs() < 1.0,
                        "canvas must share toolbar's left edge"
                    );
                    assert!(rect.top() > origin.y, "canvas must start below toolbar");
                    assert!(
                        rect.width() > 600.0,
                        "toolbar must not consume canvas width"
                    );
                },
            );
        });
    }

    #[test]
    fn shallow_canvas_keeps_all_upgrade_icons_and_prices_in_view() {
        for defenses in [false, true] {
            let bounds = if defenses {
                vec2(850.0, 390.0)
            } else {
                vec2(620.0, 270.0)
            };
            let available = vec2(720.0, 195.0);
            let scale = fit_scale(available, bounds);
            for index in 0..if defenses { 12 } else { 6 } {
                let node = canvas_position(index, defenses, defenses, true) * scale;
                assert!(node.x.abs() + 32.0 * scale < available.x / 2.0);
                assert!(node.y - 32.0 * scale > -available.y / 2.0);
                assert!(node.y + 45.0 * scale + 11.0 < available.y / 2.0);
            }
        }
    }

    #[test]
    fn compact_graph_fits_its_authored_bounds() {
        for (available, bounds) in [
            (vec2(680.0, 300.0), vec2(740.0, 460.0)),
            (vec2(680.0, 325.0), vec2(930.0, 480.0)),
        ] {
            let scale = fit_scale(available, bounds);
            assert!(bounds.x * scale <= available.x);
            assert!(bounds.y * scale <= available.y);
        }
        for column in 0..3 {
            for rank in 0..3 {
                assert!(
                    defense_position(column, rank + 1).y > defense_position(column, rank).y + 120.0
                );
            }
        }
    }
    #[test]
    fn recommendation_selects_once_without_overriding_manual_selection() {
        let mut view = View::default();
        view.recommend(Some(3));
        assert_eq!(view.selected, 3);
        view.selected = 2;
        view.recommend(Some(3));
        assert_eq!(view.selected, 2);
        view.recommend(None);
        view.recommend(Some(3));
        assert_eq!(view.selected, 3);
    }
    #[test]
    fn zoom_is_bounded_and_skills_never_overlap() {
        let mut view = View::default();
        view.zoom_by(100.0);
        assert_eq!(view.zoom, 2.5);
        view.zoom_by(0.001);
        assert_eq!(view.zoom, 0.6);
        for a in 0..6 {
            for b in a + 1..6 {
                assert!((skill_position(a) - skill_position(b)).length() >= 190.0);
            }
        }
    }
    #[test]
    fn whole_catalogue_routes_every_real_prerequisite_and_reuses_geometry_across_purchase_states() {
        let catalog = horde_core::skill_catalog::skills();
        let mut nodes: Vec<_> = catalog
            .nodes
            .iter()
            .map(|n| Node {
                position: vec2(n.position[0], n.position[1]),
                title: n.name.clone(),
                state: "LOCKED".into(),
                icon: skill_icon(&n.icon),
                acquired: false,
                available: false,
                prerequisites: n
                    .prerequisites
                    .iter()
                    .map(|id| catalog.nodes.iter().position(|p| p.id == *id).unwrap())
                    .collect(),
            })
            .collect();
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
        nodes[1].acquired = true;
        nodes[2].available = true;
        assert!(std::sync::Arc::ptr_eq(&first, &cache.resolve(&nodes)));
        nodes[0].position.x += 5.0;
        assert!(!std::sync::Arc::ptr_eq(&first, &cache.resolve(&nodes)));
    }
    #[test]
    fn minimum_zoom_keeps_a_short_title_and_state_on_distinct_readable_rows() {
        let nodes = vec![Node {
            position: egui::Vec2::ZERO,
            title: "Controlled penetration training".into(),
            state: "LOCKED".into(),
            icon: Icon::Firearm,
            acquired: false,
            available: false,
            prerequisites: vec![],
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
                    160.0,
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
                if text.galley.text() == "LOCKED" {
                    assert!(canvas.contains_rect(rect));
                    state = Some(rect);
                }
            }
        }
        let title = title.expect("Node name must be painted at minimum scale");
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
            for scale in [MIN_SKILL_SCALE, 1.0, 1.4] {
                for node in &catalog.nodes {
                    let job = caption_job(&node.name, scale);
                    let width = job.wrap.max_width;
                    let galley = ui.painter().layout_job(job);
                    assert!(!galley.elided, "{} elided at {scale}", node.name);
                    assert!(galley.rows.len() <= TITLE_ROWS, "{}", node.name);
                    assert!(galley.rect.width() <= width + 0.1, "{}", node.name);
                    assert!(
                        galley.rect.center().x.abs() < 0.5,
                        "{} not centred",
                        node.name
                    );
                    checked += 1;
                }
            }
        });
        output.textures_delta.clear();
        // egui may run a pass more than once.
        assert!(checked > 0 && checked % (catalog.nodes.len() * 3) == 0);
        assert!(
            catalog
                .nodes
                .iter()
                .any(|n| n.name == "Survivor foundations")
        );
    }
}
