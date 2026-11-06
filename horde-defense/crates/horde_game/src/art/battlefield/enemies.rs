//! Bodies and live hostiles: sprite, shadow, contact and windup gauges, impact
//! light, health and boss callouts.
//!
//! Hostiles paint as layered meshes ([`super::batch::FieldMeshes`]) whatever
//! their number: the underlay (contact lane bands, windup ground rings), one
//! shadow mesh, one sprite mesh in feet-then-id order with each hit flash
//! right after its sprite, the overlay (outline, stun and windup rings,
//! health and windup bars), then the callout text. Rejected enemy art keeps
//! the per-enemy procedural figure path.
use super::super::{AMBER, ArenaProjection, GREEN, RED, bar, figure, line, soft_shadow};
use super::batch::{FieldMeshes, MeshBuilder};
use crate::{
    battle_view::{BattleViewModel, EnemyView, HealthView},
    visual_assets::{self, Actor, ArtTextures, HitFlash},
};
use bevy_egui::egui::{
    self, Align2, Color32 as C, FontId, Painter, Pos2, Rect, Stroke, Vec2, vec2,
};

/// Segments of the batched outline, stun and windup rings.
const RING_SEGMENTS: u32 = 20;

pub(super) fn tint(rgba: Option<[f32; 4]>) -> C {
    rgba.map_or(C::WHITE, |v| {
        C::from_rgba_unmultiplied(
            (v[0] * 255.) as u8,
            (v[1] * 255.) as u8,
            (v[2] * 255.) as u8,
            (v[3] * 255.) as u8,
        )
    })
}

/// The survivor rig's soft shadow, which the hostiles' shadow mesh draws.
#[derive(Clone, Copy, Debug)]
pub(super) struct RigShadow {
    pub at: Pos2,
    pub size: Vec2,
    pub alpha: f32,
}

/// What [`paint`] drew beyond the hostiles themselves.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct HostilesDrawn {
    /// The rig's shadow was drawn in the hostiles' shadow layer; the
    /// per-enemy path for rejected art leaves it to the rig.
    pub rig_shadow: bool,
    /// Hostiles drawn with a hit flash in the batched sprite layer.
    pub flashing: usize,
    /// Quads those hit flashes appended to the sprite layer.
    pub flash_quads: usize,
}

/// Paints every hostile and reports what it drew ([`HostilesDrawn`]).
pub(super) fn paint(
    p: &Painter,
    arena: ArenaProjection,
    view: &BattleViewModel,
    textures: ArtTextures,
    outlines: bool,
    reduced: bool,
    rig_shadow: RigShadow,
) -> HostilesDrawn {
    if textures.enemy_art_rejected {
        for e in &view.enemies {
            enemy(p, arena, view, e, textures, outlines, reduced);
        }
        return HostilesDrawn::default();
    }
    let mut drawn = HostilesDrawn {
        rig_shadow: true,
        ..Default::default()
    };
    let sy = arena.lane_height();
    let masks = visual_assets::flash_masks(p.ctx());
    let mut layers = FieldMeshes::new(textures, view.enemies.len());
    let mut figures = Vec::new();
    let mut labels = Vec::new();
    layers
        .shadows
        .shadow(rig_shadow.at, rig_shadow.size, rig_shadow.alpha);
    for e in &view.enemies {
        let at = arena.world(e.position[0], e.position[1]);
        let pose = &e.pose;
        underlay(&mut layers.underlay, arena, view, e, at);
        let actor = e.sprite.map(Actor::Enemy);
        let has_walk = e.sprite.is_some_and(|s| s.is_walk()) && textures.walkers.is_some();
        let painted_at = arena.world(
            e.position[0] + pose.offset[0],
            e.position[1] + if has_walk { 0. } else { pose.offset[1] },
        );
        let figure_height = e.height.pixels(sy);
        let size = vec2(figure_height * 1.2, figure_height);
        layers.shadows.shadow(
            at + vec2(1., figure_height * 0.48),
            vec2(sy * 0.26 * (1. + pose.lunge * 0.12), sy * 0.095),
            0.7,
        );
        let quad = actor.and_then(|actor| {
            visual_assets::actor_quad(textures, actor, painted_at, size, tint(e.tint), pose)
        });
        match (quad, actor) {
            (Some(quad), Some(actor)) => {
                layers.actors.sprite(&quad);
                if let (Some(strength), Some(masks)) = (e.hit_flash, masks.as_deref()) {
                    drawn.flashing += 1;
                    drawn.flash_quads += layers.actors.flash(
                        masks,
                        textures,
                        actor,
                        painted_at,
                        size,
                        HitFlash { strength, reduced },
                        pose,
                    );
                }
            }
            _ => figures.push((painted_at, e)),
        }
        overlay(&mut layers.overlay, sy, e, at, outlines);
        match e.health {
            HealthView::Critical => labels.push((
                at + vec2(0.0, -sy * 0.44),
                Align2::CENTER_BOTTOM,
                "CRIT · FIGHTING",
                8.0,
            )),
            HealthView::Bar { .. } | HealthView::Hidden => {}
        }
        if let Some(label) = e.label {
            labels.push((
                at + vec2(0.0, -sy * 0.7),
                Align2::CENTER_CENTER,
                label,
                10.0,
            ));
        }
    }
    layers.underlay.paint(p);
    layers.shadows.paint(p);
    layers.actors.paint(p);
    // A sprite missing from its sheet (never a rejected catalogue) falls back
    // to its procedural figure just above the sprite layer.
    for (painted_at, e) in figures {
        figure(
            p,
            painted_at,
            sy * e.fallback_scale,
            e.kind,
            e.boss,
            e.phase,
            false,
        );
    }
    layers.overlay.paint(p);
    for (at, align, text, size) in labels {
        p.text(at, align, text, FontId::proportional(size), AMBER);
    }
    drawn
}

/// Contact lane band with its readiness gauge, and the windup ground ring.
fn underlay(
    mesh: &mut MeshBuilder,
    arena: ArenaProjection,
    view: &BattleViewModel,
    e: &EnemyView,
    at: Pos2,
) {
    let sy = arena.lane_height();
    let build_left = view.construction_bounds[0];
    if let Some(contact) = e.contact {
        mesh.quad(
            Rect::from_min_max(
                arena.world(1.5, e.lane as f32 + 0.05),
                arena.world(build_left, e.lane as f32 + 0.95),
            ),
            RED.gamma_multiply(0.18),
        );
        mesh.line(
            arena.world(1.52, e.lane as f32 + 0.95),
            arena.world(1.52, e.lane as f32 + 0.95 - contact.ready * 0.9),
            3.0,
            if contact.recovering { GREEN } else { RED },
        );
    }
    if e.windup.is_some() {
        mesh.ring(
            at + vec2(0., sy * 0.3),
            vec2(sy * 0.36, sy * 0.13),
            2.,
            RING_SEGMENTS,
            RED,
        );
    }
}

/// Outline, windup bar, stun ring and health bar, above every sprite.
fn overlay(mesh: &mut MeshBuilder, sy: f32, e: &EnemyView, at: Pos2, outlines: bool) {
    if outlines {
        mesh.ring(at, Vec2::splat(sy * 0.4), 1., RING_SEGMENTS, AMBER);
    }
    if let Some(progress) = e.windup {
        mesh.bar(
            Rect::from_center_size(at + vec2(0., sy * 0.43), vec2(sy * 0.66, 3.)),
            progress,
            RED,
        );
    }
    if e.stunned {
        mesh.ring(
            at + vec2(0., -sy * 0.3),
            Vec2::splat(sy * 0.16),
            1.4,
            RING_SEGMENTS,
            AMBER,
        );
    }
    if let HealthView::Bar { fraction, boss } = e.health {
        mesh.bar(
            Rect::from_center_size(
                at + vec2(0.0, -sy * 0.41),
                vec2(if boss { sy * 1.1 } else { sy * 0.65 }, 3.0),
            ),
            fraction,
            if boss { AMBER } else { RED },
        );
    }
}

/// The per-enemy vector path for rejected enemy art: procedural figures.
fn enemy(
    p: &Painter,
    arena: ArenaProjection,
    view: &BattleViewModel,
    e: &EnemyView,
    textures: ArtTextures,
    outlines: bool,
    reduced: bool,
) {
    let sy = arena.lane_height();
    let build_left = view.construction_bounds[0];
    let at = arena.world(e.position[0], e.position[1]);
    let pose = &e.pose;
    if let Some(contact) = e.contact {
        let rr = Rect::from_min_max(
            arena.world(1.5, e.lane as f32 + 0.05),
            arena.world(build_left, e.lane as f32 + 0.95),
        );
        p.rect_filled(rr, 0.0, RED.gamma_multiply(0.18));
        line(
            p,
            arena.world(1.52, e.lane as f32 + 0.95),
            arena.world(1.52, e.lane as f32 + 0.95 - contact.ready * 0.9),
            3.0,
            if contact.recovering { GREEN } else { RED },
        );
    }
    let actor = e.sprite.map(Actor::Enemy);
    let has_walk = e.sprite.is_some_and(|s| s.is_walk()) && textures.walkers.is_some();
    let painted_at = arena.world(
        e.position[0] + pose.offset[0],
        e.position[1] + if has_walk { 0. } else { pose.offset[1] },
    );
    let figure_height = e.height.pixels(sy);
    if outlines {
        p.circle_stroke(at, sy * 0.4, Stroke::new(1., AMBER));
    }
    if let Some(progress) = e.windup {
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
    let drawn = actor.is_some_and(|actor| {
        visual_assets::actor_animated(
            p,
            textures,
            actor,
            painted_at,
            vec2(figure_height * 1.2, figure_height),
            tint(e.tint),
            pose,
        )
    });
    if !drawn {
        figure(
            p,
            painted_at,
            sy * e.fallback_scale,
            e.kind,
            e.boss,
            e.phase,
            false,
        );
    }
    if let (Some(strength), Some(actor)) = (e.hit_flash, actor) {
        visual_assets::actor_hit_flash(
            p,
            textures,
            actor,
            painted_at,
            vec2(figure_height * 1.2, figure_height),
            HitFlash { strength, reduced },
            pose,
        );
    }
    if e.stunned {
        p.circle_stroke(at + vec2(0., -sy * 0.3), sy * 0.16, Stroke::new(1.4, AMBER));
    }
    match e.health {
        HealthView::Critical => {
            p.text(
                at + vec2(0.0, -sy * 0.44),
                Align2::CENTER_BOTTOM,
                "CRIT · FIGHTING",
                FontId::proportional(8.0),
                AMBER,
            );
        }
        HealthView::Bar { fraction, boss } => {
            bar(
                p,
                Rect::from_center_size(
                    at + vec2(0.0, -sy * 0.41),
                    vec2(if boss { sy * 1.1 } else { sy * 0.65 }, 3.0),
                ),
                fraction,
                if boss { AMBER } else { RED },
            );
        }
        HealthView::Hidden => {}
    }
    if let Some(label) = e.label {
        p.text(
            at + vec2(0.0, -sy * 0.7),
            Align2::CENTER_CENTER,
            label,
            FontId::proportional(10.0),
            AMBER,
        );
    }
}
