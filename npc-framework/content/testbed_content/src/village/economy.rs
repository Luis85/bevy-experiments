//! Atomic inventory/currency operations. No output or currency is fabricated by trade.
use super::{catalog, market, model::*};
use bevy_ecs::prelude::*;
use npc_framework::interaction::items::{self, entity_of};
use npc_framework::interaction::{CarriedBy, Inventory, Item};
use npc_framework::prelude::*;
use serde_json::json;

pub fn free_items(world: &World, holder: Entity) -> Vec<(SimId, Item)> {
    world
        .get::<Inventory>(holder)
        .into_iter()
        .flat_map(|i| i.items.iter())
        .filter_map(|id| {
            if world
                .get_resource::<Reservations>()
                .is_some_and(|r| r.count(*id) > 0)
            {
                return None;
            }
            Some((*id, world.get::<Item>(entity_of(world, *id)?)?.clone()))
        })
        .collect()
}
pub fn count(world: &World, holder: Entity, kind: &str) -> u32 {
    free_items(world, holder)
        .iter()
        .filter(|(_, i)| i.kind == kind)
        .map(|(_, i)| i.stack)
        .sum()
}
pub fn fit(world: &World, holder: Entity, incoming: &[Item]) -> Result<(), String> {
    fit_without(world, holder, incoming, &[])
}

/// Pure post-spend capacity check. Only actual items carried by this holder
/// may free weight or slots; unrelated IDs never create imaginary capacity.
pub fn fit_without(
    world: &World,
    holder: Entity,
    incoming: &[Item],
    spent: &[SimId],
) -> Result<(), String> {
    let inv = world.get::<Inventory>(holder).ok_or("no inventory")?;
    let holder_id = world.get::<SimId>(holder).copied();
    let carried = items::carried_items(world, holder);
    let excluded: Vec<_> = carried
        .iter()
        .copied()
        .filter(|id| {
            spent.contains(id)
                && entity_of(world, *id).is_some_and(|e| {
                    world
                        .get::<CarriedBy>(e)
                        .is_some_and(|c| Some(c.holder) == holder_id)
                })
        })
        .collect();
    let mut stacks: Vec<Item> = inv
        .items
        .iter()
        .filter(|id| !excluded.contains(id))
        .filter_map(|id| world.get::<Item>(entity_of(world, *id)?).cloned())
        .collect();
    let mut weight = carried
        .iter()
        .filter(|id| !excluded.contains(id))
        .filter_map(|id| world.get::<Item>(entity_of(world, *id)?))
        .try_fold(0_u32, |sum, item| {
            sum.checked_add(items::item_weight(world, item))
        })
        .ok_or("too much weight")?;
    for item in incoming {
        let def = items::kind_def(world, &item.kind).ok_or("unknown item")?;
        weight = weight
            .checked_add(
                def.weight_g
                    .checked_mul(item.stack)
                    .ok_or("too much weight")?,
            )
            .ok_or("too much weight")?;
        let mut remaining = item.stack;
        if def.stackable {
            for stack in stacks
                .iter_mut()
                .filter(|s| s.kind == item.kind && s.quality == item.quality)
            {
                let add = remaining.min(def.max_stack.saturating_sub(stack.stack));
                stack.stack += add;
                remaining -= add;
            }
        }
        while remaining > 0 {
            let add = remaining.min(def.max_stack.max(1));
            stacks.push(Item::new(&item.kind).quality(item.quality).stack(add));
            remaining -= add;
        }
    }
    if weight > inv.max_weight_g {
        return Err(format!(
            "pack would weigh {weight} g; capacity {} g",
            inv.max_weight_g
        ));
    }
    if stacks.len() > inv.slots as usize {
        return Err("not enough inventory slots".into());
    }
    Ok(())
}
pub fn grant(world: &mut World, holder: Entity, item: Item) {
    let Some(def) = items::kind_def(world, &item.kind).cloned() else {
        return;
    };
    let mut remaining = item.stack;
    let existing: Vec<_> = world
        .get::<Inventory>(holder)
        .map(|i| i.items.clone())
        .unwrap_or_default();
    if def.stackable {
        for id in existing {
            let Some(e) = entity_of(world, id) else {
                continue;
            };
            if let Some(mut stack) = world.get_mut::<Item>(e)
                && stack.kind == item.kind
                && stack.quality == item.quality
            {
                let add = remaining.min(def.max_stack.saturating_sub(stack.stack));
                stack.stack += add;
                remaining -= add;
            }
        }
    }
    while remaining > 0 {
        let add = remaining.min(def.max_stack.max(1));
        let (_, e) = spawn_sim(
            world,
            (
                Item::new(&item.kind).quality(item.quality).stack(add),
                Perceivable::new(crate::world::kinds::ITEM),
            ),
        );
        items::put_in_inventory(world, holder, e);
        remaining -= add;
    }
}
pub fn selected(
    world: &World,
    holder: Entity,
    kind: &str,
    n: u32,
) -> Result<Vec<(SimId, Item)>, String> {
    let mut remaining = n;
    let mut out = Vec::new();
    for (id, mut item) in free_items(world, holder)
        .into_iter()
        .filter(|(_, i)| i.kind == kind)
    {
        let take = remaining.min(item.stack);
        item.stack = take;
        out.push((id, item));
        remaining -= take;
        if remaining == 0 {
            break;
        }
    }
    if remaining > 0 {
        Err(format!(
            "only {} unreserved {kind} available",
            n - remaining
        ))
    } else {
        Ok(out)
    }
}
pub fn consume(world: &mut World, holder: Entity, kind: &str, n: u32) -> Result<(), String> {
    let items = selected(world, holder, kind, n)?;
    for (id, item) in items {
        let Some(e) = entity_of(world, id) else {
            continue;
        };
        let left = world
            .get::<Item>(e)
            .map_or(0, |i| i.stack)
            .saturating_sub(item.stack);
        if left == 0 {
            items::take_from_holder(world, e);
            world.despawn(e);
        } else if let Some(mut i) = world.get_mut::<Item>(e) {
            i.stack = left;
        }
    }
    Ok(())
}
pub fn near(world: &World, a: Entity, b: Entity) -> bool {
    world
        .get::<SimPos>(a)
        .zip(world.get::<SimPos>(b))
        .is_some_and(|(a, b)| a.dist2(*b) <= 2_250_000)
}
pub fn check_trade(
    world: &World,
    buyer: Entity,
    seller: Entity,
    kind: &str,
    n: u32,
) -> Result<(), String> {
    market::quote(world, buyer, seller, kind, n).map(|_| ())
}
pub fn trade(
    world: &mut World,
    buyer: Entity,
    seller: Entity,
    kind: &str,
    n: u32,
) -> Result<(), String> {
    let quoted = market::quote(world, buyer, seller, kind, n)?;
    let goods = selected(world, seller, kind, n)?;
    let cost = quoted.total;
    if items::kind_def(world, kind).is_some_and(|def| def.stackable) {
        consume(world, seller, kind, n)?;
        for (_, item) in goods {
            grant(world, buyer, item);
        }
    } else {
        // Tools have identity, wear and possibly other capability components.
        for (id, _) in goods {
            let item = entity_of(world, id).expect("validated owned item");
            items::put_in_inventory(world, buyer, item);
        }
    }
    world
        .get_mut::<Wallet>(buyer)
        .expect("checked wallet")
        .coins -= cost;
    world
        .get_mut::<Wallet>(seller)
        .expect("checked wallet")
        .coins += cost;
    world.resource_mut::<VillageState>().transactions += 1;
    market::record_trade(world, buyer, seller, kind, n, cost);
    let bid = *world.get::<SimId>(buyer).expect("sim buyer");
    let sid = *world.get::<SimId>(seller).expect("sim seller");
    let mut receipt = json!({"buyer":bid,"seller":sid,"kind":kind,"quantity":n,"coins":cost});
    let mut sale_receipt = json!({"kind":kind,"quantity":n,"coins":cost});
    if market::enabled(world) {
        receipt["unit_price"] = json!(quoted.unit_price);
        sale_receipt["unit_price"] = json!(quoted.unit_price);
    }
    record_effect(world, Some(bid), "VillageTrade", &receipt);
    // A controlled seller needs its own receipt: the agent event ring retains
    // controlled-unit effects, not the merchant buyer's private event stream.
    // This journals the seller's completed sale without creating a second trade.
    record_effect(world, Some(sid), "VillageSaleCompleted", &sale_receipt);
    Ok(())
}
pub fn merchant_check(
    world: &World,
    actor: Entity,
    trader: Entity,
    kind: &str,
    n: u32,
    buy: bool,
) -> Result<(), String> {
    // Public listing presence is stable knowledge; its current remaining stock
    // is only actionable when the two actors can physically trade.
    if market::enabled(world) && !near(world, actor, trader) {
        return Err("move within 1.5 m of the trader".into());
    }
    let role = world
        .get::<Villager>(trader)
        .ok_or("target is not a villager")?
        .role;
    if role == Role::Visitor
        && !(market::enabled(world) && buy && market::sells(world, trader, kind))
    {
        return Err("target is not a shopkeeper".into());
    }
    if buy {
        if !market::sells(world, trader, kind) {
            return Err("this shop does not sell that good".into());
        }
        check_trade(world, actor, trader, kind, n)
    } else {
        let construction_demand = super::colony::construction_materials(world, trader)
            .iter()
            .any(|(material, quantity)| *material == kind && *quantity > 0);
        let household_demand = super::colony::household_materials(world, trader)
            .iter()
            .any(|(material, quantity)| *material == kind && *quantity > 0);
        if !catalog::wants_for(world, role, kind) && !construction_demand && !household_demand {
            return Err("this shop has no use for that good".into());
        }
        // Dynamic merchant stock is discoverable only while trading nearby.
        // Validate reach first, including direct command/handler requests.
        if !near(world, actor, trader) {
            return Err("move within 1.5 m of the trader".into());
        }
        let limit = if market::enabled(world) {
            super::production::purchase_target(world, trader, kind)
        } else {
            catalog::stock_limit(kind)
        };
        if count(world, trader, kind).saturating_add(n) > limit {
            return Err(if market::enabled(world) {
                "the buyer has no open procurement demand for that quantity".into()
            } else {
                "the shop has enough stock".into()
            });
        }
        if market::enabled(world)
            && let Some(limit) = super::production::purchase_bid_limit(world, trader, kind)
            && market::quote(world, trader, actor, kind, n)?.unit_price > limit
        {
            return Err("this quote exceeds the buyer's current procurement bid limit".into());
        }
        check_trade(world, trader, actor, kind, n)
    }
}
pub fn restore(world: &mut World, e: Entity, name: &str, amount: i32) {
    let need = world
        .resource::<npc_framework::needs::NeedRegistry>()
        .id(name);
    if let Some(id) = need
        && let Some(mut needs) = world.get_mut::<npc_framework::needs::Needs>(e)
    {
        let v = needs.get_mut(id);
        v.value = v.value.saturating_add(amount).min(100_000);
    }
}
pub fn need(world: &World, e: Entity, name: &str) -> i32 {
    world
        .get_resource::<npc_framework::needs::NeedRegistry>()
        .and_then(|r| r.id(name))
        .and_then(|id| {
            world
                .get::<npc_framework::needs::Needs>(e)
                .map(|n| n.get(id).value)
        })
        .unwrap_or(100_000)
}
pub fn produce(world: &mut World, kind: &str, count: u32) {
    *world
        .resource_mut::<VillageState>()
        .produced
        .entry(kind.into())
        .or_default() += u64::from(count);
}
