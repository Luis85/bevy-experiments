mod battlefield;
pub use battlefield::{RenderOptions, battlefield};

use crate::visual_assets::{self, ArtTextures, Prop};
use bevy_egui::egui::{
    self, Align2, Color32 as C, FontId, Painter, Pos2, Rect, Stroke, Vec2, pos2, vec2,
};
use horde_core::{
    content::{EnemyKind, StructureKind},
    sim::Attempt,
};

pub const INK: C = C::from_rgb(17, 23, 24);
pub const PANEL: C = C::from_rgb(27, 35, 35);
pub const TEXT: C = C::from_rgb(230, 230, 212);
pub const MUTED: C = C::from_rgb(164, 180, 172);
pub const AMBER: C = C::from_rgb(239, 181, 82);
pub const GREEN: C = C::from_rgb(146, 195, 145);
pub const RED: C = C::from_rgb(234, 115, 91);

/// One projection contract for drawing, cursor inversion, cells and area radii.
/// The vertical world extent is the frozen attempt lane count, never UI state.
#[derive(Clone, Copy, Debug)]
pub struct ArenaProjection {
    pub rect: Rect,
    pub lanes: u8,
    pub width: f32,
    /// Presentation-only forest space; world mapping and input always use rect.
    pub effects_clip: Rect,
}
impl ArenaProjection {
    #[cfg(test)]
    pub fn new(rect: Rect, lanes: u8) -> Self {
        Self::with_geometry(rect, lanes, 30.0)
    }
    pub fn with_geometry(rect: Rect, lanes: u8, width: f32) -> Self {
        assert!(lanes > 0 && width.is_finite() && width > 0.0);
        Self {
            rect,
            lanes,
            width,
            effects_clip: rect,
        }
    }
    pub fn with_effects_clip(mut self, clip: Rect) -> Self {
        assert!(clip.contains_rect(self.rect));
        self.effects_clip = clip;
        self
    }
    pub fn for_attempt(rect: Rect, attempt: &Attempt) -> Self {
        Self::with_geometry(rect, attempt.lane_count, attempt.width_yards)
    }
    pub fn yard_width(self) -> f32 {
        self.rect.width() / self.width
    }
    pub fn world(self, x: f32, y: f32) -> Pos2 {
        pos2(
            self.rect.left() + x / self.width * self.rect.width(),
            self.rect.top() + y / f32::from(self.lanes) * self.rect.height(),
        )
    }
    pub fn inverse(self, p: Pos2) -> [f32; 2] {
        [
            (p.x - self.rect.left()) / self.rect.width() * self.width,
            (p.y - self.rect.top()) / self.rect.height() * f32::from(self.lanes),
        ]
    }
    pub fn lane_height(self) -> f32 {
        self.rect.height() / f32::from(self.lanes)
    }
    pub fn radius(self, radius: f32) -> Vec2 {
        vec2(
            radius / self.width * self.rect.width(),
            radius * self.lane_height(),
        )
    }
}
#[cfg(test)]
pub fn world(r: Rect, x: f32, y: f32) -> Pos2 {
    ArenaProjection::new(r, horde_core::sim::LANES).world(x, y)
}
#[cfg(test)]
pub fn inverse(r: Rect, p: Pos2) -> [f32; 2] {
    ArenaProjection::new(r, horde_core::sim::LANES).inverse(p)
}

fn line(p: &Painter, a: Pos2, b: Pos2, width: f32, c: C) {
    p.line_segment([a, b], Stroke::new(width, c));
}
fn box_at(p: &Painter, center: Pos2, size: Vec2, c: C) {
    p.rect_filled(Rect::from_center_size(center, size), 0.0, c);
}

fn paint_ground(p: &Painter, r: Rect, theme: usize, textures: ArtTextures) {
    if !visual_assets::ground(p, r, textures, theme) {
        let soils = [
            C::from_rgb(46, 65, 52),
            C::from_rgb(48, 62, 62),
            C::from_rgb(91, 79, 55),
            C::from_rgb(59, 70, 72),
            C::from_rgb(40, 65, 49),
        ];
        p.rect_filled(r, 0., soils[theme % soils.len()]);
    }
}

/// The forest belongs to the world surrounding the complete projected arena.
/// Clip every prop to its peripheral strip so dressing can never conceal a
/// combat lane, a contact position, a construction cell or the aiming origin.
pub fn world_backdrop(
    p: &Painter,
    canvas: Rect,
    projection: ArenaProjection,
    theme: usize,
    textures: ArtTextures,
) {
    let arena = projection.rect;
    paint_ground(p, canvas, theme, textures);
    let strips = peripheral_strips(canvas, arena);
    let unit = projection.lane_height().max(1.);
    for (edge, strip) in strips.into_iter().enumerate() {
        if !strip.is_positive() {
            continue;
        }
        let clipped = p.with_clip_rect(strip.intersect(p.clip_rect()));
        let horizontal = edge < 2;
        let span = if horizontal {
            arena.width()
        } else {
            arena.height()
        };
        let spacing = match theme % 5 {
            2 => 2.8,
            4 => 1.05,
            _ => 1.35,
        };
        let count = ((span / (unit * spacing)).ceil() as usize).clamp(1, 44);
        let mut dressing = Vec::with_capacity(count + 3);
        for i in 0..count {
            // Presentation-only variation never consumes simulation randomness.
            let noise = ((i * 37 + edge * 13 + theme * 7) % 23) as f32 / 23.;
            let along = (i as f32 + 0.15 + noise * 0.7) / count as f32;
            let prop = peripheral_kind(theme, i + edge * 3);
            let height = unit
                * match prop {
                    Prop::TreeBroadleaf | Prop::TreePine => 1.35 + noise * 0.5,
                    Prop::Bush => 0.55 + noise * 0.35,
                    _ => 0.45 + noise * 0.3,
                };
            let tint = if theme % 5 == 2 {
                C::from_rgb(174, 157, 123)
            } else {
                C::from_rgb(176, 193, 174)
            };
            let size = vec2(unit * 1.8, height);
            if let Some(foot) = perimeter_foot(prop, size, arena, strip, edge, along, unit * 0.08) {
                dressing.push((prop, foot, size, tint));
            }
        }
        // A few outpost objects share the trees' ground plane. Their feet,
        // rather than their image centers, determine depth and placement.
        if horizontal {
            for &(prop, along) in peripheral_details(theme, edge) {
                let height = if prop == Prop::Lamp {
                    unit * 1.35
                } else {
                    unit * 0.75
                };
                let size = vec2(unit * 1.5, height);
                if let Some(foot) =
                    perimeter_foot(prop, size, arena, strip, edge, along, unit * 0.08)
                {
                    dressing.push((prop, foot, size, C::from_rgb(158, 170, 153)));
                }
            }
        }
        dressing.sort_by(|a, b| a.1.y.total_cmp(&b.1.y).then(a.1.x.total_cmp(&b.1.x)));
        for (prop, foot, size, tint) in dressing {
            visual_assets::prop_grounded(&clipped, textures, prop, foot, size, tint);
        }
    }
}

fn peripheral_strips(canvas: Rect, arena: Rect) -> [Rect; 4] {
    [
        Rect::from_min_max(canvas.min, pos2(canvas.right(), arena.top())),
        Rect::from_min_max(pos2(canvas.left(), arena.bottom()), canvas.max),
        Rect::from_min_max(pos2(canvas.left(), arena.top()), arena.left_bottom()),
        Rect::from_min_max(arena.right_top(), pos2(canvas.right(), arena.bottom())),
    ]
}

/// Place the complete sprite just outside its legal clearing edge. Grounded
/// source bounds, rather than sprite centers or the window edge, set the gap.
fn perimeter_foot(
    prop: Prop,
    size: Vec2,
    arena: Rect,
    strip: Rect,
    edge: usize,
    along: f32,
    gap: f32,
) -> Option<Pos2> {
    let bounds = visual_assets::prop_ground_rect(prop, Pos2::ZERO, size);
    let foot = match edge {
        0 => pos2(
            arena.left() + arena.width() * along - bounds.center().x,
            arena.top() - gap - bounds.bottom(),
        ),
        1 => pos2(
            arena.left() + arena.width() * along - bounds.center().x,
            arena.bottom() + gap - bounds.top(),
        ),
        2 => pos2(
            arena.left() - gap - bounds.right(),
            arena.top() + arena.height() * along - bounds.center().y,
        ),
        _ => pos2(
            arena.right() + gap - bounds.left(),
            arena.top() + arena.height() * along - bounds.center().y,
        ),
    };
    inset_tree_foot(prop, foot, size, strip)
}

fn inset_tree_foot(prop: Prop, foot: Pos2, size: Vec2, strip: Rect) -> Option<Pos2> {
    let safe = strip.shrink(2.0);
    let sprite = visual_assets::prop_ground_rect(prop, foot, size);
    if !safe.is_positive() || sprite.width() > safe.width() || sprite.height() > safe.height() {
        return None;
    }
    let dx = if sprite.left() < safe.left() {
        safe.left() - sprite.left()
    } else if sprite.right() > safe.right() {
        safe.right() - sprite.right()
    } else {
        0.0
    };
    let dy = if sprite.top() < safe.top() {
        safe.top() - sprite.top()
    } else if sprite.bottom() > safe.bottom() {
        safe.bottom() - sprite.bottom()
    } else {
        0.0
    };
    Some(foot + vec2(dx, dy))
}

fn peripheral_kind(theme: usize, index: usize) -> Prop {
    match theme % 5 {
        2 => match index % 7 {
            1 => Prop::Debris,
            4 => Prop::Fence,
            6 => Prop::Bush,
            _ => Prop::Rock,
        },
        4 => {
            if index % 4 == 1 {
                Prop::Bush
            } else {
                Prop::TreeBroadleaf
            }
        }
        3 => {
            if index % 7 == 2 {
                Prop::Bush
            } else {
                Prop::TreePine
            }
        }
        _ => match index % 5 {
            1 => Prop::Bush,
            2 | 4 => Prop::TreeBroadleaf,
            _ => Prop::TreePine,
        },
    }
}
fn peripheral_details(theme: usize, edge: usize) -> &'static [(Prop, f32)] {
    match (theme % 5, edge) {
        (2, 0) => &[(Prop::Wreck, 0.66), (Prop::Fence, 0.24)],
        (2, _) => &[(Prop::Debris, 0.45)],
        (4, 0) => &[(Prop::Fence, 0.23)],
        (4, _) => &[(Prop::Rock, 0.74)],
        (3, 0) => &[(Prop::Wreck, 0.68)],
        (3, _) => &[(Prop::Debris, 0.26)],
        (_, 0) => &[(Prop::Fence, 0.18), (Prop::Wreck, 0.66), (Prop::Lamp, 0.72)],
        _ => &[(Prop::Debris, 0.35)],
    }
}

/// Apply after the actors so the complete world shares one gentle edge falloff.
/// Central combat remains fully lit; the fixed aim origin and geometry do not move.
pub fn world_atmosphere(p: &Painter, canvas: Rect) {
    edge_shade(p, canvas);
}

fn edge_shade(p: &Painter, canvas: Rect) {
    let mut mesh = egui::epaint::Mesh::default();
    let depth = canvas.height().min(canvas.width()) * 0.16;
    // Smooth vertex color interpolation gives darker corners and gentler side
    // falloff, preserving the survivor anchored close to the left-hand edge.
    for (inset, strength) in [(0., 1.), (depth * 0.45, 0.35), (depth, 0.)] {
        let r = canvas.shrink(inset);
        let positions = [
            r.left_top(),
            r.center_top(),
            r.right_top(),
            r.right_center(),
            r.right_bottom(),
            r.center_bottom(),
            r.left_bottom(),
            r.left_center(),
        ];
        for (at, opacity) in positions
            .into_iter()
            .zip([185., 110., 185., 75., 185., 125., 185., 75.])
        {
            mesh.colored_vertex(at, C::from_black_alpha((opacity * strength) as u8));
        }
    }
    for ring in 0..2 {
        for side in 0..8 {
            let a = ring * 8 + side;
            let b = ring * 8 + (side + 1) % 8;
            mesh.add_triangle(a, b, b + 8);
            mesh.add_triangle(a, b + 8, a + 8);
        }
    }
    p.add(egui::Shape::mesh(mesh));
}

pub fn figure(
    p: &Painter,
    at: Pos2,
    size: f32,
    kind: EnemyKind,
    boss: bool,
    phase: f32,
    outline: bool,
) {
    let s = size;
    let coat = if boss {
        C::from_rgb(135, 73, 53)
    } else {
        match kind {
            EnemyKind::Shambler => C::from_rgb(126, 144, 104),
            EnemyKind::Runner => C::from_rgb(193, 151, 102),
            EnemyKind::Brute => C::from_rgb(123, 108, 89),
            EnemyKind::Armored => C::from_rgb(133, 154, 158),
            EnemyKind::Demolisher => C::from_rgb(197, 150, 61),
        }
    };
    let skin = C::from_rgb(164, 163, 121);
    p.add(egui::Shape::ellipse_filled(
        at + vec2(s * 0.1, s * 0.55),
        vec2(s * 0.66, s * 0.24),
        C::from_black_alpha(65),
    ));
    let sway = phase.sin() * s * 0.07;
    if outline {
        p.circle_stroke(at, s * 0.78, Stroke::new(1.5, AMBER));
    }
    let leg = phase.sin() * s * 0.19;
    line(
        p,
        at + vec2(s * 0.15, s * 0.18),
        at + vec2(s * 0.25 + leg, s * 0.55),
        s * 0.25,
        C::from_rgb(54, 60, 52),
    );
    line(
        p,
        at + vec2(-s * 0.18, s * 0.19),
        at + vec2(-s * 0.24 - leg, s * 0.52),
        s * 0.25,
        C::from_rgb(64, 70, 59),
    );
    box_at(p, at + vec2(sway, 0.0), vec2(s * 0.88, s * 0.65), coat);
    box_at(
        p,
        at + vec2(s * 0.27 + sway, s * 0.02),
        vec2(s * 0.18, s * 0.6),
        coat.gamma_multiply(0.72),
    );
    line(
        p,
        at + vec2(-s * 0.3, -s * 0.08),
        at + vec2(-s * 0.78, -s * 0.16 + sway),
        s * 0.18,
        coat,
    );
    box_at(
        p,
        at + vec2(-s * 0.83, -s * 0.16 + sway),
        vec2(s * 0.2, s * 0.16),
        skin,
    );
    p.circle_filled(at + vec2(-s * 0.18 + sway, -s * 0.44), s * 0.27, skin);
    box_at(
        p,
        at + vec2(-s * 0.32 + sway, -s * 0.43),
        vec2(s * 0.15, s * 0.08),
        C::from_rgb(64, 59, 47),
    );
    if matches!(kind, EnemyKind::Armored) || boss {
        box_at(
            p,
            at + vec2(sway, -s * 0.05),
            vec2(s * 0.62, s * 0.36),
            C::from_rgb(75, 89, 87),
        );
        line(
            p,
            at + vec2(-s * 0.24, -s * 0.1),
            at + vec2(s * 0.24, -s * 0.1),
            s * 0.07,
            AMBER.gamma_multiply(0.7),
        );
    }
    if matches!(kind, EnemyKind::Demolisher) {
        line(
            p,
            at + vec2(-s * 0.6, 0.0),
            at + vec2(-s * 0.6, -s * 0.9),
            s * 0.12,
            C::from_rgb(93, 82, 60),
        );
        box_at(
            p,
            at + vec2(-s * 0.6, -s * 0.88),
            vec2(s * 0.6, s * 0.22),
            C::from_rgb(133, 140, 131),
        );
    }
    if boss {
        for i in 0..3 {
            box_at(
                p,
                at + vec2((i as f32 - 1.0) * s * 0.23, s * 0.14),
                vec2(s * 0.12, s * 0.22),
                AMBER,
            );
        }
    }
}

pub fn structure(p: &Painter, at: Pos2, size: Vec2, kind: StructureKind) {
    let r = Rect::from_center_size(at, size);
    p.rect_filled(r.translate(vec2(3.0, 4.0)), 1.0, C::from_black_alpha(75));
    match kind {
        StructureKind::Wall => {
            p.rect_filled(r, 1.0, C::from_rgb(120, 115, 89));
            for i in 0..4 {
                let x = r.left() + size.x * (i as f32 + 0.5) / 4.0;
                line(
                    p,
                    pos2(x, r.top()),
                    pos2(x, r.bottom()),
                    2.0,
                    C::from_rgb(76, 80, 67),
                );
            }
            line(
                p,
                r.left_top() + vec2(1.0, 4.0),
                r.right_bottom() - vec2(1.0, 4.0),
                3.0,
                C::from_rgb(154, 145, 101),
            );
            p.rect_stroke(
                r,
                1.0,
                Stroke::new(1.0, C::from_rgb(164, 158, 123)),
                egui::StrokeKind::Inside,
            );
        }
        StructureKind::Trap => {
            p.rect_filled(r, 1.0, C::from_rgb(75, 72, 53));
            for i in 0..5 {
                let x = r.left() + size.x * (i as f32 + 0.5) / 5.0;
                line(
                    p,
                    pos2(x, r.bottom() - 2.0),
                    pos2(x + size.x * 0.08, r.top() + 2.0),
                    2.0,
                    C::from_rgb(194, 185, 145),
                );
            }
            line(
                p,
                r.left_bottom(),
                r.right_bottom(),
                3.0,
                AMBER.gamma_multiply(0.6),
            );
        }
        StructureKind::Turret => {
            for d in [vec2(-1.0, -1.0), vec2(-1.0, 1.0), vec2(1.0, 1.0)] {
                line(p, at, at + d * size * 0.42, 3.0, C::from_rgb(56, 65, 60));
            }
            p.circle_filled(at, size.y * 0.32, C::from_rgb(104, 131, 117));
            line(
                p,
                at,
                at + vec2(size.x * 0.65, 0.0),
                size.y * 0.18,
                C::from_rgb(189, 191, 158),
            );
            box_at(
                p,
                at + vec2(0.0, -size.y * 0.14),
                vec2(size.x * 0.3, size.y * 0.32),
                C::from_rgb(55, 67, 60),
            );
            p.circle_filled(at + vec2(size.x * 0.16, 0.0), 2.0, AMBER);
        }
    }
}

fn soft_shadow(p: &Painter, at: Pos2, size: Vec2, alpha: f32) {
    for (spread, opacity) in [(1.35, 10.), (1.12, 15.), (0.9, 23.)] {
        p.add(egui::Shape::ellipse_filled(
            at,
            size * spread,
            C::from_black_alpha((opacity * alpha) as u8),
        ));
    }
}
fn structure_fallback(
    p: &Painter,
    at: Pos2,
    size: Vec2,
    kind: StructureKind,
    target: Vec2,
    recoil: f32,
) {
    if kind != StructureKind::Turret {
        structure(p, at, size, kind);
        return;
    }
    let direction = target.normalized();
    for d in [vec2(-1., -1.), vec2(-1., 1.), vec2(1., 1.)] {
        line(p, at, at + d * size * 0.42, 3., C::from_rgb(56, 65, 60));
    }
    p.circle_filled(at, size.y * 0.30, C::from_rgb(104, 131, 117));
    let breech = at - direction * recoil * size.y * 0.10;
    line(
        p,
        breech,
        breech + direction * size.y * 0.70,
        size.y * 0.16,
        C::from_rgb(173, 178, 153),
    );
    p.circle_filled(breech, size.y * 0.18, C::from_rgb(64, 78, 68));
}

pub fn bar(p: &Painter, r: Rect, value: f32, color: C) {
    p.rect_filled(r, 1.0, C::from_black_alpha(100));
    let mut fill = r;
    fill.max.x = fill.min.x + fill.width() * value.clamp(0.0, 1.0);
    p.rect_filled(fill, 1.0, color);
}

pub fn camp_scene(p: &Painter, r: Rect, time: f32) {
    p.rect_filled(r, 0.0, C::from_rgb(47, 62, 59));
    let horizon = r.top() + r.height() * 0.37;
    // Layered, original skyline and checkpoint silhouette.
    for i in 0..22 {
        let x = r.left() + i as f32 * r.width() / 21.0;
        let h = 30.0 + ((i * 37) % 73) as f32;
        let b = Rect::from_min_max(
            pos2(x, horizon - h),
            pos2(x + r.width() / 18.0, horizon + 20.0),
        );
        p.rect_filled(b, 0.0, C::from_rgb(38, 51, 49));
        for w in 0..3 {
            box_at(
                p,
                pos2(x + 8.0 + w as f32 * 9.0, horizon - h + 12.0),
                vec2(3.0, 5.0),
                C::from_rgb(64, 75, 65),
            );
        }
    }
    p.rect_filled(
        Rect::from_min_max(pos2(r.left(), horizon), r.max),
        0.0,
        C::from_rgb(40, 48, 40),
    );
    let road = vec![
        pos2(r.left() + r.width() * 0.38, horizon),
        pos2(r.left() + r.width() * 0.65, horizon),
        pos2(r.right(), r.bottom()),
        pos2(r.left(), r.bottom()),
    ];
    p.add(egui::Shape::convex_polygon(
        road,
        C::from_rgb(53, 57, 48),
        Stroke::NONE,
    ));
    for i in 0..12 {
        let x = r.left() + i as f32 * r.width() / 11.0;
        line(
            p,
            pos2(x, horizon + 20.0),
            pos2(x, r.bottom()),
            1.0,
            C::from_rgb(86, 94, 76),
        );
    }
    for j in 0..8 {
        let y = horizon + 20.0 + j as f32 * 30.0;
        line(
            p,
            pos2(r.left(), y),
            pos2(r.right(), y),
            1.0,
            C::from_rgb(75, 85, 70),
        );
    }
    let base = pos2(r.left() + r.width() * 0.64, r.top() + r.height() * 0.69);
    for i in 0..6 {
        structure(
            p,
            base + vec2((i as f32 - 2.5) * 33.0, 0.0),
            vec2(30.0, 27.0),
            StructureKind::Wall,
        );
    }
    structure(
        p,
        base + vec2(80.0, -24.0),
        vec2(36.0, 32.0),
        StructureKind::Turret,
    );
    let lamp = base + vec2(-116.0, -100.0);
    line(
        p,
        lamp,
        lamp + vec2(0.0, 150.0),
        5.0,
        C::from_rgb(24, 33, 29),
    );
    line(
        p,
        lamp,
        lamp + vec2(28.0, 0.0),
        4.0,
        C::from_rgb(24, 33, 29),
    );
    p.add(egui::Shape::convex_polygon(
        vec![
            lamp + vec2(27.0, 4.0),
            lamp + vec2(-25.0, 160.0),
            lamp + vec2(115.0, 160.0),
        ],
        C::from_rgba_unmultiplied(230, 186, 92, 14),
        Stroke::NONE,
    ));
    box_at(p, lamp + vec2(27.0, 3.0), vec2(16.0, 5.0), AMBER);
    for i in 0..8 {
        let x = (i as f32 * 0.127 + time * 0.006).fract();
        let y = (i as f32 * 0.313 + time * 0.008).fract();
        p.circle_filled(
            r.min + vec2(r.width() * x, r.height() * y),
            1.0,
            C::from_rgba_unmultiplied(206, 186, 125, 60),
        );
    }
}

pub fn country_scene_textured(
    p: &Painter,
    r: Rect,
    biome: &str,
    culture: &str,
    stage: usize,
    textures: ArtTextures,
) {
    if visual_assets::panorama(p, r, textures, biome) {
        p.rect_filled(
            Rect::from_min_max(pos2(r.left(), r.bottom() - 22.), r.max),
            0.,
            C::from_black_alpha(180),
        );
        p.text(
            r.left_bottom() + vec2(10., -5.),
            Align2::LEFT_BOTTOM,
            format!("{} · {}", biome.to_uppercase(), culture),
            FontId::proportional(11.),
            TEXT,
        );
    } else {
        country_scene(p, r, biome, culture, stage);
    }
}
pub fn country_scene(p: &Painter, r: Rect, biome: &str, culture: &str, stage: usize) {
    let (sky, ground) = match biome {
        "Arid" => (C::from_rgb(155, 132, 89), C::from_rgb(105, 88, 60)),
        "Boreal" => (C::from_rgb(129, 154, 159), C::from_rgb(94, 111, 104)),
        "Tropical" => (C::from_rgb(87, 123, 106), C::from_rgb(54, 86, 58)),
        "Alpine" => (C::from_rgb(110, 135, 142), C::from_rgb(74, 87, 80)),
        _ => (C::from_rgb(109, 130, 123), C::from_rgb(66, 82, 63)),
    };
    p.rect_filled(r, 0.0, sky);
    if biome == "Alpine" || biome == "Boreal" {
        for n in 0..7 {
            let x = r.left() + r.width() * n as f32 / 6.0;
            let h = r.height() * (0.5 + (n % 3) as f32 * 0.12);
            p.add(egui::Shape::convex_polygon(
                vec![
                    pos2(x - 100.0, r.bottom()),
                    pos2(x, r.bottom() - h),
                    pos2(x + 120.0, r.bottom()),
                ],
                ground.gamma_multiply(0.85),
                Stroke::NONE,
            ));
        }
    }
    let horizon = r.top() + r.height() * 0.68;
    p.rect_filled(
        Rect::from_min_max(pos2(r.left(), horizon), r.max),
        0.0,
        ground,
    );
    let east = culture.contains("Asia");
    let tropical = biome == "Tropical";
    let arid = biome == "Arid";
    for n in 0..13 {
        let x = r.left() + n as f32 * r.width() / 12.0;
        let w = r.width() / 17.0;
        let h = 18.0 + ((n * 17 + stage * 11) % 23) as f32;
        let material = if arid {
            C::from_rgb(170, 146, 101)
        } else if tropical {
            C::from_rgb(106, 109, 70)
        } else {
            C::from_rgb(79, 91, 82)
        };
        let building = Rect::from_min_max(pos2(x, horizon - h), pos2(x + w, horizon));
        p.rect_filled(building, 0.0, material);
        if !arid {
            let peak = if east { 12.0 } else { 8.0 };
            p.add(egui::Shape::convex_polygon(
                vec![
                    building.left_top() - vec2(6.0, 0.0),
                    pos2(building.center().x, building.top() - peak),
                    building.right_top() + vec2(6.0, 0.0),
                ],
                C::from_rgb(61, 72, 62),
                Stroke::NONE,
            ));
        }
        for window in 0..3 {
            box_at(
                p,
                pos2(x + 7.0 + window as f32 * w / 4.0, horizon - h + 8.0),
                vec2(4.0, 6.0),
                C::from_rgb(37, 51, 46),
            );
        }
        if tropical {
            let at = pos2(x + w + 9.0, horizon);
            line(p, at, at + vec2(3.0, -38.0), 3.0, C::from_rgb(55, 75, 48));
            for d in [-1.0, 1.0] {
                line(
                    p,
                    at + vec2(3.0, -37.0),
                    at + vec2(d * 19.0, -25.0),
                    4.0,
                    C::from_rgb(50, 82, 45),
                );
            }
        }
    }
    for i in 0..3 {
        let at = pos2(r.right() - 70.0 - i as f32 * 55.0, horizon + 4.0);
        box_at(p, at, vec2(43.0, 12.0), C::from_rgb(61, 75, 67));
        box_at(
            p,
            at + vec2(-2.0, -9.0),
            vec2(27.0, 12.0),
            C::from_rgb(76, 88, 73),
        );
        for dx in [-13.0, 13.0] {
            p.circle_filled(at + vec2(dx, 7.0), 4.0, C::from_rgb(25, 37, 33));
        }
    }
    p.rect_filled(
        Rect::from_min_max(pos2(r.left(), r.bottom() - 24.0), r.max),
        0.0,
        C::from_black_alpha(140),
    );
    p.text(
        r.left_bottom() + vec2(10.0, -6.0),
        Align2::LEFT_BOTTOM,
        format!("{} · {}", biome.to_uppercase(), culture),
        FontId::proportional(11.0),
        TEXT,
    );
}

#[cfg(test)]
mod terrain_tests {
    use super::*;

    #[test]
    fn projection_covers_every_dynamic_lane_and_preserves_world_cursor_and_radius_geometry() {
        for lanes in [11u8, 13, 15, 17] {
            for size in [vec2(1392.0, 692.0), vec2(729.0, 310.0)] {
                let rect = Rect::from_min_size(pos2(24.0, 124.0), size);
                let projection = ArenaProjection::new(rect, lanes);
                assert_eq!(
                    projection.world(0.5, f32::from(lanes) * 0.5).y,
                    rect.center().y
                );
                assert!(projection.lane_height() >= 18.0);
                for lane in 0..lanes {
                    for x in [0.5, 4.2, 20.0, 29.5] {
                        let original = [x, f32::from(lane) + 0.5];
                        let point = projection.world(original[0], original[1]);
                        assert!(rect.contains(point));
                        let restored = projection.inverse(point);
                        assert!(
                            (restored[0] - original[0]).abs() < 0.001
                                && (restored[1] - original[1]).abs() < 0.001
                        );
                    }
                }
                let radius = projection.radius(2.0);
                assert!((radius.y - projection.lane_height() * 2.0).abs() < 0.001);
            }
        }
    }
    #[test]
    fn every_drawn_tree_fits_its_full_source_bounds_inside_the_peripheral_strip() {
        for strip in [
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1050.0, 124.0)),
            Rect::from_min_max(pos2(0.0, 616.0), pos2(1050.0, 700.0)),
            Rect::from_min_max(pos2(0.0, 124.0), pos2(24.0, 616.0)),
        ] {
            for prop in [Prop::TreeBroadleaf, Prop::TreePine] {
                for foot in [
                    strip.left_bottom(),
                    strip.right_bottom(),
                    strip.center_bottom() + vec2(0.0, 30.0),
                ] {
                    let size = vec2(90.0, 75.0);
                    match inset_tree_foot(prop, foot, size, strip) {
                        Some(foot) => assert!(
                            strip
                                .shrink(1.9)
                                .contains_rect(visual_assets::prop_ground_rect(prop, foot, size))
                        ),
                        None => {
                            let rect = visual_assets::prop_ground_rect(prop, foot, size);
                            assert!(
                                rect.width() > strip.shrink(2.0).width()
                                    || rect.height() > strip.shrink(2.0).height()
                            );
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn dressing_tracks_all_actual_clearing_edges_and_keeps_complete_sprites_outside_legal_space() {
        for size in [
            vec2(1440.0, 856.0),
            vec2(1050.0 / 1.35, 700.0 / 1.35 - 44.0),
        ] {
            let canvas = Rect::from_min_size(Pos2::ZERO, size);
            let maximum = Rect::from_min_max(pos2(24.0, 124.0), canvas.max - vec2(24.0, 84.0));
            for (lanes, width) in [(3, 12.0), (7, 16.0), (17, 30.0)] {
                let extent = vec2(
                    maximum.width() * width / 30.0,
                    (maximum.height() * (lanes as f32 / 11.0).min(1.0)).max(86.0),
                );
                let arena = if lanes == 17 {
                    maximum
                } else {
                    Rect::from_center_size(maximum.center(), extent)
                };
                let unit = arena.height() / lanes as f32;
                let gap = unit * 0.08;
                let mut visible = [0; 4];
                for (edge, strip) in peripheral_strips(canvas, arena).into_iter().enumerate() {
                    for prop in [Prop::TreeBroadleaf, Prop::TreePine, Prop::Bush, Prop::Rock] {
                        for along in [0.0, 0.2, 0.5, 0.8, 1.0] {
                            let size = vec2(unit * 1.8, unit * 1.6);
                            if let Some(foot) =
                                perimeter_foot(prop, size, arena, strip, edge, along, gap)
                            {
                                let drawn = visual_assets::prop_ground_rect(prop, foot, size);
                                assert!(canvas.contains_rect(drawn));
                                assert!(strip.shrink(1.9).contains_rect(drawn));
                                assert!(
                                    !arena.intersects(drawn),
                                    "Dressing cannot conceal legal lanes or cells"
                                );
                                let distance = match edge {
                                    0 => arena.top() - drawn.bottom(),
                                    1 => drawn.top() - arena.bottom(),
                                    2 => arena.left() - drawn.right(),
                                    _ => drawn.left() - arena.right(),
                                };
                                assert!(
                                    distance >= 1.9 && distance <= gap.max(2.0) + 0.01,
                                    "{lanes}x{width} edge{edge}: sprite gap{distance} must follow the clearing, not the canvas"
                                );
                                visible[edge] += 1;
                            }
                        }
                    }
                }
                assert!(visible[0] > 0 && visible[1] > 0);
                if lanes < 17 {
                    assert!(
                        visible.iter().all(|count| *count > 0),
                        "Opening and intermediate clearings need all four perimeter cues"
                    );
                }
            }
        }
    }

    #[test]
    fn continuous_ground_preserves_the_shared_line_without_a_combat_grid() {
        let ctx = egui::Context::default();
        let r = Rect::from_min_size(pos2(20., 50.), vec2(600., 220.));
        let mut output = ctx.run_ui(Default::default(), |ui| {
            battlefield::terrain::paint(
                ui.painter(),
                ArenaProjection::new(r, 11),
                0,
                ArtTextures {
                    ground: Some(egui::TextureId::User(1)),
                    ..Default::default()
                },
            );
        });
        let lines: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|shape| {
                if let egui::Shape::LineSegment { points, .. } = &shape.shape {
                    Some(*points)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(lines, vec![[world(r, 1.5, 0.), world(r, 1.5, 11.)]]);
        assert!(
            output.shapes.iter().any(|shape| {
                matches!(&shape.shape, egui::Shape::Mesh(mesh)
                if mesh.texture_id == egui::TextureId::User(1))
            }),
            "Original pixel ground must be drawn"
        );
        for shape in &output.shapes {
            if let egui::Shape::Mesh(mesh) = &shape.shape
                && mesh.texture_id == egui::TextureId::User(1)
            {
                assert!(
                    mesh.vertices.iter().all(|vertex| vertex.color.a() <= 192)
                        && mesh.vertices.iter().any(|vertex| vertex.color.a() >= 160)
                        && mesh.vertices.iter().any(|vertex| vertex.color.a() == 0),
                    "Ground material stays visible while patch edges blend continuously"
                );
                assert!(
                    mesh.vertices[0].uv.x < mesh.vertices[3].uv.x
                        && mesh.vertices[0].uv.y < mesh.vertices[12].uv.y,
                    "Ground motifs must never be reflected into a carpet"
                );
            }
        }
        output.textures_delta.clear();
    }

    #[test]
    fn forest_and_outpost_dressing_are_clipped_outside_all_combat_lanes() {
        let ctx = egui::Context::default();
        let canvas = Rect::from_min_size(pos2(0., 0.), vec2(1000., 700.));
        let arena = canvas.shrink2(vec2(12., 60.));
        let mut output = ctx.run_ui(Default::default(), |ui| {
            world_backdrop(
                ui.painter(),
                canvas,
                ArenaProjection::new(arena, 11),
                0,
                ArtTextures {
                    props: Some(egui::TextureId::User(2)),
                    ..Default::default()
                },
            );
        });
        let props: Vec<_> = output
            .shapes
            .iter()
            .filter(|shape| {
                matches!(&shape.shape, egui::Shape::Mesh(mesh)
                if mesh.texture_id == egui::TextureId::User(2))
            })
            .collect();
        assert!(!props.is_empty());
        assert!(
            props.len() <= 4 * (44 + 5),
            "Peripheral dressing stays bounded"
        );
        let first_strip = props[0].clip_rect;
        let mut previous_foot = f32::NEG_INFINITY;
        for prop in &props {
            if prop.clip_rect == first_strip
                && let egui::Shape::Mesh(mesh) = &prop.shape
            {
                let foot = mesh
                    .vertices
                    .iter()
                    .map(|vertex| vertex.pos.y)
                    .fold(f32::NEG_INFINITY, f32::max);
                assert!(
                    foot >= previous_foot - 0.001,
                    "Peripheral objects render from far feet to near feet"
                );
                previous_foot = foot;
            }
        }
        for prop in props {
            assert!(
                !prop.clip_rect.intersect(arena).is_positive(),
                "A tree, fence or wreck must never cover tactical ground"
            );
        }
        output.textures_delta.clear();
    }

    #[test]
    fn biome_dressing_uses_arid_rocks_tropical_canopies_and_boreal_pines() {
        for index in 0..128 {
            assert!(matches!(
                peripheral_kind(2, index),
                Prop::Rock | Prop::Debris | Prop::Fence | Prop::Bush
            ));
            assert!(matches!(
                peripheral_kind(4, index),
                Prop::TreeBroadleaf | Prop::Bush
            ));
            assert!(matches!(
                peripheral_kind(3, index),
                Prop::TreePine | Prop::Bush
            ));
        }
        assert!((0..8).any(|index| peripheral_kind(0, index) == Prop::TreePine));
        assert!((0..8).any(|index| peripheral_kind(0, index) == Prop::TreeBroadleaf));
        assert!(peripheral_details(2, 0).len() <= 2);
        assert!(peripheral_details(4, 0).len() <= 2);
    }

    #[test]
    fn world_mapping_keeps_every_lane_and_the_fixed_defender_anchor() {
        for size in [vec2(1000., 580.), vec2(600., 350.), vec2(1920., 880.)] {
            let r = Rect::from_min_size(pos2(17., 51.), size);
            for lane in 0..11 {
                for x in [0.5, 1.5, 3., 15., 30.] {
                    let y = lane as f32 + 0.5;
                    let at = world(r, x, y);
                    assert!(r.contains(at));
                    let restored = inverse(r, at);
                    assert!((restored[0] - x).abs() < 0.00001);
                    assert!((restored[1] - y).abs() < 0.00001);
                }
            }
            assert_eq!(world(r, 0.5, 5.5).y, r.center().y);
        }
    }
}
