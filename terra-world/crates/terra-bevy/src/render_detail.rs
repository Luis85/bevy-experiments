//! Bounded authored presentation for natural resources, local ground cover and
//! gameplay territories. These meshes never alter collision or source data.
use super::{LayerVisual, RenderQuality, RenderQualitySettings, geo_transform};
use crate::{
    GeoAnchor, ResidentChunks, SandboxRuntime, WorldEnvironment, WorldRuntime,
    environment::{inside_polygon, inside_ribbon, road_width_m},
    geometry::MeshBuilder,
};
use bevy::prelude::*;
use std::collections::{BTreeMap, BTreeSet};
use std::hash::{Hash, Hasher};
use terra_core::{FeatureKind, GeoPosition, Layer, LocalFrame, TileKey};

/// Optional detail policy for embedding games. All requested budgets are also
/// hard-capped by the renderer; turning this off does not pause ecology.
#[derive(Resource, Clone)]
pub struct WorldDetailSettings {
    pub ground_cover: bool,
    pub ground_radius_m: f64,
    pub ground_clusters: usize,
    pub natural_resources: bool,
    pub settlement_boundaries: bool,
}
impl Default for WorldDetailSettings {
    fn default() -> Self {
        Self {
            ground_cover: true,
            ground_radius_m: 96.,
            ground_clusters: 640,
            natural_resources: true,
            settlement_boundaries: true,
        }
    }
}
#[derive(Resource, Default)]
pub(super) struct DetailState {
    material: Option<Handle<StandardMaterial>>,
    water_material: Option<Handle<StandardMaterial>>,
    marker_material: Option<Handle<StandardMaterial>>,
    resources: BTreeMap<String, (Entity, String)>,
    resource_meshes: BTreeMap<String, Handle<Mesh>>,
    ground: Option<(Entity, Handle<Mesh>)>,
    ground_focus: Option<GeoPosition>,
    ground_signature: u64,
    next_ground_refresh: f64,
    territories: Vec<(Entity, Handle<Mesh>)>,
    territory_signature: u64,
}

pub(super) fn render_world_detail(
    mut commands: Commands,
    runtime: Res<WorldRuntime>,
    resident: Res<ResidentChunks>,
    sandbox: Res<SandboxRuntime>,
    settings: Res<WorldDetailSettings>,
    quality: Res<RenderQualitySettings>,
    time: Res<Time>,
    mut state: ResMut<DetailState>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mut settings = (*settings).clone();
    settings.ground_clusters = settings
        .ground_clusters
        .min(quality.quality.ground_cluster_limit());
    if quality.quality == RenderQuality::Performance {
        settings.ground_radius_m = settings.ground_radius_m.min(64.);
    }
    let material = state
        .material
        .get_or_insert_with(|| {
            materials.add(StandardMaterial {
                perceptual_roughness: 0.94,
                cull_mode: None,
                ..default()
            })
        })
        .clone();
    let water_material = state
        .water_material
        .get_or_insert_with(|| {
            materials.add(StandardMaterial {
                perceptual_roughness: 0.24,
                clearcoat: 0.35,
                cull_mode: None,
                ..default()
            })
        })
        .clone();
    let marker_material = state
        .marker_material
        .get_or_insert_with(|| {
            materials.add(StandardMaterial {
                unlit: true,
                cull_mode: None,
                ..default()
            })
        })
        .clone();
    let mut visible: Vec<_> = sandbox
        .state
        .ecology
        .resources
        .keys()
        .filter_map(|id| sandbox.state.resource_nodes.get(id))
        .filter(|n| {
            settings.natural_resources
                && n.position.surface_distance_m(runtime.focus) <= 2200.
                && resident.terrain_height(n.position).is_some()
        })
        .collect();
    visible.sort_by(|a, b| {
        a.position
            .surface_distance_m(runtime.focus)
            .total_cmp(&b.position.surface_distance_m(runtime.focus))
            .then(a.id.cmp(&b.id))
    });
    visible.truncate(128);
    let wanted: BTreeSet<_> = visible.iter().map(|n| n.id.clone()).collect();
    let stale: Vec<_> = state
        .resources
        .keys()
        .filter(|id| !wanted.contains(*id))
        .cloned()
        .collect();
    for id in stale {
        if let Some((entity, _)) = state.resources.remove(&id) {
            commands.entity(entity).despawn();
        }
    }
    for node in visible {
        let stage = resource_stage(node.remaining, node.capacity);
        let variant = stable_hash(node.id.as_bytes()) % 3;
        let item = match node.item.as_str() {
            "wood" | "stone" | "berries" | "fiber" | "fish" | "iron_ore" | "water" => {
                node.item.as_str()
            }
            _ => "stone",
        };
        let key = format!("{item}:{stage}:{variant}");
        let visual_key = format!(
            "{key}:{}:{}:{}",
            node.position.latitude.to_bits(),
            node.position.longitude.to_bits(),
            node.position.altitude_m.to_bits()
        );
        if state
            .resources
            .get(&node.id)
            .is_some_and(|(_, old)| old == &visual_key)
        {
            continue;
        }
        if let Some((old, _)) = state.resources.remove(&node.id) {
            commands.entity(old).despawn();
        }
        let mesh = state
            .resource_meshes
            .entry(key.clone())
            .or_insert_with(|| meshes.add(natural_resource_mesh(item, stage, variant as u8)))
            .clone();
        let entity = commands
            .spawn((
                GeoAnchor(node.position),
                geo_transform(node.position, &runtime.frame),
                LayerVisual(if matches!(item, "wood" | "berries" | "fiber") {
                    Layer::Vegetation
                } else {
                    Layer::Terrain
                }),
                Mesh3d(mesh),
                MeshMaterial3d(if matches!(item, "water" | "fish") {
                    water_material.clone()
                } else {
                    material.clone()
                }),
            ))
            .id();
        state
            .resources
            .insert(node.id.clone(), (entity, visual_key));
    }
    // Resource geometry keys are finite (seven types × three stages × three variants).
    // Routine quantity updates change meshes only when crossing a depletion stage.
    let mut sig = std::collections::hash_map::DefaultHasher::new();
    resident_terrain_signature(&resident).hash(&mut sig);
    for structure in sandbox.state.structures.values() {
        structure.id.hash(&mut sig);
        structure.position.latitude.to_bits().hash(&mut sig);
        structure.position.longitude.to_bits().hash(&mut sig);
        structure.radius_m.to_bits().hash(&mut sig);
    }
    for road in sandbox
        .state
        .living
        .roads
        .iter()
        .filter(|r| r.id.starts_with("authored:"))
    {
        road.id.hash(&mut sig);
        road.width_m.to_bits().hash(&mut sig);
    }
    runtime.config.base_data_revision.hash(&mut sig);
    settings.ground_cover.hash(&mut sig);
    settings.ground_clusters.min(2048).hash(&mut sig);
    settings.ground_radius_m.to_bits().hash(&mut sig);
    let ground_signature = sig.finish();
    let moved = state
        .ground_focus
        .is_none_or(|p| p.surface_distance_m(runtime.focus) > 24.);
    if (moved || state.ground_signature != ground_signature)
        && time.elapsed_secs_f64() >= state.next_ground_refresh
    {
        state.next_ground_refresh = time.elapsed_secs_f64() + 0.8;
        state.ground_focus = Some(runtime.focus);
        state.ground_signature = ground_signature;
        if let Some((entity, handle)) = state.ground.take() {
            commands.entity(entity).despawn();
            meshes.remove(handle.id());
        }
        if settings.ground_cover {
            if let Some(mesh) = ground_cover_mesh(&resident, &sandbox, runtime.focus, &settings) {
                let handle = meshes.add(mesh);
                let entity = commands
                    .spawn((
                        GeoAnchor(runtime.focus),
                        geo_transform(runtime.focus, &runtime.frame),
                        LayerVisual(Layer::Vegetation),
                        Mesh3d(handle.clone()),
                        MeshMaterial3d(material.clone()),
                    ))
                    .id();
                state.ground = Some((entity, handle));
            }
        }
    }
    let mut sig = std::collections::hash_map::DefaultHasher::new();
    ground_signature.hash(&mut sig);
    settings.settlement_boundaries.hash(&mut sig);
    for claim in sandbox.state.settlements.territories.values().take(64) {
        claim.id.hash(&mut sig);
        claim.tier.radius_m().to_bits().hash(&mut sig);
        claim.center.latitude.to_bits().hash(&mut sig);
        claim.center.longitude.to_bits().hash(&mut sig);
        claim.center.altitude_m.to_bits().hash(&mut sig);
        sandbox
            .state
            .may_build(sandbox.player_id, claim.faction_owner, claim.founder)
            .hash(&mut sig);
    }
    let territory_signature = sig.finish();
    if territory_signature != state.territory_signature {
        state.territory_signature = territory_signature;
        for (entity, handle) in state.territories.drain(..) {
            commands.entity(entity).despawn();
            meshes.remove(handle.id());
        }
        if settings.settlement_boundaries {
            for claim in sandbox.state.settlements.territories.values().take(64) {
                let owned =
                    sandbox
                        .state
                        .may_build(sandbox.player_id, claim.faction_owner, claim.founder);
                let color = if owned {
                    [0.56, 0.78, 0.47, 1.]
                } else {
                    [0.84, 0.59, 0.33, 1.]
                };
                let radius = claim.tier.radius_m();
                if let Some(mesh) = territory_mesh(&resident, claim.center, radius, color) {
                    let handle = meshes.add(mesh);
                    let entity = commands
                        .spawn((
                            GeoAnchor(claim.center),
                            geo_transform(claim.center, &runtime.frame),
                            LayerVisual(Layer::Buildings),
                            Mesh3d(handle.clone()),
                            MeshMaterial3d(marker_material.clone()),
                        ))
                        .id();
                    state.territories.push((entity, handle));
                }
            }
        }
    }
}

fn resource_stage(remaining: u32, capacity: u32) -> u8 {
    if remaining == 0 {
        0
    } else if remaining.saturating_mul(3) < capacity {
        1
    } else {
        2
    }
}
fn stable_hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325, |h, b| {
        (h ^ *b as u64).wrapping_mul(0x100000001b3)
    })
}

/// Loaded source pages are immutable Arc values. Their bounded identities are
/// enough to refresh terrain-dependent overlays without hashing every sample.
pub(super) fn resident_terrain_signature(resident: &ResidentChunks) -> u64 {
    let mut signature = std::collections::hash_map::DefaultHasher::new();
    resident.level.hash(&mut signature);
    for (key, chunk) in &resident.chunks {
        key.hash(&mut signature);
        (std::sync::Arc::as_ptr(chunk) as usize).hash(&mut signature);
    }
    signature.finish()
}

/// Drape one bounded boundary ribbon segment. Checking the small rectangle of
/// touched source tiles prevents loaded endpoints from bridging an unloaded
/// page between them, including when tile X wraps at the date line.
pub(super) fn terrain_boundary_quad(
    resident: &ResidentChunks,
    frame: &LocalFrame,
    corners: [[f64; 3]; 4],
    lift_m: f64,
) -> Option<[Vec3; 4]> {
    let mut points = [Vec3::ZERO; 4];
    let mut keys = Vec::with_capacity(9);
    for (index, corner) in corners.iter().enumerate() {
        let mut point = frame.to_geo(*corner).ok()?;
        keys.push(TileKey::at(point, resident.level).ok()?);
        point.altitude_m = resident.terrain_height(point)? + lift_m;
        points[index] = Vec3::from_array(frame.to_local(point).map(|v| v as f32));
    }
    // Include edge midpoints and the center when deriving tile coverage. This
    // also keeps small curved geographic projections conservative near poles.
    for index in 0..4 {
        let middle = std::array::from_fn(|axis| {
            (corners[index][axis] + corners[(index + 1) % 4][axis]) * 0.5
        });
        keys.push(TileKey::at(frame.to_geo(middle).ok()?, resident.level).ok()?);
    }
    let middle = std::array::from_fn(|axis| corners.iter().map(|p| p[axis]).sum::<f64>() * 0.25);
    keys.push(TileKey::at(frame.to_geo(middle).ok()?, resident.level).ok()?);
    let side = 1_i64.checked_shl(u32::from(resident.level))?;
    let origin_x = i64::from(keys[0].x);
    let relative_x = |x: u32| (i64::from(x) - origin_x + side / 2).rem_euclid(side) - side / 2;
    let min_x = keys.iter().map(|key| relative_x(key.x)).min()?;
    let max_x = keys.iter().map(|key| relative_x(key.x)).max()?;
    let min_y = keys.iter().map(|key| key.y).min()?;
    let max_y = keys.iter().map(|key| key.y).max()?;
    // A polar projection that spans too many pages is omitted rather than
    // performing unbounded work or inventing a connecting surface.
    if (max_x - min_x + 1) * i64::from(max_y - min_y + 1) > 16 {
        return None;
    }
    for x in min_x..=max_x {
        for y in min_y..=max_y {
            let key = TileKey {
                level: resident.level,
                x: (origin_x + x).rem_euclid(side) as u32,
                y,
            };
            if !resident.chunks.contains_key(&key) {
                return None;
            }
        }
    }
    points
        .iter()
        .all(|point| point.is_finite())
        .then_some(points)
}
fn ground_cover_mesh(
    resident: &ResidentChunks,
    sandbox: &SandboxRuntime,
    center: GeoPosition,
    settings: &WorldDetailSettings,
) -> Option<Mesh> {
    let radius = if settings.ground_radius_m.is_finite() {
        settings.ground_radius_m.clamp(24., 180.)
    } else {
        96.
    };
    let budget = settings.ground_clusters.min(2048);
    let frame = LocalFrame::new(center);
    let mut mesh = MeshBuilder::default();
    let weather = terra_core::WeatherSample::default();
    let mut footprints: Vec<_> = sandbox
        .state
        .structures
        .values()
        .filter(|s| s.position.surface_distance_m(center) < radius + s.radius_m)
        .take(512)
        .map(|s| (s.position, s.radius_m))
        .collect();
    footprints.extend(sandbox.state.living.facility_footprints());
    let mut rng = 0x9e3779b97f4a7c15_u64;
    let mut random = || {
        rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1);
        (rng >> 32) as f64 / u32::MAX as f64
    };
    for cluster in 0..budget {
        let distance = radius * random().sqrt();
        let angle = random() * std::f64::consts::TAU;
        let mut p = frame
            .to_geo([angle.cos() * distance, 0., angle.sin() * distance])
            .ok()?;
        let Some(sample) = resident.sample_environment(p, &weather) else {
            continue;
        };
        if sample.water || sample.slope_degrees > 48. {
            continue;
        }
        p.altitude_m = sample.terrain_height_m + 0.035;
        let Some(key) = TileKey::at(p, resident.level).ok() else {
            continue;
        };
        let Some(chunk) = resident.chunks.get(&key) else {
            continue;
        };
        if chunk.features.iter().any(|f| match f.kind {
            FeatureKind::Building | FeatureKind::Farm | FeatureKind::Water => {
                inside_polygon(p, &f.points)
            }
            FeatureKind::Road => inside_ribbon(p, &f.points, road_width_m(f) + 1.0),
            FeatureKind::Waterway => inside_ribbon(p, &f.points, f.height_m.max(1.) as f64 + 1.0),
            _ => false,
        }) {
            continue;
        }
        if footprints
            .iter()
            .any(|(position, r)| position.surface_distance_m(p) < r + 1.)
            || sandbox
                .state
                .living
                .roads
                .iter()
                .filter(|r| r.id.starts_with("authored:"))
                .any(|r| inside_ribbon(p, &r.points, r.width_m as f64 + 1.))
        {
            continue;
        }
        let base = Vec3::from_array(frame.to_local(p).map(|v| v as f32));
        for blade in 0..7 {
            let angle = random() as f32 * std::f32::consts::TAU;
            let offset = Vec3::new(angle.cos(), 0., angle.sin()) * random() as f32 * 1.0;
            let height = 0.14 + random() as f32 * 0.34;
            let width = 0.035 + random() as f32 * 0.035;
            let side = Vec3::new(angle.sin(), 0., -angle.cos()) * width;
            let bend = Vec3::new(angle.cos(), 0., angle.sin()) * height * 0.3;
            let mut bottom = base + offset;
            if let Ok(at) = frame.to_geo(bottom.to_array().map(f64::from)) {
                if let Some(h) = resident.terrain_height(at) {
                    bottom.y += (h - at.altitude_m + 0.035) as f32;
                }
            }
            let middle = bottom + Vec3::Y * height * 0.6 + bend * 0.3;
            let tip = bottom + Vec3::Y * height + bend;
            let color = if (blade + cluster) % 4 == 0 {
                [0.43, 0.48, 0.23, 1.]
            } else {
                [0.24, 0.37, 0.15, 1.]
            };
            mesh.quad(
                bottom - side,
                bottom + side,
                middle + side * 0.5,
                middle - side * 0.5,
                color,
            );
            mesh.triangle(middle - side * 0.5, middle + side * 0.5, tip, color);
        }
        if cluster % 23 == 0 {
            let top = base + Vec3::Y * 0.30;
            mesh.cylinder(base, top, 0.014, [0.29, 0.38, 0.16, 1.], 5);
            mesh.ellipsoid(top, Vec3::new(0.07, 0.035, 0.07), [0.83, 0.72, 0.35, 1.], 6);
        }
    }
    mesh.finish()
}

/// Depletion retains a readable stump/outcrop/plant base, instead of making a
/// resource disappear and silently respawn. Meshes are shared by state/variant.
pub fn natural_resource_mesh(item: &str, stage: u8, variant: u8) -> Mesh {
    let mut b = MeshBuilder::default();
    let full = stage > 0;
    let factor = if stage == 1 { 0.65 } else { 1. };
    let shift = variant.min(2) as f32 * 0.12;
    match item {
        "wood" => {
            b.cylinder(Vec3::ZERO, Vec3::Y * 0.48, 0.40, [0.30, 0.21, 0.13, 1.], 10);
            b.cylinder(
                Vec3::Y * 0.485,
                Vec3::Y * 0.50,
                0.33,
                [0.61, 0.46, 0.27, 1.],
                10,
            );
            if full {
                for i in 0..if stage == 1 { 2 } else { 4 } {
                    let start = Vec3::new(
                        -1.1 + i as f32 * 0.18,
                        0.24 + i as f32 * 0.15,
                        0.8 + i as f32 * 0.26,
                    );
                    let end = start + Vec3::new(2.3 + shift, 0.08, 0.10);
                    b.cylinder(start, end, 0.18, [0.35, 0.25, 0.14, 1.], 8);
                    b.cylinder(end, end + Vec3::X * 0.015, 0.15, [0.65, 0.50, 0.30, 1.], 8);
                }
            }
        }
        "berries" | "fiber" => {
            for i in 0..5 {
                let angle = i as f32 * 2.3 + shift;
                let p = Vec3::new(angle.cos() * 0.52, 0., angle.sin() * 0.52);
                b.cylinder(p, p + Vec3::Y * 0.55, 0.035, [0.29, 0.23, 0.13, 1.], 5);
                if full {
                    b.ellipsoid(
                        p + Vec3::Y * (0.55 + shift),
                        Vec3::new(0.40, 0.37, 0.36) * factor,
                        [0.20, 0.35, 0.15, 1.],
                        8,
                    );
                    if item == "berries" {
                        for j in 0..3 {
                            let a = j as f32 * 2.1 + angle;
                            b.ellipsoid(
                                p + Vec3::new(
                                    a.cos() * 0.32,
                                    0.65 + j as f32 * 0.07,
                                    a.sin() * 0.32,
                                ),
                                Vec3::splat(0.055),
                                [0.49, 0.12, 0.19, 1.],
                                6,
                            );
                        }
                    } else {
                        b.cylinder(
                            p,
                            p + Vec3::Y * (1.05 + shift),
                            0.024,
                            [0.47, 0.49, 0.25, 1.],
                            5,
                        );
                        b.ellipsoid(
                            p + Vec3::Y * (1.05 + shift),
                            Vec3::new(0.07, 0.14, 0.07),
                            [0.64, 0.58, 0.34, 1.],
                            6,
                        );
                    }
                }
            }
        }
        "water" | "fish" => {
            for i in 0..9 {
                let a = i as f32 * std::f32::consts::TAU / 9.;
                b.ellipsoid(
                    Vec3::new(a.cos() * 0.85, 0.15, a.sin() * 0.85),
                    Vec3::new(0.28, 0.20, 0.24),
                    [0.40, 0.43, 0.40, 1.],
                    6,
                );
            }
            if full {
                b.ellipsoid(
                    Vec3::new(0., 0.10, 0.),
                    Vec3::new(0.82, 0.04, 0.82),
                    [0.15, 0.36, 0.42, 1.],
                    12,
                );
                if item == "fish" {
                    b.cylinder(
                        Vec3::new(-0.7, 0.2, 0.1),
                        Vec3::new(-0.5, 1.7, -0.4),
                        0.025,
                        [0.48, 0.35, 0.20, 1.],
                        6,
                    );
                    b.cylinder(
                        Vec3::new(-0.5, 1.7, -0.4),
                        Vec3::new(0.3, 0.2, -0.1),
                        0.008,
                        [0.65, 0.65, 0.50, 1.],
                        5,
                    );
                }
            }
        }
        _ => {
            for i in 0..if full { 5 } else { 3 } {
                let a = i as f32 * 2.4 + shift;
                let p = Vec3::new(
                    a.cos() * 0.62,
                    if full { 0.28 + i as f32 * 0.10 } else { 0.09 },
                    a.sin() * 0.62,
                );
                let size = if full {
                    Vec3::new(0.55, 0.42, 0.48) * factor
                } else {
                    Vec3::new(0.33, 0.13, 0.28)
                };
                b.ellipsoid(
                    p,
                    size,
                    [0.37 + shift * 0.14, 0.40 + shift * 0.1, 0.39, 1.],
                    6,
                );
                if full && item == "iron_ore" {
                    b.ellipsoid(
                        p + Vec3::new(-0.06, size.y * 0.68, -0.14),
                        size * Vec3::new(0.58, 0.22, 0.43),
                        [0.48, 0.24, 0.13, 1.],
                        6,
                    );
                }
            }
        }
    }
    b.finish().expect("resource includes a persistent base")
}

fn territory_mesh(
    resident: &ResidentChunks,
    center: GeoPosition,
    radius: f64,
    color: [f32; 4],
) -> Option<Mesh> {
    let frame = LocalFrame::new(center);
    let mut b = MeshBuilder::default();
    let radius = radius.clamp(10., 1000.);
    let segments = ((radius * std::f64::consts::TAU / 12.).ceil() as usize).clamp(64, 512);
    let point = |angle: f64, r: f64| [angle.cos() * r, 0., angle.sin() * r];
    for i in 0..segments {
        // A fine dashed survey line leaves the underlying land legible.
        if i % 4 == 3 {
            continue;
        }
        let a = i as f64 * std::f64::consts::TAU / segments as f64;
        let c = (i + 1) as f64 * std::f64::consts::TAU / segments as f64;
        let width = if radius > 400. { 0.85 } else { 0.52 };
        if let Some([a0, a1, c1, c0]) = terrain_boundary_quad(
            resident,
            &frame,
            [
                point(a, radius - width),
                point(a, radius + width),
                point(c, radius + width),
                point(c, radius - width),
            ],
            0.28,
        ) {
            b.quad(a0, a1, c1, c0, color);
        }
    }
    if let Some(height) = resident.terrain_height(center) {
        let base = Vec3::Y * (height - center.altitude_m) as f32;
        // A non-solid ownership marker, not an invented building or collider.
        let tall = if radius > 400. {
            5.5
        } else if radius > 150. {
            4.5
        } else {
            3.5
        };
        b.cylinder(
            base,
            base + Vec3::Y * tall,
            0.055,
            [0.48, 0.45, 0.34, 1.],
            8,
        );
        let flags = if radius > 400. {
            3
        } else if radius > 150. {
            2
        } else {
            1
        };
        for i in 0..flags {
            let y = tall - 0.2 - i as f32 * 0.7;
            b.quad(
                base + Vec3::new(0., y, 0.),
                base + Vec3::new(1.45, y - 0.12, 0.06),
                base + Vec3::new(1.15, y - 0.58, 0.04),
                base + Vec3::new(0., y - 0.5, 0.),
                color,
            );
        }
    }
    b.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::mesh::VertexAttributeValues;
    #[test]
    fn resource_depletion_changes_readable_geometry_with_bounded_palette() {
        for item in [
            "wood", "stone", "berries", "fiber", "fish", "iron_ore", "water",
        ] {
            let mut sizes = Vec::new();
            for stage in 0..3 {
                let mesh = natural_resource_mesh(item, stage, 1);
                let VertexAttributeValues::Float32x3(points) =
                    mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap()
                else {
                    panic!()
                };
                assert!(points.len() < 6000);
                assert!(points.iter().flatten().all(|p| p.is_finite()));
                sizes.push(points.len());
            }
            assert!(sizes[2] > sizes[0], "{item} should expose depletion");
        }
        assert_eq!(resource_stage(0, 100), 0);
        assert_eq!(resource_stage(1, 100), 1);
        assert_eq!(resource_stage(100, 100), 2);
    }
    #[test]
    fn ground_cover_is_bounded_and_never_fills_water_or_unknown_coverage() {
        use std::sync::Arc;
        use terra_core::{HeightField, WorldChunk, WorldFeature};
        let center = GeoPosition::new(48.137, 11.575, 60.).unwrap();
        let key = TileKey::at(center, 15).unwrap();
        let mut resident = ResidentChunks {
            level: 15,
            ..default()
        };
        let sandbox = SandboxRuntime::new(center);
        let settings = WorldDetailSettings {
            ground_clusters: 64,
            ground_radius_m: 24.,
            ..default()
        };
        assert!(ground_cover_mesh(&resident, &sandbox, center, &settings).is_none());
        for k in key.neighbors(1) {
            resident.chunks.insert(
                k,
                Arc::new(WorldChunk {
                    key: k,
                    heights: HeightField::constant(60.),
                    features: vec![],
                    sources: vec![],
                    warnings: vec![],
                }),
            );
        }
        let mesh = ground_cover_mesh(&resident, &sandbox, center, &settings).unwrap();
        let VertexAttributeValues::Float32x3(points) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap()
        else {
            panic!()
        };
        assert!(!points.is_empty() && points.len() < 10_000);
        assert!(points.iter().flatten().all(|v| v.is_finite()));
        for (k, data) in &mut resident.chunks {
            let bounds = k.bounds();
            let polygon = vec![
                bounds.point(0., 0., 60.).unwrap(),
                bounds.point(1., 0., 60.).unwrap(),
                bounds.point(1., 1., 60.).unwrap(),
                bounds.point(0., 1., 60.).unwrap(),
            ];
            Arc::make_mut(data).features.push(WorldFeature {
                id: format!("water:{}:{}", k.x, k.y),
                kind: FeatureKind::Water,
                points: polygon,
                height_m: 0.,
                properties: BTreeMap::new(),
            });
        }
        assert!(ground_cover_mesh(&resident, &sandbox, center, &settings).is_none());
    }
    #[test]
    fn territory_lines_require_resident_height_and_drape_to_it() {
        use std::sync::Arc;
        use terra_core::{HeightField, WorldChunk};
        let center = GeoPosition::new(48.137, 11.575, 10.).unwrap();
        let key = TileKey::at(center, 15).unwrap();
        let mut resident = ResidentChunks::default();
        resident.level = 15;
        assert!(territory_mesh(&resident, center, 100., [1.; 4]).is_none());
        for k in key.neighbors(1) {
            resident.chunks.insert(
                k,
                Arc::new(WorldChunk {
                    key: k,
                    heights: HeightField::constant(60.),
                    features: vec![],
                    sources: vec![],
                    warnings: vec![],
                }),
            );
        }
        let mesh = territory_mesh(&resident, center, 100., [1.; 4]).unwrap();
        let VertexAttributeValues::Float32x3(points) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap()
        else {
            panic!()
        };
        assert!(points.iter().all(|p| p[1] > 49.9));
        assert!(points.len() < 6000);
    }
}
