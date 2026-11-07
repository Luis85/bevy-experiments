//! Bodies and live hostiles: sprite, shadow, contact and windup gauges, impact
//! light, health and boss callouts.
//!
//! Hostiles paint as layered meshes ([`super::batch::FieldMeshes`]) whatever
//! their number: the underlay (one contact band per lane, one windup ground
//! ring per stack), one
//! shadow mesh, one sprite mesh in the view's painter order (lane feet, x
//! descending, id; hostiles at their crowd-staggered spot) with each hit flash
//! right after its sprite, the overlay (outline, stun and windup rings,
//! health and windup bars), then the callout text. Rejected enemy art keeps
//! the per-enemy procedural figure path.
use super::super::{AMBER, ArenaProjection, GREEN, RED, bar, figure, line, soft_shadow};
use super::batch::{FieldMeshes, MeshBuilder};
use crate::{
    battle_view::{BattleViewModel, ContactView, EnemyView, HealthView},
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
        for (lane, contact) in lane_contacts(view) {
            let (band, gauge) = contact_band(arena, view, lane, contact);
            p.rect_filled(band, 0.0, RED.gamma_multiply(0.18));
            line(p, gauge[0], gauge[1], 3.0, gauge_color(contact));
        }
        for (e, ring) in view.enemies.iter().zip(windup_rings(view)) {
            enemy(p, arena, e, ring, textures, outlines, reduced);
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
    for (lane, contact) in lane_contacts(view) {
        let (band, gauge) = contact_band(arena, view, lane, contact);
        layers.underlay.quad(band, RED.gamma_multiply(0.18));
        layers
            .underlay
            .line(gauge[0], gauge[1], 3.0, gauge_color(contact));
    }
    for (e, ring) in view.enemies.iter().zip(windup_rings(view)) {
        let [x, y] = e.drawn();
        let at = arena.world(x, y);
        let pose = &e.pose;
        if ring {
            windup_ring(&mut layers.underlay, sy, at);
        }
        let actor = e.sprite.map(Actor::Enemy);
        let has_walk = e.sprite.is_some_and(|s| s.is_walk()) && textures.walkers.is_some();
        let painted_at = arena.world(
            x + pose.offset[0],
            y + if has_walk { 0. } else { pose.offset[1] },
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

/// The contact of each lane with a hostile on the line: one band per lane
/// however many hostiles stack there (W1-69, enemies overlap), so the band
/// keeps one shade and its gauge shows the readiest of them.
pub(super) fn lane_contacts(view: &BattleViewModel) -> Vec<(u8, ContactView)> {
    let mut lanes: Vec<Option<ContactView>> = vec![None; usize::from(view.lanes)];
    for e in &view.enemies {
        if let Some(contact) = e.contact
            && let Some(slot) = lanes.get_mut(usize::from(e.lane))
            && slot.is_none_or(|held| contact.ready > held.ready)
        {
            *slot = Some(contact);
        }
    }
    lanes
        .into_iter()
        .zip(0u8..)
        .filter_map(|(contact, lane)| contact.map(|contact| (lane, contact)))
        .collect()
}

/// A contact lane's band and its readiness gauge (from, to).
fn contact_band(
    arena: ArenaProjection,
    view: &BattleViewModel,
    lane: u8,
    contact: ContactView,
) -> (Rect, [Pos2; 2]) {
    let lane = f32::from(lane);
    (
        Rect::from_min_max(
            arena.world(1.5, lane + 0.05),
            arena.world(view.construction_bounds[0], lane + 0.95),
        ),
        [
            arena.world(1.52, lane + 0.95),
            arena.world(1.52, lane + 0.95 - contact.ready * 0.9),
        ],
    )
}

fn gauge_color(contact: ContactView) -> C {
    if contact.recovering { GREEN } else { RED }
}

/// Hostiles closer than this along their lane share one windup ring.
pub(super) const STACK_YARDS: f32 = 0.3;

/// Which hostiles of `view.enemies` draw a windup ground ring: every one
/// winding up, except one within [`STACK_YARDS`] of a body already ringed in
/// its lane. Bodies stack on the line and at walls since W1-69 (enemies
/// overlap), and one ring per stack reads where six would smear; each body
/// still draws its own windup bar.
pub(super) fn windup_rings(view: &BattleViewModel) -> Vec<bool> {
    let mut ringed: Vec<Option<f32>> = vec![None; usize::from(view.lanes)];
    view.enemies
        .iter()
        .map(|e| {
            if e.windup.is_none() {
                return false;
            }
            let Some(last) = ringed.get_mut(usize::from(e.lane)) else {
                return true;
            };
            if last.is_some_and(|x| (x - e.position[0]).abs() < STACK_YARDS) {
                return false;
            }
            *last = Some(e.position[0]);
            true
        })
        .collect()
}

/// How far over its feet, in lane heights, a hostile's health bar is centred.
pub(crate) const HEALTH_BAR_LIFT: f32 = 0.41;
/// The windup ground ring under the feet at `at`; the contact bands are drawn
/// per lane first.
fn windup_ring(mesh: &mut MeshBuilder, sy: f32, at: Pos2) {
    mesh.ring(
        at + vec2(0., sy * 0.3),
        vec2(sy * 0.36, sy * 0.13),
        2.,
        RING_SEGMENTS,
        RED,
    );
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
                at + vec2(0.0, -sy * HEALTH_BAR_LIFT),
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
    e: &EnemyView,
    ring: bool,
    textures: ArtTextures,
    outlines: bool,
    reduced: bool,
) {
    let sy = arena.lane_height();
    let [x, y] = e.drawn();
    let at = arena.world(x, y);
    let pose = &e.pose;
    let actor = e.sprite.map(Actor::Enemy);
    let has_walk = e.sprite.is_some_and(|s| s.is_walk()) && textures.walkers.is_some();
    let painted_at = arena.world(
        x + pose.offset[0],
        y + if has_walk { 0. } else { pose.offset[1] },
    );
    let figure_height = e.height.pixels(sy);
    if outlines {
        p.circle_stroke(at, sy * 0.4, Stroke::new(1., AMBER));
    }
    if let Some(progress) = e.windup {
        if ring {
            p.add(egui::Shape::ellipse_stroke(
                at + vec2(0., sy * 0.3),
                vec2(sy * 0.36, sy * 0.13),
                Stroke::new(2., RED),
            ));
        }
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
                    at + vec2(0.0, -sy * HEALTH_BAR_LIFT),
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
