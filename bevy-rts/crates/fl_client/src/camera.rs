//! Isometric RTS camera: orthographic rig with pan, edge-pan, zoom and rotation.
use std::f32::consts::{FRAC_PI_4, TAU};

use bevy::camera::ScalingMode;
use bevy::input::mouse::{AccumulatedMouseScroll, MouseScrollUnit};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::map_view::MapView;
use crate::selection::InputGate;

/// True isometric pitch (atan(1/sqrt 2)) in radians.
const ISO_PITCH: f32 = 35.264_f32 * std::f32::consts::PI / 180.0;

/// Orthographic near plane, metres.
pub const CAMERA_NEAR: f32 = 0.1;
/// Orthographic far plane, metres.
pub const CAMERA_FAR: f32 = 2000.0;
/// Clearance kept between the camera plane and the nearest visible ground, metres.
const NEAR_CLEARANCE: f32 = 50.0;
/// Pixel scroll units per line unit (matches Bevy's `MouseScrollPixelsPerLine` default).
const PIXELS_PER_LINE: f32 = 100.0;

/// Orbit-style rig around a ground focus point.
#[derive(Component, Clone, Debug)]
pub struct RtsCamera {
    /// Ground point the camera looks at.
    pub focus: Vec3,
    /// Rotation about the vertical axis, radians.
    pub yaw: f32,
    /// Downward tilt, radians.
    pub pitch: f32,
    /// Distance from focus along the view axis (orthographic: only clipping matters).
    pub distance: f32,
    /// Visible world height in metres (the zoom level).
    pub view_height: f32,
}

impl Default for RtsCamera {
    fn default() -> Self {
        Self {
            focus: Vec3::ZERO,
            yaw: FRAC_PI_4,
            pitch: ISO_PITCH,
            distance: 400.0,
            view_height: 120.0,
        }
    }
}

/// Tunables for the camera controls.
#[derive(Resource, Clone, Debug)]
pub struct CameraSettings {
    /// Pan speed in view-heights per second.
    pub pan_speed: f32,
    /// Width of the screen-edge pan zone in pixels.
    pub edge_margin_px: f32,
    /// Smallest view height (most zoomed in).
    pub zoom_min: f32,
    /// Largest view height (most zoomed out).
    pub zoom_max: f32,
    /// Fractional view-height change per scroll unit.
    pub zoom_step: f32,
    /// Free rotation speed, radians per second.
    pub rotate_speed: f32,
}

impl Default for CameraSettings {
    fn default() -> Self {
        Self {
            pan_speed: 1.0,
            edge_margin_px: 8.0,
            zoom_min: 15.0,
            zoom_max: 600.0,
            zoom_step: 0.1,
            rotate_speed: 90.0_f32.to_radians(),
        }
    }
}

/// View-space depths (from the camera plane) of the nearest and farthest visible ground.
///
/// In an orthographic view the screen's vertical half extent `view_height / 2` maps to a
/// ground offset of `view_height / (2 tan pitch)` along the horizontal view direction, which
/// shifts the depth by the same amount either side of `distance` (the focus depth).
pub fn ground_depth_range(r: &RtsCamera) -> (f32, f32) {
    let half = r.view_height / (2.0 * r.pitch.tan());
    (r.distance - half, r.distance + half)
}

/// Rig distance that keeps the whole visible ground between the near and far planes.
///
/// The ortho near plane is fixed, so the camera is pulled back as the view grows instead:
/// `distance = max(base, half_depth + NEAR_CLEARANCE)`.
pub fn fit_distance(view_height: f32, pitch: f32, base: f32) -> f32 {
    base.max(view_height / (2.0 * pitch.tan()) + NEAR_CLEARANCE)
}

/// Scroll amount in line units, whatever the device reports.
pub fn scroll_lines(delta: f32, unit: MouseScrollUnit) -> f32 {
    match unit {
        MouseScrollUnit::Line => delta,
        MouseScrollUnit::Pixel => delta / PIXELS_PER_LINE,
    }
}

/// Camera transform for a rig: placed behind/above the focus, looking at it.
pub fn rig_transform(r: &RtsCamera) -> Transform {
    let rot = Quat::from_euler(EulerRot::YXZ, r.yaw, -r.pitch, 0.0);
    Transform::from_translation(r.focus + rot * Vec3::new(0.0, 0.0, r.distance))
        .looking_at(r.focus, Vec3::Y)
}

/// WASD and arrow keys as (x right, y forward), each axis in -1..=1.
pub fn pan_input(keys: &ButtonInput<KeyCode>) -> Vec2 {
    let axis = |pos: &[KeyCode], neg: &[KeyCode]| {
        let p = pos.iter().any(|k| keys.pressed(*k));
        let n = neg.iter().any(|k| keys.pressed(*k));
        f32::from(p) - f32::from(n)
    };
    Vec2::new(
        axis(
            &[KeyCode::KeyD, KeyCode::ArrowRight],
            &[KeyCode::KeyA, KeyCode::ArrowLeft],
        ),
        axis(
            &[KeyCode::KeyW, KeyCode::ArrowUp],
            &[KeyCode::KeyS, KeyCode::ArrowDown],
        ),
    )
}

/// Screen-edge pan as (x right, y forward); zero without a cursor or window.
pub fn edge_pan_input(cursor: Option<Vec2>, window: Vec2, margin: f32) -> Vec2 {
    let Some(c) = cursor else {
        return Vec2::ZERO;
    };
    if window.x <= 0.0 || window.y <= 0.0 {
        return Vec2::ZERO;
    }
    let mut v = Vec2::ZERO;
    if c.x < margin {
        v.x -= 1.0;
    }
    if c.x > window.x - margin {
        v.x += 1.0;
    }
    // Screen y grows downward; the top edge pans forward.
    if c.y < margin {
        v.y += 1.0;
    }
    if c.y > window.y - margin {
        v.y -= 1.0;
    }
    v
}

/// World-space focus movement for a pan input at the given yaw.
pub fn pan_delta(input: Vec2, yaw: f32, view_height: f32, speed: f32, dt: f32) -> Vec3 {
    let input = input.clamp_length_max(1.0);
    let rot = Quat::from_rotation_y(yaw);
    (rot * Vec3::X * input.x + rot * Vec3::NEG_Z * input.y) * view_height * speed * dt
}

/// Keeps the focus on the map (sim (x, y) maps to Bevy (x, 0, z)).
pub fn clamp_focus(focus: Vec3, size_m: Vec2) -> Vec3 {
    Vec3::new(
        focus.x.clamp(0.0, size_m.x.max(0.0)),
        focus.y,
        focus.z.clamp(0.0, size_m.y.max(0.0)),
    )
}

/// New view height after `scroll` wheel units (positive zooms in), clamped to settings.
pub fn zoom(view_height: f32, scroll: f32, s: &CameraSettings) -> f32 {
    // Bound the per-call factor so a huge delta cannot make the height zero or negative.
    let factor = (1.0 - scroll * s.zoom_step).clamp(0.1, 10.0);
    (view_height * factor).clamp(s.zoom_min, s.zoom_max)
}

/// Nearest multiple of 45 degrees, wrapped to [0, 2pi).
pub fn snap_yaw(yaw: f32) -> f32 {
    ((yaw / FRAC_PI_4).round() * FRAC_PI_4).rem_euclid(TAU)
}

/// Spawns the camera and installs the control systems.
pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CameraSettings>()
            .add_systems(Startup, spawn_camera)
            .add_systems(
                Update,
                (
                    pan_camera,
                    zoom_camera,
                    rotate_camera,
                    clamp_camera,
                    apply_rig,
                )
                    .chain(),
            );
    }
}

fn spawn_camera(mut commands: Commands, map: Option<Res<MapView>>) {
    let focus = map.map_or(Vec3::ZERO, |m| {
        Vec3::new(m.size_m.x * 0.5, 0.0, m.size_m.y * 0.5)
    });
    let mut rig = RtsCamera { focus, ..default() };
    rig.distance = fit_distance(rig.view_height, rig.pitch, rig.distance);
    commands.spawn((
        Camera3d::default(),
        Projection::from(OrthographicProjection {
            scaling_mode: ScalingMode::FixedVertical {
                viewport_height: rig.view_height,
            },
            near: CAMERA_NEAR,
            far: CAMERA_FAR,
            ..OrthographicProjection::default_3d()
        }),
        rig_transform(&rig),
        rig,
    ));
}

fn pan_camera(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    gate: InputGate,
    settings: Res<CameraSettings>,
    window: Option<Single<&Window, With<PrimaryWindow>>>,
    mut rigs: Query<&mut RtsCamera>,
) {
    let mut input = if gate.keyboard_blocked() {
        Vec2::ZERO
    } else {
        pan_input(&keys)
    };
    if let Some(w) = window {
        input += edge_pan_input(w.cursor_position(), w.size(), settings.edge_margin_px);
    }
    if input == Vec2::ZERO {
        return;
    }
    for mut rig in &mut rigs {
        let delta = pan_delta(
            input,
            rig.yaw,
            rig.view_height,
            settings.pan_speed,
            time.delta_secs(),
        );
        rig.focus += delta;
    }
}

fn zoom_camera(
    scroll: Option<Res<AccumulatedMouseScroll>>,
    settings: Res<CameraSettings>,
    mut rigs: Query<&mut RtsCamera>,
) {
    let Some(scroll) = scroll else {
        return;
    };
    let base_distance = RtsCamera::default().distance;
    if scroll.delta.y == 0.0 {
        return;
    }
    let lines = scroll_lines(scroll.delta.y, scroll.unit);
    for mut rig in &mut rigs {
        rig.view_height = zoom(rig.view_height, lines, &settings);
        rig.distance = fit_distance(rig.view_height, rig.pitch, base_distance);
    }
}

fn rotate_camera(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    gate: InputGate,
    settings: Res<CameraSettings>,
    mut rigs: Query<&mut RtsCamera>,
) {
    if gate.keyboard_blocked() {
        return;
    }
    let dir = f32::from(keys.pressed(KeyCode::KeyE)) - f32::from(keys.pressed(KeyCode::KeyQ));
    let snap = keys.just_pressed(KeyCode::Home);
    if dir == 0.0 && !snap {
        return;
    }
    for mut rig in &mut rigs {
        if snap {
            rig.yaw = snap_yaw(rig.yaw);
        }
        rig.yaw = (rig.yaw + dir * settings.rotate_speed * time.delta_secs()).rem_euclid(TAU);
    }
}

fn clamp_camera(map: Option<Res<MapView>>, mut rigs: Query<&mut RtsCamera>) {
    let Some(map) = map else {
        return;
    };
    for mut rig in &mut rigs {
        let clamped = clamp_focus(rig.focus, map.size_m);
        if clamped != rig.focus {
            rig.focus = clamped;
        }
    }
}

fn apply_rig(mut cams: Query<(&RtsCamera, &mut Transform, &mut Projection), Changed<RtsCamera>>) {
    for (rig, mut tf, mut proj) in &mut cams {
        *tf = rig_transform(rig);
        if let Projection::Orthographic(o) = &mut *proj {
            o.scaling_mode = ScalingMode::FixedVertical {
                viewport_height: rig.view_height,
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use bevy::time::TimeUpdateStrategy;

    use super::*;

    #[test]
    fn pan_forward_moves_along_yaw() {
        let d = pan_delta(Vec2::new(0.0, 1.0), 0.0, 120.0, 1.0, 0.1);
        assert!(d.z < 0.0);
        assert!(d.x.abs() < 1e-5);
    }

    #[test]
    fn edge_pan_left_edge() {
        let v = edge_pan_input(Some(Vec2::new(2.0, 300.0)), Vec2::new(800.0, 600.0), 8.0);
        assert_eq!(v.x, -1.0);
        assert_eq!(v.y, 0.0);
    }

    #[test]
    fn edge_pan_zero_size_window() {
        let v = edge_pan_input(Some(Vec2::new(2.0, 2.0)), Vec2::ZERO, 8.0);
        assert_eq!(v, Vec2::ZERO);
        assert_eq!(
            edge_pan_input(None, Vec2::new(800.0, 600.0), 8.0),
            Vec2::ZERO
        );
    }

    #[test]
    fn zoom_is_clamped() {
        let s = CameraSettings::default();
        let (mut a, mut b) = (120.0, 120.0);
        for _ in 0..10_000 {
            a = zoom(a, 1.0, &s);
            b = zoom(b, -1.0, &s);
            assert!((s.zoom_min..=s.zoom_max).contains(&a));
            assert!((s.zoom_min..=s.zoom_max).contains(&b));
        }
        assert_eq!(a, s.zoom_min);
        assert_eq!(b, s.zoom_max);
    }

    #[test]
    fn clamp_focus_inside_map() {
        let size = Vec2::new(1000.0, 500.0);
        assert_eq!(
            clamp_focus(Vec3::new(-5.0, 3.0, 900.0), size),
            Vec3::new(0.0, 3.0, 500.0)
        );
        assert_eq!(
            clamp_focus(Vec3::new(10.0, 0.0, 20.0), size),
            Vec3::new(10.0, 0.0, 20.0)
        );
    }

    #[test]
    fn snap_yaw_nearest_45() {
        assert!(snap_yaw(0.3).abs() < 1e-6);
        assert!((snap_yaw(0.5) - FRAC_PI_4).abs() < 1e-6);
        let neg = snap_yaw(-0.1);
        assert!(neg < 1e-5 || (neg - TAU).abs() < 1e-5);
    }

    #[test]
    fn rig_looks_at_focus() {
        let r = RtsCamera {
            focus: Vec3::new(50.0, 0.0, 70.0),
            ..default()
        };
        let t = rig_transform(&r);
        let want = (r.focus - t.translation).normalize();
        assert!((*t.forward() - want).length() < 1e-4);
    }

    /// An app with the camera plugin, yaw 0.3 rad, and `key` pressed for one frame.
    fn yaw_after(key: KeyCode, console_open: bool) -> f32 {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(ButtonInput::<KeyCode>::default())
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::ZERO))
            .add_plugins(CameraPlugin);
        app.init_resource::<crate::debug::ConsoleState>();
        app.world_mut()
            .resource_mut::<crate::debug::ConsoleState>()
            .open = console_open;
        app.update();
        for mut rig in app
            .world_mut()
            .query::<&mut RtsCamera>()
            .iter_mut(app.world_mut())
        {
            rig.yaw = 0.3;
        }
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(key);
        app.update();
        app.world_mut()
            .query::<&RtsCamera>()
            .single(app.world())
            .unwrap()
            .yaw
    }

    #[test]
    fn home_snaps_yaw_and_r_does_not() {
        assert!(yaw_after(KeyCode::Home, false).abs() < 1e-6);
        assert!((yaw_after(KeyCode::KeyR, false) - 0.3).abs() < 1e-6);
    }

    #[test]
    fn camera_keys_ignored_while_console_open() {
        assert!((yaw_after(KeyCode::Home, true) - 0.3).abs() < 1e-6);
    }

    #[test]
    fn wasd_moves_focus() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(ButtonInput::<KeyCode>::default())
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
                100,
            )))
            .add_plugins(CameraPlugin);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyW);
        for _ in 0..3 {
            app.update();
        }
        let rig = app
            .world_mut()
            .query::<&RtsCamera>()
            .single(app.world())
            .unwrap()
            .clone();
        // Default yaw is 45 degrees: forward is (-sin, 0, -cos) = (-1, 0, -1) / sqrt 2.
        let dir = rig.focus.with_y(0.0).normalize();
        assert!(rig.focus.x < 0.0 && rig.focus.z < 0.0, "{:?}", rig.focus);
        assert!((dir - Vec3::new(-1.0, 0.0, -1.0).normalize()).length() < 1e-4);
        // 3 updates, first has dt 0 or 100 ms: moved by at most speed * view_height * 0.3 s.
        assert!(rig.focus.length() <= 120.0 * 0.3 + 1e-3);
    }

    #[test]
    fn max_zoom_ground_is_inside_clip_range() {
        let s = CameraSettings::default();
        let mut r = RtsCamera {
            view_height: s.zoom_max,
            ..default()
        };
        // The stock distance (400) is not enough at zoom_max; the fitted one is.
        r.distance = fit_distance(r.view_height, r.pitch, RtsCamera::default().distance);
        let (near, far) = ground_depth_range(&r);
        assert!(near > CAMERA_NEAR, "near {near}");
        assert!(far < CAMERA_FAR, "far {far}");
        // And the check is meaningful: the unfitted rig would clip.
        let stock = RtsCamera {
            view_height: s.zoom_max,
            ..default()
        };
        assert!(ground_depth_range(&stock).0 <= CAMERA_NEAR);
    }

    #[test]
    fn every_zoom_level_is_inside_clip_range() {
        let s = CameraSettings::default();
        let mut vh = s.zoom_min;
        while vh <= s.zoom_max {
            let r = RtsCamera {
                view_height: vh,
                distance: fit_distance(vh, ISO_PITCH, 400.0),
                ..default()
            };
            let (near, far) = ground_depth_range(&r);
            assert!(near > CAMERA_NEAR && far < CAMERA_FAR, "vh {vh}");
            vh += 5.0;
        }
    }

    #[test]
    fn pixel_scroll_is_normalised_and_zoom_stays_positive() {
        let s = CameraSettings::default();
        assert_eq!(scroll_lines(250.0, MouseScrollUnit::Pixel), 2.5);
        assert_eq!(scroll_lines(3.0, MouseScrollUnit::Line), 3.0);
        for raw in [1.0e6_f32, -1.0e6, 5000.0, -5000.0] {
            let lines = scroll_lines(raw, MouseScrollUnit::Pixel);
            let z = zoom(120.0, lines, &s);
            assert!(
                z.is_finite() && z >= s.zoom_min && z <= s.zoom_max,
                "{raw} -> {z}"
            );
        }
        // A huge raw (un-normalised) value must not drive the height to <= 0 pre-clamp.
        assert_eq!(zoom(120.0, 1.0e6, &s), s.zoom_min);
        assert_eq!(zoom(120.0, -1.0e6, &s), s.zoom_max);
    }
}
