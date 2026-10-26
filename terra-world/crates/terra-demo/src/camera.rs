use crate::{
    control::{ControlMode, ControlSet, PlayerControl, WorldViewport},
    interface::UiState,
};
use bevy::{
    input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll},
    prelude::*,
};
use terra_bevy::{
    LivingMotion, NetworkStatus, PoliticalWorld, ResidentChunks, SandboxRuntime, WorldEnvironment,
    WorldRuntime, WorldSet,
    environment::{inside_polygon, sample_chunk_height},
};
use terra_core::{ActorKind, FeatureKind, GeoPosition, LocalFrame, TileKey, TravelMode};

pub struct ExplorerCameraPlugin;
impl Plugin for ExplorerCameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_camera)
            .add_systems(
                Update,
                control_camera
                    .after(ControlSet::Input)
                    .before(WorldSet::Stream),
            )
            .add_systems(
                Update,
                sync_camera.after(WorldSet::Stream).before(WorldSet::Render),
            );
    }
}
#[derive(Component)]
pub struct ExplorerCamera;
#[derive(Resource)]
pub struct CameraRig {
    pub position: GeoPosition,
    pub yaw: f32,
    pub pitch: f32,
    pub needs_grounding: bool,
    pub teleport: Option<GeoPosition>,
    pub altitude_offset: f64,
    pub follow_distance: f32,
    pub(crate) last_mode: ControlMode,
}

fn atlas_view_scale(window: &Window, viewport: &WorldViewport) -> f32 {
    let content = viewport.rect(window, true);
    ((window.height() / 900.) / (content.width().max(1.) / 856.)).max(1.)
}

fn setup_camera(
    mut commands: Commands,
    mut runtime: ResMut<WorldRuntime>,
    options: Res<crate::DemoOptions>,
    mut politics: ResMut<PoliticalWorld>,
) {
    politics.atlas = options.atlas;
    if options.noon && !runtime.network_authority {
        runtime.clock.unix_seconds = (runtime.clock.unix_seconds / 86400.).floor() * 86400.
            + ((12. - runtime.focus.longitude / 15.).rem_euclid(24.)) * 3600.;
    }
    politics.fit_at_epoch(runtime.clock.unix_seconds as i64);
    let offset = if options.camp_view {
        [12., 24., 35.]
    } else {
        [55., 95., 140.]
    };
    let position = LocalFrame::new(runtime.focus).to_geo(offset).unwrap();
    commands.insert_resource(CameraRig {
        position,
        yaw: if options.camp_view { 0.32 } else { 0.34 },
        pitch: if options.camp_view { -0.56 } else { -0.55 },
        needs_grounding: true,
        teleport: None,
        altitude_offset: offset[1],
        follow_distance: 5.5,
        last_mode: ControlMode::Rts,
    });
    commands.spawn((
        ExplorerCamera,
        Camera3d::default(),
        Projection::Perspective(PerspectiveProjection {
            far: 50_000.,
            near: 0.1,
            fov: 56_f32.to_radians(),
            ..default()
        }),
        Transform::from_xyz(55., 95., 140.).looking_at(Vec3::ZERO, Vec3::Y),
        DistanceFog {
            color: Color::srgb(0.63, 0.73, 0.78),
            falloff: FogFalloff::Linear {
                start: 2500.,
                end: 9000.,
            },
            ..default()
        },
    ));
}

/// Camera-local input to physical east/north/up. Looking up never launches a
/// ground unit; aircraft alone may add explicit vertical intent.
fn movement_velocity(direction: Vec2, yaw: f32, vertical: f32, speed: f64) -> [f64; 3] {
    let local = Vec3::new(direction.x, 0., direction.y).clamp_length_max(1.);
    let horizontal = Quat::from_rotation_y(yaw) * local;
    let requested = Vec3::new(horizontal.x, vertical, horizontal.z).clamp_length_max(1.);
    [
        requested.x as f64 * speed,
        -requested.z as f64 * speed,
        requested.y as f64 * speed,
    ]
}

fn chase_offsets(kind: ActorKind, requested_distance: f32) -> (f32, f32) {
    match kind {
        ActorKind::Human | ActorKind::Creature => (1.68, requested_distance.clamp(2.5, 14.)),
        ActorKind::Animal => (1.0, (requested_distance + 1.).clamp(3., 16.)),
        ActorKind::Vehicle => (2.05, (requested_distance + 3.5).clamp(5., 20.)),
        ActorKind::Ship => (3.0, (requested_distance + 7.).clamp(8., 28.)),
        ActorKind::Helicopter => (2.3, (requested_distance + 9.).clamp(9., 32.)),
        ActorKind::Airplane => (2.0, (requested_distance + 12.).clamp(12., 40.)),
        ActorKind::Balloon => (2.0, (requested_distance + 9.).clamp(10., 35.)),
    }
}

/// A conservative chase-camera sweep against the heightfield. Unlike clamping
/// only the end point, this keeps the camera in front of a ridge behind a unit.
fn clear_chase_position(
    start: GeoPosition,
    desired: GeoPosition,
    resident: &ResidentChunks,
    sandbox: &SandboxRuntime,
) -> GeoPosition {
    let frame = LocalFrame::new(start);
    let delta = frame.to_local(desired);
    let mut result = start;
    let facility_footprints = sandbox.state.living.facility_footprints();
    for step in 1..=24 {
        let fraction = step as f64 / 24.;
        let Ok(candidate) = frame.to_geo(delta.map(|x| x * fraction)) else {
            break;
        };
        let Some(ground) = resident.terrain_height(candidate) else {
            break;
        };
        if candidate.altitude_m < ground + 0.45 {
            break;
        }
        let source_wall = TileKey::at(candidate, resident.level)
            .ok()
            .and_then(|key| resident.chunks.get(&key))
            .is_some_and(|chunk| {
                chunk.features.iter().any(|f| {
                    f.kind == FeatureKind::Building
                        && inside_polygon(candidate, &f.points)
                        && candidate.altitude_m
                            < sample_chunk_height(chunk, candidate)
                                + f.height_m.max(2.) as f64
                                + 0.3
                })
            });
        let authored_wall = sandbox.state.structures.values().any(|s| {
            candidate.surface_distance_m(s.position) < s.radius_m + 0.3
                && candidate.altitude_m < s.position.altitude_m + if s.housing { 5.5 } else { 1.8 }
        });
        let facility_wall = facility_footprints.iter().any(|(p, r)| {
            candidate.surface_distance_m(*p) < r + 0.25 && candidate.altitude_m < p.altitude_m + 5.5
        });
        if source_wall || authored_wall || facility_wall {
            break;
        }
        result = candidate;
    }
    result
}

#[allow(clippy::too_many_arguments)]
fn control_camera(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    buttons: Res<ButtonInput<MouseButton>>,
    mouse: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    mut rig: ResMut<CameraRig>,
    mut runtime: ResMut<WorldRuntime>,
    resident: Res<ResidentChunks>,
    ui: Res<UiState>,
    mut politics: ResMut<PoliticalWorld>,
    window: Single<&Window>,
    control: Res<PlayerControl>,
    mut locomotion: ResMut<LivingMotion>,
    mut net: Option<ResMut<NetworkStatus>>,
    viewport: Res<WorldViewport>,
    sandbox: Res<SandboxRuntime>,
) {
    let in_view = window
        .cursor_position()
        .is_some_and(|p| viewport.contains(&window, p, politics.atlas));
    locomotion.active = false;
    locomotion.velocity_enu_m_s = [0.; 3];
    if let Some(net) = net.as_mut() {
        net.desired_velocity_enu_m_s = [0.; 3];
    }
    if politics.atlas {
        if !ui.editing_gps && in_view {
            politics.atlas_zoom =
                (politics.atlas_zoom * (1. + scroll.delta.y * 0.12).max(0.2)).clamp(0.6, 20_000.);
            if buttons.pressed(MouseButton::Right) || buttons.pressed(MouseButton::Middle) {
                let zoom = politics.atlas_zoom;
                politics.atlas_center += Vec2::new(-mouse.delta.x, mouse.delta.y)
                    * 1.2
                    * atlas_view_scale(&window, &viewport)
                    * (900. / window.height())
                    / zoom;
                politics.atlas_center = politics
                    .atlas_center
                    .clamp(Vec2::new(-500., -250.), Vec2::new(500., 250.));
            }
        }
        return;
    }
    if control.mode.is_possessed() {
        if let Some(unit) = control
            .possessed
            .and_then(|id| sandbox.state.living.units.get(&id))
        {
            let entering = rig.last_mode == ControlMode::Rts;
            rig.last_mode = control.mode;
            rig.needs_grounding = false;
            if !ui.editing_gps {
                if buttons.pressed(MouseButton::Right) && in_view {
                    rig.yaw -= mouse.delta.x * 0.003;
                    rig.pitch = (rig.pitch - mouse.delta.y * 0.003).clamp(-1.35, 1.25);
                }
                if in_view && control.mode == ControlMode::ThirdPerson {
                    rig.follow_distance =
                        (rig.follow_distance - scroll.delta.y * 0.75).clamp(2.5, 14.);
                }
                let direction = Vec2::new(
                    (keys.pressed(KeyCode::KeyD) as u8 as f32)
                        - (keys.pressed(KeyCode::KeyA) as u8 as f32),
                    (keys.pressed(KeyCode::KeyS) as u8 as f32)
                        - (keys.pressed(KeyCode::KeyW) as u8 as f32),
                );
                let air = unit.kind.travel_mode() == TravelMode::Air;
                let vertical = if air {
                    (keys.pressed(KeyCode::KeyR) as u8 as f32)
                        - (keys.pressed(KeyCode::KeyF) as u8 as f32)
                } else {
                    0.
                };
                let sprint = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
                let capability = unit.speed_limit_m_s();
                let speed = if sprint {
                    capability
                } else {
                    capability
                        * if unit.kind == ActorKind::Human {
                            0.60
                        } else {
                            0.72
                        }
                };
                let velocity = movement_velocity(direction, rig.yaw, vertical, speed);
                let accepted = sandbox
                    .state
                    .living
                    .accounts
                    .get(&sandbox.player_id)
                    .is_some_and(|a| a.possessed && a.active_unit == unit.id);
                if accepted {
                    locomotion.active = true;
                    locomotion.velocity_enu_m_s = velocity;
                    locomotion.heading_deg = if velocity[0].hypot(velocity[1]) > 0.001 {
                        velocity[0].atan2(velocity[1]).to_degrees() as f32
                    } else {
                        -rig.yaw.to_degrees()
                    };
                    if let Some(net) = net.as_mut() {
                        net.desired_velocity_enu_m_s = velocity;
                        net.follow_player = false;
                    }
                }
            }
            let (eye_height, distance) = chase_offsets(unit.kind, rig.follow_distance);
            let mut eye = unit.position;
            eye.altitude_m += eye_height as f64;
            let desired = if control.mode == ControlMode::FirstPerson {
                eye
            } else {
                let rotation = Quat::from_euler(EulerRot::YXZ, rig.yaw, rig.pitch, 0.);
                let offset = rotation * Vec3::Z * distance + Vec3::Y * 0.55;
                let position = LocalFrame::new(eye)
                    .to_geo(offset.to_array().map(|x| x as f64))
                    .unwrap_or(eye);
                clear_chase_position(eye, position, &resident, &sandbox)
            };
            if entering || rig.position.surface_distance_m(desired) > 150. {
                rig.position = desired;
            } else {
                let frame = LocalFrame::new(rig.position);
                let delta = frame.to_local(desired);
                let rate = if control.mode == ControlMode::FirstPerson {
                    30.
                } else {
                    12.
                };
                let blend = 1. - (-rate * time.delta_secs_f64().min(0.25)).exp();
                let blended = frame.to_geo(delta.map(|x| x * blend)).unwrap_or(desired);
                rig.position = if control.mode == ControlMode::FirstPerson {
                    blended
                } else {
                    clear_chase_position(eye, blended, &resident, &sandbox)
                };
            }
            runtime.focus = GeoPosition {
                altitude_m: 0.,
                ..unit.position
            };
            return;
        }
    }
    rig.last_mode = ControlMode::Rts;
    if let Some(position) = rig.teleport.take() {
        rig.position = position;
        rig.needs_grounding = true;
        runtime.focus = GeoPosition {
            altitude_m: 0.,
            ..position
        };
    }
    if rig.needs_grounding {
        if let Some(ground) = resident.terrain_height(rig.position) {
            rig.position.altitude_m = ground + rig.altitude_offset;
            rig.needs_grounding = false;
        }
    }
    if !ui.editing_gps {
        let rotating = buttons.pressed(MouseButton::Middle)
            || buttons.pressed(MouseButton::Right)
                && (keys.pressed(KeyCode::AltLeft) || keys.pressed(KeyCode::AltRight));
        if rotating && in_view {
            rig.yaw -= mouse.delta.x * 0.003;
            rig.pitch = (rig.pitch - mouse.delta.y * 0.003).clamp(-1.45, -0.08);
        }
        let direction = if keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight)
        {
            Vec2::ZERO
        } else {
            Vec2::new(
                keys.pressed(KeyCode::KeyD) as u8 as f32 - keys.pressed(KeyCode::KeyA) as u8 as f32,
                keys.pressed(KeyCode::KeyS) as u8 as f32 - keys.pressed(KeyCode::KeyW) as u8 as f32,
            )
        };
        let altitude = resident
            .terrain_height(rig.position)
            .map(|h| rig.position.altitude_m - h)
            .unwrap_or(95.);
        let speed = (altitude * 0.75).clamp(18., 900.)
            * if keys.pressed(KeyCode::ShiftLeft) {
                3.
            } else {
                1.
            };
        let velocity = movement_velocity(
            direction,
            rig.yaw,
            keys.pressed(KeyCode::KeyR) as u8 as f32 - keys.pressed(KeyCode::KeyF) as u8 as f32,
            speed,
        );
        let dt = time.delta_secs_f64().min(0.1);
        let mut delta = [velocity[0] * dt, velocity[2] * dt, -velocity[1] * dt];
        if in_view && scroll.delta.y != 0. {
            let rotation = Quat::from_euler(EulerRot::YXZ, rig.yaw, rig.pitch, 0.);
            let zoom =
                rotation * Vec3::NEG_Z * scroll.delta.y * (altitude as f32 * 0.11).clamp(2., 180.);
            for (d, z) in delta.iter_mut().zip(zoom.to_array()) {
                *d += z as f64;
            }
        }
        if delta.iter().any(|x| x.abs() > 1e-6) {
            rig.position = LocalFrame::new(rig.position)
                .to_geo(delta)
                .unwrap_or(rig.position);
            if let Some(net) = net.as_mut() {
                net.follow_player = false;
            }
        }
        // The legacy reference player stays operable when joining a host that
        // explicitly disabled its living layer.
        if let Some(net) = net.as_mut() {
            if !sandbox.state.living.enabled {
                net.desired_velocity_enu_m_s = [
                    (keys.pressed(KeyCode::ArrowRight) as u8 as f64
                        - keys.pressed(KeyCode::ArrowLeft) as u8 as f64)
                        * 5.,
                    (keys.pressed(KeyCode::ArrowUp) as u8 as f64
                        - keys.pressed(KeyCode::ArrowDown) as u8 as f64)
                        * 5.,
                    0.,
                ];
                if net.follow_player {
                    if let Some(actor) = net.actor_id.and_then(|id| net.actors.get(&id)) {
                        if let Ok(pos) = LocalFrame::new(actor.position).to_geo([15., 22., 32.]) {
                            rig.position = pos;
                            rig.yaw = 0.42;
                            rig.pitch = -0.52;
                        }
                    }
                }
            }
        }
    }
    if let Some(ground) = resident.terrain_height(rig.position) {
        rig.position.altitude_m = rig.position.altitude_m.max(ground + 3.);
    }
    rig.position.altitude_m = rig.position.altitude_m.min(30_000.);
    runtime.focus = GeoPosition {
        altitude_m: 0.,
        ..rig.position
    };
}

fn sync_camera(
    rig: Res<CameraRig>,
    runtime: Res<WorldRuntime>,
    politics: Res<PoliticalWorld>,
    control: Res<PlayerControl>,
    viewport: Res<WorldViewport>,
    window: Single<&Window>,
    mut camera: Query<(&mut Transform, &mut Projection), With<ExplorerCamera>>,
) {
    if let Ok((mut transform, mut projection)) = camera.single_mut() {
        if let Projection::Perspective(perspective) = &mut *projection {
            perspective.fov = if politics.atlas {
                std::f32::consts::FRAC_PI_4
            } else {
                match control.mode {
                    ControlMode::FirstPerson => 78_f32.to_radians(),
                    ControlMode::ThirdPerson => 65_f32.to_radians(),
                    ControlMode::Rts => 56_f32.to_radians(),
                }
            };
            perspective.near = if politics.atlas { 0.001 } else { 0.1 };
        }
        if politics.atlas {
            *transform = Transform::from_xyz(
                politics.atlas_center.x,
                politics.atlas_center.y,
                1400. * atlas_view_scale(&window, &viewport) / politics.atlas_zoom,
            );
            return;
        }
        transform.translation =
            Vec3::from_array(runtime.frame.to_local(rig.position).map(|v| v as f32));
        transform.rotation = Quat::from_euler(EulerRot::YXZ, rig.yaw, rig.pitch, 0.);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn possession_input_uses_camera_yaw_without_diagonal_speed_gain() {
        let north = movement_velocity(Vec2::new(0., -1.), 0., 0., 4.);
        assert!((north[0]).abs() < 1e-6 && (north[1] - 4.).abs() < 1e-6 && north[2] == 0.);
        let west = movement_velocity(Vec2::new(0., -1.), std::f32::consts::FRAC_PI_2, 0., 4.);
        assert!((west[0] + 4.).abs() < 1e-5 && west[1].abs() < 1e-5);
        let diagonal = movement_velocity(Vec2::new(1., -1.), 0., 1., 4.);
        assert!((diagonal.iter().map(|v| v * v).sum::<f64>().sqrt() - 4.).abs() < 1e-5);
    }
    #[test]
    fn chase_distance_matches_actor_scale_and_is_bounded() {
        assert!(chase_offsets(ActorKind::Vehicle, 5.5).1 > chase_offsets(ActorKind::Human, 5.5).1);
        assert_eq!(chase_offsets(ActorKind::Human, 1000.).1, 14.);
        assert_eq!(chase_offsets(ActorKind::Human, -1000.).1, 2.5);
    }
}
