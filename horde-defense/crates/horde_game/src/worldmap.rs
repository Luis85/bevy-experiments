use crate::art::{AMBER, GREEN, MUTED, RED, TEXT};
use bevy_egui::egui::{self, Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, Vec2, pos2, vec2};
use horde_core::{sim::Game, world::world};

fn project(r: Rect, p: [f32; 2]) -> Pos2 {
    pos2(
        r.left() + (p[0] + 180.0) / 360.0 * r.width(),
        r.top() + (85.0 - p[1]) / 145.0 * r.height(),
    )
}
fn inside(p: Pos2, poly: &[Pos2]) -> bool {
    let mut hit = false;
    if poly.len() < 3 {
        return false;
    }
    let mut j = poly.len() - 1;
    for i in 0..poly.len() {
        let a = poly[i];
        let b = poly[j];
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            hit = !hit;
        }
        j = i;
    }
    hit
}
fn cross(a: Pos2, b: Pos2, c: Pos2) -> f64 {
    let (ax, ay) = (f64::from(a.x), f64::from(a.y));
    let (bx, by) = (f64::from(b.x), f64::from(b.y));
    let (cx, cy) = (f64::from(c.x), f64::from(c.y));
    (bx - ax) * (cy - ay) - (by - ay) * (cx - ax)
}
fn triangulate(points: &[Pos2]) -> Vec<u32> {
    // GeoJSON closes every ring, and coordinate rounding can produce additional
    // adjacent duplicate vertices. Neither is a distinct corner of the polygon.
    // Keep original indices because the mesh still uses the original vertices.
    let mut live: Vec<usize> = Vec::with_capacity(points.len());
    for (index, point) in points.iter().enumerate() {
        if !point.x.is_finite() || !point.y.is_finite() {
            return vec![];
        }
        if live.last().is_none_or(|last| points[*last] != *point) {
            live.push(index);
        }
    }
    if live.len() > 1 && points[live[0]] == points[*live.last().expect("nonempty ring")] {
        live.pop();
    }
    if live.len() < 3 {
        return vec![];
    }
    let min_x = live
        .iter()
        .map(|i| f64::from(points[*i].x))
        .fold(f64::INFINITY, f64::min);
    let max_x = live
        .iter()
        .map(|i| f64::from(points[*i].x))
        .fold(f64::NEG_INFINITY, f64::max);
    let min_y = live
        .iter()
        .map(|i| f64::from(points[*i].y))
        .fold(f64::INFINITY, f64::min);
    let max_y = live
        .iter()
        .map(|i| f64::from(points[*i].y))
        .fold(f64::NEG_INFINITY, f64::max);
    let epsilon = f64::EPSILON * (max_x - min_x).max(max_y - min_y).powi(2).max(1.0) * 64.0;
    // Removing a collinear middle point preserves the polygon's signed area.
    // This also removes zero-area backtracking slivers from source rounding.
    loop {
        if live.len() < 3 {
            return vec![];
        }
        let redundant = (0..live.len()).find(|&k| {
            cross(
                points[live[(k + live.len() - 1) % live.len()]],
                points[live[k]],
                points[live[(k + 1) % live.len()]],
            )
            .abs()
                <= epsilon
        });
        if let Some(k) = redundant {
            live.remove(k);
        } else {
            break;
        }
    }
    let origin = points[live[0]];
    let area: f64 = live
        .windows(2)
        .map(|pair| cross(origin, points[pair[0]], points[pair[1]]))
        .sum();
    if area.abs() <= epsilon {
        return vec![];
    }
    let sign = area.signum();
    let mut triangles = Vec::with_capacity((live.len() - 2) * 3);
    while live.len() > 3 {
        let ear = (0..live.len()).find(|&k| {
            let a = live[(k + live.len() - 1) % live.len()];
            let b = live[k];
            let c = live[(k + 1) % live.len()];
            if cross(points[a], points[b], points[c]) * sign <= epsilon {
                return false;
            }
            // Boundary points must block an ear too: clipping across one can
            // create crossing diagonals and leave an untriangulatable remainder.
            !live
                .iter()
                .copied()
                .filter(|v| *v != a && *v != b && *v != c)
                .any(|v| {
                    let q = points[v];
                    cross(points[a], points[b], q) * sign >= -epsilon
                        && cross(points[b], points[c], q) * sign >= -epsilon
                        && cross(points[c], points[a], q) * sign >= -epsilon
                })
        });
        let Some(k) = ear else {
            // Never render a partial fill for invalid externally authored rings.
            return vec![];
        };
        triangles.extend([
            live[(k + live.len() - 1) % live.len()] as u32,
            live[k] as u32,
            live[(k + 1) % live.len()] as u32,
        ]);
        live.remove(k);
    }
    triangles.extend(live.into_iter().map(|index| index as u32));
    triangles
}

pub struct MapCache {
    size: Vec2,
    polygons: Vec<Vec<(Vec<Pos2>, Vec<u32>)>>,
    zoom: f32,
    pan: Vec2,
}
impl Default for MapCache {
    fn default() -> Self {
        Self {
            size: Vec2::ZERO,
            polygons: vec![],
            zoom: 1.,
            pan: Vec2::ZERO,
        }
    }
}
impl MapCache {
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        game: &Game,
        selected: &mut String,
    ) -> Option<String> {
        let data = world();
        ui.horizontal(|ui| {
            if ui.small_button("-").on_hover_text("Zoom out").clicked() {
                self.zoom = (self.zoom / 1.35).max(1.);
            }
            ui.label(format!("{:.0}%", self.zoom * 100.));
            if ui.small_button("+").on_hover_text("Zoom in").clicked() {
                self.zoom = (self.zoom * 1.35).min(16.);
            }
            if ui.small_button("Reset map").clicked() {
                self.zoom = 1.;
                self.pan = Vec2::ZERO;
            }
            ui.label(
                egui::RichText::new("Scroll to zoom · drag to explore")
                    .small()
                    .color(MUTED),
            );
        });
        let size = vec2(
            ui.available_width(),
            (ui.available_width() * 0.49)
                .min(ui.available_height() - 40.0)
                .max(220.0),
        );
        if self.size != size || self.polygons.is_empty() {
            self.size = size;
            let rr = Rect::from_min_size(Pos2::ZERO, size);
            self.polygons = data
                .countries
                .iter()
                .map(|c| {
                    c.polygons
                        .iter()
                        .filter_map(|poly| {
                            let pts: Vec<_> = poly
                                .iter()
                                .filter(|p| p[1] >= -60.0)
                                .map(|p| project(rr, *p))
                                .collect();
                            if pts.len() < 3 {
                                return None;
                            }
                            // Dateline-spanning rings are drawn as outlines rather than a triangle across the map.
                            let span = pts.iter().map(|p| p.x).fold(f32::NEG_INFINITY, f32::max)
                                - pts.iter().map(|p| p.x).fold(f32::INFINITY, f32::min);
                            let triangles = if span > size.x * 0.7 {
                                vec![]
                            } else {
                                triangulate(&pts)
                            };
                            Some((pts, triangles))
                        })
                        .collect()
                })
                .collect();
        }
        let (rect, response) = ui.allocate_exact_size(size, Sense::click_and_drag());
        if response.hovered() {
            let scroll = ui.input_mut(|input| {
                let value = input.smooth_scroll_delta.y;
                input.smooth_scroll_delta.y = 0.;
                value
            });
            if scroll != 0. {
                let old = self.zoom;
                self.zoom = (self.zoom * (scroll * 0.004).exp()).clamp(1., 16.);
                let focus = response.hover_pos().unwrap_or(rect.center()) - rect.min;
                self.pan = focus - (focus - self.pan) * (self.zoom / old);
            }
        }
        if response.dragged() {
            self.pan += response.drag_delta();
        }
        self.pan.x = self.pan.x.clamp(size.x * (1. - self.zoom), 0.);
        self.pan.y = self.pan.y.clamp(size.y * (1. - self.zoom), 0.);
        let view = Rect::from_min_size(rect.min + self.pan, size * self.zoom);
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, Color32::from_rgb(22, 39, 42));
        for lat in [-45.0, 0.0, 45.0] {
            let y = project(view, [0.0, lat]).y;
            painter.line_segment(
                [pos2(rect.left(), y), pos2(rect.right(), y)],
                Stroke::new(0.5, Color32::from_rgb(44, 61, 61)),
            );
        }
        for lon in [-120.0, -60.0, 0.0, 60.0, 120.0] {
            let x = project(view, [lon, 0.0]).x;
            painter.line_segment(
                [pos2(x, rect.top()), pos2(x, rect.bottom())],
                Stroke::new(0.5, Color32::from_rgb(44, 61, 61)),
            );
        }
        let pointer = response.hover_pos();
        let mut hover: Option<usize> = None;
        for (n, country) in data.countries.iter().enumerate() {
            let active = game.campaign.world.active.as_deref() == Some(country.id.as_str());
            let cleared = game.campaign.world.cleared.contains(&country.id);
            let accessible = game.campaign.world.can_enter(&country.id);
            let chosen = country.id == *selected;
            let color = if chosen {
                AMBER
            } else if cleared {
                GREEN.gamma_multiply(0.6)
            } else if active {
                Color32::from_rgb(153, 159, 104)
            } else if accessible {
                Color32::from_rgb(103, 130, 106)
            } else {
                Color32::from_rgb(62, 79, 73)
            };
            for (points, indices) in &self.polygons[n] {
                let pts: Vec<_> = points
                    .iter()
                    .map(|p| pos2(p.x * self.zoom + view.left(), p.y * self.zoom + view.top()))
                    .collect();
                if pointer.is_some_and(|p| inside(p, &pts)) {
                    hover = Some(n);
                }
                if !indices.is_empty() {
                    let mesh = egui::Mesh {
                        indices: indices.clone(),
                        vertices: pts
                            .iter()
                            .map(|pos| egui::epaint::Vertex {
                                pos: *pos,
                                uv: egui::epaint::WHITE_UV,
                                color,
                            })
                            .collect(),
                        ..Default::default()
                    };
                    painter.add(egui::Shape::mesh(mesh));
                }
                for edge in pts.windows(2) {
                    if edge[0].distance(edge[1]) < view.width() * 0.4 {
                        painter.line_segment(
                            [edge[0], edge[1]],
                            Stroke::new(
                                if chosen { 1.4 } else { 0.6 },
                                if chosen {
                                    TEXT
                                } else {
                                    Color32::from_rgb(28, 45, 44)
                                },
                            ),
                        );
                    }
                }
            }
            if chosen || active {
                let at = project(view, country.center);
                painter.circle_filled(at, 3.0, AMBER);
                painter.circle_stroke(at, 6.0, Stroke::new(1.0, AMBER));
            }
        }
        if let Some(country) = data.country(selected) {
            for neighbor in &country.neighbors {
                if let Some(other) = data.country(neighbor) {
                    let a = project(view, country.center);
                    let b = project(view, other.center);
                    if a.distance(b) < view.width() * 0.45 {
                        painter.line_segment([a, b], Stroke::new(1.0, AMBER.gamma_multiply(0.65)));
                        painter.circle_filled(b, 2.0, AMBER);
                    }
                }
            }
        }
        for alert in game
            .strategy
            .outbreaks
            .iter()
            .filter(|o| o.outcome == horde_core::strategy::OutbreakOutcome::Active)
        {
            if let Some(c) = data.country(&alert.country_id) {
                let at = project(view, c.center);
                painter.circle_stroke(at, 10.0, Stroke::new(2.0, RED));
                painter.text(
                    at,
                    Align2::CENTER_CENTER,
                    "!",
                    FontId::proportional(14.0),
                    TEXT,
                );
            }
        }
        for country in &data.countries {
            let guards = game
                .strategy
                .guards
                .iter()
                .filter(|g| g.country_id == country.id)
                .count();
            let deployments = game
                .battles
                .iter()
                .filter(|b| !b.settled && b.country_id == country.id)
                .count();
            if guards == 0 && deployments == 0 {
                continue;
            }
            let center = project(view, country.center);
            let at = center + vec2(10., -13.);
            let label = if guards > 0 {
                format!("G{guards}")
            } else {
                format!("D{deployments}")
            };
            let badge = Rect::from_center_size(at, vec2(24., 18.));
            painter.rect_filled(badge, 2., Color32::from_rgb(25, 43, 37));
            painter.rect_stroke(
                badge,
                2.,
                Stroke::new(1., if guards > 0 { GREEN } else { AMBER }),
                egui::StrokeKind::Inside,
            );
            painter.text(
                at,
                Align2::CENTER_CENTER,
                label,
                FontId::proportional(11.),
                TEXT,
            );
        }
        if let Some(index) = hover {
            let country = &data.countries[index];
            if let Some(p) = pointer {
                painter.text(
                    pos2(
                        (p.x + 12.).min(rect.right() - 120.),
                        (p.y - 15.).max(rect.top() + 20.),
                    ),
                    Align2::LEFT_BOTTOM,
                    &country.name,
                    FontId::proportional(14.0),
                    TEXT,
                );
            }
            if response.clicked() {
                *selected = country.id.clone();
                return Some(country.id.clone());
            }
        }
        painter.text(
            rect.left_bottom() + vec2(12.0, -12.0),
            Align2::LEFT_BOTTOM,
            "SELECT A COUNTRY · G: GARRISON · D: DEPLOYMENT",
            FontId::proportional(11.0),
            MUTED,
        );
        painter.text(
            rect.right_bottom() + vec2(-12.0, -12.0),
            Align2::RIGHT_BOTTOM,
            "Natural Earth · scenario population estimates",
            FontId::proportional(10.0),
            MUTED,
        );
        ui.horizontal_wrapped(|ui| {
            ui.colored_label(AMBER, "Selected");
            ui.colored_label(GREEN, "Freed");
            ui.colored_label(MUTED, "Neighbors unlock after liberation");
            ui.colored_label(
                RED,
                format!(
                    "{} regions infected",
                    data.countries.len() - game.campaign.world.cleared.len()
                ),
            );
        });
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn polygon_area(points: &[Pos2]) -> f64 {
        if points.len() < 3 {
            return 0.0;
        }
        points
            .windows(2)
            .map(|pair| cross(points[0], pair[0], pair[1]))
            .sum::<f64>()
            .abs()
            * 0.5
    }
    fn mesh_area(points: &[Pos2], indices: &[u32]) -> f64 {
        indices
            .as_chunks::<3>()
            .0
            .iter()
            .map(|triangle| {
                cross(
                    points[triangle[0] as usize],
                    points[triangle[1] as usize],
                    points[triangle[2] as usize],
                )
                .abs()
                    * 0.5
            })
            .sum()
    }
    #[test]
    fn closed_concave_ring_with_duplicate_vertices_fills_without_wedges() {
        let points = [
            pos2(0.0, 0.0),
            pos2(4.0, 0.0),
            pos2(4.0, 0.0),
            pos2(4.0, 4.0),
            pos2(2.0, 2.0),
            pos2(0.0, 4.0),
            pos2(0.0, 0.0),
        ];
        let indices = triangulate(&points);
        assert!(!indices.is_empty());
        assert!((mesh_area(&points, &indices) - 12.0).abs() < 1e-9);
    }
    #[test]
    fn every_rendered_country_ring_preserves_its_geographical_area() {
        for size in [
            vec2(320.0, 220.0),
            vec2(1078.0, 528.0),
            vec2(2160.0, 1060.0),
        ] {
            let r = Rect::from_min_size(Pos2::ZERO, size);
            for country in &world().countries {
                for (ring_number, ring) in country.polygons.iter().enumerate() {
                    let points: Vec<_> = ring
                        .iter()
                        .filter(|p| p[1] >= -60.0)
                        .map(|p| project(r, *p))
                        .collect();
                    if points.len() < 3 {
                        continue;
                    }
                    let span = points.iter().map(|p| p.x).fold(f32::NEG_INFINITY, f32::max)
                        - points.iter().map(|p| p.x).fold(f32::INFINITY, f32::min);
                    if span > size.x * 0.7 {
                        continue;
                    }
                    let expected = polygon_area(&points);
                    let indices = triangulate(&points);
                    let actual = mesh_area(&points, &indices);
                    // Sudan's generalized source border contains a tiny crossing
                    // needle near 33.97 E / 9.48 N after source rounding. Its
                    // signed area differs from the visible triangles by 0.057%.
                    // Other rings must conserve area to numerical precision.
                    let relative_tolerance = if country.id == "SDN" { 1e-3 } else { 1e-6 };
                    let tolerance = (expected * relative_tolerance).max(1e-7);
                    assert!(
                        (actual - expected).abs() <= tolerance,
                        "{} ring {} at {:?}: expected area {}, rendered area {}",
                        country.id,
                        ring_number,
                        size,
                        expected,
                        actual
                    );
                }
            }
        }
    }
}
