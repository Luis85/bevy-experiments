//! What an ordinary resident does for the settlement: carry materials to a
//! site, build, lay surplus in a store and restock the stall. Each function
//! is one bounded step for a `life.*` leaf, and each has a pure plan that the
//! matching tree condition reads, so a guard that passes can start its leaf.
//!
//! A hauler never carries somebody else's property. Goods fetched from a
//! store are bought at the quote and sold on at the quote: to the site, which
//! the civic fund pays for, or through the stall, which credits the hauler.
use super::model::{ConstructionSite, Depot, held};
use super::{depot, sites};
use crate::village::frontier::economy_spec;
use crate::village::{Wallet, ai, catalog, economy as econ, market, production};
use bevy_ecs::prelude::*;
use npc_framework::interaction::Item;
use npc_framework::prelude::*;

type Step = Result<(), &'static str>;

/// Without a coat, a resident carries goods to the boat only above this Warmth.
const BOAT_TRIP_WARMTH: i32 = 60_000;

/// Coins a resident may tie up in goods carried for the settlement.
pub(crate) fn spendable(world: &World, actor: Entity) -> u32 {
    let floor = economy_spec::tuning(world).map_or(0, |tuning| tuning.destitution_floor);
    world
        .get::<Wallet>(actor)
        .map_or(0, |wallet| wallet.coins.saturating_sub(floor))
}

/// Units of `kind` a resident does not give to a building site: what the
/// funded plan wants in the pack, and always a hearth's worth of fuel.
pub(crate) fn kept(world: &World, actor: Entity, kind: &str) -> u32 {
    production::desired_stock(world, actor, kind).max(hearth_reserve(world, actor, kind))
}

/// A hearth's worth of fuel `kind` a resident always keeps: no building site
/// gets it, so the production plan does not count it as supply either.
pub(crate) fn hearth_reserve(world: &World, actor: Entity, kind: &str) -> u32 {
    crate::village::colony::home_heating(world, actor)
        .filter(|home| home.fuel_kind == kind)
        .map_or(0, |home| {
            let normal = economy_spec::tuning(world).map_or(0, |tuning| tuning.winter_fuel_reserve);
            let reserve = crate::village::frontier::settlements::member(world, actor)
                .map_or(normal, |region| {
                    crate::village::colony::fuel_level_reserve(world, region, normal)
                });
            home.fuel_required.max(reserve)
        })
}

/// The largest count up to `wanted` that fits the pack.
pub(crate) fn fitting(world: &World, actor: Entity, kind: &str, wanted: u32) -> u32 {
    (1..=wanted.min(20))
        .rev()
        .find(|count| econ::fit(world, actor, &[Item::new(kind).stack(*count)]).is_ok())
        .unwrap_or(0)
}

pub(crate) fn nearest_first(
    world: &World,
    actor: Entity,
    mut list: Vec<(SimId, Entity)>,
) -> Vec<(SimId, Entity)> {
    let here = world.get::<SimPos>(actor).copied().unwrap_or_default();
    list.sort_by_key(|(id, entity)| {
        (
            world
                .get::<SimPos>(*entity)
                .map_or(i64::MAX, |pos| pos.dist2(here)),
            *id,
        )
    });
    list
}

pub(crate) fn reachable(world: &World, actor: Entity, id: SimId, entity: Entity) -> bool {
    world
        .get::<SimPos>(entity)
        .is_some_and(|pos| !crate::village::routing::avoiding(world, actor, *pos, Some(id)))
}

/// One trip with building materials.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MaterialTrip {
    /// Hand over goods already in the pack.
    Deliver {
        site: SimId,
        kind: String,
        count: u32,
    },
    /// Buy the goods at a store of the settlement first.
    Fetch {
        store: SimId,
        site: SimId,
        kind: String,
        count: u32,
    },
}

/// The materials trip this resident would start now, if any.
pub fn material_trip(world: &World, actor: Entity) -> Option<MaterialTrip> {
    let settlement = depot::home_settlement(world, actor)?;
    let open = sites::open_sites(world, settlement);
    if open.is_empty() {
        return None;
    }
    let me = *world.get::<SimId>(actor)?;
    let now = world.resource::<SimClock>().tick;
    let trip = economy_spec::tuning(world).map_or(8, |tuning| tuning.haul_trip_units);
    let budget = sites::site_budget(world);
    let payable =
        |kind: &str| market::unit_price(world, kind).map_or(0, |price| budget / price.max(1));
    for (site_id, site_entity) in &open {
        let Some(site) = world.get::<ConstructionSite>(*site_entity) else {
            continue;
        };
        if !reachable(world, actor, *site_id, *site_entity) {
            continue;
        }
        // Goods bought for this site are delivered whatever the carrier's own
        // wishes for them are.
        if let Some(hold) = site
            .incoming
            .get(&me)
            .filter(|hold| hold.loaded && hold.until > now)
        {
            let count = hold
                .count
                .min(econ::count(world, actor, &hold.kind))
                .min(site.open(&hold.kind, Some(me), now))
                .min(payable(&hold.kind));
            if count > 0 {
                return Some(MaterialTrip::Deliver {
                    site: *site_id,
                    kind: hold.kind.clone(),
                    count,
                });
            }
        }
        for kind in site.cost.keys() {
            let wanted = site.open(kind, Some(me), now).min(payable(kind)).min(trip);
            if wanted == 0 {
                continue;
            }
            let spare = econ::count(world, actor, kind).saturating_sub(kept(world, actor, kind));
            if spare > 0 {
                return Some(MaterialTrip::Deliver {
                    site: *site_id,
                    kind: kind.clone(),
                    count: spare.min(wanted),
                });
            }
        }
        if !depot::any(world) {
            continue;
        }
        let coins = spendable(world, actor);
        for kind in site.cost.keys() {
            let wanted = site.open(kind, Some(me), now).min(payable(kind)).min(trip);
            let price = market::unit_price(world, kind).unwrap_or(u32::MAX).max(1);
            let wanted = wanted.min(coins / price);
            if wanted == 0 {
                continue;
            }
            for (store_id, store) in nearest_first(world, actor, depot::depots(world, settlement)) {
                if !depot::serves(world, store, actor) || !reachable(world, actor, store_id, store)
                {
                    continue;
                }
                let count = fitting(
                    world,
                    actor,
                    kind,
                    wanted.min(depot::available(world, store, kind, Some(me))),
                );
                if count > 0 {
                    return Some(MaterialTrip::Fetch {
                        store: store_id,
                        site: *site_id,
                        kind: kind.clone(),
                        count,
                    });
                }
            }
        }
    }
    None
}

fn entity(world: &World, id: SimId) -> Result<Entity, &'static str> {
    world
        .resource::<SimIndex>()
        .get(id)
        .ok_or("the target is gone")
}

/// `life.haul materials`: one step of a materials trip. Village sites come
/// first; without one, an order for a played character's homestead upgrade
/// of the settlement (paid from that owner's escrow, `live::haul_step`).
pub(crate) fn haul_materials(world: &mut World, actor: Entity) -> Step {
    let Some(trip) = material_trip(world, actor) else {
        return crate::village::live::haul_step(world, actor).map_err(|_| {
            "no building site or homestead order of this settlement needs materials this \
             resident can bring"
        });
    };
    let me = *world.get::<SimId>(actor).ok_or("no identity")?;
    match trip {
        MaterialTrip::Deliver { site, kind, count } => {
            let target = entity(world, site)?;
            production::intent(world, actor, "haul", &kind, "", Some(site));
            sites::promise(world, target, me, &kind, count, false);
            if !ai::approach_work(world, actor, target) {
                return Ok(());
            }
            sites::deliver(world, actor, target, &kind, count)
                .map_err(|_| "the site refused the delivery")
        }
        MaterialTrip::Fetch {
            store,
            site,
            kind,
            count,
        } => {
            let shelf = entity(world, store)?;
            let target = entity(world, site)?;
            production::intent(world, actor, "fetch", &kind, "", Some(store));
            sites::promise(world, target, me, &kind, count, false);
            depot::reserve(world, shelf, me, &kind, count);
            if !ai::approach_work(world, actor, shelf) {
                return Ok(());
            }
            depot::buy(world, actor, shelf, &kind, count)
                .map_err(|_| "the store refused the purchase")?;
            sites::promise(world, target, me, &kind, count, true);
            Ok(())
        }
    }
}

/// `life.build` on a site: walk there and work one stage. Without a village
/// site, the homestead upgrade this resident was hired for.
pub(crate) fn build(world: &mut World, actor: Entity) -> Step {
    let Some((site, target)) = sites::work_target(world, actor) else {
        return crate::village::live::build_step(world, actor)
            .map_err(|_| "no supplied building site of this settlement needs a builder");
    };
    let me = *world.get::<SimId>(actor).ok_or("no identity")?;
    production::intent(world, actor, "build", "", "", Some(site));
    let walk = world.resource::<SimClock>().tick.saturating_add(
        economy_spec::tuning(world).map_or(1200, |tuning| tuning.haul_reservation_ticks),
    );
    sites::claim(world, target, me, walk);
    if !ai::approach_work(world, actor, target) {
        return Ok(());
    }
    sites::begin_work(world, actor, target).map_err(|_| "the site is not ready for a builder")
}

/// Whether a supplied site of the resident's settlement waits for a builder.
pub fn site_needs_builder(world: &World, actor: Entity) -> bool {
    sites::work_target(world, actor).is_some()
        || crate::village::live::work_target(world, actor).is_some()
}

/// The deposit this resident would make now: `(store, good, units)`.
pub fn surplus_trip(world: &World, actor: Entity) -> Option<(SimId, String, u32)> {
    if !depot::any(world) {
        return None;
    }
    let settlement = depot::home_settlement(world, actor)?;
    let stores = depot::depots(world, settlement);
    if stores.is_empty() {
        return None;
    }
    let me = world.get::<SimId>(actor).copied();
    let now = world.resource::<SimClock>().tick;
    let (working, minimum) = economy_spec::tuning(world).map_or((2, 4), |tuning| {
        (tuning.depot_working_stock, tuning.depot_deposit_min)
    });
    let stores = nearest_first(world, actor, stores);
    for kind in catalog::trade_kinds(world) {
        // Goods the resident has no use for go to the store whole: there is
        // no working stock of them to keep.
        let unused = market::no_use(world, actor, kind);
        let (working, minimum) = if unused { (0, 1) } else { (working, minimum) };
        if econ::count(world, actor, kind) < working.saturating_add(minimum)
            || !depot::storable(world, kind)
        {
            continue;
        }
        let surplus = if unused {
            econ::count(world, actor, kind)
                .saturating_sub(production::desired_stock(world, actor, kind))
        } else {
            market::available_to_sell(world, actor, kind).saturating_sub(working)
        };
        if surplus < minimum {
            continue;
        }
        for (store_id, store) in &stores {
            let room = world
                .get::<Depot>(*store)
                .filter(|state| state.accepts(kind))
                .map_or(0, |state| state.room(me, now));
            if room > 0 && reachable(world, actor, *store_id, *store) {
                return Some((*store_id, (*kind).to_owned(), surplus.min(room).min(100)));
            }
        }
    }
    None
}

/// The trip to the trading post this resident would make for the boat:
/// `(post, good, units)`. While the boat is in (or due today) and buys a good
/// the island holds above its stock targets, a resident of any settlement
/// with unsold stock of it beyond its working stock carries it there.
pub fn boat_trip(world: &World, actor: Entity) -> Option<(SimId, String, u32)> {
    let (post_id, post) = crate::village::merchant::post(world)?;
    // A long walk to the water is not worth a chill: only somebody warm, or
    // wearing a coat, goes.
    if econ::need(world, actor, "Warmth") < BOAT_TRIP_WARMTH
        && !crate::village::living::has_coat(world, actor)
    {
        return None;
    }
    let me = world.get::<SimId>(actor).copied();
    let now = world.resource::<SimClock>().tick;
    let working = economy_spec::tuning(world).map_or(2, |tuning| tuning.depot_working_stock);
    for kind in catalog::trade_kinds(world) {
        if econ::count(world, actor, kind) <= working || !depot::storable(world, kind) {
            continue;
        }
        let wanted = crate::village::merchant::wanted_surplus(world, kind);
        if wanted == 0 {
            continue;
        }
        let surplus = market::available_to_sell(world, actor, kind)
            .saturating_sub(working)
            .min(econ::count(world, actor, kind).saturating_sub(kept(world, actor, kind)));
        if surplus == 0 {
            continue;
        }
        let room = world
            .get::<Depot>(post)
            .filter(|state| state.accepts(kind))
            .map_or(0, |state| state.room(me, now));
        if room > 0 && reachable(world, actor, post_id, post) {
            return Some((
                post_id,
                (*kind).to_owned(),
                surplus.min(room).min(wanted).min(100),
            ));
        }
    }
    None
}

/// `life.haul boat`: carry surplus the boat buys to the trading post.
pub(crate) fn haul_boat(world: &mut World, actor: Entity) -> Step {
    let (post, kind, count) =
        boat_trip(world, actor).ok_or("the boat wants nothing this resident has to spare")?;
    let shelf = entity(world, post)?;
    production::intent(world, actor, "store", &kind, "", Some(post));
    if !ai::approach_work(world, actor, shelf) {
        return Ok(());
    }
    depot::deposit(world, actor, shelf, &kind, count).map_err(|_| "the post refused the goods")
}

/// `life.haul surplus`: carry own surplus to a store of the settlement.
pub(crate) fn haul_surplus(world: &mut World, actor: Entity) -> Step {
    let (store, kind, count) = surplus_trip(world, actor)
        .ok_or("no surplus worth a trip, or no store of this settlement has room")?;
    let shelf = entity(world, store)?;
    production::intent(world, actor, "store", &kind, "", Some(store));
    if !ai::approach_work(world, actor, shelf) {
        return Ok(());
    }
    depot::deposit(world, actor, shelf, &kind, count).map_err(|_| "the store refused the goods")
}

/// One trip that restocks a stall from the settlement's storage.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RestockTrip {
    Deliver {
        stall: SimId,
        kind: String,
        count: u32,
    },
    Fetch {
        store: SimId,
        stall: SimId,
        kind: String,
        count: u32,
    },
}

/// The restocking trip this resident would start now, if any.
pub fn restock_trip(world: &World, actor: Entity) -> Option<RestockTrip> {
    if !depot::any(world) {
        return None;
    }
    let settlement = depot::home_settlement(world, actor)?;
    let all = depot::depots(world, settlement);
    // The trading post at the water is the boat's stall, not the village
    // market: storage is not carried out there (that drew the harbour's food
    // 90 m from its homes and cut its births).
    let post = crate::village::merchant::post(world).map(|(id, _)| id);
    let public = |entity: &Entity| {
        world
            .get::<Depot>(*entity)
            .is_some_and(|state| state.public)
            && world.get::<SimId>(*entity).copied() != post
    };
    if !all.iter().any(|(_, entity)| public(entity)) {
        return None;
    }
    let me = *world.get::<SimId>(actor)?;
    let now = world.resource::<SimClock>().tick;
    let (level, trip) = economy_spec::tuning(world).map_or((4, 8), |tuning| {
        (tuning.stall_restock_level, tuning.haul_trip_units)
    });
    let coins = spendable(world, actor);
    for (stall_id, stall_entity) in all.iter().filter(|(_, entity)| public(entity)) {
        let stall = world.get::<Depot>(*stall_entity)?;
        if !reachable(world, actor, *stall_id, *stall_entity) {
            continue;
        }
        if let Some(hold) = stall
            .incoming
            .get(&me)
            .filter(|hold| hold.loaded && hold.until > now)
        {
            let count = hold
                .count
                .min(econ::count(world, actor, &hold.kind))
                .min(stall.room(Some(me), now));
            if count > 0 {
                return Some(RestockTrip::Deliver {
                    stall: *stall_id,
                    kind: hold.kind.clone(),
                    count,
                });
            }
        }
        let room = stall.room(Some(me), now);
        if room == 0 {
            continue;
        }
        for (store_id, store_entity) in all.iter().filter(|(_, entity)| !public(entity)) {
            let Some(store) = world.get::<Depot>(*store_entity) else {
                continue;
            };
            if !depot::serves(world, *store_entity, actor)
                || !reachable(world, actor, *store_id, *store_entity)
            {
                continue;
            }
            for kind in store.lots.keys() {
                let on_shelf =
                    stall
                        .stock(kind)
                        .saturating_add(held(&stall.incoming, kind, Some(me), now));
                if on_shelf >= level {
                    continue;
                }
                let price = market::unit_price(world, kind).unwrap_or(u32::MAX).max(1);
                let wanted = (level - on_shelf)
                    .min(room)
                    .min(trip)
                    .min(coins / price)
                    .min(store.available(kind, Some(me), now));
                let count = fitting(world, actor, kind, wanted);
                if count > 0 {
                    return Some(RestockTrip::Fetch {
                        store: *store_id,
                        stall: *stall_id,
                        kind: kind.clone(),
                        count,
                    });
                }
            }
        }
    }
    None
}

/// `life.haul restock`: buy from storage, then lay the goods on the stall.
pub(crate) fn haul_restock(world: &mut World, actor: Entity) -> Step {
    let trip = restock_trip(world, actor)
        .ok_or("no stall of this settlement is short of a good its storage holds")?;
    let me = *world.get::<SimId>(actor).ok_or("no identity")?;
    match trip {
        RestockTrip::Deliver { stall, kind, count } => {
            let target = entity(world, stall)?;
            production::intent(world, actor, "restock", &kind, "", Some(stall));
            depot::promise(world, target, me, &kind, count, true);
            if !ai::approach_work(world, actor, target) {
                return Ok(());
            }
            depot::deposit(world, actor, target, &kind, count)
                .map_err(|_| "the stall refused the goods")
        }
        RestockTrip::Fetch {
            store,
            stall,
            kind,
            count,
        } => {
            let shelf = entity(world, store)?;
            let target = entity(world, stall)?;
            production::intent(world, actor, "fetch", &kind, "", Some(store));
            depot::promise(world, target, me, &kind, count, false);
            depot::reserve(world, shelf, me, &kind, count);
            if !ai::approach_work(world, actor, shelf) {
                return Ok(());
            }
            depot::buy(world, actor, shelf, &kind, count)
                .map_err(|_| "the store refused the purchase")?;
            depot::promise(world, target, me, &kind, count, true);
            Ok(())
        }
    }
}
