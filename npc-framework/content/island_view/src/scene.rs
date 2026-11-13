//! Fixed daylight: the sky clear colour, ambient light and the sun (the
//! island's atmosphere layers its own lighting on top).

use bevy::{light::CascadeShadowConfigBuilder, prelude::*};

const SKY: Color = Color::srgb(0.56, 0.76, 0.93);

pub(crate) fn plugin(app: &mut App) {
    app.insert_resource(ClearColor(SKY))
        .insert_resource(GlobalAmbientLight {
            color: Color::srgb(0.9, 0.93, 1.0),
            brightness: 650.0,
            ..default()
        })
        .add_systems(Startup, spawn_sun);
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
