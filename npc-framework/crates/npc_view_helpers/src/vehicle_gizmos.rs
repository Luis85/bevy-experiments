//! Crew gizmos for `npc_vehicles` (feature `vehicles`): a square around
//! each vehicle (green while it can move, red when it cannot) and a line
//! from it to each occupant. Read-only on the sim. Vehicles and riders in
//! [`ViewHidden`] are skipped.

use bevy::color::palettes::css::{LIME, RED, WHITE};
use bevy::prelude::*;
use npc_kernel::{SimIndex, SimPos};
use npc_vehicles::{MountedIn, Occupants, Vehicle, crew_allows_move};

use crate::{ViewHidden, ViewSystems, hidden, sim_to_world};

/// Whether crew gizmos are drawn (default on). An app may bind a key to it.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct VehicleGizmos {
    pub enabled: bool,
}

impl Default for VehicleGizmos {
    fn default() -> Self {
        Self { enabled: true }
    }
}

pub(crate) fn plugin(app: &mut App) {
    app.init_resource::<VehicleGizmos>()
        .add_systems(Update, draw_crews.in_set(ViewSystems::Present));
}

fn lift(p: SimPos) -> Vec3 {
    sim_to_world(p) + Vec3::Y * 0.08
}

/// Whether a rider's line to its vehicle is drawn: both are shown.
pub fn crew_line_shown(view: Option<&ViewHidden>, rider: Entity, vehicle: Entity) -> bool {
    !hidden(view, rider) && !hidden(view, vehicle)
}

fn draw_crews(
    settings: Res<VehicleGizmos>,
    index: Option<Res<SimIndex>>,
    view: Option<Res<ViewHidden>>,
    vehicles: Query<(Entity, &SimPos, &Vehicle, &Occupants)>,
    riders: Query<(Entity, &SimPos, &MountedIn)>,
    positions: Query<&SimPos>,
    mut gizmos: Gizmos,
) {
    let Some(index) = index.filter(|_| settings.enabled) else {
        return;
    };
    let view = view.as_deref();
    let flat = Quat::from_rotation_arc(Vec3::Z, Vec3::Y);
    for (e, pos, v, occ) in &vehicles {
        if hidden(view, e) {
            continue;
        }
        let color = if crew_allows_move(v, occ).is_ok() {
            LIME
        } else {
            RED
        };
        gizmos.rect(Isometry3d::new(lift(*pos), flat), Vec2::splat(1.2), color);
    }
    for (rider, pos, m) in &riders {
        let Some(ve) = index.get(m.vehicle) else {
            continue;
        };
        let Some(vp) = positions
            .get(ve)
            .ok()
            .filter(|_| crew_line_shown(view, rider, ve))
        else {
            continue;
        };
        gizmos.line(lift(*vp), lift(*pos) + Vec3::Y * 0.4, WHITE);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crew_lines_need_both_ends_shown() {
        let mut world = World::new();
        let (rider, cart) = (world.spawn_empty().id(), world.spawn_empty().id());
        assert!(crew_line_shown(None, rider, cart));
        let view = ViewHidden([cart].into_iter().collect());
        assert!(!crew_line_shown(Some(&view), rider, cart), "hidden cart");
        let view = ViewHidden([rider].into_iter().collect());
        assert!(!crew_line_shown(Some(&view), rider, cart), "hidden rider");
        assert!(crew_line_shown(Some(&ViewHidden::default()), rider, cart));
    }
}
