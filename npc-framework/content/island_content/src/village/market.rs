//! A physical market with finite offers and slowly learned commodity references.
//!
//! Inspired by BazaarBot's inventory-backed offers and outcome-based price learning.
//! This is an independent integer implementation, not its remote auction model:
//! every settlement still validates reach, money, goods and carrying capacity.
use super::{Villager, Wallet, catalog, economy as econ, living};
use bevy_app::App;
use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use npc_framework::crafting::RecipeRegistry;
use npc_framework::kernel::hash::SimHashResource;
use npc_framework::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub const MAX_UNIT_PRICE: u32 = 1_000_000;
const HISTORY_LIMIT: usize = 16;
const LISTING_LIMIT: usize = 32;
const MILLI: u32 = 1000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub struct Listing {
    pub quantity: u32,
    pub minimum_unit_price: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub struct MarketSample {
    pub tick: u64,
    /// Price used to fund this round's offers, before its learning step.
    pub offered_at_unit_price: u32,
    pub unit_price: u32,
    pub supply: u32,
    pub funded_demand: u32,
    /// Genuine purchase intentions backed below the current quote. These do
    /// not count as executable demand or create upward scarcity pressure.
    #[serde(default)]
    pub below_reference_bid_units: u32,
    /// Conditional unit ceiling from the remaining cash of buyers that can
    /// carry this good. It is indicative, not a simultaneous escrow promise.
    #[serde(default)]
    pub max_affordable_bid: Option<u32>,
    pub traded_units: u64,
    pub turnover: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub struct Commodity {
    pub reference_milli: u32,
    pub pending_units: u64,
    pub pending_turnover: u64,
    pub total_units: u64,
    pub total_turnover: u64,
    pub history: Vec<MarketSample>,
}

#[derive(Resource, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub struct MarketState {
    pub last_round: Option<u64>,
    pub commodities: BTreeMap<String, Commodity>,
    pub listings: BTreeMap<SimId, BTreeMap<String, Listing>>,
}

fn whole_price(reference_milli: u32) -> u32 {
    (reference_milli.saturating_add(MILLI / 2) / MILLI).clamp(1, MAX_UNIT_PRICE)
}

impl MarketState {
    /// The same rounded reference used by simulation quotes, available to a
    /// read-only view resource without requiring access to its simulation World.
    pub fn price(&self, kind: &str) -> Option<u32> {
        self.commodities
            .get(kind)
            .map(|row| whole_price(row.reference_milli))
    }
}

impl SimHashResource for MarketState {
    const NAME: &'static str = "island_market";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.opt(self.last_round.as_ref(), |h, tick| h.u64(*tick));
        h.u32(self.commodities.len() as u32);
        for (kind, row) in &self.commodities {
            h.str(kind);
            h.u32(row.reference_milli);
            h.u64(row.pending_units);
            h.u64(row.pending_turnover);
            h.u64(row.total_units);
            h.u64(row.total_turnover);
            h.u32(row.history.len() as u32);
            for sample in &row.history {
                h.u64(sample.tick);
                h.u32(sample.offered_at_unit_price);
                h.u32(sample.unit_price);
                h.u32(sample.supply);
                h.u32(sample.funded_demand);
                h.u32(sample.below_reference_bid_units);
                h.opt(sample.max_affordable_bid.as_ref(), |h, price| h.u32(*price));
                h.u64(sample.traded_units);
                h.u64(sample.turnover);
            }
        }
        h.u32(self.listings.len() as u32);
        for (actor, listings) in &self.listings {
            h.u32(actor.0);
            h.u32(listings.len() as u32);
            for (kind, listing) in listings {
                h.str(kind);
                h.u32(listing.quantity);
                h.opt(listing.minimum_unit_price.as_ref(), |h, p| h.u32(*p));
            }
        }
    }
    fn snapshot_fns() -> Option<npc_framework::snapshot::ResourceSnapshotFns> {
        Some(npc_framework::snapshot::serde_resource_fns::<Self>())
    }
}

/// Called only for the opt-in economy, before content freeze.
pub fn register(app: &mut App) {
    app.register_sim_resource::<MarketState>();
    app.init_resource::<MarketState>();
}

fn commodity(seed: u32) -> Commodity {
    Commodity {
        reference_milli: seed.clamp(1, MAX_UNIT_PRICE) * MILLI,
        pending_units: 0,
        pending_turnover: 0,
        total_units: 0,
        total_turnover: 0,
        history: Vec::new(),
    }
}

/// Indicative commodity price in whole coins. Beliefs retain milli-coin precision.
/// Quoting is pure; prices change only at simulation market rounds.
pub fn unit_price(world: &World, kind: &str) -> Option<u32> {
    let seed = catalog::price_in(world, kind)?;
    if !catalog::trade_kinds(world).contains(&kind) {
        return None;
    }
    Some(
        world
            .get_resource::<MarketState>()
            .and_then(|state| state.price(kind))
            .unwrap_or(seed)
            .clamp(1, MAX_UNIT_PRICE),
    )
}

/// The learned reference of a traded good in milli-coins (its base price
/// before the first round).
pub fn reference_milli(world: &World, kind: &str) -> Option<u32> {
    let seed = catalog::price_in(world, kind)?;
    Some(
        world
            .get_resource::<MarketState>()
            .and_then(|state| state.commodities.get(kind))
            .map_or(seed.saturating_mul(MILLI), |row| row.reference_milli),
    )
}

/// The reservation price a seller holds to (regional): residents and stores
/// do; a played character or visitor sets its own minimum in an offer.
fn reservation_for(world: &World, seller: Entity, kind: &str) -> Option<u32> {
    if world
        .get::<Villager>(seller)
        .is_some_and(|person| person.role == super::Role::Visitor)
    {
        return None;
    }
    super::stock::reservation(world, kind)
}

fn listing(world: &World, actor: Entity, kind: &str) -> Option<Listing> {
    let id = world.get::<SimId>(actor)?;
    world
        .get_resource::<MarketState>()?
        .listings
        .get(id)?
        .get(kind)
        .copied()
}

/// Living world: `actor` (a resident with a trade) has no use for `kind`: it
/// is not food, water, clothing, the hearth's fuel or the trade's tool, and
/// its current trade neither makes nor uses it. Such goods (a former tailor's
/// thread, say) are carried whole to a store (`civic::surplus_trip`), which
/// sells them to the trade that uses them. (Not part of the per-person seller
/// test: that runs in every market scan.)
pub fn no_use(world: &World, actor: Entity, kind: &str) -> bool {
    use super::frontier::economy_spec::GoodTag;
    let Some(role) = world.get::<Villager>(actor).map(|person| person.role) else {
        return false;
    };
    // What everybody uses is read from the data's tags: food, drink,
    // clothing, fuel; the trade's own goods and tool from its profession.
    if role == super::Role::Visitor
        || catalog::outputs_for(world, role).contains(&kind)
        || catalog::wants_for(world, role, kind)
        || catalog::role_tool_in(world, role) == Some(kind)
        || [
            GoodTag::Food,
            GoodTag::Drink,
            GoodTag::Clothing,
            GoodTag::Fuel,
        ]
        .into_iter()
        .any(|tag| catalog::has_tag(world, kind, tag))
        || !super::frontier::economy_spec::tuning(world).is_some_and(|t| t.unused_goods_to_market)
    {
        return false;
    }
    catalog::trade_kinds(world).contains(&kind)
}

/// Explicit offers can include a visitor's goods. Unlisted visitor stock is private.
fn automatic_seller(world: &World, actor: Entity, kind: &str) -> bool {
    if let Some(profile) = super::frontier::labor::profile(world, actor) {
        // A change of profession does not make previously produced real goods
        // disappear from the market. Personal reserves remain protected below.
        return profile
            .allowed_roles
            .iter()
            .any(|role| catalog::outputs_for(world, *role).contains(&kind));
    }
    world
        .get::<Villager>(actor)
        .is_some_and(|v| catalog::outputs_for(world, v.role).contains(&kind))
}

pub fn sells(world: &World, actor: Entity, kind: &str) -> bool {
    // A store or stall is a seller of record for whatever lies on its shelf.
    if let Some(depot) = world.get::<super::civic::Depot>(actor) {
        return depot.stock(kind) > 0;
    }
    person_sells(world, actor, kind)
}

/// [`sells`] for a living actor only. Scans over every entity use this, so a
/// store costs them nothing; stores are ranked through the civic index.
pub(super) fn person_sells(world: &World, actor: Entity, kind: &str) -> bool {
    if !living(world, actor) {
        return false;
    }
    if listing(world, actor, kind).is_some() {
        return available_to_sell(world, actor, kind) > 0;
    }
    automatic_seller(world, actor, kind)
}

/// Published quantity is bounded by actual, unreserved physical stock.
pub fn available_to_sell(world: &World, actor: Entity, kind: &str) -> u32 {
    if let Some(depot) = world.get::<super::civic::Depot>(actor) {
        return depot.available(kind, None, world.resource::<SimClock>().tick);
    }
    let stock = econ::count(world, actor, kind);
    if let Some(offer) = listing(world, actor, kind) {
        return offer.quantity.min(stock);
    }
    if !automatic_seller(world, actor, kind) {
        return 0;
    }
    stock.saturating_sub(super::production::desired_stock(world, actor, kind))
}

pub fn asking_price(world: &World, seller: Entity, kind: &str) -> Option<u32> {
    let reference = unit_price(world, kind)?;
    Some({
        listing(world, seller, kind)
            .and_then(|offer| offer.minimum_unit_price)
            .map_or(reference, |minimum| reference.max(minimum))
            .max(reservation_for(world, seller, kind).unwrap_or(0))
    })
}

/// Whether `kind` is a staple: water, a food, a fuel or the winter coat.
pub fn is_staple(world: &World, kind: &str) -> bool {
    let roles = super::roles::roles(world);
    kind == roles.drink
        || kind == roles.garment
        || roles.fuels.contains(&kind)
        || catalog::food_restore_in(world, kind).is_some()
}

/// The price `buyer` pays `seller` for one unit of `kind` (closing fix A).
/// For a staple a resident's planned bid below the asking price stands, as
/// long as it covers what the seller's own inputs cost (`reservation_for`:
/// input cost plus the maker's margin, or the raw floor for a gathered
/// good) and any minimum the seller listed: a need about to go unmet is
/// served at a price that still pays the maker, never below it.
pub fn price_for(world: &World, buyer: Entity, seller: Entity, kind: &str) -> Option<u32> {
    let asking = asking_price(world, seller, kind)?;
    if !is_staple(world, kind) {
        return Some(asking);
    }
    let Some(bid) = super::production::purchase_bid_limit(world, buyer, kind) else {
        return Some(asking);
    };
    let floor = reservation_for(world, seller, kind)
        .unwrap_or(1)
        .max(
            listing(world, seller, kind)
                .and_then(|offer| offer.minimum_unit_price)
                .unwrap_or(1),
        )
        .max(1);
    Some(asking.min(bid.max(floor)))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TradeQuote {
    pub unit_price: u32,
    pub total: u32,
}

/// The same live validation used by clients and autonomous physical settlement.
/// No rejected request is a price-learning observation.
pub fn quote(
    world: &World,
    buyer: Entity,
    seller: Entity,
    kind: &str,
    n: u32,
) -> Result<TradeQuote, String> {
    if buyer == seller || !living(world, buyer) || !living(world, seller) {
        return Err("trade needs two living actors".into());
    }
    if !(1..=20).contains(&n) {
        return Err("quantity must be 1..=20".into());
    }
    if !econ::near(world, buyer, seller) {
        return Err("move within 1.5 m of the trader".into());
    }
    let unit_price = price_for(world, buyer, seller, kind).ok_or("this good is not traded here")?;
    let total = unit_price.checked_mul(n).ok_or("price overflow")?;
    // A seller fond of a played character takes something off (regional).
    let discount = super::live::discount_percent(world, buyer, seller);
    let total = (total - total * discount / 100).max(1);
    let b = world.get::<Wallet>(buyer).ok_or("buyer has no wallet")?;
    let s = world.get::<Wallet>(seller).ok_or("seller has no wallet")?;
    if b.coins < total {
        return Err(format!("buyer cannot afford {total} coins"));
    }
    if s.coins.checked_add(total).is_none() {
        return Err("seller wallet is full".into());
    }
    {
        let automatic_stock = automatic_seller(world, seller, kind);
        if (automatic_stock || listing(world, seller, kind).is_some())
            && available_to_sell(world, seller, kind) < n
        {
            return Err("not enough goods are currently offered for sale".into());
        }
    }
    let goods = econ::selected(world, seller, kind, n)?;
    econ::fit(
        world,
        buyer,
        &goods.into_iter().map(|(_, item)| item).collect::<Vec<_>>(),
    )?;
    Ok(TradeQuote { unit_price, total })
}

pub fn input_cost(world: &World, recipe: &str) -> Option<u32> {
    let def = world
        .get_resource::<RecipeRegistry>()?
        .recipes
        .get(recipe)?;
    def.inputs.iter().try_fold(0_u32, |total, input| {
        total.checked_add(unit_price(world, &input.key)?.checked_mul(input.count)?)
    })
}

pub fn check_offer(
    world: &World,
    actor: Entity,
    kind: &str,
    quantity: u32,
    minimum: Option<u32>,
) -> Result<(), String> {
    if !living(world, actor) || world.get::<SimId>(actor).is_none() {
        return Err("only a living actor can offer goods".into());
    }
    if !catalog::trade_kinds(world).contains(&kind) {
        return Err("this good is not traded here".into());
    }
    if !(1..=20).contains(&quantity) || econ::count(world, actor, kind) < quantity {
        return Err("offer 1..=20 goods from your unreserved inventory".into());
    }
    if minimum.is_some_and(|p| !(1..=MAX_UNIT_PRICE).contains(&p)) {
        return Err(format!("minimum unit price must be 1..={MAX_UNIT_PRICE}"));
    }
    let state = world
        .get_resource::<MarketState>()
        .ok_or("market is not initialized")?;
    let id = world.get::<SimId>(actor).expect("checked actor");
    if state
        .listings
        .get(id)
        .is_some_and(|offers| offers.len() >= LISTING_LIMIT && !offers.contains_key(kind))
    {
        return Err("too many active goods offers".into());
    }
    Ok(())
}

pub fn post_offer(
    world: &mut World,
    actor: Entity,
    kind: &str,
    quantity: u32,
    minimum: Option<u32>,
) -> Result<(), String> {
    check_offer(world, actor, kind, quantity, minimum)?;
    let id = *world.get::<SimId>(actor).expect("checked actor");
    world
        .resource_mut::<MarketState>()
        .listings
        .entry(id)
        .or_default()
        .insert(
            kind.into(),
            Listing {
                quantity,
                minimum_unit_price: minimum,
            },
        );
    record_effect(
        world,
        Some(id),
        "VillageGoodsOffered",
        &json!({"kind":kind,"quantity":quantity,"minimum_unit_price":minimum}),
    );
    Ok(())
}

pub fn has_offer(world: &World, actor: Entity, kind: &str) -> bool {
    listing(world, actor, kind).is_some()
}

pub fn withdraw_offer(world: &mut World, actor: Entity, kind: &str) -> Result<(), String> {
    if !has_offer(world, actor, kind) {
        return Err("this actor has no active offer for that good".into());
    }
    let id = *world.get::<SimId>(actor).expect("offer actor");
    let mut state = world.resource_mut::<MarketState>();
    if let Some(offers) = state.listings.get_mut(&id) {
        offers.remove(kind);
        if offers.is_empty() {
            state.listings.remove(&id);
        }
    }
    record_effect(
        world,
        Some(id),
        "VillageGoodsWithdrawn",
        &json!({"kind":kind}),
    );
    Ok(())
}

/// Successful physical transfers are the only trade samples accepted by learning.
pub fn record_trade(
    world: &mut World,
    buyer: Entity,
    seller: Entity,
    kind: &str,
    quantity: u32,
    total: u32,
) {
    record_sale(world, buyer, seller, kind, quantity, total, true);
}

/// A sale to the merchant boat from a store (an export): a price sample like
/// any trade, but no use of the good on the island, so it stays out of the
/// trade window the stock targets read (S4 fix: the boat's purchases counted
/// as the settlement's daily use and raised the targets of what it bought).
pub fn record_export(world: &mut World, store: Entity, kind: &str, quantity: u32, total: u32) {
    record_sale(world, store, store, kind, quantity, total, false);
}

fn record_sale(
    world: &mut World,
    buyer: Entity,
    seller: Entity,
    kind: &str,
    quantity: u32,
    total: u32,
    island_use: bool,
) {
    let Some(seed) = catalog::price_in(world, kind) else {
        return;
    };
    let seller_id = world.get::<SimId>(seller).copied();
    if island_use {
        super::stock::record(world, seller, kind, quantity);
    }
    super::live::trade_booked(world, buyer, seller, kind, quantity, total);
    let mut state = world.resource_mut::<MarketState>();
    let row = state
        .commodities
        .entry(kind.into())
        .or_insert_with(|| commodity(seed));
    row.pending_units = row.pending_units.saturating_add(u64::from(quantity));
    row.pending_turnover = row.pending_turnover.saturating_add(u64::from(total));
    row.total_units = row.total_units.saturating_add(u64::from(quantity));
    row.total_turnover = row.total_turnover.saturating_add(u64::from(total));
    if let Some(id) = seller_id
        && let Some(offers) = state.listings.get_mut(&id)
    {
        if let Some(offer) = offers.get_mut(kind) {
            offer.quantity = offer.quantity.saturating_sub(quantity);
            if offer.quantity == 0 {
                offers.remove(kind);
            }
        }
        if offers.is_empty() {
            state.listings.remove(&id);
        }
    }
}

/// Learning blends observed clearing prices with actual unsatisfied offer pressure.
/// The response is bounded to 1/16 per round, including at price limits.
fn learn(reference: u32, supply: u32, demand: u32, units: u64, turnover: u64) -> u32 {
    if supply == 0 && demand == 0 && units == 0 {
        return reference;
    }
    let reference = i64::from(reference);
    let observed = turnover
        .saturating_mul(u64::from(MILLI))
        .checked_div(units)
        .map_or(reference, |value| {
            i64::try_from(value).unwrap_or(i64::from(MAX_UNIT_PRICE) * i64::from(MILLI))
        });
    let imbalance = i64::from(demand) - i64::from(supply);
    let mass = (i64::from(demand) + i64::from(supply)).max(1);
    let pressure = reference * imbalance / mass / 4;
    let step = (reference / 16).max(1);
    (observed.saturating_add(pressure))
        .clamp(reference - step, reference + step)
        .clamp(
            i64::from(MILLI),
            i64::from(MAX_UNIT_PRICE) * i64::from(MILLI),
        ) as u32
}

fn affordability_feedback(reference: u32, proposed: u32, max_bid: Option<u32>) -> u32 {
    let Some(max_bid) = max_bid else {
        return proposed;
    };
    let step = (reference / 16).max(1);
    proposed
        .min(max_bid.clamp(1, MAX_UNIT_PRICE) * MILLI)
        .clamp(
            reference.saturating_sub(step),
            reference.saturating_add(step),
        )
        .clamp(MILLI, MAX_UNIT_PRICE * MILLI)
}

pub fn round_ticks(world: &World) -> u64 {
    (world.resource::<SimConfig>().day_length / 8).clamp(40, 300)
}

/// The host calls production::refresh before this, in the deterministic Effects seam.
pub fn tick(world: &mut World) {
    let now = world.resource::<SimClock>().tick;
    let interval = round_ticks(world);
    if world
        .resource::<MarketState>()
        .last_round
        .is_some_and(|last| now < last.saturating_add(interval))
    {
        return;
    }
    // No market day in a storm (regional weather): nobody can gather or
    // carry, so an order that goes unfilled says nothing about scarcity.
    // Prices rest until the weather clears; goods in hand still change hands.
    if super::weather::storm(world) {
        world.resource_mut::<MarketState>().last_round = Some(now);
        return;
    }
    let kinds = catalog::trade_kinds(world);
    let people: Vec<_> = world
        .resource::<SimIndex>()
        .map
        .values()
        .copied()
        .filter(|e| world.get::<Villager>(*e).is_some() && living(world, *e))
        .collect();
    let mut supply = BTreeMap::<String, u32>::new();
    let mut demand = BTreeMap::<String, u32>::new();
    let mut lower_bids = BTreeMap::<String, u32>::new();
    let mut affordability = BTreeMap::<String, u32>::new();
    let mut bid_kinds = kinds.to_vec();
    bid_kinds.sort_by_key(|kind| {
        let priority = if *kind == super::roles::roles(world).drink {
            0
        } else if catalog::food_restore_in(world, kind).is_some() {
            1
        } else if *kind == super::roles::roles(world).garment {
            2
        } else {
            3
        };
        (priority, *kind)
    });
    for actor in &people {
        let mut budget = world.get::<Wallet>(*actor).map_or(0, |wallet| wallet.coins);
        let mut funded = BTreeMap::<&str, u32>::new();
        // Honor the planner's real investment/need allocations across ALL
        // goods before publishing any conditional future-stock counter-bid.
        for kind in &bid_kinds {
            let Some(price) = unit_price(world, kind) else {
                continue;
            };
            let missing = super::production::purchase_target(world, *actor, kind)
                .saturating_sub(econ::count(world, *actor, kind))
                .min(20);
            if missing == 0 || budget == 0 {
                continue;
            }
            let explicit_limit = super::production::purchase_bid_limit(world, *actor, kind);
            let reserved_price = explicit_limit.unwrap_or(price).min(MAX_UNIT_PRICE);
            if reserved_price == 0 {
                continue;
            }
            let backed_quantity = missing.min(budget / reserved_price);
            let capacity = (1..=backed_quantity)
                .rev()
                .find(|n| econ::fit(world, *actor, &[Item::new(*kind).stack(*n)]).is_ok())
                .unwrap_or(0);
            if capacity == 0 {
                continue;
            }
            let cash_ceiling = budget
                .checked_div(capacity)
                .unwrap_or(0)
                .min(MAX_UNIT_PRICE);
            let maximum = explicit_limit.map_or(cash_ceiling, |limit| limit.min(cash_ceiling));
            let cap = affordability.entry((*kind).into()).or_default();
            *cap = (*cap).max(maximum);
            budget -= capacity * reserved_price;
            funded.insert(*kind, capacity);
            // A whole joined-material order can be genuinely funded at its
            // lower reservation price. It is not executable at a higher quote,
            // and reserving its full limit preserves complementary inputs.
            let total = if price <= reserved_price {
                demand.entry((*kind).into()).or_default()
            } else {
                lower_bids.entry((*kind).into()).or_default()
            };
            *total = total.saturating_add(capacity);
        }
        for kind in &bid_kinds {
            let Some(price) = unit_price(world, kind) else {
                continue;
            };
            let missing = super::production::purchase_wanted(world, *actor, kind)
                .saturating_sub(econ::count(world, *actor, kind))
                .saturating_sub(funded.get(kind).copied().unwrap_or(0));
            if missing == 0 || budget == 0 || budget >= price {
                // An affordable but intentionally unfunded forecast is NOT a
                // paid order. Re-upgrading it here would undo working-capital
                // reservation and could spend a maker's entire purse on water.
                continue;
            }
            if econ::fit(world, *actor, &[Item::new(*kind)]).is_err() {
                continue;
            }
            let maximum = super::production::purchase_bid_limit(world, *actor, kind)
                .unwrap_or(budget)
                .min(budget)
                .min(MAX_UNIT_PRICE);
            if maximum == 0 {
                continue;
            }
            let cap = affordability.entry((*kind).into()).or_default();
            *cap = (*cap).max(maximum);
            let total = lower_bids.entry((*kind).into()).or_default();
            *total = total.saturating_add(1);
            // The one below-reference unit uses this buyer's actual remaining
            // bid budget, so it cannot also back another conditional bid.
            budget -= maximum;
        }
        for kind in kinds {
            // An expensive private listing is not executable supply at the
            // current reference and cannot force that reference down by spam.
            if sells(world, *actor, kind)
                && asking_price(world, *actor, kind) <= unit_price(world, kind)
            {
                let total = supply.entry((*kind).into()).or_default();
                *total = total.saturating_add(available_to_sell(world, *actor, kind));
            }
        }
    }
    // Shelves of stores and stalls are executable supply at the reference,
    // unless the reference is below the good's reservation price (regional).
    for (_, depot) in super::civic::all_depots(world) {
        if let Some(depot) = world.get::<super::civic::Depot>(depot) {
            for kind in depot.lots.keys() {
                if super::stock::reservation(world, kind)
                    .is_some_and(|reserve| Some(reserve) > unit_price(world, kind))
                {
                    continue;
                }
                let total = supply.entry(kind.clone()).or_default();
                *total = total.saturating_add(depot.stock(kind));
            }
        }
    }
    let live_offers: BTreeMap<_, BTreeMap<_, _>> = people
        .iter()
        .filter_map(|actor| {
            let id = *world.get::<SimId>(*actor)?;
            let offers: BTreeMap<_, _> = world
                .resource::<MarketState>()
                .listings
                .get(&id)?
                .iter()
                .filter_map(|(kind, listing)| {
                    let quantity = listing.quantity.min(econ::count(world, *actor, kind));
                    (quantity > 0).then(|| {
                        (
                            kind.clone(),
                            Listing {
                                quantity,
                                ..*listing
                            },
                        )
                    })
                })
                .collect();
            (!offers.is_empty()).then_some((id, offers))
        })
        .collect();
    let seeds: BTreeMap<&str, u32> = kinds
        .iter()
        .filter_map(|kind| Some((*kind, catalog::price_in(world, kind)?)))
        .collect();
    let ceiling_percent =
        super::frontier::economy_spec::tuning(world).map(|tuning| tuning.price_ceiling_percent);
    let floor_percent = super::frontier::economy_spec::tuning(world)
        .map(|tuning| tuning.price_floor_percent)
        .filter(|percent| *percent > 0);
    // Regional: stock up to the island's target is working stock. Only what is
    // offered beyond it is a glut, and an order the stock on offer can fill is
    // no scarcity. What the merchant boat's announced order still wants is
    // sold already, so it is cover too (S4: with the trade window three days
    // long, planks sawn for the boat's order and waiting for its call read
    // as a glut and held the quote at its floor for days).
    let covers: Option<BTreeMap<&str, u32>> = super::frontier::economy_spec::tuning(world)
        .filter(|tuning| tuning.stock_cover_pricing && tuning.stock_target_days > 0)
        .map(|_| {
            kinds
                .iter()
                .map(|kind| {
                    (
                        *kind,
                        super::stock::island_target(world, kind)
                            .saturating_add(super::merchant::held_for_order(world, kind)),
                    )
                })
                .collect()
        });
    // Regional: the lowest price any resident or store sells each good at.
    let reservations: BTreeMap<&str, u32> = kinds
        .iter()
        .filter_map(|kind| Some((*kind, super::stock::reservation(world, kind)?)))
        .collect();
    let mut state = world.resource_mut::<MarketState>();
    state.listings = live_offers;
    state.last_round = Some(now);
    for kind in kinds {
        let Some(seed) = seeds.get(kind).copied() else {
            continue;
        };
        let row = state
            .commodities
            .entry((*kind).into())
            .or_insert_with(|| commodity(seed));
        let offered = supply.get(*kind).copied().unwrap_or(0);
        let wanted = demand.get(*kind).copied().unwrap_or(0);
        // Regional: units delivered this round answer as many unfilled orders.
        // A good that is made to order never sits unsold, so without this one
        // open order reads as total scarcity and holds the quote at its ceiling
        // however fast the makers deliver.
        let unmet = if ceiling_percent.is_some() {
            wanted.saturating_sub(u32::try_from(row.pending_units).unwrap_or(u32::MAX))
        } else {
            wanted
        };
        let offered_at_unit_price = whole_price(row.reference_milli);
        let maximum = affordability.get(*kind).copied();
        // With stock cover, supply beyond the cover is a glut; within it the
        // stock on offer answers the open orders it can fill, so the price
        // moves with the share of orders left over, not with the stock itself.
        let (glut, short, supply_term) = match covers.as_ref().and_then(|covers| covers.get(kind)) {
            Some(cover) => {
                let glut = offered.saturating_sub(*cover);
                (
                    glut,
                    unmet.saturating_sub(offered),
                    glut.max(offered.min(unmet)),
                )
            }
            None => (offered, unmet, offered),
        };
        let proposed = learn(
            row.reference_milli,
            supply_term,
            unmet,
            row.pending_units,
            row.pending_turnover,
        );
        row.reference_milli = affordability_feedback(row.reference_milli, proposed, maximum);
        if ceiling_percent.is_some()
            && glut == 0
            && short == 0
            && row.reference_milli > seed * MILLI
        {
            // Regional: a quote above the base price with no order left unfilled
            // and nobody undercutting is stale. Made-to-order goods are never
            // left unsold, so without this a shortage price would stay up after
            // the shortage.
            let step = (row.reference_milli / 16).max(1);
            row.reference_milli = row.reference_milli.saturating_sub(step).max(seed * MILLI);
        }
        if ceiling_percent.is_some()
            && covers.is_some()
            && glut == 0
            && row.reference_milli < seed * MILLI
        {
            // Regional, with stock cover: a quote below the base price while
            // the stock on offer is within the island's cover is as stale as
            // one above it with nothing short. Without this a good nobody
            // ordered for a while kept the low quote of an old glut.
            let step = (row.reference_milli / 16).max(1);
            row.reference_milli = row.reference_milli.saturating_add(step).min(seed * MILLI);
        }
        if let Some(reserve) = reservations.get(kind)
            && glut == 0
            && row.reference_milli < reserve * MILLI
        {
            // Regional: a quote below every seller's reservation price is not
            // executable; it climbs towards the lowest ask by the usual step.
            let step = (row.reference_milli / 16).max(1);
            row.reference_milli = row
                .reference_milli
                .saturating_add(step)
                .min(reserve * MILLI);
        }
        if let Some(percent) = ceiling_percent {
            // Regional bound: scarcity may multiply a price, never without limit.
            let ceiling = u64::from(seed) * u64::from(percent) * u64::from(MILLI) / 100;
            row.reference_milli =
                (u64::from(row.reference_milli).min(ceiling.max(u64::from(MILLI)))) as u32;
        }
        if let Some(percent) = floor_percent {
            // Regional floor: unsold stock may halve a price, not erase it.
            let floor = u64::from(seed) * u64::from(percent) * u64::from(MILLI) / 100;
            row.reference_milli = (u64::from(row.reference_milli).max(floor)) as u32;
        }
        row.history.push(MarketSample {
            tick: now,
            offered_at_unit_price,
            unit_price: whole_price(row.reference_milli),
            supply: offered,
            funded_demand: wanted,
            below_reference_bid_units: lower_bids.get(*kind).copied().unwrap_or(0),
            max_affordable_bid: maximum,
            traded_units: row.pending_units,
            turnover: row.pending_turnover,
        });
        if row.history.len() > HISTORY_LIMIT {
            row.history.remove(0);
        }
        row.pending_units = 0;
        row.pending_turnover = 0;
    }
}

/// Published market news contains no individual wallet, inventory or position.
pub fn public_book(world: &World) -> Vec<Value> {
    catalog::trade_kinds(world).iter().filter_map(|kind| {
        let price = unit_price(world, kind)?;
        let row = world.get_resource::<MarketState>().and_then(|state| state.commodities.get(*kind));
        let sample = row.and_then(|row| row.history.last());
        Some(json!({"kind":kind,"unit_price":price,"round_tick":sample.map(|s|s.tick),"offered_at_unit_price":sample.map(|s|s.offered_at_unit_price),
            "offered_supply":sample.map_or(0,|s|s.supply),"funded_demand":sample.map_or(0,|s|s.funded_demand),
            "below_reference_bid_units":sample.map_or(0,|s|s.below_reference_bid_units),"max_affordable_bid":sample.and_then(|s|s.max_affordable_bid),
            "traded_units":sample.map_or(0,|s|s.traded_units),"turnover":sample.map_or(0,|s|s.turnover),
            "total_traded_units":row.map_or(0,|r|r.total_units),"history":row.map(|r|&r.history)}))
    }).collect()
}

pub fn own_offers(world: &World, actor: Entity) -> Vec<Value> {
    let Some(id) = world.get::<SimId>(actor) else {
        return Vec::new();
    };
    world.get_resource::<MarketState>().and_then(|state|state.listings.get(id))
        .into_iter().flat_map(|offers|offers.iter()).map(|(kind, offer)| json!({
            "kind":kind,"quantity":offer.quantity.min(econ::count(world,actor,kind)),
            "minimum_unit_price":offer.minimum_unit_price,"unit_price":asking_price(world,actor,kind),
        })).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn finite_unfilled_demand_raises_reference_and_unsold_surplus_lowers_it() {
        assert!(learn(10_000, 1, 9, 0, 0) > 10_000);
        assert!(learn(10_000, 9, 1, 0, 0) < 10_000);
        assert_eq!(learn(10_000, 0, 0, 0, 0), 10_000);
        assert_eq!(learn(1000, 100, 0, 0, 0), 1000);
    }
    #[test]
    fn transaction_evidence_is_volume_weighted_and_each_step_is_bounded() {
        assert_eq!(learn(16_000, 2, 2, 10, 300), 17_000);
        assert_eq!(learn(16_000, 2, 2, 10, 10), 15_000);
        assert_eq!(
            learn(MAX_UNIT_PRICE * MILLI, 0, 100, u64::MAX, u64::MAX),
            MAX_UNIT_PRICE * MILLI - MAX_UNIT_PRICE * MILLI / 16
        );
    }

    #[test]
    fn lower_priced_cash_bids_restore_affordability_without_upward_pressure() {
        assert_eq!(affordability_feedback(210_000, 210_000, Some(200)), 200_000);
        assert_eq!(affordability_feedback(100_000, 106_250, Some(50)), 93_750);
        assert_eq!(affordability_feedback(34_000, 36_125, Some(200)), 36_125);
        assert_eq!(affordability_feedback(100_000, 100_000, None), 100_000);
    }
}
