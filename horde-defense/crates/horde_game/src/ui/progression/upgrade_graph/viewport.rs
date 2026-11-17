use super::model::{CANVAS, Emphasis, Node, Tone, fade_fill};
use super::pictogram::{lock_mark, pictogram};
use super::wires::{
    LONG_EDGE, STUB_LENGTH, WireCache, label_spots, overlaps, route_length, segment_meets,
    skill_obstacle, stub_label, wire_stub,
};
use crate::art::{AMBER, INK, MUTED, TEXT};
use bevy_egui::egui::{self, Align, Align2, Color32, FontId, Rect, Sense, Stroke, pos2, vec2};

pub(super) const MIN_SCALE: f32 = 0.35;
pub(super) const MAX_SCALE: f32 = 1.6;
/// Below these scales captions would collide, so they are left out.
pub(super) const TITLE_SCALE: f32 = 0.55;
const BADGE_SCALE: f32 = 0.45;
/// How much further off its stub's leg, in points, a whole stub label may
/// go before it is shortened (CF-2-02).
const LABEL_LIFTS: [f32; 5] = [0.0, 8.0, 16.0, 24.0, 32.0];

/// Zoom, pan and selection. UI view state only; never saved in `Campaign`.
#[derive(Clone)]
pub(in crate::ui::progression) struct View {
    pub selected: usize,
    /// Pixels per world unit; 0 until the first canvas pass picks a default.
    pub(super) scale: f32,
    /// Screen offset of the layout centre from the canvas centre.
    pub(super) pan: egui::Vec2,
    pub(super) focus_selected: bool,
    /// A world position the focus centres instead of the selected node's
    /// (a review still that frames several rows); any pan, zoom, focus or
    /// frame clears it.
    pub(super) focus_point: Option<egui::Vec2>,
    pub(super) frame: Option<Rect>,
    pub(super) canvas_size: egui::Vec2,
}
impl Default for View {
    fn default() -> Self {
        Self {
            selected: 0,
            scale: 0.0,
            pan: egui::Vec2::ZERO,
            focus_selected: true,
            focus_point: None,
            frame: None,
            canvas_size: egui::Vec2::ZERO,
        }
    }
}
impl View {
    /// Select a node and centre it on the next pass.
    pub(in crate::ui::progression) fn focus(&mut self, index: usize) {
        self.selected = index;
        self.focus_selected = true;
        self.focus_point = None;
        self.frame = None;
    }
    /// Keep the selection but centre the world `position` (placed as a node
    /// position) on the next passes, until the view is moved.
    pub(in crate::ui::progression) fn focus_on_point(&mut self, position: egui::Vec2) {
        self.focus_selected = true;
        self.focus_point = Some(position);
        self.frame = None;
    }
    /// Fit a world-space rectangle (a branch) on the next pass.
    pub(in crate::ui::progression) fn frame_world(&mut self, world: Rect) {
        self.frame = Some(world);
        self.focus_selected = false;
        self.focus_point = None;
    }
    pub(super) fn scale(&self) -> f32 {
        if self.scale > 0.0 { self.scale } else { 1.0 }
    }
    /// Zoom around the canvas centre (toolbar buttons).
    pub(in crate::ui::progression) fn zoom_by(&mut self, multiplier: f32) {
        self.zoom_at(multiplier, egui::Vec2::ZERO);
    }
    /// Zoom keeping the world point under `anchor` (relative to the canvas
    /// centre) fixed on screen.
    pub(super) fn zoom_at(&mut self, multiplier: f32, anchor: egui::Vec2) {
        let old = self.scale();
        let new = (old * multiplier).clamp(MIN_SCALE, MAX_SCALE);
        self.pan = anchor - (anchor - self.pan) * (new / old);
        self.scale = new;
        self.focus_selected = false;
        self.focus_point = None;
        self.frame = None;
    }
}

/// A readable default: full size on wide canvases, a little smaller on narrow ones.
pub(super) fn default_scale(width: f32) -> f32 {
    (width / 1250.0).clamp(0.6, 1.0)
}

pub(in crate::ui::progression) struct Graph<'a> {
    nodes: &'a [Node],
}

impl<'a> Graph<'a> {
    pub(in crate::ui::progression) fn skills(nodes: &'a [Node]) -> Self {
        Self { nodes }
    }

    /// Draw the canvas into exactly `size` and handle wheel zoom, drag pan,
    /// node clicks and the minimap. Returns the canvas rectangle.
    pub(in crate::ui::progression) fn show(
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
            view.focus_point = None;
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
            && let Some(position) = view
                .focus_point
                .or_else(|| self.nodes.get(view.selected).map(|node| node.position))
        {
            view.pan = -(position - origin) * scale - vec2(0.0, 26.0 * scale);
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
        // Every wire drawn, in screen points, and the stubs whose labels go
        // on once every wire is down, so a label is placed clear of them all.
        let mut drawn: Vec<Vec<egui::Pos2>> = Vec::with_capacity(ordered.len());
        let mut labels = Vec::new();
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
            painter.add(egui::Shape::line(points.clone(), stroke));
            let wing = egui::vec2(-direction.y, direction.x) * 4.0;
            painter.line_segment([tip, tip - direction * 8.0 + wing], stroke);
            painter.line_segment([tip, tip - direction * 8.0 - wing], stroke);
            if long && scale >= TITLE_SCALE {
                let color = if related {
                    TEXT.gamma_multiply(0.85)
                } else {
                    MUTED.gamma_multiply(if searching { 0.4 } else { 0.8 })
                };
                labels.push((drawn.len(), parent, color));
            }
            drawn.push(points);
        }
        if !labels.is_empty() {
            let envelopes: Vec<Rect> = self
                .nodes
                .iter()
                .map(|node| {
                    let world = skill_obstacle(node);
                    Rect::from_two_pos(point(world.min.to_vec2()), point(world.max.to_vec2()))
                })
                .collect();
            let font = FontId::proportional((11.0 * scale).max(11.0));
            let mut placed: Vec<Rect> = Vec::with_capacity(labels.len());
            for (wire, parent, color) in labels {
                // A label meets no node's icon and caption, no other wire and
                // no label already placed (CF-2-02).
                let clear = |rect: Rect, placed: &[Rect]| {
                    !envelopes.iter().any(|&e| overlaps(rect, e))
                        && !placed.iter().any(|&p| overlaps(rect, p))
                        && drawn.iter().enumerate().all(|(i, points)| {
                            i == wire || !points.windows(2).any(|w| segment_meets(rect, w[0], w[1]))
                        })
                };
                let text = stub_label(&self.nodes[parent].title);
                let spots = label_spots(&drawn[wire], 0.0);
                let layout = |width: f32, rows: usize| {
                    let mut job =
                        egui::text::LayoutJob::simple(text.clone(), font.clone(), color, width);
                    job.wrap.max_rows = rows;
                    job.wrap.break_anywhere = rows == 1;
                    painter.layout_job(job)
                };
                let full = painter.layout_no_wrap(text.clone(), font.clone(), color);
                let width = full.size().x;
                // Whole on one row, then on two, at the first clear spot by
                // the stub, then up to `LABEL_LIFTS` further off its leg;
                // else shortened on one row by the stub's far end.
                let galleys = [full, layout(width * 0.6, 2)];
                let whole = LABEL_LIFTS.into_iter().find_map(|lift| {
                    let spots = label_spots(&drawn[wire], lift);
                    galleys.iter().find_map(|galley| {
                        spots.iter().find_map(|&(at, anchor)| {
                            let rect = anchor.anchor_size(at, galley.size());
                            clear(rect, &placed).then(|| (rect, galley.clone()))
                        })
                    })
                });
                let found = whole.or_else(|| {
                    let mut short = width;
                    while short > 4.0 * font.size {
                        short -= font.size;
                        let galley = layout(short, 1);
                        let spot = spots.iter().take(4).find_map(|&(at, anchor)| {
                            let rect = anchor.anchor_size(at, galley.size());
                            clear(rect, &placed).then_some(rect)
                        });
                        if let Some(rect) = spot {
                            return Some((rect, galley));
                        }
                    }
                    None
                });
                if let Some((rect, galley)) = found {
                    placed.push(rect);
                    painter.galley(rect.min, galley, color);
                }
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
            response.on_hover_text(node.tooltip());
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
            view.focus_point = None;
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
pub(super) fn minimap_rect(canvas: Rect, bounds: egui::Vec2) -> Option<Rect> {
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

pub(super) fn bounded_pan(
    pan: egui::Vec2,
    viewport: egui::Vec2,
    bounds: egui::Vec2,
    scale: f32,
) -> egui::Vec2 {
    let limit = ((bounds * scale - viewport) * 0.5).max(egui::Vec2::ZERO);
    pan.clamp(-limit, limit)
}
/// Node captions wrap to at most this many centred rows.
pub(super) const TITLE_ROWS: usize = 2;
/// A node name wraps onto a second centred line; only a pathological third
/// line is elided. Authored nodes are spaced for two caption rows.
pub(super) fn caption_job(title: &str, scale: f32, color: Color32) -> egui::text::LayoutJob {
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

/// Layout size (with caption margins) and centre.
pub(super) fn skill_bounds(nodes: &[Node]) -> (egui::Vec2, egui::Vec2) {
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
