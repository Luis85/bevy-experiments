//! Bursts (W5-05, design-vfx-lighting section 6): a grenade and a Molotov
//! each play six procedural frames over the Explosion event's half second,
//! built from vector parts and the rig sheet's explosion (cell 12) and flame
//! (cell 13) icons, and leave a scorch mark that fades over three seconds.
//!
//! The two read apart without colour: a grenade is a white core, a shock
//! ring, debris and grey smoke; a Molotov is glass glints, an orange bloom,
//! flame tongues and dark smoke, with no ring. Reduced effects draw one
//! static ring instead: solid for a grenade, dashed for a Molotov.
//!
//! Presentation only: built from the frame's `BattleViewModel`; nothing here
//! reads the core RNG or changes the attempt.
use super::super::{ArenaProjection, box_at, line};
use super::fx;
use crate::{
    battle_view::{BattleViewModel, SCORCH_MAX, SCORCH_SECONDS, ScorchView},
    visual_assets::{self, ArtTextures},
};
use bevy_egui::egui::{Color32 as C, Painter, Pos2, Shape, Stroke, Vec2, vec2};
use horde_core::{ordnance::OrdnanceKind, sim::EventKind};
use std::f32::consts::TAU;

/// Frames one burst plays over its life.
pub(crate) const EXPLOSION_FRAMES: usize = 6;
/// The most bursts one frame draws: the newest ones.
pub(crate) const EXPLOSIONS_MAX: usize = 8;
/// A rig-sheet icon is drawn at most this many screen pixels per texel; the
/// vector parts carry the blast's size.
pub(crate) const SPRITE_MAX_MAGNIFICATION: f32 = 1.5;
/// Dashes of the reduced Molotov ring.
pub(crate) const REDUCED_DASHES: usize = 12;

const DEBRIS: usize = 8;
const PUFFS: usize = 5;
const TONGUES: usize = 5;
const GLINTS: usize = 6;
const DARK_SMOKE: usize = 3;
/// The Molotov's tint on the explosion icon.
const MOLOTOV_FIREBALL_RGB: [u8; 3] = [255, 170, 90];

/// What one drawn part of a burst is: the shape vocabulary the two ordnance
/// kinds are told apart by.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Part {
    /// Grenade: the white flash at the centre.
    Core,
    /// Grenade: the expanding ring; it carries the blast radius.
    ShockRing,
    /// The explosion icon (cell 12).
    Fireball,
    /// Grenade: thrown fragments.
    Debris,
    /// Grenade: grey dust puffs.
    Puff,
    /// Rising smoke: grey rounds for a grenade, dark flat banks for a Molotov.
    Smoke,
    /// Molotov: shards of glass.
    Glint,
    /// Molotov: the orange ground glow.
    Bloom,
    /// Molotov: a flame icon (cell 13).
    Tongue,
    /// Reduced grenade: one solid static ring.
    SolidRing,
    /// Reduced Molotov: one dash of the static dashed ring.
    Dash,
    /// The ground mark left behind.
    Scorch,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Geom {
    Disc {
        at: Pos2,
        radii: Vec2,
    },
    Ring {
        at: Pos2,
        radii: Vec2,
        width: f32,
    },
    Line {
        a: Pos2,
        b: Pos2,
        width: f32,
    },
    /// A rig-sheet icon: the flame (cell 13) or the explosion (cell 12).
    Sprite {
        flame: bool,
        at: Pos2,
        size: Vec2,
    },
}

/// One drawn part: its kind, geometry and colour (a sprite's tint).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Prim {
    pub part: Part,
    pub geom: Geom,
    pub color: C,
}

/// One burst to draw.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Blast {
    pub at: Pos2,
    /// The blast radii in points (yards across, lanes down).
    pub range: Vec2,
    /// One lane's height in points.
    pub lane: f32,
    /// Seconds since the Explosion event.
    pub age: f32,
    pub ordnance: OrdnanceKind,
    /// Turns the debris, puffs and tongues, so bursts do not look alike.
    pub phase: f32,
}

/// The frame a burst of `age` seconds shows, in `0..EXPLOSION_FRAMES`, and
/// how far through that frame it is, in 0..1.
pub(crate) fn frame(age: f32) -> (usize, f32) {
    let at =
        (age / fx::fx_lifetime(EventKind::Explosion)).clamp(0.0, 1.0) * EXPLOSION_FRAMES as f32;
    let index = (at as usize).min(EXPLOSION_FRAMES - 1);
    (index, (at - index as f32).clamp(0.0, 1.0))
}

fn rgba([r, g, b]: [u8; 3], alpha: f32) -> C {
    C::from_rgba_unmultiplied(r, g, b, alpha.clamp(0.0, 255.0) as u8)
}

/// The `n`th of `count` points on the ellipse `range * spread` around `at`.
fn around(blast: &Blast, n: usize, count: usize, spread: f32) -> Pos2 {
    let angle = blast.phase + n as f32 / count as f32 * TAU;
    blast.at + vec2(angle.cos() * blast.range.x, angle.sin() * blast.range.y) * spread
}

/// The parts of `blast`, in paint order. Reduced effects have one static
/// ring: its parts do not depend on the age.
pub(crate) fn prims(blast: &Blast, reduced: bool) -> Vec<Prim> {
    let mut out = Vec::new();
    if reduced {
        reduced_ring(blast, &mut out);
        return out;
    }
    let (index, t) = frame(blast.age);
    match blast.ordnance {
        OrdnanceKind::Grenade => grenade(blast, index, t, &mut out),
        OrdnanceKind::Molotov => molotov(blast, index, t, &mut out),
    }
    out
}

/// Reduced effects: a solid ring for a grenade, a dashed one for a Molotov,
/// at the blast radius, the same for the whole life.
fn reduced_ring(blast: &Blast, out: &mut Vec<Prim>) {
    let color = rgba([239, 181, 82], 150.0);
    match blast.ordnance {
        OrdnanceKind::Grenade => out.push(Prim {
            part: Part::SolidRing,
            geom: Geom::Ring {
                at: blast.at,
                radii: blast.range,
                width: 1.5,
            },
            color,
        }),
        OrdnanceKind::Molotov => {
            let point = |angle: f32| {
                blast.at + vec2(angle.cos() * blast.range.x, angle.sin() * blast.range.y)
            };
            let step = TAU / REDUCED_DASHES as f32;
            out.extend((0..REDUCED_DASHES).map(|n| Prim {
                part: Part::Dash,
                geom: Geom::Line {
                    a: point(n as f32 * step),
                    b: point((n as f32 + 0.5) * step),
                    width: 1.5,
                },
                color,
            }));
        }
    }
}

fn disc(part: Part, at: Pos2, radii: Vec2, color: C) -> Prim {
    Prim {
        part,
        geom: Geom::Disc { at, radii },
        color,
    }
}

/// White core, shock ring, fireball with debris, fireball with dust puffs,
/// then rising smoke (frames 4 and 5).
fn grenade(blast: &Blast, index: usize, t: f32, out: &mut Vec<Prim>) {
    let Blast {
        at, range, lane, ..
    } = *blast;
    let ring = |radii: Vec2, width: f32, rgb: [u8; 3], alpha: f32| Prim {
        part: Part::ShockRing,
        geom: Geom::Ring { at, radii, width },
        color: rgba(rgb, alpha),
    };
    let fireball = |scale: f32, alpha: f32| Prim {
        part: Part::Fireball,
        geom: Geom::Sprite {
            flame: false,
            at,
            size: range * (1.2 * scale),
        },
        color: C::from_white_alpha(alpha as u8),
    };
    let smoke = |rise: f32, radius: f32, alpha: f32, out: &mut Vec<Prim>| {
        out.extend((0..PUFFS).map(|n| {
            disc(
                Part::Smoke,
                around(blast, n, PUFFS, 0.5) - vec2(0.0, lane * rise),
                Vec2::splat(lane * radius),
                rgba([96, 94, 90], alpha),
            )
        }));
    };
    match index {
        0 => out.push(disc(
            Part::Core,
            at,
            range * (0.18 + 0.12 * t),
            rgba([255, 250, 232], 240.0),
        )),
        1 => {
            out.push(disc(
                Part::Core,
                at,
                range * (0.30 - 0.12 * t),
                rgba([255, 224, 150], 200.0 * (1.0 - t)),
            ));
            out.push(ring(range * (0.35 + 0.65 * t), 2.5, [255, 236, 190], 220.0));
        }
        2 => {
            out.push(ring(range, 2.0, [239, 181, 82], 170.0));
            out.push(fireball(1.0, 255.0));
            out.extend((0..DEBRIS).map(|n| Prim {
                part: Part::Debris,
                geom: Geom::Line {
                    a: around(blast, n, DEBRIS, 0.35 + 0.25 * t),
                    b: around(blast, n, DEBRIS, 0.6 + 0.35 * t),
                    width: 1.5,
                },
                color: rgba([136, 125, 91], 220.0),
            }));
        }
        3 => {
            out.push(ring(range, 1.5, [239, 181, 82], 110.0));
            out.push(fireball(1.3, 178.0));
            out.extend((0..PUFFS).map(|n| {
                disc(
                    Part::Puff,
                    around(blast, n, PUFFS, 0.5),
                    Vec2::splat(lane * (0.14 + 0.05 * t)),
                    rgba([150, 146, 138], 150.0),
                )
            }));
        }
        4 => {
            out.push(ring(range, 1.0, [239, 181, 82], 60.0 * (1.0 - t)));
            smoke(0.10 * t, 0.2, 120.0, out);
        }
        _ => smoke(0.10 + 0.12 * t, 0.2 + 0.05 * t, 120.0 * (1.0 - t), out),
    }
}

/// Glass glints and an orange bloom (never a ring), flame tongues, the
/// tinted fireball with tongues, the tongues settling, then dark smoke.
fn molotov(blast: &Blast, index: usize, t: f32, out: &mut Vec<Prim>) {
    let Blast {
        at, range, lane, ..
    } = *blast;
    let bloom = |spread: f32, rgb: [u8; 3], alpha: f32| {
        disc(Part::Bloom, at, range * spread, rgba(rgb, alpha))
    };
    let tongues = |scale: f32, alpha: f32, out: &mut Vec<Prim>| {
        let size = vec2(lane * 0.22, lane * 0.34) * scale;
        out.extend((0..TONGUES).map(|n| Prim {
            part: Part::Tongue,
            geom: Geom::Sprite {
                flame: true,
                at: around(blast, n, TONGUES, 0.45) - vec2(0.0, size.y * 0.4),
                size,
            },
            color: C::from_white_alpha(alpha as u8),
        }));
    };
    let smoke = |rise: f32, alpha: f32, out: &mut Vec<Prim>| {
        out.extend((0..DARK_SMOKE).map(|n| {
            disc(
                Part::Smoke,
                around(blast, n, DARK_SMOKE, 0.4) - vec2(0.0, lane * rise),
                vec2(lane * 0.26, lane * 0.12),
                rgba([46, 40, 36], alpha),
            )
        }));
    };
    match index {
        0 => {
            out.push(bloom(0.2 + 0.2 * t, [255, 150, 60], 210.0));
            out.extend((0..GLINTS).map(|n| {
                let from = around(blast, n, GLINTS, 0.15 + 0.3 * t);
                let along = (from - at).normalized();
                Prim {
                    part: Part::Glint,
                    geom: Geom::Line {
                        a: from,
                        b: from + along * (lane * 0.08),
                        width: 1.0,
                    },
                    color: rgba([214, 236, 220], 230.0),
                }
            }));
        }
        1 => {
            out.push(bloom(0.4 + 0.3 * t, [255, 150, 60], 150.0 * (1.0 - t)));
            tongues(0.6 + 0.4 * t, 240.0, out);
        }
        2 => {
            out.push(Prim {
                part: Part::Fireball,
                geom: Geom::Sprite {
                    flame: false,
                    at,
                    size: range * 1.2,
                },
                color: rgba(MOLOTOV_FIREBALL_RGB, 235.0),
            });
            tongues(1.0, 240.0, out);
        }
        3 => tongues(1.0 - 0.3 * t, 200.0, out),
        4 => {
            tongues(0.7 - 0.3 * t, 200.0 * (1.0 - t), out);
            smoke(0.08 * t, 130.0, out);
        }
        _ => {
            out.push(bloom(0.5, [160, 70, 30], 70.0 * (1.0 - t)));
            smoke(0.08 + 0.10 * t, 130.0 * (1.0 - t), out);
        }
    }
}

/// The scorch mark's part: a dark ground disc fading over
/// [`SCORCH_SECONDS`]; in reduced effects it holds one alpha and then goes.
pub(crate) fn scorch_prim(arena: ArenaProjection, scorch: &ScorchView, reduced: bool) -> Prim {
    let unit = vec2(arena.yard_width(), arena.lane_height());
    let fade = (1.0 - scorch.age / SCORCH_SECONDS).clamp(0.0, 1.0);
    Prim {
        part: Part::Scorch,
        geom: Geom::Disc {
            at: arena.world(scorch.center[0], scorch.center[1]),
            radii: unit * (scorch.radius * 0.7),
        },
        color: rgba([28, 24, 20], if reduced { 70.0 } else { 110.0 * fade }),
    }
}

/// `size` shrunk (never grown) so an icon of `texels` source pixels covers
/// at most [`SPRITE_MAX_MAGNIFICATION`] screen pixels per texel.
pub(crate) fn clamp_sprite(size: Vec2, texels: Vec2, pixels_per_point: f32) -> Vec2 {
    let cap = texels * (SPRITE_MAX_MAGNIFICATION / pixels_per_point.max(0.01));
    let scale = (cap.x / size.x.max(0.01))
        .min(cap.y / size.y.max(0.01))
        .min(1.0);
    size * scale
}

/// Which pass a part is painted in: ground parts under the icons, thrown
/// and rising parts over them. Three passes for every burst together, so
/// the bursts cost at most three primitives however many there are.
fn over(part: Part) -> bool {
    matches!(part, Part::Debris | Part::Puff | Part::Smoke | Part::Glint)
}

fn paint_vector(p: &Painter, prim: &Prim) {
    match prim.geom {
        Geom::Disc { at, radii } => {
            p.add(Shape::ellipse_filled(at, radii, prim.color));
        }
        Geom::Ring { at, radii, width } => {
            p.add(Shape::ellipse_stroke(
                at,
                radii,
                Stroke::new(width, prim.color),
            ));
        }
        Geom::Line { a, b, width } => line(p, a, b, width, prim.color),
        Geom::Sprite { .. } => {}
    }
}

fn paint_sprite(p: &Painter, textures: ArtTextures, prim: &Prim) {
    let Geom::Sprite { flame, at, size } = prim.geom else {
        return;
    };
    if let Some(texels) = visual_assets::ordnance_effect_texels(textures, flame) {
        let size = clamp_sprite(size, texels, p.pixels_per_point());
        visual_assets::ordnance_effect(p, textures, flame, at, size, prim.color);
        return;
    }
    // No rig sheet (a failed load, the headless gates): a plain stand-in.
    let alpha = f32::from(prim.color.a());
    if flame {
        box_at(p, at, size * vec2(0.5, 1.0), rgba([230, 132, 48], alpha));
    } else {
        p.add(Shape::ellipse_filled(
            at,
            size * 0.5,
            rgba([240, 150, 60], alpha),
        ));
    }
}

/// The bursts `view` shows: its newest [`EXPLOSIONS_MAX`] live Explosion
/// events, oldest first.
pub(crate) fn blasts(arena: ArenaProjection, view: &BattleViewModel) -> Vec<Blast> {
    let unit = vec2(arena.yard_width(), arena.lane_height());
    let mut out: Vec<_> = view
        .fx
        .iter()
        .rev()
        .filter(|e| e.kind == EventKind::Explosion && e.age < fx::fx_lifetime(EventKind::Explosion))
        .take(EXPLOSIONS_MAX)
        .map(|e| Blast {
            at: arena.world(e.to[0], e.to[1]),
            range: unit * (e.value as f32 / 100.).max(0.1),
            lane: unit.y,
            age: e.age,
            ordnance: e.ordnance.unwrap_or(OrdnanceKind::Grenade),
            phase: (e.actor_id.unwrap_or(e.seq) % 19) as f32 * 0.3,
        })
        .collect();
    out.reverse();
    out
}

/// The parts of every burst `view` shows, in burst order: what [`paint`]
/// draws.
pub(crate) fn drawn(arena: ArenaProjection, view: &BattleViewModel, reduced: bool) -> Vec<Prim> {
    blasts(arena, view)
        .iter()
        .flat_map(|blast| prims(blast, reduced))
        .collect()
}

/// World pass, above the actors: every live burst.
pub(super) fn paint(
    p: &Painter,
    arena: ArenaProjection,
    view: &BattleViewModel,
    reduced: bool,
    textures: ArtTextures,
) {
    let prims = drawn(arena, view, reduced);
    let sprite = |prim: &&Prim| matches!(prim.geom, Geom::Sprite { .. });
    for prim in prims
        .iter()
        .filter(|prim| !sprite(prim) && !over(prim.part))
    {
        paint_vector(p, prim);
    }
    for prim in prims.iter().filter(sprite) {
        paint_sprite(p, textures, prim);
    }
    for prim in prims.iter().filter(|prim| !sprite(prim) && over(prim.part)) {
        paint_vector(p, prim);
    }
}

/// World pass, on the ground under every body: the scorch marks.
pub(super) fn scorch(p: &Painter, arena: ArenaProjection, view: &BattleViewModel, reduced: bool) {
    // The view already keeps the newest marks; a hand-built one is capped too.
    let newest = view.scorch.len().saturating_sub(SCORCH_MAX);
    for mark in &view.scorch[newest..] {
        paint_vector(p, &scorch_prim(arena, mark, reduced));
    }
}

#[cfg(test)]
mod tests;
