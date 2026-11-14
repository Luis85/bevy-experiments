//! Read accessors for views and tools of the coastal world (`living_world`).
//!
//! Everything here is pure: nothing reserves, walks, trades or writes. Each
//! function answers with an empty or neutral value in a world without the
//! regional profile, so a view can call them unconditionally.
pub mod economy;
pub use economy::{
    GoodFlow, GoodSeason, HeldUpLine, MarketLine, NeedAlert, NeedCause, Outlook, PriceLine, Purpose,
    SeasonSummary, Shortage, Trend, UseLine, WithoutCause, WithoutLine, editor_mode, good_flow,
    ConsignedView, consigned, need_alert, outlook, price_line, price_lines, season_summaries,
    season_summary,
};
use super::frontier::life_spec::{Shock, Weather};
use super::resident::{BlockKind, ResidentTraits};
use super::{Season, demography, fields, merchant, resident, weather};
use bevy_ecs::prelude::*;
use npc_framework::prelude::*;
use serde::Serialize;

pub use super::demography::{Census, Departed, HouseholdView, PersonView};
pub use super::fields::{FieldView, PlotView};
pub use super::indoors::WorkplaceView;
pub use super::indoors::{BedView, GuestBedView, HomeView, INN_BEDS, InnView, Repose};
pub use super::live::{
    ChestView, DaySummary, GatheringView, GoalView, GoalsView, RelationView, TierView,
    UpgradeSiteView,
};
pub use super::merchant::{BoatView, VisitTotals};
pub use super::resident::{ConsideredOption, DecisionView};

/// Short present-tense activity of a resident ("gathering", "asleep at
/// home"); `None` for an actor that runs no resident leaf.
pub fn activity_label(world: &World, entity: Entity) -> Option<&'static str> {
    resident::label(world, entity)
}

/// Where a resident would buy one unit of `kind` now: a store, a stall or a
/// seller who stands still or is within a few steps. `None` when nobody
/// offers it at a price the resident can pay.
pub fn supplier(world: &World, entity: Entity, kind: &str) -> Option<SimId> {
    super::ai::market_supplier(world, entity, kind)
        .and_then(|seller| world.get::<SimId>(seller).copied())
}

/// An inn as a place: entrance, doorway, body, facing, guest beds and guests.
/// `None` when `building` is not an inn.
pub fn inn(world: &World, building: SimId) -> Option<InnView> {
    super::indoors::inn(world, building)
}

/// Every inn, ascending by identity.
pub fn inns(world: &World) -> Vec<InnView> {
    super::indoors::inns(world)
}

/// Every wild animal, ascending by identity: kind, position, target, state
/// (`grazing`, `walking`, `fleeing`, `dead`) and speed (living world).
pub fn wildlife(world: &World) -> Vec<super::wildlife::WildlifeView> {
    super::wildlife::views(world)
}

/// Everybody who has died, in order of death: name, profession, age, cause,
/// day, household and settlement. A resident's body is removed a day after
/// death; from then on `laid_to_rest` is true and the identity is gone.
pub fn departed(world: &World) -> Vec<Departed> {
    super::demography::departed(world)
}

/// The last finished day of a played character: earned, spent, gathered,
/// made, received, met, the low point of each need, and whether it slept.
/// `None` before its first morning.
pub fn day_summary(world: &World, unit: SimId) -> Option<DaySummary> {
    super::live::day_summary(world, unit)
}

/// The same account of the day so far.
pub fn day_so_far(world: &World, unit: SimId) -> Option<DaySummary> {
    super::live::day_so_far(world, unit)
}

/// Tick a played character sleeps until; `None` when awake. A host may run
/// the simulation faster until then; the simulation itself skips nothing.
pub fn asleep_until(world: &World, unit: SimId) -> Option<u64> {
    super::live::asleep_until(world, unit)
}

/// The repeat order of a played character: `(resource, left, done)`.
pub fn repeat_order(world: &World, unit: SimId) -> Option<(SimId, u32, u32)> {
    super::live::repeat_order(world, unit)
}

/// The relationships of a resident or a played character, strongest first.
pub fn relationships(world: &World, of: SimId) -> Vec<RelationView> {
    super::live::relationships(world, of)
}

/// The goods a resident likes and dislikes as a gift: `(liked, disliked)`.
pub fn gift_tastes(world: &World, resident: SimId) -> Option<(String, String)> {
    super::live::tastes(world, resident)
}

/// The goal chain of a played character: done, current and next.
pub fn goals(world: &World, unit: SimId) -> GoalsView {
    super::live::goals(world, unit)
}

/// The homestead tiers of a played character, with the reason one cannot be
/// built yet.
pub fn homestead_tiers(world: &World, unit: SimId) -> Vec<TierView> {
    super::live::homestead_tiers(world, unit)
}

/// The homestead chest of a played character; `None` before a storage tier.
pub fn chest(world: &World, unit: SimId) -> Option<ChestView> {
    super::live::chest(world, unit)
}

/// The homestead upgrade under way of a played character: materials wanted,
/// delivered and ordered, escrow, work left, the hired builder.
pub fn upgrade_site(world: &World, unit: SimId) -> Option<UpgradeSiteView> {
    super::live::upgrade_site(world, unit)
}

/// The stage of work under way at the upgrade's door: `(ticks left, ticks
/// in the stage)`; `None` while nobody works there.
pub fn upgrade_stage(world: &World, unit: SimId) -> Option<(u64, u64)> {
    super::live::upgrade_stage(world, unit)
}

/// One workplace, founding or built: body, entrance, door and facing.
pub fn workplace(world: &World, building: SimId) -> Option<WorkplaceView> {
    super::indoors::workplace(world, building)
}

/// Every workplace, ascending by identity.
pub fn workplaces(world: &World) -> Vec<WorkplaceView> {
    super::indoors::workplaces(world)
}

/// The calendar of village gatherings, soonest first.
pub fn gatherings(world: &World) -> Vec<GatheringView> {
    super::live::gatherings(world)
}

/// Residents in mourning: `(mourner, for whom, until day)`.
pub fn mourning(world: &World) -> Vec<(SimId, String, u64)> {
    super::live::mourning(world)
}

/// The latest decision of a resident as data. An option that was passed over
/// carries `does_not_apply` instead of a formatted sentence.
pub fn latest_decision(world: &World, entity: Entity) -> Option<DecisionView> {
    resident::decision(world, entity)
}

/// One block of a resident's own day, after chronotype and diligence.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct ScheduleBlockView {
    pub kind: BlockKind,
    /// Permille of the day, 0 is dawn; `from > to` wraps past dawn.
    pub from_permille: i32,
    pub to_permille: i32,
    /// The same in ticks of the day.
    pub from_tick: u64,
    pub to_tick: u64,
    /// The resident is inside the block now.
    pub active: bool,
}

/// The effective schedule of a resident: the world's blocks shifted by the
/// resident's chronotype, with leisure delayed by diligence. A leisure block
/// the delay swallows is left out.
pub fn schedule_blocks(world: &World, entity: Entity) -> Vec<ScheduleBlockView> {
    let Some(spec) = resident::spec(world) else {
        return Vec::new();
    };
    let traits: ResidentTraits = resident::traits(world, entity);
    let schedule = &spec.schedule;
    let day_length = world.resource::<SimConfig>().day_length.max(1);
    let shift = traits.chronotype * schedule.chronotype_shift / 100;
    let ticks = |permille: i32| permille as u64 * day_length / 1000;
    schedule
        .blocks
        .iter()
        .filter_map(|block| {
            let mut from = block.from;
            if block.kind == BlockKind::Leisure {
                let length = if block.to > block.from {
                    block.to - block.from
                } else {
                    block.to + 1000 - block.from
                };
                let delay = (traits.diligence - 50) * schedule.diligence_shift / 50;
                if delay >= length {
                    return None;
                }
                from += delay;
            }
            let from = (from + shift).rem_euclid(1000);
            let to = (block.to + shift).rem_euclid(1000);
            Some(ScheduleBlockView {
                kind: block.kind,
                from_permille: from,
                to_permille: to,
                from_tick: ticks(from),
                to_tick: ticks(to),
                active: resident::in_block(world, entity, block.kind),
            })
        })
        .collect()
}

/// What the season and today's weather do.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SeasonEffects {
    /// Calendar day, first day is 1.
    pub day: u64,
    /// Day within the season, first day is 1.
    pub day_of_season: u64,
    pub days_per_season: u64,
    /// Year of four seasons, first year is 1.
    pub year: u64,
    pub season: Season,
    pub night: bool,
    pub weather: Weather,
    pub shock: Option<Shock>,
    /// Crop growth in percent of a clear day; 0 in winter and under snow.
    pub growth_percent: u32,
    /// Fields lie fallow.
    pub fallow: bool,
    /// It is cold in the open now: night, winter or snow.
    pub cold: bool,
    /// Warmth lost in the open, percent of an ordinary cold day.
    pub warmth_loss_percent: u32,
    /// Hearth logs a heated home burns per day, and the ticks one log lasts.
    pub hearth_logs_per_day: u32,
    pub hearth_log_ticks: u64,
    /// Outdoor work stops and the boat does not land.
    pub outdoor_work_halted: bool,
}

/// Season effects: growth, cold and hearth logs per day.
pub fn season_effects(world: &World) -> SeasonEffects {
    let (day, season, night) = super::calendar(world);
    let season_days = super::days_per_season(world);
    let day_length = world.resource::<SimConfig>().day_length.max(1);
    let log_ticks = super::colony::heating_period_ticks(world).max(1);
    SeasonEffects {
        day: day + 1,
        day_of_season: day % season_days + 1,
        days_per_season: season_days,
        year: day / (season_days * 4) + 1,
        season,
        night,
        weather: weather::today(world),
        shock: weather::shock(world),
        growth_percent: weather::growth_percent(world),
        fallow: season == Season::Winter,
        cold: super::living::cold(world),
        warmth_loss_percent: weather::warmth_loss_percent(world),
        hearth_logs_per_day: day_length.div_ceil(log_ticks) as u32,
        hearth_log_ticks: log_ticks,
        outdoor_work_halted: weather::storm(world),
    }
}

/// One day of a forecast.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct ForecastDay {
    /// Calendar day, first day is 1.
    pub day: u64,
    pub season: Season,
    pub weather: Weather,
}

/// Weather now and the days ahead.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct WeatherView {
    pub today: ForecastDay,
    /// Tomorrow first; `economy.weather.forecast_days` long.
    pub forecast: Vec<ForecastDay>,
    pub shock: Option<Shock>,
    pub growth_percent: u32,
    pub warmth_loss_percent: u32,
}

/// Weather now and a short forecast. The forecast is exact: weather is a
/// function of the world seed and the day.
pub fn weather(world: &World) -> WeatherView {
    let (day, season, _) = super::calendar(world);
    let season_days = super::days_per_season(world);
    WeatherView {
        today: ForecastDay {
            day: day + 1,
            season,
            weather: weather::today(world),
        },
        forecast: weather::forecast(world)
            .into_iter()
            .map(|(day, weather)| ForecastDay {
                day: day + 1,
                season: super::season_of(day, season_days),
                weather,
            })
            .collect(),
        shock: weather::shock(world),
        growth_percent: weather::growth_percent(world),
        warmth_loss_percent: weather::warmth_loss_percent(world),
    }
}

/// Household and family of a resident: members, ages, partner, children.
pub fn household(world: &World, resident: SimId) -> Option<HouseholdView> {
    demography::household(world, resident)
}

/// Births, deaths by cause, arrivals and the present headcount.
pub fn census(world: &World) -> Census {
    demography::census(world)
}

/// Boat status and next visit.
pub fn boat(world: &World) -> BoatView {
    merchant::status(world)
}

/// Field plots with crop and growth stage.
pub fn fields(world: &World) -> Vec<FieldView> {
    fields::views(world)
}

/// The building a resident is inside (its home, or the inn for a guest in a
/// rented bed); `None` outdoors. A resident indoors stands on the entrance
/// point: hide it, or draw it inside.
pub fn inside(world: &World, resident: SimId) -> Option<SimId> {
    super::indoors::inside(world, resident)
}

/// Everybody inside a building, ascending by identity.
pub fn occupants(world: &World, building: SimId) -> Vec<SimId> {
    super::indoors::occupants(world, building)
}

/// A home as a place: entrance cell, body footprint, facing, beds with the
/// resident or child of each, and who is inside now.
pub fn home(world: &World, house: SimId) -> Option<HomeView> {
    super::indoors::home(world, house)
}

/// Every home, founding or built, ascending by identity.
pub fn homes(world: &World) -> Vec<HomeView> {
    super::indoors::homes(world)
}

/// The bed of a resident: `(home, bed index)`.
pub fn bed_of(world: &World, resident: SimId) -> Option<(SimId, u32)> {
    super::indoors::bed_of(world, resident)
}

/// Whether a resident is awake, resting awake or asleep.
pub fn repose(world: &World, entity: Entity) -> Repose {
    super::indoors::repose(world, entity)
}

/// One good's use in a settlement, estimated from the needs of its people.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct UseView {
    pub good: String,
    /// Units used in one day: what residents and children drink or eat.
    pub per_day: u32,
    /// Units in the packs of the settlement's residents and in its stores.
    pub stock: u32,
    /// Whole days the stock lasts; `None` when nothing is used.
    pub days_left: Option<u32>,
}

/// One day's use of water and of every food in a settlement. Food is shared
/// between the foods in proportion to what is in stock; with no food in
/// stock it is counted as bread. The estimate follows the decay of Thirst and
/// Hunger; production inputs and hearth fuel are not part of it.
pub fn daily_use(world: &World, settlement: SimId) -> Vec<UseView> {
    use npc_framework::needs::NeedRegistry;
    let members = super::demography::members(world, settlement);
    let children = world
        .get_resource::<super::demography::Demography>()
        .map_or(0, |state| {
            state
                .children
                .iter()
                .filter(|child| child.settlement == settlement)
                .count() as u64
        });
    let ration = super::frontier::economy_spec::registry(world).map_or(50, |registry| {
        u64::from(registry.spec.demography.child_ration_percent)
    });
    let day = world.resource::<SimConfig>().day_length;
    let registry = world.resource::<NeedRegistry>();
    let decay = |need: &str| {
        registry
            .id(need)
            .map_or(0, |id| registry.def(id).decay_per_tick.max(0) as u64)
    };
    let stock = |kind: &str| -> u32 {
        let packs: u32 = members
            .iter()
            .map(|(_, actor)| super::economy::count(world, *actor, kind))
            .sum();
        let stores: u32 = super::civic::depots(world, settlement)
            .into_iter()
            .filter_map(|(_, entity)| world.get::<super::civic::Depot>(entity))
            .map(|depot| depot.stock(kind))
            .sum();
        packs + stores
    };
    let people = members.len() as u64;
    let mut out = Vec::new();
    let mut push = |good: &str, per_day: u64| {
        let held = stock(good);
        out.push(UseView {
            good: good.to_owned(),
            per_day: per_day as u32,
            stock: held,
            days_left: held.checked_div(per_day as u32),
        });
    };
    push(
        super::roles::roles(world).drink,
        (people * decay("Thirst") * day).div_ceil(90_000),
    );
    let hunger = (people * 100 + children * ration) * decay("Hunger") * day / 100;
    let foods: Vec<(&'static str, u64, u64)> = super::catalog::trade_kinds(world)
        .iter()
        .filter_map(|kind| {
            let restore = super::catalog::food_restore_in(world, kind)?.max(1) as u64;
            Some((*kind, restore, u64::from(stock(kind))))
        })
        .collect();
    let held: u64 = foods.iter().map(|(_, _, units)| units).sum();
    for (kind, restore, units) in &foods {
        let share = (hunger * units).checked_div(held).unwrap_or(
            if *kind == super::roles::roles(world).baked {
                hunger
            } else {
                0
            },
        );
        push(kind, share.div_ceil(*restore));
    }
    // Every other good: the units sold in the settlement per day, from the
    // moving sum of recent trade (regional; nothing before the first sale).
    for kind in super::catalog::trade_kinds(world) {
        if *kind == super::roles::roles(world).drink
            || foods.iter().any(|(food, _, _)| food == kind)
        {
            continue;
        }
        let per_day = super::stock::volume_per_day(world, settlement, kind);
        if per_day > 0 {
            push(kind, u64::from(per_day));
        }
    }
    out
}

/// Somebody who makes a good.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ProducerView {
    pub resident: SimId,
    pub name: String,
    /// Lower-case profession.
    pub role: &'static str,
    pub settlement: Option<SimId>,
    pub workplace: SimId,
    /// Units the resident offers for sale now.
    pub offers: u32,
}

/// The living residents whose profession makes `kind`, ascending by identity.
pub fn producers(world: &World, kind: &str) -> Vec<ProducerView> {
    super::demography::residents(world)
        .into_iter()
        .filter_map(|(id, actor)| {
            let person = world.get::<super::Villager>(actor)?;
            super::catalog::outputs_for(world, person.role)
                .contains(&kind)
                .then(|| ProducerView {
                    resident: id,
                    name: person.name.clone(),
                    role: super::frontier::economy_spec::trade_label(world, person.role),
                    settlement: super::frontier::settlements::member(world, actor),
                    workplace: person.workplace,
                    offers: super::market::available_to_sell(world, actor, kind),
                })
        })
        .collect()
}

/// One building of the catalogue and whether the village can pay for it now.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CatalogueView {
    pub building: String,
    pub display: String,
    /// Materials at today's prices.
    pub estimated_cost: u32,
    pub available: bool,
    /// One sentence: why it can or cannot be built now (the first of
    /// `reasons` that stops it, else the money).
    pub reason: String,
    /// Every sentence that applies, one each: the money, materials nobody
    /// has for sale, the open-site allowance, a homestead tier it belongs
    /// to, and whether any settlement has ground it fits on.
    pub reasons: Vec<String>,
}

/// The building catalogue with a reason for each entry: the fund and the
/// building levy can pay for it, or how many coins are missing; materials
/// short on the island; the open-site allowance; the homestead tier that
/// grows the cottage into it; and placement. Placement searches the ground
/// of every settlement, so call this when the build menu opens, not every
/// frame.
pub fn catalogue(world: &World) -> Vec<CatalogueView> {
    let budget = super::civic::budget(world);
    let levy = super::civic::levy_capacity(world);
    let settlements = super::frontier::settlements::all(world);
    let full = !settlements.is_empty()
        && settlements.iter().all(|(id, _)| {
            super::civic::open_sites(world, *id).len() >= super::civic::MAX_OPEN_SITES
        });
    let radius = super::frontier::economy_spec::tuning(world)
        .map_or(36, |tuning| tuning.planner_search_radius_cells);
    let tier = |building: &str| {
        super::frontier::economy_spec::registry(world)?
            .spec
            .live
            .homestead
            .iter()
            .find(|tier| tier.building == building)
            .cloned()
    };
    super::civic::catalog(world)
        .iter()
        .map(|def| {
            let mut reasons = Vec::new();
            let mut blocked = None;
            for stack in &def.cost {
                let offered: u32 = settlements
                    .iter()
                    .map(|(id, _)| super::stock::on_offer(world, *id, &stack.good))
                    .fold(0, u32::saturating_add);
                if offered < stack.count {
                    reasons.push(format!(
                        "Needs {} {} and only {offered} are for sale on the island now; haulers wait \
                         until somebody makes or gathers more.",
                        stack.count, stack.good
                    ));
                }
            }
            if full {
                let sentence = format!(
                    "Every settlement already has {} open building sites; finish or cancel one first.",
                    super::civic::MAX_OPEN_SITES
                );
                blocked.get_or_insert(sentence.clone());
                reasons.push(sentence);
            }
            if let Some(tier) = tier(&def.id) {
                reasons.push(format!(
                    "A played character's cottage grows into this home with the homestead tier \"{}\"{}.",
                    tier.name,
                    if tier.after.is_empty() {
                        String::new()
                    } else {
                        format!(", after the tier \"{}\"", tier.after)
                    }
                ));
            }
            if !settlements.is_empty()
                && settlements.iter().all(|(id, _)| {
                    super::civic::find_site(world, def, *id, radius).is_none()
                })
            {
                let sentence = format!(
                    "No settlement has free, level ground for it within {radius} m of its centre."
                );
                blocked.get_or_insert(sentence.clone());
                reasons.push(sentence);
            }
            let estimated_cost = def
                .cost
                .iter()
                .map(|stack| {
                    stack
                        .count
                        .saturating_mul(super::market::unit_price(world, &stack.good).unwrap_or(0))
                })
                .fold(0, u32::saturating_add);
            let untraded = def
                .cost
                .iter()
                .find(|stack| super::market::unit_price(world, &stack.good).is_none());
            let available = untraded.is_none()
                && estimated_cost <= budget.saturating_add(levy)
                && blocked.is_none();
            let money = match untraded {
                Some(stack) => format!("Needs {}, which nobody trades here.", stack.good),
                None if estimated_cost <= budget => format!(
                    "The civic fund can pay: about {estimated_cost} of the {budget} coins it may spend."
                ),
                None if estimated_cost <= budget.saturating_add(levy) => format!(
                    "About {estimated_cost} coins: the fund may spend {budget} and the building levy raises the rest."
                ),
                None => format!(
                    "About {estimated_cost} coins: the fund may spend {budget} and the building levy can raise only {levy} more."
                ),
            };
            let money_blocks = untraded.is_some() || estimated_cost > budget.saturating_add(levy);
            reasons.insert(0, money.clone());
            let reason = match blocked {
                Some(sentence) if !money_blocks => sentence,
                _ => money,
            };
            CatalogueView {
                building: def.id.clone(),
                display: def.display.clone(),
                estimated_cost,
                available,
                reason,
                reasons,
            }
        })
        .collect()
}

/// The age of a resident for display.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct AgeView {
    /// `child`, `adult` or `elder`.
    pub stage: &'static str,
    /// Age in seasons, the unit of the data, and in years of four seasons.
    pub seasons: u64,
    pub calendar_years: u64,
    /// The age to show: a year of four seasons is short, so the stages are
    /// laid over a human life. Coming of age is 18, an elder is 65 and the
    /// years in between are spread evenly over the seasons of adult life.
    pub years: u64,
    /// For the inspector, for example `Adult · 34`.
    pub label: String,
}

/// The age of a resident: its life stage and an age in years a reader takes
/// at face value. `None` for somebody without a recorded birth (a played
/// character, a visitor).
pub fn age(world: &World, resident: SimId) -> Option<AgeView> {
    let actor = world.resource::<SimIndex>().get(resident)?;
    let days = demography::age_days(world, actor)?;
    let spec = &super::frontier::economy_spec::registry(world)?
        .spec
        .demography;
    let seasons = days / super::days_per_season(world).max(1);
    let (adult, elder) = (
        u64::from(spec.adult_age_seasons).max(1),
        u64::from(spec.elder_age_seasons).max(u64::from(spec.adult_age_seasons) + 1),
    );
    let years = if seasons < adult {
        seasons * 18 / adult
    } else {
        18 + (seasons - adult) * 47 / (elder - adult)
    };
    let stage = match demography::stage(world, actor) {
        demography::LifeStage::Child => "child",
        demography::LifeStage::Adult => "adult",
        demography::LifeStage::Elder => "elder",
    };
    let mut label = stage.to_owned();
    if let Some(first) = label.get_mut(0..1) {
        first.make_ascii_uppercase();
    }
    Some(AgeView {
        stage,
        seasons,
        calendar_years: seasons / 4,
        years,
        label: format!("{label} · {years}"),
    })
}

/// Births, deaths and arrivals by calendar year, oldest first (the last
/// eight years in which something happened).
pub fn yearly_census(world: &World) -> Vec<demography::YearTally> {
    demography::yearly(world)
}

/// How far the current action of an actor has come: `(done, total)` in
/// ticks, for every timed activity of a played character: a stage of
/// homestead work, a gathering, a craft, a night's sleep, a rest on the
/// ground or a nap, and the homestead upgrade under way. For a resident's
/// gathering (whose start is not recorded) `total` is 0 and `done` is the
/// ticks still to go.
pub fn activity_progress(world: &World, entity: Entity) -> Option<(u64, u64)> {
    let now = world.resource::<SimClock>().tick;
    let id = world.get::<SimId>(entity).copied()?;
    if let Some(work) = world.get::<super::live::UpgradeWork>(entity) {
        let total = work.ends.saturating_sub(work.started).max(1);
        return Some((now.saturating_sub(work.started).min(total), total));
    }
    if let Some(gathering) = world.get::<super::Gathering>(entity) {
        return Some(
            super::live::timer_progress(world, id, "gather", gathering.ends)
                .unwrap_or((gathering.ends.saturating_sub(now), 0)),
        );
    }
    if let Some(craft) = world.get::<npc_framework::crafting::Crafting>(entity) {
        let total = craft.ends.saturating_sub(craft.started).max(1);
        return Some((now.saturating_sub(craft.started).min(total), total));
    }
    if let Some(state) = world.get::<super::indoors::Indoors>(entity)
        && state.asleep_until > now
    {
        let total = state.asleep_until.saturating_sub(state.since).max(1);
        return Some((now.saturating_sub(state.since).min(total), total));
    }
    // A rest anywhere else: on the ground, or a nap.
    if let Some(until) = world
        .get::<super::Villager>(entity)
        .map(|person| person.resting_until)
        .filter(|until| *until > now)
        && let Some(progress) = super::live::timer_progress(world, id, "rest", until)
    {
        return Some(progress);
    }
    let upgrade = world
        .get_resource::<super::live::LiveState>()
        .and_then(|state| state.upgrades.get(&id))?;
    let total = super::frontier::economy_spec::registry(world)?
        .spec
        .live
        .homestead
        .iter()
        .find(|tier| tier.key == upgrade.key)
        .map_or(0, |tier| u64::from(tier.work_ticks));
    Some((total.saturating_sub(upgrade.remaining_ticks), total))
}

/// The planner's line for a resident, raw (keys, search figures: for
/// tools) and readable (display names only: for players).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PlanLinesView {
    pub raw: String,
    pub readable: String,
}

/// [`PlanLinesView`] of `entity`.
pub fn plan_lines(world: &World, entity: Entity) -> PlanLinesView {
    PlanLinesView {
        raw: super::goap::explain(world, entity),
        readable: super::goap::explain_readable(world, entity),
    }
}

/// [`activity_progress`] as a fraction in permille (0..=1000); `None` when
/// nothing timed is under way or its length is unknown. A gathering without
/// a timer is measured against its resource's gathering time.
pub fn activity_progress_permille(world: &World, entity: Entity) -> Option<u32> {
    let (mut done, mut total) = activity_progress(world, entity)?;
    if total == 0
        && let Some(gathering) = world.get::<super::Gathering>(entity)
        && let Some(patch) = world
            .resource::<SimIndex>()
            .get(gathering.patch)
            .and_then(|patch| world.get::<super::ResourcePatch>(patch))
    {
        let now = world.resource::<SimClock>().tick;
        total = u64::from(patch.gather_ticks).max(1);
        done = total.saturating_sub(gathering.ends.saturating_sub(now));
    }
    (total > 0).then(|| (done.min(total) * 1000 / total) as u32)
}

/// Where a resident works, as a player reads it: "the Harbour tailor's shop
/// 1" for a building the village built, else "the Harbour carpenter's
/// workplace". `None` without a workplace.
pub fn workplace_name(world: &World, resident: SimId) -> Option<String> {
    let actor = world.resource::<SimIndex>().get(resident)?;
    let person = world.get::<super::Villager>(actor)?;
    let place = world.resource::<SimIndex>().get(person.workplace)?;
    if let Some(building) = world.get::<super::civic::Building>(place) {
        return Some(format!("the {}", building.label));
    }
    let settlement = super::frontier::settlements::member(world, place)
        .and_then(|id| super::frontier::settlements::definition(world, id))
        .map(|def| def.label.clone());
    let role = super::frontier::economy_spec::trade_label(world, person.role).replace('_', " ");
    Some(match settlement {
        Some(label) => format!("the {label} {role}'s workplace"),
        None => format!("the {role}'s workplace"),
    })
}

/// The pack of an actor: what it weighs and may weigh, in grams.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct PackView {
    pub weight_g: u32,
    pub max_weight_g: u32,
}

/// Weight and capacity of an actor's pack and equipment.
pub fn pack(world: &World, entity: Entity) -> Option<PackView> {
    use npc_framework::interaction::items;
    let inventory = world.get::<items::Inventory>(entity)?;
    let weight_g = items::carried_items(world, entity)
        .into_iter()
        .filter_map(|id| world.resource::<SimIndex>().get(id))
        .filter_map(|item| world.get::<npc_framework::interaction::Item>(item))
        .map(|item| {
            items::kind_def(world, &item.kind)
                .map_or(0, |def| def.weight_g.saturating_mul(item.stack))
        })
        .fold(0, u32::saturating_add);
    Some(PackView {
        weight_g,
        max_weight_g: inventory.max_weight_g,
    })
}

/// The tool in an actor's hand: `(good, uses left)` of the carried tool its
/// current activity uses (see [`held`]), else of its profession's tool.
/// `None` when it carries neither.
pub fn held_tool(world: &World, entity: Entity) -> Option<(String, u32)> {
    if let Some(view) = held(world, entity).filter(|view| view.carried) {
        return Some((view.tool, view.uses_left));
    }
    let role = world.get::<super::Villager>(entity)?.role;
    let kind = super::catalog::role_tool_in(world, role)?;
    let tool = super::living::usable_tool(world, entity, kind)?;
    Some((kind.to_owned(), uses_left(world, tool)))
}

fn uses_left(world: &World, tool: Entity) -> u32 {
    world
        .get::<npc_framework::crafting::Durability>(tool)
        .map_or(0, |durability| durability.current)
}

/// The tool an actor's current activity uses, for drawing it in the hand.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct HeldView {
    /// The tool good, for example `axe`.
    pub tool: String,
    /// Uses left of the carried one; 0 when none is carried.
    pub uses_left: u32,
    /// Whether a usable one is carried: without it the work goes slower,
    /// bare-handed.
    pub carried: bool,
    /// `gather`, `craft`, `hunt` or `build`.
    pub activity: &'static str,
    /// A played character's homestead tool tier (for example
    /// `better_tools`) once built: its tools are the better ones.
    pub tier: Option<String>,
}

/// The tool implied by an actor's current activity: the gathering tool of
/// the patch's good, the tool of the recipe being made, the hunter's spear,
/// the carpenter's tool while building. `None` when the activity uses none.
pub fn held(world: &World, entity: Entity) -> Option<HeldView> {
    let tier = super::live::tool_tier(world, entity);
    let carried = |kind: &str, activity: &'static str| {
        let tool = super::living::usable_tool(world, entity, kind);
        HeldView {
            tool: kind.to_owned(),
            uses_left: tool.map_or(0, |tool| uses_left(world, tool)),
            carried: tool.is_some(),
            activity,
            tier: tier.clone(),
        }
    };
    if let Some(gathering) = world.get::<super::Gathering>(entity) {
        let patch = world.resource::<SimIndex>().get(gathering.patch)?;
        let kind = world.get::<super::ResourcePatch>(patch)?.kind.clone();
        let tool = super::catalog::gather_tool_in(world, &kind)
            .or_else(|| (kind == super::catalog::FISH).then_some(super::catalog::FISHING_ROD))?;
        return Some(carried(tool, "gather"));
    }
    if let Some(craft) = world.get::<npc_framework::crafting::Crafting>(entity) {
        let tool = craft
            .tools
            .iter()
            .filter_map(|id| world.resource::<SimIndex>().get(*id))
            .find_map(|item| world.get::<npc_framework::interaction::Item>(item))
            .map(|item| item.kind.clone())
            .or_else(|| {
                world
                    .resource::<npc_framework::crafting::RecipeRegistry>()
                    .recipes
                    .get(&craft.recipe)?
                    .tools
                    .first()
                    .cloned()
            })?;
        return Some(carried(&tool, "craft"));
    }
    if world.get::<super::hunting::Hunting>(entity).is_some() {
        let tool = super::catalog::role_tool_in(world, super::roles::roles(world).hunter)
            .unwrap_or(super::roles::roles(world).hunt_tool);
        return Some(carried(tool, "hunt"));
    }
    if world.get::<super::live::UpgradeWork>(entity).is_some()
        || world.get::<super::civic::SiteWork>(entity).is_some()
        || world
            .get::<super::homestead::CottageBuild>(entity)
            .is_some()
    {
        let tool = super::catalog::role_tool_in(world, super::roles::roles(world).builder)?;
        return Some(carried(tool, "build"));
    }
    None
}

/// Why a need of an actor is low, in one sentence from what the simulation
/// knows now; `None` while the need is above half. `need` is `Hunger`,
/// `Thirst`, `Energy` or `Warmth`.
pub fn need_reason(world: &World, entity: Entity, need: &str) -> Option<String> {
    if super::economy::need(world, entity, need) >= 50_000 {
        return None;
    }
    let coins = world
        .get::<super::Wallet>(entity)
        .map_or(0, |wallet| wallet.coins);
    let carried = |kinds: &[&str]| {
        kinds
            .iter()
            .any(|kind| super::economy::count(world, entity, kind) > 0)
    };
    let offered = |kinds: &[&str]| {
        kinds
            .iter()
            .any(|kind| super::ai::market_supplier(world, entity, kind).is_some())
    };
    let foods: Vec<&str> = super::catalog::trade_kinds(world)
        .iter()
        .copied()
        .filter(|kind| super::catalog::food_restore_in(world, kind).is_some())
        .collect();
    Some(match need {
        "Hunger" if carried(&foods) => "Food is in the pack; it has not eaten yet.".into(),
        "Hunger" if offered(&foods) => "No food in the pack; somebody nearby sells some.".into(),
        "Hunger" if coins == 0 => "No food in the pack and no coins to buy any.".into(),
        "Hunger" => "No food in the pack, and nobody offers any it can pay for.".into(),
        "Thirst" if carried(&[super::roles::roles(world).drink]) => {
            "Water is in the pack; it has not drunk yet.".into()
        }
        "Thirst" if offered(&[super::roles::roles(world).drink]) => {
            "No water in the pack; somebody nearby sells some.".into()
        }
        "Thirst" => {
            let dry = world
                .try_query::<&super::ResourcePatch>()
                .is_some_and(|mut query| {
                    query
                        .iter(world)
                        .filter(|patch| patch.kind == super::roles::roles(world).drink)
                        .all(|patch| patch.remaining == 0)
                });
            if dry {
                "No water in the pack and the wells are dry.".into()
            } else {
                "No water in the pack, and nobody offers any it can pay for.".into()
            }
        }
        "Energy" if super::indoors::asleep(world, entity) => {
            "Asleep; the night restores it.".into()
        }
        "Energy" => "Tired: a night in a bed restores it; a rest at home helps a little.".into(),
        "Warmth" if !super::living::has_coat(world, entity) => {
            "No warm coat, and it is cold outside.".into()
        }
        "Warmth" => "Out in the cold; a heated home restores it.".into(),
        _ => return None,
    })
}
