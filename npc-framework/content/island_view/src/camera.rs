//! RTS camera (as the village's): WASD/arrows pan, mouse wheel zooms, Q/E
//! orbit, PageUp/PageDown tilt, F centres on the selected unit. On the
//! regional island the pitch also follows the zoom, the middle mouse button
//! orbits and tilts, and the cursor at a window edge pans.

use bevy::{
    input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollUnit},
    picking::hover::Hovered,
    prelude::*,
    window::PrimaryWindow,
};
use npc_framework::prelude::{SimIndex, SimPos};
use npc_framework::view_helpers::{ScreenshotRequest, Selected, ViewSystems, sim_to_world};

const MIN_DISTANCE: f32 = 5.0;
const MAX_DISTANCE: f32 = 80.0;
const MIN_PITCH: f32 = 0.35;
const MAX_PITCH: f32 = 1.45;
/// Pan limit around the compound, metres.
pub(crate) const PAN_LIMIT: f32 = 30.0;

/// Island pitch at settlement zoom: low enough to see doors and facades.
pub(crate) const ISLAND_NEAR_PITCH: f32 = 0.663;
/// Island pitch at the overview (58 degrees, the framing's own pitch).
pub(crate) const ISLAND_FAR_PITCH: f32 = 1.012_291;
const ISLAND_NEAR_DISTANCE: f32 = 30.0;
const ISLAND_FAR_DISTANCE: f32 = 240.0;
/// Radians per pixel of a middle-button drag.
const DRAG_YAW: f32 = 0.006;
const DRAG_PITCH: f32 = 0.004;
/// The cursor pans the island camera inside this band at a window edge.
const EDGE_PX: f32 = 14.0;

/// The pitch the island camera rests at for a zoom distance: 38 degrees up
/// to 30 m, easing to 58 degrees from 240 m outward.
pub(crate) fn island_pitch(distance: f32) -> f32 {
    let t = ((distance.max(1.0) / ISLAND_NEAR_DISTANCE).ln()
        / (ISLAND_FAR_DISTANCE / ISLAND_NEAR_DISTANCE).ln())
    .clamp(0.0, 1.0);
    let t = t * t * (3.0 - 2.0 * t);
    ISLAND_NEAR_PITCH + (ISLAND_FAR_PITCH - ISLAND_NEAR_PITCH) * t
}

/// The pitch after a zoom from `from` to `to` metres. The camera moves along
/// the curve; a tilt the player chose stays as an offset from it.
pub(crate) fn rezoomed_pitch(pitch: f32, from: f32, to: f32) -> f32 {
    (pitch + island_pitch(to) - island_pitch(from)).clamp(MIN_PITCH, MAX_PITCH)
}

/// Pan input (`-1..1` per axis, screen right and screen down positive) for a
/// cursor at `cursor` in a window of `size` logical pixels.
pub(crate) fn edge_pan(cursor: Vec2, size: Vec2) -> Vec2 {
    let axis = |at: f32, extent: f32| {
        if extent < EDGE_PX * 4.0 || at < 0.0 || at > extent {
            0.0
        } else if at < EDGE_PX {
            -(1.0 - at / EDGE_PX)
        } else if at > extent - EDGE_PX {
            1.0 - (extent - at) / EDGE_PX
        } else {
            0.0
        }
    };
    Vec2::new(axis(cursor.x, size.x), axis(cursor.y, size.y))
}

/// Startup switch for screen-edge panning: `ISLAND_VIEW_EDGE_PAN=off` turns
/// it off for the whole run, whatever the player's settings say.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct EdgePan(pub bool);

/// The player's preferences for the island camera. Host presentation data:
/// a host inserts or edits it (the Settings screen does); it never reaches
/// the simulation, a save or a replay.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct IslandCameraSettings {
    /// Pan when the cursor rests at a window edge, in Oversee and Shape.
    pub edge_pan: bool,
    /// Multiplier on keyboard and edge panning; 1 is the authored speed.
    pub pan_speed: f32,
}

impl Default for IslandCameraSettings {
    fn default() -> Self {
        Self {
            edge_pan: true,
            pan_speed: 1.0,
        }
    }
}

impl IslandCameraSettings {
    /// Slowest and fastest pan a host may ask for.
    pub const PAN_SPEED_RANGE: (f32, f32) = (0.5, 2.0);

    fn speed(self) -> f32 {
        let (low, high) = Self::PAN_SPEED_RANGE;
        if self.pan_speed.is_finite() {
            self.pan_speed.clamp(low, high)
        } else {
            1.0
        }
    }
}

/// Orbit camera state: looks at `focus` from `distance` at `pitch` above the
/// ground plane, rotated `yaw` around +Y (yaw 0 = camera on the +Z side).
/// Reflected so a capture script can place the view over the remote
/// protocol; it is view state and changing it changes no simulation result.
#[derive(Component, Reflect, Clone, Copy, Debug, PartialEq)]
#[reflect(Component)]
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

/// Actual camera input intent, outside simulation state. A serial distinguishes a fresh input
/// system run from stale state while modal/UI focus suppresses the camera's input set.
#[derive(Resource, Default)]
pub(crate) struct CameraManualIntent {
    pub serial: u64,
    pub active: bool,
}

pub(crate) fn plugin(app: &mut App) {
    app.insert_resource(EdgePan(
        std::env::var("ISLAND_VIEW_EDGE_PAN").as_deref() != Ok("off"),
    ));
    app.init_resource::<CameraManualIntent>()
        .init_resource::<IslandCameraSettings>()
        .register_type::<RtsCamera>()
        .add_systems(Startup, spawn_camera)
        .add_systems(
            Update,
            (focus_at_start, rts_camera)
                .chain()
                .in_set(ViewSystems::Input),
        );
}

/// A start pose that left the pitch at the testbed default takes the island
/// curve instead; an explicit pitch (`--camera X,Z,DIST,YAW,PITCH`) is kept.
pub(crate) fn island_start(mut start: RtsCamera) -> RtsCamera {
    if start.pitch == RtsCamera::default().pitch {
        start.pitch = island_pitch(start.distance);
    }
    start
}

fn spawn_camera(
    mut commands: Commands,
    start: Option<Res<CameraStart>>,
    regional: Option<Res<island_content::village::frontier::terrain::TerrainState>>,
) {
    let mut rts = start.map(|s| s.0).unwrap_or_default();
    if regional.is_some() {
        rts = island_start(rts);
    }
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
    mut manual: ResMut<CameraManualIntent>,
    mode: Option<Res<crate::IslandMode>>,
    experience: Option<Res<crate::PlayerExperience>>,
    terrain: Option<Res<island_content::village::terrain::TerrainState>>,
    regional: Option<Res<island_content::village::frontier::terrain::TerrainState>>,
    (mouse, motion): (
        Option<Res<ButtonInput<MouseButton>>>,
        Option<Res<AccumulatedMouseMotion>>,
    ),
    (edge, settings, capture, windows, hovered): (
        Option<Res<EdgePan>>,
        Option<Res<IslandCameraSettings>>,
        Option<Res<ScreenshotRequest>>,
        Query<&Window, With<PrimaryWindow>>,
        Query<&Hovered, With<Node>>,
    ),
) {
    let settings = settings.map(|settings| *settings).unwrap_or_default();
    // The preference applies to the island camera only.
    let pan_speed = if regional.is_some() {
        settings.speed()
    } else {
        1.0
    };
    let dt = time.delta_secs();
    let character = mode.as_deref() == Some(&crate::IslandMode::Character);
    let player = character && experience.is_some_and(|experience| experience.enabled);
    let focus_target = selected_pos(selected.as_deref(), index.as_deref(), &positions);
    // Island only. A capture run must not depend on where the mouse rests,
    // and a cursor over a panel is using that panel.
    let edge = if regional.is_some()
        && !character
        && capture.is_none()
        && edge.is_some_and(|edge| edge.0)
        && settings.edge_pan
        && !hovered.iter().any(Hovered::get)
    {
        windows
            .iter()
            .next()
            .filter(|window| window.focused)
            .and_then(|window| Some(edge_pan(window.cursor_position()?, window.size())))
            .unwrap_or(Vec2::ZERO)
    } else {
        Vec2::ZERO
    };
    let drag = if regional.is_some()
        && mouse
            .as_deref()
            .is_some_and(|mouse| mouse.pressed(MouseButton::Middle))
    {
        motion.as_deref().map_or(Vec2::ZERO, |motion| motion.delta)
    } else {
        Vec2::ZERO
    };
    manual.serial = manual.serial.saturating_add(1);
    manual.active = keys.as_deref().is_some_and(|keys| {
        [
            KeyCode::KeyW,
            KeyCode::KeyA,
            KeyCode::KeyS,
            KeyCode::KeyD,
            KeyCode::ArrowUp,
            KeyCode::ArrowLeft,
            KeyCode::ArrowDown,
            KeyCode::ArrowRight,
        ]
        .into_iter()
        .any(|key| {
            keys.pressed(key)
                && (!character
                    || matches!(
                        key,
                        KeyCode::ArrowUp
                            | KeyCode::ArrowLeft
                            | KeyCode::ArrowDown
                            | KeyCode::ArrowRight
                    ))
        }) || (keys.just_pressed(KeyCode::KeyF) && focus_target.is_some())
    }) || edge != Vec2::ZERO;
    for (mut rts, mut transform) in &mut cameras {
        if edge != Vec2::ZERO {
            let world = Quat::from_rotation_y(rts.yaw) * Vec3::new(edge.x, 0.0, edge.y);
            let speed = rts.distance * 0.8 * pan_speed;
            rts.focus += world * speed * dt;
        }
        if drag != Vec2::ZERO {
            rts.yaw -= drag.x * DRAG_YAW;
            rts.pitch = (rts.pitch + drag.y * DRAG_PITCH).clamp(MIN_PITCH, MAX_PITCH);
        }
        // A chord (Ctrl+S, Ctrl+Z) is a command, never a camera move.
        let chord = keys.as_deref().is_some_and(|keys| {
            keys.any_pressed([
                KeyCode::ControlLeft,
                KeyCode::ControlRight,
                KeyCode::AltLeft,
                KeyCode::AltRight,
                KeyCode::SuperLeft,
                KeyCode::SuperRight,
            ])
        });
        if let Some(keys) = keys.as_deref().filter(|_| !chord) {
            let pressed =
                |a: KeyCode, b: KeyCode| (!character && keys.pressed(a)) || keys.pressed(b);
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
                let speed = rts.distance * 0.8 * pan_speed;
                rts.focus += world * speed * dt;
            }
            if keys.pressed(KeyCode::KeyQ) {
                rts.yaw -= 1.5 * dt;
            }
            if keys.pressed(if player { KeyCode::KeyR } else { KeyCode::KeyE }) {
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
                let before = rts.distance;
                rts.distance = (rts.distance * 0.9f32.powf(lines)).clamp(
                    MIN_DISTANCE,
                    if regional.is_some() {
                        600.0
                    } else if terrain.is_some() {
                        180.0
                    } else {
                        MAX_DISTANCE
                    },
                );
                if regional.is_some() {
                    rts.pitch = rezoomed_pitch(rts.pitch, before, rts.distance);
                }
            }
        }
        let pan_limit = regional.as_ref().map_or(
            if terrain.is_some() { 90.0 } else { PAN_LIMIT },
            |terrain| terrain.width.max(terrain.height) as f32 * 0.5 + 20.0,
        );
        rts.focus = rts
            .focus
            .clamp(Vec3::splat(-pan_limit), Vec3::splat(pan_limit));
        if regional.is_none() {
            rts.focus.y = 0.0;
        }
        let want = rts.transform();
        if *transform != want {
            *transform = want;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn island_pitch_shows_facades_up_close_and_steepens_smoothly_to_the_overview() {
        // Close and settlement zoom sit between 35 and 45 degrees.
        for distance in [5.0_f32, 14.0, 26.0, 40.0, 62.0] {
            let degrees = island_pitch(distance).to_degrees();
            assert!((35.0..=45.0).contains(&degrees), "{distance} m: {degrees}");
        }
        assert!((island_pitch(5.0).to_degrees() - 38.0).abs() < 0.1);
        // The overview keeps the framing's 58 degrees.
        for distance in [240.0_f32, 330.0, 600.0] {
            assert!((island_pitch(distance).to_degrees() - 58.0).abs() < 0.01);
        }
        assert!((ISLAND_FAR_PITCH - 58_f32.to_radians()).abs() < 1e-6);
        // Never decreasing, and no wheel notch (10 % of distance) jumps.
        let mut distance = 5.0_f32;
        let mut previous = island_pitch(distance);
        while distance < 600.0 {
            distance /= 0.9;
            let pitch = island_pitch(distance);
            assert!(pitch >= previous, "pitch falls at {distance} m");
            assert!(
                (pitch - previous).to_degrees() < 1.7,
                "pitch jumps at {distance} m"
            );
            previous = pitch;
        }
    }

    #[test]
    fn zooming_keeps_a_chosen_tilt_and_stays_inside_the_limits() {
        // On the curve, a zoom lands on the curve.
        let pitch = rezoomed_pitch(island_pitch(330.0), 330.0, 40.0);
        assert!((pitch - island_pitch(40.0)).abs() < 1e-6);
        // Five degrees of extra tilt survive the same zoom.
        let extra = 5_f32.to_radians();
        let pitch = rezoomed_pitch(island_pitch(330.0) + extra, 330.0, 40.0);
        assert!((pitch - island_pitch(40.0) - extra).abs() < 1e-6);
        // There and back is where it started.
        let back = rezoomed_pitch(rezoomed_pitch(0.9, 80.0, 20.0), 20.0, 80.0);
        assert!((back - 0.9).abs() < 1e-6);
        assert_eq!(rezoomed_pitch(MIN_PITCH, 600.0, 5.0), MIN_PITCH);
        assert_eq!(rezoomed_pitch(MAX_PITCH, 5.0, 600.0), MAX_PITCH);
    }

    #[test]
    fn only_an_unspecified_start_pitch_takes_the_island_curve() {
        let unspecified = RtsCamera {
            distance: 62.0,
            ..default()
        };
        assert_eq!(island_start(unspecified).pitch, island_pitch(62.0));
        let explicit = RtsCamera {
            distance: 62.0,
            pitch: 70_f32.to_radians(),
            ..default()
        };
        assert_eq!(island_start(explicit), explicit);
    }

    #[test]
    fn the_cursor_pans_only_inside_the_edge_band() {
        let size = Vec2::new(1280.0, 720.0);
        assert_eq!(edge_pan(Vec2::new(640.0, 360.0), size), Vec2::ZERO);
        assert_eq!(edge_pan(Vec2::new(30.0, 690.0), size), Vec2::ZERO);
        assert_eq!(edge_pan(Vec2::new(0.0, 360.0), size), Vec2::new(-1.0, 0.0));
        assert_eq!(
            edge_pan(Vec2::new(1280.0, 360.0), size),
            Vec2::new(1.0, 0.0)
        );
        assert_eq!(edge_pan(Vec2::new(640.0, 0.0), size), Vec2::new(0.0, -1.0));
        let corner = edge_pan(Vec2::new(1273.0, 713.0), size);
        assert!((corner - Vec2::splat(0.5)).abs().max_element() < 1e-4);
        // Outside the window, or a window too small to have a middle: nothing.
        assert_eq!(edge_pan(Vec2::new(-4.0, 360.0), size), Vec2::ZERO);
        assert_eq!(edge_pan(Vec2::new(5.0, 5.0), Vec2::splat(40.0)), Vec2::ZERO);
    }

    fn island_app() -> (App, Entity) {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<CameraManualIntent>()
            .init_resource::<ButtonInput<MouseButton>>()
            .init_resource::<AccumulatedMouseMotion>()
            .init_resource::<AccumulatedMouseScroll>()
            .insert_resource(EdgePan(true))
            .insert_resource(island_content::village::frontier::terrain::TerrainState {
                width: 256,
                height: 256,
                cells: Vec::new(),
                revision: 0,
            })
            .add_systems(Update, rts_camera);
        let start = RtsCamera {
            focus: Vec3::new(10.0, 0.0, 10.0),
            yaw: 0.0,
            pitch: island_pitch(60.0),
            distance: 60.0,
        };
        let camera = app.world_mut().spawn((start, start.transform())).id();
        (app, camera)
    }

    #[test]
    fn the_wheel_moves_the_island_camera_along_the_pitch_curve() {
        let (mut app, camera) = island_app();
        app.world_mut()
            .resource_mut::<AccumulatedMouseScroll>()
            .delta = Vec2::new(0.0, 4.0);
        app.update();
        let pose = *app.world().get::<RtsCamera>(camera).unwrap();
        assert!(pose.distance < 40.0);
        assert!((pose.pitch - island_pitch(pose.distance)).abs() < 1e-5);
        assert!(pose.pitch < island_pitch(60.0));
        assert_eq!(pose.focus, Vec3::new(10.0, 0.0, 10.0));
    }

    #[test]
    fn a_middle_button_drag_orbits_and_tilts_without_moving_the_focus() {
        let (mut app, camera) = island_app();
        let before = *app.world().get::<RtsCamera>(camera).unwrap();
        // Motion without the button is ignored.
        app.world_mut()
            .resource_mut::<AccumulatedMouseMotion>()
            .delta = Vec2::new(50.0, 25.0);
        app.update();
        assert_eq!(*app.world().get::<RtsCamera>(camera).unwrap(), before);
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Middle);
        app.world_mut()
            .resource_mut::<AccumulatedMouseMotion>()
            .delta = Vec2::new(50.0, 25.0);
        app.update();
        let after = *app.world().get::<RtsCamera>(camera).unwrap();
        assert!((after.yaw - (before.yaw - 50.0 * DRAG_YAW)).abs() < 1e-5);
        assert!((after.pitch - (before.pitch + 25.0 * DRAG_PITCH)).abs() < 1e-5);
        assert_eq!(after.focus, before.focus);
        assert_eq!(after.distance, before.distance);
        assert!(!app.world().resource::<CameraManualIntent>().active);
        // A huge drag stops at the tilt limits.
        app.world_mut()
            .resource_mut::<AccumulatedMouseMotion>()
            .delta = Vec2::new(0.0, 9000.0);
        app.update();
        assert_eq!(
            app.world().get::<RtsCamera>(camera).unwrap().pitch,
            MAX_PITCH
        );
    }

    fn edge_app(settings: IslandCameraSettings) -> (App, Entity) {
        let (mut app, camera) = island_app();
        app.insert_resource(settings);
        let mut window = Window {
            focused: true,
            ..default()
        };
        window.resolution.set(1280.0, 720.0);
        // At the right-hand edge, halfway down.
        window.set_cursor_position(Some(Vec2::new(1279.0, 360.0)));
        app.world_mut().spawn((window, PrimaryWindow));
        (app, camera)
    }

    fn travel(app: &mut App, camera: Entity) -> f32 {
        let before = app.world().get::<RtsCamera>(camera).unwrap().focus;
        // Two updates: the first frame of a test app has no elapsed time.
        app.update();
        std::thread::sleep(std::time::Duration::from_millis(20));
        app.update();
        app.world().get::<RtsCamera>(camera).unwrap().focus.x - before.x
    }

    #[test]
    fn the_edge_pan_setting_switches_it_off_and_pan_speed_scales_it() {
        let (mut on, camera) = edge_app(IslandCameraSettings::default());
        assert!(travel(&mut on, camera) > 0.0, "the cursor at the edge pans");
        assert!(on.world().resource::<CameraManualIntent>().active);

        let (mut off, camera) = edge_app(IslandCameraSettings {
            edge_pan: false,
            ..default()
        });
        assert_eq!(travel(&mut off, camera), 0.0);
        assert!(!off.world().resource::<CameraManualIntent>().active);

        // The startup switch wins over the setting.
        let (mut forced, camera) = edge_app(IslandCameraSettings::default());
        forced.insert_resource(EdgePan(false));
        assert_eq!(travel(&mut forced, camera), 0.0);

        // Out-of-range and broken speeds are held to the allowed range.
        let speed = |pan_speed: f32| {
            IslandCameraSettings {
                pan_speed,
                ..default()
            }
            .speed()
        };
        assert_eq!(speed(1.5), 1.5);
        assert_eq!(speed(0.01), 0.5);
        assert_eq!(speed(40.0), 2.0);
        assert_eq!(speed(f32::NAN), 1.0);
        assert_eq!(IslandCameraSettings::default().speed(), 1.0);
    }

    #[test]
    fn other_scenes_keep_their_fixed_pitch_and_ignore_the_mouse_orbit() {
        let (mut app, camera) = island_app();
        app.world_mut()
            .remove_resource::<island_content::village::frontier::terrain::TerrainState>();
        let before = *app.world().get::<RtsCamera>(camera).unwrap();
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Middle);
        app.world_mut()
            .resource_mut::<AccumulatedMouseMotion>()
            .delta = Vec2::new(50.0, 25.0);
        app.world_mut()
            .resource_mut::<AccumulatedMouseScroll>()
            .delta = Vec2::new(0.0, 4.0);
        app.update();
        let after = *app.world().get::<RtsCamera>(camera).unwrap();
        assert!(after.distance < before.distance);
        assert_eq!(after.pitch, before.pitch);
        assert_eq!(after.yaw, before.yaw);
    }
}
