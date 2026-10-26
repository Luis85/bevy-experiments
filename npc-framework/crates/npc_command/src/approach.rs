//! Approach-then-act: the one "walk into reach, then act" engine every
//! order executor shares (the command × interaction glue's `Interact` and
//! `Mount` orders use it; so can a game's custom orders and verbs).
//!
//! An [`ApproachSpec`] says where to walk ([`ApproachSpec::goal`], through
//! the locomotion seam: `MoveIntent` at the unit's `Commandable` speed, so
//! a `NavAgent` path-finds) and when the unit is close enough
//! ([`Reach`]: a distance to a point, or an edge distance to a rectangle
//! such as a blocker's footprint; arriving at the goal counts too,
//! [`ApproachSpec::in_reach`]). [`approach_step`] runs one engine pass:
//! in reach → the walk stops and it returns [`ApproachStep::InReach`] (act
//! now); else it (re)starts the walk when the intent's goal differs and
//! returns [`ApproachStep::Walking`], or [`ApproachStep::Blocked`] once the
//! mover reported `Blocked` for `CommandConfig::block_fail_ticks` ticks
//! (the walk is stopped). [`approach_then`] maps that onto an
//! [`OrderStep`] around an `act` closure.
//!
//! [`ApproachSpec::to_footprint`] plans the walk to a footprint (a wall,
//! a building): the free cell around it nearest to the unit
//! (`SpatialGrid::nearest_free_cell_around`), in reach of its edge. Plan
//! it once (when the order starts) and keep the spec in the executor's
//! state, so the goal does not jump while the unit walks.
//!
//! Integer-only sim code; reads last tick's `MoveStatus` like every order.

use bevy_ecs::prelude::*;
use npc_kernel::SimPos;
use npc_kernel::spatial::{Footprint, SpatialGrid};
use npc_locomotion::{MoveIntent, MoveStatus, clear_move_intent, set_move_intent};
use serde::{Deserialize, Serialize};

use crate::CommandConfig;
use crate::exec::OrderStep;
use crate::types::Commandable;

/// When a unit is close enough to act.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Reach {
    /// Within `mm` of `point` (centre distance, inclusive).
    Point { point: SimPos, mm: i32 },
    /// Within `mm` of the rectangle `min..=max` (edge distance, inclusive;
    /// 0 inside it), e.g. a footprint's area
    /// (`SpatialGrid::footprint_area`).
    Area { min: SimPos, max: SimPos, mm: i32 },
}

impl Reach {
    /// Whether `pos` is in reach.
    pub fn contains(&self, pos: SimPos) -> bool {
        match *self {
            Reach::Point { point, mm } => {
                let r = i64::from(mm.max(0));
                pos.dist2(point) <= r * r
            }
            Reach::Area { min, max, mm } => {
                let r = i64::from(mm.max(0));
                dist2_to_area(pos, min, max) <= r * r
            }
        }
    }
}

/// Squared distance (mm²) from `p` to the rectangle `min..=max` (0 inside).
pub fn dist2_to_area(p: SimPos, min: SimPos, max: SimPos) -> i64 {
    let axis = |v: i32, lo: i32, hi: i32| -> i64 {
        let (v, lo, hi) = (i64::from(v), i64::from(lo), i64::from(hi));
        (lo - v).max(v - hi).max(0)
    };
    let dx = axis(p.x, min.x.min(max.x), min.x.max(max.x));
    let dz = axis(p.z, min.z.min(max.z), min.z.max(max.z));
    dx * dx + dz * dz
}

/// A walk into reach: walk to `goal` (stopping within `tolerance_mm` of
/// it) until [`Reach::contains`] the unit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ApproachSpec {
    pub reach: Reach,
    pub goal: SimPos,
    pub tolerance_mm: i32,
}

impl ApproachSpec {
    /// Walk to `point` until within `reach_mm` of it, stopping within 3/4
    /// of the reach (what the interaction glue uses).
    pub fn to_point(point: SimPos, reach_mm: i32) -> Self {
        let reach = reach_mm.max(0);
        Self {
            reach: Reach::Point { point, mm: reach },
            goal: point,
            tolerance_mm: i32::try_from(i64::from(reach) * 3 / 4).unwrap_or(i32::MAX),
        }
    }

    /// Walk next to the footprint `footprint` around `pos` until within
    /// `reach_mm` of its edge: the goal is the free cell around it nearest
    /// to `from` (rings `1..=max_ring`, see
    /// `SpatialGrid::nearest_free_cell_around`), reached within
    /// `tolerance_mm`. `None`: no free cell around it.
    pub fn to_footprint(
        grid: &SpatialGrid,
        from: SimPos,
        pos: SimPos,
        footprint: Footprint,
        reach_mm: i32,
        tolerance_mm: i32,
        max_ring: u32,
    ) -> Option<Self> {
        let (min, max) = grid.footprint_area(pos, footprint);
        let reach = Reach::Area {
            min,
            max,
            mm: reach_mm,
        };
        let goal = if reach.contains(from) {
            from
        } else {
            grid.nearest_free_cell_around(from, pos, footprint, max_ring)?
        };
        Some(Self {
            reach,
            goal,
            tolerance_mm,
        })
    }
}

impl ApproachSpec {
    /// In reach: [`Reach::contains`] `pos`, or `pos` is within
    /// `tolerance_mm` of the goal (where the walk ends: the unit cannot get
    /// any closer, so a goal next to a footprint counts as reached).
    pub fn in_reach(&self, pos: SimPos) -> bool {
        let t = i64::from(self.tolerance_mm.max(0));
        self.reach.contains(pos) || pos.dist2(self.goal) <= t * t
    }
}

/// What one [`approach_step`] did.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ApproachStep {
    /// In reach; the walk is stopped. Act now.
    InReach,
    /// Still walking.
    Walking,
    /// The walk was blocked for `CommandConfig::block_fail_ticks`; stopped.
    Blocked,
}

fn blocked_too_long(world: &World, unit: Entity) -> bool {
    let limit = world
        .get_resource::<CommandConfig>()
        .copied()
        .unwrap_or_default()
        .block_fail_ticks;
    matches!(
        world.get::<MoveStatus>(unit),
        Some(MoveStatus::Blocked { ticks }) if *ticks >= limit
    )
}

/// One engine pass of a walk into reach (see the module docs). A unit
/// without a position counts as in reach (nothing to walk).
pub fn approach_step(world: &mut World, unit: Entity, spec: &ApproachSpec) -> ApproachStep {
    let here = world.get::<SimPos>(unit).copied();
    if here.is_none_or(|p| spec.in_reach(p)) {
        clear_move_intent(world, unit);
        return ApproachStep::InReach;
    }
    if world.get::<MoveIntent>(unit).map(|i| i.goal) != Some(spec.goal) {
        let c = world.get::<Commandable>(unit).copied().unwrap_or_default();
        set_move_intent(
            world,
            unit,
            MoveIntent {
                goal: spec.goal,
                speed_mm_per_tick: c.speed_mm_per_tick,
                tolerance_mm: spec.tolerance_mm,
            },
        );
    } else if blocked_too_long(world, unit) {
        clear_move_intent(world, unit);
        return ApproachStep::Blocked;
    }
    ApproachStep::Walking
}

/// [`approach_step`], then `act` once in reach: `Walking` → `Running`,
/// `Blocked` → `Failed("blocked")`, `InReach` → `act(world, unit)`.
pub fn approach_then(
    world: &mut World,
    unit: Entity,
    spec: &ApproachSpec,
    act: impl FnOnce(&mut World, Entity) -> OrderStep,
) -> OrderStep {
    match approach_step(world, unit, spec) {
        ApproachStep::Walking => OrderStep::Running,
        ApproachStep::Blocked => OrderStep::Failed(crate::exec::BLOCKED.into()),
        ApproachStep::InReach => act(world, unit),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reach_point_and_area() {
        let p = Reach::Point {
            point: SimPos::new(0, 0),
            mm: 1000,
        };
        assert!(p.contains(SimPos::new(1000, 0)));
        assert!(!p.contains(SimPos::new(800, 800)));
        let a = Reach::Area {
            min: SimPos::new(0, 0),
            max: SimPos::new(2000, 1000),
            mm: 500,
        };
        assert!(a.contains(SimPos::new(1000, 500)), "inside");
        assert!(a.contains(SimPos::new(2500, 1000)), "edge distance 500");
        assert!(
            !a.contains(SimPos::new(2400, 1400)),
            "corner distance > 500"
        );
        assert_eq!(dist2_to_area(SimPos::new(-3, 4), a_min(), a_max()), 9);
    }

    fn a_min() -> SimPos {
        SimPos::new(0, 0)
    }
    fn a_max() -> SimPos {
        SimPos::new(10, 10)
    }

    #[test]
    fn to_point_keeps_the_glue_tolerance() {
        let s = ApproachSpec::to_point(SimPos::new(5, 5), 1200);
        assert_eq!(s.tolerance_mm, 900);
        assert_eq!(s.goal, SimPos::new(5, 5));
        assert_eq!(
            ApproachSpec::to_point(SimPos::new(0, 0), -5).tolerance_mm,
            0
        );
    }
}
