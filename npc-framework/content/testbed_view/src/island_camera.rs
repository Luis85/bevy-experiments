//! Frame the island in the space left by native controls, without changing simulation state.
use crate::{IslandMode, RtsCamera};
use bevy::prelude::*;
use npc_framework::view_helpers::ViewSystems;

#[derive(Resource)]
pub(crate) struct AutoFrame;

#[derive(Clone, Copy)]
enum Target {
    Overview,
    Point(Vec3, f32),
}
#[derive(Resource, Default)]
struct Framing {
    target: Option<Target>,
    frames: u8,
    viewport: Option<Vec2>,
    panel: Option<Rect>,
    manual_serial: u64,
}

pub(crate) fn plugin(app: &mut App) {
    if !testbed_content::village::colony::enabled(app.world()) {
        return;
    }
    app.init_resource::<Framing>().add_systems(
        Update,
        frame
            .in_set(ViewSystems::Present)
            .after(crate::colony_panel::refresh)
            .before(crate::village_world::VillageProjectionSet),
    );
}
pub(crate) fn overview(world: &mut World, initial: bool) {
    if initial && !world.contains_resource::<AutoFrame>() {
        return;
    }
    let mut framing = world.get_resource_or_init::<Framing>();
    framing.target = Some(Target::Overview);
    framing.frames = 2;
}
pub(crate) fn focus(world: &mut World, target: Vec3, distance: f32) {
    let mut framing = world.get_resource_or_init::<Framing>();
    framing.target = Some(Target::Point(target, distance));
    framing.frames = 2;
}
pub(crate) fn character(world: &mut World) {
    for (mut camera, mut transform) in world
        .query::<(&mut RtsCamera, &mut Transform)>()
        .iter_mut(world)
    {
        camera.distance = 26.0;
        *transform = camera.transform();
    }
}

fn available_area(viewport: Rect, panel: Option<Rect>) -> Rect {
    let mut area = Rect::new(
        viewport.min.x + 20.0,
        viewport.min.y + 20.0,
        viewport.max.x - 20.0,
        viewport.max.y - 58.0,
    );
    if let Some(panel) = panel {
        area.max.x = (panel.min.x - 18.0).max(area.min.x + 100.0);
    }
    area
}

/// A translated ground-plane ray centers the target at the usable screen point.
fn frame_target(
    camera: &Camera,
    mut pose: RtsCamera,
    target: Vec3,
    area: Rect,
    fit: bool,
) -> Option<RtsCamera> {
    for _ in 0..12 {
        pose.focus = crate::village_world::framed_focus(camera, &pose, target, area.center())?;
        if !fit {
            return Some(pose);
        }
        let transform = GlobalTransform::from(pose.transform());
        let mut ratio = 0.0_f32;
        for point in [
            Vec3::new(-30., 0., -25.),
            Vec3::new(30., 0., -25.),
            Vec3::new(-30., 0., 25.),
            Vec3::new(30., 0., 25.),
            Vec3::new(0., 6., 0.),
        ] {
            let screen = camera.world_to_viewport(&transform, point).ok()?;
            let offset = (screen - area.center()).abs();
            ratio = ratio
                .max(offset.x / (area.width() * 0.5))
                .max(offset.y / (area.height() * 0.5));
        }
        if ratio <= 0.97 {
            return Some(pose);
        }
        pose.distance = (pose.distance * ratio * 1.04).min(180.0);
    }
    Some(pose)
}

fn frame(
    mode: Res<IslandMode>,
    mut state: ResMut<Framing>,
    mut cameras: Query<(&Camera, &mut RtsCamera, &mut Transform)>,
    panels: Query<
        (&Node, &ComputedNode, &UiGlobalTransform),
        With<crate::colony_panel::LedgerRoot>,
    >,
    manual: Option<Res<crate::camera::CameraManualIntent>>,
) {
    if *mode == IslandMode::Character {
        return;
    }
    if let Some(manual) = manual {
        if manual.serial != state.manual_serial && manual.active {
            state.target = None;
        }
        state.manual_serial = manual.serial;
    }
    let Some((camera, mut pose, mut transform)) = cameras.iter_mut().next() else {
        return;
    };
    let Some(viewport) = camera.logical_viewport_rect() else {
        return;
    };
    let panel = panels
        .iter()
        .find(|(node, size, _)| node.display != Display::None && size.size().min_element() > 0.0)
        .map(|(_, size, transform)| {
            Rect::from_center_size(
                transform.translation * size.inverse_scale_factor(),
                size.size() * size.inverse_scale_factor(),
            )
        });
    let changed = state.viewport != Some(viewport.size()) || state.panel != panel;
    if changed && state.target.is_some() {
        state.frames = 2;
    }
    state.viewport = Some(viewport.size());
    state.panel = panel;
    if state.frames == 0 {
        return;
    }
    state.frames -= 1;
    if state.frames > 0 {
        return;
    }
    let Some(target) = state.target else { return };
    let area = available_area(viewport, panel);
    let (base, at, fit) = match target {
        Target::Overview => (
            RtsCamera {
                focus: Vec3::ZERO,
                distance: 68.0,
                pitch: 58_f32.to_radians(),
                yaw: std::f32::consts::PI,
            },
            Vec3::ZERO,
            true,
        ),
        Target::Point(at, distance) => (
            RtsCamera {
                focus: at,
                distance,
                ..*pose
            },
            at,
            false,
        ),
    };
    if let Some(framed) = frame_target(camera, base, at, area, fit) {
        *pose = framed;
        *transform = pose.transform();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::camera::{CameraProjection, ComputedCameraValues, RenderTargetInfo};
    #[test]
    fn overview_bounds_fit_beside_the_ledger_at_both_capture_sizes() {
        for (width, height, panel_width) in [(1280, 720, 384.), (1024, 768, 352.)] {
            let projection = PerspectiveProjection {
                aspect_ratio: width as f32 / height as f32,
                ..default()
            };
            let camera = Camera {
                computed: ComputedCameraValues {
                    clip_from_view: projection.get_clip_from_view(),
                    target_info: Some(RenderTargetInfo {
                        physical_size: UVec2::new(width, height),
                        scale_factor: 1.0,
                    }),
                    ..default()
                },
                ..default()
            };
            let viewport = Rect::new(0., 0., width as f32, height as f32);
            let area = available_area(
                viewport,
                Some(Rect::new(
                    width as f32 - panel_width - 12.,
                    12.,
                    width as f32 - 12.,
                    height as f32 - 12.,
                )),
            );
            let pose = frame_target(
                &camera,
                RtsCamera {
                    focus: Vec3::ZERO,
                    distance: 68.,
                    yaw: std::f32::consts::PI,
                    pitch: 58_f32.to_radians(),
                },
                Vec3::ZERO,
                area,
                true,
            )
            .unwrap();
            for point in [
                Vec3::new(-30., 0., -25.),
                Vec3::new(30., 0., -25.),
                Vec3::new(-30., 0., 25.),
                Vec3::new(30., 0., 25.),
            ] {
                let pixel = camera
                    .world_to_viewport(&GlobalTransform::from(pose.transform()), point)
                    .unwrap();
                assert!(
                    area.contains(pixel),
                    "{width}x{height}: {pixel:?} outside {area:?}"
                );
            }
        }
    }
}
