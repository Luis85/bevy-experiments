//! Read-only presentation of immutable simulation snapshots.
//!
//! Every living unit has one real GPU point. A bounded nearby set also receives
//! a category model, so zooming out never makes the population disappear. One
//! background task packs a published snapshot; render frames upload only a tiny
//! uniform and, when the camera moves, a small detail-model buffer.

mod geometry;
mod scenery;

use std::{collections::HashMap, sync::Arc, time::Instant};

use battle_core::{
    BRIDGE_HALF_WIDTH, MapKind, RIVER_HALF_WIDTH, Team, UnitCategory, UnitKind, arena_half_extent,
    terrain_height,
};
use bevy::{
    camera::visibility::NoFrustumCulling,
    light::GlobalAmbientLight,
    prelude::*,
    render::batching::NoAutomaticBatching,
    tasks::{AsyncComputeTaskPool, Task, block_on},
};

use crate::{
    app::{AppSet, Session},
    camera::{BattleCamera, BattleCameraSet},
    crowd::{CrowdFrame, CrowdInstances, InstanceData},
    runtime::BattleSnapshot,
};
use geometry::{Geometry, category_mesh, linear, point_mesh};

/// Detailed models have a bounded vertex cost, independent of army size.
const MAX_DETAIL_MODELS: usize = 6_000;
const DETAIL_RADIUS: f32 = 125.0;
const LOD_CELL_SIZE: f32 = 32.0;
const CELL_CANDIDATE_BUDGET: usize = 256;

pub struct BattleWorldPlugin;

impl Plugin for BattleWorldPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WorldRevision>()
            .init_resource::<RenderCounts>()
            .add_systems(Startup, setup_assets)
            .add_systems(
                Update,
                (rebuild_world, sync_crowd)
                    .chain()
                    .after(BattleCameraSet)
                    .in_set(AppSet::Presentation),
            );
    }
}

/// Submitted representations before frustum/depth/occlusion tests. Points also
/// remain beneath nearby models; represented_units counts each unit only once.
#[derive(Resource, Default, Debug)]
pub struct RenderCounts {
    pub represented_units: usize,
    pub detail_models: usize,
    pub point_only_units: usize,
    pub submitted_points: usize,
    pub draw_batches: usize,
    pub snapshot_bytes: usize,
    pub packing_ms: f32,
    pub generation: u64,
    pub source_tick: u64,
    pub packing: bool,
}

#[derive(Component)]
struct WorldEntity;

#[derive(Clone, Copy, PartialEq, Eq)]
enum BatchRole {
    Points,
    Details,
}

#[derive(Component)]
struct ArmyBatch {
    team: usize,
    role: BatchRole,
}

#[derive(Resource, Default)]
struct WorldRevision {
    generation: u64,
    upload_revision: u64,
    packed: Option<PackedSnapshot>,
    packing: Option<Task<PackedSnapshot>>,
    last_detail_view: Option<Transform>,
    last_detail_refresh: f64,
}

struct PackedSnapshot {
    generation: u64,
    tick: u64,
    elapsed_secs: f32,
    span_ticks: u32,
    categories: [UnitCategory; 2],
    points: [Arc<Vec<InstanceData>>; 2],
    buckets: [HashMap<(i32, i32), Vec<u32>>; 2],
    packing_ms: f32,
}

#[derive(Resource)]
struct WorldAssets {
    terrain: Handle<Mesh>,
    scenery: Handle<Mesh>,
    water: Handle<Mesh>,
    models: [[Handle<Mesh>; 3]; 7],
    points: [Handle<Mesh>; 7],
    ground_material: Handle<StandardMaterial>,
    scenery_material: Handle<StandardMaterial>,
    water_material: Handle<StandardMaterial>,
}

fn kind_index(kind: UnitKind) -> usize {
    match kind {
        UnitKind::Infantry => 0,
        UnitKind::Heavy => 1,
        UnitKind::Ranger => 2,
    }
}

fn setup_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let dummy = || Mesh::from(Cuboid::new(1.0, 0.1, 1.0));
    commands.insert_resource(WorldAssets {
        terrain: meshes.add(dummy()),
        scenery: meshes.add(dummy()),
        water: meshes.add(dummy()),
        models: UnitCategory::ALL
            .map(|category| UnitKind::ALL.map(|kind| meshes.add(category_mesh(category, kind)))),
        points: UnitCategory::ALL.map(|category| meshes.add(point_mesh(category))),
        ground_material: materials.add(StandardMaterial {
            base_color: Color::WHITE,
            perceptual_roughness: 0.98,
            ..default()
        }),
        scenery_material: materials.add(StandardMaterial {
            base_color: Color::WHITE,
            perceptual_roughness: 0.88,
            ..default()
        }),
        water_material: materials.add(StandardMaterial {
            base_color: Color::WHITE,
            perceptual_roughness: 0.34,
            metallic: 0.16,
            ..default()
        }),
    });
    commands.insert_resource(GlobalAmbientLight {
        color: Color::srgb(0.75, 0.83, 0.89),
        brightness: 340.0,
        ..default()
    });
    commands.spawn((
        Name::new("Late afternoon sun"),
        DirectionalLight {
            illuminance: 12_000.0,
            color: Color::srgb(1.0, 0.89, 0.73),
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_xyz(-55.0, 95.0, 35.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

#[allow(clippy::too_many_arguments)]
fn rebuild_world(
    mut commands: Commands,
    session: Res<Session>,
    mut revision: ResMut<WorldRevision>,
    mut counts: ResMut<RenderCounts>,
    assets: Res<WorldAssets>,
    mut meshes: ResMut<Assets<Mesh>>,
    old: Query<Entity, With<WorldEntity>>,
) {
    if revision.generation == session.generation {
        return;
    }
    let Some(battle) = &session.battle else {
        return;
    };
    for entity in &old {
        commands.entity(entity).despawn();
    }
    let extent = arena_half_extent(&battle.config);
    // Reuse these assets across scenario changes; category assets are shared.
    if let Some(mut mesh) = meshes.get_mut(&assets.terrain) {
        *mesh = terrain_mesh(battle.config.map, extent);
    }
    if let Some(mut mesh) = meshes.get_mut(&assets.scenery) {
        *mesh = scenery::scenery_mesh(&battle.config, extent);
    }
    if let Some(mut mesh) = meshes.get_mut(&assets.water) {
        *mesh = scenery::water_mesh(extent);
    }
    commands.spawn((
        WorldEntity,
        Name::new("Battlefield"),
        Mesh3d(assets.terrain.clone()),
        MeshMaterial3d(assets.ground_material.clone()),
        Transform::IDENTITY,
    ));
    commands.spawn((
        WorldEntity,
        Name::new("Standards and scenery"),
        Mesh3d(assets.scenery.clone()),
        MeshMaterial3d(assets.scenery_material.clone()),
        Transform::IDENTITY,
    ));
    if battle.config.map == MapKind::Crossing {
        commands.spawn((
            WorldEntity,
            Name::new("River"),
            Mesh3d(assets.water.clone()),
            MeshMaterial3d(assets.water_material.clone()),
            Transform::IDENTITY,
        ));
    }
    for (team, army) in [
        (Team::Blue, &battle.config.blue),
        (Team::Red, &battle.config.red),
    ] {
        for role in [BatchRole::Points, BatchRole::Details] {
            let is_point = role == BatchRole::Points;
            let mesh = if is_point {
                assets.points[army.category.index()].clone()
            } else {
                assets.models[army.category.index()][kind_index(army.kind)].clone()
            };
            commands.spawn((
                WorldEntity,
                Name::new(format!(
                    "{} / {} / {}",
                    team.label(),
                    army.category.label(),
                    if is_point { "points" } else { "detail" }
                )),
                ArmyBatch {
                    team: team.index(),
                    role,
                },
                Mesh3d(mesh),
                CrowdInstances {
                    data: Arc::new(Vec::new()),
                    revision: 0,
                    generation: session.generation,
                    source_tick: battle.stats.tick,
                    point_lod: is_point,
                },
                Transform::IDENTITY,
                NoFrustumCulling,
                // Each army owns a different instance buffer, even when its
                // mesh is shared. Merging phase items would draw only one army.
                NoAutomaticBatching,
            ));
        }
    }
    revision.generation = session.generation;
    revision.packed = None;
    // Keep at most one old job alive until it completes, then discard it. A
    // rapid series of scenario edits cannot create a queue of million-unit jobs.
    revision.last_detail_view = None;
    *counts = RenderCounts {
        packing: true,
        ..default()
    };
}

fn pack_snapshot(snapshot: Arc<BattleSnapshot>, generation: u64) -> PackedSnapshot {
    let began = Instant::now();
    let units = &snapshot.units;
    let armies = [&snapshot.config.blue, &snapshot.config.red];
    let categories = armies.map(|army| army.category);
    let speeds = armies.map(|army| army.category.definition(army.kind).speed.max(0.001));
    let colors = [linear(0.30, 0.68, 0.80), linear(0.84, 0.39, 0.28)];
    let mut data: [Vec<InstanceData>; 2] =
        std::array::from_fn(|team| Vec::with_capacity(snapshot.stats.alive[team] as usize));
    let mut buckets: [HashMap<(i32, i32), Vec<u32>>; 2] = std::array::from_fn(|team| {
        HashMap::with_capacity((snapshot.stats.alive[team] as usize / 200).max(4))
    });
    for index in 0..units.len() {
        if !units.alive[index] {
            continue;
        }
        let team = units.teams[index].index();
        let p = units.positions[index];
        let previous = units.previous_positions[index];
        let color = colors[team];
        let variation = 0.89 + ((index.wrapping_mul(13) % 31) as f32 / 30.0) * 0.16;
        let packed_index = data[team].len() as u32;
        data[team].push(InstanceData {
            positions: [p.x, p.y, previous.x, previous.y],
            appearance: [
                color[0] * variation,
                color[1] * variation,
                color[2] * variation,
                1.0,
            ],
            behavior: [
                units.headings[index],
                (units.velocities[index].length() / speeds[team]).clamp(0.0, 1.0),
                units.attack_flash[index],
                (index % 997) as f32 * 1.37,
            ],
        });
        let cell = (
            (p.x / LOD_CELL_SIZE).floor() as i32,
            (p.y / LOD_CELL_SIZE).floor() as i32,
        );
        buckets[team].entry(cell).or_default().push(packed_index);
    }
    PackedSnapshot {
        generation,
        tick: snapshot.stats.tick,
        elapsed_secs: snapshot.stats.elapsed_secs,
        span_ticks: snapshot.span_ticks,
        categories,
        points: data.map(Arc::new),
        buckets,
        packing_ms: began.elapsed().as_secs_f32() * 1000.0,
    }
}

#[allow(clippy::too_many_arguments)]
fn sync_crowd(
    session: Res<Session>,
    time: Res<Time<Real>>,
    mut revision: ResMut<WorldRevision>,
    mut counts: ResMut<RenderCounts>,
    mut frame: ResMut<CrowdFrame>,
    cameras: Query<&Transform, With<BattleCamera>>,
    mut batches: Query<(&ArmyBatch, &mut CrowdInstances)>,
) {
    let Some(snapshot) = &session.battle else {
        return;
    };
    let mut uploaded = false;
    if revision.packing.as_ref().is_some_and(Task::is_finished) {
        // is_finished guarantees that this poll cannot block the event loop.
        let packed = block_on(revision.packing.take().expect("finished packing task"));
        if packed.generation == session.generation {
            revision.upload_revision = revision.upload_revision.wrapping_add(1);
            for (batch, mut instances) in &mut batches {
                if batch.role == BatchRole::Points {
                    instances.data = Arc::clone(&packed.points[batch.team]);
                    instances.revision = revision.upload_revision;
                    instances.generation = packed.generation;
                    instances.source_tick = packed.tick;
                }
            }
            counts.represented_units = packed.points.iter().map(|data| data.len()).sum();
            counts.submitted_points = counts.represented_units;
            counts.packing_ms = packed.packing_ms;
            counts.generation = packed.generation;
            counts.source_tick = packed.tick;
            revision.packed = Some(packed);
            uploaded = true;
        }
    }
    if revision.packing.is_none()
        && revision
            .packed
            .as_ref()
            .is_none_or(|packed| packed.tick != snapshot.stats.tick)
    {
        let snapshot = Arc::clone(snapshot);
        let generation = session.generation;
        revision.packing = Some(
            AsyncComputeTaskPool::get().spawn(async move { pack_snapshot(snapshot, generation) }),
        );
    }
    counts.packing = revision.packing.is_some();
    let Some(packed) = &revision.packed else {
        return;
    };
    // A delayed background upload must never reuse a newer publication's alpha
    // and rewind the old pose pair. Catch up to the old pair's endpoint instead.
    let alpha = if packed.tick == snapshot.stats.tick {
        session.interpolation()
    } else {
        1.0
    };
    let animation_time = (packed.elapsed_secs
        - (1.0 - alpha) * packed.span_ticks as f32 * battle_core::SIM_DT)
        .max(0.0);
    let map = match snapshot.config.map {
        MapKind::Meadow => 0.0,
        MapKind::Valley => 1.0,
        MapKind::Crossing => 2.0,
    };
    frame.params = Vec4::new(alpha, map, animation_time, 0.0);

    let Ok(view) = cameras.single() else {
        return;
    };
    let moved = revision.last_detail_view.as_ref().is_none_or(|previous| {
        previous.translation.distance_squared(view.translation) > 1.5 * 1.5
            || previous.rotation.dot(view.rotation).abs() < 0.9995
    });
    let refresh =
        uploaded || (moved && time.elapsed_secs_f64() - revision.last_detail_refresh >= 0.10);
    if !refresh {
        return;
    }
    let details = nearby_models(packed, view);
    let detail_count: usize = details.iter().map(Vec::len).sum();
    let draw_batches = packed.points.iter().filter(|data| !data.is_empty()).count()
        + details.iter().filter(|data| !data.is_empty()).count();
    let generation = packed.generation;
    let tick = packed.tick;
    revision.upload_revision = revision.upload_revision.wrapping_add(1);
    for (batch, mut instances) in &mut batches {
        if batch.role == BatchRole::Details {
            instances.data = Arc::new(details[batch.team].clone());
            instances.revision = revision.upload_revision;
            instances.generation = generation;
            instances.source_tick = tick;
        }
    }
    counts.detail_models = detail_count;
    counts.point_only_units = counts.represented_units.saturating_sub(detail_count);
    counts.draw_batches = draw_batches;
    counts.snapshot_bytes =
        (counts.submitted_points + detail_count) * std::mem::size_of::<InstanceData>();
    revision.last_detail_view = Some(*view);
    revision.last_detail_refresh = time.elapsed_secs_f64();
}

fn nearby_models(packed: &PackedSnapshot, camera: &Transform) -> [Vec<InstanceData>; 2] {
    let mut candidates: Vec<(f32, usize, usize)> = Vec::with_capacity(MAX_DETAIL_MODELS * 2);
    let position = camera.translation;
    let forward = camera.forward().as_vec3();
    let min_x = ((position.x - DETAIL_RADIUS) / LOD_CELL_SIZE).floor() as i32;
    let max_x = ((position.x + DETAIL_RADIUS) / LOD_CELL_SIZE).floor() as i32;
    let min_z = ((position.z - DETAIL_RADIUS) / LOD_CELL_SIZE).floor() as i32;
    let max_z = ((position.z + DETAIL_RADIUS) / LOD_CELL_SIZE).floor() as i32;
    for team in 0..2 {
        let elevation = packed.categories[team].elevation().max(0.0) + 1.0;
        // Only nearby bucket samples are inspected. A dense million-unit cell
        // cannot turn a camera drag into a full population scan.
        for x in min_x..=max_x {
            for z in min_z..=max_z {
                let Some(bucket) = packed.buckets[team].get(&(x, z)) else {
                    continue;
                };
                let stride = bucket.len().div_ceil(CELL_CANDIDATE_BUDGET).max(1);
                for &index in bucket.iter().step_by(stride) {
                    let instance = packed.points[team][index as usize];
                    let delta = Vec3::new(instance.positions[0], elevation, instance.positions[1])
                        - position;
                    let distance = delta.length_squared();
                    if distance <= DETAIL_RADIUS * DETAIL_RADIUS && delta.dot(forward) > -2.0 {
                        candidates.push((distance, team, index as usize));
                    }
                }
            }
        }
    }
    candidates.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
    candidates.truncate(MAX_DETAIL_MODELS);
    let mut detail: [Vec<InstanceData>; 2] = std::array::from_fn(|_| Vec::new());
    for (_, team, index) in candidates {
        detail[team].push(packed.points[team][index]);
    }
    detail
}

fn terrain_mesh(map: MapKind, extent: f32) -> Mesh {
    let half = extent * 1.55;
    // Keep terrain facets close to analytic shader heights on large maps;
    // a fixed 64-cell grid would bury point markers beneath coarse valley faces.
    let divisions = ((half * 2.0 / 12.0).ceil() as usize).clamp(64, 320);
    let mut xs: Vec<f32> = (0..=divisions)
        .map(|i| -half + 2.0 * half * i as f32 / divisions as f32)
        .collect();
    let mut zs = xs.clone();
    if map == MapKind::Crossing {
        xs.extend([
            -RIVER_HALF_WIDTH - 1.0,
            -RIVER_HALF_WIDTH,
            RIVER_HALF_WIDTH,
            RIVER_HALF_WIDTH + 1.0,
        ]);
        zs.extend([-BRIDGE_HALF_WIDTH, BRIDGE_HALF_WIDTH]);
    }
    xs.sort_by(f32::total_cmp);
    xs.dedup_by(|a, b| (*a - *b).abs() < 0.001);
    zs.sort_by(f32::total_cmp);
    zs.dedup_by(|a, b| (*a - *b).abs() < 0.001);
    let mut geometry = Geometry::default();
    let point = |x, z| Vec3::new(x, terrain_height(map, x, z), z);
    for x in xs.windows(2) {
        for z in zs.windows(2) {
            let center_x = (x[0] + x[1]) * 0.5;
            let center_z = (z[0] + z[1]) * 0.5;
            if map == MapKind::Crossing
                && center_x.abs() <= RIVER_HALF_WIDTH + 1.0
                && center_z.abs() <= BRIDGE_HALF_WIDTH
            {
                continue;
            }
            let noise = (center_x * 0.13 + center_z * 0.21).sin() * 0.025
                + (center_x * 0.71 - center_z * 0.43).cos() * 0.012;
            let edge =
                ((center_x.abs().max(center_z.abs()) - extent) / (extent * 0.3)).clamp(0.0, 1.0);
            let mut base =
                Vec3::new(0.49 + noise, 0.56 + noise, 0.39 + noise) * (1.0 - edge * 0.08);
            if center_z.abs() < 1.8 && center_x.abs() < extent * 0.82 {
                base = base.lerp(Vec3::new(0.63, 0.57, 0.43), 0.35);
            }
            let color = linear(base.x, base.y, base.z);
            let alternate = linear(base.x * 0.98, base.y * 0.98, base.z * 0.98);
            geometry.triangle(
                point(x[0], z[0]),
                point(x[0], z[1]),
                point(x[1], z[1]),
                color,
                [0.0; 2],
            );
            geometry.triangle(
                point(x[0], z[0]),
                point(x[1], z[1]),
                point(x[1], z[0]),
                alternate,
                [0.0; 2],
            );
        }
    }
    geometry.mesh()
}
