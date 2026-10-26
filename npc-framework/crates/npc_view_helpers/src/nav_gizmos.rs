//! Path gizmos for `npc_nav` agents (feature `nav`): a line from each
//! agent through its remaining waypoints, a small circle per waypoint and a
//! larger one on marked waypoints (doors to open). Read-only on the sim.
//! Agents in [`ViewHidden`] are skipped.

use bevy::color::palettes::css::{ORANGE, YELLOW};
use bevy::prelude::*;
use npc_kernel::SimPos;
use npc_nav::Path;

use crate::{ViewHidden, ViewSystems, hidden, sim_to_world};

/// Whether path gizmos are drawn (default on). An app may bind a key to it.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct NavGizmos {
    pub enabled: bool,
}

impl Default for NavGizmos {
    fn default() -> Self {
        Self { enabled: true }
    }
}

pub(crate) fn plugin(app: &mut App) {
    app.init_resource::<NavGizmos>()
        .add_systems(Update, draw_paths.in_set(ViewSystems::Present));
}

/// Lifted slightly above the ground so the lines are not z-fighting it.
fn lift(p: SimPos) -> Vec3 {
    sim_to_world(p) + Vec3::Y * 0.05
}

fn draw_paths(
    settings: Res<NavGizmos>,
    view: Option<Res<ViewHidden>>,
    agents: Query<(Entity, &SimPos, &Path)>,
    mut gizmos: Gizmos,
) {
    if !settings.enabled {
        return;
    }
    let flat = Quat::from_rotation_arc(Vec3::Z, Vec3::Y);
    for (e, pos, path) in &agents {
        if hidden(view.as_deref(), e) {
            continue;
        }
        let mut from = lift(*pos);
        for (i, &wp) in path.waypoints.iter().enumerate().skip(path.next as usize) {
            let to = lift(wp);
            gizmos.line(from, to, YELLOW);
            let marked = path.marker_at(i as u32).is_some();
            let (radius, color) = if marked {
                (0.4, ORANGE)
            } else {
                (0.12, YELLOW)
            };
            gizmos.circle(Isometry3d::new(to, flat), radius, color);
            from = to;
        }
    }
}
