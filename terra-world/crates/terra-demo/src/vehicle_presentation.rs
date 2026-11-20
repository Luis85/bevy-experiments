//! Bounded vehicle presentation: shared cockpit assets, model-sized terrain
//! alignment and nearby headlights. Rendering never changes authoritative pose.
use crate::{
    camera::ExplorerCamera,
    control::{ControlMode, PlayerControl},
    vehicle_models,
};
use bevy::prelude::*;
use std::collections::{BTreeMap, BTreeSet};
use terra_bevy::rules::{TurnIndicator, VehicleModel};
use terra_bevy::{
    PoliticalWorld, ResidentChunks, SandboxRuntime, WorldRuntime, WorldSet,
    environment::{physical_surface, vehicle_ground_sample, vehicle_ground_support},
    render::geo_transform,
};
use terra_core::{GeoPosition, Layer, LocalFrame};

pub struct VehiclePresentationPlugin;
impl Plugin for VehiclePresentationPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (cockpit, lights, wheels)
                .after(WorldSet::Render)
                .after(crate::remote::ActorVisuals),
        );
    }
}

#[derive(Component)]
struct WheelsAttached {
    body: u64,
    model: VehicleModel,
}
#[derive(Component)]
struct RollingWheel {
    body: u64,
    radius: f32,
    front: bool,
    angle: f32,
    steering: f32,
    previous_heading: f32,
    wheelbase: f32,
    suspension_offset: f64,
    suspension_velocity: f64,
}
#[derive(Default)]
struct WheelAssets {
    meshes: BTreeMap<u8, Handle<Mesh>>,
    material: Option<Handle<StandardMaterial>>,
}
/// Wheels inherit the already interpolated/terrain-aligned visual body. Their
/// animation never writes gameplay position, fuel or steering state.
fn wheels(
    mut commands: Commands,
    time: Res<Time>,
    sandbox: Res<SandboxRuntime>,
    runtime: Res<WorldRuntime>,
    resident: Res<ResidentChunks>,
    roots: Query<
        (
            Entity,
            &terra_bevy::render::ActorSceneAnchor,
            Option<&WheelsAttached>,
            Option<&Children>,
            &Transform,
        ),
        Without<RollingWheel>,
    >,
    scene_roots: Query<Has<terra_bevy::render::ActorSceneActive>>,
    mut existing: Query<
        (
            Entity,
            &mut RollingWheel,
            &ChildOf,
            &mut Transform,
            &mut Visibility,
        ),
        With<RollingWheel>,
    >,
    mut cache: Local<WheelAssets>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // The visual root can survive a save restore or appearance replacement.
    // Rebuild only our wheels when its body/model changes; supplied scenes and
    // game-owned attachments on the same root must remain intact.
    let mut replaced = BTreeSet::new();
    for (entity, anchor, attached, children, _) in &roots {
        let vehicle = sandbox.state.transport.appearance(anchor.body_id);
        if attached.is_none() && vehicle.is_none() {
            continue;
        }
        if attached.is_some_and(|old| {
            old.body == anchor.body_id && vehicle.as_ref().is_some_and(|v| old.model == v.model)
        }) {
            continue;
        }
        if let Some(children) = children {
            for child in children {
                if existing.contains(*child) {
                    commands.entity(*child).despawn();
                }
            }
        }
        replaced.insert(entity);
        let Some(vehicle) = vehicle else {
            if attached.is_some() {
                commands.entity(entity).remove::<WheelsAttached>();
            }
            continue;
        };
        let (radius, axles) = vehicle_models::wheel_layout(vehicle.model);
        let width = vehicle_models::vehicle_dimensions(vehicle.model).x;
        let mesh = cache
            .meshes
            .entry(vehicle.model as u8)
            .or_insert_with(|| meshes.add(vehicle_models::rolling_wheel_mesh(radius, width * 0.10)))
            .clone();
        let material = cache
            .material
            .get_or_insert_with(|| {
                materials.add(StandardMaterial {
                    base_color: Color::WHITE,
                    perceptual_roughness: 0.76,
                    ..default()
                })
            })
            .clone();
        let heading = sandbox
            .state
            .living
            .units
            .get(&anchor.body_id)
            .map_or(0., |u| u.heading_deg);
        commands
            .entity(entity)
            .insert(WheelsAttached {
                body: anchor.body_id,
                model: vehicle.model,
            })
            .with_children(|p| {
                for (index, z) in axles.iter().enumerate() {
                    for x in [-width * 0.43, width * 0.43] {
                        p.spawn((
                            RollingWheel {
                                body: anchor.body_id,
                                radius,
                                front: index == 0,
                                angle: 0.,
                                steering: 0.,
                                previous_heading: heading,
                                wheelbase: vehicle.model.spec().wheelbase_m as f32,
                                suspension_offset: 0.,
                                suspension_velocity: 0.,
                            },
                            Mesh3d(mesh.clone()),
                            MeshMaterial3d(material.clone()),
                            Transform::from_xyz(x, radius, *z),
                            Visibility::Inherited,
                        ));
                    }
                }
            });
    }
    let dt = if runtime.clock.paused {
        0.
    } else {
        time.delta_secs().min(0.25)
    };
    let weather = runtime.effective_weather();
    for (_, mut wheel, parent, mut transform, mut visibility) in &mut existing {
        if replaced.contains(&parent.parent()) {
            continue;
        }
        *visibility = if scene_roots.get(parent.parent()).unwrap_or(false) {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
        let Some(unit) = sandbox.state.living.units.get(&wheel.body) else {
            continue;
        };
        let heading = f64::from(unit.heading_deg).to_radians();
        let speed = (unit.velocity_enu_m_s[0] * heading.sin()
            + unit.velocity_enu_m_s[1] * heading.cos()) as f32;
        wheel.angle = (wheel.angle - speed * dt / wheel.radius).rem_euclid(std::f32::consts::TAU);
        let measured = if dt > 0.001 && speed.abs() > 0.2 {
            let yaw = ((unit.heading_deg - wheel.previous_heading + 540.).rem_euclid(360.) - 180.)
                .to_radians();
            (yaw / dt * wheel.wheelbase / speed)
                .atan()
                .clamp(-0.60, 0.60)
        } else {
            0.
        };
        // Autonomous movement intentionally has no private steering actuator;
        // both owned and public replicas derive it from accepted body motion.
        let manual = sandbox
            .state
            .transport
            .vehicles
            .get(&wheel.body)
            .filter(|v| {
                sandbox
                    .state
                    .living
                    .accounts
                    .get(&v.owner)
                    .is_some_and(|a| a.possessed && a.active_unit == wheel.body)
            });
        let target = manual.map_or(measured, |v| v.dynamics.steering_rad as f32);
        wheel.steering += (target - wheel.steering) * (1. - (-12. * dt).exp());
        wheel.previous_heading = unit.heading_deg;
        transform.rotation = Quat::from_rotation_y(if wheel.front { -wheel.steering } else { 0. })
            * Quat::from_rotation_x(wheel.angle);
        // Sample at each actual visual axle, including the additional axles on
        // heavy models. The body is already interpolated and ground-aligned;
        // residual terrain height drives only bounded visual spring travel.
        if let Ok((_, _, _, _, root)) = roots.get(parent.parent()) {
            let rest = root.transform_point(Vec3::new(
                transform.translation.x,
                wheel.radius,
                transform.translation.z,
            ));
            if let Ok(position) = runtime.frame.to_geo(rest.as_dvec3().to_array()) {
                if let Some(ground) = vehicle_ground_sample(&resident, position, &weather) {
                    let up = f64::from((root.rotation * Vec3::Y).y).max(0.35);
                    let target =
                        (ground.height_m + f64::from(wheel.radius) - position.altitude_m) / up;
                    let (offset, velocity) = terra_core::suspension_spring(
                        wheel.suspension_offset,
                        wheel.suspension_velocity,
                        target,
                        f64::from(dt),
                    );
                    wheel.suspension_offset = offset;
                    wheel.suspension_velocity = velocity;
                    transform.translation.y = wheel.radius + offset as f32;
                }
            }
        }
    }
}

pub fn align_to_surface(
    transform: &mut Transform,
    position: GeoPosition,
    heading: f32,
    model: VehicleModel,
    resident: &ResidentChunks,
    runtime: &WorldRuntime,
) {
    let [width, _, _] = model.dimensions_m();
    let weather = runtime.effective_weather();
    if let Some(support) = vehicle_ground_support(
        resident,
        position,
        f64::from(heading),
        model.spec().wheelbase_m,
        width,
        &weather,
    ) {
        transform.rotate_local_x(support.pitch_radians.clamp(-0.55, 0.55) as f32);
        transform.rotate_local_z(support.roll_radians.clamp(-0.4, 0.4) as f32);
        if let Some(center) = physical_surface(resident, position, &weather) {
            transform.translation.y +=
                (support.height_m - center.terrain_height_m).clamp(-0.4, 0.8) as f32;
        }
    }
}

#[derive(Component)]
struct Cockpit;
#[derive(Default)]
struct CockpitAssets {
    meshes: BTreeMap<u8, Handle<Mesh>>,
    material: Option<Handle<StandardMaterial>>,
}
fn cockpit(
    mut commands: Commands,
    s: Res<SandboxRuntime>,
    control: Res<PlayerControl>,
    camera: Single<Entity, With<ExplorerCamera>>,
    mut existing: Query<(Entity, &mut Mesh3d, &mut Visibility), With<Cockpit>>,
    mut cache: Local<CockpitAssets>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let vehicle = control
        .possessed
        .and_then(|id| s.state.transport.appearance(id))
        .filter(|_| control.mode == ControlMode::FirstPerson);
    let Some(v) = vehicle else {
        for (_, _, mut visibility) in &mut existing {
            *visibility = Visibility::Hidden;
        }
        return;
    };
    let mesh = cache
        .meshes
        .entry(v.model as u8)
        .or_insert_with(|| meshes.add(vehicle_models::cockpit_mesh(v.model)))
        .clone();
    let material = cache
        .material
        .get_or_insert_with(|| {
            materials.add(StandardMaterial {
                base_color: Color::WHITE,
                perceptual_roughness: 0.72,
                unlit: true,
                ..default()
            })
        })
        .clone();
    if let Some((_, mut handle, mut visibility)) = existing.iter_mut().next() {
        handle.0 = mesh;
        *visibility = Visibility::Visible;
    } else {
        commands.entity(*camera).with_children(|p| {
            p.spawn((
                Cockpit,
                Mesh3d(mesh),
                MeshMaterial3d(material),
                Transform::IDENTITY,
                Visibility::Visible,
            ));
        });
    }
}

#[derive(Component)]
struct LightRig {
    unit: u64,
    model: VehicleModel,
    position: GeoPosition,
    heading: f32,
}
#[derive(Component)]
struct Lamp {
    unit: u64,
    side: i8,
    front: bool,
}
#[derive(Component)]
struct Beam {
    unit: u64,
}
#[derive(Default)]
struct LightAssets {
    lens: Option<Handle<Mesh>>,
    white: Option<Handle<StandardMaterial>>,
    red: Option<Handle<StandardMaterial>>,
    amber: Option<Handle<StandardMaterial>>,
}
fn lights(
    mut commands: Commands,
    time: Res<Time>,
    s: Res<SandboxRuntime>,
    runtime: Res<WorldRuntime>,
    resident: Res<ResidentChunks>,
    politics: Res<PoliticalWorld>,
    mut roots: Query<
        (Entity, &mut LightRig, &mut Transform, &mut Visibility),
        (Without<Lamp>, Without<Beam>),
    >,
    mut lamps: Query<
        (
            &Lamp,
            &mut MeshMaterial3d<StandardMaterial>,
            &mut Visibility,
        ),
        (Without<LightRig>, Without<Beam>),
    >,
    mut beams: Query<(&Beam, &mut SpotLight)>,
    mut cache: Local<LightAssets>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let lens = cache
        .lens
        .get_or_insert_with(|| meshes.add(Cuboid::new(0.25, 0.13, 0.035)))
        .clone();
    let white = cache
        .white
        .get_or_insert_with(|| {
            materials.add(StandardMaterial {
                base_color: Color::srgb(1., 0.94, 0.72),
                emissive: LinearRgba::rgb(8., 7., 4.),
                unlit: true,
                ..default()
            })
        })
        .clone();
    let red = cache
        .red
        .get_or_insert_with(|| {
            materials.add(StandardMaterial {
                base_color: Color::srgb(0.95, 0.08, 0.03),
                emissive: LinearRgba::rgb(4., 0.05, 0.01),
                unlit: true,
                ..default()
            })
        })
        .clone();
    let amber = cache
        .amber
        .get_or_insert_with(|| {
            materials.add(StandardMaterial {
                base_color: Color::srgb(1., 0.5, 0.06),
                emissive: LinearRgba::rgb(8., 3., 0.04),
                unlit: true,
                ..default()
            })
        })
        .clone();
    let visible = runtime.config.layers.contains(Layer::Actors) && !politics.atlas;
    let mut nearby: Vec<_> = s
        .state
        .living
        .units
        .values()
        .filter_map(|u| s.state.transport.appearance(u.id).map(|v| (u, v)))
        .filter(|(u, _)| u.position.surface_distance_m(runtime.focus) < 220.)
        .collect();
    nearby.sort_by(|(a, _), (b, _)| {
        a.position
            .surface_distance_m(runtime.focus)
            .total_cmp(&b.position.surface_distance_m(runtime.focus))
    });
    nearby.truncate(16);
    let needed: BTreeSet<_> = nearby.iter().map(|(u, _)| u.id).collect();
    let lit: BTreeSet<_> = nearby
        .iter()
        .filter(|(_, v)| v.lights)
        .take(4)
        .map(|(u, _)| u.id)
        .collect();
    let mut found = BTreeSet::new();
    for (entity, mut rig, mut transform, mut visibility) in &mut roots {
        if !needed.contains(&rig.unit) {
            commands.entity(entity).despawn();
            continue;
        }
        let Some((unit, v)) = nearby.iter().find(|(u, _)| u.id == rig.unit) else {
            continue;
        };
        if rig.model != v.model {
            // Every lamp and beam below belongs to this presentation rig.
            // A reused body ID must receive the new model's dimensions.
            commands.entity(entity).despawn();
            continue;
        }
        found.insert(rig.unit);
        let blend = 1. - (-18. * time.delta_secs_f64().min(0.25)).exp();
        let frame = LocalFrame::new(rig.position);
        let delta = frame.to_local(unit.position);
        rig.position = if rig.position.surface_distance_m(unit.position) > 80. {
            unit.position
        } else {
            frame
                .to_geo(delta.map(|x| x * blend))
                .unwrap_or(unit.position)
        };
        rig.heading +=
            ((unit.heading_deg - rig.heading + 540.).rem_euclid(360.) - 180.) * blend as f32;
        *transform = geo_transform(rig.position, &runtime.frame);
        transform.rotate_local_y(-rig.heading.to_radians());
        align_to_surface(
            &mut transform,
            rig.position,
            rig.heading,
            v.model,
            &resident,
            &runtime,
        );
        *visibility = if visible {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
    for (unit, v) in &nearby {
        if found.contains(&unit.id) {
            continue;
        }
        let dimensions = vehicle_models::vehicle_dimensions(v.model);
        let mut transform = geo_transform(unit.position, &runtime.frame);
        transform.rotate_local_y(-unit.heading_deg.to_radians());
        commands
            .spawn((
                LightRig {
                    unit: unit.id,
                    model: v.model,
                    position: unit.position,
                    heading: unit.heading_deg,
                },
                transform,
                if visible {
                    Visibility::Visible
                } else {
                    Visibility::Hidden
                },
            ))
            .with_children(|p| {
                for side in [-1, 1] {
                    for front in [true, false] {
                        p.spawn((
                            Lamp {
                                unit: unit.id,
                                side,
                                front,
                            },
                            Mesh3d(lens.clone()),
                            MeshMaterial3d(if front { white.clone() } else { red.clone() }),
                            Transform::from_xyz(
                                side as f32 * dimensions.x * 0.34,
                                dimensions.y * 0.39,
                                if front {
                                    -dimensions.z * 0.502
                                } else {
                                    dimensions.z * 0.502
                                },
                            ),
                            Visibility::Hidden,
                        ));
                    }
                }
                // Four nearby cars at most illuminate surfaces; distant lamps still
                // communicate signals without adding clustered spot-light work.
                p.spawn((
                    Beam { unit: unit.id },
                    SpotLight {
                        color: Color::srgb(1., 0.94, 0.79),
                        intensity: 0.,
                        range: 65.,
                        radius: 0.1,
                        inner_angle: 0.18,
                        outer_angle: 0.42,
                        shadow_maps_enabled: false,
                        ..default()
                    },
                    Transform::from_xyz(0., dimensions.y * 0.4, -dimensions.z * 0.51)
                        .with_rotation(Quat::from_rotation_x(-0.055)),
                ));
            });
    }
    let flash = (s.state.transport.elapsed_seconds * 2.).floor() as u64 % 2 == 0;
    for (lamp, mut material, mut visibility) in &mut lamps {
        let Some(v) = s.state.transport.appearance(lamp.unit) else {
            *visibility = Visibility::Hidden;
            continue;
        };
        let blinking = flash
            && match v.indicator {
                TurnIndicator::Hazard => true,
                TurnIndicator::Left => lamp.side < 0,
                TurnIndicator::Right => lamp.side > 0,
                TurnIndicator::Off => false,
            };
        material.0 = if blinking {
            amber.clone()
        } else if lamp.front {
            white.clone()
        } else {
            red.clone()
        };
        *visibility = if visible && (blinking || v.lights) {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
    for (beam, mut light) in &mut beams {
        light.intensity = if visible && lit.contains(&beam.unit) {
            22_000.
        } else {
            0.
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use terra_bevy::render::ActorSceneAnchor;
    use terra_core::ActorKind;

    fn fixture() -> (App, u64, Entity) {
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
        let body = *sandbox.state.transport.vehicles.keys().next().unwrap();
        let mut app = App::new();
        app.insert_resource(sandbox)
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
            .init_resource::<Time>()
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<StandardMaterial>>()
            .add_systems(Update, (wheels, lights));
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(std::time::Duration::from_millis(100));
        let root = app
            .world_mut()
            .spawn((
                ActorSceneAnchor {
                    body_id: body,
                    kind: ActorKind::Vehicle,
                    species: None,
                },
                Transform::default(),
                Visibility::Inherited,
            ))
            .id();
        (app, body, root)
    }

    fn wheel_entities(app: &mut App) -> Vec<Entity> {
        let mut query = app
            .world_mut()
            .query_filtered::<Entity, With<RollingWheel>>();
        query.iter(app.world()).collect()
    }

    fn light_entity(app: &mut App, body: u64) -> Option<Entity> {
        let mut query = app.world_mut().query::<(Entity, &LightRig)>();
        query
            .iter(app.world())
            .find(|(_, rig)| rig.unit == body)
            .map(|(entity, _)| entity)
    }

    #[test]
    fn restored_model_rebuilds_wheels_and_lamps_without_losing_other_children() {
        let (mut app, body, root) = fixture();
        let accessory = app.world_mut().spawn(ChildOf(root)).id();
        app.update();
        let old_wheels = wheel_entities(&mut app);
        assert_eq!(old_wheels.len(), 4);
        let old_lights = light_entity(&mut app, body).unwrap();

        // A restore can rewind next_id and place another model on the same
        // visual root; use the six-wheel model to expose retained old axles.
        app.world_mut()
            .resource_mut::<SandboxRuntime>()
            .state
            .transport
            .vehicles
            .get_mut(&body)
            .unwrap()
            .model = VehicleModel::Semi;
        let authoritative = app.world().resource::<SandboxRuntime>().state.clone();
        app.update();
        assert_eq!(
            app.world().resource::<SandboxRuntime>().state,
            authoritative
        );
        assert_eq!(
            app.world().get::<WheelsAttached>(root).unwrap().model,
            VehicleModel::Semi
        );
        assert!(app.world().get_entity(accessory).is_ok());
        assert_eq!(
            app.world().get::<ChildOf>(accessory).unwrap().parent(),
            root
        );
        for old in old_wheels {
            assert!(app.world().get_entity(old).is_err());
        }
        assert!(app.world().get_entity(old_lights).is_err());
        let rebuilt = wheel_entities(&mut app);
        assert_eq!(rebuilt.len(), 6);
        let (radius, axles) = vehicle_models::wheel_layout(VehicleModel::Semi);
        for entity in &rebuilt {
            let wheel = app.world().get::<RollingWheel>(*entity).unwrap();
            let transform = app.world().get::<Transform>(*entity).unwrap();
            assert_eq!(wheel.radius, radius);
            assert_eq!(transform.translation.y, radius);
            assert!(axles.contains(&transform.translation.z));
        }
        let rig = light_entity(&mut app, body).unwrap();
        assert_eq!(
            app.world().get::<LightRig>(rig).unwrap().model,
            VehicleModel::Semi
        );
        let dimensions = vehicle_models::vehicle_dimensions(VehicleModel::Semi);
        for child in app.world().get::<Children>(rig).unwrap() {
            let transform = app.world().get::<Transform>(*child).unwrap();
            if let Some(lamp) = app.world().get::<Lamp>(*child) {
                assert_eq!(
                    transform.translation.x,
                    lamp.side as f32 * dimensions.x * 0.34
                );
                assert_eq!(transform.translation.y, dimensions.y * 0.39);
                assert_eq!(transform.translation.z.abs(), dimensions.z * 0.502);
            } else if app.world().get::<Beam>(*child).is_some() {
                assert_eq!(transform.translation.z, -dimensions.z * 0.51);
            }
        }
        app.update();
        assert_eq!(
            wheel_entities(&mut app),
            rebuilt,
            "unchanged models must not rebuild each frame"
        );
        assert_eq!(light_entity(&mut app, body), Some(rig));

        app.world_mut()
            .resource_mut::<SandboxRuntime>()
            .state
            .transport
            .vehicles
            .remove(&body);
        app.update();
        assert!(wheel_entities(&mut app).is_empty());
        assert!(app.world().get::<WheelsAttached>(root).is_none());
        assert!(light_entity(&mut app, body).is_none());
        assert!(app.world().get_entity(accessory).is_ok());
    }

    #[test]
    fn autonomous_and_remote_wheels_steer_from_motion_manual_wheels_use_actuator() {
        let (mut app, body, _) = fixture();
        app.update();
        let set_motion = |app: &mut App, heading: f32| {
            let mut sandbox = app.world_mut().resource_mut::<SandboxRuntime>();
            let unit = sandbox.state.living.units.get_mut(&body).unwrap();
            unit.heading_deg = heading;
            let angle = f64::from(heading).to_radians();
            unit.velocity_enu_m_s = [angle.sin() * 5., angle.cos() * 5., 0.];
        };
        let assert_front_turn = |app: &mut App, sign: f32| {
            let mut query = app.world_mut().query::<(&RollingWheel, &Transform)>();
            let mut fronts = 0;
            for (wheel, transform) in query.iter(app.world()) {
                let axle = transform.rotation * Vec3::X;
                if wheel.front {
                    fronts += 1;
                    assert!(
                        axle.z * sign > 0.05,
                        "front axle must follow the visible turn"
                    );
                } else {
                    assert!(axle.z.abs() < 0.001, "rear axles only roll");
                }
            }
            assert_eq!(fronts, 2);
        };
        set_motion(&mut app, 12.);
        {
            let mut sandbox = app.world_mut().resource_mut::<SandboxRuntime>();
            sandbox.state.living.accounts.get_mut(&1).unwrap().possessed = false;
            sandbox
                .state
                .transport
                .vehicles
                .get_mut(&body)
                .unwrap()
                .dynamics
                .steering_rad = 0.;
        }
        let authoritative = app.world().resource::<SandboxRuntime>().state.clone();
        app.update();
        assert_front_turn(&mut app, 1.);
        assert_eq!(
            app.world().resource::<SandboxRuntime>().state,
            authoritative
        );

        {
            let mut sandbox = app.world_mut().resource_mut::<SandboxRuntime>();
            let account = sandbox.state.living.accounts.get_mut(&1).unwrap();
            account.possessed = true;
            account.active_unit = body;
            sandbox
                .state
                .transport
                .vehicles
                .get_mut(&body)
                .unwrap()
                .dynamics
                .steering_rad = -0.45;
        }
        app.update();
        assert_front_turn(&mut app, -1.);

        // A public replica has no private dynamics; it must show the same
        // accepted turn as an owned company or RTS vehicle.
        {
            let mut sandbox = app.world_mut().resource_mut::<SandboxRuntime>();
            let vehicle = sandbox.state.transport.vehicles.remove(&body).unwrap();
            sandbox
                .state
                .transport
                .visible_vehicles
                .insert(body, (&vehicle).into());
        }
        set_motion(&mut app, 24.);
        app.update();
        assert_front_turn(&mut app, 1.);
        let rolling = wheel_entities(&mut app)[0];
        let before_hold = *app.world().get::<Transform>(rolling).unwrap();
        app.world_mut()
            .resource_mut::<WorldRuntime>()
            .clock
            .time_scale = 0.;
        app.update();
        assert_ne!(
            *app.world().get::<Transform>(rolling).unwrap(),
            before_hold,
            "holding the calendar does not stop physical driving or rolling wheels"
        );
        let before: Vec<_> = wheel_entities(&mut app)
            .into_iter()
            .map(|entity| (entity, *app.world().get::<Transform>(entity).unwrap()))
            .collect();
        app.world_mut().resource_mut::<WorldRuntime>().clock.paused = true;
        app.update();
        for (entity, transform) in before {
            assert_eq!(*app.world().get::<Transform>(entity).unwrap(), transform);
        }
    }

    #[test]
    fn wheel_springs_use_loaded_height_samples_and_never_mutate_authority() {
        let (mut app, _, _) = fixture();
        app.update();
        let origin = app.world().resource::<WorldRuntime>().focus;
        let key = terra_core::TileKey::at(origin, 15).unwrap();
        let terrain = |height| {
            std::sync::Arc::new(terra_core::WorldChunk {
                key,
                heights: terra_core::HeightField::constant(height),
                features: vec![],
                sources: vec![],
                warnings: vec![],
            })
        };
        {
            let mut resident = app.world_mut().resource_mut::<ResidentChunks>();
            resident.level = 15;
            resident.chunks.insert(key, terrain(100.18));
        }
        let authority = app.world().resource::<SandboxRuntime>().state.clone();
        for _ in 0..10 {
            app.update();
        }
        let wheels = wheel_entities(&mut app);
        assert_eq!(wheels.len(), 4);
        for entity in &wheels {
            let wheel = app.world().get::<RollingWheel>(*entity).unwrap();
            let transform = app.world().get::<Transform>(*entity).unwrap();
            assert!((0.17..0.19).contains(&wheel.suspension_offset));
            assert!((transform.translation.y - wheel.radius - 0.18).abs() < 0.01);
        }
        let offsets: Vec<_> = wheels
            .iter()
            .map(|e| {
                app.world()
                    .get::<RollingWheel>(*e)
                    .unwrap()
                    .suspension_offset
            })
            .collect();
        app.world_mut()
            .resource_mut::<ResidentChunks>()
            .chunks
            .clear();
        app.update();
        for (entity, expected) in wheels.iter().zip(&offsets) {
            assert_eq!(
                app.world()
                    .get::<RollingWheel>(*entity)
                    .unwrap()
                    .suspension_offset,
                *expected
            );
        }
        app.world_mut()
            .resource_mut::<ResidentChunks>()
            .chunks
            .insert(key, terrain(100.5));
        app.world_mut().resource_mut::<WorldRuntime>().clock.paused = true;
        app.update();
        for (entity, expected) in wheels.iter().zip(&offsets) {
            assert_eq!(
                app.world()
                    .get::<RollingWheel>(*entity)
                    .unwrap()
                    .suspension_offset,
                *expected
            );
        }
        assert_eq!(app.world().resource::<SandboxRuntime>().state, authority);
    }
}
