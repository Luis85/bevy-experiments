use crate::interface::UiState;
use bevy::{
    input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll},
    prelude::*,
};
use terra_bevy::{
    NetworkStatus, PoliticalWorld, ResidentChunks, WorldEnvironment, WorldRuntime, WorldSet,
};
use terra_core::{GeoPosition, LocalFrame};
pub struct ExplorerCameraPlugin;
impl Plugin for ExplorerCameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_camera)
            .add_systems(Update, control_camera.before(WorldSet::Stream))
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
}
fn atlas_view_scale(window: &Window) -> f32 {
    // Keep fitted maps in the central viewport when the side panels take up a
    // larger share of a small window. The map remains centered and zoomable.
    ((window.height() / 900.) / ((window.width() - 584.).max(1.) / 856.)).max(1.)
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
        [15., 22., 32.]
    } else {
        [200., 650., 650.]
    };
    let position = LocalFrame::new(runtime.focus).to_geo(offset).unwrap();
    commands.insert_resource(CameraRig {
        position,
        yaw: if options.camp_view { 0.42 } else { -0.28 },
        pitch: if options.camp_view { -0.52 } else { -0.65 },
        needs_grounding: true,
        teleport: None,
        altitude_offset: offset[1],
    });
    commands.spawn((
        ExplorerCamera,
        Camera3d::default(),
        Projection::Perspective(PerspectiveProjection {
            far: 50_000.,
            near: 0.001,
            ..default()
        }),
        Transform::from_xyz(200., 650., 650.).looking_at(Vec3::ZERO, Vec3::Y),
        DistanceFog {
            color: Color::srgb(0.5, 0.65, 0.7),
            falloff: FogFalloff::Linear {
                start: 1200.,
                end: 4800.,
            },
            ..default()
        },
    ));
}
fn control_camera(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    buttons: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    mut rig: ResMut<CameraRig>,
    mut runtime: ResMut<WorldRuntime>,
    resident: Res<ResidentChunks>,
    ui: Res<UiState>,
    mut politics: ResMut<PoliticalWorld>,
    window: Single<&Window>,
    mut net: Option<ResMut<NetworkStatus>>,
) {
    if let Some(net) = net.as_mut() {
        net.desired_velocity_enu_m_s = [0.; 3];
    }
    let in_view = window.cursor_position().is_some_and(|p| {
        p.x > 280.
            && p.x < window.width() - 296.
            && p.y > if politics.atlas { 184. } else { 134. }
            && p.y < window.height() - if politics.atlas { 220. } else { 96. }
    });
    if politics.atlas {
        if !ui.editing_gps && in_view {
            politics.atlas_zoom =
                (politics.atlas_zoom * (1. + scroll.delta.y * 0.12).max(0.2)).clamp(0.6, 20_000.);
            if buttons.pressed(MouseButton::Right) && in_view {
                let zoom = politics.atlas_zoom;
                politics.atlas_center += Vec2::new(-motion.delta.x, motion.delta.y)
                    * 1.2
                    * atlas_view_scale(&window)
                    * (900. / window.height())
                    / zoom;
                politics.atlas_center = politics
                    .atlas_center
                    .clamp(Vec2::new(-500., -250.), Vec2::new(500., 250.));
            }
        }
        return;
    }
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
    if let Some(net) = net.as_mut() {
        net.desired_velocity_enu_m_s = [0.; 3];
    }
    if !ui.editing_gps {
        if buttons.pressed(MouseButton::Right) && in_view {
            rig.yaw -= motion.delta.x * 0.003;
            rig.pitch = (rig.pitch - motion.delta.y * 0.003).clamp(-1.5, 1.4);
        }
        let rotation = Quat::from_euler(EulerRot::YXZ, rig.yaw, rig.pitch, 0.);
        let mut direction = Vec3::ZERO;
        if keys.pressed(KeyCode::KeyW) {
            direction.z -= 1.;
        }
        if keys.pressed(KeyCode::KeyS) {
            direction.z += 1.;
        }
        if keys.pressed(KeyCode::KeyA) {
            direction.x -= 1.;
        }
        if keys.pressed(KeyCode::KeyD) {
            direction.x += 1.;
        }
        direction = rotation * direction;
        if keys.pressed(KeyCode::KeyE) {
            direction.y += 1.;
        }
        if keys.pressed(KeyCode::KeyQ) {
            direction.y -= 1.;
        }
        if in_view && scroll.delta.y != 0. {
            direction += rotation * Vec3::NEG_Z * scroll.delta.y * 6.;
        }
        let speed = if keys.pressed(KeyCode::ShiftLeft) {
            1200.
        } else {
            160.
        };
        if direction.length_squared() > 0. {
            let p = runtime.frame.to_local(rig.position);
            let delta = direction.clamp_length_max(4.) * speed * time.delta_secs().min(0.1);
            if let Ok(pos) = runtime.frame.to_geo([
                p[0] + delta.x as f64,
                p[1] + delta.y as f64,
                p[2] + delta.z as f64,
            ]) {
                rig.position = pos;
            }
            if let Some(net) = net.as_mut() {
                net.follow_player = false;
            }
        }
        if let Some(net) = net.as_mut() {
            net.desired_velocity_enu_m_s = [0.; 3];
            if keys.pressed(KeyCode::ArrowUp) {
                net.desired_velocity_enu_m_s[1] += 5.;
            }
            if keys.pressed(KeyCode::ArrowDown) {
                net.desired_velocity_enu_m_s[1] -= 5.;
            }
            if keys.pressed(KeyCode::ArrowLeft) {
                net.desired_velocity_enu_m_s[0] -= 5.;
            }
            if keys.pressed(KeyCode::ArrowRight) {
                net.desired_velocity_enu_m_s[0] += 5.;
            }
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
    window: Single<&Window>,
    mut camera: Query<&mut Transform, With<ExplorerCamera>>,
) {
    if let Ok(mut transform) = camera.single_mut() {
        if politics.atlas {
            *transform = Transform::from_xyz(
                politics.atlas_center.x,
                politics.atlas_center.y,
                1400. * atlas_view_scale(&window) / politics.atlas_zoom,
            );
            return;
        }
        transform.translation =
            Vec3::from_array(runtime.frame.to_local(rig.position).map(|v| v as f32));
        transform.rotation = Quat::from_euler(EulerRot::YXZ, rig.yaw, rig.pitch, 0.);
    }
}
