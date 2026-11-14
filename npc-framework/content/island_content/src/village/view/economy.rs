//! Economy views for the interface (review fix pass, S5): price lines, the
//! flow of one good through a village, a resident's own reason on a need
//! alert, the village outlook and the season summary.
//!
//! Pure queries over `&World`: none writes anything, and calling them never
//! changes a state hash (`tests/island_economy_views.rs`).
use crate::village::frontier::{economy_spec, oversee, settlements};
use crate::village::{
    Villager, Wallet, catalog, civic, colony, demography, economy as econ, living, market,
    production, roles, stock,
};
use bevy_ecs::prelude::*;
use npc_framework::prelude::*;
use serde::Serialize;

/// Rounds between `price` and `day_ago`.
pub const DAY_AGO_ROUNDS: usize = 8;
/// A move smaller than this (permille) reads as steady.
pub const STEADY_PERMILLE: i32 = 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Trend {
    Rising,
    Falling,
    Steady,
}

/// The last market round of one good.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct MarketLine {
    pub tick: u64,
    /// Units offered for sale in the round.
    pub supply: u32,
    /// Units buyers wanted and could pay the price for.
    pub funded_demand: u32,
    /// Units that changed hands.
    pub sold: u32,
    /// `funded_demand - sold`, never below zero.
    pub unmet: u32,
    /// Units wanted only for less than the price.
    pub below_price: u32,
}

/// Price and recent market of one good.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PriceLine {
    pub good: String,
    pub display: String,
    /// Coins a unit now.
    pub price: u32,
    /// The price one round ago.
    pub previous: Option<u32>,
    /// The price [`DAY_AGO_ROUNDS`] rounds ago; `None` while history is shorter.
    pub day_ago: Option<u32>,
    /// `price` against `day_ago`, in permille (0 without `day_ago`).
    pub change_permille: i32,
    pub trend: Trend,
    /// The price stands at the catalog's ceiling (`tuning.price_ceiling_percent`).
    pub at_ceiling: bool,
    pub last_market: Option<MarketLine>,
    /// Up to 16 round prices, oldest first.
    pub history: Vec<u32>,
}

/// [`PriceLine`] of `good`; `None` for a good the market does not trade.
pub fn price_line(world: &World, good: &str) -> Option<PriceLine> {
    let state = world.get_resource::<market::MarketState>()?;
    let row = state.commodities.get(good)?;
    let price = state.price(good)?;
    let history: Vec<u32> = row.history.iter().map(|sample| sample.unit_price).collect();
    let back = |rounds: usize| {
        history
            .len()
            .checked_sub(rounds + 1)
            .and_then(|index| history.get(index))
            .copied()
    };
    let previous = back(1);
    let day_ago = back(DAY_AGO_ROUNDS);
    let change_permille = day_ago.filter(|old| *old > 0).map_or(0, |old| {
        ((i64::from(price) - i64::from(old)) * 1000 / i64::from(old))
            .clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
    });
    let trend = if change_permille > STEADY_PERMILLE {
        Trend::Rising
    } else if change_permille < -STEADY_PERMILLE {
        Trend::Falling
    } else {
        Trend::Steady
    };
    let registry = economy_spec::registry(world);
    let ceiling = registry.and_then(|registry| {
        let base = registry.good(good)?.base_price;
        Some((u64::from(base) * u64::from(registry.tuning().price_ceiling_percent) / 100).max(1))
    });
    let last_market = row.history.last().map(|sample| {
        let sold = u32::try_from(sample.traded_units).unwrap_or(u32::MAX);
        MarketLine {
            tick: sample.tick,
            supply: sample.supply,
            funded_demand: sample.funded_demand,
            sold,
            unmet: sample.funded_demand.saturating_sub(sold),
            below_price: sample.below_reference_bid_units,
        }
    });
    Some(PriceLine {
        good: good.to_owned(),
        display: catalog::display_in(world, good).unwrap_or(good).to_owned(),
        price,
        previous,
        day_ago,
        change_permille,
        trend,
        at_ceiling: ceiling.is_some_and(|ceiling| u64::from(price) >= ceiling),
        last_market,
        history,
    })
}

/// [`price_line`] of every traded good, in key order.
pub fn price_lines(world: &World) -> Vec<PriceLine> {
    let Some(state) = world.get_resource::<market::MarketState>() else {
        return Vec::new();
    };
    state
        .commodities
        .keys()
        .filter_map(|good| price_line(world, good))
        .collect()
}

/// What a good is wanted for.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum Purpose {
    Eat,
    Drink,
    Fuel,
    /// An input of a recipe the user makes.
    Input {
        recipe: String,
    },
    /// A material of an open building site.
    Build {
        site: SimId,
    },
    Tool,
    /// Stock the user keeps.
    Keep,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct UseLine {
    /// The resident; `None` for a building site.
    pub who: Option<SimId>,
    /// The resident's name or the site's label.
    pub label: String,
    pub purpose: Purpose,
    pub wanted: u32,
    pub has: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum WithoutCause {
    NoCoins,
    NoSeller,
    TooDear { quote: u32, limit: u32 },
    TooFar,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct WithoutLine {
    pub who: SimId,
    pub label: String,
    pub wanted: u32,
    pub cause: WithoutCause,
    pub sentence: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct HeldUpLine {
    /// The site's label.
    pub what: String,
    pub who: Option<SimId>,
    /// The site, when it is one.
    pub site: Option<SimId>,
    pub missing: u32,
    pub since_tick: Option<u64>,
}

/// One good in one village: who uses it, who goes without and why, and what
/// waits for it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct GoodFlow {
    pub good: String,
    pub used_by: Vec<UseLine>,
    pub going_without: Vec<WithoutLine>,
    pub held_up: Vec<HeldUpLine>,
}

fn purpose_of(world: &World, role: crate::village::Role, good: &str) -> Purpose {
    let roles = roles::roles(world);
    if catalog::food_restore_in(world, good).is_some() {
        return Purpose::Eat;
    }
    if good == roles.drink {
        return Purpose::Drink;
    }
    if good == roles.fuel {
        return Purpose::Fuel;
    }
    if catalog::role_tool_in(world, role) == Some(good)
        || catalog::has_tag(world, good, economy_spec::GoodTag::Tool)
    {
        return Purpose::Tool;
    }
    let registry = economy_spec::registry(world);
    for recipe in catalog::recipes_for(world, role) {
        let uses = registry
            .and_then(|registry| registry.spec.recipe(recipe))
            .is_some_and(|def| def.inputs.iter().any(|stack| stack.good == good));
        if uses {
            return Purpose::Input {
                recipe: registry
                    .and_then(|registry| registry.recipe_label(recipe))
                    .unwrap_or(recipe)
                    .to_owned(),
            };
        }
    }
    Purpose::Keep
}

/// Units of `good` for sale in `settlement`: residents' offers and store shelves.
fn on_offer(world: &World, settlement: SimId, good: &str) -> u32 {
    let mut units = 0_u32;
    for (_, actor) in demography::members(world, settlement) {
        units = units.saturating_add(market::available_to_sell(world, actor, good));
    }
    for (_, store) in civic::depots(world, settlement) {
        if let Some(shelf) = world.get::<civic::Depot>(store) {
            units = units.saturating_add(shelf.stock(good));
        }
    }
    units
}

/// [`GoodFlow`] of `good` in `settlement`.
pub fn good_flow(world: &World, settlement: SimId, good: &str) -> GoodFlow {
    let mut flow = GoodFlow {
        good: good.to_owned(),
        ..GoodFlow::default()
    };
    let plan = world.get_resource::<production::ProductionPlan>();
    let offered = on_offer(world, settlement, good);
    let quote = market::unit_price(world, good);
    for (id, actor) in demography::members(world, settlement) {
        if !crate::village::living(world, actor) {
            continue;
        }
        let Some(person) = world.get::<Villager>(actor) else {
            continue;
        };
        let wanted = plan
            .and_then(|plan| plan.wanted.get(&id))
            .and_then(|goods| goods.get(good))
            .copied()
            .unwrap_or(0);
        let funded = plan
            .and_then(|plan| plan.procurement.get(&id))
            .and_then(|goods| goods.get(good))
            .copied()
            .unwrap_or(0);
        let has = econ::count(world, actor, good);
        let wanted = wanted.max(funded);
        if wanted == 0 {
            continue;
        }
        flow.used_by.push(UseLine {
            who: Some(id),
            label: person.name.clone(),
            purpose: purpose_of(world, person.role, good),
            wanted,
            has,
        });
        if has >= wanted {
            continue;
        }
        let coins = world.get::<Wallet>(actor).map_or(0, |wallet| wallet.coins);
        let limit = plan
            .and_then(|plan| plan.bid_limits.get(&id))
            .and_then(|goods| goods.get(good))
            .copied();
        let display = catalog::display_in(world, good).unwrap_or(good);
        let (cause, sentence) = if coins == 0 {
            (
                WithoutCause::NoCoins,
                format!("{} has no coins for {display}.", person.name),
            )
        } else if offered == 0 {
            (
                WithoutCause::NoSeller,
                format!("Nobody here sells {display}."),
            )
        } else if let (Some(quote), Some(limit)) = (quote, limit)
            && limit < quote
        {
            (
                WithoutCause::TooDear { quote, limit },
                format!(
                    "{display} costs {quote} coins; {} can pay {limit}.",
                    person.name
                ),
            )
        } else if funded == 0 {
            (
                WithoutCause::NoCoins,
                format!(
                    "{} cannot afford {display} after its other needs.",
                    person.name
                ),
            )
        } else {
            continue;
        };
        flow.going_without.push(WithoutLine {
            who: id,
            label: person.name.clone(),
            wanted: wanted - has,
            cause,
            sentence,
        });
    }
    for (id, entity) in civic::open_sites(world, settlement) {
        let Some(site) = world.get::<civic::ConstructionSite>(entity) else {
            continue;
        };
        let needed = site.cost.get(good).copied().unwrap_or(0);
        if needed == 0 {
            continue;
        }
        let delivered = site.delivered.get(good).copied().unwrap_or(0);
        let incoming: u32 = site
            .incoming
            .values()
            .filter(|hold| hold.kind == good)
            .map(|hold| hold.count)
            .sum();
        let missing = needed.saturating_sub(delivered.saturating_add(incoming));
        flow.used_by.push(UseLine {
            who: None,
            label: site.label.clone(),
            purpose: Purpose::Build { site: id },
            wanted: needed,
            has: delivered,
        });
        if missing > 0 {
            flow.held_up.push(HeldUpLine {
                what: site.label.clone(),
                who: site.builder,
                site: Some(id),
                missing,
                since_tick: site.last_delivery_at.or(Some(site.placed_at)),
            });
        }
    }
    flow
}

/// Why a need is low, as the resident would say it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum NeedCause {
    HasItNotUsed,
    OnTheWay { target: SimId, good: String },
    NoCoins,
    NoSeller,
    SourceEmpty,
    Asleep,
    NoCoat,
    Outdoors,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct NeedAlert {
    pub need: String,
    pub value_milli: i32,
    pub cause: NeedCause,
    /// The resident's own reason in one sentence.
    pub sentence: String,
    /// When the need fell below half; `None` when the simulation does not
    /// keep it.
    pub since_tick: Option<u64>,
}

/// [`NeedAlert`] for `need` of `entity`; `None` while the need is above half.
/// `need` is `Hunger`, `Thirst`, `Energy` or `Warmth`.
pub fn need_alert(world: &World, entity: Entity, need: &str) -> Option<NeedAlert> {
    let value_milli = econ::need(world, entity, need);
    let sentence = super::need_reason(world, entity, need)?;
    let roles = roles::roles(world);
    let goods: Vec<&str> = match need {
        "Hunger" => catalog::trade_kinds(world)
            .iter()
            .copied()
            .filter(|kind| catalog::food_restore_in(world, kind).is_some())
            .collect(),
        "Thirst" => vec![roles.drink],
        _ => Vec::new(),
    };
    let carried = goods
        .iter()
        .any(|kind| econ::count(world, entity, kind) > 0);
    // On the way: the resident's current job is to buy or fetch one of them.
    let intent = world
        .get::<production::JobIntent>(entity)
        .filter(|intent| goods.iter().any(|kind| *kind == intent.good))
        .and_then(|intent| Some((intent.target?, intent.good.clone())));
    let cause = match need {
        "Hunger" | "Thirst" if carried => NeedCause::HasItNotUsed,
        "Hunger" | "Thirst" if intent.is_some() => {
            let (target, good) = intent.expect("checked");
            NeedCause::OnTheWay { target, good }
        }
        "Thirst" if sentence.contains("dry") => NeedCause::SourceEmpty,
        "Hunger" | "Thirst"
            if world
                .get::<Wallet>(entity)
                .is_some_and(|wallet| wallet.coins == 0) =>
        {
            NeedCause::NoCoins
        }
        "Hunger" | "Thirst" => NeedCause::NoSeller,
        "Energy" => NeedCause::Asleep,
        "Warmth" if !living::has_coat(world, entity) => NeedCause::NoCoat,
        _ => NeedCause::Outdoors,
    };
    let sentence = match &cause {
        NeedCause::OnTheWay { target, good } => {
            let seller = world
                .resource::<SimIndex>()
                .get(*target)
                .and_then(|seller| world.get::<Villager>(seller))
                .map_or_else(|| "a seller".to_owned(), |person| person.name.clone());
            format!(
                "On the way to {seller} for {}.",
                catalog::display_in(world, good).unwrap_or(good)
            )
        }
        _ => sentence,
    };
    Some(NeedAlert {
        need: need.to_owned(),
        value_milli,
        cause,
        sentence,
        since_tick: None,
    })
}

/// The coming shortage of a village.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Shortage {
    pub good: String,
    pub days_left: u32,
    pub cause_sentence: String,
}

/// A village at a glance.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Outlook {
    pub day: u64,
    pub season: crate::village::Season,
    pub population: u32,
    /// Days the food in packs and stores lasts at today's use; `None` while
    /// nothing is eaten.
    pub food_days: Option<u32>,
    pub water_days: Option<u32>,
    pub fuel_days: Option<u32>,
    /// Beds free (negative: residents without a bed).
    pub beds_free: i32,
    pub homeless: u32,
    /// Grown residents whose trade has no work they can do now.
    pub idle_hands: u32,
    /// Free places in any job of the village.
    pub open_places: u32,
    /// Days until the cold season; `Some(0)` while it lasts.
    pub cold_in_days: Option<u32>,
    pub cold_now: bool,
    /// Living residents without a coat worn or carried.
    pub without_coat: u32,
    pub next_shortage: Option<Shortage>,
}

/// Units of `good` in the packs of `settlement`'s residents and on its shelves.
fn held(world: &World, settlement: SimId, good: &str) -> u32 {
    let mut units = 0_u32;
    for (_, actor) in demography::members(world, settlement) {
        units = units.saturating_add(econ::count(world, actor, good));
    }
    for (_, store) in civic::depots(world, settlement) {
        if let Some(shelf) = world.get::<civic::Depot>(store) {
            units = units.saturating_add(shelf.stock(good));
        }
    }
    units
}

fn days_of(stock: u32, per_day: u32) -> Option<u32> {
    (per_day > 0).then(|| stock / per_day)
}

/// [`Outlook`] of `settlement`.
pub fn outlook(world: &World, settlement: SimId) -> Outlook {
    let (day, season, _) = crate::village::calendar(world);
    let people = demography::members(world, settlement);
    let living_people: Vec<(SimId, Entity)> = people
        .iter()
        .copied()
        .filter(|(_, actor)| crate::village::living(world, *actor))
        .collect();
    let roles = roles::roles(world);
    let goods: Vec<&'static str> = catalog::trade_kinds(world).to_vec();
    let foods: Vec<&'static str> = goods
        .iter()
        .copied()
        .filter(|kind| catalog::food_restore_in(world, kind).is_some())
        .collect();
    let (food_stock, food_use) = foods.iter().fold((0_u32, 0_u32), |(s, u), kind| {
        (
            s.saturating_add(held(world, settlement, kind)),
            u.saturating_add(stock::daily_use(world, settlement, kind)),
        )
    });
    let water_days = days_of(
        held(world, settlement, roles.drink),
        stock::daily_use(world, settlement, roles.drink),
    );
    let fuel_days = days_of(
        held(world, settlement, roles.fuel),
        stock::daily_use(world, settlement, roles.fuel),
    );
    // Beds of the village's homes.
    let mut beds = 0_i64;
    if let Some(mut query) = world.try_query::<(Entity, &colony::Household)>() {
        for (entity, home) in query.iter(world) {
            let member = settlements::member(world, entity).or_else(|| {
                world
                    .get::<civic::Building>(entity)
                    .map(|building| building.settlement)
            });
            if member == Some(settlement) {
                beds += i64::from(home.capacity);
            }
        }
    }
    let homeless = living_people
        .iter()
        .filter(|(id, _)| colony::home_of(world, *id).is_none())
        .count() as u32;
    let idle_hands = living_people
        .iter()
        .filter(|(_, actor)| {
            world.get::<Villager>(*actor).is_some()
                && demography::stage(world, *actor) != demography::LifeStage::Child
                && !crate::village::frontier::labor::work_feasible(world, *actor)
        })
        .count() as u32;
    let open_places = oversee::profession_table(world, settlement)
        .iter()
        .filter(|row| row.adjustable)
        .map(|row| row.free_seats)
        .sum();
    let per_season = crate::village::days_per_season(world).max(1);
    let cold_now = living::coat_season(world) || season == crate::village::Season::Winter;
    let cold_in_days = Some(if cold_now {
        0
    } else {
        // Seasons until winter, then the days left of this one.
        let index = match season {
            crate::village::Season::Spring => 0,
            crate::village::Season::Summer => 1,
            crate::village::Season::Autumn => 2,
            crate::village::Season::Winter => 3,
        };
        let left_here = per_season - day % per_season;
        (left_here + (2 - index.min(2)) * per_season) as u32
    });
    let without_coat = living_people
        .iter()
        .filter(|(_, actor)| {
            !living::has_coat(world, *actor) && econ::count(world, *actor, roles.garment) == 0
        })
        .count() as u32;
    // The used good with the fewest days left, below twice the stock target.
    let target_days = economy_spec::tuning(world).map_or(2, |tuning| tuning.stock_target_days);
    let mut next_shortage: Option<Shortage> = None;
    for good in &goods {
        let per_day = stock::daily_use(world, settlement, good);
        if per_day == 0 {
            continue;
        }
        let days_left = held(world, settlement, good) / per_day;
        if days_left >= target_days.saturating_mul(2) {
            continue;
        }
        if next_shortage
            .as_ref()
            .is_some_and(|best| best.days_left <= days_left)
        {
            continue;
        }
        let display = catalog::display_in(world, good).unwrap_or(good);
        let cause_sentence = if on_offer(world, settlement, good) == 0 {
            format!("Nobody here has {display} to sell.")
        } else {
            format!("{display} is used faster than it is made.")
        };
        next_shortage = Some(Shortage {
            good: (*good).to_owned(),
            days_left,
            cause_sentence,
        });
    }
    Outlook {
        day,
        season,
        population: living_people.len() as u32,
        food_days: days_of(food_stock, food_use),
        water_days,
        fuel_days,
        beds_free: (beds - living_people.len() as i64)
            .clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
        homeless,
        idle_hands,
        open_places,
        cold_in_days,
        cold_now,
        without_coat,
        next_shortage,
    }
}

/// One good over a season.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct GoodSeason {
    pub good: String,
    pub produced: u64,
    pub eaten_or_used: u64,
    pub spoiled: u64,
    pub imported: u64,
    pub exported: u64,
    pub price_start: Option<u32>,
    pub price_end: Option<u32>,
}

/// One season of the island.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct SeasonSummary {
    /// Seasons since the founding, from 0.
    pub season: u32,
    pub year: u32,
    pub name: String,
    pub population_start: u32,
    pub population_end: u32,
    pub births: u32,
    /// `(cause, residents)`, by cause.
    pub deaths: Vec<(String, u32)>,
    pub arrivals: u32,
    /// `(building, finished)`, by building.
    pub built: Vec<(String, u32)>,
    pub goods: Vec<GoodSeason>,
    pub tools_worn: u32,
    pub fund_start: u32,
    pub fund_end: u32,
    pub boat_visits: u32,
    /// The good that ran shortest, and its fewest days left.
    pub worst_shortage: Option<(String, u32)>,
}

/// The seasons the ledger holds, oldest first (at most eight).
pub fn season_summaries(world: &World) -> Vec<u32> {
    world
        .get_resource::<crate::village::ledger::SeasonLedger>()
        .map_or_else(Vec::new, |ledger| {
            ledger.seasons.iter().map(|tally| tally.season).collect()
        })
}

/// [`SeasonSummary`] of `season`, while the ledger holds it; the current
/// season reads its figures so far.
pub fn season_summary(world: &World, season: u32) -> Option<SeasonSummary> {
    let ledger = world.get_resource::<crate::village::ledger::SeasonLedger>()?;
    let tally = ledger.seasons.iter().find(|tally| tally.season == season)?;
    Some(crate::village::ledger::summary(world, tally))
}

/// One lot an owner keeps on a store or stall shelf.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ConsignedView {
    pub depot: SimId,
    pub good: String,
    pub units: u32,
    /// The market quote a unit sells at now.
    pub quote: Option<u32>,
}

/// What `owner` has laid on shelves, by depot then good (review fix S2.6).
pub fn consigned(world: &World, owner: SimId) -> Vec<ConsignedView> {
    let mut out = Vec::new();
    for (depot, entity) in civic::all_depots(world) {
        let Some(shelf) = world.get::<civic::Depot>(entity) else {
            continue;
        };
        for (good, owners) in &shelf.lots {
            if let Some(units) = owners.get(&owner).copied().filter(|units| *units > 0) {
                out.push(ConsignedView {
                    depot,
                    good: good.clone(),
                    units,
                    quote: market::unit_price(world, good),
                });
            }
        }
    }
    out
}

/// Whether free placement (Shape's editor mode) is on.
pub fn editor_mode(world: &World) -> bool {
    world
        .get_resource::<crate::village::frontier::shape::EditorMode>()
        .is_some_and(|mode| mode.on)
}
