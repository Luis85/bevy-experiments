use super::MenuState;
use bevy::{
    camera::{Exposure, RenderTarget, visibility::RenderLayers},
    prelude::*,
    render::render_resource::TextureFormat,
};
use terra_bevy::geometry::{MeshBuilder, feature_meshes_lod, terrain_mesh_lod};
use terra_core::{GeoPosition, LocalFrame, TileKey};

#[derive(Resource)]
pub struct PreviewImage(pub Handle<Image>);
#[derive(Component)]
pub(super) struct Avatar;
#[derive(Component)]
struct PreviewBody;
#[derive(Resource)]
pub(super) struct AvatarMaterials {
    skin: Handle<StandardMaterial>,
    hair: Handle<StandardMaterial>,
    outfit: Handle<StandardMaterial>,
}

pub fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    // A small immutable scene uses the same source mesh builders as the game.
    // It never opens a save, starts simulation, or sends provider requests.
    let focus = GeoPosition::new(48.137, 11.575, 0.).expect("fixed menu location");
    let frame = LocalFrame::new(focus);
    let key = TileKey::at(focus, 15).expect("fixed menu tile");
    let ground = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        perceptual_roughness: 0.95,
        reflectance: 0.02,
        ..default()
    });
    for key in key.neighbors(1) {
        let chunk = terra_stream::procedural_chunk(key, 33, 42);
        let local = frame.to_local(key.center());
        let transform = Transform::from_xyz(local[0] as f32, local[1] as f32, local[2] as f32);
        if let Some(mesh) = terrain_mesh_lod(&chunk, 1) {
            commands.spawn((
                Mesh3d(meshes.add(mesh)),
                MeshMaterial3d(ground.clone()),
                transform,
            ));
        }
        for mesh in feature_meshes_lod(&chunk, 96, &Default::default(), 2).into_values() {
            commands.spawn((
                Mesh3d(meshes.add(mesh)),
                MeshMaterial3d(ground.clone()),
                transform,
            ));
        }
    }
    commands.spawn((
        Camera3d::default(),
        IsDefaultUiCamera,
        Msaa::Off,
        Exposure { ev100: 9.7 },
        Camera {
            order: 0,
            clear_color: Color::srgb(0.37, 0.49, 0.45).into(),
            ..default()
        },
        Transform::from_xyz(980., 740., 780.).looking_at(Vec3::new(0., 50., 0.), Vec3::Y),
        DistanceFog {
            color: Color::srgb(0.35, 0.47, 0.42),
            falloff: FogFalloff::Linear {
                start: 500.,
                end: 2500.,
            },
            ..default()
        },
    ));
    commands.spawn((
        DirectionalLight {
            illuminance: 12_000.,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.85, -0.8, 0.)),
        RenderLayers::layer(0),
    ));
    // The window camera draws its own UI. An extra camera with a different
    // MSAA mode and no clear owns another intermediate texture, which preserves
    // stale menu text between screens instead of compositing over this scene.

    let image = images.add(Image::new_target_texture(
        320,
        420,
        TextureFormat::Rgba8Unorm,
        Some(TextureFormat::Rgba8UnormSrgb),
    ));
    commands.insert_resource(PreviewImage(image.clone()));
    commands.spawn((
        Camera3d::default(),
        Msaa::Off,
        Exposure { ev100: 9.5 },
        AmbientLight {
            color: Color::srgb(0.91, 0.95, 1.),
            brightness: 260.,
            ..default()
        },
        Camera {
            order: -1,
            clear_color: Color::srgb(0.12, 0.21, 0.18).into(),
            ..default()
        },
        RenderTarget::Image(image.into()),
        RenderLayers::layer(1),
        Transform::from_xyz(1.55, 1.35, 2.7).looking_at(Vec3::new(0., 0.95, 0.), Vec3::Y),
    ));
    // A separate studio key cannot inherit terrain shadows from the backdrop.
    commands.spawn((
        DirectionalLight {
            illuminance: 8_000.,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.65, -0.5, 0.)),
        RenderLayers::layer(1),
    ));
    commands.spawn((
        PointLight {
            intensity: 60_000.,
            range: 12.,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_xyz(-2., 2.5, 2.8),
        RenderLayers::layer(1),
    ));
    let platform = materials.add(StandardMaterial {
        base_color: Color::srgb(0.19, 0.30, 0.23),
        perceptual_roughness: 0.9,
        ..default()
    });
    commands.spawn((
        Mesh3d(meshes.add(Cylinder::new(0.85, 0.08).mesh().resolution(48))),
        MeshMaterial3d(platform),
        Transform::from_xyz(0., -0.04, 0.),
        RenderLayers::layer(1),
    ));
    let skin = materials.add(Color::srgb(0.79, 0.59, 0.40));
    let hair = materials.add(Color::srgb(0.21, 0.12, 0.07));
    let outfit = materials.add(Color::srgb(0.32, 0.46, 0.24));
    let trousers = materials.add(Color::srgb(0.20, 0.25, 0.24));
    let boots = materials.add(Color::srgb(0.14, 0.13, 0.11));
    let eyes = materials.add(Color::srgb(0.05, 0.065, 0.055));
    let root = commands
        .spawn((
            Avatar,
            Transform::from_rotation(Quat::from_rotation_y(0.1)),
            Visibility::default(),
        ))
        .id();
    let mut parts = Vec::new();
    let mut cuboid = |center: Vec3, size: Vec3, material: Handle<StandardMaterial>| {
        parts.push(
            commands
                .spawn((
                    PreviewBody,
                    Mesh3d(meshes.add(Cuboid::from_size(size))),
                    MeshMaterial3d(material),
                    Transform::from_translation(center),
                    RenderLayers::layer(1),
                ))
                .id(),
        );
    };
    cuboid(
        Vec3::new(0., 1.03, 0.),
        Vec3::new(0.43, 0.61, 0.23),
        outfit.clone(),
    );
    for side in [-1., 1.] {
        cuboid(
            Vec3::new(side * 0.125, 0.43, 0.),
            Vec3::new(0.16, 0.74, 0.19),
            trousers.clone(),
        );
        cuboid(
            Vec3::new(side * 0.125, 0.075, 0.055),
            Vec3::new(0.19, 0.14, 0.29),
            boots.clone(),
        );
        cuboid(
            Vec3::new(side * 0.28, 0.99, 0.),
            Vec3::new(0.14, 0.58, 0.16),
            outfit.clone(),
        );
        cuboid(
            Vec3::new(side * 0.28, 0.67, 0.01),
            Vec3::new(0.105, 0.12, 0.11),
            skin.clone(),
        );
    }
    cuboid(
        Vec3::new(0., 1.40, 0.),
        Vec3::new(0.14, 0.13, 0.14),
        skin.clone(),
    );
    cuboid(
        Vec3::new(0., 1.59, 0.),
        Vec3::new(0.29, 0.31, 0.26),
        skin.clone(),
    );
    cuboid(
        Vec3::new(0., 1.755, -0.015),
        Vec3::new(0.32, 0.10, 0.29),
        hair.clone(),
    );
    for side in [-1., 1.] {
        cuboid(
            Vec3::new(side * 0.064, 1.63, 0.134),
            Vec3::new(0.025, 0.025, 0.008),
            eyes.clone(),
        );
    }
    commands.entity(root).add_children(&parts);
    commands.insert_resource(AvatarMaterials { skin, hair, outfit });
    // Small direction marker gives the preview a grounded, readable facing.
    let mut marker = MeshBuilder::default();
    marker.triangle(
        Vec3::new(-0.1, 0.005, 0.65),
        Vec3::new(0.1, 0.005, 0.65),
        Vec3::new(0., 0.005, 0.80),
        [0.7, 0.86, 0.56, 1.],
    );
    if let Some(mesh) = marker.finish() {
        commands.spawn((
            Mesh3d(meshes.add(mesh)),
            MeshMaterial3d(ground),
            RenderLayers::layer(1),
        ));
    }
}

pub fn update(
    menu: Res<MenuState>,
    handles: Option<Res<AvatarMaterials>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut avatar: Query<&mut Transform, With<Avatar>>,
) {
    let Some(handles) = handles else {
        return;
    };
    let (skin, hair, outfit) = crate::character::appearance_colors(&menu.draft.character);
    for (handle, rgb) in [
        (&handles.skin, skin),
        (&handles.hair, hair),
        (&handles.outfit, outfit),
    ] {
        if let Some(mut material) = materials.get_mut(handle) {
            material.base_color = Color::srgb(rgb[0], rgb[1], rgb[2]);
        }
    }
    for mut transform in &mut avatar {
        transform.scale = Vec3::from_array(crate::character::body_scale(&menu.draft.character));
        transform.rotation = Quat::from_rotation_y(menu.preview_angle);
    }
}
