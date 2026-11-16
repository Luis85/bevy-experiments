//! The merchant boat (regional profile only): the island's one external
//! source and sink.
//!
//! The boat calls at the port settlement every few days and stays some hours.
//! It buys what lies in that settlement's stores and stalls above a kept
//! level, paying each owner at once, and it sells the civic fund goods the
//! port is short of, which are laid in the trading post as the settlement's
//! stock and sold on at the market price. The merchant's purse is finite and
//! is never refilled: coins move between the island and the purse only by
//! these trades, so island coins plus purse are constant. Goods the boat buys
//! leave the world and goods it sells enter it; both are counted here.
use super::civic::{self, CIVIC_OWNER, Depot, Goods};
use super::frontier::life_spec::MerchantSpec;
use super::frontier::{Settlement, economy_spec, settlements};
use super::{VillageState, Villager, Wallet, catalog, economy as econ, living, market};
use bevy_app::App;
use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use npc_framework::kernel::hash::SimHashResource;
use npc_framework::kernel::rng::sim_range;
use npc_framework::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeMap;

/// The boat looks at the shore this often; it trades once per round while in.
const WATCH_TICKS: u64 = 50;

/// Owner of the imports the boat leaves on the market shelf on consignment
/// (what the civic fund cannot pay for): whoever buys them pays the boat's
/// purse. Never a resident's id.
pub const MERCHANT_OWNER: SimId = SimId(u32::MAX);

/// Credits the boat's purse for `coins` of its consigned imports sold off a
/// shelf; false when `owner` is not the boat (or there is no boat).
pub(crate) fn credit_consignment(world: &mut World, owner: SimId, coins: u32) -> bool {
    if owner != MERCHANT_OWNER {
        return false;
    }
    let Some(mut merchant) = world.get_resource_mut::<Merchant>() else {
        return false;
    };
    merchant.purse = merchant.purse.saturating_add(coins);
    merchant.coins_earned += u64::from(coins);
    true
}

/// Units of `kind` in `depot` that belong to the boat itself.
fn consigned(depot: &Depot, kind: &str) -> u32 {
    depot
        .lots
        .get(kind)
        .and_then(|owners| owners.get(&MERCHANT_OWNER))
        .copied()
        .unwrap_or(0)
}
const TRADE_ROUND_TICKS: u64 = 600;

/// Totals of one visit.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub struct VisitTotals {
    pub arrived_at: u64,
    pub left_at: u64,
    /// Units and coins of island goods the boat bought.
    pub bought_units: u32,
    pub bought_coins: u32,
    /// Units and coins of goods the boat sold to the civic fund.
    pub sold_units: u32,
    pub sold_coins: u32,
    /// Worth, at the boat's price, of what it laid out on consignment.
    pub consigned_coins: u32,
    pub settlers: u32,
    pub bought: Goods,
    pub sold: Goods,
}

/// One good the boat announced it will buy at its next call: a standing
/// order the island's makers work to, paid from the boat's purse at the
/// trading post. It expires when the boat leaves that call.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub struct BoatOrder {
    pub good: String,
    pub units: u32,
    /// Coins a unit, above the island's quote of the day it was announced.
    pub unit_price: u32,
    /// Units bought against the order so far.
    pub delivered: u32,
}

#[derive(Resource, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub struct Merchant {
    /// Coins the merchant owns. Part of every conservation sum.
    pub purse: u32,
    pub settlement: Option<SimId>,
    /// The trading post: a civic stall at the port.
    pub post: Option<SimId>,
    pub founded: bool,
    /// Calendar day of the next landing.
    pub next_visit_day: u64,
    /// Tick the boat leaves, while it is in.
    pub docked_until: Option<u64>,
    /// Goods still aboard for sale, while it is in.
    pub hold: Goods,
    pub visit: VisitTotals,
    pub last_visit: Option<VisitTotals>,
    pub visits: u32,
    /// Landings a storm postponed.
    pub delays: u32,
    /// Units that entered and left the world by boat, by good.
    pub imported: BTreeMap<String, u64>,
    pub exported: BTreeMap<String, u64>,
    /// Coins the boat paid the island and the island paid the boat.
    pub coins_paid: u64,
    pub coins_earned: u64,
    pub settlers: u32,
    /// Coins of boat duty the boat paid the civic fund.
    #[serde(default)]
    pub duty_paid: u64,
    /// The outside world's price factor of each good the boat trades,
    /// permille (1000: the catalog price). Drifts between visits (S2.3).
    pub outside: BTreeMap<String, u32>,
    /// What the next visit carries (set when the boat leaves).
    pub next_hold: Goods,
    /// Where the boat ties up: the centre of the sea cell nearest to the
    /// port settlement's centre. The trading post stands beside it.
    #[serde(default)]
    pub berth: Option<SimPos>,
    /// The port's market stall near its centre, where imports are sold.
    #[serde(default)]
    pub market: Option<SimId>,
    /// What the boat will buy at its next call (announced as it leaves).
    #[serde(default)]
    pub order: Vec<BoatOrder>,
    /// Orders announced so far: where the rotation over the goods stands.
    #[serde(default)]
    pub orders_announced: u32,
}
fn hash_goods(h: &mut Fnv64, goods: &Goods) {
    h.u32(goods.len() as u32);
    for (kind, count) in goods {
        h.str(kind);
        h.u32(*count);
    }
}
fn hash_visit(h: &mut Fnv64, visit: &VisitTotals) {
    h.u64(visit.arrived_at);
    h.u64(visit.left_at);
    for value in [
        visit.bought_units,
        visit.bought_coins,
        visit.sold_units,
        visit.sold_coins,
        visit.consigned_coins,
        visit.settlers,
    ] {
        h.u32(value);
    }
    hash_goods(h, &visit.bought);
    hash_goods(h, &visit.sold);
}
impl SimHashResource for Merchant {
    const NAME: &'static str = "regional_merchant_v1";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u32(self.purse);
        h.opt(self.settlement.as_ref(), |h, id| h.u32(id.0));
        h.opt(self.post.as_ref(), |h, id| h.u32(id.0));
        h.u8(u8::from(self.founded));
        h.u64(self.next_visit_day);
        h.opt(self.docked_until.as_ref(), |h, tick| h.u64(*tick));
        hash_goods(h, &self.hold);
        hash_visit(h, &self.visit);
        h.opt(self.last_visit.as_ref(), hash_visit);
        h.u32(self.visits);
        h.u32(self.delays);
        for ledger in [&self.imported, &self.exported] {
            h.u32(ledger.len() as u32);
            for (kind, units) in ledger {
                h.str(kind);
                h.u64(*units);
            }
        }
        h.u64(self.coins_paid);
        h.u64(self.coins_earned);
        h.u32(self.settlers);
        h.u64(self.duty_paid);
        h.opt(self.berth.as_ref(), |h, pos| {
            h.i32(pos.x);
            h.i32(pos.z);
        });
        h.opt(self.market.as_ref(), |h, id| h.u32(id.0));
        h.u32(self.order.len() as u32);
        for order in &self.order {
            h.str(&order.good);
            h.u32(order.units);
            h.u32(order.unit_price);
            h.u32(order.delivered);
        }
        h.u32(self.orders_announced);
        h.u32(self.outside.len() as u32);
        for (good, factor) in &self.outside {
            h.str(good);
            h.u32(*factor);
        }
        hash_goods(h, &self.next_hold);
    }
    fn snapshot_fns() -> Option<npc_framework::snapshot::ResourceSnapshotFns> {
        Some(npc_framework::snapshot::serde_resource_fns::<Self>())
    }
}

/// Regional profile only.
pub fn register(app: &mut App) {
    app.init_resource::<Merchant>()
        .register_sim_resource::<Merchant>();
}

fn spec(world: &World) -> Option<&MerchantSpec> {
    economy_spec::registry(world)
        .map(|registry| &registry.spec.merchant)
        .filter(|merchant| merchant.enabled)
}

/// Call once after the settlements exist: the merchant's purse and first call.
pub fn spawn(world: &mut World) {
    let Some(spec) = spec(world).cloned() else {
        return;
    };
    let settlement = settlements::all(world)
        .into_iter()
        .find(|(_, entity)| {
            world
                .get::<Settlement>(*entity)
                .is_some_and(|settlement| settlement.key == spec.settlement)
        })
        .map(|(id, _)| id);
    let day = super::calendar(world).0;
    let mut merchant = world.resource_mut::<Merchant>();
    merchant.purse = spec.purse;
    merchant.settlement = settlement;
    merchant.next_visit_day = day + spec.first_visit_day;
}

/// The sea cell nearest to a settlement's centre (ties: lower z, then lower
/// x), as the centre of that cell: the boat's berth. Sea is water reached
/// from the edge of the map, so a pond never counts.
pub fn berth_of(world: &World, settlement: SimId) -> Option<SimPos> {
    use super::frontier::terrain;
    let centre = settlements::definition(world, settlement)?.center;
    let bounds = terrain::bounds(world);
    let water = |cell: CellCoord| {
        terrain::cell(world, cell.x, cell.z)
            .is_some_and(|value| value.surface == terrain::TerrainSurface::Water)
    };
    let mut sea = vec![false; bounds.len()];
    let mut queue = std::collections::VecDeque::new();
    for index in 0..bounds.len() as u32 {
        let cell = bounds.coord(index);
        let edge = [(0, -1), (-1, 0), (1, 0), (0, 1)]
            .iter()
            .any(|(dx, dz)| !bounds.contains(CellCoord::new(cell.x + dx, cell.z + dz)));
        if edge && water(cell) {
            sea[index as usize] = true;
            queue.push_back(cell);
        }
    }
    while let Some(cell) = queue.pop_front() {
        for (dx, dz) in [(0, -1), (-1, 0), (1, 0), (0, 1)] {
            let next = CellCoord::new(cell.x + dx, cell.z + dz);
            if let Some(index) = bounds.index(next)
                && !sea[index as usize]
                && water(next)
            {
                sea[index as usize] = true;
                queue.push_back(next);
            }
        }
    }
    sea.iter()
        .enumerate()
        .filter(|(_, sea)| **sea)
        .map(|(index, _)| {
            let cell = bounds.coord(index as u32);
            let pos = SimPos::new(cell.x * 1000 + 500, cell.z * 1000 + 500);
            (pos.dist2(centre), cell.z, cell.x, pos)
        })
        .min_by_key(|(distance, z, x, _)| (*distance, *z, *x))
        .map(|(_, _, _, pos)| pos)
}

/// The merchant's coins: add them to the island's in a conservation check.
pub fn purse(world: &World) -> u32 {
    world
        .get_resource::<Merchant>()
        .map_or(0, |merchant| merchant.purse)
}

fn price(world: &World, kind: &str, percent: u32) -> Option<u32> {
    let base = catalog::price_in(world, kind)?;
    let factor = outside_factor(world, kind);
    Some(
        ((u64::from(base) * u64::from(percent) * u64::from(factor)).div_ceil(100_000))
            .clamp(1, 1_000_000) as u32,
    )
}

/// The outside price factor of `kind`, permille (1000 until it drifts).
pub fn outside_factor(world: &World, kind: &str) -> u32 {
    world
        .get_resource::<Merchant>()
        .and_then(|merchant| merchant.outside.get(kind).copied())
        .unwrap_or(1000)
}

/// `sim_rand` purposes of the boat's drift (id `MERCHANT_OWNER`, tick field
/// `visits * 1024 + good index`).
const BOAT_PRICE_DRIFT: u32 = 0x424f_4131;
const BOAT_HOLD_SHARE: u32 = 0x424f_4132;
const BOAT_HOLD_MISSING: u32 = 0x424f_4133;

/// Whether the island can fill an order of `kind` by the next call: a good
/// a recipe or a crop makes (the production plan answers the order), or a
/// gathered good nobody eats (its gatherers' surplus goes to the post).
/// Gathered food (berries) is eaten by the people who gather it and never
/// reaches the post, and a good that spoils before the next call cannot be
/// kept for it: neither is announced.
fn orderable(world: &World, kind: &str, interval_days: u64) -> bool {
    let Some(registry) = economy_spec::registry(world) else {
        return false;
    };
    let Some(good) = registry.spec.goods.iter().find(|good| good.key == kind) else {
        return false;
    };
    if good
        .shelf_life_days
        .is_some_and(|days| u64::from(days) <= interval_days)
    {
        return false;
    }
    let made = registry
        .spec
        .recipes
        .iter()
        .any(|recipe| recipe.outputs.iter().any(|output| output.good == kind))
        || registry
            .spec
            .crops
            .iter()
            .any(|crop| crop.harvest.good == kind);
    made || good.food_value.is_none()
}

/// Whether the island lacks `kind` for an order: nothing of it on offer
/// anywhere. Being below its stock target is not lacking: a young island is
/// below every target, and an order is what pulls a trade above it (the
/// closing run found every good counted short, so no order was announced).
fn island_lacks(world: &World, kind: &str) -> bool {
    island_stock(world, kind).0 == 0
}

/// Whether the island is short of `kind`: less on offer than its target.
fn island_short(world: &World, kind: &str) -> bool {
    let (offer, target) = island_stock(world, kind);
    offer < target
}

/// As the boat leaves: the outside prices drift and the next hold is set.
/// An island shortage fills the hold to the maximum (the island sent word).
fn drift(world: &mut World, spec: &MerchantSpec) {
    let d = &spec.drift;
    let seed = world.resource::<WorldSeed>().0;
    let visits = u64::from(world.resource::<Merchant>().visits);
    let mut goods: Vec<&str> = spec
        .buys
        .iter()
        .map(|buy| buy.good.as_str())
        .chain(spec.sells.iter().map(|sell| sell.good.as_str()))
        .collect();
    goods.sort_unstable();
    goods.dedup();
    let step = i32::try_from(d.price_step_permille).unwrap_or(0);
    let mut outside = world.resource::<Merchant>().outside.clone();
    for (index, good) in goods.iter().enumerate() {
        let tick = visits * 1024 + index as u64;
        let factor = i64::from(outside.get(*good).copied().unwrap_or(1000));
        let moved = if step > 0 {
            i64::from(sim_range(
                seed,
                MERCHANT_OWNER,
                tick,
                BOAT_PRICE_DRIFT,
                -step,
                step + 1,
            ))
        } else {
            0
        };
        let revert = (1000 - factor) / i64::from(d.revert_divisor.max(1));
        let next = (factor + moved + revert).clamp(
            i64::from(d.price_min_permille),
            i64::from(d.price_max_permille),
        );
        outside.insert((*good).to_owned(), next as u32);
    }
    let mut sells: Vec<&super::frontier::life_spec::MerchantSell> = spec.sells.iter().collect();
    sells.sort_by(|a, b| a.good.cmp(&b.good));
    let mut hold = Goods::new();
    let mut missing = 0_usize;
    for sell in sells {
        let index = goods
            .iter()
            .position(|good| *good == sell.good)
            .unwrap_or(0) as u64;
        let tick = visits * 1024 + index;
        let share = if d.hold_max_percent > d.hold_min_percent {
            sim_range(
                seed,
                MERCHANT_OWNER,
                tick,
                BOAT_HOLD_SHARE,
                d.hold_min_percent as i32,
                d.hold_max_percent as i32 + 1,
            ) as u32
        } else {
            d.hold_min_percent
        };
        let gone = d.missing_permille > 0
            && (sim_range(seed, MERCHANT_OWNER, tick, BOAT_HOLD_MISSING, 0, 1000) as u32)
                < d.missing_permille
            && missing < spec.sells.len() / 2;
        let units = if island_short(world, &sell.good) {
            sell.units * d.hold_max_percent / 100
        } else if gone {
            missing += 1;
            0
        } else {
            sell.units * share / 100
        };
        if units > 0 {
            hold.insert(sell.good.clone(), units);
        }
    }
    let next_day = {
        let mut merchant = world.resource_mut::<Merchant>();
        merchant.outside = outside.clone();
        merchant.next_hold = hold.clone();
        merchant.next_visit_day
    };
    record_effect(
        world,
        None,
        "VillageBoatForecast",
        &json!({"next_visit_day":next_day + 1,
            "prices":outside.iter().map(|(good, factor)| json!({"good":good,"factor":factor}))
                .collect::<Vec<_>>(),
            "aboard":hold}),
    );
}

/// What the boat's next visit looks like, for the interface.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct BoatOutlook {
    pub good: String,
    /// The outside price factor, permille.
    pub factor_permille: u32,
    /// Units aboard at the next visit (goods it sells).
    pub units_next: u32,
    /// The boat buys it.
    pub buys: bool,
}

/// The boat's outlook by good (key order); empty without a boat.
pub fn outlook(world: &World) -> Vec<BoatOutlook> {
    let (Some(spec), Some(merchant)) = (spec(world), world.get_resource::<Merchant>()) else {
        return Vec::new();
    };
    let mut goods: Vec<&str> = spec
        .buys
        .iter()
        .map(|buy| buy.good.as_str())
        .chain(spec.sells.iter().map(|sell| sell.good.as_str()))
        .collect();
    goods.sort_unstable();
    goods.dedup();
    goods
        .into_iter()
        .map(|good| BoatOutlook {
            good: good.to_owned(),
            factor_permille: merchant.outside.get(good).copied().unwrap_or(1000),
            units_next: merchant.next_hold.get(good).copied().unwrap_or(0),
            buys: spec.buys.iter().any(|buy| buy.good == good),
        })
        .collect()
}

/// Units of `kind` the port settlement holds: its residents' packs and its
/// stores and stalls.
fn port_stock(world: &World, settlement: SimId, kind: &str) -> u32 {
    let carried: u32 = super::demography::members(world, settlement)
        .into_iter()
        .map(|(_, actor)| econ::count(world, actor, kind))
        .sum();
    let stored: u32 = civic::depots(world, settlement)
        .into_iter()
        .filter_map(|(_, entity)| world.get::<Depot>(entity))
        .map(|depot| depot.stock(kind))
        .sum();
    carried + stored
}

/// Units of `kind` the whole island has on offer, and its island stock
/// target (`stock::island_target`: the settlements' targets for a good made in
/// batches or grown once a season; none for a good gathered all the time,
/// such as logs, which has no cover).
fn island_stock(world: &World, kind: &str) -> (u32, u32) {
    let offer = settlements::all(world)
        .into_iter()
        .map(|(id, _)| super::stock::on_offer(world, id, kind))
        .fold(0, u32::saturating_add);
    (offer, super::stock::island_target(world, kind))
}

/// Units of `kind` the island holds above its stock targets that the boat
/// would buy: its surplus, while the boat is in or due today and buys it,
/// and what its announced order still wants, at any time.
pub fn wanted_surplus(world: &World, kind: &str) -> u32 {
    let (Some(spec), Some(merchant)) = (spec(world), world.get_resource::<Merchant>()) else {
        return 0;
    };
    let today = super::calendar(world).0;
    if merchant.docked_until.is_none() && merchant.next_visit_day > today {
        // The announced order is laid at the post ahead of the call (a long
        // walk to the water does not fit in a short stay); surplus waits
        // for the boat's day.
        return order_left(merchant, kind);
    }
    let Some(buy) = spec.buys.iter().find(|buy| buy.good == kind) else {
        return order_left(merchant, kind);
    };
    let bought = merchant.visit.bought.get(kind).copied().unwrap_or(0);
    let (offer, target) = island_stock(world, kind);
    // The announced order is bought whatever the stock targets say.
    offer
        .saturating_sub(target.max(buy.keep_units))
        .min(buy.max_units.saturating_sub(bought))
        .max(order_left(merchant, kind))
}

/// Units of `kind` lying at the trading post that the boat's open order
/// still wants: held for the boat, so no hauler takes them to a site or a
/// stall before the call (closing fix).
pub(crate) fn held_for_order(world: &World, kind: &str) -> u32 {
    world
        .get_resource::<Merchant>()
        .map_or(0, |merchant| order_left(merchant, kind))
}

/// Units of `kind` the announced order still wants.
fn order_left(merchant: &Merchant, kind: &str) -> u32 {
    merchant
        .order
        .iter()
        .filter(|order| order.good == kind)
        .map(|order| order.units.saturating_sub(order.delivered))
        .sum()
}

/// The boat's announced order still open, as demand for the production plan
/// of the port settlement (`live::order_demand` is the same for homesteads).
pub(crate) fn order_demand(world: &World, settlement: SimId) -> BTreeMap<String, u32> {
    let mut goods = BTreeMap::new();
    let Some(merchant) = world.get_resource::<Merchant>() else {
        return goods;
    };
    if spec(world).is_none() || merchant.settlement != Some(settlement) {
        return goods;
    }
    for order in &merchant.order {
        let left = order.units.saturating_sub(order.delivered);
        if left > 0 {
            *goods.entry(order.good.clone()).or_default() += left;
        }
    }
    goods
}

/// As the boat leaves: the open order expires (nothing was reserved or held
/// for it) and the next one is announced, rotating over the goods the boat
/// buys that a trade of the island makes, never fiber or thread.
fn announce(world: &mut World, spec: &MerchantSpec) {
    let closed = std::mem::take(&mut world.resource_mut::<Merchant>().order);
    for order in &closed {
        record_effect(
            world,
            None,
            "VillageBoatOrderClosed",
            &json!({"good":order.good,"units":order.units,"delivered":order.delivered,
                "unit_price":order.unit_price}),
        );
    }
    if spec.order_goods == 0 || spec.order_units == 0 {
        return;
    }
    let made: Vec<&str> = {
        let mut query = world.query::<(Entity, &Villager)>();
        let roles: std::collections::BTreeSet<super::Role> = query
            .iter(world)
            .filter(|(entity, _)| living(world, *entity))
            .map(|(_, person)| person.role)
            .collect();
        spec.buys
            .iter()
            // The buy list is data: what the boat must never take (the
            // tailors' fiber and thread) is simply not on it.
            .map(|buy| buy.good.as_str())
            .filter(|good| {
                roles
                    .iter()
                    .any(|role| catalog::outputs_for(world, *role).contains(good))
            })
            .collect()
    };
    if made.is_empty() {
        return;
    }
    // The boat never asks for what the island lacks or cannot deliver by
    // its next call, and asks first for what is dear abroad (S2.3); equal
    // factors keep the rotation.
    let start = world.resource::<Merchant>().orders_announced as usize;
    let mut ranked: Vec<(std::cmp::Reverse<u32>, usize, &str)> = (0..made.len())
        .map(|index| made[(start + index) % made.len()])
        .filter(|good| !island_lacks(world, good))
        .filter(|good| orderable(world, good, spec.interval_days))
        .enumerate()
        .map(|(index, good)| (std::cmp::Reverse(outside_factor(world, good)), index, good))
        .collect();
    ranked.sort_unstable();
    let count = (spec.order_goods as usize).min(ranked.len());
    let mut order = Vec::with_capacity(count);
    for (_, _, good) in ranked.into_iter().take(count) {
        let Some(quote) =
            market::unit_price(world, good).or_else(|| catalog::price_in(world, good))
        else {
            continue;
        };
        let unit_price = ((u64::from(quote)
            * u64::from(spec.order_price_percent)
            * u64::from(outside_factor(world, good)))
            / 100_000)
            .max(u64::from(quote) + 1)
            .min(1_000_000) as u32;
        order.push(BoatOrder {
            good: good.into(),
            units: spec.order_units,
            unit_price,
            delivered: 0,
        });
    }
    let next_day = {
        let mut merchant = world.resource_mut::<Merchant>();
        merchant.orders_announced = merchant.orders_announced.wrapping_add(1);
        merchant.order = order.clone();
        merchant.next_visit_day
    };
    record_effect(
        world,
        None,
        "VillageBoatOrder",
        &json!({"next_visit_day":next_day + 1,"order":order.iter().map(|o| json!({
            "good":o.good,"units":o.units,"unit_price":o.unit_price})).collect::<Vec<_>>()}),
    );
}

/// The trading post, where surplus for the boat is laid.
pub fn post(world: &World) -> Option<(SimId, Entity)> {
    let id = world.get_resource::<Merchant>()?.post?;
    Some((id, world.resource::<SimIndex>().get(id)?))
}

/// The boat buys what lies in the port's stores and stalls above the island's
/// stock targets (and never below a store's kept level), up to its limit of
/// the good per visit and what its purse can pay.
fn buy_exports(world: &mut World, spec: &MerchantSpec, settlement: SimId) {
    buy_order(world, settlement);
    let now = world.resource::<SimClock>().tick;
    for buy in &spec.buys {
        // Grain is not surplus while the island is short of the flour or
        // bread made from it (closing fix: the boat bought grain and sold the
        // island flour and bread at the same call).
        if feeds_a_dear_good(world, &buy.good) {
            continue;
        }
        let Some(unit) = price(world, &buy.good, buy.price_percent) else {
            continue;
        };
        let depots = civic::depots(world, settlement);
        let total: u32 = depots
            .iter()
            .filter_map(|(_, entity)| world.get::<Depot>(*entity))
            .map(|depot| {
                depot
                    .available(&buy.good, None, now)
                    .saturating_sub(consigned(depot, &buy.good))
            })
            .sum();
        let (already, coins) = {
            let merchant = world.resource::<Merchant>();
            (
                merchant.visit.bought.get(&buy.good).copied().unwrap_or(0),
                merchant.purse,
            )
        };
        let (offer, target) = island_stock(world, &buy.good);
        let surplus = offer.saturating_sub(target);
        let mut left = total
            .saturating_sub(buy.keep_units)
            .min(surplus)
            .min(buy.max_units.saturating_sub(already))
            .min(coins / unit);
        if left == 0 {
            continue;
        }
        let mut bought = 0;
        for (store_id, store) in depots {
            if left == 0 {
                break;
            }
            let Some(mut depot) = world.get_mut::<Depot>(store) else {
                continue;
            };
            // The boat's own consigned units are not for sale to it; they
            // are the last owner, so `take` reaches them only past these.
            let take = depot
                .available(&buy.good, None, now)
                .saturating_sub(consigned(&depot, &buy.good))
                .min(left);
            if take == 0 {
                continue;
            }
            depot.sold_units = depot.sold_units.saturating_add(u64::from(take));
            depot.turnover = depot.turnover.saturating_add(u64::from(take * unit));
            let lots = depot.take(&buy.good, take);
            left -= take;
            bought += take;
            for (owner, units) in lots {
                let coins = unit * units;
                let payee = (owner != CIVIC_OWNER)
                    .then(|| world.resource::<SimIndex>().get(owner))
                    .flatten()
                    .filter(|entity| {
                        living(world, *entity)
                            && world
                                .get::<Wallet>(*entity)
                                .is_some_and(|wallet| wallet.coins.checked_add(coins).is_some())
                    });
                match payee {
                    Some(entity) => {
                        world.get_mut::<Wallet>(entity).expect("payee").coins += coins;
                        record_effect(
                            world,
                            Some(owner),
                            "VillageStallSale",
                            &json!({"store":store_id,"kind":buy.good,"quantity":units,
                                "coins":coins,"unit_price":unit,"buyer":"merchant boat"}),
                        );
                    }
                    None => {
                        let mut state = world.resource_mut::<VillageState>();
                        state.fund = state.fund.saturating_add(coins);
                    }
                }
            }
            market::record_export(world, store, &buy.good, take, take * unit);
        }
        if bought > 0 {
            let coins = bought * unit;
            let mut merchant = world.resource_mut::<Merchant>();
            merchant.purse -= coins;
            merchant.coins_paid += u64::from(coins);
            merchant.visit.bought_units += bought;
            merchant.visit.bought_coins += coins;
            *merchant.visit.bought.entry(buy.good.clone()).or_default() += bought;
            *merchant.exported.entry(buy.good.clone()).or_default() += u64::from(bought);
            world.resource_mut::<VillageState>().transactions += 1;
        }
    }
}

/// Whether `kind` is the island's grain while something made from it (its
/// flour, its bread) is dear (above its base price). Only the grain: fuel
/// goes into every oven, and logs must still leave as surplus.
fn feeds_a_dear_good(world: &World, kind: &str) -> bool {
    let Some(registry) = economy_spec::registry(world) else {
        return false;
    };
    if kind != super::roles::roles(world).grain {
        return false;
    }
    let dear = |good: &str| {
        market::unit_price(world, good)
            .zip(catalog::price_in(world, good))
            .is_some_and(|(price, base)| price > base)
    };
    let outputs = |input: &str| -> Vec<&str> {
        registry
            .spec
            .recipes
            .iter()
            .filter(|recipe| recipe.inputs.iter().any(|stack| stack.good == input))
            .flat_map(|recipe| recipe.outputs.iter().map(|stack| stack.good.as_str()))
            .collect()
    };
    outputs(kind)
        .into_iter()
        .any(|made| dear(made) || outputs(made).into_iter().any(dear))
}

/// The announced order is bought first, at its price, from whatever lies at
/// the port's stores and stalls (never the boat's own consigned goods), up
/// to what it still wants and the purse can pay; each owner is paid at once.
fn buy_order(world: &mut World, settlement: SimId) {
    let now = world.resource::<SimClock>().tick;
    let lines = world.resource::<Merchant>().order.clone();
    for (line, order) in lines.iter().enumerate() {
        let unit = order.unit_price.max(1);
        let purse = world.resource::<Merchant>().purse;
        let mut left = order
            .units
            .saturating_sub(order.delivered)
            .min(purse / unit);
        if left == 0 {
            continue;
        }
        let mut bought = 0;
        for (store_id, store) in civic::depots(world, settlement) {
            if left == 0 {
                break;
            }
            let Some(mut depot) = world.get_mut::<Depot>(store) else {
                continue;
            };
            let take = depot
                .available(&order.good, None, now)
                .saturating_sub(consigned(&depot, &order.good))
                .min(left);
            if take == 0 {
                continue;
            }
            depot.sold_units = depot.sold_units.saturating_add(u64::from(take));
            depot.turnover = depot.turnover.saturating_add(u64::from(take * unit));
            let lots = depot.take(&order.good, take);
            left -= take;
            bought += take;
            for (owner, units) in lots {
                let coins = unit * units;
                let payee = (owner != CIVIC_OWNER)
                    .then(|| world.resource::<SimIndex>().get(owner))
                    .flatten()
                    .filter(|entity| {
                        living(world, *entity)
                            && world
                                .get::<Wallet>(*entity)
                                .is_some_and(|wallet| wallet.coins.checked_add(coins).is_some())
                    });
                match payee {
                    Some(entity) => {
                        world.get_mut::<Wallet>(entity).expect("payee").coins += coins;
                        record_effect(
                            world,
                            Some(owner),
                            "VillageStallSale",
                            &json!({"store":store_id,"kind":order.good,"quantity":units,
                                "coins":coins,"unit_price":unit,"buyer":"merchant boat order"}),
                        );
                    }
                    None => {
                        let mut state = world.resource_mut::<VillageState>();
                        state.fund = state.fund.saturating_add(coins);
                    }
                }
            }
            // Not booked as an island trade: the order's price is the boat's,
            // and it must not move the island's quotes or the margins that
            // pull residents into a trade (a standing order is demand for the
            // people already in it).
        }
        if bought > 0 {
            let coins = bought * unit;
            let mut merchant = world.resource_mut::<Merchant>();
            merchant.purse -= coins;
            merchant.coins_paid += u64::from(coins);
            merchant.order[line].delivered += bought;
            merchant.visit.bought_units += bought;
            merchant.visit.bought_coins += coins;
            *merchant.visit.bought.entry(order.good.clone()).or_default() += bought;
            *merchant.exported.entry(order.good.clone()).or_default() += u64::from(bought);
            world.resource_mut::<VillageState>().transactions += 1;
        }
    }
}

/// The civic fund buys what the port is short of into the market (else the
/// trading post); what the fund cannot pay for is laid there on consignment
/// ([`MERCHANT_OWNER`]) and paid to the boat's purse when residents buy it.
fn sell_imports(world: &mut World, spec: &MerchantSpec, settlement: SimId, post: Entity) {
    let now = world.resource::<SimClock>().tick;
    // The trade balance (closing fix): this visit's sales stay within what
    // the boat bought plus its share of `trade_balance_percent` of the
    // island's coins a year.
    let allowance = {
        let island: u64 = world
            .try_query::<&Wallet>()
            .map(|mut query| {
                query
                    .iter(world)
                    .map(|wallet| u64::from(wallet.coins))
                    .sum()
            })
            .unwrap_or(0)
            + u64::from(world.resource::<VillageState>().fund);
        let year_days = super::days_per_season(world).max(1) * 4;
        let visits = (year_days / spec.interval_days.max(1)).max(1);
        (island * u64::from(spec.trade_balance_percent) / 100 / visits) as u32
    };
    for sell in &spec.sells {
        let Some(unit) = price(world, &sell.good, sell.price_percent) else {
            continue;
        };
        let budget = {
            let visit = &world.resource::<Merchant>().visit;
            if spec.trade_balance_percent >= 100 {
                u32::MAX
            } else {
                visit
                    .bought_coins
                    .saturating_add(allowance)
                    .saturating_sub(visit.sold_coins.saturating_add(visit.consigned_coins))
            }
        };
        let aboard = world
            .resource::<Merchant>()
            .hold
            .get(&sell.good)
            .copied()
            .unwrap_or(0);
        // What the port lacks of its kept level, or the island of its stock
        // targets (at most `want_units` a round: a crop's target is a whole
        // season's harvest, which the boat must not replace), whichever is
        // more.
        // The island is short only while the good is dear (above its base
        // price): below its target but cheap, it is not missing.
        let (offer, target) = island_stock(world, &sell.good);
        let dear = market::unit_price(world, &sell.good)
            .zip(catalog::price_in(world, &sell.good))
            .is_some_and(|(price, base)| price > base);
        // Only against a real shortage (closing fix): the port's kept level
        // counted even while the good was cheap, so the boat sold staples at
        // every call and the island's coins drained to its purse.
        let lacking = if dear {
            sell.want_units
                .saturating_sub(port_stock(world, settlement, &sell.good))
                .max(target.saturating_sub(offer).min(sell.want_units))
        } else {
            0
        };
        let room = world
            .get::<Depot>(post)
            .map_or(0, |depot| depot.room(None, now));
        let wanted = aboard.min(lacking).min(room).min(budget / unit.max(1));
        let units = wanted.min(civic::budget(world) / unit);
        let consign = wanted - units;
        if consign > 0 {
            world
                .get_mut::<Depot>(post)
                .expect("market")
                .add(&sell.good, MERCHANT_OWNER, consign);
            let mut merchant = world.resource_mut::<Merchant>();
            if let Some(left) = merchant.hold.get_mut(&sell.good) {
                *left -= consign;
            }
            merchant.hold.retain(|_, units| *units > 0);
            merchant.visit.sold_units += consign;
            merchant.visit.consigned_coins = merchant
                .visit
                .consigned_coins
                .saturating_add(consign.saturating_mul(unit));
            *merchant.visit.sold.entry(sell.good.clone()).or_default() += consign;
            *merchant.imported.entry(sell.good.clone()).or_default() += u64::from(consign);
        }
        if units == 0 {
            continue;
        }
        let coins = units * unit;
        {
            let mut state = world.resource_mut::<VillageState>();
            state.fund -= coins;
            state.transactions += 1;
        }
        world
            .get_mut::<Depot>(post)
            .expect("trading post")
            .add(&sell.good, CIVIC_OWNER, units);
        let mut merchant = world.resource_mut::<Merchant>();
        merchant.purse = merchant.purse.saturating_add(coins);
        merchant.coins_earned += u64::from(coins);
        if let Some(left) = merchant.hold.get_mut(&sell.good) {
            *left -= units;
        }
        merchant.hold.retain(|_, units| *units > 0);
        merchant.visit.sold_units += units;
        merchant.visit.sold_coins += coins;
        *merchant.visit.sold.entry(sell.good.clone()).or_default() += units;
        *merchant.imported.entry(sell.good.clone()).or_default() += u64::from(units);
    }
}

/// The boat duty on a visit's trade worth `value`: `boat_duty_percent` of it
/// (rounded to the nearest coin) moves from the boat's purse to the civic
/// fund, never more than the purse holds.
fn duty(world: &mut World, value: u32) {
    let percent = economy_spec::tuning(world).map_or(0, |tuning| tuning.boat_duty_percent);
    let due = ((u64::from(value) * u64::from(percent) + 50) / 100) as u32;
    let paid = due.min(world.resource::<Merchant>().purse);
    if paid == 0 {
        return;
    }
    {
        let mut merchant = world.resource_mut::<Merchant>();
        merchant.purse -= paid;
        merchant.duty_paid += u64::from(paid);
    }
    let mut state = world.resource_mut::<VillageState>();
    state.fund = state.fund.saturating_add(paid);
    record_effect(
        world,
        None,
        "VillageBoatDuty",
        &json!({"coins":paid,"trade_value":value,"percent":percent}),
    );
}

fn trade(world: &mut World, spec: &MerchantSpec) {
    let (Some(settlement), Some(post)) = ({
        let merchant = world.resource::<Merchant>();
        (merchant.settlement, merchant.post)
    }) else {
        return;
    };
    let Some(post) = world.resource::<SimIndex>().get(post) else {
        return;
    };
    buy_exports(world, spec, settlement);
    // Imports land at the market near the centre, else at the post.
    let shelf = world
        .resource::<Merchant>()
        .market
        .and_then(|id| world.resource::<SimIndex>().get(id))
        .unwrap_or(post);
    sell_imports(world, spec, settlement, shelf);
    super::colony_demand_changed(world);
}

fn land(world: &mut World, spec: &MerchantSpec, now: u64) {
    {
        let mut merchant = world.resource_mut::<Merchant>();
        merchant.docked_until = Some(now + spec.stay_ticks);
        // What the last departure announced; the first visit carries the
        // listed units.
        merchant.hold = if merchant.visits == 0 {
            spec.sells
                .iter()
                .map(|sell| (sell.good.clone(), sell.units))
                .collect()
        } else {
            std::mem::take(&mut merchant.next_hold)
        };
        merchant.visit = VisitTotals {
            arrived_at: now,
            ..Default::default()
        };
        merchant.visits += 1;
    }
    let settlers = super::demography::arrive(world);
    {
        let mut merchant = world.resource_mut::<Merchant>();
        merchant.visit.settlers = settlers;
        merchant.settlers += settlers;
    }
    let (purse, hold, visit) = {
        let merchant = world.resource::<Merchant>();
        (merchant.purse, merchant.hold.clone(), merchant.visits)
    };
    record_effect(
        world,
        None,
        "VillageMerchantArrived",
        &json!({"visit":visit,"purse":purse,"aboard":hold,"settlers":settlers,
            "leaves_at":now + spec.stay_ticks}),
    );
    trade(world, spec);
}

fn leave(world: &mut World, spec: &MerchantSpec, now: u64, reason: &str) {
    trade(world, spec);
    let day = super::calendar(world).0;
    let totals = {
        let mut merchant = world.resource_mut::<Merchant>();
        merchant.docked_until = None;
        merchant.hold.clear();
        merchant.visit.left_at = now;
        merchant.next_visit_day = day + spec.interval_days;
        let totals = merchant.visit.clone();
        merchant.last_visit = Some(totals.clone());
        totals
    };
    // The duty is settled once a visit, on everything traded: per-trade
    // rounding would lose most of it on small lots.
    duty(world, totals.bought_coins.saturating_add(totals.sold_coins));
    drift(world, spec);
    announce(world, spec);
    let (purse, visit) = {
        let merchant = world.resource::<Merchant>();
        (merchant.purse, merchant.visits)
    };
    record_effect(
        world,
        None,
        "VillageMerchantVisit",
        &json!({"visit":visit,"reason":reason,"bought_units":totals.bought_units,
            "bought_coins":totals.bought_coins,"sold_units":totals.sold_units,
            "sold_coins":totals.sold_coins,"bought":totals.bought,"sold":totals.sold,
            "settlers":totals.settlers,"purse":purse,
            "next_visit_day":day + spec.interval_days + 1}),
    );
}

/// Once per tick in the regional Effects pass.
pub fn tick(world: &mut World) {
    let Some(spec) = spec(world).cloned() else {
        return;
    };
    let now = world.resource::<SimClock>().tick;
    if !world.resource::<Merchant>().founded && now >= 1 {
        let settlement = world.resource::<Merchant>().settlement;
        let berth = settlement.and_then(|settlement| berth_of(world, settlement));
        let post = settlement.and_then(|settlement| {
            civic::found_building(
                world,
                &spec.post,
                settlement,
                berth,
                "founded with the village at the waterfront",
            )
        });
        // The market near the centre, where the imports are sold.
        let market = settlement
            .filter(|_| !spec.market.is_empty())
            .and_then(|settlement| {
                civic::found_building(
                    world,
                    &spec.market,
                    settlement,
                    None,
                    "founded with the village as its market",
                )
            });
        let mut merchant = world.resource_mut::<Merchant>();
        merchant.founded = true;
        merchant.post = post;
        merchant.berth = berth;
        merchant.market = market;
    }
    if !now.is_multiple_of(WATCH_TICKS) {
        return;
    }
    let (docked_until, next_visit_day, ready) = {
        let merchant = world.resource::<Merchant>();
        (
            merchant.docked_until,
            merchant.next_visit_day,
            merchant.post.is_some() && merchant.settlement.is_some(),
        )
    };
    if !ready {
        return;
    }
    let storm = super::weather::storm(world);
    if let Some(until) = docked_until {
        if storm {
            leave(world, &spec, now, "a storm drove the boat off");
        } else if now >= until {
            leave(world, &spec, now, "the boat sailed on schedule");
        } else if now.is_multiple_of(TRADE_ROUND_TICKS) {
            trade(world, &spec);
        }
        return;
    }
    let (day, _, night) = super::calendar(world);
    if day < next_visit_day
        || night
        || super::resident::day_permille(world) < spec.arrive_permille as i32
    {
        return;
    }
    if storm {
        let delays = {
            let mut merchant = world.resource_mut::<Merchant>();
            merchant.next_visit_day = day + 1;
            merchant.delays += 1;
            merchant.delays
        };
        record_effect(
            world,
            None,
            "VillageMerchantDelayed",
            &json!({"reason":"storm","next_visit_day":day + 2,"delays":delays}),
        );
        return;
    }
    land(world, &spec, now);
}

/// The boat, for panels and for drawing it at the port.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct BoatView {
    /// The world has a merchant boat.
    pub enabled: bool,
    pub docked: bool,
    /// Tick the boat leaves, while it is in.
    pub leaves_at: Option<u64>,
    /// Calendar day (first day is 1) and earliest tick of the next landing.
    pub next_visit_day: u64,
    pub next_visit_tick: u64,
    pub purse: u32,
    /// Goods aboard for sale, while it is in.
    pub aboard: Vec<(String, u32)>,
    pub settlement: Option<SimId>,
    pub post: Option<SimId>,
    /// Entrance of the trading post, beside the berth.
    pub dock: Option<SimPos>,
    /// Where the boat ties up: the centre of the sea cell nearest to the
    /// port's centre. Draw the boat (and a jetty) here.
    pub berth: Option<SimPos>,
    /// The port's market stall near its centre, where the imports are sold.
    pub market: Option<SimId>,
    pub visits: u32,
    pub this_visit: Option<VisitTotals>,
    pub last_visit: Option<VisitTotals>,
    /// What the boat buys and sells, with today's prices.
    pub buys: Vec<(String, u32)>,
    pub sells: Vec<(String, u32)>,
    /// What the boat announced it will buy at its next call ("next call
    /// wants 10 plank at 4"): good, units, price and units already bought.
    pub next_order: Vec<BoatOrder>,
}

/// Boat status and next visit.
pub fn status(world: &World) -> BoatView {
    let (Some(spec), Some(merchant)) = (spec(world), world.get_resource::<Merchant>()) else {
        return BoatView {
            enabled: false,
            docked: false,
            leaves_at: None,
            next_visit_day: 0,
            next_visit_tick: 0,
            purse: 0,
            aboard: Vec::new(),
            settlement: None,
            post: None,
            dock: None,
            berth: None,
            market: None,
            visits: 0,
            this_visit: None,
            last_visit: None,
            buys: Vec::new(),
            sells: Vec::new(),
            next_order: Vec::new(),
        };
    };
    let config = world.resource::<SimConfig>();
    let day_length = config.day_length.max(1);
    let today = super::calendar(world).0;
    let now = world.resource::<SimClock>().tick;
    let start_of_today = now - now % day_length;
    let next_visit_tick = start_of_today
        + merchant.next_visit_day.saturating_sub(today) * day_length
        + day_length * u64::from(spec.arrive_permille) / 1000;
    BoatView {
        enabled: true,
        docked: merchant.docked_until.is_some(),
        leaves_at: merchant.docked_until,
        next_visit_day: merchant.next_visit_day + 1,
        next_visit_tick: next_visit_tick.max(now),
        purse: merchant.purse,
        aboard: merchant
            .hold
            .iter()
            .map(|(kind, units)| (kind.clone(), *units))
            .collect(),
        settlement: merchant.settlement,
        post: merchant.post,
        dock: merchant
            .post
            .and_then(|id| world.resource::<SimIndex>().get(id))
            .and_then(|entity| world.get::<civic::Building>(entity))
            .map(|building| building.plot.door()),
        berth: merchant.berth,
        market: merchant.market,
        visits: merchant.visits,
        this_visit: merchant.docked_until.map(|_| merchant.visit.clone()),
        last_visit: merchant.last_visit.clone(),
        buys: spec
            .buys
            .iter()
            .filter_map(|buy| {
                Some((
                    buy.good.clone(),
                    price(world, &buy.good, buy.price_percent)?,
                ))
            })
            .collect(),
        sells: spec
            .sells
            .iter()
            .filter_map(|sell| {
                Some((
                    sell.good.clone(),
                    price(world, &sell.good, sell.price_percent)?,
                ))
            })
            .collect(),
        next_order: merchant.order.clone(),
    }
}

/// Rations the boat lands with a settler: goods that enter the world.
pub(super) fn land_rations(world: &mut World, actor: Entity) {
    let roles = super::roles::roles(world);
    for (kind, units) in [(roles.baked, 2_u32), (roles.drink, 2)] {
        econ::grant(
            world,
            actor,
            npc_framework::interaction::Item::new(kind).stack(units),
        );
        *world
            .resource_mut::<Merchant>()
            .imported
            .entry(kind.into())
            .or_default() += u64::from(units);
    }
    let _ = world.get::<Villager>(actor);
}
