//! Inspectable progression canvas. Geometry and view state are presentation-only:
//! node states, paths and search matches arrive from core queries already made.
use crate::art::{AMBER, INK, MUTED, TEXT};
use bevy_egui::egui::{self, Align, Align2, Color32, FontId, Rect, Sense, Stroke, pos2, vec2};
use horde_core::{campaign::Skill, content::StructureKind};
use std::collections::BTreeSet;

/// "Need N more coins": readable, but quieter than an error.
pub(super) const MUTED_RED: Color32 = Color32::from_rgb(201, 118, 100);
const CANVAS: Color32 = Color32::from_rgb(12, 17, 17);
/// Wires longer than this (world units) draw as short labelled stubs unless
/// they lie on the highlighted path, so the root does not span the canvas.
pub(super) const LONG_EDGE: f32 = 900.0;
const STUB_LENGTH: f32 = 70.0;
pub(super) const MIN_SCALE: f32 = 0.35;
pub(super) const MAX_SCALE: f32 = 1.6;
/// Below these scales captions would collide, so they are left out.
const TITLE_SCALE: f32 = 0.55;
const BADGE_SCALE: f32 = 0.45;

#[derive(Clone, Copy)]
pub(super) enum Icon {
    Skill(Skill),
    Firearm,
    Grenade,
    Flame,
    /// Endurance (stamina): a lightning bolt.
    Stamina,
    /// Perception (weak spots): an aiming sight.
    Sight,
    Defense(StructureKind, u32),
}

/// How a node reads on the canvas; derived from core `NodeState`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Tone {
    Owned,
    Buyable,
    NeedsCoins,
    Prerequisite,
    Milestone,
    Unavailable,
}
impl Tone {
    pub(super) fn locked(self) -> bool {
        matches!(
            self,
            Self::Prerequisite | Self::Milestone | Self::Unavailable
        )
    }
    pub(super) fn badge_color(self) -> Color32 {
        match self {
            Self::Owned | Self::Buyable => AMBER,
            Self::NeedsCoins => MUTED_RED,
            _ => MUTED,
        }
    }
    fn dot_color(self) -> Color32 {
        match self {
            Self::Owned => AMBER,
            Self::Buyable => TEXT,
            Self::NeedsCoins => MUTED_RED,
            _ => MUTED.gamma_multiply(0.55),
        }
    }
}

pub(super) struct Node {
    pub position: egui::Vec2,
    pub title: String,
    /// Short canvas caption: "OWNED", "40 c", "40 c · need 15", "Locked", "Win 3".
    pub badge: String,
    pub tone: Tone,
    pub icon: Icon,
    pub prerequisites: Vec<usize>,
}

/// Per-frame emphasis the screen computed from core queries.
#[derive(Default)]
pub(super) struct Emphasis {
    /// The selected node's unowned chain, the target included.
    pub path: BTreeSet<usize>,
    /// Search matches while a query is active; everything else dims.
    pub matches: Option<BTreeSet<usize>>,
    /// A just-learned node and its pulse phase in 0..1.
    pub pulse: Option<(usize, f32)>,
}

/// Zoom, pan and selection. UI view state only; never saved in `Campaign`.
#[derive(Clone)]
pub(super) struct View {
    pub selected: usize,
    /// Pixels per world unit; 0 until the first canvas pass picks a default.
    scale: f32,
    /// Screen offset of the layout centre from the canvas centre.
    pan: egui::Vec2,
    focus_selected: bool,
    frame: Option<Rect>,
    canvas_size: egui::Vec2,
}
impl Default for View {
    fn default() -> Self {
        Self {
            selected: 0,
            scale: 0.0,
            pan: egui::Vec2::ZERO,
            focus_selected: true,
            frame: None,
            canvas_size: egui::Vec2::ZERO,
        }
    }
}
impl View {
    /// Select a node and centre it on the next pass.
    pub(super) fn focus(&mut self, index: usize) {
        self.selected = index;
        self.focus_selected = true;
        self.frame = None;
    }
    /// Fit a world-space rectangle (a branch) on the next pass.
    pub(super) fn frame_world(&mut self, world: Rect) {
        self.frame = Some(world);
        self.focus_selected = false;
    }
    pub(super) fn scale(&self) -> f32 {
        if self.scale > 0.0 { self.scale } else { 1.0 }
    }
    /// Zoom around the canvas centre (toolbar buttons).
    pub(super) fn zoom_by(&mut self, multiplier: f32) {
        self.zoom_at(multiplier, egui::Vec2::ZERO);
    }
    /// Zoom keeping the world point under `anchor` (relative to the canvas
    /// centre) fixed on screen.
    fn zoom_at(&mut self, multiplier: f32, anchor: egui::Vec2) {
        let old = self.scale();
        let new = (old * multiplier).clamp(MIN_SCALE, MAX_SCALE);
        self.pan = anchor - (anchor - self.pan) * (new / old);
        self.scale = new;
        self.focus_selected = false;
        self.frame = None;
    }
}

/// A readable default: full size on wide canvases, a little smaller on narrow ones.
fn default_scale(width: f32) -> f32 {
    (width / 1250.0).clamp(0.6, 1.0)
}

pub(super) struct Graph<'a> {
    nodes: &'a [Node],
}
type Routes = std::sync::Arc<std::collections::BTreeMap<(usize, usize), Vec<egui::Vec2>>>;
#[derive(Clone, Default)]
struct WireCache {
    layout: Option<u64>,
    routes: Routes,
}
impl WireCache {
    fn resolve(&mut self, nodes: &[Node]) -> Routes {
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

/// Blend `color` toward the canvas so faded icons stay opaque over wires.
fn fade_fill(color: Color32, fade: f32) -> Color32 {
    let mix = |a: u8, b: u8| (f32::from(b) + (f32::from(a) - f32::from(b)) * fade).round() as u8;
    Color32::from_rgb(
        mix(color.r(), CANVAS.r()),
        mix(color.g(), CANVAS.g()),
        mix(color.b(), CANVAS.b()),
    )
}

impl<'a> Graph<'a> {
    pub(super) fn skills(nodes: &'a [Node]) -> Self {
        Self { nodes }
    }

    /// Draw the canvas into exactly `size` and handle wheel zoom, drag pan,
    /// node clicks and the minimap. Returns the canvas rectangle.
    pub(super) fn show(
        &self,
        ui: &mut egui::Ui,
        id: egui::Id,
        view: &mut View,
        emphasis: &Emphasis,
        size: egui::Vec2,
    ) -> Rect {
        let (rect, canvas) = ui.allocate_exact_size(size, Sense::click_and_drag());
        let (bounds, origin) = skill_bounds(self.nodes);
        if view.scale <= 0.0 {
            view.scale = default_scale(rect.width());
            view.focus_selected = true;
        }
        if view.canvas_size != egui::Vec2::ZERO && (view.canvas_size - rect.size()).length() > 1.0 {
            view.focus_selected = true;
        }
        if canvas.dragged() {
            view.pan += canvas.drag_delta();
            view.focus_selected = false;
            view.frame = None;
        }
        if let Some(pointer) = ui
            .input(|i| i.pointer.hover_pos())
            .filter(|p| rect.contains(*p) && ui.clip_rect().contains(*p))
        {
            let (wheel, pinch) = ui.input_mut(|i| {
                let delta = i.smooth_scroll_delta;
                i.smooth_scroll_delta = egui::Vec2::ZERO;
                (delta, i.zoom_delta())
            });
            let factor = pinch * (wheel.y * 0.002).exp();
            if (factor - 1.0).abs() > 1e-4 {
                view.zoom_at(factor, pointer - rect.center());
            }
            if wheel.x != 0.0 {
                view.pan.x += wheel.x;
            }
        }
        if let Some(world) = view.frame.take() {
            let fit =
                fit_scale(rect.size(), world.size() + vec2(240.0, 200.0)).clamp(MIN_SCALE, 1.0);
            view.scale = fit;
            let mut target = world.center().to_vec2();
            // A branch taller or wider than the canvas starts at its entry.
            if (world.height() + 200.0) * fit > rect.height() {
                target.y = world.top() - 60.0 + rect.height() * 0.5 / fit;
            }
            if (world.width() + 240.0) * fit > rect.width() {
                target.x = world.left() - 100.0 + rect.width() * 0.5 / fit;
            }
            view.pan = -(target - origin) * fit;
        }
        let scale = view.scale;
        if view.focus_selected
            && let Some(node) = self.nodes.get(view.selected)
        {
            view.pan = -(node.position - origin) * scale - vec2(0.0, 26.0 * scale);
        }
        view.pan = bounded_pan(view.pan, rect.size(), bounds, scale);
        view.focus_selected = false;
        view.canvas_size = rect.size();
        let center = rect.center() + view.pan;
        let point = |v: egui::Vec2| center + (v - origin) * scale;
        let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
        painter.rect_filled(rect, 0.0, CANVAS);
        painter.rect_stroke(
            rect,
            0.0,
            Stroke::new(1.0, MUTED.gamma_multiply(0.45)),
            egui::StrokeKind::Inside,
        );
        let selected = view.selected;
        let searching = emphasis.matches.is_some();
        let routes = ui.ctx().data_mut(|d| {
            d.get_temp_mut_or_default::<WireCache>(id.with("authored_wire_routes"))
                .resolve(self.nodes)
        });
        // Unrelated wires first, so the path and the selection draw on top.
        let mut ordered: Vec<_> = routes.iter().collect();
        ordered.sort_by_key(|((parent, child), _)| {
            usize::from(*child == selected || *parent == selected)
                + 2 * usize::from(emphasis.path.contains(child))
        });
        for (&(parent, child), route) in ordered {
            if route.len() < 2 {
                continue;
            }
            let on_path = emphasis.path.contains(&child)
                && (emphasis.path.contains(&parent) || self.nodes[parent].tone == Tone::Owned);
            let related = child == selected || parent == selected;
            let (mut color, width) = if on_path {
                (AMBER, 2.5)
            } else if related {
                (TEXT.gamma_multiply(0.85), 1.6)
            } else if self.nodes[child].tone == Tone::Owned {
                (AMBER.gamma_multiply(0.5), 1.4)
            } else {
                (MUTED.gamma_multiply(0.3), 1.2)
            };
            if searching && !on_path && !related {
                color = color.gamma_multiply(0.5);
            }
            let stroke = Stroke::new(width, color);
            let long = route_length(route) > LONG_EDGE && !on_path;
            let world: Vec<_> = if long {
                wire_stub(route, STUB_LENGTH)
            } else {
                route.clone()
            };
            let points: Vec<_> = world.iter().map(|v| point(*v)).collect();
            if points.len() < 2 {
                continue;
            }
            let tip = *points.last().expect("nonempty route");
            let direction = (tip - points[points.len() - 2]).normalized();
            let start = points[0];
            // The label sits beyond the stub's far end, along its overall
            // direction, so a stub with a short bend never puts the label
            // over the child's icon.
            let lead = (tip - start).normalized();
            painter.add(egui::Shape::line(points, stroke));
            let wing = egui::vec2(-direction.y, direction.x) * 4.0;
            painter.line_segment([tip, tip - direction * 8.0 + wing], stroke);
            painter.line_segment([tip, tip - direction * 8.0 - wing], stroke);
            if long && scale >= TITLE_SCALE {
                let (anchor, offset) = if lead.x > 0.5 {
                    (Align2::RIGHT_CENTER, vec2(-4.0, 0.0))
                } else if lead.x < -0.5 {
                    (Align2::LEFT_CENTER, vec2(4.0, 0.0))
                } else if lead.y > 0.0 {
                    (Align2::CENTER_BOTTOM, vec2(0.0, -3.0))
                } else {
                    (Align2::CENTER_TOP, vec2(0.0, 3.0))
                };
                painter.text(
                    start + offset,
                    anchor,
                    stub_label(&self.nodes[parent].title),
                    FontId::proportional((11.0 * scale).max(11.0)),
                    if related {
                        TEXT.gamma_multiply(0.85)
                    } else {
                        MUTED.gamma_multiply(if searching { 0.4 } else { 0.8 })
                    },
                );
            }
        }
        let mut clicked = canvas.clicked();
        for (index, node) in self.nodes.iter().enumerate() {
            let node_center = point(node.position);
            let icon_rect = Rect::from_center_size(node_center, vec2(64.0, 64.0) * scale);
            let hit_rect = Rect::from_center_size(
                node_center + vec2(0.0, 24.0) * scale,
                vec2(160.0, 122.0) * scale,
            )
            .intersect(rect);
            if !hit_rect.is_positive() {
                continue;
            }
            // Arrow keys move the selection, so nodes are not Tab stops.
            let response = ui.interact(hit_rect, id.with(("node", index)), Sense::CLICK);
            response.widget_info(|| {
                egui::WidgetInfo::selected(
                    egui::WidgetType::Button,
                    true,
                    view.selected == index,
                    format!("{}: {}. Select to inspect.", node.title, node.badge),
                )
            });
            if response.clicked() {
                view.selected = index;
                clicked = true;
            }
            let is_selected = view.selected == index;
            let matched = emphasis.matches.as_ref().map(|m| m.contains(&index));
            let mut fade: f32 = if matched == Some(false) {
                0.22
            } else if node.tone.locked() {
                0.5
            } else {
                1.0
            };
            if is_selected {
                fade = fade.max(0.9);
            } else if response.hovered() {
                fade = fade.max(0.75);
            }
            let (fill, edge, glyph, edge_width) = match node.tone {
                Tone::Owned => (AMBER, AMBER, INK, 1.5),
                Tone::Buyable => (INK, TEXT, TEXT, 2.0),
                _ => (INK, MUTED, MUTED, 1.0),
            };
            painter.rect_filled(icon_rect, 0.0, fade_fill(fill, fade));
            painter.rect_stroke(
                icon_rect,
                0.0,
                Stroke::new(edge_width, edge.gamma_multiply(fade)),
                egui::StrokeKind::Inside,
            );
            if is_selected {
                painter.rect_stroke(
                    icon_rect.expand(5.0 * scale),
                    0.0,
                    Stroke::new(2.0, AMBER),
                    egui::StrokeKind::Inside,
                );
            } else if emphasis.path.contains(&index) {
                painter.rect_stroke(
                    icon_rect.expand(4.0 * scale),
                    0.0,
                    Stroke::new(1.5, AMBER.gamma_multiply(0.75)),
                    egui::StrokeKind::Inside,
                );
            }
            if matched == Some(true) {
                painter.rect_stroke(
                    icon_rect.expand(9.0 * scale),
                    0.0,
                    Stroke::new(1.0, AMBER),
                    egui::StrokeKind::Inside,
                );
            }
            if let Some((pulse, phase)) = emphasis.pulse
                && pulse == index
            {
                painter.rect_stroke(
                    icon_rect.expand((6.0 + 22.0 * phase) * scale),
                    0.0,
                    Stroke::new(3.0, AMBER.gamma_multiply(1.0 - phase)),
                    egui::StrokeKind::Outside,
                );
            }
            pictogram(
                &painter,
                icon_rect.shrink(12.0 * scale),
                node.icon,
                if node.tone == Tone::Owned {
                    glyph
                } else {
                    glyph.gamma_multiply(fade)
                },
            );
            if node.tone.locked() {
                lock_mark(
                    &painter,
                    icon_rect.right_top() + vec2(-2.0, 2.0) * scale,
                    scale,
                    MUTED.gamma_multiply(fade.max(0.6)),
                );
            }
            let mut caption_bottom = icon_rect.bottom() + 4.0;
            if scale >= TITLE_SCALE {
                let galley =
                    painter.layout_job(caption_job(&node.title, scale, TEXT.gamma_multiply(fade)));
                let title_top = icon_rect.bottom() + 7.0;
                caption_bottom = title_top + galley.size().y;
                // Centre-aligned galleys are laid out around x = 0.
                painter.galley(pos2(node_center.x, title_top), galley, TEXT);
            }
            if scale >= BADGE_SCALE {
                painter.text(
                    pos2(node_center.x, caption_bottom + 3.0),
                    Align2::CENTER_TOP,
                    &node.badge,
                    FontId::proportional((12.0 * scale).max(11.0)),
                    node.tone.badge_color().gamma_multiply(fade.max(0.45)),
                );
            }
            response.on_hover_text(format!(
                "{} · {}\nSelect for effects and purchase action.",
                node.title, node.badge
            ));
        }
        if self.minimap(ui, id, view, emphasis, rect, (bounds, origin)) {
            clicked = true;
        }
        if clicked {
            // The canvas takes keyboard input (arrows, Enter) back from fields.
            ui.memory_mut(|m| {
                if let Some(focused) = m.focused() {
                    m.surrender_focus(focused);
                }
            });
        }
        rect
    }

    /// Corner overview: every node as a state-coloured dot plus the viewport.
    /// Click or drag it to move the view. Returns whether it was used.
    fn minimap(
        &self,
        ui: &mut egui::Ui,
        id: egui::Id,
        view: &mut View,
        emphasis: &Emphasis,
        canvas: Rect,
        (bounds, origin): (egui::Vec2, egui::Vec2),
    ) -> bool {
        let Some(mini) = minimap_rect(canvas, bounds) else {
            return false;
        };
        let response = ui.interact(mini, id.with("minimap"), Sense::click_and_drag());
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Other, true, "Tree overview")
        });
        let world_min = origin - bounds * 0.5;
        let k = (mini.width() / bounds.x).min(mini.height() / bounds.y);
        let inset = (mini.size() - bounds * k) * 0.5;
        let to_mini = |w: egui::Vec2| mini.min + inset + (w - world_min) * k;
        let used = response.clicked() || response.dragged();
        if used && let Some(pointer) = response.interact_pointer_pos() {
            let world = world_min + (pointer - mini.min - inset) / k;
            view.pan = bounded_pan(
                -(world - origin) * view.scale,
                canvas.size(),
                bounds,
                view.scale,
            );
            view.focus_selected = false;
            view.frame = None;
            ui.ctx().request_repaint();
        }
        let painter = ui.painter().with_clip_rect(mini.intersect(ui.clip_rect()));
        // Translucent at rest so nodes under the corner stay readable; opaque
        // while the pointer is on it, when the overview is what you read.
        let backdrop = if response.hovered() || response.dragged() {
            Color32::from_rgb(7, 11, 11)
        } else {
            Color32::from_rgba_unmultiplied(7, 11, 11, 150)
        };
        painter.rect_filled(mini, 0.0, backdrop);
        painter.rect_stroke(
            mini,
            0.0,
            Stroke::new(
                1.0,
                MUTED.gamma_multiply(if response.hovered() { 0.9 } else { 0.55 }),
            ),
            egui::StrokeKind::Inside,
        );
        for (index, node) in self.nodes.iter().enumerate() {
            let dim = emphasis
                .matches
                .as_ref()
                .is_some_and(|m| !m.contains(&index));
            let color = if emphasis.path.contains(&index) {
                AMBER
            } else {
                node.tone.dot_color()
            };
            painter.circle_filled(
                to_mini(node.position),
                1.8,
                if dim {
                    color.gamma_multiply(0.25)
                } else {
                    color
                },
            );
        }
        if let Some(node) = self.nodes.get(view.selected) {
            painter.circle_stroke(to_mini(node.position), 4.0, Stroke::new(1.5, AMBER));
        }
        let half = canvas.size() * 0.5 / view.scale;
        let centre = origin - view.pan / view.scale;
        let viewport =
            Rect::from_min_max(to_mini(centre - half), to_mini(centre + half)).intersect(mini);
        painter.rect_stroke(
            viewport,
            0.0,
            Stroke::new(1.0, TEXT),
            egui::StrokeKind::Inside,
        );
        used
    }
}

/// Bottom-right overview rectangle, sized to the layout's aspect ratio and
/// kept small so it covers only a few percent of the canvas.
fn minimap_rect(canvas: Rect, bounds: egui::Vec2) -> Option<Rect> {
    let width = (canvas.width() * 0.16).min(150.0);
    let height = (width * bounds.y / bounds.x).min(canvas.height() * 0.25);
    let width = width.min(height * bounds.x / bounds.y);
    (width >= 60.0 && height >= 40.0).then(|| {
        Rect::from_min_size(
            canvas.right_bottom() - vec2(width + 8.0, height + 8.0),
            vec2(width, height),
        )
    })
}

/// A small padlock at the icon's top-right corner.
fn lock_mark(painter: &egui::Painter, corner: egui::Pos2, scale: f32, color: Color32) {
    let unit = (scale * 1.0).max(0.7);
    let body = Rect::from_min_size(corner + vec2(-14.0, 7.0) * unit, vec2(11.0, 8.0) * unit);
    let stroke = Stroke::new(1.2, color);
    painter.rect_filled(body.expand(2.0), 0.0, CANVAS);
    painter.rect_filled(body, 0.0, color);
    let left = body.left() + 2.5 * unit;
    let right = body.right() - 2.5 * unit;
    let top = body.top() - 4.0 * unit;
    painter.line_segment([pos2(left, body.top()), pos2(left, top)], stroke);
    painter.line_segment([pos2(right, body.top()), pos2(right, top)], stroke);
    painter.line_segment([pos2(left, top), pos2(right, top)], stroke);
}

pub(super) fn stub_label(parent: &str) -> String {
    // The pixel font has no arrow glyphs; the stub's own arrowhead shows direction.
    format!("from {parent}")
}

fn route_length(route: &[egui::Vec2]) -> f32 {
    route.windows(2).map(|w| (w[1] - w[0]).length()).sum()
}

/// The last `length` world units of a route, ending at the child.
fn wire_stub(route: &[egui::Vec2], length: f32) -> Vec<egui::Vec2> {
    let mut stub = vec![*route.last().expect("nonempty route")];
    let mut left = length;
    for pair in route.windows(2).rev() {
        let (a, b) = (pair[0], pair[1]);
        let segment = (b - a).length();
        if segment >= left {
            stub.push(b + (a - b).normalized() * left);
            break;
        }
        left -= segment;
        stub.push(a);
    }
    stub.reverse();
    stub
}

/// The node nearest to `from` in screen direction `dir` (a unit vector), or
/// `None` at the edge of the layout. Sideways distance costs more than reach.
pub(super) fn nearest_in_direction(nodes: &[Node], from: usize, dir: egui::Vec2) -> Option<usize> {
    let origin = nodes.get(from)?.position;
    nodes
        .iter()
        .enumerate()
        .filter(|(index, _)| *index != from)
        .filter_map(|(index, node)| {
            let delta = node.position - origin;
            let along = delta.dot(dir);
            if along <= 1.0 {
                return None;
            }
            let across = (delta - dir * along).length();
            Some((index, along + across * 2.5))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(index, _)| index)
}

fn bounded_pan(
    pan: egui::Vec2,
    viewport: egui::Vec2,
    bounds: egui::Vec2,
    scale: f32,
) -> egui::Vec2 {
    let limit = ((bounds * scale - viewport) * 0.5).max(egui::Vec2::ZERO);
    pan.clamp(-limit, limit)
}
/// Node captions wrap to at most this many centred rows.
const TITLE_ROWS: usize = 2;
/// A node name wraps onto a second centred line; only a pathological third
/// line is elided. Authored nodes are spaced for two caption rows.
fn caption_job(title: &str, scale: f32, color: Color32) -> egui::text::LayoutJob {
    let width = (160.0 * scale).clamp(120.0, 180.0);
    let mut job = egui::text::LayoutJob::simple(
        title.to_owned(),
        FontId::proportional((15.0 * scale).clamp(12.0, 20.0)),
        color,
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
            let cost: f32 = route_length(&path) + path.len() as f32 * 8.0;
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
                        let cost = route_length(&path) + path.len() as f32 * 8.0;
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
        // Body development: each attribute reuses the pictogram of what it raises.
        "vitality" => Icon::Skill(Skill::SecondWind),
        "strength" => Icon::Skill(Skill::Sweep),
        "coordination" => Icon::Skill(Skill::QuickReload),
        "endurance" => Icon::Stamina,
        "perception" => Icon::Sight,
        _ => Icon::Firearm,
    }
}
/// Layout size (with caption margins) and centre.
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
        Icon::Stamina => {
            for (a, b) in [
                ([15.0, 2.0], [6.0, 13.0]),
                ([6.0, 13.0], [12.0, 13.0]),
                ([12.0, 13.0], [9.0, 22.0]),
                ([9.0, 22.0], [18.0, 10.0]),
                ([18.0, 10.0], [12.0, 10.0]),
                ([12.0, 10.0], [15.0, 2.0]),
            ] {
                segment(a, b);
            }
        }
        Icon::Sight => {
            p.circle_stroke(point(12.0, 12.0), 7.0 * unit, stroke);
            segment([12.0, 1.0], [12.0, 8.0]);
            segment([12.0, 16.0], [12.0, 23.0]);
            segment([1.0, 12.0], [8.0, 12.0]);
            segment([16.0, 12.0], [23.0, 12.0]);
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
        let mut by_attribute: BTreeMap<String, std::collections::BTreeSet<String>> =
            BTreeMap::new();
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
}
