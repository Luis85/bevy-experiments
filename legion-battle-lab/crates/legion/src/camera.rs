//! Orbit camera controls are presentation-only and never change simulation.

use battle_core::{arena_half_extent, terrain_height};
use bevy::{
    input::mouse::{MouseMotion, MouseScrollUnit, MouseWheel},
    input_focus::InputFocus,
    prelude::*,
    render::view::{Msaa, NoIndirectDrawing},
    window::PrimaryWindow,
};

use crate::{
    app::{AppSet, Screen, Session},
    ui::UiPointerCapture,
};

pub struct BattleCameraPlugin;

/// Marker used by smoke tests and future selection tools.
#[derive(Component)]
pub struct BattleCamera;

#[derive(SystemSet, Debug, Clone, Eq, PartialEq, Hash)]
pub struct BattleCameraSet;

impl Plugin for BattleCameraPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<OrbitCamera>()
            .add_systems(Startup, spawn_camera)
            .add_systems(
                Update,
                control_camera
                    .in_set(BattleCameraSet)
                    .in_set(AppSet::Presentation),
            );
    }
}

#[derive(Resource)]
struct OrbitCamera {
    center: Vec3,
    yaw: f32,
    pitch: f32,
    distance: f32,
    elevation_focus: f32,
    generation: u64,
    screen: Screen,
}

impl Default for OrbitCamera {
    fn default() -> Self {
        Self {
            center: Vec3::ZERO,
            yaw: -0.12,
            pitch: 0.83,
            distance: 75.0,
            elevation_focus: 1.1,
            generation: 0,
            screen: Screen::Menu,
        }
    }
}

fn spawn_camera(mut commands: Commands) {
    commands.spawn((
        Name::new("Battle camera"),
        BattleCamera,
        Camera3d::default(),
        Projection::Perspective(PerspectiveProjection {
            fov: 0.74,
            near: 0.1,
            far: 2500.0,
            ..default()
        }),
        Transform::from_xyz(-15.0, 55.0, 60.0).looking_at(Vec3::ZERO, Vec3::Y),
        Msaa::Off,
        NoIndirectDrawing,
        DistanceFog {
            color: Color::srgb(0.57, 0.66, 0.67),
            directional_light_color: Color::NONE,
            falloff: FogFalloff::Linear {
                start: 130.0,
                end: 420.0,
            },
            ..default()
        },
    ));
}

fn reset(orbit: &mut OrbitCamera, session: &Session, screen: &Screen) {
    let Some(battle) = &session.battle else {
        return;
    };
    let half = battle
        .units
        .positions
        .iter()
        .fold(Vec2::splat(8.0), |size, p| {
            size.max(Vec2::new(p.x.abs(), p.y.abs()))
        });
    let blue_height = battle.config.blue.category.elevation().max(0.0);
    let red_height = battle.config.red.category.elevation().max(0.0);
    orbit.elevation_focus = (blue_height + red_height) * 0.5 + 1.1;
    orbit.center = Vec3::new(0.0, orbit.elevation_focus, 0.0);
    orbit.yaw = -0.12;
    orbit.pitch = 0.86;
    orbit.distance =
        (half.x.max(half.y * 1.2) * 2.5).max(43.0 + (blue_height - red_height).abs() * 1.1);
    if *screen != Screen::Battle {
        orbit.center.x = half.x * 0.16;
        orbit.yaw = -0.31;
        orbit.pitch = 0.62;
        orbit.distance *= 0.95;
    }
    orbit.generation = session.generation;
    orbit.screen = screen.clone();
}

#[allow(clippy::too_many_arguments)]
fn control_camera(
    time: Res<Time<Real>>,
    session: Res<Session>,
    screen: Res<State<Screen>>,
    mut orbit: ResMut<OrbitCamera>,
    keys: Res<ButtonInput<KeyCode>>,
    buttons: Res<ButtonInput<MouseButton>>,
    mut motion: MessageReader<MouseMotion>,
    mut wheel: MessageReader<MouseWheel>,
    capture: Res<UiPointerCapture>,
    focus: Res<InputFocus>,
    primary_window: Query<(), With<PrimaryWindow>>,
    mut cameras: Query<(&mut Transform, &mut DistanceFog, &mut Projection), With<BattleCamera>>,
) {
    // Always drain input; returning to the world must never replay old motion.
    let delta = motion
        .read()
        .fold(Vec2::ZERO, |sum, event| sum + event.delta);
    let scroll: f32 = wheel
        .read()
        .map(|event| {
            event.y
                * if event.unit == MouseScrollUnit::Line {
                    1.0
                } else {
                    0.025
                }
        })
        .sum();
    let Some(battle) = &session.battle else {
        return;
    };
    if orbit.generation != session.generation || orbit.screen != *screen.get() {
        reset(&mut orbit, &session, screen.get());
    }
    let extent = arena_half_extent(&battle.config);
    let dt = time.delta_secs().min(0.05);
    if *screen.get() == Screen::Battle && !session.help {
        // Bevy initially focuses the primary window. Only an actual UI entity
        // owns the keyboard; --quick must not require a clearing click first.
        let ui_has_focus = focus
            .get()
            .is_some_and(|entity| !primary_window.contains(entity));
        if !ui_has_focus {
            if keys.just_pressed(KeyCode::Home) || keys.just_pressed(KeyCode::KeyR) {
                reset(&mut orbit, &session, screen.get());
            }
            let mut movement = Vec3::ZERO;
            let forward = Vec3::new(-orbit.yaw.sin(), 0.0, -orbit.yaw.cos());
            let right = Vec3::new(orbit.yaw.cos(), 0.0, -orbit.yaw.sin());
            if keys.pressed(KeyCode::KeyW) || keys.pressed(KeyCode::ArrowUp) {
                movement += forward;
            }
            if keys.pressed(KeyCode::KeyS) || keys.pressed(KeyCode::ArrowDown) {
                movement -= forward;
            }
            if keys.pressed(KeyCode::KeyA) || keys.pressed(KeyCode::ArrowLeft) {
                movement -= right;
            }
            if keys.pressed(KeyCode::KeyD) || keys.pressed(KeyCode::ArrowRight) {
                movement += right;
            }
            let speed = orbit.distance
                * if keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight) {
                    0.95
                } else {
                    0.42
                };
            orbit.center += movement.normalize_or_zero() * speed * dt;
            if keys.pressed(KeyCode::KeyQ) {
                orbit.yaw += dt * 0.9;
            }
            if keys.pressed(KeyCode::KeyE) {
                orbit.yaw -= dt * 0.9;
            }
        }
        if !capture.0 {
            if buttons.pressed(MouseButton::Right) {
                orbit.yaw -= delta.x * 0.005;
                orbit.pitch = (orbit.pitch + delta.y * 0.004).clamp(0.18, 1.43);
            }
            orbit.distance = (orbit.distance * (-scroll * 0.12).exp()).clamp(9.0, extent * 4.5);
        }
        orbit.center.x = orbit.center.x.clamp(-extent, extent);
        orbit.center.z = orbit.center.z.clamp(-extent, extent);
        orbit.center.y = terrain_height(battle.config.map, orbit.center.x, orbit.center.z).max(0.0)
            + orbit.elevation_focus;
    } else if *screen.get() == Screen::Menu {
        orbit.yaw += dt * 0.017;
    }
    let horizontal = orbit.distance * orbit.pitch.cos();
    let offset = Vec3::new(
        orbit.yaw.sin() * horizontal,
        orbit.distance * orbit.pitch.sin(),
        orbit.yaw.cos() * horizontal,
    );
    for (mut transform, mut fog, mut projection) in &mut cameras {
        *transform =
            Transform::from_translation(orbit.center + offset).looking_at(orbit.center, Vec3::Y);
        fog.falloff = FogFalloff::Linear {
            start: extent * 1.8,
            end: extent * 5.5,
        };
        if let Projection::Perspective(perspective) = &mut *projection {
            // Million-unit formations extend beyond the small-scene default.
            perspective.far = (extent * 8.0).max(2_500.0);
        }
    }
}
