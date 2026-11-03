use crate::{
    animation::{self, ActorPose, ParticleKind, SceneCache},
    visual_assets::{self, Actor, ArtTextures, HitFlash, Prop, PropMotion},
};
use bevy::log::warn;
use bevy_egui::egui::{
    self, Align2, Color32 as C, FontId, Painter, Pos2, Rect, Stroke, Vec2, pos2, vec2,
};
use horde_core::{
    content::{BossKind, EnemyKind, Mode, StructureKind, content},
    enemy_catalog::{EnemyVisual, VisualSprite},
    ordnance::{self, OrdnanceKind},
    sim::{Attempt, EnemyPhase, EventKind},
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

pub fn terrain(p: &Painter, arena: ArenaProjection, theme: usize, textures: ArtTextures) {
    let r = arena.rect;
    paint_ground(p, r, theme, textures);
    // A faded chalk line joins all eleven contact positions. Lane beds and
    // construction cells appear only when the player needs those coordinates.
    line(
        p,
        arena.world(1.5, 0.),
        arena.world(1.5, f32::from(arena.lanes)),
        1.,
        C::from_rgba_unmultiplied(205, 184, 119, 100),
    );
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

pub struct RenderOptions {
    pub construction: bool,
    pub outlines: bool,
    pub numbers: bool,
    pub reduced: bool,
    pub blood: bool,
    pub theme: usize,
    pub textures: ArtTextures,
}
pub fn battlefield(p: &Painter, r: Rect, a: &Attempt, aim: [f32; 2], options: RenderOptions) {
    let arena = ArenaProjection::for_attempt(r, a);
    let RenderOptions {
        construction,
        outlines,
        numbers,
        reduced,
        blood,
        theme,
        textures,
    } = options;
    let motion = p.ctx().data_mut(|data| {
        let cache =
            data.get_temp_mut_or_default::<SceneCache>(egui::Id::new("lastlight_cosmetic_scenes"));
        if p.layer_id() == egui::LayerId::background() {
            cache.sample(a, reduced)
        } else {
            cache.sample_scoped(a, reduced, p.layer_id().id.value())
        }
    });
    terrain(p, arena, theme, textures);
    paint_ambient(p, arena, &motion);
    paint_fire_zones(p, r, a, reduced, textures);
    let sy = arena.lane_height();
    let [build_left, build_right] = a.construction_bounds();
    let cell_width = a.construction_cell_width();
    if construction {
        p.rect_filled(
            Rect::from_min_max(
                arena.world(build_left, 0.0),
                arena.world(build_right, f32::from(arena.lanes)),
            ),
            0.0,
            C::from_rgba_unmultiplied(146, 195, 145, 12),
        );
        for column in 0..=5 {
            let x = build_left + column as f32 * cell_width;
            line(
                p,
                arena.world(x, 0.0),
                arena.world(x, f32::from(arena.lanes)),
                1.0,
                GREEN.gamma_multiply(0.45),
            );
        }
        for lane in 0..=arena.lanes {
            line(
                p,
                arena.world(build_left, lane as f32),
                arena.world(build_right, lane as f32),
                1.0,
                GREEN.gamma_multiply(0.35),
            );
        }
        for lane in 0..arena.lanes {
            p.text(
                arena.world(build_left - 0.2, lane as f32 + 0.5),
                Align2::RIGHT_CENTER,
                format!("{:02}", lane + 1),
                FontId::monospace(10.),
                MUTED,
            );
        }
        p.text(
            arena.world(build_right + 0.2, 0.2),
            Align2::LEFT_TOP,
            "NO BUILDING BEYOND MIDPOINT",
            FontId::proportional(10.0),
            MUTED,
        );
    }
    for s in &a.structures {
        let at = arena.world(s.x(), s.y());
        let pose = &motion.structures[&s.id];
        let direction = arena.world(pose.target[0], pose.target[1]) - at;
        let prop = match s.kind {
            StructureKind::Trap => Prop::Trap,
            StructureKind::Wall => Prop::Wall,
            StructureKind::Turret => Prop::Turret,
        };
        soft_shadow(p, at + vec2(2., sy * 0.29), vec2(sy * 0.48, sy * 0.14), 0.7);
        let tint = if s.kind == StructureKind::Trap && s.charges == 0 {
            C::from_rgb(132, 126, 109)
        } else {
            C::WHITE
        };
        if !visual_assets::prop_animated(
            p,
            textures,
            prop,
            at,
            vec2(arena.yard_width() * s.footprint_width(), sy * 0.88),
            tint,
            PropMotion {
                angle: direction.y.atan2(direction.x),
                target: Some(arena.world(pose.target[0], pose.target[1])),
                recoil: pose.recoil,
                trigger: pose.trigger,
                hit: pose.hit,
            },
        ) {
            structure_fallback(
                p,
                at,
                vec2(arena.yard_width() * s.footprint_width(), sy * 0.65),
                s.kind,
                direction,
                pose.recoil,
            );
        }
        if s.hp < s.max_hp {
            let damage = 1. - s.hp.max(0) as f32 / s.max_hp as f32;
            for n in 0..3 {
                let start = at + vec2((n as f32 - 1.) * sy * 0.12, -sy * 0.08);
                line(
                    p,
                    start,
                    start + vec2(sy * 0.06, sy * 0.17),
                    1.,
                    C::from_black_alpha((damage * 150.) as u8),
                );
            }
        }
        bar(
            p,
            Rect::from_center_size(at + vec2(0.0, sy * 0.42), vec2(sy * 0.6, 3.0)),
            s.hp as f32 / s.max_hp as f32,
            GREEN,
        );
        if s.kind == StructureKind::Trap {
            p.text(
                at + vec2(0.0, -sy * 0.42),
                Align2::CENTER_CENTER,
                format!("{}", s.charges),
                FontId::monospace(10.0),
                TEXT,
            );
        }
    }
    let corpse_height = |corpse: &animation::DeathPose| {
        let boss = corpse.appearance.is_some_and(|v| v.boss);
        visual_height(
            sy,
            boss,
            corpse.visual.as_ref(),
            corpse.appearance.map(|v| v.kind),
        )
    };
    let settled_batched = visual_assets::corpse_batch(
        p,
        textures,
        motion
            .deaths
            .iter()
            .filter(|body| body.collapse > 0.9)
            .map(|body| {
                let height = corpse_height(body);
                (
                    arena.world(body.at[0], body.at[1]) + vec2(0., height * 0.28),
                    vec2(height * 1.25, height * 0.45),
                    visual_tint(body.visual.as_ref()).gamma_multiply(0.8),
                )
            }),
    );
    for corpse in &motion.deaths {
        if settled_batched && corpse.collapse > 0.9 {
            continue;
        }
        let at = arena.world(corpse.at[0], corpse.at[1]);
        let height = corpse_height(corpse);
        if let Some(appearance) = corpse.appearance {
            let actor = corpse.visual.as_ref().map_or_else(
                || {
                    actor_kind(
                        appearance.kind,
                        appearance.boss,
                        appearance
                            .boss_kind
                            .unwrap_or(content().levels[a.level].boss),
                    )
                },
                |visual| actor_sprite(visual.sprite),
            );
            let pose = ActorPose {
                frame: corpse.frame,
                lean: corpse.lean,
                scale: [1. + corpse.collapse * 0.16, 1. - corpse.collapse * 0.72],
                ..Default::default()
            };
            visual_assets::actor_animated(
                p,
                textures,
                actor,
                at,
                vec2(height * 1.2, height),
                visual_tint(corpse.visual.as_ref()).gamma_multiply(0.8),
                &pose,
            );
        } else {
            p.add(egui::Shape::ellipse_filled(
                at + vec2(0., sy * 0.25),
                vec2(sy * 0.3, sy * 0.10),
                C::from_black_alpha(90),
            ));
        }
    }
    let mut visible: Vec<_> = a
        .enemies
        .iter()
        .filter(|e| e.condition(a.mode) != horde_core::sim::EnemyCondition::Defeated)
        .collect();
    // Passing offsets are authoritative geometry. Painter order follows actual
    // feet rather than creation order when a faster actor passes behind another.
    visible.sort_by(|a, b| a.y().total_cmp(&b.y()).then(a.id.cmp(&b.id)));
    for e in visible {
        if !e.in_view() {
            continue;
        }
        let at = arena.world(e.x, e.y());
        let pose = &motion.enemies[&e.id];
        let scale = if e.boss {
            0.57
        } else if e.kind == EnemyKind::Brute {
            0.44
        } else {
            0.34
        };
        let phase = if reduced || !pose.moving {
            0.0
        } else {
            a.elapsed()
                * if e.kind == EnemyKind::Runner {
                    12.0
                } else {
                    5.0
                }
                + e.id as f32
        };
        if e.contact() {
            let rr = Rect::from_min_max(
                arena.world(1.5, e.lane as f32 + 0.05),
                arena.world(build_left, e.lane as f32 + 0.95),
            );
            p.rect_filled(rr, 0.0, RED.gamma_multiply(0.18));
            let ready = if e.attack_phase == EnemyPhase::Recovery {
                (1. - e.attack_left as f32 / e.windup_total.max(1) as f32).clamp(0., 1.)
            } else {
                pose.windup
            };
            line(
                p,
                arena.world(1.52, e.lane as f32 + 0.95),
                arena.world(1.52, e.lane as f32 + 0.95 - ready * 0.9),
                3.0,
                if e.attack_phase == EnemyPhase::Recovery {
                    GREEN
                } else {
                    RED
                },
            );
        }
        let visual = e.resolved.as_ref().map(|profile| &profile.visual);
        let actor = visual.map_or_else(
            || {
                actor_kind(
                    e.kind,
                    e.boss,
                    e.boss_kind.unwrap_or(content().levels[a.level].boss),
                )
            },
            |visual| actor_sprite(visual.sprite),
        );
        let has_walk =
            (actor as usize) < textures.walkers.len() && textures.walkers[actor as usize].is_some();
        let painted_at = arena.world(
            e.x + pose.offset[0],
            e.y() + if has_walk { 0. } else { pose.offset[1] },
        );
        let figure_height = visual_height(sy, e.boss, visual, Some(e.kind));
        if outlines {
            p.circle_stroke(at, sy * 0.4, Stroke::new(1., AMBER));
        }
        if e.attack_phase == EnemyPhase::Windup {
            let progress = pose.windup;
            p.add(egui::Shape::ellipse_stroke(
                at + vec2(0., sy * 0.3),
                vec2(sy * 0.36, sy * 0.13),
                Stroke::new(2., RED),
            ));
            bar(
                p,
                Rect::from_center_size(at + vec2(0., sy * 0.43), vec2(sy * 0.66, 3.)),
                progress,
                RED,
            );
        }
        soft_shadow(
            p,
            at + vec2(1., figure_height * 0.48),
            vec2(sy * 0.26 * (1. + pose.lunge * 0.12), sy * 0.095),
            0.7,
        );
        if !visual_assets::actor_animated(
            p,
            textures,
            actor,
            painted_at,
            vec2(figure_height * 1.2, figure_height),
            visual_tint(visual),
            pose,
        ) {
            figure(p, painted_at, sy * scale, e.kind, e.boss, phase, false);
        }
        if e.hit_stop_left > 0 {
            visual_assets::actor_hit_flash(
                p,
                textures,
                actor,
                painted_at,
                vec2(figure_height * 1.2, figure_height),
                HitFlash {
                    strength: (e.hit_stop_left as f32
                        / horde_core::sim::ENEMY_HIT_STOP_TICKS as f32)
                        .sqrt(),
                    reduced,
                },
                pose,
            );
        }
        if e.injury.stunned {
            p.circle_stroke(at + vec2(0., -sy * 0.3), sy * 0.16, Stroke::new(1.4, AMBER));
        }
        if e.critically_wounded(a.mode) {
            p.text(
                at + vec2(0.0, -sy * 0.44),
                Align2::CENTER_BOTTOM,
                "CRIT · FIGHTING",
                FontId::proportional(8.0),
                AMBER,
            );
        } else if e.hp < e.max_hp || e.boss {
            bar(
                p,
                Rect::from_center_size(
                    at + vec2(0.0, -sy * 0.41),
                    vec2(if e.boss { sy * 1.1 } else { sy * 0.65 }, 3.0),
                ),
                (e.hp.max(0) as f32 / e.max_hp as f32).clamp(0.0, 1.0),
                if e.boss { AMBER } else { RED },
            );
        }
        if e.boss {
            let label = if e.attack_phase == EnemyPhase::Recovery {
                "RECOVERING · WINDOW OPEN"
            } else if e.exposed_at(a.tick) {
                "EXPOSED · FIRE NOW"
            } else {
                match content().levels[a.level].boss {
                    BossKind::Breacher => "BREACHER",
                    BossKind::Caller => "CALLER",
                    BossKind::SiegeBrute => "SIEGE BRUTE",
                }
            };
            p.text(
                at + vec2(0.0, -sy * 0.7),
                Align2::CENTER_CENTER,
                label,
                FontId::proportional(10.0),
                AMBER,
            );
        }
    }
    let origin = arena.world(a.defender_position()[0], a.defender_position()[1]);
    soft_shadow(
        p,
        origin + vec2(1., sy * 0.87 * 0.48),
        vec2(sy * 0.30, sy * 0.11),
        0.8,
    );
    let equipment = crate::gear_appearance::from_frozen_gear(&a.profile, &a.gear_snapshot);
    let appearance = if a.mode == Mode::Npc {
        horde_core::survivors::Appearance::Female
    } else {
        a.profile.appearance
    };
    let manual_target = arena.world(aim[0], aim[1]);
    let mut player_sample = crate::defender_presentation::sample(
        a,
        &motion.defender,
        [manual_target.x, manual_target.y],
        crate::defender_presentation::brace_at(a, a.tick),
        reduced,
    );
    let target =
        crate::defender_presentation::target_world(a, &motion.defender, player_sample.action, aim);
    let target = arena.world(target[0], target[1]);
    player_sample.aim = [target.x, target.y];
    let player_pose = visual_assets::player_model_id(appearance).and_then(|model| {
        visual_assets::resolve_player_model(
            &model,
            equipment.as_ref(),
            player_sample,
            origin,
            vec2(sy * 0.82, sy * 0.87),
        )
    });
    let player_pose = match player_pose {
        Ok(pose) => match visual_assets::draw_player_model(p, &pose) {
            Ok(()) => Some(pose),
            Err(error) => {
                player_model_error(p, origin, &error);
                None
            }
        },
        Err(error) => {
            player_model_error(p, origin, &error);
            None
        }
    };
    paint_defender_feedback(p, origin, sy, &motion.defender, reduced);
    paint_thrown_ordnance(p, r, a, reduced, textures, equipment.as_ref());
    for e in &a.events {
        let age = a.tick.saturating_sub(e.tick) as f32 / 60.0;
        let alpha = (1.0 - age / 0.45).clamp(0.0, 1.0);
        match e.kind {
            EventKind::Explosion if age < 0.5 => {
                let at = arena.world(e.to[0], e.to[1]);
                let progress = (age / 0.5).clamp(0., 1.);
                let radius = (e.value as f32 / 100.).max(0.1);
                let range = vec2(arena.yard_width(), sy) * radius;
                let alpha = ((1. - progress) * if reduced { 105. } else { 190. }) as u8;
                p.add(egui::Shape::ellipse_stroke(
                    at,
                    range * if reduced { 1. } else { 0.25 + progress * 0.75 },
                    Stroke::new(
                        if reduced { 1. } else { 2. },
                        C::from_rgba_unmultiplied(239, 181, 82, alpha),
                    ),
                ));
                let sprite = !reduced
                    && visual_assets::ordnance_effect(
                        p,
                        textures,
                        false,
                        at,
                        range * (0.9 + progress * 0.8),
                        C::from_white_alpha(alpha),
                    );
                if !reduced && !sprite {
                    for n in 0..15 {
                        let theta = n as f32 * 2.399_963;
                        let spread = ((n + 1) as f32 / 16.).sqrt() * (0.15 + progress * 0.85);
                        let point =
                            at + vec2(theta.cos() * range.x, theta.sin() * range.y) * spread;
                        let size = sy * (0.10 + (1. - progress) * 0.14);
                        box_at(
                            p,
                            point,
                            vec2(size, size),
                            C::from_rgba_unmultiplied(230, 152 + (n % 3) as u8 * 20, 65, alpha),
                        );
                    }
                }
            }
            EventKind::BossWarning if age < 1.0 => {
                p.rect_filled(
                    Rect::from_center_size(
                        arena.world(a.width_yards * 0.8, a.defender_position()[1]),
                        vec2((r.width() * 0.34).min(290.), 36.),
                    ),
                    2.,
                    PANEL,
                );
                p.text(
                    arena.world(a.width_yards * 0.8, a.defender_position()[1]),
                    Align2::CENTER_CENTER,
                    "BOSS APPROACHING",
                    FontId::proportional(14.),
                    AMBER,
                );
            }
            EventKind::Reinforcements if age < 0.45 => {
                let lane = e.lane.unwrap_or(a.lane_count / 2) as f32 + 0.5;
                p.text(
                    arena.world(a.width_yards - 1.0, lane),
                    Align2::RIGHT_CENTER,
                    "REINFORCEMENTS",
                    FontId::proportional(10.),
                    AMBER,
                );
            }
            EventKind::Spawn | EventKind::Boss if age < 0.45 => {
                let at = arena.world(a.width_yards - 0.5, e.to[1]);
                p.add(egui::Shape::convex_polygon(
                    vec![
                        at + vec2(0.0, -6.0),
                        at + vec2(-8.0, 0.0),
                        at + vec2(0.0, 6.0),
                    ],
                    AMBER,
                    Stroke::NONE,
                ));
                p.text(
                    at + vec2(-12.0, 0.0),
                    Align2::RIGHT_CENTER,
                    if e.kind == EventKind::Boss {
                        "BOSS"
                    } else {
                        "INCOMING"
                    },
                    FontId::proportional(10.0),
                    AMBER,
                );
            }
            EventKind::Shot
                if age < 0.12
                    && !e.actor_id.is_some_and(|id| {
                        a.structures
                            .iter()
                            .any(|s| s.id == id && s.kind == StructureKind::Trap)
                    }) =>
            {
                let from = if e.actor_id.is_none() {
                    player_pose
                        .as_ref()
                        .map(|pose| pos2(pose.muzzle[0], pose.muzzle[1]))
                        .unwrap_or_else(|| arena.world(e.from[0], e.from[1]))
                } else {
                    a.structures
                        .iter()
                        .find(|s| e.actor_id == Some(s.id) && s.kind == StructureKind::Turret)
                        .filter(|_| textures.props.is_some())
                        .map_or_else(
                            || arena.world(e.from[0], e.from[1]),
                            |s| {
                                let pose = &motion.structures[&s.id];
                                visual_assets::turret_muzzle(
                                    arena.world(s.x(), s.y()),
                                    vec2(arena.yard_width() * s.footprint_width(), sy * 0.88),
                                    PropMotion {
                                        target: Some(arena.world(pose.target[0], pose.target[1])),
                                        recoil: pose.recoil,
                                        ..Default::default()
                                    },
                                )
                            },
                        )
                };
                if !reduced && age < 0.065 {
                    let direction = (arena.world(e.to[0], e.to[1]) - from).normalized();
                    let side = vec2(-direction.y, direction.x);
                    let length = sy * (0.12 + (1. - age / 0.065) * 0.12);
                    p.add(egui::Shape::convex_polygon(
                        vec![
                            from - direction * 1.,
                            from + side * length * 0.3,
                            from + direction * length,
                            from - side * length * 0.3,
                        ],
                        C::from_rgba_unmultiplied(248, 207, 117, ((1. - age / 0.065) * 230.) as u8),
                        Stroke::NONE,
                    ));
                }
                line(
                    p,
                    from,
                    arena.world(e.to[0], e.to[1]),
                    if reduced { 1.0 } else { 1.25 },
                    AMBER.gamma_multiply(1.0 - age / 0.12),
                );
            }
            EventKind::Damage if age < 0.3 && e.target_id.is_none() => {
                line(
                    p,
                    arena.world(1.5, e.from[1]),
                    arena.world(1.5, e.from[1] + 0.5),
                    4.0,
                    RED.gamma_multiply(alpha),
                );
                p.circle_stroke(
                    origin,
                    sy * 0.7,
                    Stroke::new(2.0, RED.gamma_multiply(alpha)),
                );
            }
            EventKind::Hit
            | EventKind::WeakPoint
            | EventKind::Kill
            | EventKind::Armor
            | EventKind::Melee
                if alpha > 0.0 =>
            {
                let at = arena.world(e.to[0], e.to[1]);
                if blood && !reduced && e.kind == EventKind::Kill {
                    for n in 0..4 {
                        p.circle_filled(
                            at + vec2((n as f32 * 2.3).sin() * 9.0, (n as f32 * 4.2).cos() * 6.0),
                            2.0,
                            C::from_rgb(102, 52, 42).gamma_multiply(alpha),
                        );
                    }
                }
                let color = if e.kind == EventKind::Armor {
                    MUTED
                } else if e.kind == EventKind::WeakPoint {
                    AMBER
                } else {
                    TEXT
                };
                if !reduced {
                    for d in [
                        vec2(-1.0, -1.0),
                        vec2(1.0, -1.0),
                        vec2(-1.0, 1.0),
                        vec2(1.0, 1.0),
                    ] {
                        line(
                            p,
                            at + d * 4.0,
                            at + d * 8.0,
                            1.5,
                            color.gamma_multiply(alpha),
                        );
                    }
                }
                // Currency feedback stays separate from the real-time resolved
                // damage-number cache used by battlefield presentation.
                if numbers && e.kind == EventKind::Kill && e.value > 0 {
                    p.text(
                        at + vec2(0.0, -10.0 - if reduced { 0. } else { age * 24.0 }),
                        Align2::CENTER_CENTER,
                        if e.kind == EventKind::Kill {
                            format!("+{}c", e.value)
                        } else {
                            format!("{}", e.value)
                        },
                        FontId::proportional(13.0),
                        color.gamma_multiply(alpha),
                    );
                }
            }
            _ => {}
        }
    }
    paint_particles(p, arena, &motion);
}

fn paint_fire_zones(p: &Painter, r: Rect, a: &Attempt, reduced: bool, textures: ArtTextures) {
    let arena = ArenaProjection::for_attempt(r, a);
    let unit = vec2(arena.yard_width(), arena.lane_height());
    for zone in &a.ordnance.fire_zones {
        let center = ordnance::cell_center(zone.cell);
        let at = arena.world(center[0], center[1]);
        let radii = unit * zone.radius;
        let fade = (zone.ticks_left as f32 / 45.).min(1.);
        p.add(egui::Shape::ellipse_filled(
            at,
            radii,
            C::from_rgba_unmultiplied(79, 43, 23, (fade * 90.) as u8),
        ));
        p.add(egui::Shape::ellipse_stroke(
            at,
            radii,
            Stroke::new(
                1.,
                C::from_rgba_unmultiplied(229, 136, 58, (fade * 125.) as u8),
            ),
        ));
        // The authoritative radius remains readable; small pixel flames stay
        // sparse enough that hostiles and contact windups remain legible.
        let count = if reduced { 9 } else { 21 };
        for n in 0..count {
            let theta = n as f32 * 2.399_963 + (zone.id % 19) as f32 * 0.3;
            let spread = ((n + 1) as f32 / (count + 1) as f32).sqrt() * 0.83;
            let point = at + vec2(theta.cos() * radii.x, theta.sin() * radii.y) * spread;
            let flicker = if reduced {
                0.5
            } else {
                ((a.tick / 5 + n * 7) % 5) as f32 / 4.
            };
            let px = (unit.y * 0.055).max(1.);
            let height = px * (2. + flicker * 2.);
            if visual_assets::ordnance_effect(
                p,
                textures,
                true,
                point - vec2(0., height * 0.4),
                vec2(px * 4., height * 2.),
                C::from_white_alpha((fade * 220.) as u8),
            ) {
                continue;
            }
            box_at(
                p,
                point - vec2(0., height * 0.5),
                vec2(px * 2., height),
                C::from_rgba_unmultiplied(218, 104, 38, (fade * 190.) as u8),
            );
            box_at(
                p,
                point - vec2(px * 0.2, height * 0.3),
                vec2(px, height * 0.5),
                C::from_rgba_unmultiplied(246, 187, 79, (fade * 200.) as u8),
            );
        }
    }
}
fn player_model_error(p: &Painter, origin: Pos2, error: &str) {
    let key = egui::Id::new(("lastlight.player_model.error", p.layer_id().id));
    let changed = p.ctx().data_mut(|data| {
        if data.get_temp::<String>(key).as_deref() == Some(error) {
            false
        } else {
            data.insert_temp(key, error.to_owned());
            true
        }
    });
    if changed {
        warn!("Survivor artwork unavailable: {error}");
    }
    p.text(
        origin,
        Align2::CENTER_CENTER,
        "Survivor art unavailable",
        FontId::proportional(10.),
        RED,
    );
}

fn paint_thrown_ordnance(
    p: &Painter,
    r: Rect,
    a: &Attempt,
    reduced: bool,
    textures: ArtTextures,
    equipment: &horde_core::survivors::EquipmentVisual,
) {
    let arena = ArenaProjection::for_attempt(r, a);
    let sy = arena.lane_height();
    let origin = arena.world(a.defender_position()[0], a.defender_position()[1]);
    let size = vec2(sy * 0.82, sy * 0.87);
    let appearance = if a.mode == Mode::Npc {
        horde_core::survivors::Appearance::Female
    } else {
        a.profile.appearance
    };
    let model_id = visual_assets::player_model_id(appearance).ok();
    let cache_key = egui::Id::new(("lastlight.player_release.v1", p.layer_id().id));
    let mut releases = p
        .ctx()
        .data(|data| data.get_temp::<crate::defender_presentation::ReleaseOrigins>(cache_key))
        .unwrap_or_default();
    let live = a
        .ordnance
        .projectiles
        .iter()
        .map(|projectile| {
            (
                projectile.id,
                crate::defender_presentation::release_tick(a.tick, &a.events, projectile),
            )
        })
        .collect();
    releases.begin_frame(
        a.id,
        a.tick,
        model_id.as_deref().unwrap_or_default(),
        equipment,
        &live,
    );
    for projectile in &a.ordnance.projectiles {
        let end = ordnance::cell_center(projectile.cell);
        let end = arena.world(end[0], end[1]);
        let born = crate::defender_presentation::release_tick(a.tick, &a.events, projectile);
        let source = releases.source_point((projectile.id, born), || {
            let model_id = model_id.as_deref()?;
            let sample = crate::defender_presentation::release_sample_for_projectile(
                a,
                projectile,
                [end.x, end.y],
                reduced,
            );
            visual_assets::resolve_player_model(model_id, equipment, sample, origin, size)
                .ok()?
                .source_sockets
                .get("throw_origin")
                .copied()
        });
        let start = model_id
            .as_deref()
            .zip(source)
            .and_then(|(model_id, point)| {
                visual_assets::project_player_socket(model_id, point, origin, size).ok()
            })
            .unwrap_or(origin);
        let t = (1. - projectile.flight_left as f32 / projectile.flight_ticks.max(1) as f32)
            .clamp(0., 1.);
        let lift = (std::f32::consts::PI * t).sin()
            * sy
            * (0.9 + (end.x - start.x).abs() / r.width() * 1.2);
        let at = start.lerp(end, t) - vec2(0., lift);
        let shadow = start.lerp(end, t);
        soft_shadow(p, shadow, vec2(sy * 0.10, sy * 0.035), 0.45);
        let px = (sy * 0.045).max(1.);
        let painted = visual_assets::ordnance_sprite(
            p,
            textures,
            projectile.kind,
            at,
            vec2(sy * 0.2, sy * 0.27),
            if reduced { 0. } else { t * 7. },
        );
        if !painted {
            match projectile.kind {
                OrdnanceKind::Grenade => {
                    box_at(p, at, vec2(px * 2.5, px * 3.), C::from_rgb(101, 111, 66));
                    box_at(
                        p,
                        at - vec2(0., px * 2.),
                        vec2(px * 1.5, px),
                        C::from_rgb(193, 190, 143),
                    );
                }
                OrdnanceKind::Molotov => {
                    box_at(p, at, vec2(px * 2., px * 4.), C::from_rgb(112, 139, 84));
                    box_at(
                        p,
                        at - vec2(0., px * 2.4),
                        vec2(px, px * 1.5),
                        C::from_rgb(192, 177, 126),
                    );
                    box_at(
                        p,
                        at - vec2(px, px * 3.),
                        vec2(px * 1.5, px * 1.5),
                        C::from_rgb(239, 164, 59),
                    );
                }
            }
        }
        if !reduced {
            p.circle_stroke(end, sy * 0.11, Stroke::new(1., AMBER.gamma_multiply(0.5)));
        }
    }
    p.ctx()
        .data_mut(|data| data.insert_temp(cache_key, releases));
}
fn actor_sprite(sprite: VisualSprite) -> Actor {
    match sprite {
        VisualSprite::Shambler => Actor::Shambler,
        VisualSprite::Runner => Actor::Runner,
        VisualSprite::Brute => Actor::Brute,
        VisualSprite::Armored => Actor::Armored,
        VisualSprite::Demolisher => Actor::Demolisher,
        VisualSprite::Breacher => Actor::Breacher,
        VisualSprite::Caller => Actor::Caller,
        VisualSprite::SiegeBrute => Actor::SiegeBrute,
    }
}
fn visual_tint(visual: Option<&EnemyVisual>) -> C {
    visual.map_or(C::WHITE, |v| {
        C::from_rgba_unmultiplied(
            (v.tint[0] * 255.) as u8,
            (v.tint[1] * 255.) as u8,
            (v.tint[2] * 255.) as u8,
            (v.tint[3] * 255.) as u8,
        )
    })
}
fn visual_height(
    sy: f32,
    boss: bool,
    visual: Option<&EnemyVisual>,
    fallback: Option<EnemyKind>,
) -> f32 {
    if boss {
        sy * 1.08
    } else if let Some(v) = visual {
        sy * if v.sprite == VisualSprite::Brute {
            0.9
        } else {
            0.78
        } * v.scale
    } else if fallback == Some(EnemyKind::Brute) {
        sy * 0.9
    } else {
        sy * 0.78
    }
}
fn actor_kind(kind: EnemyKind, boss: bool, boss_kind: BossKind) -> Actor {
    if boss {
        match boss_kind {
            BossKind::Breacher => Actor::Breacher,
            BossKind::Caller => Actor::Caller,
            BossKind::SiegeBrute => Actor::SiegeBrute,
        }
    } else {
        match kind {
            EnemyKind::Shambler => Actor::Shambler,
            EnemyKind::Runner => Actor::Runner,
            EnemyKind::Brute => Actor::Brute,
            EnemyKind::Armored => Actor::Armored,
            EnemyKind::Demolisher => Actor::Demolisher,
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
fn paint_defender_feedback(
    p: &Painter,
    origin: Pos2,
    lane_height: f32,
    pose: &animation::DefenderPose,
    reduced: bool,
) {
    if pose.hit > 0. {
        p.circle_stroke(
            origin,
            lane_height * 0.52,
            Stroke::new(if reduced { 1. } else { 1.5 }, RED.gamma_multiply(pose.hit)),
        );
    }
}
fn paint_ambient(p: &Painter, arena: ArenaProjection, frame: &animation::SceneAnimation) {
    let sy = arena.lane_height();
    for particle in &frame.particles {
        let at = arena.world(particle.at[0], particle.at[1]);
        let alpha = (particle.alpha * 255.).clamp(0., 255.) as u8;
        match particle.kind {
            ParticleKind::Rain => {
                let direction = vec2(particle.angle.cos(), particle.angle.sin());
                line(
                    p,
                    at - direction * sy * particle.size,
                    at,
                    0.8,
                    C::from_rgba_unmultiplied(153, 176, 181, alpha),
                );
            }
            ParticleKind::Snow => {
                p.circle_filled(
                    at,
                    (sy * particle.size).clamp(0.7, 1.7),
                    C::from_rgba_unmultiplied(211, 219, 211, alpha),
                );
            }
            ParticleKind::Mist => {
                p.add(egui::Shape::ellipse_filled(
                    at,
                    vec2(sy * particle.size * 3.5, sy * particle.size * 0.35),
                    C::from_rgba_unmultiplied(155, 171, 156, alpha),
                ));
            }
            ParticleKind::Ash => {
                p.circle_filled(
                    at,
                    (sy * particle.size).max(0.5),
                    C::from_rgba_unmultiplied(175, 171, 154, alpha),
                );
            }
            // Ambient motes share the dust material, while impact dust is drawn
            // above actors only when its stronger opacity marks a resolved hit.
            ParticleKind::Dust if particle.alpha <= 0.15 => {
                p.circle_filled(
                    at,
                    (sy * particle.size).max(0.5),
                    C::from_rgba_unmultiplied(169, 151, 109, alpha),
                );
            }
            _ => {}
        }
    }
}
fn paint_particles(p: &Painter, arena: ArenaProjection, frame: &animation::SceneAnimation) {
    let sy = arena.lane_height();
    for particle in &frame.particles {
        let at = arena.world(particle.at[0], particle.at[1]);
        let alpha = (particle.alpha * 255.).clamp(0., 255.) as u8;
        match particle.kind {
            ParticleKind::Casing => {
                let d = vec2(particle.angle.cos(), particle.angle.sin()) * sy * particle.size;
                line(
                    p,
                    at - d,
                    at + d,
                    1.1,
                    C::from_rgba_unmultiplied(180, 143, 67, alpha),
                );
            }
            ParticleKind::Spark => {
                let d = vec2(particle.angle.cos(), particle.angle.sin()) * sy * particle.size * 1.8;
                line(
                    p,
                    at - d,
                    at + d,
                    1.,
                    C::from_rgba_unmultiplied(234, 212, 158, alpha),
                );
            }
            ParticleKind::Debris => {
                let d = vec2(particle.angle.cos(), particle.angle.sin()) * sy * particle.size;
                line(
                    p,
                    at - d,
                    at + d,
                    (sy * particle.size * 0.65).max(1.),
                    C::from_rgba_unmultiplied(136, 125, 91, alpha),
                );
            }
            ParticleKind::Dust if particle.alpha > 0.15 => {
                p.circle_filled(
                    at,
                    sy * particle.size,
                    C::from_rgba_unmultiplied(156, 144, 109, alpha),
                );
            }
            _ => {}
        }
    }
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
            terrain(
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
