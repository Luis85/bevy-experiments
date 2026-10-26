use crate::{
    geometry::{MeshBuilder, feature_meshes, terrain_mesh},
    sandbox::SandboxRuntime,
    simulation::{GeoActor, GeoAnchor, RenderQueue, ResidentChunks, WorldRuntime, WorldSet},
};
use bevy::{light::CascadeShadowConfigBuilder, prelude::*};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Mutex, mpsc},
    time::Instant,
};
use terra_core::{
    ActorKind, GeoPosition, Layer, LocalFrame, Season, TileKey, season, solar_position,
};
use terra_stream::StreamEvent;

pub struct TerraRenderPlugin;
impl Plugin for TerraRenderPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RenderQueue>()
            .init_resource::<RenderState>()
            .init_resource::<RenderTelemetry>()
            .init_resource::<MeshTasks>()
            .init_resource::<crate::politics::AtlasRenderState>()
            .add_systems(Startup, setup_lighting)
            .add_systems(
                Update,
                (
                    render_chunks,
                    render_sandbox,
                    sync_anchors,
                    animate_environment,
                    crate::politics::render_atlas,
                )
                    .chain()
                    .in_set(WorldSet::Render),
            );
    }
}

#[derive(Component)]
struct Sun;
#[derive(Component)]
struct ChunkVisual;
#[derive(Component)]
struct SandboxVisual;
#[derive(Component)]
struct CropVisual(u64);
#[derive(Component)]
pub struct LayerVisual(pub Layer);

#[derive(Resource, Default)]
pub struct RenderTelemetry {
    pub integration_ms: f64,
    pub meshes: usize,
    pub chunks: usize,
    pub pending_meshes: usize,
    pub building_meshes: usize,
}
#[derive(Resource, Default)]
struct RenderState {
    chunks: BTreeMap<TileKey, Vec<(Entity, Handle<Mesh>)>>,
    materials: BTreeMap<Layer, Handle<StandardMaterial>>,
    sandbox_entities: Vec<(Entity, Handle<Mesh>)>,
    revision: u64,
    sandbox_fingerprint: u64,
    removed: BTreeSet<String>,
    initialized: bool,
    sandbox_focus: Option<TileKey>,
    authored_visible: BTreeSet<String>,
}

fn setup_lighting(mut commands: Commands) {
    commands.insert_resource(GlobalAmbientLight {
        brightness: 260.0,
        ..default()
    });
    commands.spawn((
        Sun,
        DirectionalLight {
            illuminance: 18000.0,
            shadow_maps_enabled: true,
            ..default()
        },
        CascadeShadowConfigBuilder {
            first_cascade_far_bound: 160.,
            maximum_distance: 3500.,
            ..default()
        }
        .build(),
        Transform::from_xyz(-100., 700., 300.).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

fn material(
    layer: Layer,
    state: &mut RenderState,
    materials: &mut Assets<StandardMaterial>,
) -> Handle<StandardMaterial> {
    state
        .materials
        .entry(layer)
        .or_insert_with(|| {
            materials.add(StandardMaterial {
                base_color: Color::WHITE,
                cull_mode: None,
                perceptual_roughness: if layer == Layer::Water { 0.22 } else { 0.93 },
                metallic: if layer == Layer::Water { 0.1 } else { 0.0 },
                ..default()
            })
        })
        .clone()
}

fn clear_chunk(
    key: TileKey,
    state: &mut RenderState,
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
) {
    if let Some(old) = state.chunks.remove(&key) {
        for (entity, handle) in old {
            commands.entity(entity).despawn();
            meshes.remove(handle.id());
        }
    }
}

#[derive(Resource, Default)]
struct MeshTasks {
    pending: BTreeSet<TileKey>,
    jobs: BTreeMap<TileKey, (u64, Mutex<mpsc::Receiver<BTreeMap<Layer, Mesh>>>)>,
    revision: u64,
}

fn render_chunks(
    mut commands: Commands,
    mut state: ResMut<RenderState>,
    mut queue: ResMut<RenderQueue>,
    resident: Res<ResidentChunks>,
    runtime: Res<WorldRuntime>,
    sandbox: Res<SandboxRuntime>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut stats: ResMut<RenderTelemetry>,
    mut tasks: ResMut<MeshTasks>,
) {
    let start = Instant::now();
    let mut removed = sandbox.state.feature_edits.clone();
    removed.extend(
        sandbox
            .state
            .resource_nodes
            .values()
            .filter(|n| n.remaining == 0)
            .filter_map(|n| n.source_feature.clone()),
    );
    if removed != state.removed {
        state.removed = removed;
        tasks.revision += 1;
        tasks.pending.extend(resident.chunks.keys().copied());
    }
    for event in queue.0.drain(..) {
        if let StreamEvent::Loaded(chunk) = event {
            tasks.pending.insert(chunk.key);
        }
    }
    tasks.pending.retain(|k| resident.chunks.contains_key(k));
    let obsolete: Vec<_> = state
        .chunks
        .keys()
        .filter(|k| !resident.chunks.contains_key(k))
        .copied()
        .collect();
    for key in obsolete {
        clear_chunk(key, &mut state, &mut commands, &mut meshes);
    }
    let keys: Vec<_> = tasks.jobs.keys().copied().collect();
    let mut integrated = 0;
    for key in keys {
        if integrated >= runtime.config.integrate_per_frame {
            break;
        }
        let Some((revision, receiver)) = tasks.jobs.get(&key) else {
            continue;
        };
        let revision = *revision;
        let result = receiver.lock().ok().and_then(|r| match r.try_recv() {
            Ok(v) => Some(Some(v)),
            Err(mpsc::TryRecvError::Disconnected) => Some(None),
            _ => None,
        });
        let Some(result) = result else {
            continue;
        };
        tasks.jobs.remove(&key);
        if !resident.chunks.contains_key(&key) {
            continue;
        }
        if revision != tasks.revision {
            tasks.pending.insert(key);
            continue;
        }
        let Some(layers) = result else {
            continue;
        };
        clear_chunk(key, &mut state, &mut commands, &mut meshes);
        let mut entities = Vec::new();
        for (layer, mesh) in layers {
            let handle = meshes.add(mesh);
            let mat = material(layer, &mut state, &mut materials);
            let entity = commands
                .spawn((
                    ChunkVisual,
                    LayerVisual(layer),
                    GeoAnchor(key.center()),
                    Mesh3d(handle.clone()),
                    MeshMaterial3d(mat),
                    geo_transform(key.center(), &runtime.frame),
                    if runtime.config.layers.contains(layer) {
                        Visibility::Visible
                    } else {
                        Visibility::Hidden
                    },
                ))
                .id();
            entities.push((entity, handle));
        }
        state.chunks.insert(key, entities);
        integrated += 1;
    }
    while tasks.jobs.len() < 2 {
        let key = tasks
            .pending
            .iter()
            .filter(|k| !tasks.jobs.contains_key(k))
            .min_by(|a, b| {
                a.center()
                    .surface_distance_m(runtime.focus)
                    .total_cmp(&b.center().surface_distance_m(runtime.focus))
            })
            .copied();
        let Some(key) = key else {
            break;
        };
        tasks.pending.remove(&key);
        let Some(chunk) = resident.chunks.get(&key).cloned() else {
            continue;
        };
        let removed = state.removed.clone();
        let budget = runtime.config.max_features_per_chunk;
        let (tx, rx) = mpsc::sync_channel(1);
        let revision = tasks.revision;
        std::thread::spawn(move || {
            let mut layers = feature_meshes(&chunk, budget, &removed);
            if let Some(mesh) = terrain_mesh(&chunk) {
                layers.insert(Layer::Terrain, mesh);
            }
            let _ = tx.send(layers);
        });
        tasks.jobs.insert(key, (revision, Mutex::new(rx)));
    }
    stats.integration_ms = start.elapsed().as_secs_f64() * 1000.;
    stats.chunks = state.chunks.len();
    stats.meshes =
        state.chunks.values().map(Vec::len).sum::<usize>() + state.sandbox_entities.len();
    stats.pending_meshes = tasks.pending.len();
    stats.building_meshes = tasks.jobs.len();
}

pub fn geo_transform(position: GeoPosition, frame: &LocalFrame) -> Transform {
    let p = frame.to_local(position);
    let own = LocalFrame::new(position);
    let axis = |offset: [f64; 3]| {
        let at = own.to_geo(offset).unwrap_or(position);
        let q = frame.to_local(at);
        Vec3::new(
            (q[0] - p[0]) as f32,
            (q[1] - p[1]) as f32,
            (q[2] - p[2]) as f32,
        )
        .normalize_or_zero()
    };
    let rotation = Quat::from_mat3(&Mat3::from_cols(
        axis([1., 0., 0.]),
        axis([0., 1., 0.]),
        axis([0., 0., 1.]),
    ))
    .normalize();
    Transform::from_translation(Vec3::from_array(p.map(|x| x as f32))).with_rotation(rotation)
}

fn sync_anchors(
    runtime: Res<WorldRuntime>,
    politics: Res<crate::PoliticalWorld>,
    mut anchors: Query<(&GeoAnchor, &mut Transform), Without<GeoActor>>,
    mut actors: Query<(&GeoActor, &mut Transform)>,
    mut layers: Query<(&LayerVisual, &mut Visibility)>,
) {
    for (anchor, mut transform) in &mut anchors {
        let scale = transform.scale;
        *transform = geo_transform(anchor.0, &runtime.frame);
        transform.scale = scale;
    }
    for (actor, mut transform) in &mut actors {
        let scale = transform.scale;
        *transform = geo_transform(actor.0.position, &runtime.frame);
        transform.rotate_local_y(-(actor.0.heading_deg as f32).to_radians());
        transform.scale = scale;
    }
    for (layer, mut visibility) in &mut layers {
        *visibility = if !politics.atlas && runtime.config.layers.contains(layer.0) {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
}

/// Hash only geometry/visibility inputs. Inventory, growth clocks and repeated
/// network snapshots must not cause all houses and crops to be reallocated.
fn sandbox_visual_fingerprint(sandbox: &SandboxRuntime, focus: GeoPosition) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    let point = |p: GeoPosition, h: &mut std::collections::hash_map::DefaultHasher| {
        p.latitude.to_bits().hash(h);
        p.longitude.to_bits().hash(h);
        p.altitude_m.to_bits().hash(h);
    };
    for p in sandbox
        .state
        .plots
        .values()
        .filter(|p| p.center.surface_distance_m(focus) < 7000.)
        .take(128)
    {
        0u8.hash(&mut h);
        p.id.hash(&mut h);
        point(p.center, &mut h);
        p.radius_m.to_bits().hash(&mut h);
        sandbox
            .state
            .may_build(sandbox.player_id, p.faction_owner, p.owner)
            .hash(&mut h);
        sandbox
            .state
            .may_use(sandbox.player_id, p.faction_owner, p.owner)
            .hash(&mut h);
    }
    for s in sandbox
        .state
        .structures
        .values()
        .filter(|s| s.position.surface_distance_m(focus) < 7000.)
        .take(256)
    {
        1u8.hash(&mut h);
        s.id.hash(&mut h);
        point(s.position, &mut h);
        s.housing.hash(&mut h);
        s.workbench.hash(&mut h);
    }
    for f in sandbox
        .state
        .farms
        .values()
        .filter(|f| f.position.surface_distance_m(focus) < 7000.)
        .take(256)
    {
        2u8.hash(&mut h);
        f.id.hash(&mut h);
        point(f.position, &mut h);
    }
    for n in sandbox
        .state
        .resource_nodes
        .values()
        .filter(|n| {
            n.source_feature.is_none()
                && n.remaining > 0
                && n.position.surface_distance_m(focus) < 7000.
        })
        .take(128)
    {
        3u8.hash(&mut h);
        n.id.hash(&mut h);
        point(n.position, &mut h);
        n.item.hash(&mut h);
    }
    h.finish()
}

fn render_sandbox(
    mut commands: Commands,
    mut state: ResMut<RenderState>,
    runtime: Res<WorldRuntime>,
    sandbox: Res<SandboxRuntime>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut crops: Query<(&CropVisual, &mut Transform)>,
) {
    for (crop, mut transform) in &mut crops {
        if let Some(farm) = sandbox.state.farms.get(&crop.0) {
            transform.scale.y = 0.15 + farm.progress() * 1.4;
        }
    }
    let focus = TileKey::at(runtime.focus, 12).ok();
    let visible = sandbox
        .state
        .resource_nodes
        .values()
        .filter(|n| n.source_feature.is_none() && n.remaining > 0)
        .map(|n| n.id.clone())
        .collect::<BTreeSet<_>>();
    if state.initialized
        && state.revision == sandbox.revision
        && state.sandbox_focus == focus
        && state.authored_visible == visible
    {
        return;
    }
    let fingerprint = sandbox_visual_fingerprint(&sandbox, runtime.focus);
    state.sandbox_focus = focus;
    state.authored_visible = visible;
    state.revision = sandbox.revision;
    if state.initialized && state.sandbox_fingerprint == fingerprint {
        return;
    }
    state.sandbox_fingerprint = fingerprint;
    state.initialized = true;
    for (entity, mesh) in state.sandbox_entities.drain(..) {
        commands.entity(entity).despawn();
        meshes.remove(mesh.id());
    }
    let mat = material(Layer::Buildings, &mut state, &mut materials);
    let mut add = |builder: MeshBuilder, position: GeoPosition, crop: Option<u64>, layer: Layer| {
        if let Some(mesh) = builder.finish() {
            let handle = meshes.add(mesh);
            let mut entity = commands.spawn((
                SandboxVisual,
                LayerVisual(layer),
                GeoAnchor(position),
                Mesh3d(handle.clone()),
                MeshMaterial3d(mat.clone()),
                geo_transform(position, &runtime.frame),
            ));
            if let Some(id) = crop {
                entity.insert(CropVisual(id));
            }
            state.sandbox_entities.push((entity.id(), handle));
        }
    };
    for plot in sandbox
        .state
        .plots
        .values()
        .filter(|p| p.center.surface_distance_m(runtime.focus) < 7000.)
        .take(128)
    {
        let mut b = MeshBuilder::default();
        let color = if sandbox
            .state
            .may_build(sandbox.player_id, plot.faction_owner, plot.owner)
        {
            [0.35, 0.85, 0.65, 1.]
        } else if sandbox
            .state
            .may_use(sandbox.player_id, plot.faction_owner, plot.owner)
        {
            [0.50, 0.68, 0.95, 1.]
        } else {
            [0.95, 0.61, 0.28, 1.]
        };
        let radius = plot.radius_m as f32;
        for i in 0..64 {
            let a = i as f32 * std::f32::consts::TAU / 64.;
            let c = (i + 1) as f32 * std::f32::consts::TAU / 64.;
            b.quad(
                Vec3::new(a.cos() * (radius - 0.3), 0.35, a.sin() * (radius - 0.3)),
                Vec3::new(c.cos() * (radius - 0.3), 0.35, c.sin() * (radius - 0.3)),
                Vec3::new(c.cos() * (radius + 0.3), 0.35, c.sin() * (radius + 0.3)),
                Vec3::new(a.cos() * (radius + 0.3), 0.35, a.sin() * (radius + 0.3)),
                color,
            );
        }
        add(b, plot.center, None, Layer::Buildings);
    }
    for structure in sandbox
        .state
        .structures
        .values()
        .filter(|s| s.position.surface_distance_m(runtime.focus) < 7000.)
        .take(256)
    {
        let mut b = MeshBuilder::default();
        if structure.housing {
            b.cuboid(
                Vec3::new(0., 2., 0.),
                Vec3::new(7., 4., 6.),
                [0.63, 0.49, 0.30, 1.],
            );
            b.triangle(
                Vec3::new(-4., 4., -3.5),
                Vec3::new(4., 4., -3.5),
                Vec3::new(0., 6.5, -3.5),
                [0.36, 0.23, 0.17, 1.],
            );
            b.triangle(
                Vec3::new(4., 4., 3.5),
                Vec3::new(-4., 4., 3.5),
                Vec3::new(0., 6.5, 3.5),
                [0.36, 0.23, 0.17, 1.],
            );
            b.quad(
                Vec3::new(-4., 4., -3.5),
                Vec3::new(0., 6.5, -3.5),
                Vec3::new(0., 6.5, 3.5),
                Vec3::new(-4., 4., 3.5),
                [0.32, 0.23, 0.18, 1.],
            );
            b.quad(
                Vec3::new(0., 6.5, -3.5),
                Vec3::new(4., 4., -3.5),
                Vec3::new(4., 4., 3.5),
                Vec3::new(0., 6.5, 3.5),
                [0.40, 0.27, 0.20, 1.],
            );
            b.cuboid(
                Vec3::new(0., 1.2, 3.03),
                Vec3::new(1.4, 2.4, 0.1),
                [0.22, 0.16, 0.11, 1.],
            );
        } else if structure.workbench {
            b.cuboid(
                Vec3::new(0., 1., 0.),
                Vec3::new(2., 0.2, 1.2),
                [0.48, 0.33, 0.20, 1.],
            );
            for x in [-0.8, 0.8] {
                for z in [-0.4, 0.4] {
                    b.cuboid(
                        Vec3::new(x, 0.5, z),
                        Vec3::new(0.15, 1., 0.15),
                        [0.32, 0.23, 0.15, 1.],
                    );
                }
            }
        } else {
            for x in [-0.7, 0.7] {
                b.cuboid(
                    Vec3::new(x, 0.8, 0.),
                    Vec3::new(0.18, 1.6, 0.18),
                    [0.51, 0.36, 0.22, 1.],
                );
            }
            for y in [0.5, 1.2] {
                b.cuboid(
                    Vec3::new(0., y, 0.),
                    Vec3::new(1.6, 0.13, 0.13),
                    [0.55, 0.40, 0.25, 1.],
                );
            }
        }
        add(b, structure.position, None, Layer::Buildings);
    }
    for farm in sandbox
        .state
        .farms
        .values()
        .filter(|f| f.position.surface_distance_m(runtime.focus) < 7000.)
        .take(256)
    {
        let mut b = MeshBuilder::default();
        b.cuboid(
            Vec3::new(0., 0.02, 0.),
            Vec3::new(2., 0.04, 2.),
            [0.29, 0.20, 0.11, 1.],
        );
        for x in 0..5 {
            for z in 0..5 {
                b.cuboid(
                    Vec3::new(x as f32 * 0.35 - 0.7, 0.4, z as f32 * 0.35 - 0.7),
                    Vec3::new(0.08, 0.8, 0.08),
                    [0.73, 0.65, 0.25, 1.],
                );
            }
        }
        add(b, farm.position, Some(farm.id), Layer::Vegetation);
    }
    for resource in sandbox
        .state
        .resource_nodes
        .values()
        .filter(|n| {
            n.source_feature.is_none()
                && n.remaining > 0
                && n.position.surface_distance_m(runtime.focus) < 7000.
        })
        .take(128)
    {
        let mut b = MeshBuilder::default();
        if resource.item == "wood" {
            b.tree(Vec3::ZERO, 5.0, 7);
        } else {
            b.cuboid(
                Vec3::new(0., 0.7, 0.),
                Vec3::new(2., 1.4, 1.7),
                [0.48, 0.51, 0.50, 1.],
            );
        }
        add(
            b,
            resource.position,
            None,
            if resource.item == "wood" {
                Layer::Vegetation
            } else {
                Layer::Terrain
            },
        );
    }
}

fn animate_environment(
    runtime: Res<WorldRuntime>,
    mut sun: Query<(&mut DirectionalLight, &mut Transform), With<Sun>>,
    mut ambient: ResMut<GlobalAmbientLight>,
    mut clear: ResMut<ClearColor>,
    mut fogs: Query<&mut DistanceFog>,
    state: Res<RenderState>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let solar = if runtime.config.day_night {
        solar_position(&runtime.focus, &runtime.clock)
    } else {
        terra_core::SolarPosition {
            elevation_deg: 55.,
            azimuth_deg: 170.,
            daylight: 1.,
        }
    };
    let weather = runtime.effective_weather();
    let daylight = if runtime.config.day_night {
        solar.daylight as f32
    } else {
        1.0
    };
    if let Ok((mut light, mut transform)) = sun.single_mut() {
        let d = Vec3::from_array(solar.direction_local().map(|x| x as f32));
        *transform = Transform::from_translation(d * 1000.).looking_at(
            Vec3::ZERO,
            if d.y.abs() > 0.999 { Vec3::Z } else { Vec3::Y },
        );
        light.illuminance = 18000. * daylight * (1. - weather.cloud_cover * 0.65);
        light.color = Color::srgb(1., 0.86 + daylight * 0.12, 0.72 + daylight * 0.25);
    }
    ambient.brightness = 40. + daylight * 240.;
    let cloud = weather.cloud_cover;
    let sky = Color::srgb(
        0.02 + daylight * (0.39 + cloud * 0.10),
        0.035 + daylight * (0.59 - cloud * 0.04),
        0.06 + daylight * (0.65 - cloud * 0.08),
    );
    clear.0 = sky;
    for mut fog in &mut fogs {
        fog.color = sky;
        fog.falloff = FogFalloff::Linear {
            start: 1200.0 / (1. + weather.precipitation_mm_h * 0.2),
            end: 4800.0 / (1. + weather.precipitation_mm_h * 0.15),
        };
    }
    if let Some(handle) = state.materials.get(&Layer::Vegetation) {
        if let Some(mut m) = materials.get_mut(handle) {
            m.base_color = if runtime.config.seasons {
                match season(&runtime.focus, &runtime.clock) {
                    Season::Spring => Color::srgb(0.85, 1., 0.82),
                    Season::Summer => Color::WHITE,
                    Season::Autumn => Color::srgb(1., 0.73, 0.36),
                    Season::Winter => Color::srgb(0.86, 0.89, 0.88),
                }
            } else {
                Color::WHITE
            };
        }
    }
}

pub fn actor_mesh(kind: ActorKind) -> Mesh {
    let mut b = MeshBuilder::default();
    match kind {
        ActorKind::Human => {
            b.cuboid(
                Vec3::Y * 0.75,
                Vec3::new(0.55, 1.5, 0.4),
                [0.86, 0.63, 0.28, 1.],
            );
            b.cuboid(Vec3::Y * 1.7, Vec3::splat(0.4), [0.85, 0.73, 0.58, 1.]);
        }
        ActorKind::Animal => {
            let fur = [0.60, 0.40, 0.25, 1.];
            b.cuboid(Vec3::new(0., 0.75, 0.), Vec3::new(0.65, 0.65, 1.3), fur);
            b.cuboid(Vec3::new(0., 1.05, -0.7), Vec3::new(0.42, 0.45, 0.5), fur);
            for x in [-0.22, 0.22] {
                for z in [-0.46, 0.46] {
                    b.cuboid(
                        Vec3::new(x, 0.25, z),
                        Vec3::new(0.13, 0.5, 0.13),
                        [0.35, 0.25, 0.18, 1.],
                    );
                }
            }
        }
        ActorKind::Creature => {
            b.cuboid(
                Vec3::new(0., 1.2, 0.),
                Vec3::new(1.2, 1.6, 0.85),
                [0.38, 0.58, 0.40, 1.],
            );
            b.cuboid(
                Vec3::new(0., 2.15, -0.1),
                Vec3::new(0.8, 0.65, 0.7),
                [0.45, 0.65, 0.40, 1.],
            );
            for x in [-0.38, 0.38] {
                b.cuboid(
                    Vec3::new(x, 0.28, 0.),
                    Vec3::new(0.35, 0.56, 0.48),
                    [0.30, 0.42, 0.28, 1.],
                );
            }
            for x in [-0.3, 0.3] {
                b.triangle(
                    Vec3::new(x - 0.12, 2.45, 0.),
                    Vec3::new(x + 0.12, 2.45, 0.),
                    Vec3::new(x, 2.9, 0.),
                    [0.78, 0.72, 0.55, 1.],
                );
            }
        }
        ActorKind::Vehicle => {
            for x in [-1.0, 1.0] {
                for z in [-1.3, 1.3] {
                    b.cuboid(
                        Vec3::new(x, 0.35, z),
                        Vec3::new(0.4, 0.7, 0.7),
                        [0.12, 0.14, 0.14, 1.],
                    );
                }
            }
            b.cuboid(
                Vec3::Y * 0.8,
                Vec3::new(2., 1.1, 4.),
                [0.75, 0.28, 0.16, 1.],
            );
            b.cuboid(
                Vec3::new(0., 1.6, -0.2),
                Vec3::new(1.8, 0.8, 2.),
                [0.25, 0.36, 0.38, 1.],
            );
        }
        ActorKind::Ship => {
            b.cuboid(Vec3::Y, Vec3::new(5., 2., 14.), [0.79, 0.80, 0.71, 1.]);
            b.cuboid(
                Vec3::new(0., 3., 2.),
                Vec3::new(3., 3., 5.),
                [0.38, 0.53, 0.58, 1.],
            );
        }
        ActorKind::Helicopter => {
            b.cuboid(Vec3::ZERO, Vec3::new(2.4, 2.5, 5.), [0.82, 0.50, 0.18, 1.]);
            b.cuboid(
                Vec3::new(0., 1.8, 0.),
                Vec3::new(13., 0.12, 0.6),
                [0.22, 0.24, 0.23, 1.],
            );
            b.cuboid(
                Vec3::new(0., 0.4, 4.),
                Vec3::new(0.4, 0.6, 5.),
                [0.63, 0.36, 0.15, 1.],
            );
        }
        ActorKind::Airplane => {
            b.cuboid(Vec3::ZERO, Vec3::new(1.6, 1.5, 10.), [0.87, 0.87, 0.76, 1.]);
            b.cuboid(
                Vec3::new(0., 0., -0.6),
                Vec3::new(16., 0.2, 2.2),
                [0.82, 0.61, 0.22, 1.],
            );
            b.cuboid(
                Vec3::new(0., 0.5, 4.),
                Vec3::new(5., 0.15, 1.),
                [0.82, 0.61, 0.22, 1.],
            );
        }
        ActorKind::Balloon => {
            b.cuboid(Vec3::ZERO, Vec3::new(2., 1.5, 2.), [0.46, 0.30, 0.17, 1.]);
            // A faceted envelope keeps the reference renderer asset-free.
            for i in 0..12 {
                let a = i as f32 * std::f32::consts::TAU / 12.;
                let c = (i + 1) as f32 * std::f32::consts::TAU / 12.;
                let pa = Vec3::new(a.cos() * 5., 8., a.sin() * 5.);
                let pc = Vec3::new(c.cos() * 5., 8., c.sin() * 5.);
                let color = if i % 2 == 0 {
                    [0.90, 0.57, 0.20, 1.]
                } else {
                    [0.82, 0.27, 0.18, 1.]
                };
                b.triangle(pa, Vec3::Y * 14., pc, color);
                b.triangle(pc, Vec3::Y * 2., pa, color);
            }
        }
    }
    b.finish().expect("actor mesh contains faces")
}

#[cfg(test)]
mod cache_tests {
    use super::*;
    #[test]
    fn inventory_and_resource_quantity_do_not_rebuild_visible_geometry() {
        let focus = GeoPosition::new(48.137, 11.575, 0.).unwrap();
        let mut sandbox = SandboxRuntime::new(focus);
        sandbox
            .state
            .seed_resource("authored:cache-test".into(), focus, "wood", 10, 0.)
            .unwrap();
        let initial = sandbox_visual_fingerprint(&sandbox, focus);
        sandbox
            .state
            .players
            .get_mut(&1)
            .unwrap()
            .inventory
            .insert("wood".into(), 99);
        sandbox
            .state
            .resource_nodes
            .get_mut("authored:cache-test")
            .unwrap()
            .remaining = 9;
        assert_eq!(initial, sandbox_visual_fingerprint(&sandbox, focus));
        sandbox
            .state
            .resource_nodes
            .get_mut("authored:cache-test")
            .unwrap()
            .remaining = 0;
        assert_ne!(initial, sandbox_visual_fingerprint(&sandbox, focus));
    }
}
