//! Elite and affix tells (design-vfx-lighting §7, W5-07). An elite reads in
//! two passes. World pass: its body tint leans toward its tell's colour by
//! the view's glow ([`glow_tint`]), which pulses on sim ticks and holds
//! still in reduced effects. Cue pass: a ground ring in the tell's colour
//! and width round its feet and the tell's shape marker over its health
//! bar ([`cues`]), each on its INK backing, so no grade, light or storm
//! flash dims them. The shape carries the affix (colour-blind safe); the
//! colour is secondary. The ring and the marker never move or pulse, so
//! reduced effects draw the same cues.
use super::cue::{self, CueBatch};
use crate::{
    battle_view::{BattleViewModel, EliteView, EnemyView},
    visual_assets::enemy_visuals::TellMarker,
};
use bevy_egui::egui::{Color32 as C, Pos2, pos2, vec2};

/// The most elites that draw a ring and a marker in one frame (design §13
/// `elite_tells`): the nearest to the defence line. Every elite keeps its
/// body tint.
pub(super) const ELITE_TELLS_MAX: usize = 48;
/// Sides of an elite's ground ring.
pub(super) const RING_SEGMENTS: u32 = 12;
/// Radii of the ground ring, in lane heights: just outside a windup ring
/// (0.36 by 0.13), which shares its centre.
pub(super) const RING_RADII: [f32; 2] = [0.44, 0.16];
/// How far under the drawn feet anchor the ring is centred, in lane heights
/// (the windup ring's centre).
pub(super) const RING_DROP: f32 = 0.3;
/// Clear space between the health bar's (or the CRIT callout's) backing and
/// the marker's, in points.
const MARKER_GAP: f32 = 1.0;

/// Half the marker's size on a field of lane height `sy`: 4 to 7 pt.
pub(super) fn marker_half(sy: f32) -> f32 {
    (0.085 * sy).clamp(4.0, 7.0)
}

/// The colour of a tell's ring and marker.
pub(super) fn tell_color(elite: &EliteView) -> C {
    let [r, g, b] = elite.tell.outline_rgb;
    C::from_rgb(r, g, b)
}

/// World pass: `base` leaned toward the tell's colour by the view's glow.
pub(super) fn glow_tint(base: C, elite: &EliteView) -> C {
    let glow = elite.glow.clamp(0.0, 1.0);
    let [r, g, b] = elite.tell.outline_rgb;
    // `base` is premultiplied, so the target is too.
    let alpha = f32::from(base.a()) / 255.0;
    let lean = |from: u8, to: u8| {
        let from = f32::from(from);
        (from + (f32::from(to) * alpha - from) * glow).round() as u8
    };
    C::from_rgba_premultiplied(
        lean(base.r(), r),
        lean(base.g(), g),
        lean(base.b(), b),
        base.a(),
    )
}

/// Which hostiles of `view.enemies` draw their tell's ring and marker: every
/// elite, or the [`ELITE_TELLS_MAX`] nearest the defence line (lowest x,
/// then lowest id) when there are more.
pub(super) fn drawn(view: &BattleViewModel) -> Vec<bool> {
    let mut shown: Vec<bool> = view.enemies.iter().map(|e| e.elite.is_some()).collect();
    let mut elites: Vec<usize> = (0..shown.len()).filter(|&index| shown[index]).collect();
    if elites.len() > ELITE_TELLS_MAX {
        elites.sort_by(|&a, &b| {
            let (a, b) = (&view.enemies[a], &view.enemies[b]);
            a.position[0]
                .total_cmp(&b.position[0])
                .then(a.id.cmp(&b.id))
        });
        for &index in &elites[ELITE_TELLS_MAX..] {
            shown[index] = false;
        }
    }
    shown
}

/// Cue pass: the ground ring of the elite `e` drawn at `at`. Collected
/// before the body's other cues, so a windup ring paints over it.
pub(super) fn ring(batch: &mut CueBatch, e: &EnemyView, at: Pos2, sy: f32) {
    let Some(elite) = &e.elite else { return };
    batch.ellipse_of(
        at + vec2(0.0, sy * RING_DROP),
        vec2(sy * RING_RADII[0], sy * RING_RADII[1]),
        f32::from(elite.tell.ring_px),
        RING_SEGMENTS,
        tell_color(elite),
    );
}

/// Cue pass: the shape marker of the elite `e` drawn at `at`, over its
/// health bar (over its CRIT callout when that replaces the bar).
pub(super) fn marker(batch: &mut CueBatch, e: &EnemyView, at: Pos2, sy: f32) {
    let Some(elite) = &e.elite else { return };
    let color = tell_color(elite);
    for triangle in marker_triangles(elite.tell.marker, marker_centre(e, at, sy), marker_half(sy)) {
        batch.marker(triangle, color);
    }
}

/// Where the marker of `e` is centred: its halo clears the health bar's
/// halo, or the CRIT callout's plate, by [`MARKER_GAP`].
pub(super) fn marker_centre(e: &EnemyView, at: Pos2, sy: f32) -> Pos2 {
    let half = marker_half(sy);
    let top = super::enemies::over_body_cue_top(e, sy);
    // A marker triangle's backing reaches twice the halo past its corners.
    at - vec2(0.0, top + MARKER_GAP + cue::HALO_MARKER * 2.0 + half)
}

/// The triangles of `marker` centred on `c` with half-size `s`. Every shape
/// differs from every other by its outline, not its colour: a chevron (a V
/// of two arms), a crown (three spikes on one base line), one, two or three
/// pips (diamonds in a row), a shield (a slab over a point) and a spark (a
/// four-point star).
pub(super) fn marker_triangles(marker: TellMarker, c: Pos2, s: f32) -> Vec<[Pos2; 3]> {
    let at = |x: f32, y: f32| pos2(c.x + x * s, c.y + y * s);
    let pips = |count: u8| {
        let radius = 0.34;
        let step = 0.66;
        let first = -step * f32::from(count - 1) * 0.5;
        (0..count)
            .flat_map(|n| {
                let x = first + step * f32::from(n);
                [
                    [
                        at(x - radius, 0.0),
                        at(x, -radius * 1.6),
                        at(x + radius, 0.0),
                    ],
                    [
                        at(x - radius, 0.0),
                        at(x + radius, 0.0),
                        at(x, radius * 1.6),
                    ],
                ]
            })
            .collect::<Vec<_>>()
    };
    match marker {
        TellMarker::Chevron => vec![
            [at(-1.0, -0.6), at(0.0, 0.7), at(0.0, -0.05)],
            [at(1.0, -0.6), at(0.0, -0.05), at(0.0, 0.7)],
        ],
        TellMarker::Crown => vec![
            [at(-1.0, 0.6), at(-1.0, -0.7), at(-0.3, 0.6)],
            [at(-0.5, 0.6), at(0.0, -0.8), at(0.5, 0.6)],
            [at(0.3, 0.6), at(1.0, -0.7), at(1.0, 0.6)],
        ],
        TellMarker::Pips1 => pips(1),
        TellMarker::Pips2 => pips(2),
        TellMarker::Pips3 => pips(3),
        TellMarker::Shield => vec![
            [at(-0.8, -0.8), at(0.8, -0.8), at(0.8, -0.1)],
            [at(-0.8, -0.8), at(0.8, -0.1), at(-0.8, -0.1)],
            [at(-0.8, -0.1), at(0.8, -0.1), at(0.0, 0.9)],
        ],
        TellMarker::Spark => vec![
            [at(-0.26, 0.0), at(0.0, -1.0), at(0.26, 0.0)],
            [at(-0.26, 0.0), at(0.26, 0.0), at(0.0, 1.0)],
            [at(0.0, -0.26), at(1.0, 0.0), at(0.0, 0.26)],
            [at(0.0, -0.26), at(0.0, 0.26), at(-1.0, 0.0)],
        ],
    }
}

/// The bounds of a marker's triangles.
#[cfg(test)]
pub(super) fn marker_bounds(triangles: &[[Pos2; 3]]) -> bevy_egui::egui::Rect {
    use bevy_egui::egui::Rect;
    triangles
        .iter()
        .flatten()
        .fold(Rect::NOTHING, |bounds, point| {
            bounds.union(Rect::from_min_max(*point, *point))
        })
}

#[cfg(test)]
mod tests;
