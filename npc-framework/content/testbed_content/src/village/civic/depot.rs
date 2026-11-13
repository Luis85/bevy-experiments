//! Stores and stalls as sellers of record. Goods lie here on behalf of their
//! owners; a sale credits each owner at the quoted price at once, wherever
//! the owner is. Every transfer moves real goods and existing coins.
use super::model::{CIVIC_OWNER, CivicState, Depot, Hold};
use crate::village::frontier::{building_spec, economy_spec, settlements};
use crate::village::{VillageState, Wallet, economy as econ, living, market};
use bevy_ecs::prelude::*;
use npc_framework::interaction::Item;
use npc_framework::prelude::*;
use serde_json::json;

pub fn is_depot(world: &World, entity: Entity) -> bool {
    world.get::<Depot>(entity).is_some()
}

/// Stores and stalls of a settlement, ascending by identity.
pub fn depots(world: &World, settlement: SimId) -> Vec<(SimId, Entity)> {
    let Some(state) = world.get_resource::<CivicState>() else {
        return Vec::new();
    };
    state
        .depots
        .get(&settlement)
        .into_iter()
        .flatten()
        .filter_map(|id| Some((*id, world.resource::<SimIndex>().get(*id)?)))
        .collect()
}

/// Every store and stall of the world, ascending by identity.
pub fn all_depots(world: &World) -> Vec<(SimId, Entity)> {
    let Some(state) = world.get_resource::<CivicState>() else {
        return Vec::new();
    };
    let mut ids: Vec<SimId> = state.depots.values().flatten().copied().collect();
    ids.sort_unstable();
    ids.into_iter()
        .filter_map(|id| Some((id, world.resource::<SimIndex>().get(id)?)))
        .collect()
}

/// Whether any store or stall exists; lets hot paths skip civic work.
pub fn any(world: &World) -> bool {
    world
        .get_resource::<CivicState>()
        .is_some_and(|state| state.depots.values().any(|ids| !ids.is_empty()))
}

/// A good that may lie in a store: no wear, not worn.
pub fn storable(world: &World, kind: &str) -> bool {
    economy_spec::registry(world)
        .is_some_and(|registry| building_spec::storable(&registry.spec, kind))
}

/// The settlement a resident shops and does chores for: the home settlement,
/// else the one of the workplace.
pub fn home_settlement(world: &World, actor: Entity) -> Option<SimId> {
    settlements::member(world, actor)
        .or_else(|| crate::village::production::economic_settlement(world, actor))
}

/// Whether `buyer` may shop here: anyone at a stall, residents (of any
/// settlement) at storage. A visitor buys at the market, not from the barn.
pub fn serves(world: &World, depot: Entity, buyer: Entity) -> bool {
    world.get::<Depot>(depot).is_some_and(|state| {
        state.public
            || world
                .get::<crate::village::frontier::labor::ResidentProfile>(buyer)
                .is_some()
    })
}

/// Units of `kind` a buyer could take now, after other haulers' claims.
pub fn available(world: &World, depot: Entity, kind: &str, buyer: Option<SimId>) -> u32 {
    let now = world.resource::<SimClock>().tick;
    world
        .get::<Depot>(depot)
        .map_or(0, |state| state.available(kind, buyer, now))
}

/// The validation of a purchase, without changing anything.
pub fn check_buy(
    world: &World,
    buyer: Entity,
    depot: Entity,
    kind: &str,
    count: u32,
) -> Result<market::TradeQuote, String> {
    if !living(world, buyer) {
        return Err("only a living actor can buy".into());
    }
    if !is_depot(world, depot) {
        return Err("target is not a store or stall".into());
    }
    if !(1..=20).contains(&count) {
        return Err("quantity must be 1..=20".into());
    }
    if !econ::near(world, buyer, depot) {
        return Err("move within 1.5 m of the store".into());
    }
    if !serves(world, depot, buyer) {
        return Err("storage serves only residents; buy at a market stall".into());
    }
    let unit_price = market::unit_price(world, kind).ok_or("this good is not traded here")?;
    let total = unit_price.checked_mul(count).ok_or("price overflow")?;
    let id = world.get::<SimId>(buyer).copied();
    if available(world, depot, kind, id) < count {
        return Err("not enough of this good is in store".into());
    }
    if world
        .get::<Wallet>(buyer)
        .is_none_or(|wallet| wallet.coins < total)
    {
        return Err(format!("buyer cannot afford {total} coins"));
    }
    econ::fit(world, buyer, &[Item::new(kind).stack(count)])?;
    Ok(market::TradeQuote { unit_price, total })
}

/// Sells from the shelf at the quoted price. Each owner of the units sold is
/// credited at once; units of the settlement, or of an owner who died or
/// cannot hold more coins, credit the civic fund.
pub fn buy(
    world: &mut World,
    buyer: Entity,
    depot: Entity,
    kind: &str,
    count: u32,
) -> Result<(), String> {
    let quote = check_buy(world, buyer, depot, kind, count)?;
    let buyer_id = *world.get::<SimId>(buyer).expect("checked buyer");
    let depot_id = *world.get::<SimId>(depot).expect("depot identity");
    let lots = {
        let mut state = world.get_mut::<Depot>(depot).expect("checked depot");
        state.holds.remove(&buyer_id);
        state.sold_units = state.sold_units.saturating_add(u64::from(count));
        state.turnover = state.turnover.saturating_add(u64::from(quote.total));
        state.take(kind, count)
    };
    world
        .get_mut::<Wallet>(buyer)
        .expect("checked wallet")
        .coins -= quote.total;
    for (owner, units) in &lots {
        let coins = quote.unit_price * units;
        let payee = (*owner != CIVIC_OWNER)
            .then(|| world.resource::<SimIndex>().get(*owner))
            .flatten()
            .filter(|entity| {
                living(world, *entity)
                    && world
                        .get::<Wallet>(*entity)
                        .is_some_and(|wallet| wallet.coins.checked_add(coins).is_some())
            });
        match payee {
            Some(entity) => {
                world
                    .get_mut::<Wallet>(entity)
                    .expect("checked payee")
                    .coins += coins;
                record_effect(
                    world,
                    Some(*owner),
                    "VillageStallSale",
                    &json!({"store":depot_id,"kind":kind,"quantity":units,"coins":coins,
                        "unit_price":quote.unit_price,"buyer":buyer_id}),
                );
            }
            None if crate::village::merchant::credit_consignment(world, *owner, coins) => {}
            None => {
                let mut state = world.resource_mut::<VillageState>();
                state.fund = state.fund.saturating_add(coins);
            }
        }
    }
    econ::grant(world, buyer, Item::new(kind).stack(count));
    world.resource_mut::<VillageState>().transactions += 1;
    market::record_trade(world, buyer, depot, kind, count, quote.total);
    record_effect(
        world,
        Some(buyer_id),
        "VillageTrade",
        &json!({"buyer":buyer_id,"seller":depot_id,"kind":kind,"quantity":count,
            "coins":quote.total,"unit_price":quote.unit_price,"store":true}),
    );
    Ok(())
}

/// The validation of a deposit, without changing anything.
pub fn check_deposit(
    world: &World,
    owner: Entity,
    depot: Entity,
    kind: &str,
    count: u32,
) -> Result<(), String> {
    if !living(world, owner) || world.get::<SimId>(owner).is_none() {
        return Err("only a living actor can store goods".into());
    }
    let state = world
        .get::<Depot>(depot)
        .ok_or("target is not a store or stall")?;
    if !(1..=100).contains(&count) {
        return Err("quantity must be 1..=100".into());
    }
    if !storable(world, kind) || market::unit_price(world, kind).is_none() {
        return Err("tools, clothing and untraded goods cannot be stored".into());
    }
    if !state.accepts(kind) {
        return Err("this granary stores only the crop of its field".into());
    }
    if !econ::near(world, owner, depot) {
        return Err("move within 1.5 m of the store".into());
    }
    if econ::count(world, owner, kind) < count {
        return Err(format!(
            "carry {count} unreserved {kind} to the store first"
        ));
    }
    let now = world.resource::<SimClock>().tick;
    if state.room(world.get::<SimId>(owner).copied(), now) < count {
        return Err("the store has no room for that many".into());
    }
    Ok(())
}

/// Lays goods in the store on the owner's behalf. They stay the owner's until
/// sold, and the proceeds are the owner's.
pub fn deposit(
    world: &mut World,
    owner: Entity,
    depot: Entity,
    kind: &str,
    count: u32,
) -> Result<(), String> {
    check_deposit(world, owner, depot, kind, count)?;
    econ::consume(world, owner, kind, count)?;
    let owner_id = *world.get::<SimId>(owner).expect("checked owner");
    let depot_id = *world.get::<SimId>(depot).expect("depot identity");
    let mut state = world.get_mut::<Depot>(depot).expect("checked depot");
    state.add(kind, owner_id, count);
    state.incoming.remove(&owner_id);
    record_effect(
        world,
        Some(owner_id),
        "VillageGoodsStored",
        &json!({"store":depot_id,"kind":kind,"quantity":count}),
    );
    Ok(())
}

fn hold_until(world: &World) -> u64 {
    world.resource::<SimClock>().tick.saturating_add(
        economy_spec::tuning(world).map_or(1200, |tuning| tuning.haul_reservation_ticks),
    )
}

/// Sets `count` of `kind` aside for a hauler who is on the way.
pub(crate) fn reserve(world: &mut World, depot: Entity, hauler: SimId, kind: &str, count: u32) {
    let until = hold_until(world);
    if let Some(mut state) = world.get_mut::<Depot>(depot) {
        state.holds.insert(
            hauler,
            Hold {
                kind: kind.into(),
                count,
                until,
                loaded: false,
            },
        );
    }
}

/// Registers a restocking delivery a hauler is bringing.
pub(super) fn promise(
    world: &mut World,
    depot: Entity,
    hauler: SimId,
    kind: &str,
    count: u32,
    loaded: bool,
) {
    let until = hold_until(world);
    if let Some(mut state) = world.get_mut::<Depot>(depot) {
        let loaded = loaded
            || state
                .incoming
                .get(&hauler)
                .is_some_and(|hold| hold.loaded && hold.kind == kind);
        state.incoming.insert(
            hauler,
            Hold {
                kind: kind.into(),
                count,
                until,
                loaded,
            },
        );
    }
}

/// Drops claims that ran out.
pub(super) fn expire(world: &mut World, settlement: SimId, now: u64) {
    for (_, entity) in depots(world, settlement) {
        let stale = world.get::<Depot>(entity).is_some_and(|state| {
            state
                .holds
                .values()
                .chain(state.incoming.values())
                .any(|hold| hold.until <= now)
        });
        if stale && let Some(mut state) = world.get_mut::<Depot>(entity) {
            state.holds.retain(|_, hold| hold.until > now);
            state.incoming.retain(|_, hold| hold.until > now);
        }
    }
}

/// Goods on the shelves of a settlement, as supply for the funded plan:
/// `(store, good, units)`.
pub fn supply(world: &World, settlement: SimId) -> Vec<(SimId, String, u32)> {
    depots(world, settlement)
        .into_iter()
        .flat_map(|(id, entity)| {
            world
                .get::<Depot>(entity)
                .into_iter()
                .flat_map(move |state| {
                    state
                        .lots
                        .keys()
                        .map(move |kind| (id, kind.clone(), state.stock(kind)))
                })
        })
        .collect()
}
