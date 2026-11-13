//! Stock targets of the regional economy: how much of a good a settlement
//! uses in a day, how much it holds for sale, and whether its makers should
//! start another batch.
//!
//! - **Daily use.** Water and food from the residents' need decay (as
//!   [`super::view::daily_use`] reports it); every other good from the
//!   recent trade volume: an integer moving sum of the units sold in the
//!   settlement over the last `trade_window_rounds` market rounds, kept in
//!   the hashed [`TradeVolume`].
//! - **Stock target.** `stock_target_days` of that use. A crop's harvest good
//!   also keeps the field's own target (`stock_per_resident`), since it is
//!   grown once a season and must last until the next harvest.
//! - **Make to stock.** A maker does not start a new batch of a good while the
//!   settlement's unsold stock and open offers of it exceed the target.
//!
//! Regional profile only: nothing here is registered or read elsewhere.
use super::{Villager, catalog, economy as econ, market};
use bevy_app::App;
use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use npc_framework::kernel::hash::SimHashResource;
use npc_framework::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Units sold per settlement and good, one entry per market round.
type Round = BTreeMap<SimId, BTreeMap<String, u32>>;

/// Recent trade volume by settlement, the moving sum behind the daily use of
/// goods that are not eaten or drunk.
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub struct TradeVolume {
    /// Units sold since the last market round.
    pub pending: Round,
    /// The last rounds, oldest first; at most `trade_window_rounds`.
    pub rounds: Vec<Round>,
}
fn hash_round(h: &mut Fnv64, round: &Round) {
    h.u32(round.len() as u32);
    for (settlement, goods) in round {
        h.u32(settlement.0);
        h.u32(goods.len() as u32);
        for (kind, units) in goods {
            h.str(kind);
            h.u32(*units);
        }
    }
}
impl SimHashResource for TradeVolume {
    const NAME: &'static str = "regional_trade_volume_v1";
    fn sim_hash(&self, h: &mut Fnv64) {
        hash_round(h, &self.pending);
        h.u32(self.rounds.len() as u32);
        for round in &self.rounds {
            hash_round(h, round);
        }
    }
    fn snapshot_fns() -> Option<npc_framework::snapshot::ResourceSnapshotFns> {
        Some(npc_framework::snapshot::serde_resource_fns::<Self>())
    }
}

/// Regional profile only.
pub fn register(app: &mut App) {
    app.init_resource::<TradeVolume>()
        .register_sim_resource::<TradeVolume>();
}

fn tuning(world: &World) -> Option<&super::frontier::economy_spec::EconomyTuning> {
    super::frontier::economy_spec::tuning(world)
}

/// The settlement a seller sells in: a resident's economic settlement, or
/// the settlement of a store or stall.
pub fn settlement_of(world: &World, seller: Entity) -> Option<SimId> {
    super::production::economic_settlement(world, seller)
        .or_else(|| super::frontier::settlements::member(world, seller))
}

/// Books a sale for the moving sum. Called by [`market::record_trade`].
pub(super) fn record(world: &mut World, seller: Entity, kind: &str, units: u32) {
    if !world.contains_resource::<TradeVolume>() {
        return;
    }
    let Some(settlement) = settlement_of(world, seller) else {
        return;
    };
    let mut volume = world.resource_mut::<TradeVolume>();
    let entry = volume
        .pending
        .entry(settlement)
        .or_default()
        .entry(kind.into())
        .or_default();
    *entry = entry.saturating_add(units);
}

/// Closes a market round: the pending sales join the window.
pub(super) fn close_round(world: &mut World) {
    if !world.contains_resource::<TradeVolume>() {
        return;
    }
    let window = tuning(world).map_or(24, |tuning| tuning.trade_window_rounds.max(1)) as usize;
    let mut volume = world.resource_mut::<TradeVolume>();
    let round = std::mem::take(&mut volume.pending);
    volume.rounds.push(round);
    let extra = volume.rounds.len().saturating_sub(window);
    volume.rounds.drain(..extra);
}

/// Units of `kind` sold in a settlement per day, from the moving sum.
pub fn volume_per_day(world: &World, settlement: SimId, kind: &str) -> u32 {
    let Some(volume) = world.get_resource::<TradeVolume>() else {
        return 0;
    };
    if volume.rounds.is_empty() {
        return 0;
    }
    let sold: u64 = volume
        .rounds
        .iter()
        .filter_map(|round| round.get(&settlement)?.get(kind))
        .map(|units| u64::from(*units))
        .sum();
    let rounds_per_day =
        (world.resource::<SimConfig>().day_length / market::round_ticks(world)).max(1);
    (sold * rounds_per_day).div_ceil(volume.rounds.len() as u64) as u32
}

/// Units of `kind` a settlement uses in a day: need decay for water and food,
/// the moving sum of trade for everything else.
pub fn daily_use(world: &World, settlement: SimId, kind: &str) -> u32 {
    if kind == super::roles::roles(world).drink || catalog::food_restore_in(world, kind).is_some() {
        return super::view::daily_use(world, settlement)
            .into_iter()
            .find(|row| row.good == kind)
            .map_or(0, |row| row.per_day)
            .max(volume_per_day(world, settlement, kind));
    }
    volume_per_day(world, settlement, kind)
}

/// The field target of a crop's harvest good in a settlement, if it is one.
fn crop_target(world: &World, settlement: SimId, kind: &str) -> u32 {
    let Some(registry) = super::frontier::economy_spec::registry(world) else {
        return 0;
    };
    let people = super::demography::members(world, settlement).len() as u32;
    registry
        .spec
        .crops
        .iter()
        .filter(|crop| crop.harvest.good == kind)
        .map(|crop| crop.stock_per_resident.saturating_mul(people))
        .max()
        .unwrap_or(0)
}

/// Units of `kind` a settlement aims to hold for sale; 0 when the rule is off.
pub fn target(world: &World, settlement: SimId, kind: &str) -> u32 {
    let days = tuning(world).map_or(0, |tuning| tuning.stock_target_days);
    if days == 0 {
        return 0;
    }
    // The overseer's food and fuel priorities scale the days kept.
    let days = {
        use super::frontier::oversee::{self, PriorityTopic};
        let food = super::frontier::economy_spec::registry(world)
            .and_then(|registry| registry.good(kind))
            .is_some_and(|good| good.food_value.is_some());
        let topic = if food {
            Some(PriorityTopic::FoodReserve)
        } else if kind == super::roles::roles(world).fuel {
            Some(PriorityTopic::FuelReserve)
        } else {
            None
        };
        topic.map_or(days, |topic| {
            oversee::scale(oversee::level(world, settlement, topic, None), days)
        })
    };
    daily_use(world, settlement, kind)
        .saturating_mul(days)
        .max(crop_target(world, settlement, kind))
        .max(batch(world, kind))
        .max(coats_wanted(world, settlement, kind))
}

/// Ahead of winter (`living::coat_season`), a coat for every resident of the
/// settlement who has none: the tailors' stock target for coats.
fn coats_wanted(world: &World, settlement: SimId, kind: &str) -> u32 {
    if kind != super::roles::roles(world).garment || !super::living::coat_season(world) {
        return 0;
    }
    // Coats at low priority: made to order only.
    if super::frontier::oversee::level(
        world,
        settlement,
        super::frontier::oversee::PriorityTopic::Coats,
        None,
    ) == super::frontier::oversee::PriorityLevel::Low
    {
        return 0;
    }
    super::demography::members(world, settlement)
        .into_iter()
        .filter(|(_, actor)| {
            !super::living::has_coat(world, *actor)
                && econ::count(world, *actor, super::roles::roles(world).garment) == 0
        })
        .count() as u32
}

/// The most units one batch of any recipe yields of `kind`: a maker always
/// keeps one batch on hand without it counting as a glut.
fn batch(world: &World, kind: &str) -> u32 {
    super::frontier::economy_spec::registry(world).map_or(0, |registry| {
        registry
            .spec
            .recipes
            .iter()
            .flat_map(|recipe| &recipe.outputs)
            .filter(|output| output.good == kind)
            .map(|output| output.count)
            .max()
            .unwrap_or(0)
    })
}

/// Units of `kind` on offer in a settlement: the unsold stock of its sellers
/// (above their own reserves), its stores and stalls, and open offers.
pub fn on_offer(world: &World, settlement: SimId, kind: &str) -> u32 {
    let people: u32 = super::demography::members(world, settlement)
        .into_iter()
        .filter(|(_, actor)| market::person_sells(world, *actor, kind))
        .map(|(_, actor)| market::available_to_sell(world, actor, kind))
        .fold(0, u32::saturating_add);
    let stores: u32 = super::civic::depots(world, settlement)
        .into_iter()
        .filter_map(|(_, entity)| world.get::<super::civic::Depot>(entity))
        .map(|depot| depot.stock(kind))
        .fold(0, u32::saturating_add);
    // Offers of played characters and visitors in reach of this settlement.
    let visitors: u32 = world
        .get_resource::<market::MarketState>()
        .map_or(0, |state| {
            state
                .listings
                .iter()
                .filter_map(|(id, offers)| {
                    let actor = world.resource::<SimIndex>().get(*id)?;
                    let visitor = world
                        .get::<Villager>(actor)
                        .is_some_and(|person| person.role == super::Role::Visitor);
                    (visitor && settlement_of(world, actor) == Some(settlement)).then(|| {
                        offers.get(kind).map_or(0, |offer| {
                            offer.quantity.min(econ::count(world, actor, kind))
                        })
                    })
                })
                .fold(0, u32::saturating_add)
        });
    people.saturating_add(stores).saturating_add(visitors)
}

/// Whether the makers of `kind` in a settlement should start no new batch:
/// what is on offer already exceeds the stock target.
pub fn over_target(world: &World, settlement: SimId, kind: &str) -> bool {
    let target = target(world, settlement, kind);
    target > 0 && on_offer(world, settlement, kind) > target
}

/// Units the whole island aims to hold of `kind`: the sum of the settlements'
/// targets. Stock up to this is working stock, not a glut, for the market.
/// Every traded good has one, a gathered good (logs, water) as much as a made
/// one: without it all unsold stock of a raw good read as a glut, so its quote
/// walked down to the lowest price while supply only matched demand.
pub fn island_target(world: &World, kind: &str) -> u32 {
    let raw_cover = tuning(world).is_some_and(|tuning| tuning.raw_stock_cover);
    let stocked = raw_cover
        || super::frontier::economy_spec::registry(world).is_some_and(|registry| {
            registry
                .spec
                .recipes
                .iter()
                .any(|recipe| recipe.outputs.iter().any(|output| output.good == kind))
                || registry
                    .spec
                    .crops
                    .iter()
                    .any(|crop| crop.harvest.good == kind)
        });
    if !stocked {
        return 0;
    }
    super::frontier::settlements::all(world)
        .into_iter()
        .map(|(settlement, _)| target(world, settlement, kind))
        .fold(0, u32::saturating_add)
}

/// The lowest whole price a resident or a store offers `kind` at (regional,
/// `reservation_prices`): a made good at its input cost at today's prices
/// plus `maker_margin_percent`, a raw good at `raw_floor_percent` of its base.
/// `None` when the rule is off or outside the regional profile.
pub fn reservation(world: &World, kind: &str) -> Option<u32> {
    let tuning = tuning(world)?;
    if !tuning.reservation_prices {
        return None;
    }
    let made = super::frontier::economy_spec::registry(world)?
        .spec
        .recipes
        .iter()
        .filter_map(|recipe| {
            let output = recipe.outputs.iter().find(|output| output.good == kind)?;
            let cost = recipe.inputs.iter().try_fold(0_u64, |total, input| {
                let price = market::reference_milli(world, &input.good)?;
                Some(total + u64::from(price) * u64::from(input.count))
            })?;
            Some(
                cost * u64::from(100 + tuning.maker_margin_percent)
                    / 100
                    / u64::from(output.count.max(1)),
            )
        })
        .min();
    let milli = made.unwrap_or_else(|| {
        u64::from(catalog::price_in(world, kind).unwrap_or(1))
            * u64::from(tuning.raw_floor_percent)
            * 10
    });
    Some((milli.div_ceil(1000) as u32).clamp(1, market::MAX_UNIT_PRICE))
}
