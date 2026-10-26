//! Fixed-point ground-plane positions in millimetres. No floats in the sim.

use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use serde::{Deserialize, Serialize};

/// Position on the ground plane in millimetres. x = east, z = south (Bevy's +Z).
/// JSON: `{"x":0,"z":0}`.
#[derive(
    Component, Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect,
)]
#[reflect(Component)]
// BRP compatibility (ADR 0009): the reflected type path from before the
// P3 crate split, so `world.*` queries keep working.
#[type_path = "npc_sim::math"]
pub struct SimPos {
    pub x: i32,
    pub z: i32,
}

/// Position at the start of the current tick (for render interpolation).
/// Copied from `SimPos` in `NpcSet::Inputs` each tick by [`copy_prev_pos_system`].
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct PrevSimPos(pub SimPos);

impl SimPos {
    pub const fn new(x: i32, z: i32) -> Self {
        Self { x, z }
    }

    /// Squared distance in mm² (i64, cannot overflow for |coords| < 2^30;
    /// saturates at `i64::MAX` beyond that).
    pub fn dist2(self, other: SimPos) -> i64 {
        let dx = other.x as i128 - self.x as i128;
        let dz = other.z as i128 - self.z as i128;
        (dx * dx + dz * dz).min(i64::MAX as i128) as i64
    }

    /// Moves at most `step_mm` straight towards `goal` using integer isqrt.
    /// Returns `goal` exactly when within `step_mm`. Otherwise returns
    /// `self + d * step / isqrt(|d|²)` per axis, truncating towards 0.
    /// A non-positive step never moves (except when already at `goal`).
    pub fn step_towards(self, goal: SimPos, step_mm: i32) -> SimPos {
        // i128 so the full i32 coordinate range cannot overflow.
        let dx = goal.x as i128 - self.x as i128;
        let dz = goal.z as i128 - self.z as i128;
        let len = (dx * dx + dz * dz).isqrt();
        let step = step_mm.max(0) as i128;
        if len <= step {
            return goal;
        }
        // len > step >= 0, so len > 0; |d * step / len| < |d|, so the result
        // lies between self and goal and fits in i32.
        SimPos {
            x: (self.x as i128 + dx * step / len) as i32,
            z: (self.z as i128 + dz * step / len) as i32,
        }
    }
}

/// `NpcSet::Inputs`: `PrevSimPos = SimPos` for every positioned entity.
/// Entities that have a `SimPos` but no `PrevSimPos` get one (applied at the
/// next sync point, i.e. before `NpcSet::Needs`).
pub fn copy_prev_pos_system(
    mut with_prev: Query<(&SimPos, &mut PrevSimPos)>,
    without_prev: Query<(Entity, &SimPos), Without<PrevSimPos>>,
    mut commands: Commands,
) {
    for (pos, mut prev) in &mut with_prev {
        // Avoid spurious change detection on static places.
        if prev.0 != *pos {
            prev.0 = *pos;
        }
    }
    for (entity, pos) in &without_prev {
        commands.entity(entity).insert(PrevSimPos(*pos));
    }
}
