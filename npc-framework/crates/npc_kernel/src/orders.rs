//! Standing orders (spec §3.3): `GoTo` and `Clear`, given through
//! [`SimCommand::Order`](crate::SimCommand::Order).
//!
//! An order is stored in the unit's [`CurrentOrder`]; a unit takes orders when
//! it has that component. What a unit *does* with an order is content: its
//! tree typically guards a "follow order" branch with the `has_order`
//! condition ([`cond_has_order`]) and runs a content action that walks to
//! the order's position, then clears it.

use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use npc_ai::ThinkRequest;
use serde::{Deserialize, Serialize};

use crate::math::SimPos;

/// An order as given in a command. JSON: `{"type":"GoTo","pos":[x,z]}`, `{"type":"Clear"}`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Order {
    /// Walk to `pos` (mm), then the order is done.
    GoTo { pos: (i32, i32) },
    /// Drop the current order.
    Clear,
}

/// An order a unit currently holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
#[serde(tag = "type")]
// BRP compatibility (ADR 0009): the reflected type path from before the
// P3 crate split, so `world.*` queries keep working.
#[type_path = "npc_sim::orders"]
pub enum OrderKind {
    GoTo { pos: SimPos },
}

/// The unit's standing order (`None` = no order). Units that take orders
/// carry it (content usually `#[require]`s it).
#[derive(
    Component, Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect,
)]
#[reflect(Component)]
// BRP compatibility (ADR 0009): the reflected type path from before the
// P3 crate split, so `world.*` queries keep working.
#[type_path = "npc_sim::orders"]
pub struct CurrentOrder(pub Option<OrderKind>);

/// Stores `order` on `agent` and asks it to re-think this tick.
pub fn apply_order(world: &mut World, agent: Entity, order: Order) {
    let current = match order {
        Order::GoTo { pos: (x, z) } => Some(OrderKind::GoTo {
            pos: SimPos::new(x, z),
        }),
        Order::Clear => None,
    };
    if let Ok(mut e) = world.get_entity_mut(agent) {
        e.insert((CurrentOrder(current), ThinkRequest));
    }
}

/// Condition `has_order`: the agent holds an order. Content registers it
/// under the name it uses in its trees.
pub fn cond_has_order(world: &World, entity: Entity) -> bool {
    world
        .get::<CurrentOrder>(entity)
        .is_some_and(|o| o.0.is_some())
}

/// Clears `agent`'s order.
pub fn clear_order(world: &mut World, agent: Entity) {
    if let Some(mut order) = world.get_mut::<CurrentOrder>(agent) {
        order.0 = None;
    }
}
