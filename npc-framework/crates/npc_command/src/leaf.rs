//! Behaviour-tree integration: the `has_unit_order` condition and the
//! `ExecuteOrder` leaf, so any unit tree can include an order branch:
//!
//! ```ignore
//! b.guard("Orders", HAS_UNIT_ORDER, Abort::LowerPriority, |b| {
//!     b.leaf("ExecuteOrder", EXECUTE_ORDER_ACTION, ());
//! });
//! ```
//!
//! The leaf runs the unit's whole queue: `Running` while any order is left,
//! then `Success` (or `Failure` when the last order failed). A new order
//! re-thinks the unit (`ThinkRequest`), so the guard can preempt lower
//! branches. Aborting the leaf (a more urgent branch) pauses the current
//! order (movement stops; it resumes when the leaf runs again). The guard
//! implies the leaf can start (it starts whenever an order is held).
//!
//! The kernel's village `has_order` / `FollowOrder` (`CurrentOrder`) are a
//! different, minimal mechanism and are left untouched; the names here are
//! distinct so both can live in one app.

use bevy_ecs::prelude::*;
use npc_ai::{ParamValue, Status, StopReason};

use crate::exec::{pause_orders, step_unit};
use crate::types::{ExecutingOrder, OrderQueue};

/// The registered action name of the order leaf.
pub const EXECUTE_ORDER_ACTION: &str = "ExecuteOrder";
/// The registered condition: the unit holds an order (current or queued).
pub const HAS_UNIT_ORDER: &str = "has_unit_order";

/// Condition `has_unit_order`.
pub fn cond_has_unit_order(world: &World, entity: Entity) -> bool {
    world
        .get::<OrderQueue>(entity)
        .is_some_and(OrderQueue::has_order)
}

/// `ExecuteOrder` start: starts (or resumes) the current order at once.
pub fn execute_order_start(world: &mut World, agent: Entity, _param: &ParamValue) -> Status {
    if !cond_has_unit_order(world, agent) {
        return Status::Failure;
    }
    world.entity_mut(agent).insert(ExecutingOrder);
    step_unit(world, agent);
    let Some(q) = world.get::<OrderQueue>(agent) else {
        return Status::Failure;
    };
    if q.has_order() {
        return Status::Running;
    }
    let ok = q.last_ok;
    world.entity_mut(agent).remove::<ExecutingOrder>();
    if ok { Status::Success } else { Status::Failure }
}

/// `ExecuteOrder` stop: an aborted leaf pauses the running order.
pub fn execute_order_stop(world: &mut World, agent: Entity, reason: StopReason) {
    if let Ok(mut e) = world.get_entity_mut(agent) {
        e.remove::<ExecutingOrder>();
    }
    if reason == StopReason::Aborted {
        pause_orders(world, agent);
    }
}
