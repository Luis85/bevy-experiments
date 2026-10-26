use crate::{
    geometry::{MeshBuilder, feature_meshes_lod, terrain_mesh},
    sandbox::SandboxRuntime,
    simulation::{GeoActor, GeoAnchor, RenderQueue, ResidentChunks, WorldRuntime, WorldSet},
};
use bevy::{
    asset::RenderAssetUsages,
    image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor},
    light::CascadeShadowConfigBuilder,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};
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
    /// Resident source chunks whose terrain/feature meshes are integrated.
    pub chunks: usize,
    pub pending_meshes: usize,
    pub building_meshes: usize,
    /// Integrated source geometry, including terrain; excludes living/sandbox visuals.
    pub vertices: usize,
}
/// Read-only renderer readiness and source geometry budget diagnostics.
pub type RenderDiagnostics = RenderTelemetry;
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
    terrain_detail: Option<Handle<Image>>,
    terrain_normal: Option<Handle<Image>>,
    detail_levels: BTreeMap<TileKey, u8>,
    source_vertices: BTreeMap<TileKey, usize>,
}

fn setup_lighting(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut state: ResMut<RenderState>,
) {
    let (detail, normal) = terrain_detail_images();
    state.terrain_detail = Some(images.add(detail));
    state.terrain_normal = Some(images.add(normal));
    commands.insert_resource(GlobalAmbientLight {
        brightness: 190.0,
        ..default()
    });
    commands.spawn((
        Sun,
        DirectionalLight {
            illuminance: 24000.0,
            shadow_maps_enabled: true,
            ..default()
        },
        CascadeShadowConfigBuilder {
            first_cascade_far_bound: 70.,
            maximum_distance: 2200.,
            ..default()
        }
        .build(),
        Transform::from_xyz(-100., 700., 300.).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

/// Small authored repeating material maps, generated once. They add ground-scale
/// grain under the source height field; they are never presented as aerial imagery.
fn terrain_detail_images() -> (Image, Image) {
    const SIDE: usize = 128;
    let mut height = vec![0f32; SIDE * SIDE];
    for y in 0..SIDE {
        for x in 0..SIDE {
            let mut n = (x as u32).wrapping_mul(374761393) ^ (y as u32).wrapping_mul(668265263);
            n = (n ^ (n >> 13)).wrapping_mul(1274126177);
            let grit = (n & 1023) as f32 / 1023.;
            let organic = ((x as f32 * std::f32::consts::TAU / SIDE as f32 * 3.).sin()
                * (y as f32 * std::f32::consts::TAU / SIDE as f32 * 5.).cos()
                + 1.)
                * 0.5;
            height[y * SIDE + x] = grit * 0.65 + organic * 0.35;
        }
    }
    let mut albedo = Vec::with_capacity(SIDE * SIDE * 4);
    let mut normals = Vec::with_capacity(SIDE * SIDE * 4);
    for y in 0..SIDE {
        for x in 0..SIDE {
            let v = (212. + height[y * SIDE + x] * 39.) as u8;
            albedo.extend([v, v, v, 255]);
            let dx = (height[y * SIDE + (x + 1) % SIDE] - height[y * SIDE + (x + SIDE - 1) % SIDE])
                * 0.28;
            let dy = (height[((y + 1) % SIDE) * SIDE + x]
                - height[((y + SIDE - 1) % SIDE) * SIDE + x])
                * 0.28;
            let n = Vec3::new(-dx, -dy, 1.).normalize();
            normals.extend([
                ((n.x * 0.5 + 0.5) * 255.) as u8,
                ((n.y * 0.5 + 0.5) * 255.) as u8,
                ((n.z * 0.5 + 0.5) * 255.) as u8,
                255,
            ]);
        }
    }
    let make = |bytes, format| {
        let mut image = Image::new(
            Extent3d {
                width: SIDE as u32,
                height: SIDE as u32,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            bytes,
            format,
            RenderAssetUsages::RENDER_WORLD,
        );
        image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
            address_mode_u: ImageAddressMode::Repeat,
            address_mode_v: ImageAddressMode::Repeat,
            ..ImageSamplerDescriptor::linear()
        });
        image
    };
    (
        make(albedo, TextureFormat::Rgba8UnormSrgb),
        make(normals, TextureFormat::Rgba8Unorm),
    )
}

fn material(
    layer: Layer,
    state: &mut RenderState,
    materials: &mut Assets<StandardMaterial>,
) -> Handle<StandardMaterial> {
    let detail = if layer == Layer::Terrain {
        state.terrain_detail.clone()
    } else {
        None
    };
    let normal = if layer == Layer::Terrain {
        state.terrain_normal.clone()
    } else {
        None
    };
    state
        .materials
        .entry(layer)
        .or_insert_with(|| {
            materials.add(StandardMaterial {
                base_color: Color::WHITE,
                base_color_texture: detail,
                normal_map_texture: normal,
                cull_mode: None,
                perceptual_roughness: if layer == Layer::Water { 0.16 } else { 0.88 },
                metallic: 0.0,
                clearcoat: if layer == Layer::Water { 0.65 } else { 0.0 },
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
    state.detail_levels.remove(&key);
    state.source_vertices.remove(&key);
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
    jobs: BTreeMap<TileKey, (u64, u8, Mutex<mpsc::Receiver<BTreeMap<Layer, Mesh>>>)>,
    revision: u64,
    detail_targets: BTreeMap<TileKey, u8>,
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
    tasks
        .detail_targets
        .retain(|key, _| resident.chunks.contains_key(key));
    for &key in resident.chunks.keys() {
        let distance = distance_to_tile_m(runtime.focus, key);
        let previous = tasks
            .detail_targets
            .get(&key)
            .copied()
            .or_else(|| state.detail_levels.get(&key).copied());
        let detail = feature_detail(distance, previous);
        tasks.detail_targets.insert(key, detail);
    }
    let desired_detail = tasks.detail_targets.clone();
    for (&key, &detail) in &desired_detail {
        if state.detail_levels.get(&key).copied() != Some(detail)
            && !tasks
                .jobs
                .get(&key)
                .is_some_and(|(_, job_detail, _)| *job_detail == detail)
        {
            tasks.pending.insert(key);
        }
    }
    let keys: Vec<_> = tasks.jobs.keys().copied().collect();
    let mut integrated = 0;
    for key in keys {
        if integrated >= runtime.config.integrate_per_frame {
            break;
        }
        let Some((revision, detail, receiver)) = tasks.jobs.get(&key) else {
            continue;
        };
        let revision = *revision;
        let detail = *detail;
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
        if revision != tasks.revision || desired_detail.get(&key).copied() != Some(detail) {
            tasks.pending.insert(key);
            continue;
        }
        let Some(layers) = result else {
            continue;
        };
        clear_chunk(key, &mut state, &mut commands, &mut meshes);
        let vertices = layers.values().map(Mesh::count_vertices).sum();
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
        state.detail_levels.insert(key, detail);
        state.source_vertices.insert(key, vertices);
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
        let detail = desired_detail.get(&key).copied().unwrap_or(2);
        std::thread::spawn(move || {
            let mut layers = feature_meshes_lod(&chunk, budget, &removed, detail);
            if let Some(mesh) = terrain_mesh(&chunk) {
                layers.insert(Layer::Terrain, mesh);
            }
            let _ = tx.send(layers);
        });
        tasks.jobs.insert(key, (revision, detail, Mutex::new(rx)));
    }
    stats.integration_ms = start.elapsed().as_secs_f64() * 1000.;
    stats.chunks = state.chunks.len();
    stats.meshes =
        state.chunks.values().map(Vec::len).sum::<usize>() + state.sandbox_entities.len();
    stats.pending_meshes = tasks.pending.len();
    stats.building_meshes = tasks.jobs.len();
    stats.vertices = state.source_vertices.values().sum();
}

/// Distance to geographic bounds rather than tile center keeps nearby edges detailed.
fn distance_to_tile_m(position: GeoPosition, key: TileKey) -> f64 {
    let b = key.bounds();
    let center = (b.west + b.east) * 0.5;
    let lon = center + terra_core::normalize_longitude(position.longitude - center);
    let nearest = GeoPosition::new(
        position.latitude.clamp(b.south, b.north),
        lon.clamp(b.west, b.east),
        position.altitude_m,
    )
    .unwrap_or(position);
    position.surface_distance_m(nearest)
}
/// Hysteresis bands stop camera motion near a threshold rebuilding meshes each frame.
fn feature_detail(distance: f64, previous: Option<u8>) -> u8 {
    match previous {
        Some(0) if distance <= 650. => 0,
        Some(1) if (450.0..=1700.0).contains(&distance) => 1,
        Some(2) if distance >= 1300. => 2,
        _ if distance <= 550. => 0,
        _ if distance <= 1500. => 1,
        _ => 2,
    }
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
        light.illuminance = 24000. * daylight * (1. - weather.cloud_cover * 0.65);
        light.color = Color::srgb(1., 0.86 + daylight * 0.12, 0.72 + daylight * 0.25);
    }
    ambient.brightness = 42. + daylight * 170.;
    let cloud = weather.cloud_cover;
    let sky = Color::srgb(
        0.02 + daylight * (0.39 + cloud * 0.10),
        0.035 + daylight * (0.59 - cloud * 0.04),
        0.06 + daylight * (0.65 - cloud * 0.08),
    );
    clear.0 = sky;
    let tile = TileKey::at(runtime.focus, runtime.config.chunk_level).ok();
    let local_span = tile
        .map(|k| {
            let b = k.bounds();
            let mut north = runtime.focus;
            north.latitude = b.north;
            let mut south = runtime.focus;
            south.latitude = b.south;
            north.surface_distance_m(south) as f32 * (runtime.config.chunk_radius as f32 + 0.45)
        })
        .unwrap_or(2600.)
        .clamp(700., 12_000.);
    for mut fog in &mut fogs {
        fog.color = sky;
        fog.falloff = FogFalloff::Linear {
            start: local_span * 0.46 / (1. + weather.precipitation_mm_h * 0.2),
            end: local_span * 1.08 / (1. + weather.precipitation_mm_h * 0.15),
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

/// Shared bounded meshes face local -Z; ground actors' feet/wheels rest at y=0.
pub fn actor_mesh(kind: ActorKind) -> Mesh {
    actor_mesh_variant(kind, 0)
}
pub fn actor_mesh_variant(kind: ActorKind, variant: u64) -> Mesh {
    let mut b = MeshBuilder::default();
    let jackets = [
        [0.25, 0.43, 0.48, 1.],
        [0.63, 0.35, 0.22, 1.],
        [0.37, 0.45, 0.25, 1.],
        [0.54, 0.40, 0.54, 1.],
    ];
    let cloth = jackets[(variant % 4) as usize];
    let dark = [0.19, 0.23, 0.26, 1.];
    let skin = if variant % 3 == 0 {
        [0.55, 0.37, 0.25, 1.]
    } else {
        [0.78, 0.60, 0.43, 1.]
    };
    match kind {
        ActorKind::Human => {
            for x in [-0.12, 0.12] {
                b.ellipsoid(
                    Vec3::new(x, 0.10, -0.055),
                    Vec3::new(0.12, 0.10, 0.22),
                    [0.17, 0.14, 0.12, 1.],
                    8,
                );
                b.cylinder(
                    Vec3::new(x, 0.15, 0.),
                    Vec3::new(x, 0.87, 0.),
                    0.10,
                    dark,
                    8,
                );
            }
            b.ellipsoid(
                Vec3::new(0., 1.10, 0.),
                Vec3::new(0.27, 0.35, 0.17),
                cloth,
                12,
            );
            b.cylinder(
                Vec3::new(-0.24, 1.30, 0.),
                Vec3::new(-0.33, 0.92, -0.04),
                0.08,
                cloth,
                8,
            );
            b.cylinder(
                Vec3::new(0.24, 1.30, 0.),
                Vec3::new(0.32, 0.92, -0.04),
                0.08,
                cloth,
                8,
            );
            for x in [-0.33, 0.32] {
                b.ellipsoid(
                    Vec3::new(x, 0.89, -0.04),
                    Vec3::new(0.07, 0.09, 0.07),
                    skin,
                    8,
                );
            }
            b.cylinder(
                Vec3::new(0., 1.39, 0.),
                Vec3::new(0., 1.50, 0.),
                0.085,
                skin,
                8,
            );
            b.ellipsoid(
                Vec3::new(0., 1.65, 0.),
                Vec3::new(0.16, 0.22, 0.17),
                skin,
                12,
            );
            b.ellipsoid(
                Vec3::new(0., 1.79, 0.015),
                Vec3::new(0.17, 0.105, 0.17),
                [0.22, 0.17, 0.12, 1.],
                10,
            );
            b.ellipsoid(
                Vec3::new(0., 1.65, -0.166),
                Vec3::new(0.035, 0.045, 0.035),
                skin,
                6,
            );
            for x in [-0.062, 0.062] {
                b.ellipsoid(
                    Vec3::new(x, 1.70, -0.145),
                    Vec3::splat(0.014),
                    [0.13, 0.16, 0.15, 1.],
                    6,
                );
            }
            if variant % 4 == 2 {
                b.cuboid(
                    Vec3::new(0., 1.13, 0.20),
                    Vec3::new(0.33, 0.42, 0.16),
                    [0.37, 0.31, 0.20, 1.],
                );
            }
        }
        ActorKind::Animal => {
            let fur = if variant % 2 == 0 {
                [0.53, 0.36, 0.21, 1.]
            } else {
                [0.72, 0.68, 0.55, 1.]
            };
            b.ellipsoid(
                Vec3::new(0., 0.82, 0.),
                Vec3::new(0.36, 0.38, 0.72),
                fur,
                12,
            );
            b.ellipsoid(
                Vec3::new(0., 1.12, -0.65),
                Vec3::new(0.24, 0.30, 0.30),
                fur,
                10,
            );
            b.ellipsoid(
                Vec3::new(0., 1.03, -0.91),
                Vec3::new(0.17, 0.14, 0.23),
                [0.35, 0.28, 0.20, 1.],
                8,
            );
            for x in [-0.25, 0.25] {
                for z in [-0.47, 0.45] {
                    b.cylinder(Vec3::new(x, 0.12, z), Vec3::new(x, 0.75, z), 0.07, fur, 7);
                    b.cuboid(
                        Vec3::new(x, 0.06, z - 0.02),
                        Vec3::new(0.13, 0.12, 0.17),
                        dark,
                    );
                }
            }
            for x in [-0.18, 0.18] {
                b.ellipsoid(
                    Vec3::new(x, 1.41, -0.62),
                    Vec3::new(0.08, 0.20, 0.055),
                    fur,
                    7,
                );
            }
            b.cylinder(
                Vec3::new(0., 0.9, 0.6),
                Vec3::new(0.1, 0.65, 1.0),
                0.04,
                fur,
                6,
            );
        }
        ActorKind::Creature => {
            b.ellipsoid(
                Vec3::new(0., 1.15, 0.),
                Vec3::new(0.61, 0.77, 0.46),
                [0.34, 0.48, 0.31, 1.],
                12,
            );
            b.ellipsoid(
                Vec3::new(0., 2.03, -0.07),
                Vec3::new(0.45, 0.41, 0.39),
                [0.43, 0.55, 0.34, 1.],
                12,
            );
            for x in [-0.36, 0.36] {
                b.cylinder(
                    Vec3::new(x, 0.18, 0.),
                    Vec3::new(x, 0.94, 0.),
                    0.20,
                    [0.29, 0.37, 0.24, 1.],
                    9,
                );
                b.ellipsoid(
                    Vec3::new(x, 0.13, -0.10),
                    Vec3::new(0.26, 0.13, 0.36),
                    dark,
                    8,
                );
            }
            for x in [-0.65, 0.65] {
                b.cylinder(
                    Vec3::new(x * 0.7, 1.65, 0.),
                    Vec3::new(x, 0.8, -0.04),
                    0.17,
                    [0.35, 0.46, 0.27, 1.],
                    9,
                );
            }
            for x in [-0.31, 0.31] {
                b.cylinder(
                    Vec3::new(x, 2.3, 0.),
                    Vec3::new(x * 1.15, 2.75, 0.),
                    0.09,
                    [0.77, 0.71, 0.53, 1.],
                    7,
                );
            }
        }
        ActorKind::Vehicle => {
            let paint = jackets[((variant + 1) % 4) as usize];
            for x in [-0.94, 0.94] {
                for z in [-1.25, 1.25] {
                    b.cylinder(
                        Vec3::new(x - 0.13, 0.38, z),
                        Vec3::new(x + 0.13, 0.38, z),
                        0.38,
                        [0.12, 0.13, 0.13, 1.],
                        12,
                    );
                    b.cylinder(
                        Vec3::new(x - 0.15, 0.38, z),
                        Vec3::new(x + 0.15, 0.38, z),
                        0.17,
                        [0.59, 0.60, 0.56, 1.],
                        10,
                    );
                }
            }
            b.cuboid(Vec3::new(0., 0.69, 0.), Vec3::new(1.85, 0.48, 3.8), paint);
            b.ellipsoid(
                Vec3::new(0., 1.07, -0.35),
                Vec3::new(0.83, 0.63, 1.22),
                paint,
                12,
            );
            b.cuboid(
                Vec3::new(0., 1.25, -1.02),
                Vec3::new(1.52, 0.53, 0.05),
                [0.22, 0.37, 0.41, 1.],
            );
            b.cuboid(
                Vec3::new(0., 0.62, -1.96),
                Vec3::new(1.8, 0.14, 0.12),
                [0.47, 0.49, 0.47, 1.],
            );
            for x in [-0.62, 0.62] {
                b.cuboid(
                    Vec3::new(x, 0.78, -1.93),
                    Vec3::new(0.38, 0.20, 0.08),
                    [0.96, 0.91, 0.64, 1.],
                );
                b.cuboid(
                    Vec3::new(x, 0.77, 1.93),
                    Vec3::new(0.32, 0.18, 0.07),
                    [0.72, 0.16, 0.10, 1.],
                );
            }
            for x in [-0.94, 0.94] {
                b.cuboid(
                    Vec3::new(x, 1.26, -0.25),
                    Vec3::new(0.035, 0.42, 1.25),
                    [0.24, 0.37, 0.40, 1.],
                );
            }
            if variant % 4 == 3 {
                b.cuboid(
                    Vec3::new(0., 1.2, 0.95),
                    Vec3::new(1.65, 1.0, 1.55),
                    [0.67, 0.61, 0.45, 1.],
                );
            }
        }
        ActorKind::Ship => {
            b.ellipsoid(
                Vec3::new(0., 0.7, 0.),
                Vec3::new(2.35, 1.1, 6.9),
                [0.27, 0.37, 0.40, 1.],
                16,
            );
            b.cuboid(
                Vec3::new(0., 1.35, 0.),
                Vec3::new(3.8, 0.2, 10.5),
                [0.66, 0.55, 0.37, 1.],
            );
            b.cuboid(
                Vec3::new(0., 2.25, 1.2),
                Vec3::new(2.6, 1.7, 3.3),
                [0.82, 0.82, 0.72, 1.],
            );
            b.cuboid(
                Vec3::new(0., 2.55, -0.49),
                Vec3::new(2.2, 0.65, 0.05),
                [0.21, 0.40, 0.45, 1.],
            );
            for x in [-1.65, 1.65] {
                b.cylinder(
                    Vec3::new(x, 2., -4.),
                    Vec3::new(x, 2., 4.5),
                    0.045,
                    [0.74, 0.76, 0.71, 1.],
                    5,
                );
            }
            b.cylinder(
                Vec3::new(0., 3.1, 1.5),
                Vec3::new(0., 6.2, 1.5),
                0.08,
                [0.64, 0.64, 0.56, 1.],
                7,
            );
        }
        ActorKind::Helicopter => {
            b.ellipsoid(
                Vec3::new(0., 1.3, 0.),
                Vec3::new(1.25, 1.05, 2.6),
                cloth,
                16,
            );
            b.ellipsoid(
                Vec3::new(0., 1.55, -1.8),
                Vec3::new(1.08, 0.77, 0.8),
                [0.19, 0.35, 0.39, 1.],
                12,
            );
            b.cylinder(
                Vec3::new(0., 1.4, 1.6),
                Vec3::new(0., 1.8, 6.),
                0.21,
                cloth,
                9,
            );
            b.cuboid(Vec3::new(0., 3.05, 0.), Vec3::new(12., 0.07, 0.36), dark);
            b.cuboid(Vec3::new(0., 3.05, 0.), Vec3::new(0.36, 0.07, 12.), dark);
            b.cylinder(Vec3::new(0., 2., 0.), Vec3::new(0., 3.1, 0.), 0.10, dark, 8);
            for x in [-1.15, 1.15] {
                b.cylinder(
                    Vec3::new(x, 0.15, -1.7),
                    Vec3::new(x, 0.15, 1.7),
                    0.08,
                    dark,
                    8,
                );
                b.cylinder(
                    Vec3::new(x, 0.15, -0.9),
                    Vec3::new(x * 0.6, 0.75, -0.9),
                    0.055,
                    dark,
                    6,
                );
            }
            b.cuboid(Vec3::new(0., 2.3, 5.9), Vec3::new(0.08, 2.0, 1.0), cloth);
        }
        ActorKind::Airplane => {
            b.ellipsoid(
                Vec3::new(0., 1.2, 0.),
                Vec3::new(0.76, 0.72, 5.1),
                [0.83, 0.83, 0.76, 1.],
                16,
            );
            b.ellipsoid(
                Vec3::new(0., 1.65, -2.2),
                Vec3::new(0.67, 0.47, 1.1),
                [0.23, 0.36, 0.40, 1.],
                12,
            );
            for side in [-1., 1.] {
                b.quad(
                    Vec3::new(side * 0.5, 1.1, -1.7),
                    Vec3::new(side * 7.4, 1.1, 0.1),
                    Vec3::new(side * 7.4, 1.1, 1.1),
                    Vec3::new(side * 0.5, 1.1, 1.2),
                    cloth,
                );
            }
            b.cuboid(Vec3::new(0., 1.45, 3.8), Vec3::new(4.8, 0.12, 1.1), cloth);
            b.triangle(
                Vec3::new(0., 1.5, 3.),
                Vec3::new(0., 3.4, 4.1),
                Vec3::new(0., 1.5, 4.8),
                cloth,
            );
            for x in [-1.3, 1.3] {
                b.cylinder(
                    Vec3::new(x - 0.1, 0.24, 0.),
                    Vec3::new(x + 0.1, 0.24, 0.),
                    0.24,
                    dark,
                    9,
                );
            }
        }
        ActorKind::Balloon => {
            b.cuboid(
                Vec3::new(0., 0.75, 0.),
                Vec3::new(1.8, 1.5, 1.8),
                [0.48, 0.34, 0.19, 1.],
            );
            b.ellipsoid(Vec3::new(0., 8.2, 0.), Vec3::new(4.8, 5.2, 4.8), cloth, 16);
            for x in [-0.8, 0.8] {
                for z in [-0.8, 0.8] {
                    b.cylinder(
                        Vec3::new(x, 1.3, z),
                        Vec3::new(x * 2.1, 4.4, z * 2.1),
                        0.035,
                        [0.35, 0.30, 0.19, 1.],
                        5,
                    );
                }
            }
            b.cylinder(
                Vec3::new(0., 2., 0.),
                Vec3::new(0., 3.7, 0.),
                0.15,
                [0.85, 0.48, 0.16, 1.],
                8,
            );
        }
    }
    b.finish().expect("actor contains bounded authored faces")
}

pub fn market_mesh(seed: u64) -> Mesh {
    let mut b = MeshBuilder::default();
    let wood = [0.48, 0.33, 0.20, 1.];
    b.cuboid(Vec3::new(0., 0.92, 0.), Vec3::new(3.8, 0.22, 2.1), wood);
    for x in [-1.72, 1.72] {
        for z in [-0.86, 0.86] {
            b.cylinder(Vec3::new(x, 0., z), Vec3::new(x, 2.9, z), 0.07, wood, 7);
        }
    }
    for stripe in 0..10 {
        let x = -2. + stripe as f32 * 0.4;
        let color = if stripe % 2 == 0 {
            [0.83, 0.68, 0.34, 1.]
        } else {
            [0.24, 0.43, 0.36, 1.]
        };
        b.quad(
            Vec3::new(x, 2.65, -1.25),
            Vec3::new(x + 0.4, 2.65, -1.25),
            Vec3::new(x + 0.4, 3.1, 0.),
            Vec3::new(x, 3.1, 0.),
            color,
        );
        b.quad(
            Vec3::new(x, 3.1, 0.),
            Vec3::new(x + 0.4, 3.1, 0.),
            Vec3::new(x + 0.4, 2.65, 1.25),
            Vec3::new(x, 2.65, 1.25),
            color,
        );
    }
    for i in 0..4 {
        let x = -1.4 + i as f32 * 0.92;
        b.cuboid(
            Vec3::new(x, 1.18, 0.),
            Vec3::new(0.75, 0.32, 0.72),
            [0.52, 0.39, 0.22, 1.],
        );
        for n in 0..6 {
            b.ellipsoid(
                Vec3::new(
                    x - 0.22 + (n % 3) as f32 * 0.22,
                    1.40,
                    (n / 3) as f32 * 0.25 - 0.12,
                ),
                Vec3::new(0.12, 0.10, 0.12),
                if (i + seed as usize) % 2 == 0 {
                    [0.77, 0.38, 0.19, 1.]
                } else {
                    [0.59, 0.65, 0.26, 1.]
                },
                6,
            );
        }
    }
    b.finish().unwrap()
}

pub fn production_mesh(kind: &str, seed: u64) -> Mesh {
    let mut b = MeshBuilder::default();
    let name = kind.to_lowercase();
    if name.contains("farm") || name.contains("field") {
        b.cuboid(
            Vec3::new(0., 0.03, 0.),
            Vec3::new(12., 0.06, 9.),
            [0.37, 0.29, 0.17, 1.],
        );
        for row in 0..8 {
            for col in 0..14 {
                let p = Vec3::new(col as f32 * 0.8 - 5.2, 0., row as f32 - 3.5);
                b.cylinder(
                    p,
                    p + Vec3::Y * (0.6 + (col % 3) as f32 * 0.07),
                    0.025,
                    [0.63, 0.62, 0.26, 1.],
                    5,
                );
                b.ellipsoid(
                    p + Vec3::Y * 0.72,
                    Vec3::new(0.07, 0.15, 0.07),
                    [0.79, 0.69, 0.32, 1.],
                    6,
                );
            }
        }
    } else if name.contains("well") || name.contains("water") {
        b.cylinder(Vec3::ZERO, Vec3::Y * 0.9, 1.05, [0.56, 0.55, 0.47, 1.], 12);
        b.cylinder(
            Vec3::Y * 0.91,
            Vec3::Y * 0.93,
            0.76,
            [0.18, 0.41, 0.48, 1.],
            12,
        );
        for x in [-1.2, 1.2] {
            b.cylinder(
                Vec3::new(x, 0., 0.),
                Vec3::new(x, 2.7, 0.),
                0.09,
                [0.42, 0.31, 0.20, 1.],
                7,
            );
        }
        b.cuboid(
            Vec3::new(0., 2.5, 0.),
            Vec3::new(2.7, 0.15, 0.2),
            [0.42, 0.31, 0.20, 1.],
        );
    } else {
        let timber = name.contains("saw") || name.contains("lumber") || name.contains("wood");
        let mill = name.contains("mill") && !timber;
        let facade = if timber {
            [0.49, 0.36, 0.23, 1.]
        } else {
            [0.74, 0.67, 0.53, 1.]
        };
        b.cuboid(Vec3::new(0., 2.1, 0.), Vec3::new(7., 4.2, 6.), facade);
        for side in [-1., 1.] {
            b.quad(
                Vec3::new(side * 4., 4.1, -3.5),
                Vec3::new(0., 6.1, -3.5),
                Vec3::new(0., 6.1, 3.5),
                Vec3::new(side * 4., 4.1, 3.5),
                [0.44, 0.26, 0.19, 1.],
            );
        }
        for z in [-3., 3.] {
            b.triangle(
                Vec3::new(-3.5, 4.2, z),
                Vec3::new(3.5, 4.2, z),
                Vec3::new(0., 6., z),
                facade,
            );
        }
        b.cuboid(
            Vec3::new(0., 1.3, -3.04),
            Vec3::new(1.7, 2.6, 0.08),
            [0.29, 0.25, 0.19, 1.],
        );
        for x in [-2.3, 2.3] {
            b.cuboid(
                Vec3::new(x, 2.7, -3.05),
                Vec3::new(1.0, 1.25, 0.08),
                [0.22, 0.36, 0.39, 1.],
            );
        }
        if timber {
            for row in 0..3 {
                for col in 0..4 {
                    let p = Vec3::new(5. + row as f32 * 0.45, 0.24 + col as f32 * 0.44, 0.);
                    b.cylinder(
                        p - Vec3::Z * 2.,
                        p + Vec3::Z * 2.,
                        0.21,
                        [0.56, 0.41, 0.24, 1.],
                        8,
                    );
                }
            }
        } else if mill {
            b.cylinder(
                Vec3::new(0., 4.9, -3.4),
                Vec3::new(0., 4.9, -3.8),
                0.3,
                [0.33, 0.28, 0.19, 1.],
                9,
            );
            for i in 0..4 {
                let angle = i as f32 * std::f32::consts::FRAC_PI_2 + 0.3;
                let d = Vec3::new(angle.cos(), angle.sin(), 0.);
                b.cylinder(
                    Vec3::new(0., 4.9, -3.8),
                    Vec3::new(0., 4.9, -3.8) + d * 4.0,
                    0.12,
                    [0.47, 0.35, 0.22, 1.],
                    6,
                );
                let t = Vec3::new(-d.y, d.x, 0.);
                let a = Vec3::new(0., 4.9, -3.85) + d * 1.3;
                b.quad(
                    a - t * 0.3,
                    a + t * 0.3,
                    a + d * 2.6 + t * 0.3,
                    a + d * 2.6 - t * 0.3,
                    [0.80, 0.75, 0.59, 1.],
                );
            }
        } else {
            b.cuboid(
                Vec3::new(2., 5.6, 1.),
                Vec3::new(0.8, 3., 0.8),
                [0.48, 0.30, 0.23, 1.],
            );
            for i in 0..3 {
                b.cuboid(
                    Vec3::new(4.3, 0.35 + i as f32 * 0.7, (seed % 3) as f32 * 0.1),
                    Vec3::new(1.4, 0.68, 1.3),
                    [0.56, 0.43, 0.26, 1.],
                );
            }
        }
    }
    b.finish().unwrap()
}

#[cfg(test)]
mod cache_tests {
    use super::*;
    #[test]
    fn feature_lod_hysteresis_preserves_detail_at_nearby_tile_edges() {
        assert_eq!(feature_detail(549., None), 0);
        assert_eq!(feature_detail(600., Some(0)), 0);
        assert_eq!(feature_detail(651., Some(0)), 1);
        assert_eq!(feature_detail(500., Some(1)), 1);
        assert_eq!(feature_detail(449., Some(1)), 0);
        assert_eq!(feature_detail(1600., Some(1)), 1);
        assert_eq!(feature_detail(1701., Some(1)), 2);
        assert_eq!(feature_detail(1400., Some(2)), 2);
        assert_eq!(feature_detail(1299., Some(2)), 1);
        let p = GeoPosition::new(48.137, 11.575, 0.).unwrap();
        let key = TileKey::at(p, 15).unwrap();
        assert!(distance_to_tile_m(p, key) < 0.001);
        let at_east = GeoPosition::new(p.latitude, key.bounds().east, 0.).unwrap();
        assert!(distance_to_tile_m(at_east, key) < 0.001);
    }
    #[test]
    fn default_land_neighborhood_stays_below_two_million_source_vertices() {
        use bevy::mesh::VertexAttributeValues;
        let p = GeoPosition::new(48.137, 11.575, 0.).unwrap();
        let key = TileKey::at(p, 15).unwrap();
        let mut vertices = 0usize;
        let mut tiers = [0usize; 3];
        for key in key.neighbors(3) {
            let detail = feature_detail(distance_to_tile_m(p, key), None);
            tiers[detail as usize] += 1;
            let chunk = terra_stream::procedural_chunk(key, 65, 42);
            vertices += 65 * 65;
            for mesh in feature_meshes_lod(&chunk, 128, &BTreeSet::new(), detail).values() {
                let VertexAttributeValues::Float32x3(points) =
                    mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap()
                else {
                    panic!()
                };
                vertices += points.len();
            }
        }
        println!("49 source chunks: {vertices} vertices; near/mid/far chunks {tiers:?}");
        assert_eq!(tiers.iter().sum::<usize>(), 49);
        assert!(
            vertices < 2_000_000,
            "source view exceeded its default geometry budget: {vertices}"
        );
    }
    #[test]
    fn authored_actor_and_facility_meshes_have_bounded_finite_geometry() {
        use bevy::mesh::VertexAttributeValues;
        for kind in ActorKind::ALL {
            for variant in 0..4 {
                let mesh = actor_mesh_variant(kind, variant);
                let VertexAttributeValues::Float32x3(vertices) =
                    mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap()
                else {
                    panic!()
                };
                assert!(vertices.len() < 8_000);
                assert!(vertices.iter().flatten().all(|x| x.is_finite()));
                if kind == ActorKind::Human {
                    assert!(vertices.iter().all(|p| p[1] >= -0.001 && p[1] < 2.));
                }
            }
        }
        for mesh in [
            market_mesh(7),
            production_mesh("farm", 1),
            production_mesh("sawmill", 2),
            production_mesh("mill", 3),
            production_mesh("bakery", 4),
        ] {
            let VertexAttributeValues::Float32x3(vertices) =
                mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap()
            else {
                panic!()
            };
            assert!(vertices.len() < 16_000);
            assert!(vertices.iter().flatten().all(|x| x.is_finite()));
        }
    }
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
