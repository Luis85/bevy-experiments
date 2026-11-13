//! Ground (grass outside, Dungeon floor tiles in the workshop and the
//! store), a little view-only dressing and fixed daylight.

use bevy::{light::CascadeShadowConfigBuilder, math::Affine2, prelude::*};
use testbed_content::layout::{HALF_X, HALF_Z, INNER_Z};

use crate::assets::ViewAssets;
use crate::fog::{FogArea, FogDim};
use crate::visuals::V;

/// Side of the grass plane in metres.
const GROUND: f32 = 90.0;
/// A Dungeon floor tile is 4 units square (3 m at `V`).
const FLOOR_TILE: f32 = 4.0 * V;

const SKY: Color = Color::srgb(0.56, 0.76, 0.93);

pub(crate) fn plugin(app: &mut App) {
    app.insert_resource(ClearColor(SKY))
        .insert_resource(GlobalAmbientLight {
            color: Color::srgb(0.9, 0.93, 1.0),
            brightness: 650.0,
            ..default()
        })
        .add_systems(Startup, (spawn_ground, spawn_dressing, spawn_sun));
}

fn spawn_ground(
    mut commands: Commands,
    assets: Option<Res<ViewAssets>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let texture = assets.as_ref().map(|a| a.grass.clone());
    commands.spawn((
        Name::new("ground"),
        Mesh3d(meshes.add(Plane3d::default().mesh().size(GROUND, GROUND))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.66, 0.86, 0.42),
            base_color_texture: texture,
            uv_transform: Affine2::from_scale(Vec2::splat(22.0)),
            perceptual_roughness: 1.0,
            reflectance: 0.12,
            ..default()
        })),
    ));
    let Some(assets) = assets else { return };
    // The rooms north of the inner wall: the interior cells x -HALF_X+1 ..
    // HALF_X-1, z INNER_Z+1 .. HALF_Z-1 (a tile may run under a wall).
    let (x0, x1) = ((1 - HALF_X) as f32, HALF_X as f32);
    let (z0, z1) = ((INNER_Z + 1) as f32, HALF_Z as f32);
    let nx = ((x1 - x0) / FLOOR_TILE).ceil() as i32;
    let nz = ((z1 - z0) / FLOOR_TILE).ceil() as i32;
    for i in 0..nx {
        for j in 0..nz {
            let x = x0 + FLOOR_TILE * (i as f32 + 0.5);
            let z = z0 + FLOOR_TILE * (j as f32 + 0.5);
            commands.spawn((
                Name::new("floor tile"),
                WorldAssetRoot(assets.models.floor.clone()),
                Transform::from_xyz(x, -0.03, z).with_scale(Vec3::splat(V)),
            ));
        }
    }
}

/// View-only dressing (no sim entity): barrels and crates in the store and
/// rocks outside the walls. Kept off every cell actors use. Fogged by its
/// cell like the statics (it never changes).
fn spawn_dressing(mut commands: Commands, assets: Option<Res<ViewAssets>>) {
    let Some(assets) = assets else { return };
    let m = &assets.models;
    let items = [
        (&m.barrel, 11.2, 9.3, 0.0, 0.6),
        (&m.barrel, 10.3, 9.4, 40.0, 0.6),
        (&m.crate_large, 1.9, 9.2, 10.0, 0.5),
        (&m.rock, -16.0, 6.0, 30.0, 3.0),
        (&m.rock, -15.0, -12.0, 120.0, 2.4),
        (&m.rock, 14.5, 12.5, 200.0, 2.8),
        (&m.rock, -6.0, 14.0, 80.0, 2.2),
        (&m.lumber, -11.0, -9.2, 0.0, 2.6),
    ];
    for (scene, x, z, yaw, scale) in items {
        commands.spawn((
            Name::new("dressing"),
            WorldAssetRoot(scene.clone()),
            Transform::from_xyz(x, 0.0, z)
                .with_rotation(Quat::from_rotation_y(f32::to_radians(yaw)))
                .with_scale(Vec3::splat(scale)),
            Visibility::default(),
            FogArea::around(Vec3::new(x, 0.0, z), Vec3::new(0.5, 0.0, 0.5)),
            FogDim::default(),
        ));
    }
}

fn spawn_sun(mut commands: Commands) {
    commands.spawn((
        Name::new("sun"),
        DirectionalLight {
            illuminance: 11_000.0,
            color: Color::srgb(1.0, 0.97, 0.9),
            shadow_maps_enabled: true,
            ..default()
        },
        CascadeShadowConfigBuilder {
            first_cascade_far_bound: 25.0,
            maximum_distance: 120.0,
            ..default()
        }
        .build(),
        Transform::from_xyz(12.0, 20.0, 9.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}
