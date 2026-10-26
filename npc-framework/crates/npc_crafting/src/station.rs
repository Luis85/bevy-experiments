//! Workstations (smart objects that host recipes), their order queues and
//! tool durability.
//!
//! A workstation **advertises** like any smart object: [`Workstation`] says
//! what it is, `npc_jobs::Approach` (optional) where to stand,
//! `Reservations` who uses it (capacity 1, claimed for a craft's duration).
//! Colony sims post work on it as [`CraftingOrders`]; agents list open work
//! with [`crafting_jobs`] and pick an order they can do with
//! [`next_order_for`] (or a `Craft` leaf without a recipe).

use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use npc_interaction::items::entity_of;
use npc_kernel::SimId;
use serde::{Deserialize, Serialize};

use crate::defs::RecipeRegistry;
use crate::run::{CraftError, can_craft};

/// A workstation: the kinds it counts as (sorted, unique), e.g.
/// `["anvil", "forge"]`.
#[derive(
    Component, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect,
)]
#[reflect(Component)]
pub struct Workstation {
    pub kinds: Vec<String>,
}

impl Workstation {
    pub fn new<S: Into<String>>(kinds: impl IntoIterator<Item = S>) -> Self {
        let mut v: Vec<String> = kinds.into_iter().map(Into::into).collect();
        v.sort_unstable();
        v.dedup();
        Self { kinds: v }
    }

    pub fn is(&self, kind: &str) -> bool {
        self.kinds
            .binary_search_by(|k| k.as_str().cmp(kind))
            .is_ok()
    }
}

/// One posted order: make `recipe` `remaining` more times.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
pub struct CraftOrder {
    pub id: u32,
    pub recipe: String,
    pub remaining: u32,
}

/// The work posted on a workstation, in posting order. A successful craft
/// started from an order counts it down; at 0 it is removed.
#[derive(
    Component, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect,
)]
#[reflect(Component)]
pub struct CraftingOrders {
    pub orders: Vec<CraftOrder>,
    /// The id the next posted order gets.
    pub next_id: u32,
}

/// A tool's (or any item's) wear: `current` of `max`; at 0 it breaks.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
#[reflect(Component)]
pub struct Durability {
    pub current: u32,
    pub max: u32,
}

impl Durability {
    pub fn new(max: u32) -> Self {
        Self { current: max, max }
    }
}

/// One open order as [`crafting_jobs`] lists it.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CraftJob {
    pub workstation: SimId,
    pub order: u32,
    pub recipe: String,
    pub remaining: u32,
}

/// Posts an order for `count` (> 0) crafts of `recipe` on `workstation`,
/// which must be a [`Workstation`] that can host the recipe. Returns the
/// order id. Sim code (or the `PostCraftOrder` command) only.
pub fn post_order(
    world: &mut World,
    workstation: SimId,
    recipe: &str,
    count: u32,
) -> Result<u32, CraftError> {
    check_order(world, workstation, recipe, count)?;
    let e = entity_of(world, workstation).expect("checked");
    let mut ent = world.entity_mut(e);
    if ent.get::<CraftingOrders>().is_none() {
        ent.insert(CraftingOrders::default());
    }
    let mut orders = ent.get_mut::<CraftingOrders>().expect("inserted");
    let id = orders.next_id;
    orders.next_id = orders.next_id.saturating_add(1);
    orders.orders.push(CraftOrder {
        id,
        recipe: recipe.to_owned(),
        remaining: count,
    });
    Ok(id)
}

/// At most this many open orders per workstation (final review cleanup 7:
/// `PostCraftOrder` is external input, and every order is hashed and
/// scanned by order picking).
pub const MAX_CRAFT_ORDERS: usize = 64;

/// What [`post_order`] checks.
pub fn check_order(
    world: &World,
    workstation: SimId,
    recipe: &str,
    count: u32,
) -> Result<(), CraftError> {
    let def = world
        .get_resource::<RecipeRegistry>()
        .and_then(|r| r.get(recipe))
        .ok_or_else(|| CraftError::unknown_recipe(world, recipe))?;
    if count == 0 {
        return Err(CraftError::InvalidCount);
    }
    if def.stations.is_empty() {
        return Err(CraftError::NoWorkstationNeeded);
    }
    let e = entity_of(world, workstation).ok_or(CraftError::WorkstationGone {
        workstation: workstation.0,
    })?;
    let ws = world
        .get::<Workstation>(e)
        .ok_or(CraftError::NotAWorkstation {
            target: workstation.0,
        })?;
    if !def.hosted_by(&ws.kinds) {
        return Err(CraftError::WrongWorkstation {
            workstation: workstation.0,
            needs: def.stations.clone(),
        });
    }
    if world
        .get::<CraftingOrders>(e)
        .is_some_and(|o| o.orders.len() >= MAX_CRAFT_ORDERS)
    {
        return Err(CraftError::TooManyOrders {
            workstation: workstation.0,
            max: MAX_CRAFT_ORDERS as u32,
        });
    }
    Ok(())
}

/// Every open order on every workstation, by workstation `SimId`, then in
/// posting order: the crafting job board.
pub fn crafting_jobs(world: &mut World) -> Vec<CraftJob> {
    let mut out: Vec<CraftJob> = world
        .query::<(&SimId, &CraftingOrders)>()
        .iter(world)
        .flat_map(|(&ws, o)| {
            o.orders.iter().map(move |order| CraftJob {
                workstation: ws,
                order: order.id,
                recipe: order.recipe.clone(),
                remaining: order.remaining,
            })
        })
        .collect();
    out.sort_by_key(|j| (j.workstation, j.order));
    out
}

/// The first order on `workstation` that `actor` can craft now, as
/// `(order id, recipe)`; else why not (the first order's reason, or
/// [`CraftError::NoOrders`]).
pub fn next_order_for(
    world: &World,
    actor: Entity,
    workstation: SimId,
) -> Result<(u32, String), CraftError> {
    let e = entity_of(world, workstation).ok_or(CraftError::WorkstationGone {
        workstation: workstation.0,
    })?;
    let orders = world
        .get::<CraftingOrders>(e)
        .map(|o| o.orders.clone())
        .unwrap_or_default();
    let mut first_err = None;
    for o in orders {
        match can_craft(world, actor, &o.recipe, Some(workstation)) {
            Ok(_) => return Ok((o.id, o.recipe)),
            Err(e) => {
                first_err.get_or_insert(e);
            }
        }
    }
    Err(first_err.unwrap_or(CraftError::NoOrders {
        workstation: workstation.0,
    }))
}

/// Counts order `id` on `workstation` down by one (removed at 0).
pub fn count_down(world: &mut World, workstation: SimId, id: u32) {
    let Some(e) = entity_of(world, workstation) else {
        return;
    };
    if let Some(mut orders) = world.get_mut::<CraftingOrders>(e) {
        if let Some(o) = orders.orders.iter_mut().find(|o| o.id == id) {
            o.remaining = o.remaining.saturating_sub(1);
        }
        orders.orders.retain(|o| o.remaining > 0);
    }
}
