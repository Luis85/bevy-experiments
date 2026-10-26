//! Ground, view-only dressing, sun/moon light and the day/night cycle.

use std::f32::consts::PI;

use crate::assets::ViewAssets;
use bevy::{light::CascadeShadowConfigBuilder, math::Affine2, prelude::*};
use npc_framework::prelude::{SimClock, SimConfig};
use npc_framework::view_helpers::ViewSystems;

/// Side of the square ground tile in metres.
const TILE: f32 = 32.0;

/// The sun/moon light.
#[derive(Component)]
pub(crate) struct Sun;

pub(crate) fn plugin(app: &mut App) {
    app.insert_resource(ClearColor(DAY_SKY))
        .insert_resource(GlobalAmbientLight {
            color: Color::WHITE,
            brightness: 600.0,
            ..default()
        })
        .add_systems(Startup, (spawn_ground, spawn_dressing, spawn_sun))
        .add_systems(Update, day_night.in_set(ViewSystems::Present));
}

fn spawn_ground(
    mut commands: Commands,
    assets: Option<Res<ViewAssets>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let texture = assets.map(|a| a.grass.clone());
    commands.spawn((
        Name::new("ground"),
        Mesh3d(meshes.add(Plane3d::default().mesh().size(TILE, TILE))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.66, 0.86, 0.42),
            base_color_texture: texture,
            uv_transform: Affine2::from_scale(Vec2::splat(8.0)),
            perceptual_roughness: 1.0,
            reflectance: 0.15,
            ..default()
        })),
    ));
    // A darker skirt so the tile's edge does not end in the void.
    commands.spawn((
        Name::new("ground skirt"),
        Mesh3d(meshes.add(Plane3d::default().mesh().size(TILE * 8.0, TILE * 8.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.20, 0.32, 0.16),
            perceptual_roughness: 1.0,
            reflectance: 0.1,
            ..default()
        })),
        Transform::from_xyz(0.0, -0.02, 0.0),
    ));
}

/// What a dressing item is.
#[derive(Clone, Copy)]
enum Prop {
    TreeA,
    TreeB,
    Trees,
    RockA,
    RockC,
    Fence,
    Lumber,
}

/// (prop, x m, z m, yaw deg). All kept outside the walking area between the
/// places (roughly x ∈ [-7, 6], z ∈ [-6, 3]).
const DRESSING: &[(Prop, f32, f32, f32)] = &[
    // north-west grove
    (Prop::Trees, -12.0, -11.5, 0.0),
    (Prop::TreeA, -14.2, -7.5, 30.0),
    (Prop::TreeB, -9.0, -13.8, 70.0),
    // north-east grove
    (Prop::Trees, 12.5, -12.0, 120.0),
    (Prop::TreeB, 14.2, -8.0, 10.0),
    (Prop::TreeA, 9.5, -14.0, 200.0),
    // south-west
    (Prop::Trees, -12.5, 12.0, 240.0),
    (Prop::TreeA, -14.0, 7.0, 90.0),
    (Prop::TreeB, -8.0, 14.0, 160.0),
    // south-east
    (Prop::TreeA, 13.2, 12.8, 45.0),
    (Prop::TreeB, 14.3, 8.0, 300.0),
    (Prop::Trees, 9.5, 13.5, 30.0),
    // edges
    (Prop::TreeA, -14.5, 1.0, 0.0),
    (Prop::TreeB, 14.4, -2.0, 0.0),
    (Prop::TreeA, 1.5, -14.5, 0.0),
    (Prop::RockC, -9.5, 8.5, 20.0),
    (Prop::RockA, -10.6, 9.2, 80.0),
    (Prop::RockC, 9.8, -8.6, 140.0),
    (Prop::RockA, 11.0, 2.0, 10.0),
    (Prop::RockC, -4.0, 12.0, 60.0),
    (Prop::RockA, 6.5, 12.3, 200.0),
    (Prop::RockA, -11.5, -2.5, 300.0),
    (Prop::RockC, -1.5, -12.0, 0.0),
    // fence along the field's east and south sides
    (Prop::Fence, 7.2, 3.4, 0.0),
    (Prop::Fence, 7.2, 6.9, 0.0),
    (Prop::Fence, 1.4, 9.4, 90.0),
    (Prop::Fence, 4.9, 9.4, 90.0),
    // lumber piles
    (Prop::Lumber, -10.5, -5.0, 20.0),
    (Prop::Lumber, -10.2, -6.6, 5.0),
    (Prop::Lumber, 10.8, -7.2, 75.0),
];

fn spawn_dressing(mut commands: Commands, assets: Option<Res<ViewAssets>>) {
    let Some(assets) = assets else { return };
    for &(prop, x, z, yaw) in DRESSING {
        let (scene, scale, local_center) = match prop {
            Prop::TreeA => (&assets.tree_a, 4.0, Vec3::ZERO),
            Prop::TreeB => (&assets.tree_b, 4.2, Vec3::ZERO),
            Prop::Trees => (&assets.trees_medium, 3.2, Vec3::ZERO),
            Prop::RockA => (&assets.rock_a, 4.0, Vec3::ZERO),
            Prop::RockC => (&assets.rock_c, 3.5, Vec3::ZERO),
            // The fence mesh sits on a hex edge at local x ≈ -1.
            Prop::Fence => (&assets.fence, 3.0, Vec3::new(-1.0, 0.0, 0.0)),
            Prop::Lumber => (&assets.lumber, 4.0, Vec3::ZERO),
        };
        let rot = Quat::from_rotation_y(yaw.to_radians());
        let translation = Vec3::new(x, 0.0, z) - rot * (local_center * scale);
        commands.spawn((
            Name::new("dressing"),
            WorldAssetRoot(scene.clone()),
            Transform::from_translation(translation)
                .with_rotation(rot)
                .with_scale(Vec3::splat(scale)),
        ));
    }
}

fn spawn_sun(mut commands: Commands) {
    commands.spawn((
        Name::new("sun"),
        Sun,
        DirectionalLight {
            illuminance: 10_000.0,
            shadow_maps_enabled: true,
            ..default()
        },
        CascadeShadowConfigBuilder {
            first_cascade_far_bound: 20.0,
            maximum_distance: 80.0,
            ..default()
        }
        .build(),
        Transform::from_xyz(5.0, 10.0, 8.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

const DAY_SKY: Color = Color::srgb(0.56, 0.76, 0.93);
const DUSK_SKY: Color = Color::srgb(0.85, 0.62, 0.48);
const NIGHT_SKY: Color = Color::srgb(0.05, 0.07, 0.16);

/// Light parameters for one moment of the day.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Lighting {
    /// Unit vector from the ground towards the light.
    to_light: Vec3,
    color: Color,
    illuminance: f32,
    ambient_color: Color,
    ambient: f32,
    sky: Color,
}

fn dir(elevation_deg: f32, azimuth_deg: f32) -> Vec3 {
    let (el, az) = (elevation_deg.to_radians(), azimuth_deg.to_radians());
    Vec3::new(el.cos() * az.sin(), el.sin(), el.cos() * az.cos())
}

fn mix(a: Color, b: Color, t: f32) -> Color {
    a.mix(&b, t)
}

/// Day/night lighting for `permille` (0..1000) of the day with the night as
/// the last `night_permille`. Day: the sun arcs from east (+X) over the
/// camera side (+Z) to west. Night: a dim, blue moon. Short cross-fades at
/// dusk and dawn.
pub(crate) fn lighting(permille: u32, night_permille: u32) -> Lighting {
    let f = permille as f32 / 1000.0;
    let day_end = 1.0 - night_permille.min(1000) as f32 / 1000.0;
    const FADE: f32 = 0.04;

    // Sun along its arc, s ∈ [0, 1] over the day.
    let s = if day_end > 0.0 {
        (f / day_end).clamp(0.0, 1.0)
    } else {
        0.0
    };
    // `sin(PI)` is slightly negative in f32; clamp so `sqrt` never yields NaN.
    let height = (PI * s).sin().clamp(0.0, 1.0);
    let sun = Lighting {
        to_light: dir(10.0 + 50.0 * height, 75.0 - 150.0 * s),
        color: mix(
            Color::srgb(1.0, 0.68, 0.42),
            Color::srgb(1.0, 0.97, 0.90),
            height.sqrt(),
        ),
        illuminance: 3_500.0 + 8_500.0 * height,
        ambient_color: mix(
            Color::srgb(1.0, 0.85, 0.75),
            Color::srgb(0.85, 0.92, 1.0),
            height.sqrt(),
        ),
        ambient: 350.0 + 350.0 * height,
        sky: mix(DUSK_SKY, DAY_SKY, height.sqrt()),
    };
    let moon = Lighting {
        to_light: dir(45.0, 20.0),
        color: Color::srgb(0.55, 0.66, 1.0),
        illuminance: 1_800.0,
        ambient_color: Color::srgb(0.45, 0.55, 1.0),
        ambient: 220.0,
        sky: NIGHT_SKY,
    };

    // Night weight: 0 during the day, 1 during the night, faded at both ends.
    let night = if f >= day_end {
        ((f - day_end) / FADE).clamp(0.0, 1.0)
    } else if f < FADE {
        1.0 - f / FADE
    } else {
        0.0
    };
    let t = night * night * (3.0 - 2.0 * night);
    Lighting {
        to_light: sun.to_light.lerp(moon.to_light, t).normalize(),
        color: mix(sun.color, moon.color, t),
        illuminance: sun.illuminance + (moon.illuminance - sun.illuminance) * t,
        ambient_color: mix(sun.ambient_color, moon.ambient_color, t),
        ambient: sun.ambient + (moon.ambient - sun.ambient) * t,
        sky: mix(sun.sky, moon.sky, t),
    }
}

fn day_night(
    clock: Option<Res<SimClock>>,
    config: Option<Res<SimConfig>>,
    mut ambient: ResMut<GlobalAmbientLight>,
    mut clear: ResMut<ClearColor>,
    mut suns: Query<(&mut DirectionalLight, &mut Transform), With<Sun>>,
) {
    let (Some(clock), Some(config)) = (clock, config) else {
        return;
    };
    if config.day_length == 0 {
        return;
    }
    let l = lighting(
        config.time_of_day_permille(clock.tick),
        config.night_permille,
    );
    for (mut light, mut transform) in &mut suns {
        light.color = l.color;
        light.illuminance = l.illuminance;
        *transform = Transform::from_translation(l.to_light * 20.0).looking_at(Vec3::ZERO, Vec3::Y);
    }
    ambient.color = l.ambient_color;
    ambient.brightness = l.ambient;
    clear.0 = l.sky;
}
