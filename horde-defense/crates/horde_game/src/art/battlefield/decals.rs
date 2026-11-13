//! Blood decals (W5-06, design-vfx-lighting §6): a ground stain under each
//! of the newest [`BLOOD_DECALS_MAX`] bodies, only with the blood setting
//! on. A stain is 3 to 5 blobs whose shape is a hash of the body's id and
//! death tick (never an RNG draw), all in one untextured mesh under the
//! structures and the bodies. It spreads over its first [`SPREAD_TICKS`]
//! sim ticks (so it holds still while paused); reduced effects draw it
//! spread from the first frame.
use super::super::ArenaProjection;
use super::batch::MeshBuilder;
use crate::battle_view::crowd_stagger;
use bevy_egui::egui::{Color32 as C, Painter, Vec2, vec2};
use horde_core::sim::{Attempt, CorpseRecord};

/// The newest bodies that keep a stain.
pub(crate) const BLOOD_DECALS_MAX: usize = 64;
/// The most blobs of one stain; the least is [`BLOBS_MIN`].
pub(crate) const BLOBS_MAX: usize = 5;
pub(crate) const BLOBS_MIN: usize = 3;
/// Sim ticks a fresh stain takes to spread.
pub(crate) const SPREAD_TICKS: u64 = 18;
/// How much larger a boss's stain is.
const BOSS_SCALE: f32 = 1.6;
const SEGMENTS: u32 = 8;

/// The stain colour: (78,30,26) at alpha 150.
pub(crate) fn blood_color() -> C {
    C::from_rgba_unmultiplied(78, 30, 26, 150)
}

/// One blob of a stain, in lane heights around the stain's centre.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Blob {
    pub offset: [f32; 2],
    pub radii: [f32; 2],
}

/// The stain of one body.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Decal {
    /// The body's id.
    pub id: u64,
    /// The drawn feet of the body, in yards and lanes.
    pub at: [f32; 2],
    blobs: [Blob; BLOBS_MAX],
    count: usize,
    /// 0.4 (just fallen) to 1.0 (spread): scales every blob.
    pub spread: f32,
}
impl Decal {
    pub(crate) fn blobs(&self) -> &[Blob] {
        &self.blobs[..self.count]
    }
}

/// SplitMix64: a fixed scramble of the body's identity, not a random draw.
fn mix(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

fn decal(record: &CorpseRecord, tick: u64, reduced: bool) -> Decal {
    let seed = mix(record.id ^ record.tick.rotate_left(32));
    // The `n`th value in 0..1 of this body.
    let unit = |n: usize| (mix(seed.wrapping_add(n as u64)) >> 40) as f32 / (1u64 << 24) as f32;
    let count = BLOBS_MIN + (seed % (BLOBS_MAX - BLOBS_MIN + 1) as u64) as usize;
    let scale = if record.boss_kind.is_some() {
        BOSS_SCALE
    } else {
        1.0
    };
    let mut blobs = [Blob::default(); BLOBS_MAX];
    for (index, blob) in blobs.iter_mut().enumerate().take(count) {
        let n = index * 4;
        let radius = 0.07 + unit(n + 2) * 0.07;
        *blob = Blob {
            // The first blob sits under the body; the rest spill around it.
            offset: if index == 0 {
                [0.0; 2]
            } else {
                [
                    (unit(n) * 2.0 - 1.0) * 0.24 * scale,
                    (unit(n + 1) * 2.0 - 1.0) * 0.09 * scale,
                ]
            },
            // Flattened: a pool seen on the ground.
            radii: [radius * scale, radius * (0.4 + unit(n + 3) * 0.2) * scale],
        };
    }
    let [dx, dy] = crowd_stagger(record.id);
    let spread = if reduced {
        1.0
    } else {
        let t = (tick.saturating_sub(record.tick) as f32 / SPREAD_TICKS as f32).min(1.0);
        0.4 + 0.6 * t * (2.0 - t)
    };
    Decal {
        id: record.id,
        at: [record.at[0] + dx, record.at[1] + dy],
        blobs,
        count,
        spread,
    }
}

/// The stains of `a`, oldest first: none with `blood` off, else one for
/// each of its newest [`BLOOD_DECALS_MAX`] bodies.
pub(crate) fn decals(a: &Attempt, blood: bool, reduced: bool) -> Vec<Decal> {
    if !blood {
        return Vec::new();
    }
    let newest = a.corpses.len().saturating_sub(BLOOD_DECALS_MAX);
    a.corpses[newest..]
        .iter()
        .map(|record| decal(record, a.tick, reduced))
        .collect()
}

/// Paints the stains of `a` as one untextured mesh; nothing with `blood`
/// off or without bodies.
pub(super) fn paint(p: &Painter, arena: ArenaProjection, a: &Attempt, blood: bool, reduced: bool) {
    let decals = decals(a, blood, reduced);
    let lane = arena.lane_height();
    let color = blood_color();
    // A fan is a hub and its rim: 9 vertices, 8 triangles.
    let mut mesh = MeshBuilder::untextured(decals.len() * BLOBS_MAX * 4);
    for decal in &decals {
        let center = arena.world(decal.at[0], decal.at[1]) + vec2(0.0, lane * 0.25);
        let unit = lane * decal.spread;
        for blob in decal.blobs() {
            mesh.fan(
                center + Vec2::from(blob.offset) * unit,
                Vec2::from(blob.radii) * unit,
                SEGMENTS,
                color,
            );
        }
    }
    mesh.paint(p);
}

#[cfg(test)]
mod tests;
