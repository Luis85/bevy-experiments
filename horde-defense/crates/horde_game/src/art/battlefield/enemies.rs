//! Live hostiles in two passes. World pass ([`bodies`]): one shadow mesh
//! and one sprite mesh in the view's painter order (lane feet, x descending,
//! id; hostiles at their crowd-staggered spot) with each hit flash right
//! after its sprite, whatever their number ([`super::batch::FieldMeshes`]).
//! Rejected enemy art keeps the per-enemy procedural figure path.
//!
//! Cue pass ([`cues`], after the lighting grade): contact bands and gauges,
//! windup rings and bars, a ranged windup's dashed aim line, outlines, stun
//! rings, health bars, a holding boss's phase ring, then the callouts, each
//! on its INK backing ([`super::cue`]). An elite's body tint, ground ring
//! and shape marker are [`super::elite`] (W5-07).
use super::super::{AMBER, ArenaProjection, GREEN, RED, figure, soft_shadow};
use super::batch::FieldMeshes;
use super::cue::{self, CueBatch, Label};
use crate::{
    battle_view::{BattleViewModel, BossPhaseView, ContactView, EnemyView, HealthView},
    visual_assets::{self, Actor, ArtTextures, HitFlash},
};
use bevy_egui::egui::{Align2, Color32 as C, Painter, Pos2, Rect, Vec2, vec2};

/// The box a hostile's sprite is fitted into: `figure_height` tall, centred
/// on its drawn spot `at`.
fn body_size(figure_height: f32) -> Vec2 {
    vec2(figure_height * 1.2, figure_height)
}
/// The screen box a live hostile of `height` standing at `drawn` (yards,
/// lanes, crowd stagger included) is painted in. The damage popups of a
/// struck wall keep off the front body of its stack (CF-1-07).
pub(crate) fn body_box(
    arena: ArenaProjection,
    drawn: [f32; 2],
    height: crate::battle_view::FigureHeight,
) -> Rect {
    Rect::from_center_size(
        arena.world(drawn[0], drawn[1]),
        body_size(height.pixels(arena.lane_height())),
    )
}

/// The tint of a hostile's body: its archetype tint, leaned toward its
/// tell's colour by the view's glow when it is an elite (W5-07).
pub(super) fn body_tint(e: &EnemyView) -> C {
    let base = tint(e.tint);
    e.elite
        .as_ref()
        .map_or(base, |elite| super::elite::glow_tint(base, elite))
}

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

/// Paints every hostile's body and its cues at once (the passes
/// [`bodies`] and [`cues`] back to back, with no grade between them).
#[cfg(test)]
pub(super) fn paint(
    p: &Painter,
    arena: ArenaProjection,
    view: &BattleViewModel,
    textures: ArtTextures,
    outlines: bool,
    reduced: bool,
    rig_shadow: RigShadow,
) -> HostilesDrawn {
    let drawn = bodies(p, arena, view, textures, reduced, rig_shadow);
    let mut batch = CueBatch::new(view.enemies.len());
    cues(&mut batch, arena, view, outlines, reduced);
    batch.paint(p);
    drawn
}

/// World pass: every hostile's shadow, sprite and hit flash, and reports
/// what it drew ([`HostilesDrawn`]). Their gauges, rings and labels are
/// [`cues`], painted after the grade.
pub(super) fn bodies(
    p: &Painter,
    arena: ArenaProjection,
    view: &BattleViewModel,
    textures: ArtTextures,
    reduced: bool,
    rig_shadow: RigShadow,
) -> HostilesDrawn {
    let sy = arena.lane_height();
    if textures.enemy_art_rejected {
        for e in &view.enemies {
            enemy(p, arena, e, textures, reduced);
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
    layers
        .shadows
        .shadow(rig_shadow.at, rig_shadow.size, rig_shadow.alpha);
    for e in &view.enemies {
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
        let size = body_size(figure_height);
        layers.shadows.shadow(
            at + vec2(1., figure_height * 0.48),
            vec2(sy * 0.26 * (1. + pose.lunge * 0.12), sy * 0.095),
            0.7,
        );
        let quad = actor.and_then(|actor| {
            visual_assets::actor_quad(textures, actor, painted_at, size, body_tint(e), pose)
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
    }
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
    drawn
}

/// Cue pass: collects every hostile cue into `batch`, the same for batched
/// and rejected art. One contact band and gauge per lane, a holding boss's
/// phase ring (W2-37), one windup ground ring per stack, each ranged
/// windup's dashed aim line (W2-28), the outline, windup bar, stun ring and
/// health bar of each body, then its callouts; a holding boss's name and
/// "PHASE n - NAME" last, as one block.
pub(super) fn cues(
    batch: &mut CueBatch,
    arena: ArenaProjection,
    view: &BattleViewModel,
    outlines: bool,
    reduced: bool,
) {
    let sy = arena.lane_height();
    let font = cue::font(sy);
    let bar = cue::bar_height(sy);
    let holding = holding_boss(view);
    let stacked = holding.map(|(boss, _)| boss.id);
    for (lane, contact) in lane_contacts(view) {
        let (band, gauge) = contact_band(arena, view, lane, contact);
        batch.wash(band, RED.gamma_multiply(0.18));
        batch.line(gauge[0], gauge[1], 3.0, gauge_color(contact));
    }
    if let Some((boss, phase)) = holding {
        let [x, y] = boss.drawn();
        let (ring, _) = phase_anchors(arena.world(x, y), sy, boss.height.pixels(sy));
        batch.ellipse(
            ring,
            phase_ring(sy, phase.hold_left_ticks, reduced),
            PHASE_RING_WIDTH,
            AMBER,
        );
    }
    let tells = super::elite::drawn(view);
    for ((e, ring), tell) in view.enemies.iter().zip(windup_rings(view)).zip(tells) {
        let [x, y] = e.drawn();
        let at = arena.world(x, y);
        if tell {
            super::elite::ring(batch, e, at, sy);
        }
        if ring {
            batch.ellipse(at + vec2(0., sy * 0.3), vec2(sy * 0.36, sy * 0.13), 2., RED);
        }
        if let Some(to) = e.aim_target {
            for [a, b] in aim_dashes(at, arena.world(to[0], to[1]), sy) {
                batch.line(a, b, AIM_WIDTH, RED);
            }
        }
        if outlines {
            batch.ellipse(at, Vec2::splat(sy * 0.4), 1., AMBER);
        }
        if let Some(progress) = e.windup {
            batch.bar(
                Rect::from_center_size(at + vec2(0., sy * 0.43), vec2(sy * 0.66, bar)),
                progress,
                RED,
            );
        }
        if e.stunned {
            batch.ellipse(at + vec2(0., -sy * 0.3), Vec2::splat(sy * 0.16), 1.4, AMBER);
        }
        match e.health {
            HealthView::Bar { fraction, boss } => batch.bar(
                Rect::from_center_size(
                    at + vec2(0.0, -sy * HEALTH_BAR_LIFT),
                    vec2(if boss { sy * 1.1 } else { sy * 0.65 }, bar),
                ),
                fraction,
                if boss { AMBER } else { RED },
            ),
            HealthView::Critical => batch.label(Label::amber(
                at + vec2(0.0, -sy * CRIT_LIFT),
                Align2::CENTER_BOTTOM,
                CRIT_TEXT,
                font * CRIT_FONT,
            )),
            HealthView::Hidden => {}
        }
        if tell {
            super::elite::marker(batch, e, at, sy);
        }
        if let Some(label) = e.label
            && stacked != Some(e.id)
        {
            batch.label_block_over(
                vec![Label::amber(
                    name_anchor(at, sy),
                    Align2::CENTER_CENTER,
                    label,
                    font,
                )],
                name_floor(e, at, sy),
            );
        }
    }
    if let Some((boss, phase)) = holding {
        let [x, y] = boss.drawn();
        let at = arena.world(x, y);
        let (_, label_at) = phase_anchors(at, sy, boss.height.pixels(sy));
        let mut block = Vec::with_capacity(2);
        if let Some(name) = boss.label {
            block.push(Label::amber(
                name_anchor(at, sy),
                Align2::CENTER_CENTER,
                name,
                font,
            ));
        }
        block.push(Label::amber(
            label_at,
            Align2::CENTER_BOTTOM,
            phase.label(),
            font,
        ));
        batch.label_block_over(block, name_floor(boss, at, sy));
    }
}

/// The CRIT callout's size, as a fraction of the cue font.
const CRIT_FONT: f32 = 0.8;

/// How far over its feet, in lane heights, the bottom of a critically
/// wounded hostile's CRIT callout sits.
const CRIT_LIFT: f32 = 0.44;

/// The callout a critically wounded hostile shows in place of its bar.
const CRIT_TEXT: &str = "CRIT · FIGHTING";

/// The plate under the CRIT callout of a critically wounded hostile drawn
/// at `at` on a field of lane height `sy`, laid out with `p`'s fonts where
/// no clip moves it. The damage popups keep clear of it as of a health bar
/// (CF-3-12).
pub(crate) fn crit_plate(p: &Painter, at: Pos2, sy: f32) -> Rect {
    cue::plate(
        p,
        &Label::amber(
            at + vec2(0.0, -sy * CRIT_LIFT),
            Align2::CENTER_BOTTOM,
            CRIT_TEXT,
            cue::font(sy) * CRIT_FONT,
        ),
    )
}

/// How far over its feet, in lane heights, a hostile's name callout is
/// centred.
const NAME_LIFT: f32 = 0.7;

/// Where the name callout of a hostile drawn at `at` is centred, unless
/// [`name_floor`] lifts it.
fn name_anchor(at: Pos2, sy: f32) -> Pos2 {
    at + vec2(0.0, -sy * NAME_LIFT)
}

/// How far a hostile's name plate stays over the cue under it.
const NAME_GAP: f32 = 2.0;

/// How far over its feet, in points, the cue painted over the body of `e`
/// on a field of lane height `sy` reaches with its backing: the CRIT
/// callout's plate, or the health bar's halo (where a bar would be when
/// none is painted).
pub(super) fn over_body_cue_top(e: &EnemyView, sy: f32) -> f32 {
    match e.health {
        HealthView::Critical => {
            // The callout sits bottom-aligned at 0.44 lanes; its text is at
            // most 1.3 font sizes tall.
            sy * CRIT_LIFT + cue::font(sy) * CRIT_FONT * 1.3 + cue::CALLOUT_PAD
        }
        HealthView::Bar { .. } | HealthView::Hidden => {
            sy * HEALTH_BAR_LIFT + cue::bar_height(sy) * 0.5 + cue::HALO_BAR
        }
    }
}

/// The y the name block (a holding boss's name and phase) of `e` drawn at
/// `at` keeps its plates at or above: [`NAME_GAP`] over the bar or CRIT
/// callout under it. On a wide lane the name's centre at [`NAME_LIFT`]
/// clears it already; on the compact 17-lane field (lanes about 18 pt at
/// the 10 pt minimum font) the plate covered the boss bar (CF-3-11).
pub(super) fn name_floor(e: &EnemyView, at: Pos2, sy: f32) -> f32 {
    at.y - over_body_cue_top(e, sy) - NAME_GAP
}

#[cfg(test)]
pub(super) use super::cue::{CALLOUT_BACKING, CALLOUT_MARGIN, CALLOUT_PAD};

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

/// The per-enemy body path for rejected enemy art: a procedural figure with
/// its shadow and hit flash. Its cues are [`cues`], as for batched art.
fn enemy(p: &Painter, arena: ArenaProjection, e: &EnemyView, textures: ArtTextures, reduced: bool) {
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
            body_size(figure_height),
            body_tint(e),
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
            body_size(figure_height),
            HitFlash { strength, reduced },
            pose,
        );
    }
}
