//! Impact marks of the world pass (W5-06, design-vfx-lighting §6 "Hits"):
//! damage-scaled hit sparks and the kill burst, every spark of the frame in
//! one untextured mesh. A harder hit throws more and longer sparks; a boss
//! kill bursts 1.8 times as far. Nothing here moves the field: there is no
//! screen shake. Reduced effects draw none of it (the damage number and the
//! struck hostile's flash carry the hit).
use super::super::{AMBER, ArenaProjection, MUTED, TEXT};
use super::batch::MeshBuilder;
use super::fx::fx_lifetime;
use crate::battle_view::{BattleViewModel, FxView};
use bevy_egui::egui::{Color32 as C, Painter, vec2};
use horde_core::sim::EventKind;
use std::f32::consts::TAU;

/// The newest impact events (hits and kills) that draw sparks.
pub(crate) const HIT_EVENTS_MAX: usize = 64;
/// The most sparks one impact throws.
pub(crate) const SPARKS_PER_HIT_MAX: usize = 8;
/// Sparks of a kill burst.
pub(crate) const KILL_SPARKS: usize = 6;
/// A kill burst's spark length, in lane heights.
pub(crate) const KILL_SPARK_LANES: f32 = 0.16;
/// How much farther a boss kill bursts.
pub(crate) const BOSS_KILL_SCALE: f32 = 1.8;

/// A hit spark's length for `damage` on a lane `lane` points high: it grows
/// with the logarithm of the damage, between 6% and 25% of the lane.
pub(crate) fn spark_length(lane: f32, damage: i32) -> f32 {
    let damage = damage.max(0) as f32;
    lane * (0.05 + 0.04 * (1.0 + damage / 10.0).log2()).clamp(0.06, 0.25)
}

/// A spark's stroke width.
pub(crate) fn spark_width(lane: f32) -> f32 {
    (lane * 0.024).clamp(1.0, 3.0)
}

/// Sparks of one hit by damage tier: 4 below 20, 6 below 60, else 8.
pub(crate) fn spark_count(damage: i32) -> usize {
    match damage {
        ..20 => 4,
        20..60 => 6,
        _ => SPARKS_PER_HIT_MAX,
    }
}

/// The sparks one impact event throws.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Burst {
    pub count: usize,
    /// Length of each spark, in points.
    pub length: f32,
    pub width: f32,
    pub color: C,
}

/// The burst of `event` on a lane `lane` points high; `None` for an event
/// that is not an impact.
pub(crate) fn burst(event: &FxView, lane: f32) -> Option<Burst> {
    let width = spark_width(lane);
    match event.kind {
        EventKind::Kill => Some(Burst {
            count: KILL_SPARKS,
            length: lane * KILL_SPARK_LANES * if event.boss { BOSS_KILL_SCALE } else { 1.0 },
            width,
            color: TEXT,
        }),
        EventKind::Hit | EventKind::WeakPoint | EventKind::Armor | EventKind::Melee => {
            Some(Burst {
                count: spark_count(event.value),
                length: spark_length(lane, event.value),
                width,
                color: match event.kind {
                    EventKind::Armor => MUTED,
                    EventKind::WeakPoint => AMBER,
                    _ => TEXT,
                },
            })
        }
        _ => None,
    }
}

fn live_impact(event: &&FxView) -> bool {
    matches!(
        event.kind,
        EventKind::Hit
            | EventKind::WeakPoint
            | EventKind::Armor
            | EventKind::Melee
            | EventKind::Kill
    ) && event.age < fx_lifetime(event.kind)
}

/// The impact events of `view` that draw sparks: its newest
/// [`HIT_EVENTS_MAX`] live ones, oldest first.
pub(crate) fn drawn(view: &BattleViewModel) -> impl Iterator<Item = &FxView> {
    let count = view.fx.iter().filter(live_impact).count();
    view.fx
        .iter()
        .filter(live_impact)
        .skip(count.saturating_sub(HIT_EVENTS_MAX))
}

/// Paints every spark of `view` as one mesh: each impact's sparks leave its
/// point evenly around it, drift outward and fade over the event's lifetime.
pub(super) fn paint(p: &Painter, arena: ArenaProjection, view: &BattleViewModel, reduced: bool) {
    if reduced {
        return;
    }
    let lane = arena.lane_height();
    let mut mesh = MeshBuilder::untextured(HIT_EVENTS_MAX * SPARKS_PER_HIT_MAX)
        .pixel_floor(p.pixels_per_point());
    for event in drawn(view) {
        let Some(burst) = burst(event, lane) else {
            continue;
        };
        let progress = (event.age / fx_lifetime(event.kind)).clamp(0.0, 1.0);
        let color = burst.color.gamma_multiply(1.0 - progress);
        let at = arena.world(event.to[0], event.to[1]);
        let inner = lane * 0.06 * (1.0 + progress);
        for n in 0..burst.count {
            let angle = (n as f32 + 0.5) / burst.count as f32 * TAU;
            let direction = vec2(angle.cos(), angle.sin());
            mesh.line(
                at + direction * inner,
                at + direction * (inner + burst.length),
                burst.width,
                color,
            );
        }
    }
    mesh.paint(p);
}

#[cfg(test)]
mod tests;
