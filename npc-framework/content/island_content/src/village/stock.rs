//! Stock targets of the regional economy: how much of a good a settlement
//! uses in a day, how much it holds for sale, and whether its makers should
//! start another batch.
//!
//! - **Daily use.** Water and food from the residents' need decay (as
//!   [`super::view::daily_use`] reports it, each food's share following what
//!   was eaten); every other good from the recent trade volume: an integer
//!   moving sum of the units sold in the settlement over the last
//!   `trade_window_rounds` buckets of an eighth of a day each (3 days), kept
//!   in the hashed [`TradeVolume`]. The buckets close on the clock
//!   ([`tick`]), not with the market round, which is capped at 300 ticks.
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

/// Units sold per settlement and good, one entry per bucket (an eighth of a
/// day).
type Round = BTreeMap<SimId, BTreeMap<String, u32>>;

/// Recent trade volume by settlement, the moving sum behind the daily use of
/// goods that are not eaten or drunk.
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub struct TradeVolume {
    /// Units sold since the last bucket closed.
    pub pending: Round,
    /// The last buckets, oldest first; at most `trade_window_rounds`.
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

/// Food eaten per settlement and good, one entry per bucket, over the same
/// window as [`TradeVolume`]: the share of each food in a settlement's
/// daily use (`view::daily_use`) follows what its people actually ate, so a
/// food held in plenty does not grow its own target (closing fix).
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub struct FoodEaten {
    /// Units eaten since the last bucket closed.
    pub pending: Round,
    /// The last rounds, oldest first; at most `trade_window_rounds`.
    pub rounds: Vec<Round>,
}
impl SimHashResource for FoodEaten {
    const NAME: &'static str = "regional_food_eaten_v1";
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
        .register_sim_resource::<TradeVolume>()
        .init_resource::<FoodEaten>()
        .register_sim_resource::<FoodEaten>();
}

/// Books `units` of `kind` eaten by `actor` in its settlement.
pub(super) fn record_eaten(world: &mut World, actor: Entity, kind: &str, units: u32) {
    if !world.contains_resource::<FoodEaten>() {
        return;
    }
    let Some(settlement) = super::frontier::settlements::member(world, actor) else {
        return;
    };
    let mut eaten = world.resource_mut::<FoodEaten>();
    let entry = eaten
        .pending
        .entry(settlement)
        .or_default()
        .entry(kind.into())
        .or_default();
    *entry = entry.saturating_add(units);
}

/// Units of `kind` eaten in a settlement over the window (pending round
/// included).
pub fn eaten_recently(world: &World, settlement: SimId, kind: &str) -> u64 {
    world.get_resource::<FoodEaten>().map_or(0, |eaten| {
        eaten
            .rounds
            .iter()
            .chain(std::iter::once(&eaten.pending))
            .filter_map(|round| round.get(&settlement)?.get(kind))
            .map(|units| u64::from(*units))
            .sum()
    })
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

/// Ticks of one bucket of the trade and food windows: an eighth of a day,
/// whatever the market round is (S4 fix: the windows closed with the market
/// round, which is capped at 300 ticks, so at the shipped 24,000-tick day the
/// 24-bucket window covered 0.3 day instead of 3, and the sleep block left
/// most of it without a meal).
pub fn bucket_ticks(world: &World) -> u64 {
    (world.resource::<SimConfig>().day_length / BUCKETS_PER_DAY).max(1)
}

/// Buckets in a day.
pub const BUCKETS_PER_DAY: u64 = 8;

/// Closes a bucket of the trade and food windows every [`bucket_ticks`].
/// Called every tick in the island's Effects pass.
pub(super) fn tick(world: &mut World) {
    let now = world.resource::<SimClock>().tick;
    if now > 0 && now.is_multiple_of(bucket_ticks(world)) {
        close_round(world);
    }
}

/// Closes a bucket: the pending sales and meals join the window.
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
    if let Some(mut eaten) = world.get_resource_mut::<FoodEaten>() {
        let round = std::mem::take(&mut eaten.pending);
        eaten.rounds.push(round);
        let extra = eaten.rounds.len().saturating_sub(window);
        eaten.rounds.drain(..extra);
    }
}

/// Units of `kind` sold in a settlement per day, from the moving sum of the
/// closed buckets (eight a day).
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
    (sold * BUCKETS_PER_DAY).div_ceil(volume.rounds.len() as u64) as u32
}

/// Units of `kind` a settlement uses in a day: for a food, its share of the
/// residents' hunger by what they ate (`view::daily_use`); for water, the
/// thirst of its people or what was sold, the larger; the moving sum of trade
/// for everything else.
///
/// A food's use is what is eaten, not also what is sold (S4 fix): sales
/// swung from 0 to 143 bread a day between buckets, and a resident buying a
/// food to keep in the pack is no more use than the meal it is eaten at.
pub fn daily_use(world: &World, settlement: SimId, kind: &str) -> u32 {
    let need = || {
        super::view::daily_use(world, settlement)
            .into_iter()
            .find(|row| row.good == kind)
            .map_or(0, |row| row.per_day)
    };
    if catalog::food_restore_in(world, kind).is_some() {
        return need();
    }
    if kind == super::roles::roles(world).drink {
        return need().max(volume_per_day(world, settlement, kind));
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

/// A staple its makers keep a planned stock of (closing design): the foods,
/// gathered or made, and the coat with their inputs. Not hearth fuel, which
/// has its planned stock at the homes (`winter_fuel_reserve`, ordered by each
/// steward ahead of winter): a stall target for it is sized from every log
/// traded, planks and building included, and sent the woodcutters to hoard
/// hundreds of logs while a site's four logs waited. Not drink, which every
/// resident draws at a well when thirsty: a stall target planned 300 to 800
/// units of water in the first hours and tied the carriers up (closing
/// probes on `island_buildings`).
pub fn planned_staple(world: &World, kind: &str) -> bool {
    let roles = super::roles::roles(world);
    market::is_staple(world, kind) && kind != roles.drink && !roles.fuels.contains(&kind)
}

/// Units of `kind` a settlement aims to hold for sale; 0 when the rule is off.
pub fn target(world: &World, settlement: SimId, kind: &str) -> u32 {
    let days = tuning(world).map_or(0, |tuning| tuning.stock_target_days);
    if days == 0 {
        return 0;
    }
    // Made staples keep a planned stock of their own (closing design), more
    // for the winter: autumn and winter.
    let days = if planned_staple(world, kind) {
        tuning(world).map_or(days, |tuning| {
            let winter = matches!(
                super::calendar(world).1,
                super::Season::Autumn | super::Season::Winter
            );
            if winter {
                tuning.staple_winter_stock_days
            } else {
                tuning.staple_stock_days
            }
            .max(days)
        })
    } else {
        days
    };
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
        let days = topic.map_or(days, |topic| {
            oversee::scale(oversee::level(world, settlement, topic, None), days)
        });
        // Never more days of cover than the good keeps (S2.4).
        super::frontier::economy_spec::registry(world)
            .and_then(|registry| registry.spec.good(kind))
            .and_then(|good| good.shelf_life_days)
            .map_or(days, |life| days.min(life))
    };
    // A pen's product: while animals are unfed today, what they would yield
    // is wanted on top of what is on offer, so the keeper feeds the whole
    // herd whatever the stock (surplus goes to the stores and the boat).
    let unfed = super::livestock::unfed_yield(world, settlement, kind);
    let herd = if unfed > 0 {
        on_offer(world, settlement, kind).saturating_add(unfed)
    } else {
        0
    };
    daily_use(world, settlement, kind)
        .saturating_mul(days)
        .max(herd)
        .max(crop_target(world, settlement, kind))
        // A pen's product: one day of what its animals lay when all are
        // fed, so the keeper feeds the whole herd while that is not on offer.
        .max(super::livestock::daily_yield(world, settlement, kind))
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The shipped day: 24,000 ticks, eight buckets of 3,000.
    fn world() -> World {
        let mut world = World::new();
        world.insert_resource(SimConfig {
            day_length: 24_000,
            ..Default::default()
        });
        world.insert_resource(SimClock::default());
        world.init_resource::<TradeVolume>();
        world.init_resource::<FoodEaten>();
        world
    }

    /// Runs the clock to `until`, selling and eating in daylight only
    /// (the first 70% of the day, as the shipped schedule sleeps after it).
    fn run(world: &mut World, until: u64) {
        let settlement = SimId(1);
        while world.resource::<SimClock>().tick < until {
            let now = {
                let mut clock = world.resource_mut::<SimClock>();
                clock.tick += 1;
                clock.tick
            };
            if now % 24_000 < 16_800 && now.is_multiple_of(400) {
                let sell = |round: &mut Round| {
                    *round
                        .entry(settlement)
                        .or_default()
                        .entry("bread".into())
                        .or_default() += 1;
                };
                sell(&mut world.resource_mut::<TradeVolume>().pending);
                sell(&mut world.resource_mut::<FoodEaten>().pending);
            }
            tick(world);
        }
    }

    /// S4 fix: the windows closed with the 300-tick market round, so at the
    /// 24,000-tick day the 24-round window was 0.3 day; at dawn it held only
    /// the night and the daily use of bread fell to nothing. With buckets of
    /// an eighth of a day the window is three days, and the daily use read at
    /// dawn and at midday is the same.
    #[test]
    fn volume_per_day_and_meals_are_stable_between_dawn_and_midday() {
        let mut world = world();
        run(&mut world, 3 * 24_000);
        let dawn = volume_per_day(&world, SimId(1), "bread");
        let eaten_dawn = eaten_recently(&world, SimId(1), "bread");
        assert_eq!(world.resource::<TradeVolume>().rounds.len(), 24);
        run(&mut world, 3 * 24_000 + 12_000);
        let midday = volume_per_day(&world, SimId(1), "bread");
        let eaten_midday = eaten_recently(&world, SimId(1), "bread");
        // 42 sales a day (every 400 ticks over 16,800 ticks).
        assert_eq!(dawn, 42, "a day's sales read at dawn");
        assert!(midday.abs_diff(dawn) <= 1, "dawn {dawn}, midday {midday}");
        // The meals of three days (pending included) at both times.
        assert!(
            eaten_dawn.abs_diff(eaten_midday) <= 42 / 8 + 1,
            "dawn {eaten_dawn}, midday {eaten_midday}"
        );
    }
}
