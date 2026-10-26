//! RTS camera: WASD/arrows pan, mouse wheel zooms (clamped), Q/E orbit.

use bevy::{
    input::mouse::{AccumulatedMouseScroll, MouseScrollUnit},
    prelude::*,
};

use npc_framework::view_helpers::ViewSystems;

const MIN_DISTANCE: f32 = 6.0;
const MAX_DISTANCE: f32 = 60.0;
/// Pan limit around the origin, metres.
const PAN_LIMIT: f32 = 20.0;

/// Orbit camera state: looks at `focus` from `distance` at `pitch` above the
/// ground plane, rotated `yaw` around +Y (yaw 0 = camera on the +Z side).
#[derive(Component, Clone, Copy, Debug)]
pub struct RtsCamera {
    pub focus: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
}

impl Default for RtsCamera {
    fn default() -> Self {
        Self {
            focus: Vec3::ZERO,
            yaw: 0.0,
            pitch: 45f32.to_radians(),
            distance: 25.0,
        }
    }
}

impl RtsCamera {
    pub fn transform(&self) -> Transform {
        let offset = Vec3::new(
            self.pitch.cos() * self.yaw.sin(),
            self.pitch.sin(),
            self.pitch.cos() * self.yaw.cos(),
        ) * self.distance;
        Transform::from_translation(self.focus + offset).looking_at(self.focus, Vec3::Y)
    }
}

pub(crate) fn plugin(app: &mut App) {
    app.add_systems(Startup, spawn_camera)
        .add_systems(Update, rts_camera.in_set(ViewSystems::Input));
}

fn spawn_camera(mut commands: Commands) {
    let rts = RtsCamera::default();
    commands.spawn((
        Name::new("camera"),
        Camera3d::default(),
        rts.transform(),
        rts,
    ));
}

fn rts_camera(
    time: Res<Time>,
    keys: Option<Res<ButtonInput<KeyCode>>>,
    scroll: Option<Res<AccumulatedMouseScroll>>,
    mut cameras: Query<(&mut RtsCamera, &mut Transform)>,
) {
    let dt = time.delta_secs();
    for (mut rts, mut transform) in &mut cameras {
        if let Some(keys) = keys.as_deref() {
            let pressed = |a: KeyCode, b: KeyCode| keys.pressed(a) || keys.pressed(b);
            let mut input = Vec2::ZERO;
            if pressed(KeyCode::KeyW, KeyCode::ArrowUp) {
                input.y -= 1.0;
            }
            if pressed(KeyCode::KeyS, KeyCode::ArrowDown) {
                input.y += 1.0;
            }
            if pressed(KeyCode::KeyA, KeyCode::ArrowLeft) {
                input.x -= 1.0;
            }
            if pressed(KeyCode::KeyD, KeyCode::ArrowRight) {
                input.x += 1.0;
            }
            if input != Vec2::ZERO {
                // Screen-relative pan on the ground plane, faster when zoomed out.
                let rot = Quat::from_rotation_y(rts.yaw);
                let world = rot * Vec3::new(input.x, 0.0, input.y).normalize();
                let speed = rts.distance * 0.8;
                rts.focus += world * speed * dt;
                rts.focus = rts
                    .focus
                    .clamp(Vec3::splat(-PAN_LIMIT), Vec3::splat(PAN_LIMIT));
                rts.focus.y = 0.0;
            }
            if keys.pressed(KeyCode::KeyQ) {
                rts.yaw -= 1.5 * dt;
            }
            if keys.pressed(KeyCode::KeyE) {
                rts.yaw += 1.5 * dt;
            }
        }
        if let Some(scroll) = scroll.as_deref() {
            let lines = match scroll.unit {
                MouseScrollUnit::Line => scroll.delta.y,
                MouseScrollUnit::Pixel => scroll.delta.y / 40.0,
            };
            if lines != 0.0 {
                rts.distance =
                    (rts.distance * 0.9f32.powf(lines)).clamp(MIN_DISTANCE, MAX_DISTANCE);
            }
        }
        let want = rts.transform();
        if *transform != want {
            *transform = want;
        }
    }
}
