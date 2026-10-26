//! Render local and replicated living actors from one authoritative body model.
//! Meshes are shared and bounded; stock/needs updates never rebuild facilities.
use crate::control::{ControlMode, PlayerControl};
use bevy::prelude::*;
use std::collections::{BTreeMap, BTreeSet};
use terra_bevy::{
    NetworkStatus, PoliticalWorld, ResidentChunks, SandboxRuntime, WorldEnvironment, WorldRuntime,
    WorldSet,
    geometry::MeshBuilder,
    render::{actor_mesh_variant, geo_transform, market_mesh, production_mesh},
    rules::RoadRoute,
};
use terra_core::{ActorKind, GeoPosition, Layer, LocalFrame};

pub struct RemoteVisualPlugin;
impl Plugin for RemoteVisualPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, sync_living_visuals.after(WorldSet::Render));
    }
}

#[derive(Component, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum VisualKey {
    Unit(u64),
    Shop(u64),
    Site(u64),
    Road(u64),
    Legacy(u64),
}
#[derive(Component, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum MeshKey {
    Actor(u8, u64),
    Market(u64),
    Facility(String, u64),
    ServiceMarker,
    Road(u64),
}
#[derive(Component)]
struct LivingVisual;
#[derive(Component)]
struct RenderPose {
    position: GeoPosition,
    heading: f32,
}
#[derive(Default)]
struct SharedVisualAssets {
    meshes: BTreeMap<MeshKey, Handle<Mesh>>,
    material: Option<Handle<StandardMaterial>>,
}
struct DesiredVisual {
    key: VisualKey,
    mesh: MeshKey,
    position: GeoPosition,
    heading: f32,
    visible: bool,
    moving: bool,
}

fn heading_lerp(from: f32, to: f32, t: f32) -> f32 {
    from + ((to - from + 540.).rem_euclid(360.) - 180.) * t
}
fn service_marker_mesh() -> Mesh {
    let mut mesh = MeshBuilder::default();
    mesh.cuboid(
        Vec3::new(0., 1., 0.),
        Vec3::new(0.12, 2., 0.12),
        [0.31, 0.23, 0.16, 1.],
    );
    mesh.cuboid(
        Vec3::new(0., 2.05, 0.),
        Vec3::new(1.45, 0.65, 0.16),
        [0.18, 0.42, 0.34, 1.],
    );
    mesh.cuboid(
        Vec3::new(0., 2.05, -0.09),
        Vec3::new(1.05, 0.10, 0.035),
        [0.92, 0.77, 0.41, 1.],
    );
    mesh.finish()
        .expect("service marker contains solid cuboids")
}
fn road_signature(road: &RoadRoute, resident: &ResidentChunks) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    let mut push = |word: u64| {
        hash ^= word;
        hash = hash.wrapping_mul(0x100000001b3);
    };
    for byte in road.id.bytes() {
        push(byte as u64);
    }
    push(road.width_m.to_bits() as u64);
    for point in &road.points {
        push(point.latitude.to_bits());
        push(point.longitude.to_bits());
        push(
            resident
                .terrain_height(*point)
                .unwrap_or(point.altitude_m)
                .to_bits(),
        );
    }
    hash
}

/// A height-map pack may have no semantic roads. Authored settlement routes are
/// draped over that same heightfield and labeled as authored scenario content.
fn authored_road_mesh(road: &RoadRoute, resident: &ResidentChunks) -> Option<Mesh> {
    let anchor = *road.points.first()?;
    let frame = LocalFrame::new(anchor);
    let mut mesh = MeshBuilder::default();
    let sample = |v: Vec3, offset: f64| -> Option<Vec3> {
        let mut p = frame.to_geo(v.to_array().map(|x| x as f64)).ok()?;
        p.altitude_m = resident.terrain_height(p)? + offset;
        Some(Vec3::from_array(frame.to_local(p).map(|x| x as f32)))
    };
    for pair in road.points.windows(2) {
        let a = Vec3::from_array(frame.to_local(pair[0]).map(|x| x as f32));
        let b = Vec3::from_array(frame.to_local(pair[1]).map(|x| x as f32));
        let delta = Vec3::new(b.x - a.x, 0., b.z - a.z);
        let distance = delta.length();
        if distance < 0.1 {
            continue;
        }
        let normal = Vec3::new(-delta.z, 0., delta.x).normalize();
        let half = road.width_m.clamp(2., 30.) * 0.5;
        let segments = (distance / 5.).ceil().clamp(1., 256.) as usize;
        for i in 0..segments {
            let start = a.lerp(b, i as f32 / segments as f32);
            let end = a.lerp(b, (i + 1) as f32 / segments as f32);
            let Some(l0) = sample(start + normal * half, 0.10) else {
                continue;
            };
            let Some(l1) = sample(end + normal * half, 0.10) else {
                continue;
            };
            let Some(r1) = sample(end - normal * half, 0.10) else {
                continue;
            };
            let Some(r0) = sample(start - normal * half, 0.10) else {
                continue;
            };
            mesh.quad(l0, l1, r1, r0, [0.29, 0.30, 0.29, 1.]);
            if i % 2 == 0
                && let (Some(c0), Some(c1), Some(c2), Some(c3)) = (
                    sample(start + normal * 0.07, 0.13),
                    sample(start.lerp(end, 0.62) + normal * 0.07, 0.13),
                    sample(start.lerp(end, 0.62) - normal * 0.07, 0.13),
                    sample(start - normal * 0.07, 0.13),
                )
            {
                mesh.quad(c0, c1, c2, c3, [0.86, 0.81, 0.59, 1.]);
            }
        }
    }
    mesh.finish()
}

#[expect(
    clippy::too_many_arguments,
    clippy::type_complexity,
    reason = "Bevy injects system parameters and the query groups each visual's mutable components"
)]
fn sync_living_visuals(
    mut commands: Commands,
    time: Res<Time>,
    net: Option<Res<NetworkStatus>>,
    sandbox: Res<SandboxRuntime>,
    runtime: Res<WorldRuntime>,
    resident: Res<ResidentChunks>,
    politics: Res<PoliticalWorld>,
    control: Res<PlayerControl>,
    mut existing: Query<
        (
            Entity,
            &VisualKey,
            &mut MeshKey,
            &mut Mesh3d,
            &mut RenderPose,
            &mut Transform,
            &mut Visibility,
        ),
        With<LivingVisual>,
    >,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut assets: Local<SharedVisualAssets>,
) {
    let visible_actors = runtime.config.layers.contains(Layer::Actors) && !politics.atlas;
    let visible_buildings = runtime.config.layers.contains(Layer::Buildings) && !politics.atlas;
    let visible_roads = runtime.config.layers.contains(Layer::Roads) && !politics.atlas;
    let ground = |mut p: GeoPosition| {
        if let Some(h) = resident.terrain_height(p) {
            p.altitude_m = h;
        }
        p
    };
    let mut desired = Vec::new();
    let mut roads = BTreeMap::new();
    if sandbox.state.living.enabled {
        for unit in sandbox.state.living.units.values() {
            let first_person_body =
                control.mode == ControlMode::FirstPerson && control.possessed == Some(unit.id);
            desired.push(DesiredVisual {
                key: VisualKey::Unit(unit.id),
                mesh: MeshKey::Actor(unit.kind as u8, unit.id % 4),
                position: unit.position,
                heading: unit.heading_deg,
                visible: visible_actors && !first_person_body,
                moving: unit.velocity_enu_m_s[0].hypot(unit.velocity_enu_m_s[1]) > 0.1,
            });
        }
        for shop in sandbox.state.living.shops.values() {
            let (mesh, position) = if shop.source_feature.is_some() {
                (MeshKey::ServiceMarker, shop.position)
            } else {
                (MeshKey::Market(shop.id % 4), ground(shop.model_position()))
            };
            desired.push(DesiredVisual {
                key: VisualKey::Shop(shop.id),
                mesh,
                position,
                heading: 0.,
                visible: visible_buildings,
                moving: false,
            });
        }
        for site in sandbox.state.living.producers.values() {
            // Source-linked facilities get a small service marker instead of a
            // second invented building on top of the imported footprint/POI.
            let (mesh, position) = if site.source_feature.is_some() {
                (MeshKey::ServiceMarker, site.position)
            } else {
                (
                    MeshKey::Facility(format!("{:?}", site.kind).to_lowercase(), site.id % 4),
                    ground(site.model_position()),
                )
            };
            desired.push(DesiredVisual {
                key: VisualKey::Site(site.id),
                mesh,
                position,
                heading: 0.,
                visible: visible_buildings,
                moving: false,
            });
        }
        for road in sandbox
            .state
            .living
            .roads
            .iter()
            .filter(|r| r.id.starts_with("authored:"))
            .take(32)
        {
            let Some(anchor) = road.points.first().copied() else {
                continue;
            };
            let hash = road_signature(road, &resident);
            roads.insert(hash, road);
            desired.push(DesiredVisual {
                key: VisualKey::Road(hash),
                mesh: MeshKey::Road(hash),
                position: anchor,
                heading: 0.,
                visible: visible_roads,
                moving: false,
            });
        }
    } else if let Some(net) = &net {
        for actor in net.actors.values() {
            desired.push(DesiredVisual {
                key: VisualKey::Legacy(actor.id),
                mesh: MeshKey::Actor(actor.kind as u8, actor.id % 4),
                position: actor.position,
                heading: actor.heading_deg,
                visible: visible_actors,
                moving: actor.velocity_enu_m_s[0].hypot(actor.velocity_enu_m_s[1]) > 0.1,
            });
        }
    } else if let Some(player) = sandbox.state.players.get(&sandbox.player_id) {
        desired.push(DesiredVisual {
            key: VisualKey::Legacy(player.id),
            mesh: MeshKey::Actor(ActorKind::Human as u8, 0),
            position: player.position,
            heading: 0.,
            visible: visible_actors,
            moving: false,
        });
    }
    // Allocate only newly needed shared assets. Stock, currency, needs and
    // routine snapshots cannot change these keys or trigger geometry work.
    let needed: BTreeSet<_> = desired.iter().map(|v| v.mesh.clone()).collect();
    for key in &needed {
        if assets.meshes.contains_key(key) {
            continue;
        }
        let model = match key {
            MeshKey::Actor(kind, variant) => {
                Some(actor_mesh_variant(ActorKind::ALL[*kind as usize], *variant))
            }
            MeshKey::Market(seed) => Some(market_mesh(*seed)),
            MeshKey::Facility(kind, seed) => Some(production_mesh(kind, *seed)),
            MeshKey::ServiceMarker => Some(service_marker_mesh()),
            MeshKey::Road(hash) => roads
                .get(hash)
                .and_then(|road| authored_road_mesh(road, &resident)),
        };
        if let Some(model) = model {
            assets.meshes.insert(key.clone(), meshes.add(model));
        }
    }
    let wanted: BTreeMap<_, _> = desired.iter().map(|d| (d.key, d)).collect();
    let mut alive = BTreeSet::new();
    for (entity, key, mut mesh_key, mut mesh, mut pose, mut transform, mut visibility) in
        &mut existing
    {
        let Some(target) = wanted.get(key) else {
            commands.entity(entity).despawn();
            continue;
        };
        alive.insert(*key);
        if *mesh_key != target.mesh
            && let Some(handle) = assets.meshes.get(&target.mesh)
        {
            mesh.0 = handle.clone();
            *mesh_key = target.mesh.clone();
        }
        let is_actor = matches!(target.mesh, MeshKey::Actor(..));
        if is_actor && pose.position.surface_distance_m(target.position) < 80. {
            let blend = 1. - (-18. * time.delta_secs_f64().min(0.25)).exp();
            let frame = LocalFrame::new(pose.position);
            let delta = frame.to_local(target.position);
            pose.position = frame
                .to_geo(delta.map(|x| x * blend))
                .unwrap_or(target.position);
            pose.heading = heading_lerp(pose.heading, target.heading, blend as f32);
        } else {
            pose.position = target.position;
            pose.heading = target.heading;
        }
        *transform = geo_transform(pose.position, &runtime.frame);
        transform.rotate_local_y(-pose.heading.to_radians());
        if target.moving && !runtime.clock.paused && matches!(target.mesh, MeshKey::Actor(0..=2, _))
        {
            let id = match key {
                VisualKey::Unit(id) | VisualKey::Legacy(id) => *id,
                _ => 0,
            };
            let phase = time.elapsed_secs() * 9. + (id % 97) as f32;
            transform.translation.y += phase.sin().abs() * 0.025;
            transform.rotate_local_z(phase.sin() * 0.018);
        }
        *visibility = if target.visible {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
    let material = assets
        .material
        .get_or_insert_with(|| {
            materials.add(StandardMaterial {
                base_color: Color::WHITE,
                perceptual_roughness: 0.82,
                ..default()
            })
        })
        .clone();
    for target in desired {
        if alive.contains(&target.key) {
            continue;
        }
        let Some(mesh) = assets.meshes.get(&target.mesh).cloned() else {
            continue;
        };
        let mut transform = geo_transform(target.position, &runtime.frame);
        transform.rotate_local_y(-target.heading.to_radians());
        commands.spawn((
            LivingVisual,
            target.key,
            target.mesh,
            RenderPose {
                position: target.position,
                heading: target.heading,
            },
            Mesh3d(mesh),
            MeshMaterial3d(material.clone()),
            transform,
            if target.visible {
                Visibility::Visible
            } else {
                Visibility::Hidden
            },
        ));
    }
    // Changing worlds or restoring another settlement cannot retain an
    // unbounded collection of stale route/facility mesh assets.
    if assets.meshes.len() > 96 {
        let stale: Vec<_> = assets
            .meshes
            .keys()
            .filter(|key| !needed.contains(*key))
            .cloned()
            .collect();
        for key in stale {
            if let Some(handle) = assets.meshes.remove(&key) {
                meshes.remove(handle.id());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use terra_core::{HeightField, TileKey, WorldChunk};
    #[test]
    fn heading_interpolation_uses_short_route_across_north() {
        assert!((heading_lerp(359., 1., 0.5) - 360.).abs() < 1e-6);
        assert!((heading_lerp(1., 359., 0.5)).abs() < 1e-6);
    }
    #[test]
    fn authored_roads_drape_heightmaps_and_require_loaded_ground() {
        let origin = GeoPosition::new(48.137, 11.575, 20.).unwrap();
        let key = TileKey::at(origin, 15).unwrap();
        let data = WorldChunk {
            key,
            heights: HeightField::constant(20.),
            features: vec![],
            warnings: vec![],
            sources: vec![],
        };
        let resident = ResidentChunks {
            level: 15,
            chunks: BTreeMap::from([(key, Arc::new(data))]),
        };
        let road = RoadRoute {
            id: "authored:test".into(),
            width_m: 6.,
            points: vec![
                origin,
                LocalFrame::new(origin).to_geo([30., 0., 0.]).unwrap(),
            ],
        };
        let mesh = authored_road_mesh(&road, &resident).expect("loaded road terrain");
        assert!(mesh.count_vertices() > 24);
        assert!(
            authored_road_mesh(
                &road,
                &ResidentChunks {
                    level: 15,
                    ..default()
                }
            )
            .is_none()
        );
    }
}
