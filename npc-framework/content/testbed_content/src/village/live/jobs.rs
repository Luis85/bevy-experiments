//! Physical work on a played character's homestead (living world only).
//!
//! An upgrade is a private building site paid from its owner's escrow. Every
//! effect on it happens at the door, by somebody who walked there:
//!
//! - **Orders.** A resident of the homestead's settlement with nothing more
//!   urgent in its work hours (`life.haul materials`, after the village's own
//!   sites) fills an open order: from spare goods in its pack, or bought at a
//!   store of the settlement on the way. It walks to the door and hands the
//!   goods over; the escrow then pays it the order's price (the quote of the
//!   day the order was placed plus `haul_fee_percent`).
//! - **The hired builder** walks to the door in its work hours (`life.build`,
//!   after the village's own sites) and works there in stages of
//!   `build_stage_ticks`; each finished stage is paid `builder_wage` from the
//!   escrow. A stage it is called away from is not paid.
//! - **A friend** at a helping tier, in its free time and at most once a day
//!   (`life.help`), walks over and brings one material the upgrade misses,
//!   works one stage at the door, or harvests a ripe bed of the owner's plot.
//!
//! The civic planner never sees these sites: they are not in `CivicState`,
//! the fund pays nothing for them, and the planner neither commissions nor
//! cancels them.
use super::{
    Book, HOMESTEAD_REACH_MM, Help, LiveSpec, LiveState, NEEDS, Upgrade, UpgradeWork, booked,
    chest_capacity, entity, garden_bed, homestead, homestead_settlement, is_resident, players,
    score, spec, stage_ticks, tier_of,
};
use crate::village::civic::{self, Hold};
use crate::village::frontier::economy_spec;
use crate::village::{
    Gathering, Villager, Wallet, ai, economy as econ, living, market, production,
};
use bevy_ecs::prelude::*;
use npc_framework::crafting::Crafting;
use npc_framework::interaction::Item;
use npc_framework::prelude::*;
use serde::Serialize;
use serde_json::json;
use std::collections::BTreeMap;

type Step = Result<(), &'static str>;

fn now(world: &World) -> u64 {
    world.resource::<SimClock>().tick
}

fn claim_ticks(world: &World) -> u64 {
    economy_spec::tuning(world).map_or(1200, |tuning| tuning.haul_reservation_ticks)
}

/// Units of `kind` promised by holders other than `except`.
fn held(holds: &BTreeMap<SimId, Hold>, kind: &str, except: Option<SimId>, now: u64) -> u32 {
    civic::held(holds, kind, except, now)
}

/// Units of an order still to be brought that nobody but `me` is bringing.
fn open_order(upgrade: &Upgrade, kind: &str, me: Option<SimId>, now: u64) -> u32 {
    upgrade
        .ordered
        .get(kind)
        .copied()
        .unwrap_or(0)
        .min(upgrade.missing(kind))
        .saturating_sub(held(&upgrade.incoming, kind, me, now))
}

/// Units the open homestead orders of `settlement` still wait for, nobody
/// bringing them yet: demand for the production plan like a civic site's
/// order, so a carpenter makes the planks a player ordered when nobody has
/// any to spare.
pub(crate) fn order_demand(world: &World, settlement: SimId) -> BTreeMap<String, u32> {
    let mut goods = BTreeMap::new();
    let (Some(_), Some(state)) = (spec(world), world.get_resource::<LiveState>()) else {
        return goods;
    };
    let now = now(world);
    for (owner, upgrade) in &state.upgrades {
        if homestead_settlement(world, *owner) != Some(settlement) {
            continue;
        }
        for kind in upgrade.ordered.keys() {
            let open = open_order(upgrade, kind, None, now);
            if open > 0 {
                *goods.entry(kind.clone()).or_default() += open;
            }
        }
    }
    goods
}

/// Coins of the escrow that pay for `count` units of the open order of
/// `kind`: its share of what the order holds, all of it for the last units.
pub(super) fn order_pay(upgrade: &Upgrade, kind: &str, count: u32) -> u32 {
    let ordered = upgrade.ordered.get(kind).copied().unwrap_or(0);
    let coins = upgrade.order_coins.get(kind).copied().unwrap_or(0);
    if ordered == 0 || count == 0 {
        0
    } else if count >= ordered {
        coins
    } else {
        (u64::from(coins) * u64::from(count) / u64::from(ordered)) as u32
    }
}

/// The owner's homestead door, when the resident would work there: same
/// settlement as the resident, the owner alive.
fn door_for(world: &World, actor: Entity, owner: SimId) -> Option<SimPos> {
    let settlement = civic::home_settlement(world, actor)?;
    if homestead_settlement(world, owner) != Some(settlement) {
        return None;
    }
    homestead(world, owner).map(|(_, door)| door)
}

fn house_id(world: &World, owner: SimId) -> Option<SimId> {
    homestead(world, owner).and_then(|(house, _)| world.get::<SimId>(house).copied())
}

/// One trip with materials for a homestead order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HomesteadTrip {
    /// Carry goods already in the pack to the owner's door.
    Deliver {
        owner: SimId,
        kind: String,
        count: u32,
    },
    /// Buy the goods at a store of the settlement first.
    Fetch {
        store: SimId,
        owner: SimId,
        kind: String,
        count: u32,
    },
}

/// The homestead order this resident would carry now, if any.
pub fn homestead_trip(world: &World, actor: Entity) -> Option<HomesteadTrip> {
    spec(world)?;
    let state = world.get_resource::<LiveState>()?;
    if state
        .upgrades
        .values()
        .all(|upgrade| upgrade.ordered.is_empty())
    {
        return None;
    }
    if !is_resident(world, actor) {
        return None;
    }
    let me = *world.get::<SimId>(actor)?;
    let settlement = civic::home_settlement(world, actor)?;
    let now = now(world);
    let trip = economy_spec::tuning(world).map_or(8, |tuning| tuning.haul_trip_units);
    for (owner, upgrade) in &state.upgrades {
        if *owner == me || upgrade.ordered.is_empty() || door_for(world, actor, *owner).is_none() {
            continue;
        }
        if let Some(hold) = upgrade
            .incoming
            .get(&me)
            .filter(|hold| hold.loaded && hold.until > now)
        {
            let count = hold
                .count
                .min(econ::count(world, actor, &hold.kind))
                .min(open_order(upgrade, &hold.kind, Some(me), now));
            if count > 0 {
                return Some(HomesteadTrip::Deliver {
                    owner: *owner,
                    kind: hold.kind.clone(),
                    count,
                });
            }
        }
        for kind in upgrade.ordered.keys() {
            let open = open_order(upgrade, kind, Some(me), now).min(trip);
            if open == 0 {
                continue;
            }
            // The hauler sells its own spare stock to the order.
            // What it does not keep for itself, or what it offers for sale
            // anyway (a woodcutter's logs).
            let carried = econ::count(world, actor, kind);
            let offered = if market::person_sells(world, actor, kind) {
                market::available_to_sell(world, actor, kind).min(carried)
            } else {
                0
            };
            let spare = carried
                .saturating_sub(civic::kept(world, actor, kind))
                .max(offered);
            if spare > 0 {
                return Some(HomesteadTrip::Deliver {
                    owner: *owner,
                    kind: kind.clone(),
                    count: spare.min(open),
                });
            }
            // Or buys at a store, when the order pays at least the quote.
            let Some(price) = market::unit_price(world, kind) else {
                continue;
            };
            if order_pay(upgrade, kind, 1) < price {
                continue;
            }
            let wanted = open.min(civic::spendable(world, actor) / price.max(1));
            if wanted == 0 {
                continue;
            }
            for (store_id, store) in
                civic::nearest_first(world, actor, civic::depots(world, settlement))
            {
                if !civic::serves(world, store, actor)
                    || !civic::reachable(world, actor, store_id, store)
                {
                    continue;
                }
                let count = civic::fitting(
                    world,
                    actor,
                    kind,
                    wanted.min(civic::depot_available(world, store, kind, Some(me))),
                );
                if count > 0 {
                    return Some(HomesteadTrip::Fetch {
                        store: store_id,
                        owner: *owner,
                        kind: kind.clone(),
                        count,
                    });
                }
            }
        }
    }
    None
}

/// Notes that `hauler` is bringing `count` of `kind` to `owner`'s upgrade.
fn promise(world: &mut World, owner: SimId, hauler: SimId, kind: &str, count: u32, loaded: bool) {
    let claim = claim_ticks(world);
    let until = now(world).saturating_add(claim);
    let mut state = world.resource_mut::<LiveState>();
    let Some(upgrade) = state.upgrades.get_mut(&owner) else {
        return;
    };
    let loaded = loaded
        || upgrade
            .incoming
            .get(&hauler)
            .is_some_and(|hold| hold.loaded && hold.kind == kind);
    let hold = Hold {
        kind: kind.into(),
        count,
        until,
        loaded,
    };
    // Renewed when it changes, or once half its time has run.
    let half = until.saturating_sub(claim / 2);
    if upgrade.incoming.get(&hauler).is_none_or(|old| {
        old.kind != hold.kind || old.count != count || old.loaded != loaded || old.until < half
    }) {
        upgrade.incoming.insert(hauler, hold);
    }
}

/// Hands `count` of `kind` from `by`'s pack to `owner`'s upgrade at the door.
/// Units an open order covers are paid from the escrow at the order's price;
/// anything else is a gift. Returns the coins paid.
fn hand_over(
    world: &mut World,
    owner: SimId,
    by: Entity,
    kind: &str,
    count: u32,
) -> Result<u32, &'static str> {
    let me = *world.get::<SimId>(by).ok_or("no identity")?;
    let upgrade = world
        .resource::<LiveState>()
        .upgrades
        .get(&owner)
        .cloned()
        .ok_or("the upgrade is gone")?;
    let count = count.min(upgrade.missing(kind));
    if count == 0 {
        return Err("the upgrade does not need it any more");
    }
    econ::consume(world, by, kind, count).map_err(|_| "the goods are not in the pack")?;
    let ordered = upgrade.ordered.get(kind).copied().unwrap_or(0).min(count);
    let pay = order_pay(&upgrade, kind, ordered);
    {
        let mut state = world.resource_mut::<LiveState>();
        let site = state.upgrades.get_mut(&owner).expect("upgrade");
        *site.delivered.entry(kind.into()).or_default() += count;
        site.incoming.remove(&me);
        site.escrow -= pay;
        if let Some(coins) = site.order_coins.get_mut(kind) {
            *coins -= pay;
        }
        if let Some(open) = site.ordered.get_mut(kind) {
            *open -= ordered;
        }
        site.ordered.retain(|_, units| *units > 0);
        site.order_coins.retain(|_, coins| *coins > 0);
    }
    if pay > 0 {
        world.get_mut::<Wallet>(by).expect("wallet").coins += pay;
        if let Some(buyer) = entity(world, owner) {
            market::record_trade(world, buyer, by, kind, ordered, pay);
        }
    }
    let supplied = world
        .resource::<LiveState>()
        .upgrades
        .get(&owner)
        .is_some_and(Upgrade::supplied);
    record_effect(
        world,
        Some(me),
        "VillageHomesteadDelivered",
        &json!({"by":me,"for":owner,"goods":{kind: count},"coins":pay,"supplied":supplied}),
    );
    crate::village::colony_demand_changed(world);
    Ok(pay)
}

/// `life.haul materials` for a homestead order: one step of the trip.
pub(crate) fn haul(world: &mut World, actor: Entity) -> Step {
    let trip = homestead_trip(world, actor).ok_or("no homestead order this resident can fill")?;
    let me = *world.get::<SimId>(actor).ok_or("no identity")?;
    match trip {
        HomesteadTrip::Deliver { owner, kind, count } => {
            let door = door_for(world, actor, owner).ok_or("the homestead is gone")?;
            production::intent(world, actor, "haul", &kind, "", house_id(world, owner));
            promise(world, owner, me, &kind, count, true);
            if !ai::travel(world, actor, door) {
                return Ok(());
            }
            hand_over(world, owner, actor, &kind, count).map(|_| ())
        }
        HomesteadTrip::Fetch {
            store,
            owner,
            kind,
            count,
        } => {
            let shelf = entity(world, store).ok_or("the store is gone")?;
            production::intent(world, actor, "fetch", &kind, "", Some(store));
            promise(world, owner, me, &kind, count, false);
            civic::depot_reserve(world, shelf, me, &kind, count);
            if !ai::approach_work(world, actor, shelf) {
                return Ok(());
            }
            civic::depot_buy(world, actor, shelf, &kind, count)
                .map_err(|_| "the store refused the purchase")?;
            promise(world, owner, me, &kind, count, true);
            Ok(())
        }
    }
}

/// The homestead upgrade this resident was hired to build and can work on
/// now: supplied, with work and paid stages left.
pub fn work_target(world: &World, actor: Entity) -> Option<SimId> {
    spec(world)?;
    let me = *world.get::<SimId>(actor)?;
    world
        .get_resource::<LiveState>()?
        .upgrades
        .iter()
        .find(|(owner, upgrade)| {
            upgrade.builder == Some(me)
                && upgrade.stages > 0
                && upgrade.supplied()
                && upgrade.remaining_ticks > 0
                && door_for(world, actor, **owner).is_some()
        })
        .map(|(owner, _)| *owner)
}

fn working(world: &World, actor: Entity) -> bool {
    world.get::<UpgradeWork>(actor).is_some()
        || world.get::<civic::SiteWork>(actor).is_some()
        || world.get::<Gathering>(actor).is_some()
        || world.get::<Crafting>(actor).is_some()
        || world
            .get::<crate::village::hunting::Hunting>(actor)
            .is_some()
        || world
            .get::<crate::village::homestead::CottageBuild>(actor)
            .is_some()
}

/// Starts one stage of work at the door: up to `ticks` of the work left.
fn begin(world: &mut World, actor: Entity, owner: SimId, ticks: u64, hired: bool) -> Step {
    if working(world, actor) {
        return Err("already working");
    }
    let door = homestead(world, owner)
        .map(|(_, door)| door)
        .ok_or("the homestead is gone")?;
    let here = world.get::<SimPos>(actor).copied().ok_or("no position")?;
    if here.dist2(door) > HOMESTEAD_REACH_MM.pow(2) {
        return Err("not at the door");
    }
    let remaining = world
        .resource::<LiveState>()
        .upgrades
        .get(&owner)
        .filter(|upgrade| upgrade.supplied())
        .map(|upgrade| upgrade.remaining_ticks)
        .ok_or("the upgrade waits for materials")?;
    let ticks = ticks.min(remaining);
    if ticks == 0 {
        return Err("no work is left");
    }
    let pace = u64::from(crate::village::colony::work_pace_percent(world, actor));
    let started = now(world);
    let ends = started.saturating_add((ticks * pace / 100).max(1));
    world
        .entity_mut(actor)
        .remove::<MoveIntent>()
        .insert(UpgradeWork {
            owner,
            ends,
            started,
            ticks: ticks as u32,
            hired,
        });
    if hired && let Some(upgrade) = world.resource_mut::<LiveState>().upgrades.get_mut(&owner) {
        upgrade.builder_until = ends;
    }
    Ok(())
}

/// `life.build` on a homestead upgrade: walk to the door and work a stage.
pub(crate) fn build(world: &mut World, actor: Entity) -> Step {
    let owner = work_target(world, actor).ok_or("no homestead upgrade waits for this builder")?;
    let door = door_for(world, actor, owner).ok_or("the homestead is gone")?;
    production::intent(world, actor, "build", "", "", house_id(world, owner));
    let now = now(world);
    // The claim is renewed at every step of the way: a long walk stays
    // "walking", it does not lapse back to "hired" half way.
    let until = now.saturating_add(claim_ticks(world));
    if let Some(upgrade) = world.resource_mut::<LiveState>().upgrades.get_mut(&owner) {
        upgrade.builder_until = upgrade.builder_until.max(until);
    }
    // Within the door's reach is at the door: the owner (or a friend) may
    // stand on the door point itself, and the builder joins the work.
    let at_door = world
        .get::<SimPos>(actor)
        .is_some_and(|here| here.dist2(door) <= HOMESTEAD_REACH_MM.pow(2));
    if !at_door && !ai::travel(world, actor, door) {
        return Ok(());
    }
    begin(world, actor, owner, stage_ticks(world), true)
}

/// The errand a friend would run now, if any: the one it is on, else the
/// first homestead of a played friend that needs a hand.
pub fn help_trip(world: &World, actor: Entity) -> Option<Help> {
    use crate::village::resident::BlockKind;
    let spec = spec(world)?;
    let state = world.get_resource::<LiveState>()?;
    let me = *world.get::<SimId>(actor)?;
    let now = now(world);
    if let Some(help) = state.helpers.get(&me).filter(|help| help.until > now) {
        return Some(help.clone());
    }
    if !is_resident(world, actor)
        || !crate::village::resident::in_block(world, actor, BlockKind::Leisure)
        || !NEEDS
            .iter()
            .all(|need| econ::need(world, actor, need) > spec.help_needs_above)
    {
        return None;
    }
    let settlement = civic::home_settlement(world, actor)?;
    let day = crate::village::calendar(world).0;
    let until = now.saturating_add(claim_ticks(world));
    for (owner, _) in players(world) {
        let (tier, _) = tier_of(world, score(world, owner, me));
        if !spec.tiers.get(tier).is_some_and(|tier| tier.helps)
            || state
                .bonds
                .get(&owner)
                .and_then(|bonds| bonds.get(&me))
                .is_some_and(|bond| bond.helped_day == Some(day))
            || door_for(world, actor, owner).is_none()
        {
            continue;
        }
        let errand = |how: &str| Help {
            owner,
            how: how.into(),
            kind: String::new(),
            store: None,
            plot: None,
            bed: 0,
            until,
        };
        if let Some(upgrade) = state.upgrades.get(&owner) {
            for kind in upgrade.cost.keys() {
                if upgrade.missing(kind).saturating_sub(held(
                    &upgrade.incoming,
                    kind,
                    Some(me),
                    now,
                )) == 0
                {
                    continue;
                }
                if econ::count(world, actor, kind) > 0 {
                    return Some(Help {
                        kind: kind.clone(),
                        ..errand("material")
                    });
                }
                let Some(price) = market::unit_price(world, kind) else {
                    continue;
                };
                if civic::spendable(world, actor) < price {
                    continue;
                }
                let store = civic::nearest_first(world, actor, civic::depots(world, settlement))
                    .into_iter()
                    .find(|(id, store)| {
                        civic::serves(world, *store, actor)
                            && civic::reachable(world, actor, *id, *store)
                            && civic::depot_available(world, *store, kind, Some(me)) > 0
                            && civic::fitting(world, actor, kind, 1) == 1
                    });
                if let Some((store, _)) = store {
                    return Some(Help {
                        kind: kind.clone(),
                        store: Some(store),
                        ..errand("material")
                    });
                }
            }
            if upgrade.supplied() && upgrade.remaining_ticks > 1 {
                return Some(errand("work"));
            }
        }
        for (plot_id, plot) in crate::village::homestead::plots(world) {
            if !world
                .get::<crate::village::homestead::HomesteadPlot>(plot)
                .is_some_and(|estate| estate.owner == Some(owner))
            {
                continue;
            }
            if let Some(bed) = garden_bed(world, plot, "harvest") {
                return Some(Help {
                    plot: Some(plot_id),
                    bed,
                    ..errand("harvest")
                });
            }
        }
    }
    None
}

/// The friend's errand is done: once a day it counts as help.
fn helped(world: &mut World, friend: SimId, owner: SimId, done: serde_json::Value) {
    let day = crate::village::calendar(world).0;
    let mut state = world.resource_mut::<LiveState>();
    state.helpers.remove(&friend);
    if let Some(bond) = state
        .bonds
        .get_mut(&owner)
        .and_then(|bonds| bonds.get_mut(&friend))
    {
        bond.helped_day = Some(day);
    }
    record_effect(world, Some(friend), "VillageFriendHelped", &done);
}

/// `life.help`: one step of a friend's errand.
pub(crate) fn help(world: &mut World, actor: Entity) -> Step {
    let errand = help_trip(world, actor).ok_or("no friend's homestead needs a hand")?;
    let me = *world.get::<SimId>(actor).ok_or("no identity")?;
    let owner = errand.owner;
    world
        .resource_mut::<LiveState>()
        .helpers
        .entry(me)
        .or_insert_with(|| errand.clone());
    let door = homestead(world, owner)
        .map(|(_, door)| door)
        .ok_or("the homestead is gone")?;
    match errand.how.as_str() {
        "material" => {
            let loaded = econ::count(world, actor, &errand.kind) > 0
                && world
                    .resource::<LiveState>()
                    .upgrades
                    .get(&owner)
                    .and_then(|upgrade| upgrade.incoming.get(&me))
                    .is_some_and(|hold| hold.loaded);
            if let Some(store) = errand.store
                && !loaded
            {
                // Buy the one unit first.
                let shelf = entity(world, store).ok_or("the store is gone")?;
                promise(world, owner, me, &errand.kind, 1, false);
                civic::depot_reserve(world, shelf, me, &errand.kind, 1);
                if !ai::approach_work(world, actor, shelf) {
                    return Ok(());
                }
                civic::depot_buy(world, actor, shelf, &errand.kind, 1)
                    .map_err(|_| "the store refused the purchase")?;
            }
            promise(world, owner, me, &errand.kind, 1, true);
            if !ai::travel(world, actor, door) {
                return Ok(());
            }
            let coins = match hand_over(world, owner, actor, &errand.kind, 1) {
                Ok(coins) => coins,
                Err(reason) => {
                    // Somebody else brought it first: the errand is off.
                    world.resource_mut::<LiveState>().helpers.remove(&me);
                    return Err(reason);
                }
            };
            helped(
                world,
                me,
                owner,
                json!({"to":owner,"how":"material","kind":errand.kind,"quantity":1,
                    "coins":coins,"bought":errand.store}),
            );
            Ok(())
        }
        "work" => {
            if !ai::travel(world, actor, door) {
                return Ok(());
            }
            let left = world
                .resource::<LiveState>()
                .upgrades
                .get(&owner)
                .map_or(0, |upgrade| upgrade.remaining_ticks);
            let ticks = spec(world)
                .map_or(0, |spec| u64::from(spec.help_work_ticks))
                .min(left.saturating_sub(1));
            if ticks == 0 {
                world.resource_mut::<LiveState>().helpers.remove(&me);
                return Err("the work is all but done");
            }
            begin(world, actor, owner, ticks, false)?;
            // The errand lasts while the stage does.
            let ends = world.get::<UpgradeWork>(actor).map_or(0, |work| work.ends);
            if let Some(help) = world.resource_mut::<LiveState>().helpers.get_mut(&me) {
                help.until = ends.saturating_add(1);
            }
            Ok(())
        }
        _ => harvest(world, actor, me, &errand),
    }
}

/// A friend walks to a ripe bed of the owner's plot and harvests it into the
/// chest, or the owner's pack.
fn harvest(world: &mut World, actor: Entity, me: SimId, errand: &Help) -> Step {
    use crate::village::homestead::{self, HomesteadPlot};
    let owner = errand.owner;
    let plot = errand
        .plot
        .and_then(|id| entity(world, id))
        .ok_or("the plot is gone")?;
    if garden_bed(world, plot, "harvest").is_none_or(|bed| bed != errand.bed)
        && world
            .get::<HomesteadPlot>(plot)
            .and_then(|estate| estate.beds.get(errand.bed as usize))
            .is_none_or(|bed| {
                bed.as_ref()
                    .is_none_or(|crop| crop.ready_at.is_none_or(|ready| ready > now(world)))
            })
    {
        world.resource_mut::<LiveState>().helpers.remove(&me);
        return Err("the bed is not ripe any more");
    }
    let place = homestead::bed_position(world, plot, errand.bed).ok_or("no such bed")?;
    if !ai::travel(world, actor, place) {
        return Ok(());
    }
    let kind = world
        .get::<HomesteadPlot>(plot)
        .and_then(|estate| estate.beds.get(errand.bed as usize)?.as_ref())
        .map(|crop| crop.kind.clone())
        .ok_or("the bed is empty")?;
    let definition = homestead::crop_definition(world, &kind).ok_or("unknown crop")?;
    let (good, count) = (definition.yield_kind, definition.yield_count);
    let room = chest_capacity(world, owner).saturating_sub(
        world
            .resource::<LiveState>()
            .chests
            .get(&owner)
            .map_or(0, |chest| chest.values().sum()),
    );
    let into_chest = room >= count;
    let owner_actor = entity(world, owner).ok_or("the owner is gone")?;
    if !into_chest && econ::fit(world, owner_actor, &[Item::new(good).stack(count)]).is_err() {
        world.resource_mut::<LiveState>().helpers.remove(&me);
        return Err("neither the chest nor the owner's pack has room");
    }
    world.get_mut::<HomesteadPlot>(plot).expect("plot").beds[errand.bed as usize] = None;
    if into_chest {
        *world
            .resource_mut::<LiveState>()
            .chests
            .entry(owner)
            .or_default()
            .entry(good.to_owned())
            .or_default() += count;
    } else {
        econ::grant(world, owner_actor, Item::new(good).stack(count));
    }
    econ::produce(world, good, count);
    booked(world, owner, Book::Received, good, count);
    helped(
        world,
        me,
        owner,
        json!({"to":owner,"how":"harvest","plot":errand.plot,"bed":errand.bed,"kind":good,
            "quantity":count,"into":if into_chest {"chest"} else {"pack"}}),
    );
    Ok(())
}

/// Leaves a stage of homestead work unfinished and unpaid.
pub(crate) fn cancel_work(world: &mut World, actor: Entity) {
    // A read first: profiles without live mode never register the type.
    if world.get::<UpgradeWork>(actor).is_none() {
        return;
    }
    let Some(work) = world.entity_mut(actor).take::<UpgradeWork>() else {
        return;
    };
    let me = world.get::<SimId>(actor).copied();
    let mut state = world.resource_mut::<LiveState>();
    if let Some(upgrade) = state.upgrades.get_mut(&work.owner)
        && work.hired
    {
        upgrade.builder_until = 0;
    }
    if let Some(me) = me {
        state.helpers.remove(&me);
    }
}

/// Everybody working at, or on an errand to, `owner`'s homestead stops:
/// the upgrade is finished or abandoned. Returns who was released.
pub(super) fn release(world: &mut World, owner: SimId) -> Vec<SimId> {
    let mut workers: Vec<(SimId, Entity)> = world
        .try_query::<(Entity, &SimId, &UpgradeWork)>()
        .map(|mut query| {
            query
                .iter(world)
                .filter(|(_, _, work)| work.owner == owner)
                .map(|(entity, id, _)| (*id, entity))
                .collect()
        })
        .unwrap_or_default();
    workers.sort_unstable_by_key(|(id, _)| *id);
    for (_, worker) in &workers {
        world.entity_mut(*worker).remove::<UpgradeWork>();
    }
    let mut released: Vec<SimId> = workers.into_iter().map(|(id, _)| id).collect();
    let mut state = world.resource_mut::<LiveState>();
    let errands: Vec<SimId> = state
        .helpers
        .iter()
        .filter(|(_, help)| help.owner == owner)
        .map(|(friend, _)| *friend)
        .collect();
    for friend in errands {
        state.helpers.remove(&friend);
        released.push(friend);
    }
    released.sort_unstable();
    released.dedup();
    released
}

/// Once per tick: stages that ended are credited (and the builder's paid),
/// lapsed claims are dropped and a dead builder is replaced.
pub(super) fn tick(world: &mut World, spec: &LiveSpec) {
    let now = now(world);
    let mut workers: Vec<(SimId, Entity, UpgradeWork)> = world
        .try_query::<(Entity, &SimId, &UpgradeWork)>()
        .map(|mut query| {
            query
                .iter(world)
                .map(|(entity, id, work)| (*id, entity, *work))
                .collect()
        })
        .unwrap_or_default();
    workers.sort_unstable_by_key(|(id, _, _)| *id);
    for (id, actor, work) in workers {
        if !living(world, actor) {
            cancel_work(world, actor);
            continue;
        }
        if work.ends > now {
            continue;
        }
        world.entity_mut(actor).remove::<UpgradeWork>();
        let paid = {
            let mut state = world.resource_mut::<LiveState>();
            let Some(upgrade) = state.upgrades.get_mut(&work.owner) else {
                continue;
            };
            let done = u64::from(work.ticks).min(upgrade.remaining_ticks);
            upgrade.remaining_ticks -= done;
            let wage = if work.hired && upgrade.stages > 0 {
                upgrade.stages -= 1;
                upgrade.builder_until = 0;
                spec.builder_wage.min(upgrade.wages_held())
            } else {
                0
            };
            upgrade.escrow -= wage;
            (done, wage, upgrade.remaining_ticks)
        };
        let (done, wage, left) = paid;
        if work.hired {
            world.get_mut::<Wallet>(actor).expect("wallet").coins += wage;
            record_effect(
                world,
                Some(id),
                "VillageHomesteadBuilderWorked",
                &json!({"for":work.owner,"ticks":done,"wage":wage,"remaining_ticks":left}),
            );
        } else {
            helped(
                world,
                id,
                work.owner,
                json!({"to":work.owner,"how":"work","ticks":done,"remaining_ticks":left}),
            );
        }
    }
    if !now.is_multiple_of(20) {
        return;
    }
    // Claims that ran out, and builders who died.
    let owners: Vec<SimId> = world
        .resource::<LiveState>()
        .upgrades
        .keys()
        .copied()
        .collect();
    for owner in owners {
        let builder = world.resource::<LiveState>().upgrades[&owner].builder;
        let gone = builder.is_some_and(|id| entity(world, id).is_none_or(|e| !living(world, e)));
        let replacement = if gone {
            super::builder_for(world, owner)
        } else {
            builder
        };
        let mut state = world.resource_mut::<LiveState>();
        let upgrade = state.upgrades.get_mut(&owner).expect("upgrade");
        if upgrade.incoming.values().any(|hold| hold.until <= now) {
            upgrade.incoming.retain(|_, hold| hold.until > now);
        }
        if gone {
            upgrade.builder = replacement;
            upgrade.builder_until = 0;
        }
    }
    let mut state = world.resource_mut::<LiveState>();
    if state.helpers.values().any(|help| help.until <= now) {
        state.helpers.retain(|_, help| help.until > now);
    }
}

/// Somebody on the way to a homestead upgrade or working there.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct UpgradeWorkerView {
    pub id: SimId,
    pub name: String,
    /// `hauler`, `builder` or `friend`.
    pub job: &'static str,
    /// `fetching` (buying at a store), `bringing`, `walking` (to the door,
    /// moving), `working`, `harvesting`, `waiting` (a hired builder on its
    /// way who stands still, between stages) or `hired` (waiting for its
    /// work hours).
    pub state: &'static str,
    /// Goods being brought, if any.
    pub kind: String,
    pub count: u32,
    /// One sentence, for example `Oak is bringing 2 log.`
    pub text: String,
}

fn name_of(world: &World, id: SimId) -> String {
    entity(world, id)
        .and_then(|actor| world.get::<Villager>(actor))
        .map_or_else(
            || format!("Resident {}", id.0),
            |person| person.name.clone(),
        )
}

/// Who is working on `owner`'s upgrade or on the way to it, by id.
pub(super) fn workers(world: &World, owner: SimId) -> Vec<UpgradeWorkerView> {
    let Some(state) = world.get_resource::<LiveState>() else {
        return Vec::new();
    };
    let Some(upgrade) = state.upgrades.get(&owner) else {
        return Vec::new();
    };
    let now = now(world);
    let mut out: BTreeMap<SimId, UpgradeWorkerView> = BTreeMap::new();
    let at_work: BTreeMap<SimId, UpgradeWork> = world
        .try_query::<(&SimId, &UpgradeWork)>()
        .map(|mut query| {
            query
                .iter(world)
                .filter(|(_, work)| work.owner == owner)
                .map(|(id, work)| (*id, *work))
                .collect()
        })
        .unwrap_or_default();
    let view = |id: SimId, job: &'static str, state: &'static str, kind: &str, count: u32| {
        let name = name_of(world, id);
        let text = match state {
            "fetching" => format!("{name} is buying {count} {kind} for you at the store."),
            "bringing" => format!("{name} is bringing {count} {kind}."),
            "walking" => format!("{name} is on the way to work on your homestead."),
            "waiting" => format!("{name} is between stages of work on your homestead."),
            "working" if job == "builder" => format!("{name} is building at your door."),
            "working" => format!("{name} is lending a hand at your door."),
            "harvesting" => format!("{name} is harvesting your garden."),
            _ => format!("{name} is hired and will come in working hours."),
        };
        UpgradeWorkerView {
            id,
            name,
            job,
            state,
            kind: kind.into(),
            count,
            text,
        }
    };
    for (hauler, hold) in &upgrade.incoming {
        if hold.until <= now {
            continue;
        }
        let job = if state.helpers.contains_key(hauler) {
            "friend"
        } else {
            "hauler"
        };
        let doing = if hold.loaded { "bringing" } else { "fetching" };
        out.insert(*hauler, view(*hauler, job, doing, &hold.kind, hold.count));
    }
    for (friend, help) in &state.helpers {
        if help.owner != owner || out.contains_key(friend) || help.until <= now {
            continue;
        }
        let doing = match help.how.as_str() {
            "harvest" => "harvesting",
            _ if at_work.contains_key(friend) => "working",
            _ => "walking",
        };
        out.insert(*friend, view(*friend, "friend", doing, "", 0));
    }
    for (worker, work) in &at_work {
        let job = if work.hired { "builder" } else { "friend" };
        out.insert(*worker, view(*worker, job, "working", "", 0));
    }
    if let Some(builder) = upgrade.builder
        && !out.contains_key(&builder)
    {
        // Walking only while it moves: a builder on its claim that stands
        // still (between stages, or kept by its needs) is waiting.
        let moving =
            entity(world, builder).is_some_and(|actor| world.get::<MoveIntent>(actor).is_some());
        let doing = if upgrade.builder_until > now && moving {
            "walking"
        } else if upgrade.builder_until > now {
            "waiting"
        } else {
            "hired"
        };
        out.insert(builder, view(builder, "builder", doing, "", 0));
    }
    out.into_values().collect()
}
