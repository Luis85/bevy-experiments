use crate::{
    geometry::{MeshBuilder, feature_meshes_lod, terrain_mesh_lod},
    sandbox::SandboxRuntime,
    simulation::{GeoActor, GeoAnchor, RenderQueue, ResidentChunks, WorldRuntime, WorldSet},
};
use bevy::{
    light::{
        Atmosphere, CascadeShadowConfigBuilder, SunDisk, atmosphere::ScatteringMedium,
        light_consts::lux,
    },
    math::Affine2,
    prelude::*,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex, mpsc},
    time::{Duration, Instant},
};
use terra_core::{
    ActorKind, GeoPosition, Layer, LocalFrame, Season, TileKey, WorldChunk, season, solar_position,
};
use terra_stream::StreamEvent;

#[path = "render_animation.rs"]
mod animation;
#[path = "render_character.rs"]
mod character;
pub use character::{
    character_appearance_colors, character_body_scale, driver_seat_presentation,
    known_driver_vehicle,
};
#[path = "render_budget.rs"]
mod budget;
#[path = "render_pack.rs"]
mod pack;
#[path = "render_quality.rs"]
mod quality;
#[path = "render_scenes.rs"]
mod scenes;
#[path = "render_materials.rs"]
mod surface_materials;
pub use budget::{
    ActorSceneBudget, ActorSceneBudgetDiagnostics, ActorSceneLimits, ActorSceneUsage,
};
pub use pack::{
    ActorSceneAnimation, ActorSceneModel, AssetPackDiagnostics, AssetPackPlugin,
    bundled_wildlife_pack,
};
pub use quality::{RenderCapabilities, RenderQuality, RenderQualitySettings};
pub use scenes::{
    ActorSceneActive, ActorSceneAnchor, ActorSceneAppearance, ActorSceneInstance,
    ActorSceneOverrides, ProceduralActorMesh,
};

#[path = "render_detail.rs"]
mod detail;
#[path = "render_wildlife.rs"]
mod wildlife;
pub use detail::{WorldDetailSettings, natural_resource_mesh};
pub use wildlife::{
    WILDLIFE_DRINKING, WILDLIFE_GRAZING, WILDLIFE_POSE_COUNT, WILDLIFE_RESTING, WILDLIFE_STANDING,
    wildlife_mesh,
};

pub struct TerraRenderPlugin;
impl Plugin for TerraRenderPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RenderQueue>()
            .init_resource::<RenderState>()
            .init_resource::<RenderTelemetry>()
            .init_resource::<MeshTasks>()
            .init_resource::<WorldDetailSettings>()
            .init_resource::<RenderQualitySettings>()
            .init_resource::<RenderCapabilities>()
            .init_resource::<ActorSceneOverrides>()
            .init_resource::<ActorSceneBudget>()
            .init_resource::<budget::ActorSceneAdmission>()
            .init_resource::<detail::DetailState>()
            .init_resource::<crate::politics::AtlasRenderState>()
            .add_systems(Startup, setup_lighting)
            .add_observer(scenes::scene_ready)
            .add_systems(
                Update,
                (
                    quality::configure_presentation,
                    render_chunks,
                    render_sandbox,
                    detail::render_world_detail,
                    sync_anchors,
                    animate_environment,
                    scenes::sync_actor_scenes,
                    character::update_human_appearances,
                    animation::animate_actor_scenes,
                    crate::politics::render_atlas,
                )
                    .chain()
                    .in_set(WorldSet::Render),
            );
    }
    fn finish(&self, app: &mut App) {
        quality::detect_capabilities(app);
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
    /// Integrated keys whose immutable source instance is still resident.
    /// Old visible geometry during source replacement cannot satisfy readiness.
    pub integrated_chunks: BTreeSet<TileKey>,
    pub pending_meshes: usize,
    pub building_meshes: usize,
    /// Resident source jobs awaiting/retrying a failed build.
    pub retrying_meshes: usize,
    /// Observed disconnected/poisoned jobs in this renderer session.
    pub mesh_failures: u64,
    pub last_mesh_error: Option<String>,
    /// Integrated source geometry, including terrain; excludes living/sandbox visuals.
    pub vertices: usize,
    /// Actual terrain vertices after render-only interior reduction.
    pub terrain_vertices: usize,
    /// The corresponding unmodified source heightfield sample count.
    pub terrain_full_vertices: usize,
    pub lod_chunks: [usize; 3],
}
/// Read-only renderer readiness and source geometry budget diagnostics.
pub type RenderDiagnostics = RenderTelemetry;
#[derive(Resource, Default)]
struct RenderState {
    chunks: BTreeMap<TileKey, Vec<(Entity, Handle<Mesh>)>>,
    chunk_sources: BTreeMap<TileKey, Arc<WorldChunk>>,
    materials: BTreeMap<Layer, Handle<StandardMaterial>>,
    sandbox_entities: Vec<(Entity, Handle<Mesh>)>,
    revision: u64,
    sandbox_fingerprint: u64,
    sandbox_terrain_signature: u64,
    removed: BTreeSet<String>,
    initialized: bool,
    sandbox_focus: Option<TileKey>,
    authored_visible: BTreeSet<String>,
    terrain_detail: Option<Handle<Image>>,
    terrain_normal: Option<Handle<Image>>,
    water_normal: Option<Handle<Image>>,
    quality: Option<(RenderQuality, bool)>,
    terrain_vertices: BTreeMap<TileKey, usize>,
    detail_levels: BTreeMap<TileKey, u8>,
    source_vertices: BTreeMap<TileKey, usize>,
}

fn setup_lighting(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut state: ResMut<RenderState>,
    mut scattering: ResMut<Assets<ScatteringMedium>>,
) {
    let (detail, normal) = surface_materials::terrain_detail_images();
    state.terrain_detail = Some(images.add(detail));
    state.terrain_normal = Some(images.add(normal));
    state.water_normal = Some(images.add(surface_materials::water_normal_image()));
    commands.spawn((
        quality::WorldAtmosphere,
        Atmosphere::earth(scattering.add(ScatteringMedium::earth(128, 128))),
    ));
    commands.insert_resource(GlobalAmbientLight {
        brightness: 650.0,
        color: Color::srgb(0.66, 0.77, 0.94),
        ..default()
    });
    commands.spawn((
        Sun,
        SunDisk::EARTH,
        DirectionalLight {
            illuminance: 24000.0,
            shadow_maps_enabled: true,
            ..default()
        },
        CascadeShadowConfigBuilder {
            first_cascade_far_bound: 28.,
            maximum_distance: 1800.,
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
    let detail = if layer == Layer::Terrain {
        state.terrain_detail.clone()
    } else {
        None
    };
    let normal = if layer == Layer::Terrain {
        state.terrain_normal.clone()
    } else if layer == Layer::Water {
        state.water_normal.clone()
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
                perceptual_roughness: if layer == Layer::Water { 0.16 } else { 0.94 },
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
    state.chunk_sources.remove(&key);
    state.detail_levels.remove(&key);
    state.source_vertices.remove(&key);
    state.terrain_vertices.remove(&key);
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
    jobs: BTreeMap<TileKey, ChunkMeshJob>,
    /// Quality-wide changes invalidate every page; feature edits have their own
    /// exact per-source context so unrelated work remains usable.
    revision: u64,
    detail_targets: BTreeMap<TileKey, u8>,
    edit_targets: BTreeMap<TileKey, ChunkMeshEdits>,
    failures: BTreeMap<TileKey, MeshFailure>,
    failure_count: u64,
    last_error: Option<String>,
}

struct ChunkMeshEdits {
    source: Arc<WorldChunk>,
    removed: Arc<BTreeSet<String>>,
}

struct MeshFailure {
    source: Arc<WorldChunk>,
    revision: u64,
    detail: u8,
    removed: Arc<BTreeSet<String>>,
    attempts: u32,
    not_before: Instant,
}

fn mesh_retry_delay(attempts: u32) -> Duration {
    Duration::from_millis((500u64 << attempts.saturating_sub(1).min(6)).min(30_000))
}

/// The source Arc is retained until completion so a tile leaving and re-entering
/// residency cannot admit a mesh built for its previous immutable source page.
struct ChunkMeshJob {
    revision: u64,
    detail: u8,
    source: Arc<WorldChunk>,
    removed: Arc<BTreeSet<String>>,
    receiver: Mutex<mpsc::Receiver<BTreeMap<Layer, Mesh>>>,
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
    quality: Res<RenderQualitySettings>,
) {
    let start = Instant::now();
    let profile = (quality.quality, quality.terrain_lod);
    if state.quality != Some(profile) {
        state.quality = Some(profile);
        tasks.revision += 1;
        tasks.pending.extend(resident.chunks.keys().copied());
    }
    let mut removed = sandbox.state.feature_edits.clone();
    removed.extend(
        sandbox
            .state
            .resource_nodes
            .values()
            .filter(|n| n.remaining == 0)
            .filter_map(|n| n.source_feature.clone()),
    );
    let removed_changed = removed != state.removed;
    if removed_changed {
        state.removed = removed;
    }
    tasks
        .edit_targets
        .retain(|key, _| resident.chunks.contains_key(key));
    for (&key, source) in &resident.chunks {
        let source_changed = tasks
            .edit_targets
            .get(&key)
            .is_none_or(|target| !Arc::ptr_eq(source, &target.source));
        if removed_changed || source_changed {
            let effective: BTreeSet<_> = source
                .features
                .iter()
                .filter(|feature| state.removed.contains(&feature.id))
                .map(|feature| feature.id.clone())
                .collect();
            if source_changed
                || tasks
                    .edit_targets
                    .get(&key)
                    .is_none_or(|target| *target.removed != effective)
            {
                tasks.edit_targets.insert(
                    key,
                    ChunkMeshEdits {
                        source: Arc::clone(source),
                        removed: Arc::new(effective),
                    },
                );
                tasks.pending.insert(key);
            }
        }
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
    let desired_edits: BTreeMap<_, _> = tasks
        .edit_targets
        .iter()
        .map(|(&key, edits)| (key, Arc::clone(&edits.removed)))
        .collect();
    let revision = tasks.revision;
    tasks.failures.retain(|key, failure| {
        failure.revision == revision
            && desired_detail.get(key).copied() == Some(failure.detail)
            && desired_edits.get(key) == Some(&failure.removed)
            && resident
                .chunks
                .get(key)
                .is_some_and(|source| Arc::ptr_eq(source, &failure.source))
    });
    for (&key, &detail) in &desired_detail {
        if state.detail_levels.get(&key).copied() != Some(detail)
            && !tasks.jobs.get(&key).is_some_and(|job| job.detail == detail)
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
        let Some(job) = tasks.jobs.get(&key) else {
            continue;
        };
        let revision = job.revision;
        let detail = job.detail;
        let source_is_current = resident
            .chunks
            .get(&key)
            .is_some_and(|source| Arc::ptr_eq(source, &job.source));
        let edits_are_current = desired_edits.get(&key) == Some(&job.removed);
        let result = match job.receiver.lock() {
            Ok(receiver) => match receiver.try_recv() {
                Ok(meshes) => Some(Ok(meshes)),
                Err(mpsc::TryRecvError::Disconnected) => Some(Err("mesh worker disconnected")),
                Err(mpsc::TryRecvError::Empty) => None,
            },
            Err(_) => Some(Err("mesh receiver lock poisoned")),
        };
        let Some(result) = result else {
            continue;
        };
        tasks.jobs.remove(&key);
        if !resident.chunks.contains_key(&key) {
            continue;
        }
        if !source_is_current
            || revision != tasks.revision
            || desired_detail.get(&key).copied() != Some(detail)
            || !edits_are_current
        {
            tasks.pending.insert(key);
            continue;
        }
        let layers = match result {
            Ok(layers) => layers,
            Err(reason) => {
                let attempts = tasks
                    .failures
                    .get(&key)
                    .map_or(1, |failure| failure.attempts.saturating_add(1));
                tasks.failures.insert(
                    key,
                    MeshFailure {
                        source: Arc::clone(&resident.chunks[&key]),
                        revision,
                        detail,
                        removed: Arc::clone(&desired_edits[&key]),
                        attempts,
                        not_before: start + mesh_retry_delay(attempts),
                    },
                );
                tasks.pending.insert(key);
                tasks.failure_count = tasks.failure_count.saturating_add(1);
                let error = format!("{reason} for {key:?}; retry attempt {attempts} is delayed");
                warn!("{error}");
                tasks.last_error = Some(error);
                continue;
            }
        };
        tasks.failures.remove(&key);
        // An edit can change and return to the job's exact context while it is
        // running. A successfully admitted completion also satisfies that queued
        // request; a different context was rejected above and remains pending.
        tasks.pending.remove(&key);
        clear_chunk(key, &mut state, &mut commands, &mut meshes);
        let vertices = layers.values().map(Mesh::count_vertices).sum();
        let terrain_vertices = layers
            .get(&Layer::Terrain)
            .map(Mesh::count_vertices)
            .unwrap_or(0);
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
        state
            .chunk_sources
            .insert(key, Arc::clone(&resident.chunks[&key]));
        state.detail_levels.insert(key, detail);
        state.source_vertices.insert(key, vertices);
        state.terrain_vertices.insert(key, terrain_vertices);
        integrated += 1;
    }
    while tasks.jobs.len() < 2 {
        let key = tasks
            .pending
            .iter()
            .filter(|k| {
                !tasks.jobs.contains_key(k)
                    && tasks
                        .failures
                        .get(k)
                        .is_none_or(|failure| start >= failure.not_before)
            })
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
        let removed = Arc::clone(&desired_edits[&key]);
        let budget = runtime.config.max_features_per_chunk;
        let (tx, rx) = mpsc::sync_channel(1);
        let revision = tasks.revision;
        let detail = desired_detail.get(&key).copied().unwrap_or(2);
        let terrain_detail = quality.terrain_detail(detail);
        let source = Arc::clone(&chunk);
        let worker_removed = Arc::clone(&removed);
        std::thread::spawn(move || {
            let mut layers = feature_meshes_lod(&chunk, budget, &worker_removed, detail);
            if let Some(mesh) = terrain_mesh_lod(&chunk, terrain_detail) {
                layers.insert(Layer::Terrain, mesh);
            }
            let _ = tx.send(layers);
        });
        tasks.jobs.insert(
            key,
            ChunkMeshJob {
                revision,
                detail,
                source,
                removed,
                receiver: Mutex::new(rx),
            },
        );
    }
    stats.integration_ms = start.elapsed().as_secs_f64() * 1000.;
    stats.chunks = state.chunks.len();
    stats.integrated_chunks = state
        .chunk_sources
        .iter()
        .filter(|(key, integrated_source)| {
            resident
                .chunks
                .get(key)
                .is_some_and(|source| Arc::ptr_eq(source, integrated_source))
        })
        .map(|(&key, _)| key)
        .collect();
    stats.meshes =
        state.chunks.values().map(Vec::len).sum::<usize>() + state.sandbox_entities.len();
    stats.pending_meshes = tasks.pending.len();
    stats.building_meshes = tasks.jobs.len();
    stats.retrying_meshes = tasks.failures.len();
    stats.mesh_failures = tasks.failure_count;
    stats.last_mesh_error.clone_from(&tasks.last_error);
    stats.vertices = state.source_vertices.values().sum();
    stats.terrain_vertices = state.terrain_vertices.values().sum();
    stats.terrain_full_vertices = state
        .chunks
        .keys()
        .filter_map(|k| resident.chunks.get(k))
        .map(|c| c.heights.width * c.heights.height)
        .sum();
    stats.lod_chunks = [0; 3];
    for &lod in state.detail_levels.values() {
        stats.lod_chunks[lod.min(2) as usize] += 1;
    }
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
                && !sandbox.state.ecology.resources.contains_key(&n.id)
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

fn plot_boundary(
    resident: &ResidentChunks,
    center: GeoPosition,
    radius: f64,
    color: [f32; 4],
) -> MeshBuilder {
    let mut builder = MeshBuilder::default();
    if !radius.is_finite() || !(1.0..=1000.).contains(&radius) {
        return builder;
    }
    let frame = LocalFrame::new(center);
    let segments = ((radius * std::f64::consts::TAU / 4.).ceil() as usize).clamp(64, 512);
    let point = |angle: f64, distance: f64| [angle.cos() * distance, 0., angle.sin() * distance];
    for index in 0..segments {
        let a = index as f64 * std::f64::consts::TAU / segments as f64;
        let b = (index + 1) as f64 * std::f64::consts::TAU / segments as f64;
        if let Some([a0, b0, b1, a1]) = detail::terrain_boundary_quad(
            resident,
            &frame,
            [
                point(a, radius - 0.3),
                point(b, radius - 0.3),
                point(b, radius + 0.3),
                point(a, radius + 0.3),
            ],
            0.35,
        ) {
            builder.quad(a0, b0, b1, a1, color);
        }
    }
    builder
}

fn render_sandbox(
    mut commands: Commands,
    mut state: ResMut<RenderState>,
    runtime: Res<WorldRuntime>,
    sandbox: Res<SandboxRuntime>,
    resident: Res<ResidentChunks>,
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
    let terrain_signature = if sandbox.state.plots.is_empty() {
        0
    } else {
        detail::resident_terrain_signature(&resident)
    };
    let terrain_changed = state.sandbox_terrain_signature != terrain_signature;
    let visible = sandbox
        .state
        .resource_nodes
        .values()
        .filter(|n| {
            n.source_feature.is_none()
                && !sandbox.state.ecology.resources.contains_key(&n.id)
                && n.remaining > 0
        })
        .map(|n| n.id.clone())
        .collect::<BTreeSet<_>>();
    if state.initialized
        && state.revision == sandbox.revision
        && state.sandbox_focus == focus
        && state.authored_visible == visible
        && !terrain_changed
    {
        return;
    }
    let fingerprint = sandbox_visual_fingerprint(&sandbox, runtime.focus);
    state.sandbox_focus = focus;
    state.authored_visible = visible;
    state.revision = sandbox.revision;
    state.sandbox_terrain_signature = terrain_signature;
    if state.initialized && state.sandbox_fingerprint == fingerprint && !terrain_changed {
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
        let b = plot_boundary(&resident, plot.center, plot.radius_m, color);
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
                && !sandbox.state.ecology.resources.contains_key(&n.id)
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
    mut exposures: Query<&mut bevy::camera::Exposure, With<Camera3d>>,
    state: Res<RenderState>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    quality: Res<RenderQualitySettings>,
    time: Res<Time>,
    mut water_phase: Local<Vec2>,
    capabilities: Res<RenderCapabilities>,
) {
    let atmospheric_sky = quality.uses_atmosphere() && capabilities.atmospheric_sky;
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
        light.illuminance = if atmospheric_sky {
            lux::RAW_SUNLIGHT
        } else {
            24000.
        } * daylight
            * (1. - weather.cloud_cover * 0.65);
        light.color = Color::srgb(1., 0.86 + daylight * 0.12, 0.72 + daylight * 0.25);
    }
    ambient.brightness = if atmospheric_sky {
        45. + daylight * 120.
    } else {
        45. + daylight * 605.
    };
    ambient.color = Color::srgb(0.64 + daylight * 0.10, 0.74 + daylight * 0.08, 0.94);
    for mut exposure in &mut exposures {
        // Keep night exploration readable without flattening daytime albedo.
        exposure.ev100 = 9.7 + daylight * if atmospheric_sky { 3.3 } else { 1.1 };
    }
    let cloud = weather.cloud_cover;
    let sky = Color::srgb(
        0.018 + daylight * (0.29 + cloud * 0.16),
        0.027 + daylight * (0.47 + cloud * 0.06),
        0.050 + daylight * (0.62 - cloud * 0.025),
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
        fog.color = Color::srgb(
            0.035 + daylight * (0.46 + cloud * 0.10),
            0.045 + daylight * (0.56 + cloud * 0.06),
            0.065 + daylight * (0.63 + cloud * 0.02),
        );
        fog.directional_light_color = Color::srgba(1., 0.86, 0.65, 0.16 * daylight);
        fog.directional_light_exponent = 12.;
        fog.falloff = FogFalloff::Linear {
            start: local_span * 0.64 / (1. + weather.precipitation_mm_h * 0.18),
            end: local_span * 1.16 / (1. + weather.precipitation_mm_h * 0.12),
        };
    }
    if !runtime.clock.paused && runtime.clock.time_scale > 0. {
        *water_phase += Vec2::new(0.015, 0.00915) * time.delta_secs().min(0.1);
        water_phase.x = water_phase.x.rem_euclid(1.);
        water_phase.y = water_phase.y.rem_euclid(1.);
    }
    if let Some(handle) = state.materials.get(&Layer::Water) {
        if let Some(mut material) = materials.get_mut(handle) {
            material.uv_transform = Affine2::from_translation(*water_phase);
        }
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
    fn mesh_source(height: f32) -> Arc<WorldChunk> {
        let key = TileKey::at(GeoPosition::new(48.137, 11.575, 0.).unwrap(), 15).unwrap();
        Arc::new(WorldChunk {
            key,
            heights: terra_core::HeightField::constant(height),
            features: vec![],
            sources: vec![],
            warnings: vec![],
        })
    }

    fn mesh_source_with_trees(height: f32, ids: &[&str]) -> Arc<WorldChunk> {
        let mut source = mesh_source(height);
        let data = Arc::make_mut(&mut source);
        data.features = ids
            .iter()
            .map(|id| terra_core::WorldFeature {
                id: (*id).into(),
                kind: terra_core::FeatureKind::Tree,
                points: vec![data.key.center()],
                height_m: 8.,
                properties: BTreeMap::new(),
            })
            .collect();
        source
    }

    fn completed_mesh_job(source: Arc<WorldChunk>, revision: u64, detail: u8) -> ChunkMeshJob {
        completed_mesh_job_with_edits(source, revision, detail, Arc::default())
    }

    fn completed_mesh_job_with_edits(
        source: Arc<WorldChunk>,
        revision: u64,
        detail: u8,
        removed: Arc<BTreeSet<String>>,
    ) -> ChunkMeshJob {
        let (sender, receiver) = mpsc::sync_channel(1);
        let mut layers = feature_meshes_lod(&source, 384, &removed, detail);
        layers.insert(Layer::Terrain, terrain_mesh_lod(&source, detail).unwrap());
        sender.send(layers).unwrap();
        ChunkMeshJob {
            source,
            revision,
            detail,
            removed,
            receiver: Mutex::new(receiver),
        }
    }

    /// Exercise the real integration system without a window, GPU or worker timing.
    fn mesh_completion_app(source: Arc<WorldChunk>, job: ChunkMeshJob) -> App {
        let key = source.key;
        let config = crate::WorldConfig {
            focus: key.center(),
            ..Default::default()
        };
        let clock = terra_core::WorldClock::default();
        let settings = RenderQualitySettings::default();
        let mut app = App::new();
        app.insert_resource(WorldRuntime {
            focus: config.focus,
            frame: LocalFrame::new(config.focus),
            weather: terra_core::synthetic_weather(&config.focus, &clock, config.seed),
            clock,
            config,
            origin_revision: 0,
            stream_stats: Default::default(),
            last_error: None,
            stream_poll_ms: 0.,
            simulation_ms: 0.,
            rebases: 0,
            network_authority: false,
        })
        .insert_resource(ResidentChunks {
            level: key.level,
            chunks: BTreeMap::from([(key, source)]),
        })
        .insert_resource(SandboxRuntime::new(key.center()))
        .insert_resource(RenderState {
            quality: Some((settings.quality, settings.terrain_lod)),
            ..Default::default()
        })
        .insert_resource(MeshTasks {
            jobs: BTreeMap::from([(key, job)]),
            ..Default::default()
        })
        .insert_resource(settings)
        .init_resource::<RenderQueue>()
        .init_resource::<RenderTelemetry>()
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<StandardMaterial>>()
        .add_systems(Update, render_chunks);
        app
    }

    fn failed_mesh_job(source: Arc<WorldChunk>, revision: u64, poison: bool) -> ChunkMeshJob {
        let (sender, receiver) = mpsc::sync_channel(1);
        drop(sender);
        let receiver = Mutex::new(receiver);
        if poison {
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _guard = receiver.lock().unwrap();
                panic!("injected receiver poison");
            }));
        }
        ChunkMeshJob {
            source,
            revision,
            detail: 0,
            removed: Arc::default(),
            receiver,
        }
    }

    #[test]
    fn mesh_failure_preserves_dirty_edit_and_old_geometry_until_bounded_recovery() {
        for poison in [false, true] {
            let source = mesh_source_with_trees(120., &["edited-tree"]);
            let key = source.key;
            let mut app = mesh_completion_app(
                Arc::clone(&source),
                completed_mesh_job(Arc::clone(&source), 0, 0),
            );
            app.update();
            let old_mesh = app.world().resource::<RenderState>().chunks[&key][0]
                .1
                .clone();
            app.world_mut()
                .resource_mut::<SandboxRuntime>()
                .state
                .feature_edits
                .insert("edited-tree".into());
            // Real dispatch consumes dirty work even though the old LOD matches.
            app.update();
            assert!(!app.world().resource::<MeshTasks>().pending.contains(&key));
            assert_eq!(app.world().resource::<MeshTasks>().jobs[&key].revision, 0);
            let removed = Arc::clone(&app.world().resource::<MeshTasks>().jobs[&key].removed);
            assert_eq!(*removed, BTreeSet::from(["edited-tree".into()]));
            let mut failed = failed_mesh_job(Arc::clone(&source), 0, poison);
            failed.removed = Arc::clone(&removed);
            app.world_mut()
                .resource_mut::<MeshTasks>()
                .jobs
                .insert(key, failed);
            let before = Instant::now();
            app.update();
            let tasks = app.world().resource::<MeshTasks>();
            assert!(tasks.pending.contains(&key));
            assert!(tasks.jobs.is_empty());
            assert!(tasks.failures[&key].not_before >= before + Duration::from_millis(500));
            assert_eq!(tasks.failures[&key].attempts, 1);
            assert_eq!(
                app.world().resource::<RenderState>().chunks[&key][0].1,
                old_mesh
            );
            let telemetry = app.world().resource::<RenderTelemetry>();
            assert_eq!((telemetry.retrying_meshes, telemetry.mesh_failures), (1, 1));
            assert!(
                telemetry
                    .last_mesh_error
                    .as_deref()
                    .unwrap()
                    .contains(if poison { "poisoned" } else { "disconnected" })
            );
            // A forced future deadline makes no-sleep repeated-frame checks deterministic.
            app.world_mut()
                .resource_mut::<MeshTasks>()
                .failures
                .get_mut(&key)
                .unwrap()
                .not_before = Instant::now() + Duration::from_secs(3600);
            for _ in 0..4 {
                app.update();
            }
            assert!(app.world().resource::<MeshTasks>().jobs.is_empty());
            assert_eq!(app.world().resource::<MeshTasks>().failure_count, 1);
            app.world_mut()
                .resource_mut::<MeshTasks>()
                .failures
                .get_mut(&key)
                .unwrap()
                .not_before = Instant::now() - Duration::from_secs(1);
            app.update();
            assert_eq!(app.world().resource::<MeshTasks>().jobs.len(), 1);
            app.world_mut().resource_mut::<MeshTasks>().jobs.insert(
                key,
                completed_mesh_job_with_edits(Arc::clone(&source), 0, 0, removed),
            );
            app.update();
            let tasks = app.world().resource::<MeshTasks>();
            assert!(tasks.pending.is_empty() && tasks.jobs.is_empty() && tasks.failures.is_empty());
            assert_ne!(
                app.world().resource::<RenderState>().chunks[&key][0].1,
                old_mesh
            );
            assert_eq!(app.world().resource::<RenderTelemetry>().retrying_meshes, 0);
        }
    }

    #[test]
    fn mesh_failure_backoff_caps_and_new_source_quality_or_edits_retry_immediately() {
        assert_eq!(mesh_retry_delay(1), Duration::from_millis(500));
        assert_eq!(mesh_retry_delay(2), Duration::from_secs(1));
        assert_eq!(mesh_retry_delay(7), Duration::from_secs(30));
        assert_eq!(mesh_retry_delay(u32::MAX), Duration::from_secs(30));
        for context in ["source", "quality", "edits", "lod", "eviction"] {
            let source = mesh_source_with_trees(120., &["edited-tree"]);
            let key = source.key;
            let mut app = mesh_completion_app(
                Arc::clone(&source),
                failed_mesh_job(Arc::clone(&source), 0, false),
            );
            app.update();
            app.world_mut()
                .resource_mut::<MeshTasks>()
                .failures
                .get_mut(&key)
                .unwrap()
                .not_before = Instant::now() + Duration::from_secs(3600);
            match context {
                "source" => {
                    app.world_mut()
                        .resource_mut::<ResidentChunks>()
                        .chunks
                        .insert(key, mesh_source(140.));
                }
                "quality" => {
                    app.world_mut()
                        .resource_mut::<RenderQualitySettings>()
                        .terrain_lod = false;
                }
                "edits" => {
                    app.world_mut()
                        .resource_mut::<SandboxRuntime>()
                        .state
                        .feature_edits
                        .insert("edited-tree".into());
                }
                "lod" => {
                    let focus = LocalFrame::new(source.key.center())
                        .to_geo([3_000., 0., 0.])
                        .unwrap();
                    app.world_mut().resource_mut::<WorldRuntime>().focus = focus;
                }
                _ => {
                    app.world_mut()
                        .resource_mut::<ResidentChunks>()
                        .chunks
                        .clear();
                }
            }
            app.update();
            let tasks = app.world().resource::<MeshTasks>();
            assert!(tasks.failures.is_empty());
            if context == "eviction" {
                assert!(tasks.jobs.is_empty() && tasks.pending.is_empty());
            } else {
                assert_eq!(tasks.jobs.len(), 1);
            }
        }
    }

    #[test]
    fn repeated_mesh_failure_increases_backoff_without_losing_pending_work() {
        let source = mesh_source(120.);
        let key = source.key;
        let mut app = mesh_completion_app(
            Arc::clone(&source),
            failed_mesh_job(Arc::clone(&source), 0, false),
        );
        app.update();
        for attempt in 2..=8 {
            app.world_mut()
                .resource_mut::<MeshTasks>()
                .failures
                .get_mut(&key)
                .unwrap()
                .not_before = Instant::now() - Duration::from_secs(1);
            app.update();
            app.world_mut()
                .resource_mut::<MeshTasks>()
                .jobs
                .insert(key, failed_mesh_job(Arc::clone(&source), 0, false));
            let before = Instant::now();
            app.update();
            let tasks = app.world().resource::<MeshTasks>();
            assert!(tasks.jobs.is_empty() && tasks.pending.contains(&key));
            assert_eq!(tasks.failures[&key].attempts, attempt);
            assert!(tasks.failures[&key].not_before >= before + mesh_retry_delay(attempt));
            assert_eq!(tasks.failure_count, u64::from(attempt));
        }
    }

    fn complete_dispatched_meshes(app: &mut App) {
        let ready: Vec<_> = app
            .world()
            .resource::<MeshTasks>()
            .jobs
            .iter()
            .map(|(&key, job)| {
                (
                    key,
                    completed_mesh_job_with_edits(
                        Arc::clone(&job.source),
                        job.revision,
                        job.detail,
                        Arc::clone(&job.removed),
                    ),
                )
            })
            .collect();
        for (key, job) in ready {
            app.world_mut()
                .resource_mut::<MeshTasks>()
                .jobs
                .insert(key, job);
        }
        app.update();
    }

    fn finish_mesh_work(app: &mut App) {
        for _ in 0..64 {
            let tasks = app.world().resource::<MeshTasks>();
            assert!(tasks.jobs.len() <= 2);
            if tasks.pending.is_empty() && tasks.jobs.is_empty() {
                return;
            }
            complete_dispatched_meshes(app);
        }
        panic!("bounded mesh fixture did not finish");
    }

    fn neighborhood_mesh_app(shared: bool) -> (App, TileKey, TileKey) {
        let center = mesh_source(120.).key;
        let next = TileKey::new(center.level, center.x + 1, center.y).unwrap();
        let sources: BTreeMap<_, _> = center
            .neighbors(3)
            .into_iter()
            .map(|key| {
                let id = if shared && [center, next].contains(&key) {
                    "shared-border-tree".to_string()
                } else {
                    format!("tree:{}:{}:{}", key.level, key.x, key.y)
                };
                let mut source = mesh_source_with_trees(120., &[&id]);
                let data = Arc::make_mut(&mut source);
                data.key = key;
                data.features[0].points = vec![key.center()];
                (key, source)
            })
            .collect();
        assert_eq!(sources.len(), 49);
        let source = Arc::clone(&sources[&center]);
        let mut app = mesh_completion_app(Arc::clone(&source), completed_mesh_job(source, 0, 0));
        app.world_mut().resource_mut::<ResidentChunks>().chunks = sources;
        app.update();
        finish_mesh_work(&mut app);
        assert_eq!(app.world().resource::<RenderState>().chunks.len(), 49);
        (app, center, next)
    }

    fn has_chunk_layer(app: &App, key: TileKey, layer: Layer) -> bool {
        app.world().resource::<RenderState>().chunks[&key]
            .iter()
            .any(|(entity, _)| app.world().get::<LayerVisual>(*entity).unwrap().0 == layer)
    }

    #[test]
    fn mesh_edit_work_is_one_of_49_resident_pages_while_quality_changes_remain_global() {
        let (mut app, center, _) = neighborhood_mesh_app(false);
        let original = app.world().resource::<RenderState>().chunks.clone();
        let tree = app.world().resource::<ResidentChunks>().chunks[&center].features[0]
            .id
            .clone();
        app.world_mut()
            .resource_mut::<SandboxRuntime>()
            .state
            .feature_edits
            .insert(tree);
        app.update();
        let tasks = app.world().resource::<MeshTasks>();
        assert_eq!(
            tasks.revision, 0,
            "feature edits do not invalidate other pages"
        );
        assert_eq!(tasks.jobs.keys().copied().collect::<Vec<_>>(), vec![center]);
        assert!(tasks.pending.is_empty());
        for (&key, handles) in &original {
            assert_eq!(&app.world().resource::<RenderState>().chunks[&key], handles);
        }
        println!(
            "49 resident source pages: one-feature edit schedules 1 job, 0 additional pending pages; 48 pages retain their meshes"
        );
        finish_mesh_work(&mut app);
        assert!(!has_chunk_layer(&app, center, Layer::Vegetation));
        for (&key, handles) in &original {
            if key != center {
                assert_eq!(&app.world().resource::<RenderState>().chunks[&key], handles);
            }
        }
        app.world_mut()
            .resource_mut::<SandboxRuntime>()
            .state
            .feature_edits
            .insert("nonresident-feature".into());
        app.update();
        assert!(app.world().resource::<MeshTasks>().jobs.is_empty());
        assert!(app.world().resource::<MeshTasks>().pending.is_empty());

        app.world_mut()
            .resource_mut::<RenderQualitySettings>()
            .quality = RenderQuality::High;
        app.update();
        let tasks = app.world().resource::<MeshTasks>();
        assert_eq!(tasks.revision, 1);
        assert_eq!(tasks.jobs.len(), 2);
        assert_eq!(tasks.pending.len(), 47);
        println!(
            "49 resident source pages: quality change schedules 2 jobs and retains 47 pending pages under the two-worker limit"
        );
        finish_mesh_work(&mut app);
    }

    #[test]
    fn mesh_edits_cover_shared_feature_fragments_on_depletion_and_regrowth() {
        let (mut app, center, next) = neighborhood_mesh_app(true);
        let source = Arc::clone(&app.world().resource::<ResidentChunks>().chunks[&center]);
        let original = app.world().resource::<RenderState>().chunks.clone();
        app.world_mut()
            .resource_mut::<SandboxRuntime>()
            .state
            .register_chunk(&source);
        for remaining in [0, 1] {
            app.world_mut()
                .resource_mut::<SandboxRuntime>()
                .state
                .resource_nodes
                .get_mut("shared-border-tree")
                .unwrap()
                .remaining = remaining;
            app.update();
            let tasks = app.world().resource::<MeshTasks>();
            assert_eq!(
                tasks.jobs.keys().copied().collect::<BTreeSet<_>>(),
                BTreeSet::from([center, next])
            );
            assert!(tasks.pending.is_empty());
            finish_mesh_work(&mut app);
            assert_eq!(
                has_chunk_layer(&app, center, Layer::Vegetation),
                remaining > 0
            );
            assert_eq!(
                has_chunk_layer(&app, next, Layer::Vegetation),
                remaining > 0
            );
            for (&key, handles) in &original {
                if ![center, next].contains(&key) {
                    assert_eq!(&app.world().resource::<RenderState>().chunks[&key], handles);
                }
            }
        }
        println!(
            "49 resident source pages: shared feature depletion and regrowth each rebuild 2 fragments, preserving 47 unrelated pages"
        );
    }

    #[test]
    fn mesh_completion_accepts_unrelated_work_when_another_page_is_edited() {
        let (mut app, center, next) = neighborhood_mesh_app(false);
        let original = app.world().resource::<RenderState>().chunks[&next].clone();
        let source = Arc::clone(&app.world().resource::<ResidentChunks>().chunks[&next]);
        let detail = app.world().resource::<RenderState>().detail_levels[&next];
        app.world_mut()
            .resource_mut::<MeshTasks>()
            .jobs
            .insert(next, completed_mesh_job(source, 0, detail));
        let tree = app.world().resource::<ResidentChunks>().chunks[&center].features[0]
            .id
            .clone();
        app.world_mut()
            .resource_mut::<SandboxRuntime>()
            .state
            .feature_edits
            .insert(tree);
        app.update();
        assert_ne!(
            app.world().resource::<RenderState>().chunks[&next],
            original,
            "the unrelated current completion must integrate"
        );
        assert_eq!(
            app.world()
                .resource::<MeshTasks>()
                .jobs
                .keys()
                .copied()
                .collect::<Vec<_>>(),
            vec![center]
        );
        assert!(app.world().resource::<MeshTasks>().pending.is_empty());
        finish_mesh_work(&mut app);
    }

    #[test]
    fn mesh_completion_rejects_edits_changed_during_work_and_overlay_restore() {
        for restore in [false, true] {
            let source = mesh_source_with_trees(120., &["tree-a", "tree-b"]);
            let key = source.key;
            let mut app = mesh_completion_app(
                Arc::clone(&source),
                completed_mesh_job(Arc::clone(&source), 0, 0),
            );
            app.update();
            let original = app.world().resource::<RenderState>().chunks[&key].clone();
            app.world_mut()
                .resource_mut::<SandboxRuntime>()
                .state
                .feature_edits
                .insert("tree-a".into());
            app.update();
            let old_removed = Arc::clone(&app.world().resource::<MeshTasks>().jobs[&key].removed);
            let (sender, receiver) = mpsc::sync_channel(1);
            app.world_mut()
                .resource_mut::<MeshTasks>()
                .jobs
                .get_mut(&key)
                .unwrap()
                .receiver = Mutex::new(receiver);
            if restore {
                let restored = SandboxRuntime::new(key.center());
                *app.world_mut().resource_mut::<SandboxRuntime>() = restored;
            } else {
                app.world_mut()
                    .resource_mut::<SandboxRuntime>()
                    .state
                    .feature_edits
                    .insert("tree-b".into());
            }
            app.update();
            assert!(app.world().resource::<MeshTasks>().pending.contains(&key));
            assert_eq!(app.world().resource::<RenderState>().chunks[&key], original);
            let mut stale_layers = feature_meshes_lod(&source, 384, &old_removed, 0);
            stale_layers.insert(Layer::Terrain, terrain_mesh_lod(&source, 0).unwrap());
            sender.send(stale_layers).unwrap();
            app.update();
            assert_eq!(
                app.world().resource::<RenderState>().chunks[&key],
                original,
                "stale edited geometry must not replace the last valid mesh"
            );
            let expected = if restore {
                BTreeSet::new()
            } else {
                BTreeSet::from(["tree-a".into(), "tree-b".into()])
            };
            assert_eq!(
                *app.world().resource::<MeshTasks>().jobs[&key].removed,
                expected
            );
            finish_mesh_work(&mut app);
            assert_eq!(has_chunk_layer(&app, key, Layer::Vegetation), restore);
            assert!(app.world().resource::<MeshTasks>().pending.is_empty());
        }
    }

    #[test]
    fn mesh_completion_satisfies_a_queued_edit_that_returns_to_the_same_context() {
        let source = mesh_source_with_trees(120., &["tree-a", "tree-b"]);
        let key = source.key;
        let mut app = mesh_completion_app(
            Arc::clone(&source),
            completed_mesh_job(Arc::clone(&source), 0, 0),
        );
        app.update();
        app.world_mut()
            .resource_mut::<SandboxRuntime>()
            .state
            .feature_edits
            .insert("tree-a".into());
        app.update();
        let removed = Arc::clone(&app.world().resource::<MeshTasks>().jobs[&key].removed);
        let (sender, receiver) = mpsc::sync_channel(1);
        app.world_mut()
            .resource_mut::<MeshTasks>()
            .jobs
            .get_mut(&key)
            .unwrap()
            .receiver = Mutex::new(receiver);
        app.world_mut()
            .resource_mut::<SandboxRuntime>()
            .state
            .feature_edits
            .insert("tree-b".into());
        app.update();
        assert!(app.world().resource::<MeshTasks>().pending.contains(&key));
        app.world_mut()
            .resource_mut::<SandboxRuntime>()
            .state
            .feature_edits
            .remove("tree-b");
        app.update();
        let mut layers = feature_meshes_lod(&source, 384, &removed, 0);
        layers.insert(Layer::Terrain, terrain_mesh_lod(&source, 0).unwrap());
        sender.send(layers).unwrap();
        app.update();
        let tasks = app.world().resource::<MeshTasks>();
        assert!(tasks.pending.is_empty() && tasks.jobs.is_empty());
        assert!(has_chunk_layer(&app, key, Layer::Vegetation));
    }

    #[test]
    fn unrelated_edits_preserve_mesh_failure_backoff_and_exact_edit_change_resets_it() {
        let (mut app, center, next) = neighborhood_mesh_app(false);
        let source = Arc::clone(&app.world().resource::<ResidentChunks>().chunks[&center]);
        app.world_mut()
            .resource_mut::<MeshTasks>()
            .jobs
            .insert(center, failed_mesh_job(Arc::clone(&source), 0, false));
        app.update();
        let deadline = Instant::now() + Duration::from_secs(3600);
        app.world_mut()
            .resource_mut::<MeshTasks>()
            .failures
            .get_mut(&center)
            .unwrap()
            .not_before = deadline;
        let other_tree = app.world().resource::<ResidentChunks>().chunks[&next].features[0]
            .id
            .clone();
        app.world_mut()
            .resource_mut::<SandboxRuntime>()
            .state
            .feature_edits
            .insert(other_tree);
        app.update();
        let tasks = app.world().resource::<MeshTasks>();
        assert_eq!(tasks.failures[&center].not_before, deadline);
        assert_eq!(tasks.failures[&center].attempts, 1);
        assert!(!tasks.jobs.contains_key(&center));
        assert!(tasks.pending.contains(&center));
        assert!(tasks.jobs.contains_key(&next));
        let own_tree = source.features[0].id.clone();
        app.world_mut()
            .resource_mut::<SandboxRuntime>()
            .state
            .feature_edits
            .insert(own_tree);
        app.update();
        let tasks = app.world().resource::<MeshTasks>();
        assert!(!tasks.failures.contains_key(&center));
        assert!(tasks.jobs.contains_key(&center));
        assert!(tasks.jobs.len() <= 2);
        finish_mesh_work(&mut app);
    }

    #[test]
    fn mesh_completion_accepts_its_current_source_instance() {
        let source = mesh_source(120.);
        let key = source.key;
        let mut app = mesh_completion_app(
            Arc::clone(&source),
            completed_mesh_job(Arc::clone(&source), 0, 0),
        );
        app.update();
        assert_eq!(app.world().resource::<RenderTelemetry>().chunks, 1);
        assert_eq!(
            app.world().resource::<RenderTelemetry>().integrated_chunks,
            BTreeSet::from([key]),
        );
        assert!(
            app.world()
                .resource::<RenderState>()
                .chunks
                .contains_key(&key)
        );
        assert!(app.world().resource::<MeshTasks>().jobs.is_empty());
        assert!(app.world().resource::<MeshTasks>().pending.is_empty());
    }

    #[test]
    fn mesh_readiness_tracks_current_integrated_source_replacement_and_eviction() {
        let old = mesh_source(10.);
        let current = Arc::new((*old).clone());
        let key = old.key;
        let mut app = mesh_completion_app(Arc::clone(&old), completed_mesh_job(old, 0, 0));
        app.update();
        assert_eq!(
            app.world().resource::<RenderTelemetry>().integrated_chunks,
            BTreeSet::from([key]),
        );
        let original = app.world().resource::<RenderState>().chunks[&key].clone();

        app.world_mut()
            .resource_mut::<ResidentChunks>()
            .chunks
            .insert(key, Arc::clone(&current));
        app.update();
        // Old geometry stays visible while the replacement builds, but it is
        // not evidence that the new immutable source is ready for gameplay.
        let telemetry = app.world().resource::<RenderTelemetry>();
        assert_eq!(telemetry.chunks, 1);
        assert!(telemetry.integrated_chunks.is_empty());
        assert_eq!(app.world().resource::<RenderState>().chunks[&key], original);

        app.world_mut()
            .resource_mut::<MeshTasks>()
            .jobs
            .insert(key, completed_mesh_job(current, 0, 0));
        app.update();
        assert_eq!(
            app.world().resource::<RenderTelemetry>().integrated_chunks,
            BTreeSet::from([key]),
        );
        app.world_mut()
            .resource_mut::<ResidentChunks>()
            .chunks
            .clear();
        app.update();
        let telemetry = app.world().resource::<RenderTelemetry>();
        assert_eq!(telemetry.chunks, 0);
        assert!(telemetry.integrated_chunks.is_empty());
        assert!(
            app.world()
                .resource::<RenderState>()
                .chunk_sources
                .is_empty()
        );
    }

    #[test]
    fn mesh_completion_rejects_replaced_source_and_requeues_its_current_page() {
        use bevy::mesh::VertexAttributeValues;
        let old = mesh_source(10.);
        let current = mesh_source(120.);
        let key = current.key;
        let mut app = mesh_completion_app(Arc::clone(&current), completed_mesh_job(old, 0, 0));
        app.update();
        assert_eq!(app.world().resource::<RenderTelemetry>().chunks, 0);
        assert!(app.world().resource::<Assets<Mesh>>().is_empty());
        let retry = &app.world().resource::<MeshTasks>().jobs[&key];
        assert!(Arc::ptr_eq(&retry.source, &current));
        assert_eq!(app.world().resource::<MeshTasks>().jobs.len(), 1);

        // Replace the asynchronously launched retry with an explicitly ready
        // completion, so acceptance never depends on thread scheduling.
        app.world_mut()
            .resource_mut::<MeshTasks>()
            .jobs
            .insert(key, completed_mesh_job(Arc::clone(&current), 0, 0));
        app.update();
        assert_eq!(app.world().resource::<RenderTelemetry>().chunks, 1);
        let state = app.world().resource::<RenderState>();
        let mesh = app
            .world()
            .resource::<Assets<Mesh>>()
            .get(&state.chunks[&key][0].1)
            .unwrap();
        let VertexAttributeValues::Float32x3(points) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap()
        else {
            panic!("terrain position attribute missing")
        };
        assert!(points.iter().all(|point| point[1] > 100.));
    }

    #[test]
    fn mesh_completion_requires_source_identity_even_when_content_matches() {
        let old = mesh_source(10.);
        let current = Arc::new((*old).clone());
        let key = current.key;
        let mut app = mesh_completion_app(Arc::clone(&current), completed_mesh_job(old, 0, 0));
        app.update();
        assert_eq!(app.world().resource::<RenderTelemetry>().chunks, 0);
        assert!(Arc::ptr_eq(
            &app.world().resource::<MeshTasks>().jobs[&key].source,
            &current,
        ));
    }

    #[test]
    fn mesh_completion_retains_revision_lod_and_residency_admission() {
        for (revision, detail) in [(1, 0), (0, 1)] {
            let source = mesh_source(10.);
            let key = source.key;
            let mut app = mesh_completion_app(
                Arc::clone(&source),
                completed_mesh_job(source, revision, detail),
            );
            app.update();
            assert_eq!(app.world().resource::<RenderTelemetry>().chunks, 0);
            let retry = &app.world().resource::<MeshTasks>().jobs[&key];
            assert_eq!((retry.revision, retry.detail), (0, 0));
        }
        let source = mesh_source(10.);
        let mut app = mesh_completion_app(Arc::clone(&source), completed_mesh_job(source, 0, 0));
        app.world_mut()
            .resource_mut::<ResidentChunks>()
            .chunks
            .clear();
        app.update();
        assert_eq!(app.world().resource::<RenderTelemetry>().chunks, 0);
        assert!(app.world().resource::<MeshTasks>().jobs.is_empty());
        assert!(app.world().resource::<MeshTasks>().pending.is_empty());
    }

    #[test]
    fn plot_boundaries_drape_to_updated_heightmaps_and_never_join_missing_pages() {
        use bevy::mesh::VertexAttributeValues;
        use std::sync::Arc;
        use terra_core::{HeightField, WorldChunk};
        let key = TileKey::at(GeoPosition::new(48.137, 11.575, 0.).unwrap(), 15).unwrap();
        let mut center = key.center();
        center.altitude_m = 10.;
        let frame = LocalFrame::new(center);
        let mut resident = ResidentChunks {
            level: 15,
            chunks: BTreeMap::new(),
        };
        let unloaded = detail::resident_terrain_signature(&resident);
        assert!(
            plot_boundary(&resident, center, 40., [1.; 4])
                .finish()
                .is_none()
        );
        resident.chunks.insert(
            key,
            Arc::new(WorldChunk {
                key,
                heights: HeightField {
                    width: 2,
                    height: 2,
                    samples_m: vec![10., 80., 40., 110.],
                },
                features: vec![],
                sources: vec![],
                warnings: vec![],
            }),
        );
        let loaded = detail::resident_terrain_signature(&resident);
        assert_ne!(unloaded, loaded);
        let mesh = plot_boundary(&resident, center, 40., [1.; 4])
            .finish()
            .unwrap();
        let VertexAttributeValues::Float32x3(points) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap()
        else {
            panic!()
        };
        assert_eq!(
            points.len(),
            64 * 6,
            "the complete loaded 40 m plot should remain a complete ring"
        );
        let min_height = points.iter().map(|p| p[1]).fold(f32::INFINITY, f32::min);
        let max_height = points
            .iter()
            .map(|p| p[1])
            .fold(f32::NEG_INFINITY, f32::max);
        assert!(
            max_height - min_height > 4.,
            "the boundary stayed at a single elevation"
        );
        for point in points {
            let geo = frame.to_geo(point.map(f64::from)).unwrap();
            let height = crate::WorldEnvironment::terrain_height(&resident, geo).unwrap();
            assert!((geo.altitude_m - height - 0.35).abs() < 0.002);
        }
        let mut replacement = (**resident.chunks.get(&key).unwrap()).clone();
        replacement.heights = HeightField::constant(100.);
        resident.chunks.insert(key, Arc::new(replacement));
        assert_ne!(loaded, detail::resident_terrain_signature(&resident));
        let mesh = plot_boundary(&resident, center, 40., [1.; 4])
            .finish()
            .unwrap();
        let VertexAttributeValues::Float32x3(points) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap()
        else {
            panic!()
        };
        assert!(points.iter().all(|p| p[1] > 90.3));

        // Two loaded endpoints across a missing narrow page must not create a
        // bridge. Put this fixture at the date line to cover wrapped tile X.
        let level = 19;
        let side = 1_u32 << level;
        let row = TileKey::at(GeoPosition::new(88., 0., 0.).unwrap(), level)
            .unwrap()
            .y;
        let middle = TileKey::new(level, 0, row).unwrap();
        let left = TileKey::new(level, side - 1, row).unwrap();
        let right = TileKey::new(level, 1, row).unwrap();
        let frame = LocalFrame::new(middle.center());
        let a = frame.to_local(left.center());
        let b = frame.to_local(right.center());
        let corners = [
            [a[0], 0., a[2] - 0.2],
            [b[0], 0., b[2] - 0.2],
            [b[0], 0., b[2] + 0.2],
            [a[0], 0., a[2] + 0.2],
        ];
        resident = ResidentChunks {
            level,
            chunks: BTreeMap::new(),
        };
        let chunk = |key| {
            Arc::new(WorldChunk {
                key,
                heights: HeightField::constant(100.),
                features: vec![],
                sources: vec![],
                warnings: vec![],
            })
        };
        resident.chunks.insert(left, chunk(left));
        resident.chunks.insert(right, chunk(right));
        assert!(detail::terrain_boundary_quad(&resident, &frame, corners, 0.35).is_none());
        resident.chunks.insert(middle, chunk(middle));
        assert!(detail::terrain_boundary_quad(&resident, &frame, corners, 0.35).is_some());
    }

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
        let mut terrain_before = 0usize;
        let mut terrain_after = 0usize;
        let mut tiers = [0usize; 3];
        for key in key.neighbors(3) {
            let detail = feature_detail(distance_to_tile_m(p, key), None);
            tiers[detail as usize] += 1;
            let chunk = terra_stream::procedural_chunk(key, 65, 42);
            terrain_before += 65 * 65;
            let terrain = terrain_mesh_lod(&chunk, detail).unwrap().count_vertices();
            terrain_after += terrain;
            vertices += terrain;
            for mesh in feature_meshes_lod(&chunk, 128, &BTreeSet::new(), detail).values() {
                let VertexAttributeValues::Float32x3(points) =
                    mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap()
                else {
                    panic!()
                };
                vertices += points.len();
            }
        }
        println!(
            "49 source chunks: {vertices} vertices; near/mid/far chunks {tiers:?}; terrain {terrain_after}/{terrain_before} vertices"
        );
        assert_eq!(tiers.iter().sum::<usize>(), 49);
        assert!(
            vertices < 2_000_000,
            "source view exceeded its default geometry budget: {vertices}"
        );
        assert!(
            terrain_after < terrain_before / 2,
            "distant interiors must reduce the default terrain vertex workload"
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
