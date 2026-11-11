//! Bodies and live hostiles: sprite, shadow, contact and windup gauges, impact
//! light, health and boss callouts, and a holding boss's phase ring and label.
//!
//! Hostiles paint as layered meshes ([`super::batch::FieldMeshes`]) whatever
//! their number: the underlay (one contact band per lane, a holding boss's
//! phase ring, one windup ground ring per stack), one
//! shadow mesh, one sprite mesh in the view's painter order (lane feet, x
//! descending, id; hostiles at their crowd-staggered spot) with each hit flash
//! right after its sprite, the overlay (a ranged windup's dashed aim line,
//! outline, stun and windup rings, health and windup bars), then the callout
//! text, a holding boss's name and "PHASE n - NAME" last, moved as one
//! block so the clip never stacks them on one row. Rejected enemy art keeps
//! the per-enemy procedural figure path.
use super::super::{AMBER, ArenaProjection, GREEN, RED, bar, figure, line, soft_shadow};
use super::batch::{FieldMeshes, MeshBuilder};
use crate::{
    battle_view::{BattleViewModel, BossPhaseView, ContactView, EnemyView, HealthView},
    visual_assets::{self, Actor, ArtTextures, HitFlash},
};
use bevy_egui::egui::{
    self, Align2, Color32 as C, FontId, Painter, Pos2, Rect, Stroke, Vec2, pos2, vec2,
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
    let sy = arena.lane_height();
    // A holding boss's phase (W2-37): its ring under the bodies, its label
    // last, stacked over the boss's name (which the per-body callouts then
    // leave out).
    let phase = holding_boss(view).map(|(boss, phase)| {
        let [x, y] = boss.drawn();
        let at = arena.world(x, y);
        let (ring, label) = phase_anchors(at, sy, boss.height.pixels(sy));
        let radii = phase_ring(sy, phase.hold_left_ticks, reduced);
        let name = boss.label.map(|name| (name_anchor(at, sy), name));
        (ring, radii, label, phase.label(), boss.id, name)
    });
    let stacked = phase.as_ref().map(|phase| phase.4);
    if textures.enemy_art_rejected {
        for (lane, contact) in lane_contacts(view) {
            let (band, gauge) = contact_band(arena, view, lane, contact);
            p.rect_filled(band, 0.0, RED.gamma_multiply(0.18));
            line(p, gauge[0], gauge[1], 3.0, gauge_color(contact));
        }
        if let Some((ring, radii, ..)) = &phase {
            p.add(egui::Shape::ellipse_stroke(
                *ring,
                *radii,
                Stroke::new(PHASE_RING_WIDTH, AMBER),
            ));
        }
        for (e, ring) in view.enemies.iter().zip(windup_rings(view)) {
            let named = stacked != Some(e.id);
            enemy(p, arena, e, ring, textures, outlines, reduced, named);
        }
        if let Some((.., at, text, _, name)) = &phase {
            phase_callouts(p, *at, text, *name);
        }
        return HostilesDrawn::default();
    }
    let mut drawn = HostilesDrawn {
        rig_shadow: true,
        ..Default::default()
    };
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
    if let Some((ring, radii, ..)) = &phase {
        layers
            .underlay
            .ring(*ring, *radii, PHASE_RING_WIDTH, RING_SEGMENTS, AMBER);
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
        if let Some(to) = e.aim_target {
            for [a, b] in aim_dashes(at, arena.world(to[0], to[1]), sy) {
                layers.overlay.line(a, b, AIM_WIDTH, RED);
            }
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
        if let Some(label) = e.label
            && stacked != Some(e.id)
        {
            labels.push((name_anchor(at, sy), Align2::CENTER_CENTER, label, 10.0));
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
        callout(p, at, align, text, size);
    }
    if let Some((.., at, text, _, name)) = &phase {
        phase_callouts(p, *at, text, *name);
    }
    drawn
}

/// How far over its feet, in lane heights, a hostile's name callout is
/// centred.
const NAME_LIFT: f32 = 0.7;

/// Where the name callout of a hostile drawn at `at` is centred.
fn name_anchor(at: Pos2, sy: f32) -> Pos2 {
    at + vec2(0.0, -sy * NAME_LIFT)
}

/// A holding boss's "PHASE n - NAME" label, its bottom edge at `label_at`,
/// stacked as one block with the boss's `name` callout when it has one.
fn phase_callouts(p: &Painter, label_at: Pos2, text: &str, name: Option<(Pos2, &str)>) {
    let label = (label_at, Align2::CENTER_BOTTOM, text, 10.0);
    match name {
        Some((at, name)) => callouts(p, [(at, Align2::CENTER_CENTER, name, 10.0), label]),
        None => callouts(p, [label]),
    }
}

/// How far inside the battlefield clip a hostile's callout stays.
pub(super) const CALLOUT_MARGIN: f32 = 2.0;

/// The INK backing under every hostile callout (CF-2-05), at the alpha the
/// cue labels take (design-vfx-lighting.md, cue helpers).
pub(super) const CALLOUT_BACKING: C = C::from_rgba_unmultiplied_const(17, 23, 24, 190);
/// How far the backing reaches past its callout's text on every side.
pub(super) const CALLOUT_PAD: f32 = 2.0;

/// One amber callout: its anchor, the alignment there, its text and size.
type Callout<'a> = (Pos2, Align2, &'a str, f32);

/// A hostile's amber callout (boss name, EXPOSED, CRIT) anchored at `at` by
/// `align`, moved the least that keeps the whole text `CALLOUT_MARGIN` inside
/// the painter's clip, so a boss at the field's edge or in its top lane keeps
/// a readable name (CF-1-11). An anchor already inside paints exactly where
/// `Painter::text` did. Its text sits on a `CALLOUT_BACKING` plate padded
/// `CALLOUT_PAD` round it, so it stays legible over the bodies behind
/// (CF-2-05; the same in reduced effects).
fn callout(p: &Painter, at: Pos2, align: Align2, text: &str, size: f32) {
    callouts(p, [(at, align, text, size)]);
}

/// Callouts that move as one block, painted in order: each laid out at its
/// anchor, then the block (their union) moved the least that keeps it
/// `CALLOUT_MARGIN` inside the painter's clip, as [`callout`] moves one. A
/// holding boss's name and phase label stack this way, so clamped in the
/// top lane they keep their order and never overprint.
fn callouts<const N: usize>(p: &Painter, group: [Callout<'_>; N]) {
    let laid = group.map(|(at, align, text, size)| {
        let galley = p.layout_no_wrap(text.to_owned(), FontId::proportional(size), AMBER);
        (align.anchor_size(at, galley.size()), galley)
    });
    let block = laid
        .iter()
        .fold(Rect::NOTHING, |block, (rect, _)| block.union(*rect));
    let clip = p.clip_rect().shrink(CALLOUT_MARGIN);
    let min = pos2(
        block
            .min
            .x
            .min(clip.right() - block.width())
            .max(clip.left()),
        block
            .min
            .y
            .min(clip.bottom() - block.height())
            .max(clip.top()),
    );
    // Moved only when it would be cut: a block inside keeps its exact
    // position, so callouts away from the edges paint as before.
    let shift = if p.clip_rect().contains_rect(block) {
        Vec2::ZERO
    } else {
        min - block.min
    };
    // Every backing first, so none covers a text of the block (CF-2-05).
    for (rect, _) in &laid {
        p.rect_filled(
            rect.translate(shift).expand(CALLOUT_PAD),
            2.0,
            CALLOUT_BACKING,
        );
    }
    for (rect, galley) in laid {
        p.galley(rect.min + shift, galley, AMBER);
    }
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

/// Lead, dash and gap of an aim line, in lane heights.
pub(super) const AIM_LEAD: f32 = 0.25;
pub(super) const AIM_DASH: f32 = 0.16;
pub(super) const AIM_GAP: f32 = 0.1;
/// Width of an aim line's dashes, in points.
const AIM_WIDTH: f32 = 1.5;

/// The dashes of a ranged windup's aim line (W2-28) from the spitter's feet
/// at `from` to its target at `to`: it starts [`AIM_LEAD`] lanes clear of
/// the body and ends on the target, the last dash cut short or stretched by
/// at most a gap to meet it, so no line stops a gap short. A target
/// closer than twice the lead (a spit from just past a wall's face) starts
/// the line halfway, so a close spit still draws its twin. Static (no march
/// or pulse), so reduced effects draw the same line.
pub(super) fn aim_dashes(from: Pos2, to: Pos2, sy: f32) -> Vec<[Pos2; 2]> {
    let length = from.distance(to);
    let direction = (to - from) / length.max(f32::EPSILON);
    let (dash, step) = (sy * AIM_DASH, sy * (AIM_DASH + AIM_GAP));
    let mut dashes = Vec::new();
    let mut at = (sy * AIM_LEAD).min(length * 0.5);
    while at < length && dash > 0.0 {
        let end = if at + step >= length {
            length
        } else {
            at + dash
        };
        dashes.push([from + direction * at, from + direction * end]);
        at += step;
    }
    dashes
}

/// Radii of a holding boss's phase ring (W2-37) when `hold_left_ticks` of
/// its hold remain: a slow breath in normal effects, fixed in reduced.
pub(super) fn phase_ring(sy: f32, hold_left_ticks: u64, reduced: bool) -> Vec2 {
    let breath = if reduced {
        0.0
    } else {
        let turn = (hold_left_ticks % PHASE_BREATH_TICKS) as f32 / PHASE_BREATH_TICKS as f32;
        PHASE_BREATH * (turn * std::f32::consts::TAU).sin()
    };
    vec2(sy * 0.55, sy * 0.2) * (1.0 + breath)
}
/// Ticks of one breath of the phase ring: two over the shipped 90-tick hold.
const PHASE_BREATH_TICKS: u64 = 45;
/// How far the phase ring breathes either way, as a fraction of its radii.
const PHASE_BREATH: f32 = 0.08;
/// Width of the phase ring, in points.
pub(super) const PHASE_RING_WIDTH: f32 = 2.5;
/// How far over the boss's name callout the phase label sits, in points.
pub(super) const PHASE_LABEL_GAP: f32 = 7.0;

/// Where a holding boss drawn at `at` with figure height `figure_height`
/// rings its feet (on its shadow), and where its phase label's bottom edge
/// sits (just over its name callout).
fn phase_anchors(at: Pos2, sy: f32, figure_height: f32) -> (Pos2, Pos2) {
    (
        at + vec2(0., figure_height * 0.48),
        name_anchor(at, sy) - vec2(0.0, PHASE_LABEL_GAP),
    )
}

/// The holding boss of `view` and its phase, when the boss is drawn.
fn holding_boss(view: &BattleViewModel) -> Option<(&EnemyView, &BossPhaseView)> {
    let phase = view.boss_phase.as_ref()?;
    let boss = view.enemies.iter().find(|e| e.id == phase.boss_id)?;
    Some((boss, phase))
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
/// `named` is false for a holding boss, whose name stacks with its phase
/// label after every body.
#[allow(clippy::too_many_arguments)]
fn enemy(
    p: &Painter,
    arena: ArenaProjection,
    e: &EnemyView,
    ring: bool,
    textures: ArtTextures,
    outlines: bool,
    reduced: bool,
    named: bool,
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
    if let Some(to) = e.aim_target {
        for dash in aim_dashes(at, arena.world(to[0], to[1]), sy) {
            p.line_segment(dash, Stroke::new(AIM_WIDTH, RED));
        }
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
            callout(
                p,
                at + vec2(0.0, -sy * 0.44),
                Align2::CENTER_BOTTOM,
                "CRIT · FIGHTING",
                8.0,
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
    if let Some(label) = e.label
        && named
    {
        callout(p, name_anchor(at, sy), Align2::CENTER_CENTER, label, 10.0);
    }
}
