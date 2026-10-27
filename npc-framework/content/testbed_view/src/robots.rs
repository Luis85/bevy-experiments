//! Procedural robot bodies. All parts belong to visual entities only.
use bevy::prelude::*;
use npc_framework::perception::{Sensor, Sensors};

use crate::assets::ModelKind;

#[derive(Resource)]
pub(crate) struct RobotAssets {
    body: Handle<Mesh>,
    wheel: Handle<Mesh>,
    dome: Handle<Mesh>,
    mast: Handle<Mesh>,
    lens: Handle<Mesh>,
    rover: Handle<StandardMaterial>,
    sentinel: Handle<StandardMaterial>,
    rubber: Handle<StandardMaterial>,
    metal: Handle<StandardMaterial>,
    glass: Handle<StandardMaterial>,
}

pub(crate) fn plugin(app: &mut App) {
    app.add_systems(Startup, make_assets);
}

fn make_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let paint = |color| StandardMaterial {
        base_color: color,
        metallic: 0.35,
        perceptual_roughness: 0.45,
        ..default()
    };
    commands.insert_resource(RobotAssets {
        body: meshes.add(Cuboid::new(0.85, 0.35, 1.1)),
        wheel: meshes.add(Cylinder::new(0.24, 0.16)),
        dome: meshes.add(Sphere::new(0.18)),
        mast: meshes.add(Cylinder::new(0.055, 0.45)),
        lens: meshes.add(Cuboid::new(0.2, 0.12, 0.12)),
        rover: materials.add(paint(Color::srgb(0.83, 0.49, 0.12))),
        sentinel: materials.add(paint(Color::srgb(0.25, 0.54, 0.65))),
        rubber: materials.add(paint(Color::srgb(0.06, 0.07, 0.08))),
        metal: materials.add(paint(Color::srgb(0.42, 0.46, 0.49))),
        glass: materials.add(StandardMaterial {
            base_color: Color::srgb(0.1, 0.8, 0.9),
            emissive: LinearRgba::new(0.0, 0.4, 0.5, 1.0),
            ..default()
        }),
    });
}

/// Returns the two grasp points, so carried tools use the ordinary item view.
pub(crate) fn spawn(
    commands: &mut Commands,
    root: Entity,
    kind: ModelKind,
    sensors: Option<&Sensors>,
    assets: &RobotAssets,
) -> (Entity, Entity) {
    let tall = kind == ModelKind::Sentinel;
    let paint = if tall {
        &assets.sentinel
    } else {
        &assets.rover
    };
    let part = |commands: &mut Commands,
                name: &'static str,
                mesh: &Handle<Mesh>,
                material: &Handle<StandardMaterial>,
                transform: Transform| {
        commands
            .spawn((
                ChildOf(root),
                Name::new(name),
                Mesh3d(mesh.clone()),
                MeshMaterial3d(material.clone()),
                transform,
            ))
            .id()
    };
    part(
        commands,
        "robot chassis",
        &assets.body,
        paint,
        Transform::from_xyz(0.0, 0.46, 0.0),
    );
    // The complete body, including tires, fits the world's 1 m door cells.
    for x in [-0.4, 0.4] {
        for z in [-0.36, 0.36] {
            part(
                commands,
                "robot wheel",
                &assets.wheel,
                &assets.rubber,
                Transform::from_xyz(x, 0.24, z)
                    .with_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2)),
            );
        }
    }
    let height = if tall { 1.17 } else { 0.75 };
    if tall {
        part(
            commands,
            "sensor mast",
            &assets.mast,
            &assets.metal,
            Transform::from_xyz(0.0, 0.85, 0.0),
        );
    }
    if let Some(sensors) = sensors {
        let physical: Vec<_> = sensors
            .sensors
            .iter()
            .filter(|s| !matches!(s, Sensor::Proximity(_)))
            .collect();
        for (i, sensor) in physical.iter().enumerate() {
            let x = (i as f32 - (physical.len() as f32 - 1.0) / 2.0) * 0.38;
            part(
                commands,
                "sensor dome",
                &assets.dome,
                &assets.metal,
                Transform::from_xyz(x, height, 0.15),
            );
            let scale = if matches!(sensor, Sensor::Radar(_)) {
                Vec3::new(1.6, 1.6, 0.5)
            } else {
                Vec3::ONE
            };
            part(
                commands,
                "sensor lens",
                &assets.lens,
                &assets.glass,
                Transform::from_xyz(x, height, 0.32).with_scale(scale),
            );
        }
    }
    let left = commands
        .spawn((
            ChildOf(root),
            Name::new("robot left grip"),
            Transform::from_xyz(-0.47, 0.55, 0.55),
        ))
        .id();
    let right = commands
        .spawn((
            ChildOf(root),
            Name::new("robot right grip"),
            Transform::from_xyz(0.47, 0.55, 0.55),
        ))
        .id();
    (right, left)
}
