//! Left-click selection of a unit. Selection is view-only: it lives in
//! [`Selected`] (a `SimId`) and never touches the sim. Content marks the
//! *visual* roots of selectable units with [`Selectable`]. Sim entities in
//! [`ViewHidden`] cannot be picked, and a selected one is deselected.

use bevy::{color::palettes::css, prelude::*, window::PrimaryWindow};
use npc_kernel::{SimId, SimIndex, SimPos};

use crate::visuals::{ViewHidden, VisualIndex, VisualOf, hidden};
use crate::{ViewSystems, sim_to_world};

/// The selected unit, if any. Overlays mark it and the journal panel
/// filters to it.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Selected(pub Option<SimId>);

/// Marker on a visual root (with [`VisualOf`]) whose sim entity can be
/// selected by clicking.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Selectable;

/// A click picks the unit whose body (a point this high above its feet,
/// metres) passes within [`PICK_RADIUS`] of the cursor ray.
pub const PICK_HEIGHT: f32 = 0.7;
/// How far from the cursor ray a unit's body may be to be picked, metres.
pub const PICK_RADIUS: f32 = 0.9;

pub(crate) fn plugin(app: &mut App) {
    app.init_resource::<Selected>()
        .add_systems(
            Update,
            (click_select, drop_stale)
                .chain()
                .in_set(ViewSystems::Input),
        )
        .add_systems(Update, draw_selection.in_set(ViewSystems::Present));
}

/// Distance from `point` to the ray (`origin`, unit `dir`), and how far along
/// the ray the closest point is (negative: behind the camera).
fn ray_distance(origin: Vec3, dir: Vec3, point: Vec3) -> (f32, f32) {
    let t = (point - origin).dot(dir);
    (point.distance(origin + dir * t), t)
}

/// The unit picker every click uses (selection, the command UI's orders):
/// the unit nearest to the ray (`origin`, unit `dir`, e.g. from
/// `Camera::viewport_to_world`) among `candidates` (`(id, feet in world
/// space)`), if any body ([`PICK_HEIGHT`] above the feet) lies within
/// [`PICK_RADIUS`] in front of the camera. Ties: the one closer to the
/// camera. Hosts with their own input call it directly (see also
/// `command_ui::pick_unit`, which gathers the [`Selectable`] candidates).
pub fn pick(
    origin: Vec3,
    dir: Vec3,
    candidates: impl IntoIterator<Item = (SimId, Vec3)>,
) -> Option<SimId> {
    candidates
        .into_iter()
        .filter_map(|(id, feet)| {
            let (d, t) = ray_distance(origin, dir, feet + Vec3::Y * PICK_HEIGHT);
            (t > 0.0 && d <= PICK_RADIUS).then_some((d, t, id))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)))
        .map(|(_, _, id)| id)
}

/// Where the selectable `sim` entity is drawn: its visual's position, else
/// its `SimPos`.
fn feet(
    sim: Entity,
    pos: Option<&SimPos>,
    index: Option<&VisualIndex>,
    transforms: &Query<&GlobalTransform, With<VisualOf>>,
) -> Option<Vec3> {
    index
        .and_then(|i| i.map.get(&sim))
        .and_then(|v| transforms.get(*v).ok())
        .map(GlobalTransform::translation)
        .or_else(|| pos.map(|p| sim_to_world(*p)))
}

#[allow(clippy::too_many_arguments)]
fn click_select(
    buttons: Option<Res<ButtonInput<MouseButton>>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    index: Option<Res<VisualIndex>>,
    view: Option<Res<ViewHidden>>,
    selectable: Query<&VisualOf, With<Selectable>>,
    sims: Query<(&SimId, Option<&SimPos>)>,
    transforms: Query<&GlobalTransform, With<VisualOf>>,
    mut selected: ResMut<Selected>,
) {
    if !buttons.is_some_and(|b| b.just_pressed(MouseButton::Left)) {
        return;
    }
    let Some(cursor) = windows.single().ok().and_then(Window::cursor_position) else {
        return;
    };
    let Ok((camera, cam_transform)) = cameras.single() else {
        return;
    };
    let Ok(ray) = camera.viewport_to_world(cam_transform, cursor) else {
        return;
    };
    let candidates = selectable.iter().filter_map(|of| {
        if hidden(view.as_deref(), of.0) {
            return None;
        }
        let (id, pos) = sims.get(of.0).ok()?;
        Some((*id, feet(of.0, pos, index.as_deref(), &transforms)?))
    });
    let picked = pick(ray.origin, *ray.direction, candidates);
    if selected.0 != picked {
        selected.0 = picked;
    }
}

/// A selected unit that left the sim, or that the view hides, is
/// deselected.
fn drop_stale(
    index: Option<Res<SimIndex>>,
    view: Option<Res<ViewHidden>>,
    mut selected: ResMut<Selected>,
) {
    if let Some(id) = selected.0
        && stale(index.as_deref(), view.as_deref(), id)
    {
        selected.0 = None;
    }
}

/// Whether a selected `id` must leave the selection: gone from the sim, or
/// hidden by the view.
pub fn stale(index: Option<&SimIndex>, view: Option<&ViewHidden>, id: SimId) -> bool {
    match index.and_then(|i| i.get(id)) {
        None => true,
        Some(e) => hidden(view, e),
    }
}

fn draw_selection(
    mut gizmos: Gizmos,
    selected: Res<Selected>,
    sim_index: Option<Res<SimIndex>>,
    index: Option<Res<VisualIndex>>,
    positions: Query<&SimPos>,
    transforms: Query<&GlobalTransform, With<VisualOf>>,
) {
    let Some(id) = selected.0 else { return };
    let Some(entity) = sim_index.and_then(|i| i.get(id)) else {
        return;
    };
    let Some(at) = feet(
        entity,
        positions.get(entity).ok(),
        index.as_deref(),
        &transforms,
    ) else {
        return;
    };
    let at = at + Vec3::Y * 0.06;
    let flat = Quat::from_rotation_arc(Vec3::Z, Vec3::Y);
    gizmos.circle(Isometry3d::new(at, flat), 0.6, css::LIME);
    gizmos.circle(Isometry3d::new(at, flat), 0.68, css::LIME);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pick_nearest_in_front() {
        let origin = Vec3::new(0.0, 10.0, 10.0);
        let target = Vec3::new(1.0, 0.0, 0.0);
        let dir = (target + Vec3::Y * PICK_HEIGHT - origin).normalize();
        let hit = pick(
            origin,
            dir,
            [(SimId(1), target), (SimId(2), Vec3::new(5.0, 0.0, 0.0))],
        );
        assert_eq!(hit, Some(SimId(1)));
        // Nothing near the ray.
        assert_eq!(
            pick(origin, dir, [(SimId(2), Vec3::new(5.0, 0.0, 0.0))]),
            None
        );
        // Behind the camera.
        assert_eq!(pick(origin, -dir, [(SimId(1), target)]), None);
    }

    /// A selected unit the view hides is deselected (no ring follows it
    /// into the fog); one that is shown stays.
    #[test]
    fn hidden_selected_unit_is_deselected() {
        let mut app = App::new();
        let unit = app.world_mut().spawn(SimId(4)).id();
        let mut index = SimIndex::default();
        index.map.insert(SimId(4), unit);
        app.insert_resource(index)
            .insert_resource(Selected(Some(SimId(4))))
            .init_resource::<ViewHidden>()
            .add_systems(Update, drop_stale);
        app.update();
        assert_eq!(app.world().resource::<Selected>().0, Some(SimId(4)));
        app.world_mut().resource_mut::<ViewHidden>().0.insert(unit);
        app.update();
        assert_eq!(app.world().resource::<Selected>().0, None);
    }
}
