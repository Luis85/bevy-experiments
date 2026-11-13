//! Additive light pools (W5-04, design-vfx-lighting section 6): the glow an
//! explosion, burning ground, a muzzle flash and a turret throw on the
//! ground. Painted as one mesh after the lighting grade and under the
//! vignette and every cue, so the pools read strongest on the dark grades
//! (`Grade::light_gain`) and never cover a warning.
//!
//! Presentation only: built from the frame's `BattleViewModel`, nothing here
//! reads the core RNG or changes the attempt. Reduced effects draw none.
use super::super::ArenaProjection;
use super::{fx, weapon_fx};
use crate::{
    battle_view::{BattleViewModel, FxView},
    visual_assets::ArtTextures,
};
use bevy_egui::egui::{Color32 as C, Mesh, Painter, Pos2, Shape, Vec2, vec2};
use horde_core::sim::EventKind;
use std::f32::consts::TAU;

/// The most light pools one frame draws (acceptance G7: light pools <= 24).
pub(crate) const LIGHTS_MAX: usize = 24;

/// Rim vertices of one pool's fan.
pub(crate) const LIGHT_SEGMENTS: u32 = 16;

/// How long an explosion lights the ground, in seconds.
pub(crate) const EXPLOSION_LIGHT_SECONDS: f32 = 0.35;
/// An explosion's pool is this many blast radii wide, at this intensity.
const EXPLOSION_LIGHT_RADIUS: f32 = 1.2;
const EXPLOSION_LIGHT_INTENSITY: f32 = 0.8;
pub(crate) const EXPLOSION_LIGHT_RGB: [u8; 3] = [255, 190, 110];

/// Burning ground: a pool as wide as the zone, flickering by 0.05 around
/// 0.35 every 6 ticks.
const FIRE_LIGHT_INTENSITY: f32 = 0.35;
const FIRE_LIGHT_FLICKER: f32 = 0.05;
const FIRE_LIGHT_FLICKER_TICKS: u64 = 6;
pub(crate) const FIRE_LIGHT_RGB: [u8; 3] = [255, 150, 70];
/// A zone's light fades out over its last ticks, with its ground and flames.
const FIRE_FADE_TICKS: f32 = 45.0;

/// A muzzle pool outlives its flash by this many ticks, fading out.
pub(crate) const MUZZLE_LIGHT_TAIL_TICKS: u32 = 4;

/// One additive pool: centre and radii in points, the colour at full
/// strength and its strength in 0..=1 before the grade's light gain.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LightPool {
    pub at: Pos2,
    pub radius: Vec2,
    pub rgb: [u8; 3],
    pub intensity: f32,
}

/// The frame's light pools, at most [`LIGHTS_MAX`], by priority: explosions,
/// then burning ground, then the survivor's muzzle, then turrets; newest
/// first inside each class, so a crowded frame keeps its most important and
/// most recent lights. Reduced effects have none.
///
/// `muzzle` is the drawn survivor's muzzle socket, when the rig resolved.
pub(super) fn collect(
    arena: ArenaProjection,
    view: &BattleViewModel,
    muzzle: Option<Pos2>,
    textures: ArtTextures,
    reduced: bool,
) -> Vec<LightPool> {
    let mut pools = Vec::new();
    if reduced {
        return pools;
    }
    let unit = vec2(arena.yard_width(), arena.lane_height());
    // `view.fx` is oldest first.
    let newest_first = || view.fx.iter().rev();
    pools.extend(newest_first().filter_map(|e| explosion(arena, unit, e)));
    let mut zones: Vec<_> = view.fire_zones.iter().collect();
    zones.sort_by_key(|zone| std::cmp::Reverse(zone.id));
    let flicker = if (view.tick / FIRE_LIGHT_FLICKER_TICKS).is_multiple_of(2) {
        FIRE_LIGHT_FLICKER
    } else {
        -FIRE_LIGHT_FLICKER
    };
    pools.extend(zones.into_iter().map(|zone| LightPool {
        at: arena.world(zone.center[0], zone.center[1]),
        radius: unit * zone.radius,
        rgb: FIRE_LIGHT_RGB,
        intensity: (FIRE_LIGHT_INTENSITY + flicker)
            * (zone.ticks_left as f32 / FIRE_FADE_TICKS).min(1.0),
    }));
    for turret in [false, true] {
        pools.extend(
            newest_first()
                .filter(|e| (e.source == Some(weapon_fx::FxSource::Turret)) == turret)
                .filter_map(|e| muzzle_light(arena, view, e, muzzle, textures)),
        );
    }
    pools.retain(|pool| pool.intensity > 0.0);
    pools.truncate(LIGHTS_MAX);
    pools
}

/// An explosion's pool: 1.2 blast radii, fading from 0.8 over its life.
fn explosion(arena: ArenaProjection, unit: Vec2, e: &FxView) -> Option<LightPool> {
    if e.kind != EventKind::Explosion || e.age >= EXPLOSION_LIGHT_SECONDS {
        return None;
    }
    let blast = (e.value as f32 / 100.0).max(0.1);
    Some(LightPool {
        at: arena.world(e.to[0], e.to[1]),
        radius: unit * (blast * EXPLOSION_LIGHT_RADIUS),
        rgb: EXPLOSION_LIGHT_RGB,
        intensity: EXPLOSION_LIGHT_INTENSITY * (1.0 - e.age / EXPLOSION_LIGHT_SECONDS),
    })
}

/// The pool of a volley's first Shot event, at the drawn muzzle, from the
/// source's weapon profile: full strength while the flash shows, then
/// fading over [`MUZZLE_LIGHT_TAIL_TICKS`].
fn muzzle_light(
    arena: ArenaProjection,
    view: &BattleViewModel,
    e: &FxView,
    muzzle: Option<Pos2>,
    textures: ArtTextures,
) -> Option<LightPool> {
    if e.kind != EventKind::Shot || !e.volley.is_some_and(|volley| volley.first) {
        return None;
    }
    let profile = weapon_fx::weapon_fx(e.source?);
    let at = fx::shot_origin(arena, view, e, muzzle, textures)?;
    let flash = u32::from(profile.flash_life_ticks);
    let age = fx::shot_age_ticks(e);
    if age > flash + MUZZLE_LIGHT_TAIL_TICKS {
        return None;
    }
    let tail = age.saturating_sub(flash) as f32 / (MUZZLE_LIGHT_TAIL_TICKS + 1) as f32;
    Some(LightPool {
        at,
        radius: Vec2::splat(arena.lane_height() * profile.light_radius),
        rgb: profile.flash_rgb,
        intensity: profile.light_intensity * (1.0 - tail),
    })
}

/// The pools as one untextured mesh of [`LIGHT_SEGMENTS`]-segment fans: an
/// additive centre of `rgb * intensity * light_gain` falling to a
/// transparent rim.
pub(crate) fn pools_mesh(pools: &[LightPool], light_gain: f32) -> Mesh {
    let mut mesh = Mesh::default();
    let per_pool = LIGHT_SEGMENTS as usize + 1;
    mesh.vertices.reserve(pools.len() * per_pool);
    mesh.indices
        .reserve(pools.len() * LIGHT_SEGMENTS as usize * 3);
    for pool in pools {
        let strength = (pool.intensity * light_gain).clamp(0.0, 1.0);
        let [r, g, b] = pool.rgb.map(|v| (f32::from(v) * strength).round() as u8);
        let hub = mesh.vertices.len() as u32;
        mesh.colored_vertex(pool.at, C::from_rgb_additive(r, g, b));
        for n in 0..LIGHT_SEGMENTS {
            let angle = n as f32 / LIGHT_SEGMENTS as f32 * TAU;
            mesh.colored_vertex(
                pool.at + vec2(angle.cos() * pool.radius.x, angle.sin() * pool.radius.y),
                C::TRANSPARENT,
            );
        }
        for n in 0..LIGHT_SEGMENTS {
            mesh.add_triangle(hub, hub + 1 + n, hub + 1 + (n + 1) % LIGHT_SEGMENTS);
        }
    }
    mesh
}

/// Paints `pools` as one additive mesh; nothing when there are none.
pub(super) fn paint_pools(p: &Painter, pools: &[LightPool], light_gain: f32) {
    if !pools.is_empty() {
        p.add(Shape::mesh(pools_mesh(pools, light_gain)));
    }
}

#[cfg(test)]
mod tests;
