//! Decorative scenery stays outside the playable ground. The bridge alone is
//! shared game geometry, with the same dimensions and height as battle_core.

use battle_core::{BRIDGE_HALF_WIDTH, MapKind, RIVER_HALF_WIDTH, ScenarioConfig, terrain_height};
use bevy::prelude::*;

use super::geometry::{Geometry, linear};

fn sample(seed: u64, index: u64) -> f32 {
    let mut n = seed.wrapping_add(index.wrapping_mul(0x9e37_79b9_7f4a_7c15));
    n = (n ^ (n >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    n = (n ^ (n >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    ((n ^ (n >> 31)) >> 40) as f32 / 16_777_216.0
}

pub(super) fn scenery_mesh(config: &ScenarioConfig, extent: f32) -> Mesh {
    let mut geometry = Geometry::default();
    let wood = linear(0.31, 0.25, 0.19);
    for index in 0..56_u64 {
        let angle = sample(config.seed, index * 4 + 1) * std::f32::consts::TAU;
        let distance = extent * (1.15 + sample(config.seed, index * 4 + 2) * 0.38);
        let mut x = angle.cos() * distance;
        let mut z = angle.sin() * distance;
        // The circle's corners otherwise reach into the square playable arena.
        if x.abs().max(z.abs()) < extent + 5.0 {
            let ratio = (extent + 5.0) / x.abs().max(z.abs()).max(1.0);
            x *= ratio;
            z *= ratio;
        }
        if config.map == MapKind::Crossing && x.abs() < RIVER_HALF_WIDTH + 2.0 {
            continue;
        }
        let size = 3.3 + sample(config.seed, index * 4 + 3) * 4.5;
        let base = Vec3::new(x, terrain_height(config.map, x, z), z);
        geometry.cuboid(
            base + Vec3::Y * size * 0.26,
            Vec3::new(0.45, size * 0.52, 0.45),
            Quat::IDENTITY,
            wood,
            [0.0; 2],
        );
        geometry.cone(
            base + Vec3::Y * size * 0.28,
            size * 0.36,
            size * 0.76,
            linear(0.25, 0.38, 0.29),
            angle,
        );
        geometry.cone(
            base + Vec3::Y * size * 0.57,
            size * 0.26,
            size * 0.60,
            linear(0.32, 0.45, 0.34),
            angle + 0.4,
        );
    }
    for index in 0..24_u64 {
        let angle = sample(config.seed ^ 345, index * 3) * std::f32::consts::TAU;
        let distance = extent * (1.1 + sample(config.seed, index * 3 + 103) * 0.30);
        let x = angle.cos().signum() * (extent + 5.0 + angle.cos().abs() * distance * 0.22);
        let z = angle.sin() * distance;
        let size = 0.7 + sample(config.seed, index + 345) * 1.9;
        geometry.rock(
            Vec3::new(x, terrain_height(config.map, x, z) + size * 0.2, z),
            Vec3::new(size, size * 0.8, size * 1.1),
            angle,
            linear(0.47, 0.49, 0.43),
        );
    }
    // Four unobtrusive field standards make army orientation legible.
    for (side, color) in [
        (-1.0, linear(0.33, 0.71, 0.82)),
        (1.0, linear(0.86, 0.46, 0.35)),
    ] {
        for z in [-extent * 0.84, extent * 0.84] {
            let base = Vec3::new(
                side * extent * 0.8,
                terrain_height(config.map, side * extent * 0.8, z),
                z,
            );
            geometry.cuboid(
                base + Vec3::Y * 2.55,
                Vec3::new(0.14, 5.1, 0.14),
                Quat::IDENTITY,
                wood,
                [0.0; 2],
            );
            geometry.cuboid(
                base + Vec3::new(1.15, 4.0, 0.0),
                Vec3::new(2.3, 1.45, 0.055),
                Quat::IDENTITY,
                color,
                [0.0; 2],
            );
            geometry.rock(
                base,
                Vec3::new(0.7, 0.35, 0.7),
                0.2,
                linear(0.49, 0.49, 0.42),
            );
        }
    }
    if config.map == MapKind::Crossing {
        let deck_length = (RIVER_HALF_WIDTH + 1.0) * 2.0;
        geometry.cuboid(
            Vec3::new(0.0, 0.20, 0.0),
            Vec3::new(deck_length, 0.30, BRIDGE_HALF_WIDTH * 2.0),
            Quat::IDENTITY,
            linear(0.57, 0.43, 0.29),
            [0.0; 2],
        );
        // Plank joins remain above the deck by a tiny, stable offset.
        for plank in -9..=9 {
            geometry.cuboid(
                Vec3::new(plank as f32 * 0.62, 0.357, 0.0),
                Vec3::new(0.025, 0.012, BRIDGE_HALF_WIDTH * 2.0),
                Quat::IDENTITY,
                wood,
                [0.0; 2],
            );
        }
        for side in [-1.0, 1.0] {
            let z = side * (BRIDGE_HALF_WIDTH + 0.28);
            geometry.cuboid(
                Vec3::new(0.0, 1.1, z),
                Vec3::new(deck_length, 0.14, 0.15),
                Quat::IDENTITY,
                wood,
                [0.0; 2],
            );
            for x in [-5.7, -2.8, 0.0, 2.8, 5.7] {
                geometry.cuboid(
                    Vec3::new(x, 0.70, z),
                    Vec3::new(0.18, 1.3, 0.18),
                    Quat::IDENTITY,
                    wood,
                    [0.0; 2],
                );
            }
        }
    }
    geometry.mesh()
}

pub(super) fn water_mesh(extent: f32) -> Mesh {
    let mut geometry = Geometry::default();
    let x = RIVER_HALF_WIDTH;
    let z = extent * 1.55;
    geometry.quad(
        [
            Vec3::new(-x, -0.50, -z),
            Vec3::new(-x, -0.50, z),
            Vec3::new(x, -0.50, z),
            Vec3::new(x, -0.50, -z),
        ],
        linear(0.25, 0.48, 0.51),
        [0.0; 2],
    );
    for index in -20..=20 {
        let at = index as f32 * z / 21.0;
        let x0 = ((index as f32) * 1.7).sin() * 2.5;
        geometry.quad(
            [
                Vec3::new(x0 - 0.5, -0.48, at),
                Vec3::new(x0 - 0.5, -0.48, at + 0.12),
                Vec3::new(x0 + 1.2, -0.48, at + 0.12),
                Vec3::new(x0 + 1.2, -0.48, at),
            ],
            linear(0.37, 0.59, 0.60),
            [0.0; 2],
        );
    }
    geometry.mesh()
}
