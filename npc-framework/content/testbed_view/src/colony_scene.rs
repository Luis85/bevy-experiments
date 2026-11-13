//! Live terrain and restrained village atmosphere. These are separate view entities.
#[path = "living_scene.rs"]
pub(crate) mod living_scene;
use bevy::prelude::*;
use npc_framework::prelude::{SimClock, SimConfig, SimPos};
use npc_framework::view_helpers::ViewHidden;
use npc_framework::view_helpers::{ViewSystems, VisualOf};
use std::collections::BTreeSet;
use testbed_content::village::{
    Role, Season, VillagePlace, calendar_at,
    terrain::{TerrainState, TerrainSurface},
};

#[derive(Component)]
struct TerrainVisual {
    x: i32,
    z: i32,
}
#[derive(Resource)]
struct TerrainPalette {
    grass: [Handle<StandardMaterial>; 3],
    soil: Handle<StandardMaterial>,
    stone: Handle<StandardMaterial>,
    shore: Handle<StandardMaterial>,
    shallows: Handle<StandardMaterial>,
    season: Option<Season>,
}
#[derive(Component)]
struct Lantern;
#[derive(Component)]
struct Smoke {
    anchor: Vec3,
    phase: f32,
}
#[derive(Resource, Default)]
struct HiddenArrivals(BTreeSet<Entity>);

pub(crate) fn plugin(app: &mut App) {
    if testbed_content::village::frontier::enabled(app.world()) {
        living_scene::plugin(app);
        crate::island_atmosphere::plugin(app);
        crate::island_audio::plugin(app);
        app.init_resource::<HiddenArrivals>().add_systems(
            Update,
            hide_arrivals
                .in_set(ViewSystems::Present)
                .after(crate::fog::FogSet),
        );
        return;
    }
    if !testbed_content::village::colony::enabled(app.world()) {
        return;
    }
    app.init_resource::<HiddenArrivals>()
        .add_systems(Startup, spawn)
        .add_systems(Update, sync_terrain.in_set(ViewSystems::Sync))
        .add_systems(Update, atmosphere.in_set(ViewSystems::Present))
        .add_systems(
            Update,
            elevate_actors
                .in_set(ViewSystems::Present)
                .after(crate::village_world::corpse_pose),
        )
        .add_systems(
            Update,
            hide_arrivals
                .in_set(ViewSystems::Present)
                .after(crate::fog::FogSet),
        );
}
fn material(materials: &mut Assets<StandardMaterial>, color: Color) -> Handle<StandardMaterial> {
    materials.add(StandardMaterial {
        base_color: color,
        perceptual_roughness: 0.95,
        ..default()
    })
}
fn spawn(
    mut commands: Commands,
    terrain: Res<TerrainState>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    places: Query<(&SimPos, &VillagePlace)>,
) {
    let cube = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
    let grass = [
        material(&mut materials, Color::srgb(0.47, 0.61, 0.30)),
        material(&mut materials, Color::srgb(0.49, 0.63, 0.32)),
        material(&mut materials, Color::srgb(0.46, 0.60, 0.29)),
    ];
    let soil = material(&mut materials, Color::srgb(0.43, 0.32, 0.20));
    let stone = material(&mut materials, Color::srgb(0.51, 0.54, 0.52));
    let shore = material(&mut materials, Color::srgb(0.68, 0.61, 0.43));
    let shallows = materials.add(StandardMaterial {
        base_color: Color::srgb(0.075, 0.32, 0.34),
        perceptual_roughness: 0.95,
        reflectance: 0.03,
        ..default()
    });
    for z in -32..32 {
        for x in -32..32 {
            let cell = terrain.cell(x, z).expect("bounded terrain grid");
            let top = cell.height_mm as f32 / 1000.0;
            let height = (top + 1.2).max(0.05);
            commands.spawn((
                Name::new(format!("terrain {x},{z}")),
                TerrainVisual { x, z },
                Mesh3d(cube.clone()),
                MeshMaterial3d(grass[variation(x, z)].clone()),
                Transform::from_xyz(x as f32 + 0.5, top - height * 0.5, z as f32 + 0.5)
                    .with_scale(Vec3::new(1.0, height, 1.0)),
                if cell.surface == TerrainSurface::Water {
                    Visibility::Hidden
                } else {
                    Visibility::Inherited
                },
            ));
        }
    }
    commands.insert_resource(TerrainPalette {
        grass,
        soil,
        stone,
        shore,
        shallows,
        season: None,
    });
    let wood = material(&mut materials, Color::srgb(0.24, 0.17, 0.11));
    let metal = material(&mut materials, Color::srgb(0.14, 0.16, 0.17));
    let glass = materials.add(StandardMaterial {
        base_color: Color::srgb(0.95, 0.72, 0.32),
        emissive: LinearRgba::new(3.0, 1.8, 0.4, 1.0),
        ..default()
    });
    // Lanterns frame the public meeting space and the main paths, without blocking them.
    for (x, z) in [
        (-3.5, -1.5),
        (3.5, -1.5),
        (-9.0, 2.0),
        (9.5, 0.0),
        (-9.0, -8.0),
        (9.5, -8.0),
    ] {
        let root = commands
            .spawn((
                Name::new("village lantern"),
                Transform::from_xyz(x, 0.0, z),
                Visibility::default(),
            ))
            .id();
        for (position, scale, mat) in [
            (
                Vec3::new(0.0, 0.9, 0.0),
                Vec3::new(0.12, 1.8, 0.12),
                wood.clone(),
            ),
            (
                Vec3::new(0.0, 1.80, 0.0),
                Vec3::new(0.38, 0.08, 0.38),
                metal.clone(),
            ),
            (
                Vec3::new(0.0, 1.60, 0.0),
                Vec3::new(0.25, 0.32, 0.25),
                glass.clone(),
            ),
            (
                Vec3::new(0.0, 1.41, 0.0),
                Vec3::new(0.36, 0.07, 0.36),
                metal.clone(),
            ),
        ] {
            commands.spawn((
                ChildOf(root),
                Mesh3d(cube.clone()),
                MeshMaterial3d(mat),
                Transform::from_translation(position).with_scale(scale),
            ));
        }
        commands.spawn((
            ChildOf(root),
            Lantern,
            PointLight {
                color: Color::srgb(1.0, 0.71, 0.34),
                intensity: 0.0,
                range: 7.0,
                shadow_maps_enabled: false,
                ..default()
            },
            Transform::from_xyz(0.0, 1.65, 0.0),
        ));
    }
    // Seating makes the shared meeting place legible at colony scale.
    for (x, z) in [(-2.6, -2.7), (2.6, -2.7)] {
        let root = commands
            .spawn((
                Name::new("village gathering bench"),
                Transform::from_xyz(x, 0.0, z),
                Visibility::default(),
            ))
            .id();
        for (position, scale) in [
            (Vec3::new(0.0, 0.45, 0.0), Vec3::new(1.6, 0.12, 0.48)),
            (Vec3::new(-0.58, 0.22, 0.0), Vec3::new(0.12, 0.44, 0.35)),
            (Vec3::new(0.58, 0.22, 0.0), Vec3::new(0.12, 0.44, 0.35)),
            (Vec3::new(0.0, 0.75, 0.22), Vec3::new(1.6, 0.32, 0.10)),
        ] {
            commands.spawn((
                ChildOf(root),
                Mesh3d(cube.clone()),
                MeshMaterial3d(wood.clone()),
                Transform::from_translation(position).with_scale(scale),
            ));
        }
    }
    let smoke_mesh = meshes.add(
        Sphere::new(0.20)
            .mesh()
            .ico(1)
            .expect("one-subdivision smoke mesh"),
    );
    let smoke_material = materials.add(StandardMaterial {
        base_color: Color::srgba(0.76, 0.78, 0.75, 0.20),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });
    for (position, place) in &places {
        if !matches!(place.role, Role::Baker | Role::Smith | Role::Innkeeper) {
            continue;
        }
        let anchor = Vec3::new(
            position.x as f32 / 1000.0,
            3.0,
            position.z as f32 / 1000.0 + 2.5,
        );
        for index in 0..5 {
            commands.spawn((
                Name::new("workshop chimney smoke"),
                Smoke {
                    anchor,
                    phase: index as f32 / 5.0,
                },
                Mesh3d(smoke_mesh.clone()),
                MeshMaterial3d(smoke_material.clone()),
                Transform::from_translation(anchor),
            ));
        }
    }
}
fn variation(x: i32, z: i32) -> usize {
    (x.wrapping_mul(17).wrapping_add(z.wrapping_mul(31))).unsigned_abs() as usize % 3
}
fn sync_terrain(
    terrain: Res<TerrainState>,
    palette: Res<TerrainPalette>,
    mut cells: Query<(
        &TerrainVisual,
        &mut Transform,
        &mut MeshMaterial3d<StandardMaterial>,
        &mut Visibility,
    )>,
) {
    if !terrain.is_changed() && !palette.is_added() {
        return;
    }
    for (cell, mut transform, mut material, mut visibility) in &mut cells {
        let Some(value) = terrain.cell(cell.x, cell.z) else {
            continue;
        };
        let top = value.height_mm as f32 / 1000.0;
        let height = (top + 1.2).max(0.05);
        let shore = [(-1, 0), (1, 0), (0, -1), (0, 1)]
            .into_iter()
            .any(|(x, z)| {
                terrain
                    .cell(cell.x + x, cell.z + z)
                    .is_some_and(|neighbor| {
                        (neighbor.surface == TerrainSurface::Water)
                            != (value.surface == TerrainSurface::Water)
                    })
            });
        transform.translation.y = top - height * 0.5;
        transform.scale.y = height;
        material.0 = match value.surface {
            TerrainSurface::Grass if shore => palette.shore.clone(),
            TerrainSurface::Grass => palette.grass[variation(cell.x, cell.z)].clone(),
            TerrainSurface::Soil => palette.soil.clone(),
            TerrainSurface::Stone => palette.stone.clone(),
            TerrainSurface::Water => palette.shallows.clone(),
        };
        if value.surface == TerrainSurface::Water {
            transform.translation.y = -0.125;
            transform.scale.y = 0.02;
        }
        *visibility = if value.surface == TerrainSurface::Water && !shore {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
    }
}
fn atmosphere(
    clock: Res<SimClock>,
    config: Res<SimConfig>,
    mut palette: ResMut<TerrainPalette>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut lamps: Query<&mut PointLight, With<Lantern>>,
    mut smoke: Query<(&Smoke, &mut Transform)>,
) {
    let (_, season, night) = calendar_at(clock.tick, &config, true);
    if palette.season != Some(season) {
        let color = match season {
            Season::Spring => [0.48, 0.63, 0.32],
            Season::Summer => [0.46, 0.58, 0.28],
            Season::Autumn => [0.39, 0.47, 0.27],
            Season::Winter => [0.78, 0.84, 0.86],
        };
        for (index, handle) in palette.grass.iter().enumerate() {
            if let Some(mut material) = materials.get_mut(handle) {
                let shift = index as f32 * 0.012;
                material.base_color =
                    Color::srgb(color[0] + shift, color[1] + shift, color[2] + shift);
            }
        }
        palette.season = Some(season);
    }
    for mut lamp in &mut lamps {
        lamp.intensity = if night { 32_000.0 } else { 0.0 };
    }
    for (particle, mut transform) in &mut smoke {
        let phase = ((clock.tick % 180) as f32 / 180.0 + particle.phase).fract();
        transform.translation = particle.anchor + Vec3::new(phase * 0.9, phase * 2.6, phase * 0.35);
        transform.scale = Vec3::splat(0.4 + phase * 1.8);
    }
}
fn elevate_actors(
    terrain: Res<TerrainState>,
    positions: Query<&SimPos>,
    mut visuals: Query<(&VisualOf, &mut Transform), With<crate::visuals::ActorVisual>>,
) {
    for (of, mut transform) in &mut visuals {
        let Ok(position) = positions.get(of.0) else {
            continue;
        };
        let Some(cell) = terrain.cell(position.x.div_euclid(1000), position.z.div_euclid(1000))
        else {
            continue;
        };
        if cell.surface != TerrainSurface::Water {
            transform.translation.y += cell.height_mm as f32 / 1000.0;
        }
    }
}
fn hide_arrivals(
    awaiting: Query<(), With<testbed_content::village::colony::AwaitingArrival>>,
    mut visuals: Query<(&VisualOf, &mut Visibility), With<crate::visuals::ActorVisual>>,
    mut previous: ResMut<HiddenArrivals>,
    mut hidden: ResMut<ViewHidden>,
) {
    for (of, mut visibility) in &mut visuals {
        if awaiting.contains(of.0) {
            *visibility = Visibility::Hidden;
            previous.0.insert(of.0);
            hidden.0.insert(of.0);
        } else if previous.0.remove(&of.0) {
            *visibility = Visibility::Inherited;
            hidden.0.remove(&of.0);
        }
    }
}
