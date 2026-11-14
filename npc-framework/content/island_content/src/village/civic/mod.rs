//! A village that grows (regional profile only): buildings as data, recorded
//! placement, construction sites, stores and stalls, hauling and the civic
//! planner.
//!
//! Ownership: `placement` owns where a building may stand and the two recorded
//! commands; `sites` owns a site from the first delivery to completion or
//! cancellation; `depot` owns stores and stalls as sellers of record; `haul`
//! owns the resident steps the `life.*` leaves take; `planner` owns the slow
//! per-settlement review. Everything is registered by [`register`], which the
//! regional composition calls; older profiles never see these types.
mod depot;
mod haul;
mod model;
mod placement;
mod planner;
mod sites;

pub(crate) use depot::reserve as depot_reserve;
pub use depot::{
    all_depots, any as any_depot, available as depot_available, buy as depot_buy,
    check_buy as depot_check_buy, check_deposit as depot_check_deposit, deposit as depot_deposit,
    depots, home_settlement, is_depot, serves, storable,
};
pub use haul::{
    MaterialTrip, RestockTrip, boat_trip, material_trip, restock_trip, site_needs_builder,
    surplus_trip,
};
pub(crate) use haul::{
    build as build_step, fitting, haul_boat, haul_materials, haul_restock, haul_surplus,
    hearth_reserve, kept, nearest_first, reachable, spendable,
};
pub use model::{
    Building, CIVIC_OWNER, CivicState, ConstructionSite, Depot, Goods, Hold, PlannerState, Plot,
    SUSPEND_TICKS, SiteKind, SiteWork, SuspendedWork, held,
};
pub use placement::MAX_OPEN_PROP_SITES;
pub use placement::{
    COAST_REACH_CELLS, CancelBuilding, MAX_OPEN_SITES, MAX_SETTLEMENT_DISTANCE_MM, PlaceBuilding,
    cell_on_map, ground_check, placement_check, placement_plot, plot_for, sea,
};
pub(crate) use placement::{place, place_prop_site};
pub use planner::{can_build_home, find_site, find_site_around, house_homeless, largest_home_beds};
pub use sites::{
    begin_work, budget, build_tool_percent, cancel as cancel_site, cancel_work, check_delivery,
    deliver, fund_hold, funding, levy_capacity, material_orders, open_sites, outstanding_cost,
    reserve, site_budget,
};
pub(crate) use sites::{builder_tool, removable, remove_finished};
pub(crate) use sites::{note_grants, raise_building_levy, raise_relief_levy, settle_earmark};

use crate::village::frontier::building_spec::BuildingDef;
use crate::village::frontier::economy_spec;
use bevy_app::App;
use bevy_ecs::prelude::*;
use npc_framework::prelude::*;
use serde::Serialize;

/// Regional profile only; called by `frontier::register` before freeze.
pub fn register(app: &mut App) {
    app.init_resource::<CivicState>()
        .register_sim_resource::<CivicState>()
        .register_sim_component::<ConstructionSite>()
        .register_sim_component::<Building>()
        .register_sim_component::<Depot>()
        .register_sim_component::<SiteWork>()
        .register_sim_component::<SuspendedWork>()
        .register_command::<PlaceBuilding>()
        .register_command::<CancelBuilding>();
}

/// Call once after the settlements exist.
pub fn spawn(world: &mut World) {
    planner::spawn(world);
}

/// Called once per tick by the regional Effects composition, after resident
/// actions: finishes building stages, then runs the planner whose turn it is.
pub fn tick(world: &mut World) {
    if !world.contains_resource::<CivicState>() {
        return;
    }
    let now = world.resource::<SimClock>().tick;
    sites::tick(world, now);
    planner::tick(world, now);
}

/// Lays out a building that stands from the founding, beside `near` when the
/// ground allows and else at the nearest valid spot to the centre. It costs
/// nothing and is not counted as built. Returns the building.
/// Lays out a building of an exported world (finished, or an open site).
pub(crate) fn lay_out(
    world: &mut World,
    building: &str,
    x: i32,
    z: i32,
    rotation: u8,
    finished: bool,
) -> Result<SimId, String> {
    sites::lay_out(world, building, x, z, rotation, finished)
}

pub(crate) fn found_building(
    world: &mut World,
    building: &str,
    settlement: SimId,
    near: Option<SimPos>,
    reason: &str,
) -> Option<SimId> {
    let def = definition(world, building)?.clone();
    let radius =
        economy_spec::tuning(world).map_or(36, |tuning| tuning.planner_search_radius_cells);
    let (x, z, rotation) = near
        .and_then(|origin| planner::find_site_around(world, &def, settlement, origin, 3, 16))
        .or_else(|| planner::find_site(world, &def, settlement, radius))?;
    sites::found(world, building, x, z, rotation, reason).ok()
}

/// The buildings the world's data defines, in authored order: the build menu.
pub fn catalog(world: &World) -> &[BuildingDef] {
    economy_spec::registry(world).map_or(&[], |registry| registry.spec.buildings.as_slice())
}

/// The definition a site or finished building was made from.
pub fn definition<'a>(world: &'a World, building: &str) -> Option<&'a BuildingDef> {
    economy_spec::registry(world)?.spec.building(building)
}

/// Seats of a workplace that was built after the founding.
pub fn workplace_seats(world: &World, entity: Entity) -> Option<u32> {
    world
        .get::<Building>(entity)
        .map(|building| building.seats)
        .filter(|seats| *seats > 0)
}

/// One open construction site, for drawing and for the build panel.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SiteView {
    pub id: SimId,
    pub building: String,
    pub label: String,
    pub kind: &'static str,
    pub settlement: SimId,
    pub plot: Plot,
    pub door: SimPos,
    pub cost: Goods,
    pub delivered: Goods,
    pub work_done: u32,
    pub work_ticks: u32,
    pub civic: bool,
    pub reason: String,
    pub builder: Option<SimId>,
    /// Tick the site was placed.
    pub placed_at: u64,
    /// Tick of the last delivery of materials; `None` before the first.
    pub last_delivery_at: Option<u64>,
    /// Coins the civic fund has paid for the materials delivered so far.
    pub paid: u32,
    /// One sentence on the money: funded, or short and by how much.
    pub funding: String,
}

/// Every open construction site, ascending by identity.
pub fn sites(world: &World) -> Vec<SiteView> {
    let Some(state) = world.get_resource::<CivicState>() else {
        return Vec::new();
    };
    let now = world.resource::<SimClock>().tick;
    let mut ids: Vec<SimId> = state.sites.values().flatten().copied().collect();
    ids.sort_unstable();
    ids.into_iter()
        .filter_map(|id| {
            let site = world.get::<ConstructionSite>(world.resource::<SimIndex>().get(id)?)?;
            Some(SiteView {
                id,
                building: site.building.clone(),
                label: site.label.clone(),
                // A prop site reads `prop`; its `building` is the prop's key.
                kind: if site.is_prop() {
                    "prop"
                } else {
                    definition(world, &site.building).map_or("", |def| def.kind.name())
                },
                settlement: site.settlement,
                plot: site.plot,
                door: site.plot.door(),
                cost: site.cost.clone(),
                delivered: site.delivered.clone(),
                work_done: site.work_done,
                work_ticks: site.work_ticks,
                civic: site.civic,
                reason: site.reason.clone(),
                builder: site.builder.filter(|_| site.builder_until > now),
                placed_at: site.placed_at,
                last_delivery_at: site.last_delivery_at,
                paid: site.paid,
                funding: funding(world, site),
            })
        })
        .collect()
}

/// One finished building placed after the founding.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct BuildingView {
    pub id: SimId,
    pub building: String,
    pub label: String,
    pub kind: &'static str,
    pub settlement: SimId,
    pub plot: Plot,
    pub door: SimPos,
    pub completed_at: u64,
    /// Crop plots of a field; 0 for every other building. The plots, their
    /// crops and growth are in `village::view::fields`.
    pub crop_plots: u32,
}

/// Every finished building placed after the founding, ascending by identity.
pub fn buildings(world: &World) -> Vec<BuildingView> {
    let Some(state) = world.get_resource::<CivicState>() else {
        return Vec::new();
    };
    let mut ids: Vec<SimId> = state.buildings.values().flatten().copied().collect();
    ids.sort_unstable();
    ids.into_iter()
        .filter_map(|id| {
            let building = world.get::<Building>(world.resource::<SimIndex>().get(id)?)?;
            Some(BuildingView {
                id,
                building: building.building.clone(),
                label: building.label.clone(),
                kind: definition(world, &building.building).map_or("", |def| def.kind.name()),
                settlement: building.settlement,
                plot: building.plot,
                door: building.plot.door(),
                completed_at: building.completed_at,
                crop_plots: world
                    .resource::<SimIndex>()
                    .get(id)
                    .and_then(|entity| world.get::<crate::village::fields::Field>(entity))
                    .map_or(0, |field| field.plots.len() as u32),
            })
        })
        .collect()
}

/// The shelf of one store or stall.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DepotView {
    pub id: SimId,
    pub label: String,
    pub settlement: SimId,
    /// True for a market stall, false for storage.
    pub stall: bool,
    pub door: SimPos,
    pub capacity: u32,
    pub used: u32,
    /// Units on the shelf by good, ascending by key.
    pub stock: Vec<(String, u32)>,
    pub sold_units: u64,
    pub turnover: u64,
}

/// Every store and stall with its stock, ascending by identity.
pub fn depot_views(world: &World) -> Vec<DepotView> {
    all_depots(world)
        .into_iter()
        .filter_map(|(id, entity)| {
            let depot = world.get::<Depot>(entity)?;
            let building = world.get::<Building>(entity)?;
            Some(DepotView {
                id,
                label: building.label.clone(),
                settlement: building.settlement,
                stall: depot.public,
                door: building.plot.door(),
                capacity: depot.capacity,
                used: depot.used(),
                stock: depot
                    .lots
                    .keys()
                    .map(|kind| (kind.clone(), depot.stock(kind)))
                    .collect(),
                sold_units: depot.sold_units,
                turnover: depot.turnover,
            })
        })
        .collect()
}

/// What the planner of a settlement saw and decided at its last review.
pub fn planner(world: &World, settlement: SimId) -> Option<&PlannerState> {
    world
        .get_resource::<CivicState>()?
        .planners
        .get(&settlement)
}

/// Units of `kind` in stores, stalls and open sites: goods that are in
/// nobody's pack and not on the ground. For conservation checks and totals.
pub fn goods_in_custody(world: &World, kind: &str) -> u32 {
    let Some(state) = world.get_resource::<CivicState>() else {
        return 0;
    };
    let index = world.resource::<SimIndex>();
    let stored: u32 = state
        .depots
        .values()
        .flatten()
        .filter_map(|id| world.get::<Depot>(index.get(*id)?))
        .map(|depot| depot.stock(kind))
        .sum();
    let delivered: u32 = state
        .sites
        .values()
        .flatten()
        .filter_map(|id| world.get::<ConstructionSite>(index.get(*id)?))
        .map(|site| site.delivered.get(kind).copied().unwrap_or(0))
        .sum();
    stored
        + delivered
        + crate::village::fields::seed_in_custody(world, kind)
        + crate::village::live::goods_in_custody(world, kind)
}

/// The shared `buy`/`sell` action aimed at a store, a stall or a building
/// site: the validation. `None` when the target is none of these. Buying
/// takes goods from the shelf; selling consigns goods to a store or delivers
/// materials to a site.
pub fn trade_check(
    world: &World,
    actor: Entity,
    target: Entity,
    buy: bool,
    kind: &str,
    count: u32,
    limit_price: Option<u32>,
) -> Option<Result<(), String>> {
    let outside = || Err("the current quote is outside your limit_price".to_owned());
    if is_depot(world, target) {
        return Some(if buy {
            depot_check_buy(world, actor, target, kind, count).and_then(|quote| {
                if limit_price.is_some_and(|limit| quote.unit_price > limit) {
                    outside()
                } else {
                    Ok(())
                }
            })
        } else {
            depot_check_deposit(world, actor, target, kind, count)
        });
    }
    if world.get::<ConstructionSite>(target).is_some() {
        return Some(if buy {
            Err("a building site sells nothing; sell it the materials it needs".into())
        } else {
            check_delivery(world, actor, target, kind, count).and_then(|coins| {
                if limit_price.is_some_and(|limit| coins / count.max(1) < limit) {
                    outside()
                } else {
                    Ok(())
                }
            })
        });
    }
    None
}

/// The shared `buy`/`sell` action aimed at a store, a stall or a building
/// site: the transfer. `None` when the target is none of these.
pub fn trade(
    world: &mut World,
    actor: Entity,
    target: Entity,
    buy: bool,
    kind: &str,
    count: u32,
) -> Option<Result<(), String>> {
    if is_depot(world, target) {
        return Some(if buy {
            depot_buy(world, actor, target, kind, count)
        } else {
            depot_deposit(world, actor, target, kind, count)
        });
    }
    if world.get::<ConstructionSite>(target).is_some() {
        return Some(if buy {
            Err("a building site sells nothing".into())
        } else {
            deliver(world, actor, target, kind, count)
        });
    }
    None
}

/// Goods on the shelves of a settlement, as supply for the funded plan.
pub fn depot_supply(world: &World, settlement: SimId) -> Vec<(SimId, String, u32)> {
    depot::supply(world, settlement)
}

/// Checks the civic index against the entities it names, after a load.
pub fn validate(world: &World) -> Result<(), String> {
    let Some(state) = world.get_resource::<CivicState>() else {
        return Ok(());
    };
    let index = world.resource::<SimIndex>();
    for (settlement, ids) in &state.sites {
        for id in ids {
            if index
                .get(*id)
                .and_then(|entity| world.get::<ConstructionSite>(entity))
                .is_none_or(|site| site.settlement != *settlement)
            {
                return Err(format!("Building site {} is missing or misfiled", id.0));
            }
        }
    }
    for (settlement, ids) in &state.buildings {
        for id in ids {
            if index
                .get(*id)
                .and_then(|entity| world.get::<Building>(entity))
                .is_none_or(|building| building.settlement != *settlement)
            {
                return Err(format!("Building {} is missing or misfiled", id.0));
            }
        }
    }
    for ids in state.depots.values() {
        for id in ids {
            let Some(depot) = index.get(*id).and_then(|entity| world.get::<Depot>(entity)) else {
                return Err(format!("Store {} is missing", id.0));
            };
            if depot.used() > depot.capacity {
                return Err(format!("Store {} holds more than its capacity", id.0));
            }
        }
    }
    crate::village::fields::validate(world)?;
    crate::village::live::validate(world)?;
    crate::village::demography::validate(world)
}
