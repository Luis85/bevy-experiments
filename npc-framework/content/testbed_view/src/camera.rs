//! RTS camera (as the village's): WASD/arrows pan, mouse wheel zooms, Q/E
//! orbit, PageUp/PageDown tilt, F centres on the selected unit.

use bevy::{
    input::mouse::{AccumulatedMouseScroll, MouseScrollUnit},
    prelude::*,
};
use npc_framework::prelude::{SimIndex, SimPos};
use npc_framework::view_helpers::{Selected, ViewSystems, sim_to_world};

const MIN_DISTANCE: f32 = 5.0;
const MAX_DISTANCE: f32 = 80.0;
const MIN_PITCH: f32 = 0.35;
const MAX_PITCH: f32 = 1.45;
/// Pan limit around the compound, metres.
const PAN_LIMIT: f32 = 30.0;

/// Orbit camera state: looks at `focus` from `distance` at `pitch` above the
/// ground plane, rotated `yaw` around +Y (yaw 0 = camera on the +Z side).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct RtsCamera {
    pub focus: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
}

impl Default for RtsCamera {
    /// The whole compound, the skeletons outside its east wall and the
    /// wagon outside the gate.
    fn default() -> Self {
        Self {
            focus: Vec3::new(2.5, 0.0, -2.0),
            yaw: 0.0,
            pitch: 58f32.to_radians(),
            distance: 36.0,
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

/// The start view (insert before `Startup`; default [`RtsCamera::default`]).
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
pub struct CameraStart(pub RtsCamera);

/// Centres the camera on the selected unit once it exists (`--select`).
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct CameraFocusOnSelected {
    /// Distance to use once focused (`None`: keep).
    pub distance: Option<f32>,
}

pub(crate) fn plugin(app: &mut App) {
    app.add_systems(Startup, spawn_camera).add_systems(
        Update,
        (focus_at_start, rts_camera)
            .chain()
            .in_set(ViewSystems::Input),
    );
}

fn spawn_camera(mut commands: Commands, start: Option<Res<CameraStart>>) {
    let rts = start.map(|s| s.0).unwrap_or_default();
    commands.spawn((
        Name::new("camera"),
        Camera3d::default(),
        rts.transform(),
        rts,
    ));
}

fn selected_pos(
    selected: Option<&Selected>,
    index: Option<&SimIndex>,
    positions: &Query<&SimPos>,
) -> Option<Vec3> {
    let id = selected?.0?;
    let e = index?.get(id)?;
    positions.get(e).ok().map(|p| sim_to_world(*p))
}

/// Once the preselected unit exists, focus on it (then forget the request).
fn focus_at_start(
    mut commands: Commands,
    request: Option<Res<CameraFocusOnSelected>>,
    selected: Option<Res<Selected>>,
    index: Option<Res<SimIndex>>,
    positions: Query<&SimPos>,
    mut cameras: Query<&mut RtsCamera>,
) {
    let Some(request) = request else { return };
    let Some(at) = selected_pos(selected.as_deref(), index.as_deref(), &positions) else {
        return;
    };
    for mut rts in &mut cameras {
        rts.focus = at;
        if let Some(d) = request.distance {
            rts.distance = d;
        }
    }
    commands.remove_resource::<CameraFocusOnSelected>();
}

#[allow(clippy::too_many_arguments)]
fn rts_camera(
    time: Res<Time>,
    keys: Option<Res<ButtonInput<KeyCode>>>,
    scroll: Option<Res<AccumulatedMouseScroll>>,
    selected: Option<Res<Selected>>,
    index: Option<Res<SimIndex>>,
    positions: Query<&SimPos>,
    mut cameras: Query<(&mut RtsCamera, &mut Transform)>,
) {
    let dt = time.delta_secs();
    let focus_target = selected_pos(selected.as_deref(), index.as_deref(), &positions);
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
                let rot = Quat::from_rotation_y(rts.yaw);
                let world = rot * Vec3::new(input.x, 0.0, input.y).normalize();
                let speed = rts.distance * 0.8;
                rts.focus += world * speed * dt;
            }
            if keys.pressed(KeyCode::KeyQ) {
                rts.yaw -= 1.5 * dt;
            }
            if keys.pressed(KeyCode::KeyE) {
                rts.yaw += 1.5 * dt;
            }
            if keys.pressed(KeyCode::PageUp) {
                rts.pitch = (rts.pitch + 0.8 * dt).min(MAX_PITCH);
            }
            if keys.pressed(KeyCode::PageDown) {
                rts.pitch = (rts.pitch - 0.8 * dt).max(MIN_PITCH);
            }
            if keys.just_pressed(KeyCode::KeyF)
                && let Some(at) = focus_target
            {
                rts.focus = at;
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
        rts.focus = rts
            .focus
            .clamp(Vec3::splat(-PAN_LIMIT), Vec3::splat(PAN_LIMIT));
        rts.focus.y = 0.0;
        let want = rts.transform();
        if *transform != want {
            *transform = want;
        }
    }
}
