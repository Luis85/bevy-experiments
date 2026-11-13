//! Standalone actor-editor preview: separate visual entities, no simulation.
//!
//! The body view keeps the actor readable; Sensor ranges fits the real metre
//! extents. Both share the same rotating actor and sensor geometry.

use std::f32::consts::{FRAC_PI_2, PI};
use std::time::Duration;

use bevy::camera::{Viewport, visibility::RenderLayers};
use bevy::gltf::Gltf;
use bevy::input::mouse::{AccumulatedMouseScroll, MouseScrollUnit};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy::world_serialization::{WorldAsset, WorldInstanceReady};
use island_content::actors::{RANGE_MAX_MM, SensorSpec, human_preset};
use npc_framework::perception::Sensors;

use crate::assets::ModelKind;
use crate::robots::{self, RobotAssets};

/// The edited preset's model and sensor definitions. Replacing either
/// rebuilds only the visual preview; no actor or simulation is spawned.
#[derive(Resource, Clone, Debug, PartialEq, Eq)]
pub struct ModelPreview {
    pub model: String,
    pub sensors: Vec<SensorSpec>,
}

impl Default for ModelPreview {
    fn default() -> Self {
        let human = human_preset();
        Self {
            model: human.model,
            sensors: human.sensors,
        }
    }
}

/// The preview's display controls; all are presentation-only.
#[derive(Resource, Clone, Copy, Debug)]
pub struct ModelPreviewSettings {
    pub show_sensors: bool,
    pub fit_sensors: bool,
    pub spinning: bool,
}

impl Default for ModelPreviewSettings {
    fn default() -> Self {
        Self {
            show_sensors: true,
            fit_sensors: false,
            spinning: true,
        }
    }
}

/// Logical-pixel space reserved for the editor's sidebars and bars. The
/// 3D camera is clipped to what remains; the UI camera fills the window.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct ModelPreviewViewport {
    pub left: f32,
    pub right: f32,
    pub top: f32,
    pub bottom: f32,
}

impl Default for ModelPreviewViewport {
    fn default() -> Self {
        Self {
            left: 260.0,
            right: 460.0,
            top: 64.0,
            bottom: 64.0,
        }
    }
}

impl ModelPreviewViewport {
    /// Physical position and size, bounded by the render target. A window
    /// too small for the controls temporarily hides the 3D camera.
    pub fn physical_rect(self, size: UVec2, scale_factor: f32) -> Option<(UVec2, UVec2)> {
        let scale = if scale_factor.is_finite() && scale_factor > 0.0 {
            scale_factor
        } else {
            1.0
        };
        let pixels = |logical: f32| (logical.max(0.0) * scale).round() as u32;
        let position = UVec2::new(pixels(self.left).min(size.x), pixels(self.top).min(size.y));
        let extent = UVec2::new(
            size.x
                .saturating_sub(position.x.saturating_add(pixels(self.right))),
            size.y
                .saturating_sub(position.y.saturating_add(pixels(self.bottom))),
        );
        (extent.x > 0 && extent.y > 0).then_some((position, extent))
    }
}

/// Editor systems can run before this set to update the preview in the
/// same frame as a field edit.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ModelPreviewSystems;

/// A preview's root, exposed for inspection and headless editor tests.
#[derive(Component, Clone, Debug)]
pub struct PreviewActor {
    pub model: String,
}

#[derive(Component)]
struct PreviewCamera;

#[derive(Component)]
struct PreviewModel(ModelKind);

#[derive(Component)]
struct PreviewIdlePlayer(ModelKind);

#[derive(Resource)]
struct PreviewAssets {
    scenes: Vec<Handle<WorldAsset>>,
    gltfs: Vec<Handle<Gltf>>,
}

#[derive(Resource, Default)]
struct PreviewAnimations {
    idle: Vec<Option<(Handle<AnimationGraph>, AnimationNodeIndex)>>,
}

#[derive(Default)]
struct PreviewFraming {
    zoom: f32,
    fitted: Option<bool>,
}

/// A 3D turntable plus a full-window UI camera. Add after the host's normal
/// `DefaultPlugins` and asset plugin; no sim or `TestbedViewPlugin` needed.
pub struct ModelPreviewPlugin;

impl Plugin for ModelPreviewPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ModelPreview>()
            .init_resource::<ModelPreviewSettings>()
            .init_resource::<ModelPreviewViewport>()
            .init_resource::<PreviewAnimations>()
            .insert_resource(ClearColor(Color::srgb(0.045, 0.055, 0.07)))
            .insert_resource(GlobalAmbientLight {
                color: Color::srgb(0.92, 0.95, 1.0),
                brightness: 650.0,
                ..default()
            })
            .add_plugins(robots::plugin)
            .add_systems(PreStartup, load_assets)
            .add_systems(Startup, scene)
            .add_systems(
                Update,
                (
                    sync_model,
                    turntable,
                    idle_animation,
                    frame_camera,
                    draw_ranges,
                )
                    .chain()
                    .in_set(ModelPreviewSystems),
            );
    }
}

fn load_assets(mut commands: Commands, server: Option<Res<AssetServer>>) {
    let Some(server) = server else { return };
    commands.insert_resource(PreviewAssets {
        scenes: ModelKind::ANIMATED
            .into_iter()
            .map(|kind| {
                server.load(
                    GltfAssetLabel::Scene(0).from_asset(kind.path().expect("animated model path")),
                )
            })
            .collect(),
        gltfs: ModelKind::ANIMATED
            .into_iter()
            .map(|kind| server.load(kind.path().expect("animated model path")))
            .collect(),
    });
}

fn scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn((
        Name::new("actor preview camera"),
        PreviewCamera,
        Camera3d::default(),
        Transform::from_xyz(3.0, 3.25, 5.0).looking_at(Vec3::Y * 0.75, Vec3::Y),
    ));
    commands.spawn((
        Name::new("actor editor UI camera"),
        Camera2d,
        Camera {
            order: 1,
            clear_color: ClearColorConfig::None,
            ..default()
        },
        IsDefaultUiCamera,
        RenderLayers::layer(1),
    ));
    commands.spawn((
        Name::new("preview floor"),
        Mesh3d(meshes.add(Plane3d::default().mesh().size(140.0, 140.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.07, 0.085, 0.10),
            perceptual_roughness: 0.95,
            ..default()
        })),
        Transform::from_xyz(0.0, -0.08, 0.0),
    ));
    commands.spawn((
        Name::new("preview turntable plinth"),
        Mesh3d(meshes.add(Cylinder::new(1.35, 0.08))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.16, 0.19, 0.23),
            metallic: 0.15,
            perceptual_roughness: 0.65,
            ..default()
        })),
        Transform::from_xyz(0.0, -0.04, 0.0),
    ));
    commands.spawn((
        Name::new("preview key light"),
        DirectionalLight {
            illuminance: 10_000.0,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(3.5, 6.0, 4.5).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

fn runtime_sensors(specs: &[SensorSpec]) -> Sensors {
    specs
        .iter()
        .fold(Sensors::new(), |sensors, sensor| match *sensor {
            SensorSpec::Vision {
                range_mm,
                half_fov_deg,
            } => sensors.vision(range_mm, half_fov_deg),
            SensorSpec::Hearing { radius_mm } => sensors.hearing(radius_mm),
            SensorSpec::Lidar {
                range_mm,
                fov_deg,
                beams,
                noise_ppm,
            } => sensors.lidar(range_mm, fov_deg, beams, noise_ppm),
            SensorSpec::Camera {
                range_mm,
                half_fov_deg,
            } => sensors.camera(range_mm, half_fov_deg),
            SensorSpec::Radar {
                range_mm,
                half_fov_deg,
                range_res_mm,
                bearing_res_mdeg,
            } => sensors.radar(range_mm, half_fov_deg, range_res_mm, bearing_res_mdeg),
            SensorSpec::Reach { .. } => sensors,
        })
}

fn sync_model(
    mut commands: Commands,
    preview: Res<ModelPreview>,
    assets: Option<Res<PreviewAssets>>,
    robots: Option<Res<RobotAssets>>,
    roots: Query<Entity, With<PreviewActor>>,
    mut shown: Local<Option<ModelPreview>>,
) {
    let preview = preview.into_inner();
    if shown.as_ref() == Some(preview) {
        return;
    }
    let (Some(assets), Some(robots)) = (assets, robots) else {
        return;
    };
    for root in &roots {
        commands.entity(root).despawn();
    }
    *shown = Some(preview.clone());
    let Some(kind) = ModelKind::from_name(&preview.model) else {
        return;
    };
    let root = commands
        .spawn((
            Name::new(format!("preview {}", preview.model)),
            PreviewActor {
                model: preview.model.clone(),
            },
            Transform::default(),
            Visibility::default(),
        ))
        .id();
    if kind.is_robot() {
        robots::spawn(
            &mut commands,
            root,
            kind,
            Some(&runtime_sensors(&preview.sensors)),
            &robots,
        );
    } else {
        commands
            .spawn((
                ChildOf(root),
                PreviewModel(kind),
                WorldAssetRoot(assets.scenes[kind.index()].clone()),
                Transform::from_scale(Vec3::splat(kind.scale())),
            ))
            .observe(model_ready);
    }
}

fn model_ready(
    ready: On<WorldInstanceReady>,
    mut commands: Commands,
    models: Query<&PreviewModel>,
    children: Query<&Children>,
    names: Query<&Name>,
    players: Query<(), With<AnimationPlayer>>,
) {
    let Ok(model) = models.get(ready.entity) else {
        return;
    };
    for child in children.iter_descendants(ready.entity) {
        if players.contains(child) {
            commands.entity(child).insert(PreviewIdlePlayer(model.0));
        }
        if names
            .get(child)
            .is_ok_and(|name| crate::anim::HIDDEN_MESHES.contains(&name.as_str()))
        {
            commands.entity(child).insert(Visibility::Hidden);
        }
    }
}

fn idle_animation(
    mut commands: Commands,
    assets: Option<Res<PreviewAssets>>,
    gltfs: Res<Assets<Gltf>>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    mut animation: ResMut<PreviewAnimations>,
    mut players: Query<
        (Entity, &PreviewIdlePlayer, &mut AnimationPlayer),
        Without<AnimationGraphHandle>,
    >,
) {
    let Some(assets) = assets else {
        return;
    };
    animation
        .idle
        .resize_with(ModelKind::ANIMATED.len(), || None);
    for kind in ModelKind::ANIMATED {
        let slot = &mut animation.idle[kind.index()];
        if slot.is_some() {
            continue;
        }
        let Some(clip) = gltfs
            .get(&assets.gltfs[kind.index()])
            .and_then(|gltf| gltf.named_animations.get(kind.idle_clip()))
        else {
            continue;
        };
        let (graph, nodes) = AnimationGraph::from_clips([clip.clone()]);
        *slot = Some((graphs.add(graph), nodes[0]));
    }
    for (entity, target, mut player) in &mut players {
        let Some((graph, node)) = &animation.idle[target.0.index()] else {
            continue;
        };
        let mut transitions = AnimationTransitions::new();
        transitions
            .play(&mut player, *node, Duration::ZERO)
            .repeat();
        commands
            .entity(entity)
            .insert((AnimationGraphHandle(graph.clone()), transitions));
    }
}

fn turntable(
    time: Res<Time>,
    settings: Res<ModelPreviewSettings>,
    mut roots: Query<&mut Transform, With<PreviewActor>>,
    mut angle: Local<f32>,
) {
    if settings.spinning {
        *angle = (*angle + time.delta_secs() * 0.25).rem_euclid(2.0 * PI);
    }
    let rotation = Quat::from_rotation_y(*angle);
    for mut transform in &mut roots {
        if transform.rotation != rotation {
            transform.rotation = rotation;
        }
    }
}

fn sensor_range(sensor: &SensorSpec) -> f32 {
    let mm = match *sensor {
        SensorSpec::Vision { range_mm, .. }
        | SensorSpec::Lidar { range_mm, .. }
        | SensorSpec::Camera { range_mm, .. }
        | SensorSpec::Radar { range_mm, .. } => range_mm,
        SensorSpec::Hearing { radius_mm } | SensorSpec::Reach { radius_mm } => radius_mm,
    };
    mm.clamp(0, RANGE_MAX_MM) as f32 / 1000.0
}

fn frame_camera(
    layout: Res<ModelPreviewViewport>,
    preview: Res<ModelPreview>,
    settings: Res<ModelPreviewSettings>,
    windows: Query<&Window, With<PrimaryWindow>>,
    scroll: Option<Res<AccumulatedMouseScroll>>,
    mut cameras: Query<(&mut Camera, &mut Transform), With<PreviewCamera>>,
    mut framing: Local<PreviewFraming>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let Some((position, extent)) =
        layout.physical_rect(window.physical_size(), window.scale_factor())
    else {
        for (mut camera, _) in &mut cameras {
            camera.is_active = false;
        }
        return;
    };
    if framing.zoom <= 0.0 || framing.fitted != Some(settings.fit_sensors) {
        framing.zoom = 1.0;
        framing.fitted = Some(settings.fit_sensors);
    }
    if let (Some(cursor), Some(scroll)) = (window.cursor_position(), scroll) {
        let cursor = cursor * window.scale_factor();
        let inside = cursor.x >= position.x as f32
            && cursor.x < (position.x + extent.x) as f32
            && cursor.y >= position.y as f32
            && cursor.y < (position.y + extent.y) as f32;
        if inside {
            let amount = scroll.delta.y
                * if scroll.unit == MouseScrollUnit::Line {
                    0.10
                } else {
                    0.002
                };
            framing.zoom = (framing.zoom * (-amount).exp()).clamp(0.4, 4.0);
        }
    }
    let range = preview
        .sensors
        .iter()
        .map(sensor_range)
        .fold(1.5_f32, f32::max);
    let aspect_compensation = (extent.y as f32 / extent.x as f32).max(1.0);
    let distance = if settings.fit_sensors {
        range * 3.0
    } else {
        4.5
    } * aspect_compensation
        * framing.zoom;
    let focus = Vec3::Y * if settings.fit_sensors { 0.0 } else { 0.75 };
    for (mut camera, mut transform) in &mut cameras {
        if !camera.is_active {
            camera.is_active = true;
        }
        if camera
            .viewport
            .as_ref()
            .is_none_or(|v| v.physical_position != position || v.physical_size != extent)
        {
            camera.viewport = Some(Viewport {
                physical_position: position,
                physical_size: extent,
                ..default()
            });
        }
        let framed =
            Transform::from_translation(focus + Vec3::new(3.0, 2.5, 5.0).normalize() * distance)
                .looking_at(focus, Vec3::Y);
        if *transform != framed {
            *transform = framed;
        }
    }
}

fn draw_ranges(
    preview: Res<ModelPreview>,
    settings: Res<ModelPreviewSettings>,
    roots: Query<&Transform, With<PreviewActor>>,
    mut gizmos: Gizmos,
) {
    let Ok(root) = roots.single() else {
        return;
    };
    let rotation = root.rotation;
    let grid_extent = if settings.fit_sensors { 50 } else { 5 };
    let step = if settings.fit_sensors { 5 } else { 1 };
    let grid_color = Color::srgb(0.20, 0.23, 0.27);
    for coordinate in (-grid_extent..=grid_extent).step_by(step) {
        let c = coordinate as f32;
        let edge = grid_extent as f32;
        gizmos.line(
            Vec3::new(-edge, -0.065, c),
            Vec3::new(edge, -0.065, c),
            grid_color,
        );
        gizmos.line(
            Vec3::new(c, -0.065, -edge),
            Vec3::new(c, -0.065, edge),
            grid_color,
        );
    }
    if !settings.show_sensors {
        return;
    }
    for sensor in &preview.sensors {
        let range = sensor_range(sensor);
        if range <= 0.0 {
            continue;
        }
        let (half, color) = match *sensor {
            SensorSpec::Vision { half_fov_deg, .. } => (
                f32::from(half_fov_deg.min(180)),
                Color::srgb(0.45, 0.73, 1.0),
            ),
            SensorSpec::Camera { half_fov_deg, .. } => (
                f32::from(half_fov_deg.min(180)),
                Color::srgb(0.25, 0.88, 0.88),
            ),
            SensorSpec::Radar { half_fov_deg, .. } => (
                f32::from(half_fov_deg.min(180)),
                Color::srgb(0.93, 0.47, 0.47),
            ),
            SensorSpec::Lidar { fov_deg, .. } => (
                f32::from(fov_deg.min(360)) / 2.0,
                Color::srgb(0.98, 0.71, 0.30),
            ),
            SensorSpec::Hearing { .. } => (180.0, Color::srgb(0.53, 0.69, 0.54)),
            SensorSpec::Reach { .. } => (180.0, Color::srgb(0.88, 0.89, 0.91)),
        };
        let origin = Vec3::Y * 0.035;
        if half >= 180.0 {
            gizmos
                .circle(
                    Isometry3d::new(origin, Quat::from_rotation_x(FRAC_PI_2)),
                    range,
                    color,
                )
                .resolution(96);
        } else {
            let half = half * PI / 180.0;
            let point = |angle: f32| {
                origin + rotation * Vec3::new(angle.sin() * range, 0.0, angle.cos() * range)
            };
            gizmos.linestrip(
                (0..=64).map(|i| point(-half + 2.0 * half * i as f32 / 64.0)),
                color,
            );
            gizmos.line(origin, point(-half), color);
            gizmos.line(origin, point(half), color);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            AssetPlugin::default(),
            bevy::gizmos::GizmoPlugin,
        ))
        .init_asset::<Mesh>()
        .init_asset::<StandardMaterial>()
        .init_asset::<Image>()
        .init_asset::<Gltf>()
        .init_asset::<WorldAsset>()
        .init_asset::<AnimationClip>()
        .init_asset::<AnimationGraph>()
        .init_asset::<bevy::mesh::skinning::SkinnedMeshInverseBindposes>()
        .add_plugins(ModelPreviewPlugin);
        app
    }

    #[test]
    fn model_picker_changes_the_preview() {
        let mut app = app();
        app.update();
        app.world_mut().resource_mut::<ModelPreview>().model = "rover".into();
        app.update();
        let mut roots = app.world_mut().query::<(Entity, &PreviewActor)>();
        let (rover, root) = roots.single(app.world()).unwrap();
        assert_eq!(root.model, "rover");
        let mut parts = app.world_mut().query::<(&ChildOf, &Name)>();
        assert_eq!(
            parts
                .iter(app.world())
                .filter(|(parent, name)| parent.parent() == rover && name.as_str() == "robot wheel")
                .count(),
            4
        );
        app.world_mut().resource_mut::<ModelPreview>().model = "knight".into();
        app.update();
        assert!(
            app.world().get_entity(rover).is_err(),
            "the previous model and its linked parts are removed"
        );
        let mut roots = app.world_mut().query::<&PreviewActor>();
        assert_eq!(roots.single(app.world()).unwrap().model, "knight");
    }

    #[test]
    fn viewport_stays_inside_resized_hidpi_windows() {
        let layout = ModelPreviewViewport::default();
        assert_eq!(
            layout.physical_rect(UVec2::new(1600, 960), 1.0),
            Some((UVec2::new(260, 64), UVec2::new(880, 832)))
        );
        assert_eq!(
            layout.physical_rect(UVec2::new(3200, 1920), 2.0),
            Some((UVec2::new(520, 128), UVec2::new(1760, 1664)))
        );
        assert_eq!(layout.physical_rect(UVec2::new(600, 100), 1.0), None);
        assert_eq!(layout.physical_rect(UVec2::ZERO, 1.0), None);
    }

    #[test]
    fn sensors_change_the_robot_parts_and_ranges_keep_real_metres() {
        let mut app = app();
        *app.world_mut().resource_mut::<ModelPreview>() = ModelPreview {
            model: "rover".into(),
            sensors: vec![SensorSpec::Camera {
                range_mm: 15_000,
                half_fov_deg: 35,
            }],
        };
        app.update();
        let mut names = app.world_mut().query::<&Name>();
        assert_eq!(
            names
                .iter(app.world())
                .filter(|name| name.as_str() == "sensor dome")
                .count(),
            1
        );
        app.world_mut()
            .resource_mut::<ModelPreview>()
            .sensors
            .push(SensorSpec::Lidar {
                range_mm: 20_000,
                fov_deg: 360,
                beams: 360,
                noise_ppm: 10_000,
            });
        app.update();
        assert_eq!(
            names
                .iter(app.world())
                .filter(|name| name.as_str() == "sensor dome")
                .count(),
            2
        );
        assert_eq!(sensor_range(&SensorSpec::Reach { radius_mm: 1500 }), 1.5);
        assert_eq!(
            sensor_range(&SensorSpec::Hearing { radius_mm: 10_000 }),
            10.0
        );
        assert!(
            app.world_mut()
                .query::<&npc_framework::prelude::SimId>()
                .iter(app.world())
                .next()
                .is_none()
        );
    }
}
