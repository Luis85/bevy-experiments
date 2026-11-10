//! Regional world presentation. Meshes, dressing and atmosphere never own world rules.
use crate::fog::{FogArea, FogDim, Fogged, Look};
use bevy::{
    asset::RenderAssetUsages,
    camera::primitives::Aabb,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
    world_serialization::WorldAsset,
};
use npc_framework::prelude::{SimClock, SimConfig, SimId, SimIndex, SimPos};
use npc_framework::view_helpers::{ViewSystems, VisualIndex, VisualOf};
use testbed_content::village::{
    ResourcePatch, Role, Season, VillagePlace,
    colony::Household,
    frontier::{
        terrain::{TerrainState, TerrainSurface},
        world_spec::{Biome, LivingWorldSpec},
    },
};

const CHUNK: i32 = 16;
/// Gentle dry steps share slope lighting; banks and larger cliffs stay faceted.
const GENTLE_RISER_MM: i32 = 350;
#[derive(Component)]
pub(crate) struct TerrainChunk {
    x: i32,
    z: i32,
    signature: u64,
}
#[derive(Component)]
struct WindPlant {
    phase: f32,
    yaw: f32,
}
#[derive(Component)]
struct SceneryCell {
    x: i32,
    z: i32,
    model: Entity,
}
#[derive(Component)]
pub(crate) struct BuildingModel {
    pub station: SimId,
    pub visual: Entity,
}
#[derive(Component)]
struct Machine {
    station: SimId,
    visual: Entity,
    axis: Vec3,
}
#[derive(Component)]
struct Cloud {
    anchor: Vec3,
    phase: f32,
}
#[derive(Component)]
struct Ocean {
    half_extent: f32,
}
#[derive(Resource)]
struct TerrainStyle {
    season: Season,
}
#[derive(Component)]
struct RegionalLamp;
#[derive(Component)]
struct HearthSmoke {
    home: SimId,
    phase: f32,
    anchor: Vec3,
}
#[derive(Component)]
pub(crate) struct ResourceVisual {
    source: SimId,
    full_scale: f32,
    stage: u8,
    observed: Option<(u32, u32, usize)>,
}
impl ResourceVisual {
    pub(crate) fn new(source: SimId, full_scale: f32) -> Self {
        Self {
            source,
            full_scale,
            stage: 0,
            observed: None,
        }
    }
}
#[derive(Resource, Default)]
struct WaterFrame(f32);

pub(crate) fn plugin(app: &mut App) {
    app.init_resource::<WaterFrame>()
        .add_systems(
            Startup,
            (spawn_landscape, spawn_homes, spawn_settlement_details),
        )
        .add_systems(
            Update,
            (sync_terrain, sync_dressing, install_camera_fog).in_set(ViewSystems::Sync),
        )
        .add_systems(
            Update,
            (wind_and_water, lights_and_smoke).in_set(ViewSystems::Present),
        )
        .add_systems(
            Update,
            elevation
                .in_set(ViewSystems::Present)
                .after(crate::village_world::corpse_pose),
        )
        .add_systems(
            Update,
            resource_lifecycle
                .in_set(ViewSystems::Animate)
                .after(crate::fog::FogSet),
        );
    app.add_systems(
        Update,
        working_machinery
            .in_set(ViewSystems::Animate)
            .after(crate::fog::FogSet),
    );
}

pub(crate) fn building_ready(
    ready: On<bevy::world_serialization::WorldInstanceReady>,
    models: Query<&BuildingModel>,
    children: Query<&Children>,
    names: Query<&Name>,
    mut commands: Commands,
) {
    let Ok(building) = models.get(ready.entity) else {
        return;
    };
    for child in children.iter_descendants(ready.entity) {
        let Ok(name) = names.get(child) else { continue };
        let axis = if name.as_str().contains("windmill_top_fan") {
            Some(Vec3::Z)
        } else if name.as_str().contains("watermill_wheel")
            || name.as_str().contains("lumbermill_saw")
        {
            Some(Vec3::X)
        } else {
            None
        };
        if let Some(axis) = axis {
            commands.entity(child).insert(Machine {
                station: building.station,
                visual: building.visual,
                axis,
            });
        }
    }
}

fn working_machinery(
    clock: Res<SimClock>,
    crafts: Query<&npc_framework::crafting::Crafting>,
    fog: Query<&Fogged>,
    mut machines: Query<(&Machine, &mut Transform)>,
) {
    let active: std::collections::BTreeSet<_> = crafts
        .iter()
        .filter_map(|craft| craft.workstation)
        .collect();
    for (machine, mut transform) in &mut machines {
        if active.contains(&machine.station)
            && fog.get(machine.visual).is_ok_and(|fog| fog.0 == Look::Live)
        {
            transform.rotation = Quat::from_axis_angle(
                machine.axis,
                (clock.tick % 240) as f32 * std::f32::consts::TAU / 240.0,
            );
        }
    }
}

fn cloud_ready(
    ready: On<bevy::world_serialization::WorldInstanceReady>,
    children: Query<&Children>,
    meshes: Query<(), With<Mesh3d>>,
    mut commands: Commands,
) {
    for child in children.iter_descendants(ready.entity) {
        if meshes.contains(child) {
            commands
                .entity(child)
                .insert((bevy::light::NotShadowCaster, bevy::light::NotShadowReceiver));
        }
    }
}

fn hash(x: i32, z: i32) -> u32 {
    let n = (x as u32).wrapping_mul(374_761_393) ^ (z as u32).wrapping_mul(668_265_263);
    (n ^ (n >> 13)).wrapping_mul(1_274_126_177)
}

/// Shared projection for captions and saved map landmarks. Uses the supplied
/// known position, never a lookup of an unseen entity's live position.
pub(crate) fn raised_point(terrain: Option<&TerrainState>, pos: SimPos, lift: f32) -> Vec3 {
    let y = terrain
        .and_then(|t| t.cell(pos.x.div_euclid(1000), pos.z.div_euclid(1000)))
        .map_or(0.0, |cell| cell.height_mm.max(0) as f32 / 1000.0);
    Vec3::new(pos.x as f32 / 1000.0, y + lift, pos.z as f32 / 1000.0)
}

/// The existing exploration texture follows the actual terrain surface. One
/// mesh covers the bounded region; no per-cell view entities are introduced.
pub(crate) fn terrain_mask_mesh(terrain: &TerrainState, region: crate::fog::MaskRegion) -> Mesh {
    let mut geometry = Geometry::default();
    let mut uvs = Vec::new();
    let height = |x, z| {
        terrain
            .cell(x, z)
            .map_or(0.0, |c| c.height_mm.max(0) as f32 / 1000.0)
            + 0.07
    };
    let uv = |x: f32, z: f32| {
        [
            (x - region.min.x as f32) / region.width as f32,
            (z - region.min.z as f32) / region.height as f32,
        ]
    };
    for z in region.min.z..region.min.z + region.height as i32 {
        for x in region.min.x..region.min.x + region.width as i32 {
            let (a, b, h) = (x as f32, z as f32, height(x, z));
            geometry.quad(
                [
                    [a, h, b],
                    [a, h, b + 1.0],
                    [a + 1.0, h, b + 1.0],
                    [a + 1.0, h, b],
                ],
                [0.0, 1.0, 0.0],
                [1.0; 4],
            );
            uvs.extend([
                uv(a, b),
                uv(a, b + 1.0),
                uv(a + 1.0, b + 1.0),
                uv(a + 1.0, b),
            ]);
            for (dx, dz, normal, edge) in [
                (
                    -1,
                    0,
                    [-1.0, 0.0, 0.0],
                    [[a - 0.004, b], [a - 0.004, b + 1.0]],
                ),
                (
                    1,
                    0,
                    [1.0, 0.0, 0.0],
                    [[a + 1.004, b + 1.0], [a + 1.004, b]],
                ),
                (
                    0,
                    -1,
                    [0.0, 0.0, -1.0],
                    [[a + 1.0, b - 0.004], [a, b - 0.004]],
                ),
                (
                    0,
                    1,
                    [0.0, 0.0, 1.0],
                    [[a, b + 1.004], [a + 1.0, b + 1.004]],
                ),
            ] {
                let low = height(x + dx, z + dz);
                if low >= h {
                    continue;
                }
                let [[x0, z0], [x1, z1]] = edge;
                geometry.quad(
                    [[x0, h, z0], [x0, low, z0], [x1, low, z1], [x1, h, z1]],
                    normal,
                    [1.0; 4],
                );
                uvs.extend([uv(a + 0.5, b + 0.5); 4]);
            }
        }
    }
    geometry
        .mesh()
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
}

/// First visible heightfield cell under a pointer. Walks at most width+height
/// cells; unlike a full-grid search its cost does not scale with map area.
pub(crate) fn terrain_hit(world: &World, ray: Ray3d) -> Option<(i32, i32)> {
    let terrain = world.get_resource::<TerrainState>()?;
    let point = raycast(terrain, ray)?;
    Some((point.x.floor() as i32, point.z.floor() as i32))
}

pub(crate) fn raycast(terrain: &TerrainState, ray: Ray3d) -> Option<Vec3> {
    if ray.direction.y >= -0.000_001 {
        return None;
    }
    let bounds = terrain.bounds();
    let min = Vec2::new(bounds.min.x as f32, bounds.min.z as f32);
    let max = min + Vec2::new(bounds.width as f32, bounds.height as f32);
    let origin = Vec2::new(ray.origin.x, ray.origin.z);
    let direction = Vec2::new(ray.direction.x, ray.direction.z);
    let mut enter = 0.0_f32;
    let mut exit = f32::INFINITY;
    for axis in 0..2 {
        if direction[axis].abs() < 0.000_001 {
            if origin[axis] < min[axis] || origin[axis] >= max[axis] {
                return None;
            }
        } else {
            let a = (min[axis] - origin[axis]) / direction[axis];
            let b = (max[axis] - origin[axis]) / direction[axis];
            enter = enter.max(a.min(b));
            exit = exit.min(a.max(b));
        }
    }
    if enter > exit || exit < 0.0 {
        return None;
    }
    let start = ray.get_point(enter + 0.0001);
    let (mut x, mut z) = (start.x.floor() as i32, start.z.floor() as i32);
    let step_x = if direction.x >= 0.0 { 1 } else { -1 };
    let step_z = if direction.y >= 0.0 { 1 } else { -1 };
    let delta_x = if direction.x.abs() < 0.000_001 {
        f32::INFINITY
    } else {
        1.0 / direction.x.abs()
    };
    let delta_z = if direction.y.abs() < 0.000_001 {
        f32::INFINITY
    } else {
        1.0 / direction.y.abs()
    };
    let mut next_x = if delta_x.is_infinite() {
        f32::INFINITY
    } else {
        ((x + if step_x > 0 { 1 } else { 0 }) as f32 - origin.x) / direction.x
    };
    let mut next_z = if delta_z.is_infinite() {
        f32::INFINITY
    } else {
        ((z + if step_z > 0 { 1 } else { 0 }) as f32 - origin.y) / direction.y
    };
    for _ in 0..bounds.width + bounds.height + 2 {
        let cell = terrain.cell(x, z)?;
        let y = if cell.surface == TerrainSurface::Water {
            -0.11
        } else {
            cell.height_mm as f32 / 1000.0
        };
        let hit = (y - ray.origin.y) / ray.direction.y;
        let leave = next_x.min(next_z).min(exit);
        if hit >= enter && hit <= leave {
            return Some(ray.get_point(hit));
        }
        // An elevated cell's vertical side can be the first visible surface.
        if hit < enter && ray.get_point(enter).y >= -0.7 {
            return Some(ray.get_point(enter + 0.0001));
        }
        if leave >= exit {
            return None;
        }
        enter = leave;
        if next_x <= next_z {
            x += step_x;
            next_x += delta_x;
        } else {
            z += step_z;
            next_z += delta_z;
        }
    }
    None
}

fn color(surface: TerrainSurface, biome: Biome, x: i32, z: i32, season: Season) -> [f32; 4] {
    let mut rgb = match surface {
        TerrainSurface::Soil => [0.43, 0.32, 0.20],
        TerrainSurface::Stone => [0.49, 0.51, 0.48],
        TerrainSurface::Water => [0.10, 0.31, 0.35],
        TerrainSurface::Grass => match biome {
            Biome::Coast => [0.64, 0.65, 0.39],
            Biome::Meadow => [0.43, 0.59, 0.29],
            Biome::Woodland => [0.29, 0.43, 0.25],
            Biome::Ridge => [0.47, 0.50, 0.36],
            Biome::Wetland => [0.31, 0.45, 0.34],
        },
    };
    if surface == TerrainSurface::Grass {
        match season {
            Season::Winter => rgb = [0.79, 0.84, 0.85],
            Season::Autumn => rgb = [rgb[0] * 1.10, rgb[1] * 0.91, rgb[2] * 0.84],
            Season::Summer => rgb = [rgb[0] * 1.02, rgb[1] * 0.96, rgb[2] * 0.93],
            Season::Spring => {}
        }
    }
    // Broad tonal patches, rather than alternating checkerboard tiles.
    let shift = ((x as f32 * 0.13).sin() * (z as f32 * 0.11).cos()) * 0.022;
    Color::srgb(rgb[0] + shift, rgb[1] + shift, rgb[2] + shift)
        .to_linear()
        .to_f32_array()
}

#[derive(Default)]
struct Geometry {
    vertices: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    colors: Vec<[f32; 4]>,
    indices: Vec<u32>,
}
impl Geometry {
    fn quad(&mut self, points: [[f32; 3]; 4], normal: [f32; 3], color: [f32; 4]) {
        let start = self.vertices.len() as u32;
        self.vertices.extend(points);
        self.normals.extend([normal; 4]);
        self.colors.extend([color; 4]);
        self.indices
            .extend([start, start + 1, start + 2, start, start + 2, start + 3]);
    }
    fn mesh(self) -> Mesh {
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.vertices)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, self.colors)
        .with_inserted_indices(Indices::U32(self.indices))
    }
}

fn chunk_signature(terrain: &TerrainState, x: i32, z: i32) -> u64 {
    let mut value = 0xcbf2_9ce4_8422_2325_u64;
    // Neighbor cells participate because exposed cliff faces depend on them.
    for cz in z - 1..=z + CHUNK {
        for cx in x - 1..=x + CHUNK {
            if let Some(cell) = terrain.cell(cx, cz) {
                value ^= cell.height_mm as u64
                    ^ ((cell.surface as u64) << 32)
                    ^ ((cell.biome as u64) << 40);
                value = value.wrapping_mul(0x100_0000_01b3);
            }
        }
    }
    value
}

/// Shading only: the integer top and riser vertices remain unchanged. Reject
/// shoreline/cliff neighbours so their existing hard face lighting is preserved.
fn terrain_surface_normal(terrain: &TerrainState, x: i32, z: i32) -> [f32; 3] {
    let Some(center) = terrain.cell(x, z) else {
        return Vec3::Y.to_array();
    };
    let mut heights = [0_i32; 4];
    for (i, (dx, dz)) in [(-1, 0), (1, 0), (0, -1), (0, 1)].into_iter().enumerate() {
        let Some(neighbor) = terrain.cell(x + dx, z + dz) else {
            return Vec3::Y.to_array();
        };
        if neighbor.surface == TerrainSurface::Water
            || (neighbor.height_mm - center.height_mm).abs() > GENTLE_RISER_MM
        {
            return Vec3::Y.to_array();
        }
        heights[i] = neighbor.height_mm;
    }
    Vec3::new(
        (heights[0] - heights[1]) as f32 / 2000.0,
        1.0,
        (heights[2] - heights[3]) as f32 / 2000.0,
    )
    .normalize()
    .to_array()
}

fn terrain_mesh(terrain: &TerrainState, x: i32, z: i32, season: Season) -> Option<Mesh> {
    let mut geometry = Geometry::default();
    for cz in z..z + CHUNK {
        for cx in x..x + CHUNK {
            let Some(cell) = terrain.cell(cx, cz) else {
                continue;
            };
            if cell.surface == TerrainSurface::Water {
                continue;
            }
            let (a, b, h) = (cx as f32, cz as f32, cell.height_mm as f32 / 1000.0);
            let c = color(cell.surface, cell.biome, cx, cz, season);
            let surface_normal = terrain_surface_normal(terrain, cx, cz);
            geometry.quad(
                [
                    [a, h, b],
                    [a, h, b + 1.0],
                    [a + 1.0, h, b + 1.0],
                    [a + 1.0, h, b],
                ],
                surface_normal,
                c,
            );
            for (dx, dz, normal, edge) in [
                (-1, 0, [-1.0, 0.0, 0.0], [[a, b], [a, b + 1.0]]),
                (1, 0, [1.0, 0.0, 0.0], [[a + 1.0, b + 1.0], [a + 1.0, b]]),
                (0, -1, [0.0, 0.0, -1.0], [[a + 1.0, b], [a, b]]),
                (0, 1, [0.0, 0.0, 1.0], [[a, b + 1.0], [a + 1.0, b + 1.0]]),
            ] {
                let neighbor = terrain.cell(cx + dx, cz + dz);
                let low = neighbor.map_or(-0.7, |n| n.height_mm as f32 / 1000.0);
                if low >= h {
                    continue;
                }
                let [[x0, z0], [x1, z1]] = edge;
                let gentle = neighbor.is_some_and(|n| {
                    n.surface != TerrainSurface::Water
                        && cell.height_mm - n.height_mm <= GENTLE_RISER_MM
                });
                let (face_normal, face_color) = if gentle {
                    (surface_normal, c)
                } else {
                    (normal, [c[0] * 0.83, c[1] * 0.83, c[2] * 0.83, 1.0])
                };
                geometry.quad(
                    [[x0, h, z0], [x0, low, z0], [x1, low, z1], [x1, h, z1]],
                    face_normal,
                    face_color,
                );
            }
        }
    }
    // The ocean owns water rendering. Keep all-water chunks as bookkeeping
    // entities, without ever uploading a zero-vertex asset to the renderer.
    (!geometry.vertices.is_empty()).then(|| geometry.mesh())
}

fn scene(server: &AssetServer, path: &'static str) -> Handle<WorldAsset> {
    server.load(GltfAssetLabel::Scene(0).from_asset(path))
}

fn spawn_terrain_chunks(
    commands: &mut Commands,
    terrain: &TerrainState,
    meshes: &mut Assets<Mesh>,
    material: &Handle<StandardMaterial>,
    season: Season,
) {
    let bounds = terrain.bounds();
    for z in (bounds.min.z..bounds.min.z + bounds.height as i32).step_by(CHUNK as usize) {
        for x in (bounds.min.x..bounds.min.x + bounds.width as i32).step_by(CHUNK as usize) {
            let mut chunk = commands.spawn((
                Name::new(format!("regional terrain {x},{z}")),
                TerrainChunk {
                    x,
                    z,
                    signature: chunk_signature(terrain, x, z),
                },
                MeshMaterial3d(material.clone()),
                Transform::default(),
            ));
            if let Some(mesh) = terrain_mesh(terrain, x, z, season) {
                chunk.insert(Mesh3d(meshes.add(mesh)));
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_landscape(
    mut commands: Commands,
    terrain: Res<TerrainState>,
    spec: Res<LivingWorldSpec>,
    clock: Res<SimClock>,
    config: Res<SimConfig>,
    server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let (_, season, _) = testbed_content::village::calendar_at(clock.tick, &config, true);
    commands.insert_resource(TerrainStyle { season });
    let land = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        perceptual_roughness: 0.96,
        reflectance: 0.08,
        ..default()
    });
    spawn_terrain_chunks(&mut commands, &terrain, &mut meshes, &land, season);
    let water = materials.add(StandardMaterial {
        base_color: Color::srgb(0.075, 0.31, 0.39),
        perceptual_roughness: 0.27,
        reflectance: 0.5,
        metallic: 0.08,
        ..default()
    });
    let half_extent = terrain.width.max(terrain.height) as f32 * 0.5 + 96.0;
    // Continue the sea beyond the detailed animated patch; overview framing
    // must never expose a rectangular ocean floating against the clear colour.
    let horizon = terrain.width.max(terrain.height) as f32 * 16.0;
    commands.spawn((
        Name::new("distant sea"),
        Mesh3d(meshes.add(Plane3d::default().mesh().size(horizon, horizon))),
        MeshMaterial3d(water.clone()),
        Transform::from_xyz(0.0, -0.22, 0.0),
    ));
    commands.spawn((
        Name::new("living ocean"),
        Ocean { half_extent },
        Mesh3d(meshes.add(ocean_mesh(0.0, half_extent))),
        MeshMaterial3d(water),
        Transform::default(),
    ));
    let trees = [
        scene(&server, "living/nature/tree_single_A.gltf"),
        scene(&server, "living/nature/tree_single_B.gltf"),
        scene(&server, "living/nature/trees_A_small.gltf"),
        scene(&server, "living/nature/trees_B_small.gltf"),
    ];
    let rocks = [
        scene(&server, "living/nature/rock_single_B.gltf"),
        scene(&server, "living/nature/rock_single_D.gltf"),
        scene(&server, "living/nature/rock_single_E.gltf"),
    ];
    let reed = scene(&server, "living/nature/waterplant_A.gltf");
    for (coord, cell) in terrain.iter() {
        if coord.x.rem_euclid(3) != 0
            || coord.z.rem_euclid(3) != 0
            || cell.surface != TerrainSurface::Grass
        {
            continue;
        }
        let h = hash(coord.x, coord.z);
        let chance = match cell.biome {
            Biome::Woodland => 58,
            Biome::Meadow => 10,
            Biome::Coast => 8,
            Biome::Ridge => 20,
            Biome::Wetland => 32,
        };
        if h % 100 >= chance {
            continue;
        }
        let p = SimPos::new(coord.x * 1000 + 500, coord.z * 1000 + 500);
        if spec.settlements.iter().any(|s| {
            s.sites
                .iter()
                .any(|site| site.position.dist2(p) < 30_000_000)
                || s.homes.iter().any(|home| home.door.dist2(p) < 24_000_000)
        }) {
            continue;
        }
        let at = Vec3::new(
            p.x as f32 / 1000.0,
            cell.height_mm as f32 / 1000.0,
            p.z as f32 / 1000.0,
        );
        let yaw = (h % 628) as f32 / 100.0;
        let (model, scale, wind) = match cell.biome {
            Biome::Ridge => (
                rocks[(h as usize / 7) % rocks.len()].clone(),
                3.0 + (h % 9) as f32 * 0.25,
                false,
            ),
            Biome::Wetland => (reed.clone(), 3.0 + (h % 6) as f32 * 0.2, true),
            _ => (
                trees[(h as usize / 11) % trees.len()].clone(),
                3.4 + (h % 10) as f32 * 0.16,
                true,
            ),
        };
        let root = commands
            .spawn((
                Name::new("regional foliage"),
                Transform::from_translation(at),
                Visibility::default(),
                FogArea::around(at, Vec3::splat(1.0)),
                FogDim::default(),
            ))
            .id();
        let mut plant = commands.spawn((
            ChildOf(root),
            WorldAssetRoot(model),
            Transform::from_rotation(Quat::from_rotation_y(yaw)).with_scale(Vec3::splat(scale)),
        ));
        if wind {
            plant.insert(WindPlant {
                phase: (h % 31) as f32,
                yaw,
            });
        }
        let model = plant.id();
        commands.entity(root).insert(SceneryCell {
            x: coord.x,
            z: coord.z,
            model,
        });
    }
    let cloud = scene(&server, "living/nature/cloud_big.gltf");
    let radius = terrain.width.max(terrain.height) as f32 * 0.5;
    for (i, (x, y, z)) in [
        (-1.14, 86.0, -0.91),
        (0.61, 102.0, -1.22),
        (-1.27, 94.0, 0.48),
        (1.20, 89.0, -0.29),
        (0.18, 108.0, 1.26),
        (-0.76, 98.0, 1.13),
    ]
    .into_iter()
    .enumerate()
    {
        let anchor = Vec3::new(x * radius, y, z * radius);
        commands
            .spawn((
                Name::new("coastal cloud"),
                Cloud {
                    anchor,
                    phase: i as f32,
                },
                WorldAssetRoot(cloud.clone()),
                Transform::from_translation(anchor).with_scale(Vec3::splat(10.0 + (i % 3) as f32)),
            ))
            .observe(cloud_ready);
    }
}

fn sync_terrain(
    mut commands: Commands,
    terrain: Res<TerrainState>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut chunks: Query<(Entity, &mut TerrainChunk, Option<&Mesh3d>)>,
    clock: Res<SimClock>,
    config: Res<SimConfig>,
    mut style: ResMut<TerrainStyle>,
) {
    let (_, season, _) = testbed_content::village::calendar_at(clock.tick, &config, true);
    let season_changed = style.season != season;
    if !terrain.is_changed() && !season_changed {
        return;
    }
    for (entity, mut chunk, handle) in &mut chunks {
        let signature = chunk_signature(&terrain, chunk.x, chunk.z);
        if signature != chunk.signature || season_changed {
            match terrain_mesh(&terrain, chunk.x, chunk.z, season) {
                Some(mesh) => {
                    if let Some(handle) = handle
                        && let Some(mut asset) = meshes.get_mut(&handle.0)
                    {
                        *asset = mesh;
                    } else {
                        commands.entity(entity).insert(Mesh3d(meshes.add(mesh)));
                    }
                }
                None => {
                    if let Some(handle) = handle {
                        commands.entity(entity).remove::<(Mesh3d, Aabb)>();
                        // Each chunk owns its asset. Remove it immediately so
                        // no stale land or empty replacement reaches rendering.
                        meshes.remove(handle.id());
                    }
                }
            }
            chunk.signature = signature;
        }
    }
    style.season = season;
}

fn sync_dressing(
    terrain: Res<TerrainState>,
    mut roots: Query<(&SceneryCell, &mut Transform)>,
    mut models: Query<&mut Visibility>,
) {
    if !terrain.is_changed() {
        return;
    }
    for (cell, mut transform) in &mut roots {
        let Some(ground) = terrain.cell(cell.x, cell.z) else {
            continue;
        };
        transform.translation.y = ground.height_mm as f32 / 1000.0;
        if let Ok(mut visibility) = models.get_mut(cell.model) {
            *visibility = if ground.surface == TerrainSurface::Grass {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
        }
    }
}

fn spawn_homes(
    mut commands: Commands,
    homes: Query<
        (Entity, &SimId, &SimPos, &Household),
        Without<testbed_content::village::homestead::Cottage>,
    >,
    terrain: Res<TerrainState>,
    server: Res<AssetServer>,
    mut index: ResMut<VisualIndex>,
) {
    let homes_models = [
        scene(&server, "hex/building_home_A_blue.gltf"),
        scene(&server, "living/buildings/building_home_B_blue.gltf"),
    ];
    for (entity, id, position, house) in &homes {
        let y = terrain
            .cell(position.x.div_euclid(1000), position.z.div_euclid(1000))
            .map_or(0.0, |c| c.height_mm as f32 / 1000.0);
        let root = commands
            .spawn((
                Name::new(house.label.clone()),
                VisualOf(entity),
                crate::village_world::StaticId(*id),
                Transform::from_xyz(position.x as f32 / 1000.0, y, position.z as f32 / 1000.0),
                Visibility::default(),
                Fogged::default(),
                FogDim::default(),
            ))
            .id();
        commands.spawn((
            ChildOf(root),
            WorldAssetRoot(homes_models[(id.0 as usize / 2) % 2].clone()),
            Transform::from_xyz(
                0.0,
                0.0,
                testbed_content::village::frontier::settlements::HOME_OFFSET_MM as f32 / 1000.0,
            )
            .with_rotation(Quat::from_rotation_y(std::f32::consts::PI))
            .with_scale(Vec3::splat(4.0)),
        ));
        index.map.insert(entity, root);
    }
}

fn spawn_settlement_details(
    mut commands: Commands,
    spec: Res<LivingWorldSpec>,
    terrain: Res<TerrainState>,
    homes: Query<
        (
            &SimId,
            &SimPos,
            Option<&testbed_content::village::homestead::Cottage>,
        ),
        With<Household>,
    >,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let cube = meshes.add(Cuboid::default());
    let wood = materials.add(Color::srgb(0.28, 0.18, 0.11));
    let light = materials.add(StandardMaterial {
        base_color: Color::srgb(1.0, 0.75, 0.34),
        emissive: LinearRgba::new(2.4, 1.3, 0.3, 1.0),
        ..default()
    });
    for settlement in &spec.settlements {
        for site in &settlement.sites {
            let p = site.position;
            let y = terrain
                .cell(p.x.div_euclid(1000), p.z.div_euclid(1000))
                .map_or(0.0, |c| c.height_mm as f32 / 1000.0);
            let at = Vec3::new(p.x as f32 / 1000.0 - 2.5, y, p.z as f32 / 1000.0 - 1.0);
            let root = commands
                .spawn((
                    Name::new("village path lantern"),
                    Transform::from_translation(at),
                    Visibility::default(),
                    FogArea::around(at, Vec3::splat(0.5)),
                    FogDim::default(),
                ))
                .id();
            for (position, scale, material) in [
                (
                    Vec3::new(0.0, 1.0, 0.0),
                    Vec3::new(0.09, 2.0, 0.09),
                    wood.clone(),
                ),
                (
                    Vec3::new(0.0, 1.8, 0.0),
                    Vec3::new(0.23, 0.32, 0.23),
                    light.clone(),
                ),
            ] {
                commands.spawn((
                    ChildOf(root),
                    Mesh3d(cube.clone()),
                    MeshMaterial3d(material),
                    Transform::from_translation(position).with_scale(scale),
                ));
            }
            commands.spawn((
                ChildOf(root),
                RegionalLamp,
                PointLight {
                    color: Color::srgb(1.0, 0.69, 0.31),
                    intensity: 0.0,
                    range: 7.0,
                    shadow_maps_enabled: false,
                    ..default()
                },
                Transform::from_xyz(0.0, 1.9, 0.0),
            ));
        }
    }
    let puff = meshes.add(Sphere::new(0.15).mesh().ico(1).expect("small smoke sphere"));
    let smoke = materials.add(StandardMaterial {
        base_color: Color::srgba(0.74, 0.77, 0.76, 0.17),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });
    for (home, p, cottage) in &homes {
        let y = terrain
            .cell(p.x.div_euclid(1000), p.z.div_euclid(1000))
            .map_or(0.0, |c| c.height_mm as f32 / 1000.0);
        let anchor = Vec3::new(
            p.x as f32 / 1000.0,
            y + 3.0,
            p.z as f32 / 1000.0 + if cottage.is_some() { 0.2 } else { -3.3 },
        );
        let root = commands
            .spawn((
                Name::new("occupied hearth smoke"),
                Transform::default(),
                Visibility::default(),
                FogArea::around(anchor, Vec3::splat(0.5)),
                FogDim::default(),
            ))
            .id();
        for i in 0..3 {
            commands.spawn((
                ChildOf(root),
                HearthSmoke {
                    home: *home,
                    phase: i as f32 / 3.0,
                    anchor,
                },
                Mesh3d(puff.clone()),
                MeshMaterial3d(smoke.clone()),
                Transform::from_translation(anchor),
            ));
        }
    }
}

fn install_camera_fog(
    mut commands: Commands,
    cameras: Query<Entity, (With<Camera3d>, Without<DistanceFog>)>,
) {
    for entity in &cameras {
        commands.entity(entity).insert(DistanceFog {
            color: Color::srgb(0.58, 0.70, 0.73),
            directional_light_color: Color::srgba(1.0, 0.88, 0.68, 0.25),
            directional_light_exponent: 18.0,
            falloff: FogFalloff::from_visibility_colors(
                210.0,
                Color::srgb(0.38, 0.52, 0.61),
                Color::srgb(0.73, 0.82, 0.86),
            ),
        });
    }
}

fn ocean_mesh(time: f32, half_extent: f32) -> Mesh {
    let step = half_extent / 24.0;
    let mut geometry = Geometry::default();
    for z in -24..24 {
        for x in -24..24 {
            let a = x as f32 * step;
            let b = z as f32 * step;
            let height = |x: f32, z: f32| {
                -0.11
                    + (x * 0.29 + time * 0.55).sin() * 0.035
                    + (z * 0.31 - time * 0.43).sin() * 0.026
            };
            let points = [
                [a, height(a, b), b],
                [a, height(a, b + step), b + step],
                [a + step, height(a + step, b + step), b + step],
                [a + step, height(a + step, b), b],
            ];
            let start = geometry.vertices.len();
            geometry.quad(points, [0.0, 1.0, 0.0], [1.0; 4]);
            for (offset, [x, _, z]) in points.into_iter().enumerate() {
                // Shared positions receive identical analytic normals/colour.
                // Per-quad normals made a highly visible specular checkerboard.
                geometry.normals[start + offset] = Vec3::new(
                    -0.01015 * (x * 0.29 + time * 0.55).cos(),
                    1.0,
                    -0.00806 * (z * 0.31 - time * 0.43).cos(),
                )
                .normalize()
                .to_array();
                let brightness = 0.985 + 0.015 * (x * 0.07 + z * 0.06 + time * 0.3).sin();
                geometry.colors[start + offset] = [brightness, brightness, brightness, 1.0];
            }
        }
    }
    geometry.mesh()
}

fn wind_and_water(
    time: Res<Time>,
    mut plants: Query<(&WindPlant, &mut Transform)>,
    mut clouds: Query<(&Cloud, &mut Transform), Without<WindPlant>>,
    water: Query<(&Ocean, &Mesh3d)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut frame: ResMut<WaterFrame>,
) {
    let seconds = time.elapsed_secs();
    for (plant, mut transform) in &mut plants {
        transform.rotation = Quat::from_rotation_y(plant.yaw)
            * Quat::from_rotation_z((seconds * 0.7 + plant.phase).sin() * 0.018);
    }
    for (cloud, mut transform) in &mut clouds {
        transform.translation =
            cloud.anchor + Vec3::new((seconds * 0.018 + cloud.phase).sin() * 18.0, 0.0, 0.0);
    }
    if seconds - frame.0 >= 0.10 {
        frame.0 = seconds;
        for (ocean, handle) in &water {
            if let Some(mut mesh) = meshes.get_mut(&handle.0) {
                *mesh = ocean_mesh(seconds, ocean.half_extent);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn lights_and_smoke(
    clock: Res<SimClock>,
    config: Res<SimConfig>,
    time: Res<Time>,
    homes: Query<&Household>,
    index: Res<SimIndex>,
    fog: Query<&FogDim>,
    mut lamps: Query<&mut PointLight, With<RegionalLamp>>,
    mut smoke: Query<(&HearthSmoke, &ChildOf, &mut Transform, &mut Visibility)>,
) {
    let night = config.is_night(clock.tick);
    for mut lamp in &mut lamps {
        lamp.intensity = if night { 18000.0 } else { 0.0 };
    }
    for (puff, parent, mut transform, mut visibility) in &mut smoke {
        let lit = index
            .get(puff.home)
            .and_then(|entity| homes.get(entity).ok())
            .is_some_and(|home| home.heated_until > clock.tick)
            && fog.get(parent.parent()).is_ok_and(|dim| !dim.0);
        *visibility = if lit {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        let phase = (time.elapsed_secs() * 0.18 + puff.phase).fract();
        transform.translation = puff.anchor + Vec3::new(phase * 0.7, phase * 2.1, phase * 0.2);
        transform.scale = Vec3::splat(0.5 + phase * 1.4);
    }
}

fn elevation(
    terrain: Res<TerrainState>,
    positions: Query<&SimPos>,
    statics: Query<
        (),
        Or<(
            With<VillagePlace>,
            With<ResourcePatch>,
            With<Household>,
            With<npc_framework::prelude::Item>,
            With<npc_framework::prelude::Vehicle>,
            With<testbed_content::Prop>,
            With<testbed_content::village::hunting::Wildlife>,
            With<testbed_content::village::homestead::HomesteadPlot>,
            With<testbed_content::village::homestead::Cottage>,
        )>,
    >,
    mut roots: Query<(&VisualOf, &mut Transform, Has<crate::visuals::ActorVisual>)>,
) {
    for (of, mut transform, actor) in &mut roots {
        if !actor && !statics.contains(of.0) {
            continue;
        }
        let Ok(_position) = positions.get(of.0) else {
            continue;
        };
        if let Some(cell) = terrain.cell(
            transform.translation.x.floor() as i32,
            transform.translation.z.floor() as i32,
        ) {
            let y = cell.height_mm.max(0) as f32 / 1000.0;
            if actor {
                transform.translation.y += y;
            } else {
                transform.translation.y = y;
            }
        }
    }
}

fn resource_lifecycle(
    index: Res<SimIndex>,
    patches: Query<(
        &ResourcePatch,
        Option<&testbed_content::village::frontier::ecology::EcologicalPatch>,
    )>,
    parents: Query<&Fogged>,
    mut visuals: Query<(
        &mut ResourceVisual,
        &ChildOf,
        &mut Transform,
        &mut Visibility,
    )>,
) {
    for (mut visual, parent, mut transform, mut visibility) in &mut visuals {
        if parents
            .get(parent.parent())
            .is_ok_and(|fog| fog.0 != Look::Live)
        {
            continue;
        }
        let Some(entity) = index.get(visual.source) else {
            continue;
        };
        let Ok((patch, ecology)) = patches.get(entity) else {
            continue;
        };
        let biomass = ecology.map_or(100_000, |state| state.biomass_milli);
        let growing = ecology.map_or(0, |state| state.pending_growth.len());
        let state = (patch.remaining, biomass, growing);
        if visual.observed == Some(state) {
            continue;
        }
        visual.observed = Some(state);
        let fraction =
            (patch.remaining as f32 / patch.capacity.max(1) as f32).min(biomass as f32 / 100_000.0);
        let visible = match visual.stage {
            0 => patch.kind != "log" || fraction > 0.02,
            1 => fraction > 0.42,
            2 => fraction > 0.72,
            3 => fraction < 0.10,
            stage => growing > usize::from(stage - 4),
        };
        *visibility = if visible {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        transform.scale = Vec3::splat(
            visual.full_scale
                * if visual.stage >= 3 {
                    1.0
                } else {
                    0.40 + fraction.sqrt() * 0.60
                },
        );
    }
}

pub(crate) fn resource_details(
    commands: &mut Commands,
    server: &AssetServer,
    root: Entity,
    source: SimId,
    kind: &str,
) {
    if kind != "log" {
        return;
    }
    for (stage, path, offset, scale) in [
        (
            1,
            "living/nature/tree_single_B.gltf",
            Vec3::new(-1.2, 0.0, 0.8),
            2.5,
        ),
        (
            2,
            "living/nature/tree_single_A.gltf",
            Vec3::new(1.1, 0.0, 0.6),
            2.1,
        ),
        (3, "living/nature/tree_single_A_cut.gltf", Vec3::ZERO, 3.0),
        (
            4,
            "living/nature/tree_single_B.gltf",
            Vec3::new(-0.65, 0.0, -0.7),
            0.7,
        ),
        (
            5,
            "living/nature/tree_single_A.gltf",
            Vec3::new(0.60, 0.0, -0.8),
            0.6,
        ),
        (
            6,
            "living/nature/tree_single_B.gltf",
            Vec3::new(0.0, 0.0, -1.2),
            0.8,
        ),
    ] {
        commands.spawn((
            ChildOf(root),
            WorldAssetRoot(scene(server, path)),
            Transform::from_translation(offset).with_scale(Vec3::splat(scale)),
            Visibility::Hidden,
            ResourceVisual {
                source,
                full_scale: scale,
                stage,
                observed: None,
            },
        ));
    }
}

/// Purpose-built silhouettes; selection is a visual interpretation of site role.
pub(crate) fn building(role: Role) -> &'static str {
    match role {
        Role::Smith => "living/buildings/building_blacksmith_blue.gltf",
        Role::Miller => "living/buildings/building_windmill_blue.gltf",
        Role::Woodcutter | Role::Carpenter => "living/buildings/building_lumbermill_blue.gltf",
        Role::Miner => "living/buildings/building_mine_blue.gltf",
        Role::Innkeeper | Role::Baker => "living/buildings/building_tavern_blue.gltf",
        Role::WaterKeeper => "living/buildings/building_well_blue.gltf",
        Role::Farmer | Role::Tailor | Role::Tanner => "living/buildings/building_market_blue.gltf",
        Role::Fisher => "living/buildings/building_watermill_blue.gltf",
        _ => "living/buildings/building_home_B_blue.gltf",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use testbed_content::village::frontier::terrain::TerrainCell;
    fn plateau(height_mm: i32) -> TerrainState {
        TerrainState {
            width: 64,
            height: 64,
            cells: vec![
                TerrainCell {
                    height_mm,
                    surface: TerrainSurface::Grass,
                    biome: Biome::Ridge
                };
                4096
            ],
            revision: 0,
        }
    }

    fn terrain_view(terrain: TerrainState) -> App {
        let mut app = App::new();
        let season = testbed_content::village::calendar_at(0, &SimConfig::default(), true).1;
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<Mesh>>()
            .init_resource::<SimClock>()
            .init_resource::<SimConfig>()
            .insert_resource(TerrainStyle { season })
            .insert_resource(terrain)
            .add_systems(
                Startup,
                |mut commands: Commands,
                 terrain: Res<TerrainState>,
                 mut meshes: ResMut<Assets<Mesh>>,
                 style: Res<TerrainStyle>| {
                    spawn_terrain_chunks(
                        &mut commands,
                        &terrain,
                        &mut meshes,
                        &Handle::default(),
                        style.season,
                    );
                },
            )
            .add_systems(Update, sync_terrain);
        app
    }

    fn update_terrain_view(app: &mut App) {
        let terrain = app.world().resource::<TerrainState>().clone();
        let clock = *app.world().resource::<SimClock>();
        app.update();
        assert_eq!(*app.world().resource::<TerrainState>(), terrain);
        assert_eq!(*app.world().resource::<SimClock>(), clock);
        assert!(
            app.world()
                .resource::<Assets<Mesh>>()
                .iter()
                .all(|(_, mesh)| mesh.count_vertices() > 0)
        );
    }

    #[test]
    fn water_chunks_keep_their_identity_without_allocating_empty_meshes() {
        let mut terrain = plateau(-200);
        for cell in &mut terrain.cells {
            cell.surface = TerrainSurface::Water;
        }
        let dry_index = terrain
            .bounds()
            .index(npc_framework::prelude::CellCoord::new(0, 0))
            .unwrap() as usize;
        terrain.cells[dry_index].surface = TerrainSurface::Grass;
        terrain.cells[dry_index].height_mm = 1000;
        let mut app = terrain_view(terrain);
        update_terrain_view(&mut app);
        let world = app.world_mut();
        let mut chunks = world.query::<(&TerrainChunk, Option<&Mesh3d>)>();
        assert_eq!(chunks.iter(world).count(), 16);
        assert_eq!(
            chunks
                .iter(world)
                .filter(|(_, mesh)| mesh.is_none())
                .count(),
            15
        );
        let (chunk, _) = chunks.iter(world).find(|(_, mesh)| mesh.is_some()).unwrap();
        assert_eq!((chunk.x, chunk.z), (0, 0));
        let meshes = world.resource::<Assets<Mesh>>();
        assert_eq!(meshes.len(), 1);
        // One dry top and its four shoreline banks remain real geometry.
        assert_eq!(meshes.iter().next().unwrap().1.count_vertices(), 20);
    }

    #[test]
    fn terrain_chunk_survives_land_water_land_edits_and_season_changes() {
        let mut terrain = plateau(-200);
        terrain.width = 16;
        terrain.height = 16;
        terrain.cells.truncate(256);
        for cell in &mut terrain.cells {
            cell.surface = TerrainSurface::Water;
        }
        let mut app = terrain_view(terrain);
        update_terrain_view(&mut app);
        let chunk = app
            .world_mut()
            .query_filtered::<Entity, With<TerrainChunk>>()
            .single(app.world())
            .unwrap();
        assert!(app.world().get::<Mesh3d>(chunk).is_none());
        assert_eq!(app.world().resource::<Assets<Mesh>>().len(), 0);
        let material = app
            .world()
            .get::<MeshMaterial3d<StandardMaterial>>(chunk)
            .unwrap()
            .clone();

        let dry_index = app
            .world()
            .resource::<TerrainState>()
            .bounds()
            .index(npc_framework::prelude::CellCoord::new(0, 0))
            .unwrap() as usize;
        {
            let mut terrain = app.world_mut().resource_mut::<TerrainState>();
            terrain.cells[dry_index].surface = TerrainSurface::Grass;
            terrain.cells[dry_index].height_mm = 1000;
            terrain.revision += 1;
        }
        update_terrain_view(&mut app);
        let land = app.world().get::<Mesh3d>(chunk).unwrap().clone();
        let autumn_mesh = app
            .world()
            .resource::<Assets<Mesh>>()
            .get(&land.0)
            .unwrap()
            .clone();
        app.world_mut().resource_mut::<SimClock>().tick = 2 * SimConfig::default().day_length;
        update_terrain_view(&mut app);
        assert_eq!(app.world().get::<Mesh3d>(chunk), Some(&land));
        let winter_mesh = app.world().resource::<Assets<Mesh>>().get(&land.0).unwrap();
        assert_eq!(
            autumn_mesh.attribute(Mesh::ATTRIBUTE_POSITION),
            winter_mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        );
        assert_ne!(
            autumn_mesh.attribute(Mesh::ATTRIBUTE_COLOR),
            winter_mesh.attribute(Mesh::ATTRIBUTE_COLOR)
        );

        app.world_mut().entity_mut(chunk).insert(Aabb::default());
        {
            let mut terrain = app.world_mut().resource_mut::<TerrainState>();
            terrain.cells[dry_index].surface = TerrainSurface::Water;
            terrain.cells[dry_index].height_mm = -200;
            terrain.revision += 1;
        }
        update_terrain_view(&mut app);
        assert!(app.world().get::<Mesh3d>(chunk).is_none());
        assert!(app.world().get::<Aabb>(chunk).is_none());
        assert_eq!(app.world().resource::<Assets<Mesh>>().len(), 0);
        app.world_mut().resource_mut::<SimClock>().tick = 9 * SimConfig::default().day_length;
        update_terrain_view(&mut app);
        assert!(app.world().get::<Mesh3d>(chunk).is_none());
        assert_eq!(app.world().resource::<Assets<Mesh>>().len(), 0);
        {
            let mut terrain = app.world_mut().resource_mut::<TerrainState>();
            terrain.cells[dry_index].surface = TerrainSurface::Grass;
            terrain.cells[dry_index].height_mm = 12000;
            terrain.revision += 1;
        }
        update_terrain_view(&mut app);
        let renewed = app.world().get::<Mesh3d>(chunk).unwrap();
        let meshes = app.world().resource::<Assets<Mesh>>();
        assert_eq!(meshes.len(), 1);
        let mesh = meshes.get(&renewed.0).unwrap();
        let Some(bevy::mesh::VertexAttributeValues::Float32x3(positions)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("terrain positions")
        };
        assert_eq!(positions[0], [0.0, 12.0, 0.0]);
        let Some(bevy::mesh::VertexAttributeValues::Float32x4(colors)) =
            mesh.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!("terrain colors")
        };
        assert_eq!(
            colors[0],
            color(TerrainSurface::Grass, Biome::Ridge, 0, 0, Season::Spring)
        );
        assert_eq!(
            app.world().get::<MeshMaterial3d<StandardMaterial>>(chunk),
            Some(&material)
        );
        assert_eq!(
            app.world().get::<Transform>(chunk).unwrap().translation,
            Vec3::ZERO
        );
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, With<TerrainChunk>>()
                .iter(app.world())
                .collect::<Vec<_>>(),
            vec![chunk]
        );
    }

    #[test]
    fn wildlife_and_dropped_goods_follow_rendered_ground_without_revealing_hidden_positions() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_systems(Update, elevation);
        let mut terrain = plateau(4700);
        let seen_index = terrain
            .bounds()
            .index(npc_framework::prelude::CellCoord::new(-10, -10))
            .unwrap() as usize;
        terrain.cells[seen_index].height_mm = 200;
        app.insert_resource(terrain);
        let deer = app
            .world_mut()
            .spawn((
                SimPos::new(0, 0),
                testbed_content::village::hunting::Wildlife { habitat: SimId(7) },
            ))
            .id();
        let item = app
            .world_mut()
            .spawn((
                SimPos::new(10000, 10000),
                npc_framework::prelude::Item::new("log"),
            ))
            .id();
        let deer_visual = app
            .world_mut()
            .spawn((VisualOf(deer), Transform::default()))
            .id();
        let item_visual = app
            .world_mut()
            .spawn((VisualOf(item), Transform::from_xyz(-9.5, 0.0, -9.5)))
            .id();
        app.update();
        assert!(
            (app.world()
                .get::<Transform>(deer_visual)
                .unwrap()
                .translation
                .y
                - 4.7)
                .abs()
                < 0.0001
        );
        assert_eq!(
            app.world()
                .get::<Transform>(item_visual)
                .unwrap()
                .translation,
            Vec3::new(-9.5, 0.2, -9.5)
        );
        assert_eq!(
            *app.world().get::<SimPos>(item).unwrap(),
            SimPos::new(10000, 10000)
        );
    }
    #[test]
    fn restored_player_cottage_is_not_claimed_by_settlement_home_renderer() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<WorldAsset>()
            .init_resource::<VisualIndex>()
            .insert_resource(plateau(4700))
            .add_systems(Startup, spawn_homes);
        let household = || Household {
            label: "Home".into(),
            capacity: 2,
            residents: vec![],
            door: SimPos::new(0, 0),
            heated_until: 0,
            logs_burned: 0,
        };
        let authored = app
            .world_mut()
            .spawn((SimId(1), SimPos::new(0, 0), household()))
            .id();
        let cottage = app
            .world_mut()
            .spawn((
                SimId(2),
                SimPos::new(0, 0),
                household(),
                testbed_content::village::homestead::Cottage {
                    plot: SimId(3),
                    plot_id: "plot".into(),
                },
            ))
            .id();
        let cottage_visual = app.world_mut().spawn(Transform::default()).id();
        app.world_mut()
            .resource_mut::<VisualIndex>()
            .map
            .insert(cottage, cottage_visual);
        app.update();
        let index = app.world().resource::<VisualIndex>();
        assert!(index.map.contains_key(&authored));
        assert_eq!(index.map.get(&cottage), Some(&cottage_visual));
    }
    #[test]
    fn captions_and_fog_cover_the_raised_surface() {
        let terrain = plateau(4700);
        let point = raised_point(Some(&terrain), SimPos::new(500, 500), 2.1);
        assert!((point.y - 6.8).abs() < 0.0001);
        let region = crate::fog::MaskRegion {
            min: npc_framework::prelude::CellCoord::new(0, 0),
            width: 2,
            height: 2,
        };
        let mesh = terrain_mask_mesh(&terrain, region);
        let Some(bevy::mesh::VertexAttributeValues::Float32x3(vertices)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("positions")
        };
        assert!(vertices.iter().all(|point| point[1] > 4.7));
        let Some(bevy::mesh::VertexAttributeValues::Float32x2(uv)) =
            mesh.attribute(Mesh::ATTRIBUTE_UV_0)
        else {
            panic!("mask UV")
        };
        assert_eq!(uv[0], [0.0, 0.0]);
        assert!(uv.contains(&[1.0, 1.0]));
    }
    #[test]
    fn seasons_recolour_without_changing_topology_and_large_oceans_stay_bounded() {
        let terrain = plateau(4700);
        let summer = terrain_mesh(&terrain, 0, 0, Season::Summer).unwrap();
        let winter = terrain_mesh(&terrain, 0, 0, Season::Winter).unwrap();
        assert_eq!(summer.count_vertices(), winter.count_vertices());
        let Some(bevy::mesh::VertexAttributeValues::Float32x4(summer_colors)) =
            summer.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!("summer colors")
        };
        let Some(bevy::mesh::VertexAttributeValues::Float32x4(winter_colors)) =
            winter.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!("winter colors")
        };
        assert_ne!(summer_colors, winter_colors);
        let small = ocean_mesh(0.0, 224.0);
        let large = ocean_mesh(0.0, 352.0);
        assert_eq!(small.count_vertices(), large.count_vertices());
        let Some(bevy::mesh::VertexAttributeValues::Float32x3(points)) =
            large.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("water positions")
        };
        assert!(points.iter().any(|point| point[0] > 256.0));
        assert!(points.iter().any(|point| point[2] < -256.0));
    }
    #[test]
    fn gentle_dry_risers_share_lighting_without_moving_their_geometry() {
        let cells = vec![
            TerrainCell {
                height_mm: 200,
                surface: TerrainSurface::Grass,
                biome: Biome::Ridge,
            },
            TerrainCell {
                height_mm: 0,
                surface: TerrainSurface::Grass,
                biome: Biome::Ridge,
            },
        ];
        let terrain = TerrainState {
            width: 2,
            height: 1,
            cells,
            revision: 0,
        };
        let mesh = terrain_mesh(&terrain, -1, 0, Season::Summer).unwrap();
        let Some(bevy::mesh::VertexAttributeValues::Float32x3(points)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("terrain positions")
        };
        let Some(bevy::mesh::VertexAttributeValues::Float32x3(normals)) =
            mesh.attribute(Mesh::ATTRIBUTE_NORMAL)
        else {
            panic!("terrain normals")
        };
        let Some(bevy::mesh::VertexAttributeValues::Float32x4(colors)) =
            mesh.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!("terrain colors")
        };
        assert_eq!(points.len(), 36);
        assert_eq!(
            &points[8..12],
            &[
                [0.0, 0.2, 1.0],
                [0.0, 0.0, 1.0],
                [0.0, 0.0, 0.0],
                [0.0, 0.2, 0.0]
            ]
        );
        assert_eq!(normals[8], normals[0]);
        assert!(normals[8][1] > 0.94);
        assert_eq!(colors[8], colors[0]);
        assert_eq!(terrain.cells[0].height_mm, 200);
        assert_eq!(terrain.cells[1].height_mm, 0);
    }

    #[test]
    fn shoreline_banks_and_large_cliffs_keep_hard_face_lighting() {
        for (height, surface) in [(200, TerrainSurface::Water), (600, TerrainSurface::Grass)] {
            let terrain = TerrainState {
                width: 2,
                height: 1,
                cells: vec![
                    TerrainCell {
                        height_mm: height,
                        surface: TerrainSurface::Grass,
                        biome: Biome::Ridge,
                    },
                    TerrainCell {
                        height_mm: 0,
                        surface,
                        biome: Biome::Ridge,
                    },
                ],
                revision: 0,
            };
            let mesh = terrain_mesh(&terrain, -1, 0, Season::Summer).unwrap();
            let Some(bevy::mesh::VertexAttributeValues::Float32x3(normals)) =
                mesh.attribute(Mesh::ATTRIBUTE_NORMAL)
            else {
                panic!("terrain normals")
            };
            let Some(bevy::mesh::VertexAttributeValues::Float32x4(colors)) =
                mesh.attribute(Mesh::ATTRIBUTE_COLOR)
            else {
                panic!("terrain colors")
            };
            assert_eq!(normals[8], Vec3::X.to_array());
            assert_eq!(
                colors[8],
                [
                    colors[0][0] * 0.83,
                    colors[0][1] * 0.83,
                    colors[0][2] * 0.83,
                    1.0
                ]
            );
        }
    }

    #[test]
    fn gentle_slope_normals_follow_the_height_gradient_and_remain_upward() {
        let mut terrain = plateau(1000);
        for z in -2..=2 {
            for x in -2..=2 {
                let index = terrain
                    .bounds()
                    .index(npc_framework::prelude::CellCoord::new(x, z))
                    .unwrap() as usize;
                terrain.cells[index].height_mm = 1000 + x * 100;
            }
        }
        let normal = Vec3::from_array(terrain_surface_normal(&terrain, 0, 0));
        assert!(normal.x < -0.09 && normal.x > -0.11);
        assert!(normal.y > 0.99);
        assert!(normal.z.abs() < 0.0001);
    }

    #[test]
    fn adjacent_water_quads_share_smooth_normals_and_colour() {
        let mesh = ocean_mesh(3.0, 192.0);
        let Some(bevy::mesh::VertexAttributeValues::Float32x3(normals)) =
            mesh.attribute(Mesh::ATTRIBUTE_NORMAL)
        else {
            panic!("water normals")
        };
        let Some(bevy::mesh::VertexAttributeValues::Float32x4(colors)) =
            mesh.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!("water colors")
        };
        assert_eq!(normals[3], normals[4]);
        assert_eq!(normals[2], normals[5]);
        assert_eq!(colors[3], colors[4]);
        assert_eq!(colors[2], colors[5]);
    }
    #[test]
    fn ecology_visual_resolves_stable_identity_after_world_replacement() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<SimIndex>()
            .add_systems(Update, resource_lifecycle);
        let patch = |remaining| ResourcePatch {
            kind: "grain".into(),
            remaining,
            capacity: 100,
            replenish_at: 0,
            gather_ticks: 1,
        };
        let old = app.world_mut().spawn(patch(100)).id();
        let id = SimId(42);
        app.world_mut()
            .resource_mut::<SimIndex>()
            .map
            .insert(id, old);
        let root = app.world_mut().spawn(Fogged::default()).id();
        let visual = app
            .world_mut()
            .spawn((
                ChildOf(root),
                ResourceVisual::new(id, 2.0),
                Transform::default(),
                Visibility::Inherited,
            ))
            .id();
        app.update();
        assert_eq!(
            app.world().get::<Transform>(visual).unwrap().scale,
            Vec3::splat(2.0)
        );
        app.world_mut().despawn(old);
        let replacement = app.world_mut().spawn(patch(0)).id();
        app.world_mut()
            .resource_mut::<SimIndex>()
            .map
            .insert(id, replacement);
        app.update();
        assert_eq!(
            app.world().get::<Transform>(visual).unwrap().scale,
            Vec3::splat(0.8)
        );
    }
    #[test]
    fn edited_cell_invalidates_only_touching_chunk_signatures() {
        let cell = TerrainCell {
            height_mm: 0,
            surface: TerrainSurface::Grass,
            biome: Biome::Meadow,
        };
        let mut terrain = TerrainState {
            width: 64,
            height: 64,
            cells: vec![cell; 4096],
            revision: 0,
        };
        let near = chunk_signature(&terrain, 0, 0);
        let far = chunk_signature(&terrain, -32, -32);
        let i = terrain
            .bounds()
            .index(npc_framework::prelude::CellCoord::new(5, 5))
            .unwrap() as usize;
        terrain.cells[i].height_mm = 500;
        assert_ne!(near, chunk_signature(&terrain, 0, 0));
        assert_eq!(far, chunk_signature(&terrain, -32, -32));
    }
    #[test]
    fn pointer_hits_raised_cell_and_rejects_outside_map() {
        let cell = TerrainCell {
            height_mm: 5000,
            surface: TerrainSurface::Grass,
            biome: Biome::Ridge,
        };
        let terrain = TerrainState {
            width: 64,
            height: 64,
            cells: vec![cell; 4096],
            revision: 0,
        };
        let hit = raycast(&terrain, Ray3d::new(Vec3::new(3.5, 20.0, 4.5), Dir3::NEG_Y)).unwrap();
        assert!((hit.y - 5.0).abs() < 0.0001);
        assert!(
            raycast(
                &terrain,
                Ray3d::new(Vec3::new(80.0, 20.0, 4.5), Dir3::NEG_Y)
            )
            .is_none()
        );
    }
}
