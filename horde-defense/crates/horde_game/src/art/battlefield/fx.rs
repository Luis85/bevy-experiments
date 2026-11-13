//! Weather, impact particles and short-lived event effects: explosions,
//! callouts, tracers, contact damage and hit sparks.
use super::super::{AMBER, ArenaProjection, MUTED, RED, TEXT, box_at, line};
use super::batch::MeshBuilder;
use crate::{
    animation::{Particle, ParticleKind},
    battle_view::BattleViewModel,
    visual_assets::{self, ArtTextures, PropMotion},
};
use bevy_egui::egui::{
    self, Align2, Color32 as C, FontId, Painter, Pos2, Rect, Stroke, Vec2, vec2,
};
use horde_core::{content::StructureKind, sim::EventKind};

pub(super) struct FxOptions {
    pub numbers: bool,
    pub reduced: bool,
    pub blood: bool,
    pub textures: ArtTextures,
    /// See [`super::RenderOptions::occluder`].
    pub occluder: Option<Rect>,
}

/// The least gap between a field callout and the overlay that would cover it.
pub(crate) const OCCLUDER_GAP: f32 = 4.0;

/// The REINFORCEMENTS label's right-centre anchor: a yard inside the right
/// edge of a `width`-yard field on the lane centre line `lane`, or
/// [`OCCLUDER_GAP`] left of an `occluder` that would cover it.
pub(crate) fn reinforcements_anchor(
    arena: ArenaProjection,
    width: f32,
    lane: f32,
    occluder: Option<Rect>,
) -> Pos2 {
    let mut at = arena.world(width - 1.0, lane);
    if let Some(occluder) = occluder {
        at.x = at.x.min(occluder.left() - OCCLUDER_GAP);
    }
    at
}

/// Segments of a batched particle dot.
const DOT_SEGMENTS: u32 = 8;

/// How long after its tick an event can still be painted, in seconds: the
/// single table the field view's event filter and the painters share. No
/// painter draws an event older than this. No painter here draws the boss
/// warning or the boss's arrival any more: their banner runs on the wall
/// clock (`ui::battle::boss_banner`, W3-14), and the warning's 2.0 s is that
/// banner's nominal time.
pub(crate) fn fx_lifetime(kind: EventKind) -> f32 {
    match kind {
        EventKind::BossWarning => 2.0,
        EventKind::Explosion => 0.5,
        _ => 0.45,
    }
}

/// Weather and ambient motes as one untextured mesh (lines as 4-vertex
/// quads, dots as 8-segment fans, no feathering, sub-pixel ones floored at
/// one pixel), under every body.
pub(super) fn ambient(p: &Painter, arena: ArenaProjection, particles: &[Particle]) {
    let sy = arena.lane_height();
    let mut mesh = MeshBuilder::untextured(particles.len() * 2).pixel_floor(p.pixels_per_point());
    for particle in particles {
        let at = arena.world(particle.at[0], particle.at[1]);
        let alpha = (particle.alpha * 255.).clamp(0., 255.) as u8;
        match particle.kind {
            ParticleKind::Rain => {
                let direction = vec2(particle.angle.cos(), particle.angle.sin());
                mesh.line(
                    at - direction * sy * particle.size,
                    at,
                    0.8,
                    C::from_rgba_unmultiplied(153, 176, 181, alpha),
                );
            }
            ParticleKind::Snow => {
                mesh.fan(
                    at,
                    Vec2::splat((sy * particle.size).clamp(0.7, 1.7)),
                    DOT_SEGMENTS,
                    C::from_rgba_unmultiplied(211, 219, 211, alpha),
                );
            }
            ParticleKind::Mist => {
                mesh.fan(
                    at,
                    vec2(sy * particle.size * 3.5, sy * particle.size * 0.35),
                    DOT_SEGMENTS,
                    C::from_rgba_unmultiplied(155, 171, 156, alpha),
                );
            }
            ParticleKind::Ash => {
                mesh.fan(
                    at,
                    Vec2::splat((sy * particle.size).max(0.5)),
                    DOT_SEGMENTS,
                    C::from_rgba_unmultiplied(175, 171, 154, alpha),
                );
            }
            // Ambient motes share the dust material, while impact dust is drawn
            // above actors only when its stronger opacity marks a resolved hit.
            ParticleKind::Dust if particle.alpha <= 0.15 => {
                mesh.fan(
                    at,
                    Vec2::splat((sy * particle.size).max(0.5)),
                    DOT_SEGMENTS,
                    C::from_rgba_unmultiplied(169, 151, 109, alpha),
                );
            }
            _ => {}
        }
    }
    mesh.paint(p);
}

/// Impact particles as one untextured mesh above every actor.
pub(super) fn particles(p: &Painter, arena: ArenaProjection, particles: &[Particle]) {
    let sy = arena.lane_height();
    let mut mesh = MeshBuilder::untextured(particles.len() * 2).pixel_floor(p.pixels_per_point());
    for particle in particles {
        let at = arena.world(particle.at[0], particle.at[1]);
        let alpha = (particle.alpha * 255.).clamp(0., 255.) as u8;
        match particle.kind {
            ParticleKind::Casing => {
                let d = vec2(particle.angle.cos(), particle.angle.sin()) * sy * particle.size;
                mesh.line(
                    at - d,
                    at + d,
                    1.1,
                    C::from_rgba_unmultiplied(180, 143, 67, alpha),
                );
            }
            ParticleKind::Spark => {
                let d = vec2(particle.angle.cos(), particle.angle.sin()) * sy * particle.size * 1.8;
                mesh.line(
                    at - d,
                    at + d,
                    1.,
                    C::from_rgba_unmultiplied(234, 212, 158, alpha),
                );
            }
            ParticleKind::Debris => {
                let d = vec2(particle.angle.cos(), particle.angle.sin()) * sy * particle.size;
                mesh.line(
                    at - d,
                    at + d,
                    (sy * particle.size * 0.65).max(1.),
                    C::from_rgba_unmultiplied(136, 125, 91, alpha),
                );
            }
            ParticleKind::Dust if particle.alpha > 0.15 => {
                mesh.fan(
                    at,
                    Vec2::splat(sy * particle.size),
                    DOT_SEGMENTS,
                    C::from_rgba_unmultiplied(156, 144, 109, alpha),
                );
            }
            _ => {}
        }
    }
    mesh.paint(p);
}

/// `muzzle` is the drawn survivor's muzzle socket, when the rig resolved.
pub(super) fn events(
    p: &Painter,
    arena: ArenaProjection,
    view: &BattleViewModel,
    muzzle: Option<Pos2>,
    options: &FxOptions,
) {
    let FxOptions {
        numbers,
        reduced,
        blood,
        textures,
        occluder,
    } = *options;
    let sy = arena.lane_height();
    let origin = arena.world(view.defender.position[0], view.defender.position[1]);
    for e in &view.fx {
        let age = e.age;
        let alpha = (1.0 - age / fx_lifetime(e.kind)).clamp(0.0, 1.0);
        match e.kind {
            EventKind::Explosion if age < fx_lifetime(EventKind::Explosion) => {
                let at = arena.world(e.to[0], e.to[1]);
                let progress = (age / fx_lifetime(EventKind::Explosion)).clamp(0., 1.);
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
            EventKind::Reinforcements if age < fx_lifetime(EventKind::Reinforcements) => {
                let lane = e.lane.unwrap_or(view.lanes / 2) as f32 + 0.5;
                p.text(
                    reinforcements_anchor(arena, view.width, lane, occluder),
                    Align2::RIGHT_CENTER,
                    "REINFORCEMENTS",
                    FontId::proportional(10.),
                    AMBER,
                );
            }
            // The boss's arrival is the wall-clock banner's, not a spawn tag.
            EventKind::Spawn if age < fx_lifetime(e.kind) => {
                let at = arena.world(view.width - 0.5, e.to[1]);
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
                    "INCOMING",
                    FontId::proportional(10.0),
                    AMBER,
                );
            }
            EventKind::Shot
                if age < 0.12
                    && !e.actor_id.is_some_and(|id| {
                        view.structure(id)
                            .is_some_and(|s| s.kind == StructureKind::Trap)
                    }) =>
            {
                let from = if e.actor_id.is_none() {
                    muzzle.unwrap_or_else(|| arena.world(e.from[0], e.from[1]))
                } else {
                    e.actor_id
                        .and_then(|id| view.structure(id))
                        .filter(|s| s.kind == StructureKind::Turret)
                        .filter(|_| textures.props.is_some())
                        .map_or_else(
                            || arena.world(e.from[0], e.from[1]),
                            |s| {
                                let pose = &s.pose;
                                visual_assets::turret_muzzle(
                                    arena.world(s.position[0], s.position[1]),
                                    vec2(arena.yard_width() * s.footprint_width, sy * 0.88),
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
                    let rise = if reduced { 0.0 } else { age * 24.0 };
                    coin_popup(p, at + vec2(0.0, -12.0 - rise), e.value, alpha, reduced);
                }
            }
            _ => {}
        }
    }
}
/// Banked-coin pop-up: amber with the same eight-way dark outline as the
/// damage numbers, so it reads against any ground. Reduced effects keep it
/// still (the caller drops the drift) with a thinner outline.
fn coin_popup(p: &Painter, center: Pos2, coins: i32, alpha: f32, reduced: bool) {
    let galley = p.layout_no_wrap(
        format!("+{coins}c"),
        FontId::proportional(if reduced { 15.0 } else { 16.0 }),
        AMBER,
    );
    let origin = center - galley.size() * 0.5;
    let outline = C::from_black_alpha((235.0 * alpha.clamp(0.0, 1.0)) as u8);
    let border = if reduced { 1.0 } else { 1.5 };
    for direction in [
        vec2(-1.0, -1.0),
        vec2(0.0, -1.0),
        vec2(1.0, -1.0),
        vec2(-1.0, 0.0),
        vec2(1.0, 0.0),
        vec2(-1.0, 1.0),
        vec2(0.0, 1.0),
        vec2(1.0, 1.0),
    ] {
        p.galley_with_override_text_color(origin + direction * border, galley.clone(), outline);
    }
    p.galley_with_override_text_color(origin, galley, AMBER.gamma_multiply(alpha));
}
