//! Following orders (spec §3.3): the villager side of the kernel's standing
//! orders (`CurrentOrder`, `SimCommand::Order`).
//!
//! The villager tree's `Guard "FollowOrder" [has_order, LowerPriority]`
//! (between Forage and NightSleep) runs the `FollowOrder` action: walk to the
//! order's position, then clear the order. So an order interrupts sleep,
//! work and wander, but not the survival branches above it (urgent eat/sleep,
//! forage); an order interrupted by those is kept and resumed afterwards.
//!
//! While following: a replaced `GoTo` re-targets the walk (`order_system`),
//! a `Clear` fails the walk on the next action pass, and the branch ends.

use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use npc_framework::kernel::hash::Fnv64;
use npc_framework::kernel::orders::clear_order;
use npc_framework::prelude::*;
use serde::{Deserialize, Serialize};

use npc_framework::locomotion::MoveIntent;

use crate::actions::{MoveTo, end, end_walk, fail, set_activity, set_target, start_walk};
use crate::activity::Activity;
use crate::villager::BB_DEST;

/// Marker: the agent's running `MoveTo` belongs to the `FollowOrder` action.
#[derive(
    Component, Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect,
)]
#[component(storage = "SparseSet")]
#[reflect(Component)]
pub struct OrderMove;

impl SimHash for OrderMove {
    const NAME: &'static str = "order_move";
    fn sim_hash(&self, _h: &mut Fnv64) {}
}

/// `FollowOrder`: walk to the `GoTo` position (`BB_DEST` shows it). Success
/// at once, clearing the order, when already there; Failure without an order.
pub fn follow_order_start(world: &mut World, agent: Entity, _param: &ParamValue) -> Status {
    let order = world.get::<CurrentOrder>(agent).and_then(|o| o.0);
    let (Some(OrderKind::GoTo { pos: goal }), Some(&pos)) = (order, world.get::<SimPos>(agent))
    else {
        return fail(world, agent);
    };
    set_target(world, agent, BbValue::None, BbValue::Pos(goal.x, goal.z));
    if pos == goal {
        clear_order(world, agent);
        return Status::Success;
    }
    world.entity_mut(agent).insert(OrderMove);
    start_walk(world, agent, goal);
    set_activity(world, agent, Activity::Walk);
    Status::Running
}

/// On `Finished` the order is done and cleared, unless a different order
/// arrived meanwhile; on `Aborted` it is kept, to resume later.
pub fn stop_follow_order(world: &mut World, agent: Entity, reason: StopReason) {
    if let Ok(mut e) = world.get_entity_mut(agent) {
        e.remove::<OrderMove>();
    }
    let goal = end_walk(world, agent);
    if reason == StopReason::Finished {
        let current = world.get::<CurrentOrder>(agent).and_then(|o| o.0);
        if current.is_none() || current.map(|OrderKind::GoTo { pos }| pos) == goal {
            clear_order(world, agent);
        }
    }
    end(world, agent, reason);
}

type OrderWalk<'a> = (
    &'a CurrentOrder,
    &'a mut MoveTo,
    Option<&'a mut MoveIntent>,
    &'a mut ActiveLeaf,
    &'a mut Blackboard,
);

/// `NpcSet::Actions`, before `move_to_system`: keeps an order walk in sync
/// with the order. A replaced `GoTo` re-targets the walk (`MoveTo`, its
/// `MoveIntent` and `BB_DEST`); a cleared order fails it.
pub fn order_system(mut walkers: Query<OrderWalk, With<OrderMove>>) {
    for (order, mut move_to, intent, mut leaf, mut bb) in &mut walkers {
        if leaf.status != Status::Running {
            continue;
        }
        match order.0 {
            Some(OrderKind::GoTo { pos }) => {
                if move_to.goal != pos {
                    move_to.goal = pos;
                    if let Some(mut intent) = intent {
                        intent.goal = pos;
                    }
                    bb.set(BB_DEST, BbValue::Pos(pos.x, pos.z));
                }
            }
            None => leaf.status = Status::Failure,
        }
    }
}
