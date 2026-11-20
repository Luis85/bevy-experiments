//! Render local and replicated living actors from one authoritative body model.
//! Meshes are shared and bounded; stock/needs updates never rebuild facilities.
use crate::control::{ControlMode, PlayerControl};
use bevy::prelude::*;
use std::collections::{BTreeMap, BTreeSet};
use terra_bevy::{
    NetworkStatus, PoliticalWorld, ResidentChunks, SandboxRuntime, WorldEnvironment, WorldRuntime,
    WorldSet,
    geometry::MeshBuilder,
    render::{
        ActorSceneActive, ActorSceneAnchor, ProceduralActorMesh, WILDLIFE_DRINKING,
        WILDLIFE_GRAZING, WILDLIFE_POSE_COUNT, WILDLIFE_RESTING, WILDLIFE_STANDING,
        actor_mesh_variant, geo_transform, market_mesh, production_mesh, wildlife_mesh,
    },
    rules::{AnimalSpecies, DeliveryPhase, RoadRoute, UnitRole, WildlifeBehavior},
};
use terra_core::{ActorKind, GeoPosition, Layer, LocalFrame};

#[path = "road_presentation.rs"]
mod road_presentation;

pub struct RemoteVisualPlugin;
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct ActorVisuals;
impl Plugin for RemoteVisualPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (sync_living_visuals, place_known_drivers)
                .chain()
                .in_set(ActorVisuals)
                .after(WorldSet::Render),
        );
    }
}

#[derive(Component, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum VisualKey {
    Unit(u64),
    Cargo(u64),
    Shop(u64),
    Site(u64),
    Road(u64),
    RoadPaint,
    Legacy(u64),
    Trailer(u64),
    TransportFacility(u64),
}
#[derive(Component, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum MeshKey {
    Actor(u8, u64),
    Vehicle(u8, u8),
    Profile(u8, u8, u8, u8),
    SeatedProfile(u8, u8, u8, u8),
    Trailer,
    TransportMarker(u8),
    Wildlife(AnimalSpecies, u8),
    Hauler(u8),
    Cargo(u8),
    Market(u64),
    Facility(String, u64),
    ServiceMarker,
    Road(u64),
    RoadPaint(u64),
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
    // A source/terrain signature change retries unavailable paint; holding the
    // same unloaded neighborhood does not rebuild an empty mesh every frame.
    unavailable_paint: Option<u64>,
}
struct DesiredVisual {
    key: VisualKey,
    mesh: MeshKey,
    position: GeoPosition,
    heading: f32,
    visible: bool,
    moving: bool,
    actor: Option<ActorSceneAnchor>,
}

/// Place the single known driver on the already interpolated and terrain-aligned
/// vehicle visual. The original living avatar never moves, disappears from the
/// save, gains control or changes occupancy because of a presentation pack.
fn place_known_drivers(
    registry: Res<terra_bevy::render::ActorSceneOverrides>,
    sandbox: Res<SandboxRuntime>,
    mut roots: Query<(
        &ActorSceneAnchor,
        &mut Transform,
        &mut Visibility,
        Option<&ActorSceneActive>,
    )>,
) {
    let vehicles: BTreeMap<_, _> = roots
        .iter()
        .filter(|(anchor, _, _, _)| anchor.kind == ActorKind::Vehicle)
        .map(|(anchor, transform, visibility, active)| {
            (anchor.body_id, (*transform, *visibility, active.is_some()))
        })
        .collect();
    for (anchor, mut transform, mut visibility, _) in &mut roots {
        if let Some((vehicle, seat)) =
            terra_bevy::render::driver_seat_presentation(&registry, &sandbox.state, anchor.body_id)
        {
            if let Some((vehicle_transform, vehicle_visibility, cabin_ready)) =
                vehicles.get(&vehicle)
            {
                *transform = *vehicle_transform * Transform::from_translation(seat);
                // The procedural vehicle has a closed cabin/chassis. A seat
                // declaration alone cannot make that geometry fit a person.
                // Keep the vehicle fallback, and show a rider only when its
                // selected cabin-capable replacement is actually visible.
                if !*cabin_ready || *vehicle_visibility == Visibility::Hidden {
                    *visibility = Visibility::Hidden;
                }
            } else {
                *visibility = Visibility::Hidden;
            }
        }
    }
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

fn transport_marker_mesh(kind: u8) -> Mesh {
    let mut b = MeshBuilder::default();
    use terra_bevy::rules::TransportFacilityKind;
    let garage = kind == TransportFacilityKind::Garage as u8;
    let color = if garage {
        [0.18, 0.42, 0.36, 1.]
    } else if kind == TransportFacilityKind::Hospital as u8 {
        [0.72, 0.24, 0.2, 1.]
    } else {
        [0.2, 0.34, 0.45, 1.]
    };
    if garage {
        b.cuboid(
            Vec3::new(0., 0.03, 0.),
            Vec3::new(10., 0.06, 7.),
            [0.42, 0.43, 0.39, 1.],
        );
        b.cuboid(Vec3::new(0., 4.25, 0.), Vec3::new(10.5, 0.28, 7.5), color);
        b.cuboid(
            Vec3::new(0., 4.48, 0.),
            Vec3::new(10.1, 0.18, 7.1),
            [0.82, 0.81, 0.72, 1.],
        );
        for x in [-4.6, 4.6] {
            for z in [-3., 3.] {
                b.cuboid(
                    Vec3::new(x, 2.1, z),
                    Vec3::new(0.18, 4.2, 0.18),
                    [0.44, 0.48, 0.45, 1.],
                );
            }
        }
        for x in [-3.6, 3.6] {
            b.cuboid(
                Vec3::new(x, 0.78, 0.),
                Vec3::new(0.65, 1.55, 0.6),
                [0.78, 0.74, 0.59, 1.],
            );
            b.cuboid(
                Vec3::new(x, 1.2, -0.31),
                Vec3::new(0.43, 0.28, 0.03),
                [0.09, 0.17, 0.17, 1.],
            );
            b.cylinder(
                Vec3::new(x + 0.43, 1.3, 0.),
                Vec3::new(x + 0.43, 0.4, 0.),
                0.035,
                [0.08, 0.1, 0.1, 1.],
                8,
            );
        }
    } else {
        b.cuboid(
            Vec3::new(3., 1.2, 0.),
            Vec3::new(0.10, 2.4, 0.10),
            [0.55, 0.57, 0.53, 1.],
        );
        b.cuboid(Vec3::new(3., 2.25, 0.), Vec3::new(1.2, 0.85, 0.14), color);
        b.cuboid(
            Vec3::new(3., 2.25, -0.08),
            Vec3::new(0.72, 0.14, 0.035),
            [0.95, 0.84, 0.6, 1.],
        );
        if kind == TransportFacilityKind::Hospital as u8 {
            b.cuboid(
                Vec3::new(3., 2.25, -0.09),
                Vec3::new(0.14, 0.56, 0.03),
                [0.95, 0.84, 0.6, 1.],
            );
        }
    }
    b.finish().unwrap()
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
    push(road.rules.direction as u64);
    push(road.rules.structure as u64);
    push(road.rules.layer as i64 as u64);
    push(u64::from(road.rules.motor_access));
    push(
        road.rules
            .speed_limit_m_s
            .map_or(0, |v| u64::from(v.to_bits())),
    );
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
            mesh.quad(l0, l1, r1, r0, [0.23, 0.26, 0.26, 1.]);
            for side in [-1., 1.] {
                if let (Some(e0), Some(e1), Some(e2), Some(e3)) = (
                    sample(start + normal * (half - 0.24) * side, 0.145),
                    sample(end + normal * (half - 0.24) * side, 0.145),
                    sample(end + normal * (half - 0.12) * side, 0.145),
                    sample(start + normal * (half - 0.12) * side, 0.145),
                ) {
                    if side > 0. {
                        mesh.quad(e3, e2, e1, e0, [0.80, 0.81, 0.73, 1.]);
                    } else {
                        mesh.quad(e0, e1, e2, e3, [0.80, 0.81, 0.73, 1.]);
                    }
                }
            }
            if i % 2 == 0
                && road.rules.direction == terra_bevy::rules::RoadDirection::Both
                && road.width_m >= 4.5
            {
                if let (Some(c0), Some(c1), Some(c2), Some(c3)) = (
                    sample(start + normal * 0.07, 0.13),
                    sample(start.lerp(end, 0.62) + normal * 0.07, 0.13),
                    sample(start.lerp(end, 0.62) - normal * 0.07, 0.13),
                    sample(start - normal * 0.07, 0.13),
                ) {
                    mesh.quad(c0, c1, c2, c3, [0.86, 0.81, 0.59, 1.]);
                }
            }
        }
    }
    mesh.finish()
}

/// Haulers have a visible bed so loaded cargo communicates an actual delivery.
fn hauler_mesh(variant: u8) -> Mesh {
    let mut b = MeshBuilder::default();
    let paint = match variant % 3 {
        0 => [0.28, 0.41, 0.37, 1.],
        1 => [0.48, 0.37, 0.21, 1.],
        _ => [0.34, 0.41, 0.48, 1.],
    };
    b.cuboid(
        Vec3::new(0., 0.60, 0.10),
        Vec3::new(1.65, 0.35, 3.8),
        [0.19, 0.22, 0.22, 1.],
    );
    b.cuboid(
        Vec3::new(0., 1.25, -1.05),
        Vec3::new(1.65, 1.1, 1.40),
        paint,
    );
    b.cuboid(
        Vec3::new(0., 1.48, -1.76),
        Vec3::new(1.35, 0.5, 0.025),
        [0.16, 0.27, 0.31, 1.],
    );
    for x in [-0.835, 0.835] {
        b.cuboid(
            Vec3::new(x, 1.49, -1.05),
            Vec3::new(0.025, 0.50, 0.90),
            [0.16, 0.27, 0.31, 1.],
        );
        for z in [-1.25, 1.25] {
            b.cylinder(
                Vec3::new(x - 0.13, 0.42, z),
                Vec3::new(x + 0.13, 0.42, z),
                0.39,
                [0.10, 0.12, 0.12, 1.],
                12,
            );
        }
    }
    b.cuboid(
        Vec3::new(0., 0.91, 0.85),
        Vec3::new(1.58, 0.15, 2.15),
        [0.41, 0.32, 0.21, 1.],
    );
    for x in [-0.79, 0.79] {
        b.cuboid(Vec3::new(x, 1.18, 0.85), Vec3::new(0.10, 0.45, 2.15), paint);
    }
    b.cuboid(
        Vec3::new(0., 1.18, 1.88),
        Vec3::new(1.65, 0.45, 0.10),
        paint,
    );
    for x in [-0.56, 0.56] {
        b.cuboid(
            Vec3::new(x, 1.0, -1.78),
            Vec3::new(0.30, 0.18, 0.04),
            [0.86, 0.79, 0.55, 1.],
        );
    }
    b.finish().unwrap()
}
fn cargo_mesh(kind: u8) -> Mesh {
    let mut b = MeshBuilder::default();
    if kind == 1 {
        for i in 0..4 {
            b.cuboid(
                Vec3::new(
                    (i % 2) as f32 * 0.53 - 0.265,
                    1.15 + (i / 2) as f32 * 0.21,
                    0.9,
                ),
                Vec3::new(0.47, 0.18, 1.75),
                [0.52, 0.38, 0.21, 1.],
            );
        }
    } else if kind == 2 {
        for x in [-0.35, 0.35] {
            b.cylinder(
                Vec3::new(x, 1.02, 0.9),
                Vec3::new(x, 1.85, 0.9),
                0.28,
                [0.24, 0.37, 0.43, 1.],
                10,
            );
        }
    } else {
        for x in [-0.33, 0.33] {
            b.cuboid(
                Vec3::new(x, 1.34, 0.8),
                Vec3::new(0.59, 0.65, 1.2),
                [0.62, 0.47, 0.27, 1.],
            );
            for y in [1.08, 1.60] {
                b.cuboid(
                    Vec3::new(x, y, 0.8),
                    Vec3::new(0.62, 0.06, 1.23),
                    [0.30, 0.26, 0.18, 1.],
                );
            }
        }
    }
    b.finish().unwrap()
}

fn sync_living_visuals(
    mut commands: Commands,
    time: Res<Time>,
    net: Option<Res<NetworkStatus>>,
    sandbox: Res<SandboxRuntime>,
    runtime: Res<WorldRuntime>,
    resident: Res<ResidentChunks>,
    politics: Res<PoliticalWorld>,
    control: Res<PlayerControl>,
    scene_registry: Res<terra_bevy::render::ActorSceneOverrides>,
    mut existing: Query<
        (
            Entity,
            &VisualKey,
            &mut MeshKey,
            Option<&mut Mesh3d>,
            &mut RenderPose,
            &mut Transform,
            &mut Visibility,
            Option<&mut ProceduralActorMesh>,
            Option<&ActorSceneActive>,
            Option<&ActorSceneAnchor>,
        ),
        With<LivingVisual>,
    >,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut assets: Local<SharedVisualAssets>,
    mut animation_seconds: Local<f64>,
) {
    if !runtime.clock.paused {
        *animation_seconds += time.delta_secs_f64().clamp(0., 0.25);
    }
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
            let driver = terra_bevy::render::known_driver_vehicle(&sandbox.state, unit.id);
            let seat = terra_bevy::render::driver_seat_presentation(
                &scene_registry,
                &sandbox.state,
                unit.id,
            );
            let seated_vehicle = seat.and_then(|(id, _)| sandbox.state.living.units.get(&id));
            let first_person_body =
                control.mode == ControlMode::FirstPerson && control.possessed == Some(unit.id);
            let preview = net
                .as_ref()
                .and_then(|net| net.predicted_actor.as_ref())
                .filter(|actor| actor.id == unit.id && control.possessed == Some(unit.id));
            let velocity = preview.map_or(unit.velocity_enu_m_s, |actor| actor.velocity_enu_m_s);
            let speed = velocity[0].hypot(velocity[1]);
            let model = if seated_vehicle.is_some() {
                let p = unit
                    .owner
                    .and_then(|id| sandbox.state.transport.profiles.get(&id))
                    .cloned()
                    .unwrap_or_default();
                MeshKey::SeatedProfile(p.body_style, p.skin, p.hair, p.outfit)
            } else if let Some(animal) = sandbox.state.ecology.animals.get(&unit.id) {
                let pose = if speed > 0.12 {
                    ((*animation_seconds * (5.0 + speed.min(6.) * 2.0)) as u64 + unit.id % 8) as u8
                        % 8
                } else {
                    match animal.behavior {
                        WildlifeBehavior::Grazing => WILDLIFE_GRAZING,
                        WildlifeBehavior::Drinking => WILDLIFE_DRINKING,
                        WildlifeBehavior::Resting => WILDLIFE_RESTING,
                        _ => WILDLIFE_STANDING,
                    }
                };
                MeshKey::Wildlife(animal.species, pose)
            } else if let Some(v) = sandbox.state.transport.appearance(unit.id) {
                MeshKey::Vehicle(v.model as u8, v.paint)
            } else if unit.role == UnitRole::Player
                && unit
                    .owner
                    .and_then(|id| sandbox.state.transport.profiles.get(&id))
                    .is_some()
            {
                let p = &sandbox.state.transport.profiles[&unit.owner.unwrap()];
                MeshKey::Profile(p.body_style, p.skin, p.hair, p.outfit)
            } else if unit.role == UnitRole::Hauler && unit.kind == ActorKind::Vehicle {
                MeshKey::Hauler((unit.id % 3) as u8)
            } else {
                MeshKey::Actor(unit.kind as u8, unit.id % 4)
            };
            desired.push(DesiredVisual {
                key: VisualKey::Unit(unit.id),
                mesh: model,
                position: seated_vehicle.map_or_else(
                    || preview.map_or(unit.position, |actor| actor.position),
                    |v| v.position,
                ),
                heading: seated_vehicle.map_or_else(
                    || preview.map_or(unit.heading_deg, |actor| actor.heading_deg),
                    |v| v.heading_deg,
                ),
                visible: visible_actors
                    && !first_person_body
                    && (driver.is_none() || seated_vehicle.is_some())
                    && !(control.mode == ControlMode::FirstPerson
                        && driver.is_some()
                        && control.possessed == driver),
                moving: driver.is_none() && speed > 0.1,
                actor: Some(ActorSceneAnchor {
                    body_id: unit.id,
                    kind: unit.kind,
                    species: sandbox
                        .state
                        .ecology
                        .animals
                        .get(&unit.id)
                        .map(|a| a.species),
                }),
            });
        }
        for trailer in sandbox.state.transport.trailers.values() {
            let heading = trailer
                .attached_to
                .and_then(|id| sandbox.state.living.units.get(&id))
                .map_or(0., |u| u.heading_deg);
            let mut position = trailer.position;
            if let Some(surface) = terra_bevy::environment::physical_surface(
                &resident,
                position,
                &runtime.effective_weather(),
            ) {
                position.altitude_m = surface.terrain_height_m;
            }
            desired.push(DesiredVisual {
                key: VisualKey::Trailer(trailer.id),
                mesh: MeshKey::Trailer,
                position,
                heading,
                visible: visible_actors,
                moving: trailer.attached_to.is_some(),
                actor: None,
            });
        }
        for facility in sandbox.state.transport.facilities.values() {
            desired.push(DesiredVisual {
                key: VisualKey::TransportFacility(facility.id),
                mesh: MeshKey::TransportMarker(facility.kind as u8),
                position: ground(facility.position),
                heading: 0.,
                visible: visible_buildings,
                moving: false,
                actor: None,
            });
        }
        for vehicle in sandbox
            .state
            .transport
            .vehicles
            .values()
            .filter(|v| !v.cargo.is_empty())
        {
            if let Some(unit) = sandbox.state.living.units.get(&vehicle.unit_id) {
                let mut position = vehicle
                    .trailer
                    .and_then(|id| sandbox.state.transport.trailers.get(&id))
                    .map_or(unit.position, |t| t.position);
                if let Some(h) = resident.terrain_height(position) {
                    position.altitude_m = h;
                }
                desired.push(DesiredVisual {
                    key: VisualKey::Cargo(unit.id),
                    mesh: MeshKey::Cargo(0),
                    position,
                    heading: unit.heading_deg,
                    visible: visible_actors
                        && !(control.mode == ControlMode::FirstPerson
                            && control.possessed == Some(unit.id)),
                    moving: false,
                    actor: None,
                });
            }
        }
        for delivery in sandbox
            .state
            .living
            .deliveries
            .iter()
            .filter(|d| matches!(d.phase, DeliveryPhase::ToDropoff | DeliveryPhase::Returning))
        {
            if let Some(unit) = sandbox.state.living.units.get(&delivery.unit_id) {
                desired.push(DesiredVisual {
                    key: VisualKey::Cargo(unit.id),
                    mesh: MeshKey::Cargo(match delivery.item.as_str() {
                        "wood" | "planks" => 1,
                        "water" => 2,
                        _ => 0,
                    }),
                    position: unit.position,
                    heading: unit.heading_deg,
                    visible: visible_actors
                        && !(control.mode == ControlMode::FirstPerson
                            && control.possessed == Some(unit.id)),
                    moving: false,
                    actor: None,
                });
            }
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
                actor: None,
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
                actor: None,
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
                actor: None,
            });
        }
        if let Some(anchor) = sandbox
            .state
            .living
            .origin
            .filter(|_| !sandbox.state.living.roads.is_empty())
        {
            let mut signature = 0xcbf29ce484222325_u64;
            for road in &sandbox.state.living.roads {
                signature =
                    (signature ^ road_signature(road, &resident)).wrapping_mul(0x100000001b3);
            }
            for chunk in resident.chunks.values() {
                signature = (signature ^ (std::sync::Arc::as_ptr(chunk) as usize as u64))
                    .wrapping_mul(0x100000001b3);
            }
            desired.push(DesiredVisual {
                key: VisualKey::RoadPaint,
                mesh: MeshKey::RoadPaint(signature),
                position: anchor,
                heading: 0.,
                visible: visible_roads,
                moving: false,
                actor: None,
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
                actor: Some(ActorSceneAnchor {
                    body_id: actor.id,
                    kind: actor.kind,
                    species: None,
                }),
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
            actor: Some(ActorSceneAnchor {
                body_id: player.id,
                kind: ActorKind::Human,
                species: None,
            }),
        });
    }
    // Allocate only newly needed shared assets. Stock, currency, needs and
    // routine snapshots cannot change these keys or trigger geometry work.
    let mut needed: BTreeSet<_> = desired.iter().map(|v| v.mesh.clone()).collect();
    // Retain each used species' complete 12-pose palette. Animation switches
    // existing handles; it must not allocate geometry on every gait frame.
    let species: BTreeSet<_> = desired
        .iter()
        .filter_map(|v| {
            if let MeshKey::Wildlife(species, _) = v.mesh {
                Some(species)
            } else {
                None
            }
        })
        .collect();
    for species in species {
        for pose in 0..WILDLIFE_POSE_COUNT {
            needed.insert(MeshKey::Wildlife(species, pose));
        }
    }
    for key in &needed {
        if assets.meshes.contains_key(key)
            || matches!(key, MeshKey::RoadPaint(hash) if assets.unavailable_paint == Some(*hash))
        {
            continue;
        }
        let model = match key {
            MeshKey::Actor(kind, variant) => {
                Some(actor_mesh_variant(ActorKind::ALL[*kind as usize], *variant))
            }
            MeshKey::Vehicle(model, paint) => Some(crate::vehicle_models::vehicle_body_mesh(
                terra_bevy::rules::VEHICLE_MODELS[*model as usize],
                *paint,
            )),
            MeshKey::Profile(body, skin, hair, outfit) => Some(
                crate::vehicle_models::profile_mesh(*body, *skin, *hair, *outfit),
            ),
            MeshKey::SeatedProfile(body, skin, hair, outfit) => Some(
                crate::vehicle_models::seated_profile_mesh(*body, *skin, *hair, *outfit),
            ),
            MeshKey::Trailer => Some(crate::vehicle_models::trailer_mesh()),
            MeshKey::TransportMarker(kind) => Some(transport_marker_mesh(*kind)),
            MeshKey::Wildlife(species, pose) => Some(wildlife_mesh(*species, *pose)),
            MeshKey::Hauler(variant) => Some(hauler_mesh(*variant)),
            MeshKey::Cargo(kind) => Some(cargo_mesh(*kind)),
            MeshKey::Market(seed) => Some(market_mesh(*seed)),
            MeshKey::Facility(kind, seed) => Some(production_mesh(kind, *seed)),
            MeshKey::ServiceMarker => Some(service_marker_mesh()),
            MeshKey::Road(hash) => roads
                .get(hash)
                .and_then(|road| authored_road_mesh(road, &resident)),
            MeshKey::RoadPaint(_) => sandbox.state.living.origin.and_then(|origin| {
                road_presentation::mesh(&sandbox.state.living.roads, origin, &resident)
            }),
        };
        if let Some(model) = model {
            if matches!(key, MeshKey::RoadPaint(_)) {
                assets.unavailable_paint = None;
            }
            assets.meshes.insert(key.clone(), meshes.add(model));
        } else if let MeshKey::RoadPaint(hash) = key {
            assets.unavailable_paint = Some(*hash);
        }
    }
    let wanted: BTreeMap<_, _> = desired.iter().map(|d| (d.key, d)).collect();
    let mut alive = BTreeSet::new();
    for (
        entity,
        key,
        mut mesh_key,
        mesh,
        mut pose,
        mut transform,
        mut visibility,
        fallback,
        scene_active,
        scene_anchor,
    ) in &mut existing
    {
        let Some(target) = wanted.get(key) else {
            commands.entity(entity).despawn();
            continue;
        };
        alive.insert(*key);
        if *mesh_key != target.mesh {
            if matches!(&*mesh_key, MeshKey::SeatedProfile(..))
                != matches!(&target.mesh, MeshKey::SeatedProfile(..))
            {
                // Boarding/exiting is a discrete authority handoff, not a
                // walking interpolation through the occupied vehicle body.
                pose.position = target.position;
                pose.heading = target.heading;
            }
            if let Some(handle) = assets.meshes.get(&target.mesh) {
                if let Some(mut mesh) = mesh {
                    mesh.0 = handle.clone();
                }
                if let Some(mut fallback) = fallback {
                    fallback.mesh = handle.clone();
                }
                *mesh_key = target.mesh.clone();
            }
        }
        if scene_anchor.copied() != target.actor {
            if let Some(anchor) = target.actor {
                commands.entity(entity).insert(anchor);
            }
        }
        let is_actor = matches!(
            target.mesh,
            MeshKey::Actor(..)
                | MeshKey::Profile(..)
                | MeshKey::SeatedProfile(..)
                | MeshKey::Vehicle(..)
                | MeshKey::Trailer
                | MeshKey::Wildlife(..)
                | MeshKey::Hauler(..)
                | MeshKey::Cargo(..)
        );
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
        if let MeshKey::Vehicle(model, _) = target.mesh {
            crate::vehicle_presentation::align_to_surface(
                &mut transform,
                pose.position,
                pose.heading,
                terra_bevy::rules::VEHICLE_MODELS[model as usize],
                &resident,
                &runtime,
            );
        }
        if target.moving
            && !runtime.clock.paused
            && scene_active.is_none()
            && matches!(
                target.mesh,
                MeshKey::Actor(0 | 1 | 2, _) | MeshKey::Profile(..)
            )
        {
            let id = match key {
                VisualKey::Unit(id) | VisualKey::Legacy(id) => *id,
                _ => 0,
            };
            let phase = *animation_seconds as f32 * 9. + (id % 97) as f32;
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
        if let MeshKey::Vehicle(model, _) = target.mesh {
            crate::vehicle_presentation::align_to_surface(
                &mut transform,
                target.position,
                target.heading,
                terra_bevy::rules::VEHICLE_MODELS[model as usize],
                &resident,
                &runtime,
            );
        }
        let mut entity = commands.spawn((
            LivingVisual,
            target.key,
            target.mesh,
            RenderPose {
                position: target.position,
                heading: target.heading,
            },
            Mesh3d(mesh.clone()),
            MeshMaterial3d(material.clone()),
            transform,
            if target.visible {
                Visibility::Visible
            } else {
                Visibility::Hidden
            },
        ));
        if let Some(anchor) = target.actor {
            entity.insert((
                anchor,
                ProceduralActorMesh {
                    mesh,
                    material: material.clone(),
                },
            ));
        }
    }
    // Changing worlds or restoring another settlement cannot retain an
    // unbounded collection of stale route/facility mesh assets.
    if assets.meshes.len() > 160 {
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
            rules: Default::default(),
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

    #[test]
    fn known_driver_fallback_tracks_vehicle_hides_in_first_person_and_restores_on_exit() {
        use bevy::asset::AssetPlugin;
        use terra_bevy::render::{ActorSceneOverrides, AssetPackPlugin};
        use terra_bevy::rules::{Command, TransportCommand, TransportFacilityKind, VehicleModel};
        let origin = GeoPosition::new(48.137, 11.575, 100.).unwrap();
        let mut sandbox = SandboxRuntime::new(origin);
        sandbox
            .state
            .seed_living_demo(origin, &[], &sandbox.catalog)
            .unwrap();
        sandbox
            .state
            .ensure_transport_demo(&sandbox.catalog)
            .unwrap();
        let garage = sandbox
            .state
            .transport
            .facilities
            .values()
            .find(|f| f.kind == TransportFacilityKind::Garage)
            .unwrap()
            .clone();
        // Test setup selects an ordinary paid rental. This is a presentation
        // regression, not a claim about a physical walk or a player career.
        sandbox
            .state
            .set_player_position(1, garage.position)
            .unwrap();
        let vehicle = sandbox
            .state
            .apply(
                1,
                Command::Transport {
                    action: TransportCommand::RentVehicle {
                        model: VehicleModel::Compact,
                        facility_id: garage.id,
                    },
                },
                &sandbox.catalog,
            )
            .unwrap()
            .created_id
            .unwrap();
        sandbox
            .state
            .set_player_position(1, sandbox.state.living.units[&vehicle].position)
            .unwrap();
        sandbox
            .state
            .apply(
                1,
                Command::Transport {
                    action: TransportCommand::EnterVehicle {
                        vehicle_id: vehicle,
                    },
                },
                &sandbox.catalog,
            )
            .unwrap();
        sandbox
            .state
            .set_character_profile(1, terra_bevy::rules::CharacterProfile::default())
            .unwrap();
        let avatar = sandbox.state.living.accounts[&1].avatar_unit;
        let before = sandbox.state.clone();
        let pack = terra_bevy::asset_pack::ValidatedAssetPack::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../terra-bevy/assets/character-demo.pack.json"),
        )
        .unwrap();
        let mut player_control = PlayerControl::default();
        player_control.mode = ControlMode::ThirdPerson;
        player_control.possessed = Some(vehicle);
        let mut app = App::new();
        app.add_plugins(AssetPackPlugin::new(pack))
            .add_plugins((MinimalPlugins, AssetPlugin::default()))
            .insert_resource(sandbox)
            .insert_resource(WorldRuntime {
                config: terra_bevy::WorldConfig {
                    focus: origin,
                    ..default()
                },
                focus: origin,
                frame: LocalFrame::new(origin),
                origin_revision: 0,
                clock: default(),
                weather: default(),
                stream_stats: default(),
                last_error: None,
                stream_poll_ms: 0.,
                simulation_ms: 0.,
                rebases: 0,
                network_authority: false,
            })
            .insert_resource(PoliticalWorld::load(None).unwrap())
            .init_resource::<ResidentChunks>()
            .insert_resource(player_control)
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<StandardMaterial>>()
            .add_systems(Update, (sync_living_visuals, place_known_drivers).chain());
        app.world_mut().resource_mut::<WorldRuntime>().clock.paused = true;
        app.update();
        let root = |app: &mut App, body| {
            let mut q = app.world_mut().query::<(Entity, &ActorSceneAnchor)>();
            q.iter(app.world())
                .find(|(_, a)| a.body_id == body)
                .unwrap()
                .0
        };
        let rider = root(&mut app, avatar);
        let car = root(&mut app, vehicle);
        assert!(matches!(
            app.world().get::<MeshKey>(rider),
            Some(MeshKey::SeatedProfile(..))
        ));
        assert!(
            app.world().get::<Mesh3d>(rider).is_some(),
            "loading/unavailable scenes retain a real seated procedural mesh"
        );
        let expected =
            *app.world().get::<Transform>(car).unwrap() * Transform::from_xyz(-0.42, 0.46, -0.24);
        assert!(
            app.world()
                .get::<Transform>(rider)
                .unwrap()
                .translation
                .abs_diff_eq(expected.translation, 1e-5)
        );
        assert_eq!(app.world().resource::<SandboxRuntime>().state, before);
        assert_eq!(
            *app.world().get::<Visibility>(rider).unwrap(),
            Visibility::Hidden,
            "pending/deferred/failed vehicle keeps its procedural car but cannot safely show a rider"
        );
        assert!(app.world().get::<Mesh3d>(car).is_some());
        // Actual scene-ready/fallback marking is covered by terra-bevy's scene
        // spawner tests. Here exercise the adapter's readiness state boundary.
        app.world_mut().entity_mut(car).insert(ActorSceneActive);
        app.update();
        assert_eq!(
            *app.world().get::<Visibility>(rider).unwrap(),
            Visibility::Visible
        );
        assert!(
            app.world().get::<Mesh3d>(rider).is_some(),
            "human procedural fallback is visible inside the ready cabin"
        );
        app.world_mut().entity_mut(car).remove::<ActorSceneActive>();
        app.update();
        assert_eq!(
            *app.world().get::<Visibility>(rider).unwrap(),
            Visibility::Hidden
        );
        app.world_mut().entity_mut(car).insert(ActorSceneActive);
        app.update();
        app.world_mut()
            .resource_mut::<WorldRuntime>()
            .config
            .layers
            .set(Layer::Actors, false);
        app.update();
        assert_eq!(
            *app.world().get::<Visibility>(rider).unwrap(),
            Visibility::Hidden
        );
        app.world_mut()
            .resource_mut::<WorldRuntime>()
            .config
            .layers
            .set(Layer::Actors, true);
        app.update();
        app.world_mut().resource_mut::<PlayerControl>().mode = ControlMode::FirstPerson;
        app.update();
        assert_eq!(
            *app.world().get::<Visibility>(car).unwrap(),
            Visibility::Hidden
        );
        assert_eq!(
            *app.world().get::<Visibility>(rider).unwrap(),
            Visibility::Hidden
        );
        app.world_mut().resource_mut::<PlayerControl>().mode = ControlMode::ThirdPerson;
        app.update();
        assert_eq!(
            *app.world().get::<Visibility>(rider).unwrap(),
            Visibility::Visible
        );
        // Release/RTS is an existing control handoff, not persistent physical
        // occupancy. Clear the seat and restore the unchanged parked avatar.
        {
            let mut s = app.world_mut().resource_mut::<SandboxRuntime>();
            let catalog = s.catalog.clone();
            s.state
                .apply(
                    1,
                    terra_bevy::rules::Command::Living {
                        action: terra_bevy::rules::LivingCommand::Release,
                    },
                    &catalog,
                )
                .unwrap();
        }
        app.world_mut().resource_mut::<PlayerControl>().mode = ControlMode::Rts;
        app.world_mut().resource_mut::<PlayerControl>().possessed = None;
        app.update();
        assert!(matches!(
            app.world().get::<MeshKey>(rider),
            Some(MeshKey::Profile(..))
        ));
        assert_eq!(
            *app.world().get::<Visibility>(rider).unwrap(),
            Visibility::Visible
        );
        let released = app.world().resource::<SandboxRuntime>().state.clone();
        app.update();
        assert_eq!(app.world().resource::<SandboxRuntime>().state, released);
        // Restoring a paused manually-driven snapshot reinstalls the seat.
        app.world_mut().resource_mut::<SandboxRuntime>().state =
            serde_json::from_slice(&serde_json::to_vec(&before).unwrap()).unwrap();
        app.world_mut().resource_mut::<PlayerControl>().mode = ControlMode::ThirdPerson;
        app.world_mut().resource_mut::<PlayerControl>().possessed = Some(vehicle);
        app.update();
        assert!(matches!(
            app.world().get::<MeshKey>(rider),
            Some(MeshKey::SeatedProfile(..))
        ));
        assert_eq!(app.world().resource::<SandboxRuntime>().state, before);
        // No seat metadata: suppress the known parked avatar instead of leaving
        // a second person at the previous boarding location.
        let appearance = app
            .world()
            .resource::<ActorSceneOverrides>()
            .resolve_model(ActorKind::Vehicle, None, Some(VehicleModel::Compact))
            .unwrap()
            .clone();
        app.world_mut()
            .resource_mut::<ActorSceneOverrides>()
            .remove_vehicle_model(VehicleModel::Compact);
        app.update();
        assert_eq!(
            *app.world().get::<Visibility>(rider).unwrap(),
            Visibility::Hidden
        );
        app.world_mut()
            .resource_mut::<ActorSceneOverrides>()
            .set_vehicle_model(VehicleModel::Compact, appearance)
            .unwrap();
        app.update();
        assert_eq!(
            *app.world().get::<Visibility>(rider).unwrap(),
            Visibility::Visible
        );
        {
            let mut s = app.world_mut().resource_mut::<SandboxRuntime>();
            let catalog = s.catalog.clone();
            s.state
                .exit_vehicle_with_ground(1, &catalog, |mut p| {
                    p.altitude_m = 100.;
                    Some(p)
                })
                .unwrap();
        }
        app.world_mut().resource_mut::<PlayerControl>().possessed = Some(avatar);
        app.update();
        assert!(matches!(
            app.world().get::<MeshKey>(rider),
            Some(MeshKey::Profile(..))
        ));
        assert_eq!(
            *app.world().get::<Visibility>(rider).unwrap(),
            Visibility::Visible
        );
        let s = app.world().resource::<SandboxRuntime>();
        assert!(
            app.world()
                .get::<Transform>(rider)
                .unwrap()
                .translation
                .abs_diff_eq(
                    geo_transform(
                        s.state.living.units[&avatar].position,
                        &app.world().resource::<WorldRuntime>().frame
                    )
                    .translation,
                    1e-4
                )
        );
        assert_eq!(
            s.state.living.accounts[&1].money,
            before.living.accounts[&1].money
        );
        assert_eq!(s.state.transport.profiles, before.transport.profiles);
    }
}
