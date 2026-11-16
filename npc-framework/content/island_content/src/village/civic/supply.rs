//! Building materials across settlements: what an open site still lacks that
//! its own settlement can neither take from its stores nor make, assigned to
//! the nearest other settlement that makes or stores the good (S4 housing
//! fix: on seeds 7 and 11 Stone Ridge lost its woodcutters, its houses waited
//! a week for logs and were cancelled while the harbour stored 143 logs).
//!
//! A pure function of the world: nothing is saved. The assigned settlement's
//! production plan takes the shortfall as civic demand, its haulers carry it
//! (`haul::material_trip`, second pass) and the boat leaves it alone.
use super::haul::post_available;
use super::model::{CivicState, ConstructionSite, Depot, Goods};
use super::{depot, sites};
use crate::village::frontier::settlements;
use crate::village::{Role, Villager, catalog, living, market, production};
use bevy_ecs::prelude::*;
use npc_framework::prelude::*;
use std::collections::{BTreeMap, BTreeSet};

/// One share of a site's shortfall that another settlement supplies.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Assignment {
    /// The open site.
    pub site: SimId,
    /// The site's own settlement.
    pub settlement: SimId,
    /// The settlement that makes or stores the good.
    pub supplier: SimId,
    pub kind: String,
    pub units: u32,
}

/// Every open site of the world with its settlement, buildings before prop
/// sites, each group in identity order.
fn all_open(world: &World) -> Vec<(SimId, SimId, Entity)> {
    let Some(state) = world.get_resource::<CivicState>() else {
        return Vec::new();
    };
    let mut list: Vec<(bool, SimId, SimId, Entity)> = state
        .sites
        .iter()
        .flat_map(|(settlement, ids)| ids.iter().map(move |id| (*settlement, *id)))
        .filter_map(|(settlement, id)| {
            let entity = world.resource::<SimIndex>().get(id)?;
            let site = world.get::<ConstructionSite>(entity)?;
            Some((site.is_prop(), id, settlement, entity))
        })
        .collect();
    list.sort_unstable_by_key(|(prop, id, _, _)| (*prop, *id));
    list.into_iter()
        .map(|(_, id, settlement, entity)| (id, settlement, entity))
        .collect()
}

/// Units of `kind` a hauler could take from the stores of `settlement` now
/// (what the boat's open order holds at the trading post excluded).
fn stored(world: &World, settlement: SimId, kind: &str) -> u32 {
    let now = world.resource::<SimClock>().tick;
    depot::depots(world, settlement)
        .into_iter()
        .filter_map(|(id, entity)| {
            let shelf = world.get::<Depot>(entity)?;
            Some(post_available(
                world,
                id,
                kind,
                shelf.available(kind, None, now),
            ))
        })
        .fold(0, u32::saturating_add)
}

/// The shortfalls of open sites that their own settlement cannot meet, each
/// assigned to the nearest other settlement (by the distance from the site's
/// door to its centre, then identity) that has a resident of a trade making
/// the good, or the units in its stores. A settlement with a maker of the
/// good meets its own sites' need of it, however short it is now. Store stock
/// goes to the settlement's own sites first. Ascending by site, then good.
pub fn unmet_elsewhere(world: &World) -> Vec<Assignment> {
    let open = all_open(world);
    if open.is_empty() {
        return Vec::new();
    }
    // The goods some open site still lacks.
    let mut kinds: BTreeSet<String> = BTreeSet::new();
    for (_, _, entity) in &open {
        if let Some(site) = world.get::<ConstructionSite>(*entity) {
            kinds.extend(
                site.cost
                    .keys()
                    .filter(|kind| site.missing(kind) > 0)
                    .cloned(),
            );
        }
    }
    if kinds.is_empty() {
        return Vec::new();
    }
    // Who makes them, by the settlement whose production plan the maker is
    // in (`production::economic_settlement`).
    let mut makers: BTreeSet<(SimId, String)> = BTreeSet::new();
    if let Some(mut query) = world.try_query::<(Entity, &Villager)>() {
        for (actor, person) in query.iter(world) {
            if person.role == Role::Visitor || !living(world, actor) {
                continue;
            }
            let outputs = catalog::outputs_for(world, person.role);
            if !outputs.iter().any(|kind| kinds.contains(*kind)) {
                continue;
            }
            let Some(settlement) = production::economic_settlement(world, actor) else {
                continue;
            };
            for kind in outputs.iter().filter(|kind| kinds.contains(**kind)) {
                makers.insert((settlement, (*kind).to_owned()));
            }
        }
    }
    let regions: Vec<(SimId, SimPos)> = settlements::all(world)
        .into_iter()
        .filter_map(|(id, _)| Some((id, settlements::definition(world, id)?.center)))
        .collect();
    let mut free: BTreeMap<(SimId, String), u32> = BTreeMap::new();
    for (id, _) in &regions {
        for kind in &kinds {
            free.insert((*id, kind.clone()), stored(world, *id, kind));
        }
    }
    // Own stores first: what each site's settlement holds goes to its sites.
    let mut short: Vec<(SimId, SimId, SimPos, String, u32)> = Vec::new();
    for (site_id, settlement, entity) in &open {
        let Some(site) = world.get::<ConstructionSite>(*entity) else {
            continue;
        };
        for kind in site.cost.keys() {
            let missing = site.missing(kind);
            if missing == 0 || makers.contains(&(*settlement, kind.clone())) {
                continue;
            }
            let held = free.entry((*settlement, kind.clone())).or_default();
            let taken = (*held).min(missing);
            *held -= taken;
            if missing > taken {
                short.push((
                    *site_id,
                    *settlement,
                    site.plot.door(),
                    kind.clone(),
                    missing - taken,
                ));
            }
        }
    }
    // Then the nearest other settlement that stores or makes the good.
    let mut out = Vec::new();
    for (site, settlement, door, kind, mut left) in short {
        let mut nearest: Vec<(i64, SimId)> = regions
            .iter()
            .filter(|(id, _)| *id != settlement)
            .map(|(id, center)| (door.dist2(*center), *id))
            .collect();
        nearest.sort_unstable();
        for (_, supplier) in nearest {
            if left == 0 {
                break;
            }
            let units = if makers.contains(&(supplier, kind.clone())) {
                left
            } else {
                let held = free.entry((supplier, kind.clone())).or_default();
                let taken = (*held).min(left);
                *held -= taken;
                taken
            };
            if units == 0 {
                continue;
            }
            left -= units;
            out.push(Assignment {
                site,
                settlement,
                supplier,
                kind: kind.clone(),
                units,
            });
        }
    }
    out.sort_by(|a, b| (a.site, &a.kind, a.supplier).cmp(&(b.site, &b.kind, b.supplier)));
    out
}

/// The foreign sites whose shortfall is assigned to `supplier`, with the
/// units of each good, ascending by site.
pub fn assigned_sites(world: &World, supplier: SimId) -> Vec<(SimId, Goods)> {
    let mut sites: BTreeMap<SimId, Goods> = BTreeMap::new();
    for share in unmet_elsewhere(world)
        .into_iter()
        .filter(|share| share.supplier == supplier)
    {
        *sites
            .entry(share.site)
            .or_default()
            .entry(share.kind)
            .or_default() += share.units;
    }
    sites.into_iter().collect()
}

/// What `supplier` is to send other settlements' sites and the fund can pay
/// at today's quotes, as civic demand of its production plan (the same rule
/// as [`sites::material_orders`] for its own sites).
pub fn assigned_orders(world: &World, supplier: SimId) -> Goods {
    let mut orders = Goods::new();
    let mut coins = sites::site_budget(world);
    for (_, goods) in assigned_sites(world, supplier) {
        for (kind, units) in goods {
            let price = market::unit_price(world, &kind).unwrap_or(u32::MAX).max(1);
            let units = units.min(coins / price);
            if units > 0 {
                coins -= units * price;
                *orders.entry(kind).or_default() += units;
            }
        }
    }
    orders
}

/// Units of `kind` open sites still lack that `settlement` is to supply: its
/// own sites' shortfall and what is assigned to it. Neither the boat nor a
/// surplus trip takes these.
pub fn site_hold(world: &World, settlement: SimId, kind: &str) -> u32 {
    let own: u32 = sites::open_sites(world, settlement)
        .into_iter()
        .filter_map(|(_, entity)| world.get::<ConstructionSite>(entity))
        .map(|site| site.missing(kind))
        .fold(0, u32::saturating_add);
    if own == 0 && island_shortfall(world, kind) == 0 {
        return 0;
    }
    let assigned: u32 = unmet_elsewhere(world)
        .into_iter()
        .filter(|share| share.supplier == settlement && share.kind == kind)
        .map(|share| share.units)
        .fold(0, u32::saturating_add);
    own.saturating_add(assigned)
}

/// Units of `kind` every open site of the island still lacks.
pub fn island_shortfall(world: &World, kind: &str) -> u32 {
    all_open(world)
        .into_iter()
        .filter_map(|(_, _, entity)| world.get::<ConstructionSite>(entity))
        .map(|site| site.missing(kind))
        .fold(0, u32::saturating_add)
}
