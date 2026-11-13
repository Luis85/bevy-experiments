//! Weather, impact particles and short-lived event effects. World pass
//! ([`events`]): tracers, hit sparks (bursts are in [`super::explosion`]). Cue pass ([`event_cues`],
//! after the lighting grade): REINFORCEMENTS, the INCOMING spawn marker,
//! contact damage and banked-coin numbers, each on its INK backing
//! ([`super::cue`]). The boss warning and arrival are the wall-clock
//! banner's (`ui::battle::boss_banner`, W3-14).
use super::super::{AMBER, ArenaProjection, MUTED, RED, TEXT, line};
use super::batch::MeshBuilder;
use super::cue::{self, CueBatch, Label};
use super::weapon_fx;
use crate::{
    animation::{Particle, ParticleKind},
    battle_view::{BattleViewModel, FxView, Volley},
    visual_assets::{self, ArtTextures, PropMotion},
};
use bevy_egui::egui::{Align2, Color32 as C, Painter, Pos2, Rect, Vec2, vec2};
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

/// A Shot event's age in 60 Hz ticks.
pub(super) fn shot_age_ticks(e: &FxView) -> u32 {
    (e.age * 60.0).round() as u32
}

/// Where a Shot event's flash, tracer and light start: the drawn survivor's
/// `muzzle` socket (when the rig resolved) for the survivor, the drawn
/// barrel for a turret, else the event's own origin. `None` for a trap and
/// for an event without a source: neither draws a shot.
pub(super) fn shot_origin(
    arena: ArenaProjection,
    view: &BattleViewModel,
    e: &FxView,
    muzzle: Option<Pos2>,
    textures: ArtTextures,
) -> Option<Pos2> {
    e.source?;
    let Some(id) = e.actor_id else {
        return Some(muzzle.unwrap_or_else(|| arena.world(e.from[0], e.from[1])));
    };
    let structure = view.structure(id);
    if structure.is_some_and(|s| s.kind == StructureKind::Trap) {
        return None;
    }
    Some(
        structure
            .filter(|s| s.kind == StructureKind::Turret)
            .filter(|_| textures.props.is_some())
            .map_or_else(
                || arena.world(e.from[0], e.from[1]),
                |s| {
                    let pose = &s.pose;
                    visual_assets::turret_muzzle(
                        arena.world(s.position[0], s.position[1]),
                        vec2(
                            arena.yard_width() * s.footprint_width,
                            arena.lane_height() * 0.88,
                        ),
                        PropMotion {
                            target: Some(arena.world(pose.target[0], pose.target[1])),
                            recoil: pose.recoil,
                            ..Default::default()
                        },
                    )
                },
            ),
    )
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
        reduced,
        blood,
        textures,
        ..
    } = *options;
    let sy = arena.lane_height();
    for e in &view.fx {
        let age = e.age;
        let alpha = (1.0 - age / fx_lifetime(e.kind)).clamp(0.0, 1.0);
        match e.kind {
            EventKind::Shot => {
                if let (Some(source), Some(from)) =
                    (e.source, shot_origin(arena, view, e, muzzle, textures))
                {
                    weapon_fx::paint_shot(
                        p,
                        weapon_fx::weapon_fx(source),
                        weapon_fx::Shot {
                            from,
                            to: arena.world(e.to[0], e.to[1]),
                            lane: sy,
                            age_ticks: shot_age_ticks(e),
                            volley: e.volley.unwrap_or(Volley::SINGLE),
                            reduced,
                        },
                    );
                }
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
            }
            _ => {}
        }
    }
}
/// Cue pass: the event cues of `view`, collected into `batch`.
pub(super) fn event_cues(
    batch: &mut CueBatch,
    arena: ArenaProjection,
    view: &BattleViewModel,
    options: &FxOptions,
) {
    let FxOptions {
        numbers,
        reduced,
        occluder,
        ..
    } = *options;
    let sy = arena.lane_height();
    let font = cue::font(sy);
    let origin = arena.world(view.defender.position[0], view.defender.position[1]);
    for e in &view.fx {
        let age = e.age;
        let alpha = (1.0 - age / fx_lifetime(e.kind)).clamp(0.0, 1.0);
        match e.kind {
            EventKind::Reinforcements if age < fx_lifetime(EventKind::Reinforcements) => {
                let lane = e.lane.unwrap_or(view.lanes / 2) as f32 + 0.5;
                batch.label(Label::amber(
                    reinforcements_anchor(arena, view.width, lane, occluder),
                    Align2::RIGHT_CENTER,
                    "REINFORCEMENTS",
                    font,
                ));
            }
            // The boss's arrival is the wall-clock banner's, not a spawn tag.
            EventKind::Spawn if age < fx_lifetime(e.kind) => {
                let at = arena.world(view.width - 0.5, e.to[1]);
                batch.marker(
                    [
                        at + vec2(0.0, -6.0),
                        at + vec2(-8.0, 0.0),
                        at + vec2(0.0, 6.0),
                    ],
                    AMBER,
                );
                batch.label(Label::amber(
                    at + vec2(-12.0, 0.0),
                    Align2::RIGHT_CENTER,
                    "INCOMING",
                    font,
                ));
            }
            EventKind::Damage if age < 0.3 && e.target_id.is_none() => {
                batch.line(
                    arena.world(1.5, e.from[1]),
                    arena.world(1.5, e.from[1] + 0.5),
                    4.0,
                    RED.gamma_multiply(alpha),
                );
                batch.ellipse(
                    origin,
                    Vec2::splat(sy * 0.7),
                    2.0,
                    RED.gamma_multiply(alpha),
                );
            }
            // Currency feedback stays separate from the real-time resolved
            // damage-number cache used by battlefield presentation.
            EventKind::Kill if alpha > 0.0 && numbers && e.value > 0 => {
                let at = arena.world(e.to[0], e.to[1]);
                let rise = if reduced { 0.0 } else { age * 24.0 };
                batch.coin(at + vec2(0.0, -12.0 - rise), e.value, alpha, reduced);
            }
            _ => {}
        }
    }
}
