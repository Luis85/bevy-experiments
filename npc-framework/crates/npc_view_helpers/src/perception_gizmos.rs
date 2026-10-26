//! Perception gizmos for `npc_perception` agents (feature `perception`):
//! each vision cone (two edges and the range arc, around the agent's
//! `Facing`), a circle per proximity/hearing radius, and a ray to every
//! remembered percept: green while sensed (line of sight at the latest
//! scan), grey while only remembered. Read-only on the sim.

use bevy::color::palettes::css::{DARK_GRAY, LIME, SKY_BLUE, WHITE};
use bevy::prelude::*;
use npc_kernel::SimPos;
use npc_perception::{Facing, PerceptionMemory, Sensor, Sensors};

use crate::{ViewSystems, sim_to_world};

/// What perception gizmos draw (default: everything). An app may bind keys.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct PerceptionGizmos {
    pub enabled: bool,
    /// Proximity and hearing circles.
    pub radii: bool,
    /// Rays to remembered percepts.
    pub rays: bool,
}

impl Default for PerceptionGizmos {
    fn default() -> Self {
        Self {
            enabled: true,
            radii: true,
            rays: true,
        }
    }
}

pub(crate) fn plugin(app: &mut App) {
    app.init_resource::<PerceptionGizmos>()
        .add_systems(Update, draw_perception.in_set(ViewSystems::Present));
}

/// Slightly above the ground (below the nav path lines).
fn lift(p: SimPos) -> Vec3 {
    sim_to_world(p) + Vec3::Y * 0.04
}

/// Segments used for a cone's range arc.
const ARC_SEGMENTS: usize = 16;

fn draw_perception(
    settings: Res<PerceptionGizmos>,
    agents: Query<(
        &SimPos,
        &Sensors,
        Option<&Facing>,
        Option<&PerceptionMemory>,
    )>,
    mut gizmos: Gizmos,
) {
    if !settings.enabled {
        return;
    }
    let flat = Quat::from_rotation_arc(Vec3::Z, Vec3::Y);
    for (pos, sensors, facing, memory) in &agents {
        let origin = lift(*pos);
        let f = facing.copied().unwrap_or_default();
        let dir = Vec2::new(f.x as f32, f.z as f32).normalize_or(Vec2::Y);
        for sensor in &sensors.sensors {
            match *sensor {
                Sensor::Vision(v) => {
                    let range = v.range_mm.max(0) as f32 / 1000.0;
                    let half = f32::from(v.half_fov_deg).to_radians();
                    let point = |angle: f32| {
                        let d = Vec2::from_angle(angle).rotate(dir) * range;
                        origin + Vec3::new(d.x, 0.0, d.y)
                    };
                    let arc: Vec<Vec3> = (0..=ARC_SEGMENTS)
                        .map(|i| point(-half + 2.0 * half * i as f32 / ARC_SEGMENTS as f32))
                        .collect();
                    if v.half_fov_deg < 180 {
                        gizmos.line(origin, arc[0], SKY_BLUE);
                        gizmos.line(origin, arc[ARC_SEGMENTS], SKY_BLUE);
                    }
                    gizmos.linestrip(arc, SKY_BLUE);
                }
                Sensor::Proximity(p) if settings.radii => {
                    let r = p.radius_mm.max(0) as f32 / 1000.0;
                    gizmos.circle(Isometry3d::new(origin, flat), r, WHITE);
                }
                Sensor::Hearing(h) if settings.radii => {
                    let r = h.radius_mm.max(0) as f32 / 1000.0;
                    gizmos.circle(Isometry3d::new(origin, flat), r, DARK_GRAY);
                }
                _ => {}
            }
        }
        if settings.rays
            && let Some(memory) = memory
        {
            for percept in memory.percepts.values() {
                let color = if percept.sensed { LIME } else { DARK_GRAY };
                gizmos.line(origin, lift(percept.pos), color);
            }
        }
    }
}
