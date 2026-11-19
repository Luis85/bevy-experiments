//! Ages, couples, children, old age, inheritance and arrivals (regional
//! profile only).
//!
//! Every resident has a [`Life`]: a birthday, a seeded day on which old age
//! takes them, a partner and parents. Children are records of their household
//! ([`Child`]) until they come of age: they occupy a bed, eat from the packs
//! of the household's adults and do not work. On coming of age a child
//! becomes a resident with a trade and one aptitude of a parent. Ages are
//! counted in seasons, so `tuning.days_per_season` scales every life.
//!
//! Nothing here mints a coin: an estate goes to the household or the civic
//! fund, a coming-of-age gift comes from the parents and an arrival grant
//! from the fund.
use super::colony::{ColonyAct, Household, SocialLife};
use super::frontier::labor::{self, ResidentProfile, Settler};
use super::frontier::life_spec::DemographySpec;
use super::frontier::{SettlementMember, economy_spec, settlements};
use super::{Role, VillageState, Villager, Wallet, catalog, economy as econ, living};
use bevy_app::App;
use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use npc_framework::interaction::Item;
use npc_framework::kernel::hash::SimHashResource;
use npc_framework::kernel::rng::sim_rand;
use npc_framework::needs::NeedRegistry;
use npc_framework::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

/// `sim_rand` purposes of this module.
const AGE_PURPOSE: u32 = 0x4445_4d01;
const LIFESPAN_PURPOSE: u32 = 0x4445_4d02;
const BIRTH_PURPOSE: u32 = 0x4445_4d03;
const NAME_PURPOSE: u32 = 0x4445_4d04;
const APTITUDE_PURPOSE: u32 = 0x4445_4d05;
const DAY_OFFSET_PURPOSE: u32 = 0x4445_4d06;
/// Children are fed in rounds of this many ticks.
const FEED_ROUND_TICKS: u64 = 600;
/// A child eats when its fill drops below this.
const CHILD_MEAL_BELOW: i32 = 60_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Reflect)]
#[serde(rename_all = "snake_case")]
pub enum LifeStage {
    Child,
    Adult,
    Elder,
}
impl LifeStage {
    pub fn name(self) -> &'static str {
        match self {
            Self::Child => "child",
            Self::Adult => "adult",
            Self::Elder => "elder",
        }
    }
}

/// Age and family of one resident.
#[derive(Component, Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
#[reflect(Component)]
pub struct Life {
    /// Calendar day of birth; negative for somebody born before the calendar.
    pub born_day: i64,
    /// Calendar day on which old age takes this resident.
    pub death_day: u64,
    pub stage: LifeStage,
    pub partner: Option<SimId>,
    /// Both parents of somebody born on the island; empty for founders.
    pub parents: Vec<SimId>,
    /// The neighbour this resident is courting, since which calendar day.
    pub courting: Option<SimId>,
    pub courting_since: u64,
    pub last_birth_day: Option<u64>,
    /// Empty while alive.
    pub cause_of_death: String,
}
impl SimHash for Life {
    const NAME: &'static str = "regional_life_v1";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u64(self.born_day as u64);
        h.u64(self.death_day);
        h.u8(self.stage as u8);
        h.opt(self.partner.as_ref(), |h, id| h.u32(id.0));
        h.u32(self.parents.len() as u32);
        for parent in &self.parents {
            h.u32(parent.0);
        }
        h.opt(self.courting.as_ref(), |h, id| h.u32(id.0));
        h.u64(self.courting_since);
        h.opt(self.last_birth_day.as_ref(), |h, day| h.u64(*day));
        h.str(&self.cause_of_death);
    }
}

/// A child of a household: a record, not an actor.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub struct Child {
    pub id: u32,
    pub name: String,
    pub born_day: u64,
    /// The household the child lives in.
    pub home: SimId,
    pub settlement: SimId,
    pub parents: Vec<SimId>,
    /// 0 to 100000: how well fed the child is.
    pub fed: i32,
    /// Ticks the child has gone without any food.
    pub unfed_ticks: u64,
    /// Of age, but no workplace of the settlement has a free seat yet.
    pub waiting_for_seat: bool,
}

/// Children and the village's vital statistics.
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub struct Demography {
    /// Ascending by `id`.
    pub children: Vec<Child>,
    pub next_child: u32,
    /// The calendar day whose daily pass has run.
    pub last_day: Option<u64>,
    pub births: u32,
    pub came_of_age: u32,
    pub pairings: u32,
    pub arrivals: u32,
    /// Deaths of residents and children by cause.
    pub deaths: BTreeMap<String, u32>,
    /// Everybody who has died, in order of death: what is left of a resident
    /// once the body has been removed from the world.
    #[serde(default)]
    pub departed: Vec<Departed>,
    /// Births, deaths and arrivals by year, oldest first; the last eight.
    #[serde(default)]
    pub yearly: Vec<YearTally>,
}

/// What one year brought.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub struct YearTally {
    /// Year of the calendar, from 0.
    pub year: u64,
    pub births: u32,
    pub deaths: u32,
    pub arrivals: u32,
}

/// Books one event in the tally of the current year.
fn tally(world: &mut World, book: impl FnOnce(&mut YearTally)) {
    let year = super::calendar(world).0 / (season_days(world) * 4).max(1);
    let mut state = world.resource_mut::<Demography>();
    if state.yearly.last().is_none_or(|last| last.year != year) {
        state.yearly.push(YearTally {
            year,
            ..Default::default()
        });
        if state.yearly.len() > 8 {
            state.yearly.remove(0);
        }
    }
    book(state.yearly.last_mut().expect("this year"));
}

/// The tallies of the last years, oldest first; the last entry may be an
/// earlier year when nothing has happened in this one.
pub fn yearly(world: &World) -> Vec<YearTally> {
    world
        .get_resource::<Demography>()
        .map(|state| state.yearly.clone())
        .unwrap_or_default()
}

/// The record of somebody who died.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub struct Departed {
    /// The resident's identity while alive; `None` for a child.
    pub resident: Option<SimId>,
    pub name: String,
    /// Lower-case profession; empty for a child.
    pub role: String,
    pub age_seasons: u64,
    pub cause: String,
    /// Calendar day and tick of death.
    pub day: u64,
    pub tick: u64,
    pub household: Option<SimId>,
    pub settlement: Option<SimId>,
    /// True once the body is gone and the identity no longer resolves.
    pub laid_to_rest: bool,
}
impl SimHashResource for Demography {
    const NAME: &'static str = "regional_demography_v1";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u32(self.children.len() as u32);
        for child in &self.children {
            h.u32(child.id);
            h.str(&child.name);
            h.u64(child.born_day);
            h.u32(child.home.0);
            h.u32(child.settlement.0);
            h.u32(child.parents.len() as u32);
            for parent in &child.parents {
                h.u32(parent.0);
            }
            h.i32(child.fed);
            h.u64(child.unfed_ticks);
            h.u8(u8::from(child.waiting_for_seat));
        }
        h.u32(self.next_child);
        h.opt(self.last_day.as_ref(), |h, day| h.u64(*day));
        for value in [self.births, self.came_of_age, self.pairings, self.arrivals] {
            h.u32(value);
        }
        h.u32(self.deaths.len() as u32);
        for (cause, count) in &self.deaths {
            h.str(cause);
            h.u32(*count);
        }
        h.u32(self.departed.len() as u32);
        for gone in &self.departed {
            h.opt(gone.resident.as_ref(), |h, id| h.u32(id.0));
            h.str(&gone.name);
            h.str(&gone.role);
            h.u64(gone.age_seasons);
            h.str(&gone.cause);
            h.u64(gone.day);
            h.u64(gone.tick);
            h.opt(gone.household.as_ref(), |h, id| h.u32(id.0));
            h.opt(gone.settlement.as_ref(), |h, id| h.u32(id.0));
            h.u8(u8::from(gone.laid_to_rest));
        }
        h.u32(self.yearly.len() as u32);
        for year in &self.yearly {
            h.u64(year.year);
            h.u32(year.births);
            h.u32(year.deaths);
            h.u32(year.arrivals);
        }
    }
    fn snapshot_fns() -> Option<npc_framework::snapshot::ResourceSnapshotFns> {
        Some(npc_framework::snapshot::serde_resource_fns::<Self>())
    }
}

/// Regional profile only.
pub fn register(app: &mut App) {
    app.init_resource::<Demography>()
        .register_sim_resource::<Demography>()
        .register_sim_component::<Life>();
}

fn spec(world: &World) -> Option<&DemographySpec> {
    economy_spec::registry(world)
        .map(|registry| &registry.spec.demography)
        .filter(|demography| demography.enabled)
}

/// Living residents of a settlement, ascending by identity.
pub fn members(world: &World, settlement: SimId) -> Vec<(SimId, Entity)> {
    let Some(mut query) =
        world.try_query::<(Entity, &SimId, &SettlementMember, &ResidentProfile)>()
    else {
        return Vec::new();
    };
    let mut out: Vec<(SimId, Entity)> = query
        .iter(world)
        .filter(|(entity, _, member, _)| member.settlement == settlement && living(world, *entity))
        .map(|(entity, id, _, _)| (*id, entity))
        .collect();
    out.sort_unstable_by_key(|(id, _)| *id);
    out
}

/// Every living resident, ascending by identity.
pub fn residents(world: &World) -> Vec<(SimId, Entity)> {
    let Some(mut query) = world.try_query::<(Entity, &SimId, &ResidentProfile)>() else {
        return Vec::new();
    };
    let mut out: Vec<(SimId, Entity)> = query
        .iter(world)
        .filter(|(entity, _, _)| living(world, *entity))
        .map(|(entity, id, _)| (*id, entity))
        .collect();
    out.sort_unstable_by_key(|(id, _)| *id);
    out
}

/// Living residents plus children.
pub fn population(world: &World) -> u32 {
    residents(world).len() as u32
        + world
            .get_resource::<Demography>()
            .map_or(0, |state| state.children.len() as u32)
}

/// Households of a settlement, ascending by identity.
fn homes(world: &World, settlement: SimId) -> Vec<(SimId, Entity)> {
    let Some(mut query) = world.try_query::<(Entity, &SimId, &SettlementMember, &Household)>()
    else {
        return Vec::new();
    };
    let mut out: Vec<(SimId, Entity)> = query
        .iter(world)
        .filter(|(_, _, member, _)| member.settlement == settlement)
        .map(|(entity, id, _, _)| (*id, entity))
        .collect();
    out.sort_unstable_by_key(|(id, _)| *id);
    out
}

/// The household a resident sleeps in.
pub fn home_of(world: &World, resident: SimId) -> Option<SimId> {
    let mut query = world.try_query::<(&SimId, &Household)>()?;
    query
        .iter(world)
        .filter(|(_, home)| home.residents.contains(&resident))
        .map(|(id, _)| *id)
        .min()
}

fn living_adults(world: &World, home: &Household) -> Vec<SimId> {
    let index = world.resource::<SimIndex>();
    home.residents
        .iter()
        .copied()
        .filter(|id| index.get(*id).is_some_and(|entity| living(world, entity)))
        .collect()
}

/// Children living in a household.
pub fn children_of(world: &World, house: SimId) -> Vec<&Child> {
    world
        .get_resource::<Demography>()
        .into_iter()
        .flat_map(|state| state.children.iter())
        .filter(|child| child.home == house)
        .collect()
}

/// Beds in use in a household: its living residents and its children.
pub fn occupancy(world: &World, house: SimId) -> u32 {
    let adults = world
        .resource::<SimIndex>()
        .get(house)
        .and_then(|entity| world.get::<Household>(entity))
        .map_or(0, |home| living_adults(world, home).len() as u32);
    adults + children_of(world, house).len() as u32
}

/// Free beds of a household after its children are counted.
pub fn free_beds(world: &World, house: SimId) -> u32 {
    world
        .resource::<SimIndex>()
        .get(house)
        .and_then(|entity| world.get::<Household>(entity))
        .map_or(0, |home| {
            home.capacity.saturating_sub(occupancy(world, house))
        })
}

/// People of a settlement living in a home with too few beds, and the size
/// of the largest such family: what the civic planner must build for.
pub fn crowding(world: &World, settlement: SimId) -> (u32, u32) {
    let mut crowded = 0;
    let mut largest = 0;
    for (id, entity) in homes(world, settlement) {
        let capacity = world
            .get::<Household>(entity)
            .map_or(0, |home| home.capacity);
        let used = occupancy(world, id);
        if used > capacity {
            crowded += used - capacity;
            largest = largest.max(used);
        }
    }
    (crowded, largest)
}

/// Households of a settlement in a home with too few beds: waiting for a
/// home, and holding back their next child for want of a bed (`no_bed`).
pub fn households_waiting(world: &World, settlement: SimId) -> u32 {
    homes(world, settlement)
        .into_iter()
        .filter(|(id, entity)| {
            let capacity = world
                .get::<Household>(*entity)
                .map_or(0, |home| home.capacity);
            occupancy(world, *id) > capacity
        })
        .count() as u32
}

/// Sizes of the families of a settlement that live in a home with too few
/// beds, largest first.
pub fn crowded_families(world: &World, settlement: SimId) -> Vec<u32> {
    let mut sizes: Vec<u32> = homes(world, settlement)
        .into_iter()
        .filter_map(|(id, entity)| {
            let capacity = world.get::<Household>(entity)?.capacity;
            let used = occupancy(world, id);
            (used > capacity).then_some(used)
        })
        .collect();
    sizes.sort_unstable_by(|a, b| b.cmp(a));
    sizes
}

/// Children of a settlement who are of age and wait for a workplace seat.
pub fn waiting_youths(world: &World, settlement: SimId) -> u32 {
    world.get_resource::<Demography>().map_or(0, |state| {
        state
            .children
            .iter()
            .filter(|child| child.settlement == settlement && child.waiting_for_seat)
            .count() as u32
    })
}

fn season_days(world: &World) -> u64 {
    super::days_per_season(world)
}

/// Age in days of somebody born on calendar day `born_day`.
fn age_days_at(day: u64, born_day: i64) -> u64 {
    (day as i64 - born_day).max(0) as u64
}

/// Age of a resident in days; `None` without a [`Life`].
pub fn age_days(world: &World, actor: Entity) -> Option<u64> {
    let life = world.get::<Life>(actor)?;
    Some(age_days_at(super::calendar(world).0, life.born_day))
}

/// Life stage of a resident; adult without a [`Life`].
pub fn stage(world: &World, actor: Entity) -> LifeStage {
    world
        .get::<Life>(actor)
        .map_or(LifeStage::Adult, |life| life.stage)
}

/// Work duration of this resident in percent of an adult's: elders are slower.
pub fn work_percent(world: &World, actor: Entity) -> u32 {
    match (spec(world), world.get::<Life>(actor)) {
        (Some(spec), Some(life)) if life.stage == LifeStage::Elder => spec.elder_work_percent,
        _ => 100,
    }
}

/// Gives a resident an age: `age_seasons` when known, else a seeded adult age
/// up to `max_age_seasons`. Called for founders, settlers and grown children.
fn enroll(
    world: &mut World,
    actor: Entity,
    id: SimId,
    born_day: Option<i64>,
    max_age_seasons: u32,
    parents: Vec<SimId>,
) {
    let Some(spec) = spec(world).cloned() else {
        return;
    };
    let seed = world.resource::<WorldSeed>().0;
    let days = season_days(world) as i64;
    let today = super::calendar(world).0 as i64;
    let born_day = born_day.unwrap_or_else(|| {
        let span = u64::from(max_age_seasons.saturating_sub(spec.adult_age_seasons)) + 1;
        let age =
            i64::from(spec.adult_age_seasons) + (sim_rand(seed, id, 0, AGE_PURPOSE) % span) as i64;
        today - age * days - (sim_rand(seed, id, 0, DAY_OFFSET_PURPOSE) % days as u64) as i64
    });
    let span = u64::from(spec.lifespan_max_seasons - spec.lifespan_min_seasons) + 1;
    let lifespan = i64::from(spec.lifespan_min_seasons)
        + (sim_rand(seed, id, 0, LIFESPAN_PURPOSE) % span) as i64;
    let death_day = (born_day
        + lifespan * days
        + (sim_rand(seed, id, 1, DAY_OFFSET_PURPOSE) % days as u64) as i64)
        .max(today + 1) as u64;
    let elder =
        age_days_at(today as u64, born_day) >= u64::from(spec.elder_age_seasons) * days as u64;
    world.entity_mut(actor).insert(Life {
        born_day,
        death_day,
        stage: if elder {
            LifeStage::Elder
        } else {
            LifeStage::Adult
        },
        partner: None,
        parents,
        courting: None,
        courting_since: 0,
        last_birth_day: None,
        cause_of_death: String::new(),
    });
    if elder {
        slow_down(world, actor, spec.elder_speed_percent);
    }
    // An elder's own day (review npc-ai#11).
    super::resident::refresh_schedule(world, actor);
}

fn slow_down(world: &mut World, actor: Entity, percent: u32) {
    if let Some(mut person) = world.get_mut::<Villager>(actor) {
        person.base_speed = (person.base_speed * percent as i32 / 100).max(1);
    }
}

/// Call once after the founders exist: every founder gets a seeded age.
pub fn spawn(world: &mut World) {
    let Some(spec) = spec(world).cloned() else {
        return;
    };
    for (id, actor) in residents(world) {
        if world.get::<Life>(actor).is_none() {
            enroll(
                world,
                actor,
                id,
                None,
                spec.founder_max_age_seasons,
                Vec::new(),
            );
        }
    }
}

/// Called when a resident is settled outside this module (tests, tools).
pub(super) fn enrolled(world: &mut World, actor: Entity, id: SimId) {
    if world.get::<Life>(actor).is_none()
        && let Some(spec) = spec(world).cloned()
    {
        // Settlers are in the younger half of adult life.
        let max =
            spec.adult_age_seasons + (spec.founder_max_age_seasons - spec.adult_age_seasons) / 2;
        enroll(world, actor, id, None, max, Vec::new());
    }
}

fn pick_name(world: &World, key: u64, spec: &DemographySpec) -> String {
    let seed = world.resource::<WorldSeed>().0;
    let pick = sim_rand(seed, SimId(0), key, NAME_PURPOSE) % spec.names.len() as u64;
    spec.names[pick as usize].clone()
}

fn food_kinds(world: &World) -> Vec<&'static str> {
    economy_spec::registry(world)
        .map(|registry| {
            registry
                .trade_kinds()
                .iter()
                .copied()
                .filter(|kind| catalog::food_restore_in(world, kind).is_some())
                .collect()
        })
        .unwrap_or_default()
}

fn carried_food(world: &World, actor: Entity) -> u32 {
    food_kinds(world)
        .into_iter()
        .map(|kind| econ::count(world, actor, kind))
        .sum()
}

/// The food a household can count on when it decides on a child (closing
/// fix): what every member of the home holds, plus its share of the food in
/// its settlement's stores (the stores' food over the settlement's homes).
/// Not one person's pack at that moment: since the daily sit-down meal a
/// resident eats what it carries and buys again, so packs are nearly empty
/// between meals even in a well-fed village.
pub fn household_food(world: &World, house: SimId, settlement: SimId) -> u32 {
    let kinds = food_kinds(world);
    let members = members(world, settlement);
    let held: u32 = members
        .iter()
        .filter(|(id, _)| home_of(world, *id) == Some(house))
        .map(|(_, actor)| carried_food(world, *actor))
        .fold(0, u32::saturating_add);
    let homes: BTreeSet<SimId> = members
        .iter()
        .filter_map(|(id, _)| home_of(world, *id))
        .collect();
    let stored: u32 = super::civic::depots(world, settlement)
        .into_iter()
        .filter_map(|(_, entity)| world.get::<super::civic::Depot>(entity))
        .map(|depot| {
            kinds
                .iter()
                .map(|kind| depot.stock(kind))
                .fold(0, u32::saturating_add)
        })
        .fold(0, u32::saturating_add);
    held.saturating_add(stored / (homes.len() as u32).max(1))
}

/// Hunger the children of this resident's household add to the food this
/// resident plans to hold, in the milli-units of the Hunger need. The
/// household's adults share it equally.
pub fn dependants_hunger(world: &World, actor: Entity, horizon_ticks: u64) -> u64 {
    let Some(spec) = spec(world) else {
        return 0;
    };
    let Some(state) = world.get_resource::<Demography>() else {
        return 0;
    };
    if state.children.is_empty() {
        return 0;
    }
    let Some(id) = world.get::<SimId>(actor).copied() else {
        return 0;
    };
    let Some(house) = home_of(world, id) else {
        return 0;
    };
    let adults = world
        .resource::<SimIndex>()
        .get(house)
        .and_then(|entity| world.get::<Household>(entity))
        .map_or(1, |home| living_adults(world, home).len().max(1) as u64);
    let decay = child_decay(world, spec);
    let need: u64 = state
        .children
        .iter()
        .filter(|child| child.home == house)
        .map(|child| (100_000 - child.fed).max(0) as u64 + decay * horizon_ticks)
        .sum();
    need.div_ceil(adults)
}

/// Fill a child loses per tick.
fn child_decay(world: &World, spec: &DemographySpec) -> u64 {
    let registry = world.resource::<NeedRegistry>();
    let adult = registry
        .id("Hunger")
        .map_or(0, |id| registry.def(id).decay_per_tick.max(0) as u64);
    adult * u64::from(spec.child_ration_percent) / 100
}

fn bump(deaths: &mut BTreeMap<String, u32>, cause: &str) {
    *deaths.entry(cause.into()).or_default() += 1;
}

/// Children eat from the packs of their household's adults.
fn feed_children(world: &mut World, spec: &DemographySpec) {
    if world.resource::<Demography>().children.is_empty() {
        return;
    }
    let decay = (child_decay(world, spec) * FEED_ROUND_TICKS) as i32;
    let starve_ticks =
        u64::from(spec.child_starve_days) * world.resource::<SimConfig>().day_length.max(1);
    let foods = food_kinds(world);
    let children = world.resource::<Demography>().children.clone();
    let mut died = Vec::new();
    for mut child in children {
        child.fed = (child.fed - decay).max(0);
        if child.fed < CHILD_MEAL_BELOW {
            let adults = world
                .resource::<SimIndex>()
                .get(child.home)
                .and_then(|entity| world.get::<Household>(entity))
                .map(|home| living_adults(world, home))
                .unwrap_or_default();
            'meal: for adult in adults {
                let Some(actor) = world.resource::<SimIndex>().get(adult) else {
                    continue;
                };
                for kind in &foods {
                    if econ::count(world, actor, kind) > 0
                        && econ::consume(world, actor, kind, 1).is_ok()
                    {
                        child.fed = (child.fed
                            + catalog::food_restore_in(world, kind).unwrap_or(0))
                        .min(100_000);
                        break 'meal;
                    }
                }
            }
        }
        if child.fed < CHILD_MEAL_BELOW / 2 {
            // Nothing in the house: the civic fund buys the child a meal from
            // a store of the settlement, while it has coins and a store has food.
            child.fed = (child.fed + civic_meal(world, &child, &foods)).min(100_000);
        }
        if child.fed == 0 {
            child.unfed_ticks += FEED_ROUND_TICKS;
        } else {
            child.unfed_ticks = 0;
        }
        if child.unfed_ticks >= starve_ticks {
            died.push(child.clone());
        }
        let mut state = world.resource_mut::<Demography>();
        if let Some(slot) = state.children.iter_mut().find(|slot| slot.id == child.id) {
            *slot = child;
        }
    }
    for child in died {
        {
            let (day, tick) = (super::calendar(world).0, world.resource::<SimClock>().tick);
            let age_seasons = day.saturating_sub(child.born_day) / season_days(world);
            let mut state = world.resource_mut::<Demography>();
            state.children.retain(|slot| slot.id != child.id);
            bump(&mut state.deaths, "hunger");
            state.departed.push(Departed {
                resident: None,
                name: child.name.clone(),
                role: String::new(),
                age_seasons,
                cause: "hunger".into(),
                day,
                tick,
                household: Some(child.home),
                settlement: Some(child.settlement),
                laid_to_rest: true,
            });
        }
        tally(world, |year| year.deaths += 1);
        record_effect(
            world,
            None,
            "VillageChildDeath",
            &json!({"child":child.id,"name":child.name,"cause":"hunger","home":child.home,
                "parents":child.parents}),
        );
        super::colony_demand_changed(world);
    }
}

/// One meal for a hungry child from a store of its settlement, paid by the
/// civic fund to the owners of the food. Returns the fill it gives.
fn civic_meal(world: &mut World, child: &Child, foods: &[&'static str]) -> i32 {
    let now = world.resource::<SimClock>().tick;
    for (store_id, store) in super::civic::depots(world, child.settlement) {
        for kind in foods {
            let Some(price) = super::market::unit_price(world, kind) else {
                continue;
            };
            if world.resource::<VillageState>().fund < price
                || world
                    .get::<super::civic::Depot>(store)
                    .is_none_or(|depot| depot.available(kind, None, now) == 0)
            {
                continue;
            }
            let lots = world
                .get_mut::<super::civic::Depot>(store)
                .expect("store")
                .take(kind, 1);
            let mut paid = 0;
            for (owner, units) in lots {
                let coins = price * units;
                let payee = (owner != super::civic::CIVIC_OWNER)
                    .then(|| world.resource::<SimIndex>().get(owner))
                    .flatten()
                    .filter(|entity| {
                        living(world, *entity)
                            && world
                                .get::<Wallet>(*entity)
                                .is_some_and(|wallet| wallet.coins.checked_add(coins).is_some())
                    });
                // Food of the settlement itself costs the fund nothing.
                if let Some(entity) = payee {
                    world.get_mut::<Wallet>(entity).expect("payee").coins += coins;
                    world.resource_mut::<VillageState>().fund -= coins;
                    paid += coins;
                } else if super::merchant::credit_consignment(world, owner, coins) {
                    world.resource_mut::<VillageState>().fund -= coins;
                    paid += coins;
                }
            }
            record_effect(
                world,
                None,
                "VillageChildFed",
                &json!({"child":child.id,"name":child.name,"kind":kind,"store":store_id,
                    "coins":paid,"home":child.home}),
            );
            return catalog::food_restore_in(world, kind).unwrap_or(0);
        }
    }
    0
}

/// Elders slow down and old age takes those whose day has come.
fn age_residents(world: &mut World, spec: &DemographySpec, day: u64) {
    let elder_days = u64::from(spec.elder_age_seasons) * season_days(world);
    for (id, actor) in residents(world) {
        let Some(life) = world.get::<Life>(actor).cloned() else {
            continue;
        };
        if day >= life.death_day {
            super::ai::kill(world, id, actor, "old age");
            continue;
        }
        if life.stage == LifeStage::Adult && age_days_at(day, life.born_day) >= elder_days {
            world.get_mut::<Life>(actor).expect("life").stage = LifeStage::Elder;
            super::resident::assign_tree(world, actor);
            slow_down(world, actor, spec.elder_speed_percent);
            let name = world
                .get::<Villager>(actor)
                .map_or_else(String::new, |person| person.name.clone());
            record_effect(
                world,
                Some(id),
                "VillageElder",
                &json!({"name":name,"age_seasons":age_days_at(day, life.born_day) / season_days(world),
                    "speed_percent":spec.elder_speed_percent,
                    "work_percent":spec.elder_work_percent}),
            );
        }
    }
}

fn kin(a: &Life, a_id: SimId, b: &Life, b_id: SimId) -> bool {
    a.parents.contains(&b_id)
        || b.parents.contains(&a_id)
        || a.parents.iter().any(|parent| b.parents.contains(parent))
}

fn affinity(world: &World, from: Entity, to: SimId) -> i32 {
    world
        .get::<SocialLife>(from)
        .and_then(|social| social.affinity.get(&to))
        .copied()
        .unwrap_or(0)
}

/// Housed adults whose mutual affinity has lasted become a couple.
fn court(world: &mut World, spec: &DemographySpec, day: u64) {
    for (settlement, _) in settlements::all(world) {
        let single: Vec<(SimId, Entity)> = members(world, settlement)
            .into_iter()
            .filter(|(id, actor)| {
                world
                    .get::<Life>(*actor)
                    .is_some_and(|life| life.partner.is_none() && life.stage != LifeStage::Child)
                    && home_of(world, *id).is_some()
            })
            .collect();
        // Every mutual match, strongest first; each resident courts one.
        let mut matches = Vec::new();
        for (index, (a_id, a)) in single.iter().enumerate() {
            for (b_id, b) in &single[index + 1..] {
                let (toward, back) = (affinity(world, *a, *b_id), affinity(world, *b, *a_id));
                let (Some(a_life), Some(b_life)) = (world.get::<Life>(*a), world.get::<Life>(*b))
                else {
                    continue;
                };
                if toward >= spec.pair_affinity
                    && back >= spec.pair_affinity
                    && !kin(a_life, *a_id, b_life, *b_id)
                {
                    matches.push((toward.min(back), *a_id, *a, *b_id, *b));
                }
            }
        }
        matches.sort_by(|x, y| y.0.cmp(&x.0).then(x.1.cmp(&y.1)).then(x.3.cmp(&y.3)));
        let mut taken = BTreeSet::new();
        for (_, a_id, a, b_id, b) in matches {
            if taken.contains(&a_id) || taken.contains(&b_id) {
                continue;
            }
            taken.insert(a_id);
            taken.insert(b_id);
            let since = |world: &World, actor: Entity, other: SimId| {
                world
                    .get::<Life>(actor)
                    .filter(|life| life.courting == Some(other))
                    .map(|life| life.courting_since)
            };
            match (since(world, a, b_id), since(world, b, a_id)) {
                (Some(x), Some(y)) if day >= x.max(y) + u64::from(spec.courtship_days) => {
                    pair(world, (a_id, a), (b_id, b), settlement);
                }
                (Some(_), Some(_)) => {}
                _ => {
                    for (actor, other) in [(a, b_id), (b, a_id)] {
                        let mut life = world.get_mut::<Life>(actor).expect("life");
                        life.courting = Some(other);
                        life.courting_since = day;
                    }
                }
            }
        }
        for (id, actor) in single {
            if !taken.contains(&id)
                && let Some(mut life) = world.get_mut::<Life>(actor)
                && life.courting.is_some()
            {
                life.courting = None;
            }
        }
    }
}

fn move_home(world: &mut World, resident: SimId, house: SimId) -> bool {
    let from = home_of(world, resident);
    if from == Some(house) {
        return true;
    }
    let moved = ColonyAct {
        verb: "assign_home".into(),
        resident: Some(resident),
        house: Some(house),
        value: 0,
    }
    .perform(world)
    .is_ok();
    if moved && let Some(from) = from {
        // Children follow the last adult out of a home.
        let empty = world
            .resource::<SimIndex>()
            .get(from)
            .and_then(|entity| world.get::<Household>(entity))
            .is_none_or(|home| living_adults(world, home).is_empty());
        if empty {
            for child in &mut world.resource_mut::<Demography>().children {
                if child.home == from {
                    child.home = house;
                }
            }
        }
    }
    moved
}

/// The founding couples (closing fix B): in every settlement,
/// `founding_couples_percent` of the housed adult founders arrive paired, in
/// identity order, two by two, sharing a home.
fn found_couples(world: &mut World, spec: &DemographySpec) {
    if spec.founding_couples_percent == 0 {
        return;
    }
    for (settlement, _) in settlements::all(world) {
        let adults: Vec<(SimId, Entity)> = members(world, settlement)
            .into_iter()
            .filter(|(id, actor)| {
                world
                    .get::<Life>(*actor)
                    .is_some_and(|life| life.partner.is_none() && life.stage == LifeStage::Adult)
                    && home_of(world, *id).is_some()
                    && world
                        .get::<Villager>(*actor)
                        .is_some_and(|person| person.role != Role::Visitor)
            })
            .collect();
        let couples = adults.len() * spec.founding_couples_percent as usize / 100 / 2;
        for pairs in adults.as_chunks::<2>().0.iter().take(couples) {
            pair(world, pairs[0], pairs[1], settlement);
        }
    }
}

fn pair(world: &mut World, a: (SimId, Entity), b: (SimId, Entity), settlement: SimId) {
    for ((_, actor), (other, _)) in [(a, b), (b, a)] {
        let mut life = world.get_mut::<Life>(actor).expect("life");
        life.partner = Some(other);
        life.courting = None;
    }
    // They share the home with room for both, the first partner's first.
    let homes = [(a.0, b.0), (b.0, a.0)].map(|(host, guest)| {
        home_of(world, host)
            .filter(|house| free_beds(world, *house) > moving_children(world, guest))
    });
    let shared = match homes {
        [Some(house), _] => move_home(world, b.0, house).then_some(house),
        [None, Some(house)] => move_home(world, a.0, house).then_some(house),
        _ => None,
    };
    world.resource_mut::<Demography>().pairings += 1;
    let name = |world: &World, actor: Entity| {
        world
            .get::<Villager>(actor)
            .map_or_else(String::new, |person| person.name.clone())
    };
    record_effect(
        world,
        Some(a.0),
        "VillagePairing",
        &json!({"a":a.0,"b":b.0,"names":[name(world, a.1), name(world, b.1)],
            "settlement":settlement,"home":shared,
            "note":if shared.is_some() { "they share a home" } else {
                "no home has room for both yet" }}),
    );
    super::colony_demand_changed(world);
}

/// Children who would move with a resident: those of a home it is the only
/// adult of.
fn moving_children(world: &World, resident: SimId) -> u32 {
    let Some(house) = home_of(world, resident) else {
        return 0;
    };
    let alone = world
        .resource::<SimIndex>()
        .get(house)
        .and_then(|entity| world.get::<Household>(entity))
        .is_some_and(|home| living_adults(world, home) == [resident]);
    if alone {
        children_of(world, house).len() as u32
    } else {
        0
    }
}

/// Couples with a bed, food and time have children.
fn births(world: &mut World, spec: &DemographySpec, day: u64) {
    let seed = world.resource::<WorldSeed>().0;
    for (a_id, a) in residents(world) {
        let Some(life) = world.get::<Life>(a).cloned() else {
            continue;
        };
        let Some(b_id) = life.partner.filter(|partner| *partner > a_id) else {
            continue;
        };
        let Some(b) = world
            .resource::<SimIndex>()
            .get(b_id)
            .filter(|b| living(world, *b))
        else {
            continue;
        };
        let Some(b_life) = world.get::<Life>(b).cloned() else {
            continue;
        };
        if life.stage != LifeStage::Adult || b_life.stage != LifeStage::Adult {
            continue;
        }
        let (Some(house), Some(other)) = (home_of(world, a_id), home_of(world, b_id)) else {
            continue;
        };
        if house != other {
            continue;
        }
        let last = life.last_birth_day.max(b_life.last_birth_day);
        if last.is_some_and(|last| day < last + u64::from(spec.birth_cooldown_days)) {
            continue;
        }
        // Why a couple that could have a child does not, for explanations
        // (journal only).
        let deferred = |world: &mut World, reason: &str| {
            record_effect(
                world,
                Some(a_id),
                "VillageBirthDeferred",
                &json!({"couple":[a_id, b_id],"reason":reason,"day":day}),
            );
        };
        if children_of(world, house).len() as u32 >= spec.max_children {
            deferred(world, "max_children");
            continue;
        }
        if population(world) >= spec.max_population {
            deferred(world, "max_population");
            continue;
        }
        let settlement_of = settlements::member(world, a);
        if settlement_of.is_none_or(|settlement| {
            household_food(world, house, settlement) < spec.birth_food_units
        }) {
            deferred(world, "household_food");
            continue;
        }
        if [a, b]
            .iter()
            .any(|actor| econ::need(world, *actor, "Hunger") < 30_000)
        {
            deferred(world, "hungry");
            continue;
        }
        let Some(settlement) = settlements::member(world, a) else {
            continue;
        };
        // A free bed in the household, or a home the planner can build: one
        // child more than the beds, in a family no larger than a home the
        // planner has in its list.
        if free_beds(world, house) == 0 {
            let size = occupancy(world, house) + 1;
            let capacity = world
                .resource::<SimIndex>()
                .get(house)
                .and_then(|entity| world.get::<Household>(entity))
                .map_or(0, |home| home.capacity);
            if size > capacity + 1 || size > super::civic::largest_home_beds(world) {
                deferred(world, "no_bed");
                continue;
            }
            if !super::civic::can_build_home(world, settlement) {
                deferred(world, "no_home_affordable");
                continue;
            }
        }
        if sim_rand(seed, a_id, day, BIRTH_PURPOSE) % 1000 >= u64::from(spec.birth_chance_permille)
        {
            continue;
        }
        let child = {
            let id = world.resource::<Demography>().next_child + 1;
            Child {
                id,
                name: pick_name(world, u64::from(id) << 8 | 1, spec),
                born_day: day,
                home: house,
                settlement,
                parents: vec![a_id, b_id],
                fed: 100_000,
                unfed_ticks: 0,
                waiting_for_seat: false,
            }
        };
        for actor in [a, b] {
            world.get_mut::<Life>(actor).expect("life").last_birth_day = Some(day);
        }
        {
            let mut state = world.resource_mut::<Demography>();
            state.next_child = child.id;
            state.births += 1;
            state.children.push(child.clone());
        }
        tally(world, |year| year.births += 1);
        record_effect(
            world,
            Some(a_id),
            "VillageBirth",
            &json!({"child":child.id,"name":child.name,"parents":[a_id, b_id],"home":house,
                "settlement":settlement,"free_beds":free_beds(world, house)}),
        );
        super::colony_demand_changed(world);
    }
}

/// A child of age becomes a resident with a trade and a parent's aptitude.
fn come_of_age(world: &mut World, spec: &DemographySpec, day: u64) {
    let adult_days = u64::from(spec.adult_age_seasons) * season_days(world);
    let seed = world.resource::<WorldSeed>().0;
    let due: Vec<Child> = world
        .resource::<Demography>()
        .children
        .iter()
        .filter(|child| day >= child.born_day + adult_days)
        .cloned()
        .collect();
    for child in due {
        let parents: Vec<(SimId, Entity)> = child
            .parents
            .iter()
            .filter_map(|id| Some((*id, world.resource::<SimIndex>().get(*id)?)))
            .collect();
        // A parent's trade first, else the first free seat of the settlement.
        let seats = labor::free_seats(world, child.settlement);
        let trades: Vec<Role> = parents
            .iter()
            .filter(|(_, actor)| living(world, *actor))
            .filter_map(|(_, actor)| world.get::<Villager>(*actor).map(|person| person.role))
            .collect();
        let Some(seat) = seats
            .iter()
            .find(|(_, role)| trades.contains(role))
            .or_else(|| seats.first())
            .copied()
        else {
            let mut state = world.resource_mut::<Demography>();
            if let Some(slot) = state.children.iter_mut().find(|slot| slot.id == child.id)
                && !slot.waiting_for_seat
            {
                slot.waiting_for_seat = true;
                record_effect(
                    world,
                    None,
                    "VillageYouthWaiting",
                    &json!({"child":child.id,"name":child.name,"settlement":child.settlement,
                        "reason":"no workplace of the settlement has a free seat"}),
                );
            }
            continue;
        };
        // One aptitude of a parent, drawn with the seed.
        let mut inherited: Vec<Role> = parents
            .iter()
            .filter_map(|(_, actor)| world.get::<ResidentProfile>(*actor))
            .flat_map(|profile| profile.allowed_roles.iter().copied())
            .collect();
        inherited.sort_unstable();
        inherited.dedup();
        let aptitude = (!inherited.is_empty()).then(|| {
            inherited[(sim_rand(seed, SimId(0), u64::from(child.id), APTITUDE_PURPOSE)
                % inherited.len() as u64) as usize]
        });
        let home = world
            .resource::<SimIndex>()
            .get(child.home)
            .and_then(|entity| world.get::<Household>(entity))
            .map(|home| home.door);
        let centre = settlements::definition(world, child.settlement).map(|def| def.center);
        let Some(at) = home.or(centre) else {
            continue;
        };
        let extra: Vec<Role> = aptitude.into_iter().collect();
        let Ok(id) = labor::settle(
            world,
            child.settlement,
            &Settler {
                name: &child.name,
                at,
                seat: Some(seat),
                extra_roles: &extra,
                reason: "Came of age and took a place",
            },
        ) else {
            continue;
        };
        let actor = world.resource::<SimIndex>().get(id).expect("new adult");
        enroll(
            world,
            actor,
            id,
            Some(child.born_day as i64),
            spec.founder_max_age_seasons,
            child.parents.clone(),
        );
        {
            let mut state = world.resource_mut::<Demography>();
            state.children.retain(|slot| slot.id != child.id);
            state.came_of_age += 1;
        }
        // The bed the child slept in is now the adult's.
        if home.is_some() {
            move_home(world, id, child.home);
        }
        // A start in life from the parents' own purses.
        let mut gift = 0;
        for (_, parent) in &parents {
            if !living(world, *parent) {
                continue;
            }
            let Some(mut wallet) = world.get_mut::<Wallet>(*parent) else {
                continue;
            };
            let share = (wallet.coins / 4).min(50);
            wallet.coins -= share;
            gift += share;
        }
        world.get_mut::<Wallet>(actor).expect("wallet").coins += gift;
        record_effect(
            world,
            Some(id),
            "VillageComingOfAge",
            &json!({"resident":id,"child":child.id,"name":child.name,"parents":child.parents,
                "role":seat.1.name(),"workplace":seat.0,
                "inherited_aptitude":aptitude.map(Role::name),"home":child.home,
                "gift_coins":gift}),
        );
        super::colony_demand_changed(world);
    }
}

/// Moves crowded families of a settlement into an empty home that fits them,
/// largest need first. Called by the civic planner and when a home is built.
pub fn rehouse_families(world: &mut World, settlement: SimId) {
    if spec(world).is_none() {
        return;
    }
    loop {
        let all = homes(world, settlement);
        let mut crowded: Vec<(u32, SimId)> = all
            .iter()
            .filter_map(|(id, entity)| {
                let capacity = world.get::<Household>(*entity)?.capacity;
                let used = occupancy(world, *id);
                (used > capacity).then_some((used, *id))
            })
            .collect();
        crowded.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        let mut moved = false;
        for (size, from) in crowded {
            // The smallest empty home that holds the whole family.
            let target = all
                .iter()
                .filter(|(id, _)| occupancy(world, *id) == 0)
                .filter_map(|(id, entity)| {
                    let capacity = world.get::<Household>(*entity)?.capacity;
                    (capacity >= size).then_some((capacity, *id))
                })
                .min();
            let Some((_, to)) = target else {
                continue;
            };
            let adults = world
                .resource::<SimIndex>()
                .get(from)
                .and_then(|entity| world.get::<Household>(entity))
                .map(|home| living_adults(world, home))
                .unwrap_or_default();
            for adult in adults {
                move_home(world, adult, to);
            }
            for child in &mut world.resource_mut::<Demography>().children {
                if child.home == from {
                    child.home = to;
                }
            }
            record_effect(
                world,
                None,
                "VillageFamilyMoved",
                &json!({"from":from,"to":to,"people":size,"settlement":settlement}),
            );
            moved = true;
            break;
        }
        if !moved {
            break;
        }
    }
}

/// Children whose home has no living adult are taken in by a household of
/// the settlement: a free bed first, else the smallest household.
fn adopt_orphans(world: &mut World) {
    let orphans: Vec<Child> = world
        .resource::<Demography>()
        .children
        .iter()
        .filter(|child| {
            world
                .resource::<SimIndex>()
                .get(child.home)
                .and_then(|entity| world.get::<Household>(entity))
                .is_none_or(|home| living_adults(world, home).is_empty())
        })
        .cloned()
        .collect();
    for child in orphans {
        let target = homes(world, child.settlement)
            .into_iter()
            .filter(|(id, entity)| {
                *id != child.home
                    && world
                        .get::<Household>(*entity)
                        .is_some_and(|home| !living_adults(world, home).is_empty())
            })
            .map(|(id, _)| (free_beds(world, id) == 0, occupancy(world, id), id))
            .min();
        let Some((_, _, house)) = target else {
            continue;
        };
        if let Some(slot) = world
            .resource_mut::<Demography>()
            .children
            .iter_mut()
            .find(|slot| slot.id == child.id)
        {
            slot.home = house;
        }
        record_effect(
            world,
            None,
            "VillageAdoption",
            &json!({"child":child.id,"name":child.name,"from":child.home,"home":house}),
        );
    }
}

/// What becomes of a dead resident's coins and goods: the adults of the
/// household share them, else the civic fund takes the coins. Called by the
/// death routine before anything is dropped. Returns the cause recorded.
pub(super) fn bequeath(world: &mut World, id: SimId, actor: Entity, cause: &str) {
    if world.get::<ResidentProfile>(actor).is_none() || spec(world).is_none() {
        return;
    }
    let (age_seasons, partner) = world.get::<Life>(actor).map_or((None, None), |life| {
        (
            Some(age_days_at(super::calendar(world).0, life.born_day) / season_days(world)),
            life.partner,
        )
    });
    if let Some(mut life) = world.get_mut::<Life>(actor) {
        life.cause_of_death = cause.into();
    }
    // The partner first, then the other adults of the home, by identity.
    let house = home_of(world, id);
    let mut heirs: Vec<SimId> = partner.into_iter().collect();
    if let Some(home) = house
        .and_then(|house| world.resource::<SimIndex>().get(house))
        .and_then(|entity| world.get::<Household>(entity))
    {
        for member in living_adults(world, home) {
            if member != id && !heirs.contains(&member) {
                heirs.push(member);
            }
        }
    }
    let heirs: Vec<(SimId, Entity)> = heirs
        .into_iter()
        .filter_map(|heir| Some((heir, world.resource::<SimIndex>().get(heir)?)))
        .filter(|(_, entity)| living(world, *entity) && world.get::<Wallet>(*entity).is_some())
        .collect();
    let coins = world
        .get_mut::<Wallet>(actor)
        .map_or(0, |mut wallet| std::mem::take(&mut wallet.coins));
    let mut shares = Vec::new();
    let mut to_fund = coins;
    if !heirs.is_empty() {
        let each = coins / heirs.len() as u32;
        let mut extra = coins % heirs.len() as u32;
        for (heir, entity) in &heirs {
            let share = each + u32::from(extra > 0);
            extra = extra.saturating_sub(1);
            let mut wallet = world.get_mut::<Wallet>(*entity).expect("heir wallet");
            if let Some(total) = wallet.coins.checked_add(share) {
                wallet.coins = total;
                to_fund -= share;
                shares.push(json!({"heir":heir,"coins":share}));
            }
        }
    }
    if to_fund > 0 {
        let mut state = world.resource_mut::<VillageState>();
        state.fund = state.fund.saturating_add(to_fund);
    }
    // Plain goods go to the first heir as far as the pack holds them; tools,
    // clothing and the rest are left where the resident fell.
    let mut goods = Vec::new();
    if let Some((heir, entity)) = heirs.first().copied() {
        let kinds: Vec<&'static str> = economy_spec::registry(world)
            .map(|registry| registry.trade_kinds().to_vec())
            .unwrap_or_default();
        for kind in kinds {
            if !super::civic::storable(world, kind) {
                continue;
            }
            let mut units = econ::count(world, actor, kind);
            while units > 0 && econ::fit(world, entity, &[Item::new(kind).stack(units)]).is_err() {
                units -= 1;
            }
            if units > 0 && econ::consume(world, actor, kind, units).is_ok() {
                econ::grant(world, entity, Item::new(kind).stack(units));
                goods.push(json!({"heir":heir,"kind":kind,"quantity":units}));
            }
        }
    }
    if let Some(partner) = partner
        && let Some(entity) = world.resource::<SimIndex>().get(partner)
        && let Some(mut life) = world.get_mut::<Life>(entity)
    {
        life.partner = None;
    }
    let name = world
        .get::<Villager>(actor)
        .map_or_else(String::new, |person| person.name.clone());
    bump(&mut world.resource_mut::<Demography>().deaths, cause);
    tally(world, |year| year.deaths += 1);
    let gone = Departed {
        resident: Some(id),
        name: name.clone(),
        role: world
            .get::<Villager>(actor)
            .map_or_else(String::new, |person| person.role.name().to_owned()),
        age_seasons: age_seasons.unwrap_or(0),
        cause: cause.into(),
        day: super::calendar(world).0,
        tick: world.resource::<SimClock>().tick,
        household: house,
        settlement: world
            .get::<SettlementMember>(actor)
            .map(|member| member.settlement),
        laid_to_rest: false,
    };
    world.resource_mut::<Demography>().departed.push(gone);
    record_effect(
        world,
        Some(id),
        "VillageInheritance",
        &json!({"from":id,"name":name,"cause":cause,"age_seasons":age_seasons,
            "coins":coins,"heirs":shares,"to_civic_fund":to_fund,"goods":goods,"home":house}),
    );
}

/// Food units a settlement holds: its residents' packs and its stores.
fn settlement_food(world: &World, settlement: SimId) -> u32 {
    let foods = food_kinds(world);
    let carried: u32 = members(world, settlement)
        .into_iter()
        .map(|(_, actor)| {
            foods
                .iter()
                .map(|kind| econ::count(world, actor, kind))
                .sum::<u32>()
        })
        .sum();
    let stored: u32 = super::civic::depots(world, settlement)
        .into_iter()
        .filter_map(|(_, entity)| world.get::<super::civic::Depot>(entity))
        .map(|depot| foods.iter().map(|kind| depot.stock(kind)).sum::<u32>())
        .sum();
    carried + stored
}

/// Whether a settlement can take one more settler now: a free bed, a free
/// seat and food per head. Returns the reason it cannot.
pub fn can_settle(world: &World, settlement: SimId) -> Result<(), &'static str> {
    let Some(spec) = spec(world) else {
        return Err("this world has no demography");
    };
    if population(world) >= spec.max_population {
        return Err("the island is as full as it may get");
    }
    if labor::free_seats(world, settlement).is_empty() {
        return Err("no workplace has a free seat");
    }
    if !homes(world, settlement)
        .into_iter()
        .any(|(id, _)| free_beds(world, id) > 0)
    {
        return Err("no home has a free bed");
    }
    let heads = members(world, settlement).len() as u32
        + world
            .resource::<Demography>()
            .children
            .iter()
            .filter(|child| child.settlement == settlement)
            .count() as u32
        + 1;
    if settlement_food(world, settlement) < heads * spec.arrival_food_per_head {
        return Err("too little food per head");
    }
    if super::civic::budget(world) < spec.arrival_grant_coins {
        return Err("the civic fund cannot pay the arrival grant");
    }
    Ok(())
}

/// Settlers the boat lands now, each in the settlement with the most free
/// beds that can take them. Returns how many came.
pub(super) fn arrive(world: &mut World) -> u32 {
    let Some(spec) = spec(world).cloned() else {
        return 0;
    };
    let dock = super::merchant::status(world).dock;
    let mut landed = 0;
    // The overseer wants a trade nobody on the island can learn: the first
    // settler of this visit comes with that aptitude, to that settlement
    // (review fix S2.7; targets in (settlement, trade) order, one a visit).
    let mut wanted = super::frontier::oversee::untaught_target(world);
    for _ in 0..spec.arrivals_per_visit {
        let asked = wanted.take();
        let target = settlements::all(world)
            .into_iter()
            .filter(|(id, _)| can_settle(world, *id).is_ok())
            .filter(|(id, _)| asked.is_none_or(|(settlement, _)| settlement == *id))
            .map(|(id, _)| {
                let beds: u32 = homes(world, id)
                    .into_iter()
                    .map(|(house, _)| free_beds(world, house))
                    .sum();
                (std::cmp::Reverse(beds), id)
            })
            .min();
        let Some((_, settlement)) = target else {
            break;
        };
        let serial = u64::from(world.resource::<Demography>().arrivals) << 8 | 2;
        let name = pick_name(world, serial, &spec);
        let centre = settlements::definition(world, settlement).map(|def| def.center);
        let Some(at) = dock.or(centre) else {
            break;
        };
        let Ok(id) = labor::settle(
            world,
            settlement,
            &Settler {
                name: &name,
                at,
                seat: None,
                extra_roles: &asked.map(|(_, role)| vec![role]).unwrap_or_default(),
                reason: "Arrived by boat and took a free place",
            },
        ) else {
            break;
        };
        let actor = world.resource::<SimIndex>().get(id).expect("settler");
        enrolled(world, actor, id);
        super::merchant::land_rations(world, actor);
        let grant = spec.arrival_grant_coins.min(super::civic::budget(world));
        if grant > 0 {
            world.resource_mut::<VillageState>().fund -= grant;
            world.get_mut::<Wallet>(actor).expect("wallet").coins += grant;
        }
        super::civic::house_homeless(world, settlement);
        world.resource_mut::<Demography>().arrivals += 1;
        tally(world, |year| year.arrivals += 1);
        landed += 1;
        let role = world
            .get::<Villager>(actor)
            .map_or("", |person| person.role.name());
        record_effect(
            world,
            Some(id),
            "VillageArrival",
            &json!({"resident":id,"name":name,"settlement":settlement,"role":role,"by":"boat",
                "grant_coins":grant,"home":home_of(world, id),
                "age_seasons":age_days(world, actor).map(|days| days / season_days(world))}),
        );
    }
    landed
}

/// Once per tick in the regional Effects pass.
pub fn tick(world: &mut World) {
    let Some(spec) = spec(world).cloned() else {
        return;
    };
    let now = world.resource::<SimClock>().tick;
    if now > 0 && now.is_multiple_of(FEED_ROUND_TICKS) {
        feed_children(world, &spec);
    }
    let day = super::calendar(world).0;
    let last = world.resource::<Demography>().last_day;
    if last == Some(day) {
        return;
    }
    world.resource_mut::<Demography>().last_day = Some(day);
    if last.is_none() {
        // The founding day: nothing has happened yet but the founding couples.
        found_couples(world, &spec);
        return;
    }
    lay_to_rest(world);
    age_residents(world, &spec, day);
    adopt_orphans(world);
    court(world, &spec, day);
    births(world, &spec, day);
    come_of_age(world, &spec, day);
}

/// Removes the bodies of residents who died at least a day ago: the bed is
/// free, the identity no longer resolves, and [`Demography::departed`] keeps
/// the record. Somebody under external control is left for its host.
fn lay_to_rest(world: &mut World) {
    let now = world.resource::<SimClock>().tick;
    let day = world.resource::<SimConfig>().day_length;
    let Some(mut query) = world.try_query::<(Entity, &SimId, &ResidentProfile, &super::Vitality)>()
    else {
        return;
    };
    let mut gone: Vec<(SimId, Entity)> = query
        .iter(world)
        .filter(|(entity, _, _, vitality)| {
            vitality
                .died_at
                .is_some_and(|died| died.saturating_add(day) <= now)
                && world
                    .get::<npc_framework::agent_io::ExternalController>(*entity)
                    .is_none()
        })
        .map(|(entity, id, _, _)| (*id, entity))
        .collect();
    gone.sort_unstable_by_key(|(id, _)| *id);
    if gone.is_empty() {
        return;
    }
    for (id, actor) in gone {
        let homes: Vec<Entity> = world
            .try_query::<(Entity, &Household)>()
            .map(|mut query| {
                query
                    .iter(world)
                    .filter(|(_, home)| home.residents.contains(&id))
                    .map(|(entity, _)| entity)
                    .collect()
            })
            .unwrap_or_default();
        for home in homes {
            if let Some(mut home) = world.get_mut::<Household>(home) {
                home.residents.retain(|member| *member != id);
            }
        }
        for item in npc_framework::interaction::items::carried_items(world, actor) {
            if let Some(item) = world.resource::<SimIndex>().get(item) {
                let pos = world.get::<SimPos>(actor).copied().unwrap_or_default();
                npc_framework::interaction::items::place_in_world(world, item, pos);
            }
        }
        if let Some(record) = world
            .resource_mut::<Demography>()
            .departed
            .iter_mut()
            .find(|record| record.resident == Some(id))
        {
            record.laid_to_rest = true;
        }
        record_effect(
            world,
            Some(id),
            "VillageLaidToRest",
            &json!({"resident":id}),
        );
        let record = world
            .resource::<Demography>()
            .departed
            .iter()
            .find(|record| record.resident == Some(id))
            .cloned();
        if let Some(record) = record {
            super::live::mourn(world, &record);
        }
        world.despawn(actor);
    }
    super::colony_demand_changed(world);
}

/// Everybody who has died, in order of death.
pub fn departed(world: &World) -> Vec<Departed> {
    world
        .get_resource::<Demography>()
        .map(|state| state.departed.clone())
        .unwrap_or_default()
}

/// Whether a child of this adult's home has eaten less than `below` and no
/// adult of the home carries food: somebody has to go and fetch some.
pub fn child_needs_food(world: &World, actor: Entity, below: i32) -> bool {
    let Some(state) = world.get_resource::<Demography>() else {
        return false;
    };
    if state.children.is_empty() || spec(world).is_none() {
        return false;
    }
    let Some(house) = world.get::<SimId>(actor).and_then(|id| home_of(world, *id)) else {
        return false;
    };
    if !state
        .children
        .iter()
        .any(|child| child.home == house && child.fed < below)
    {
        return false;
    }
    let foods = food_kinds(world);
    let index = world.resource::<SimIndex>();
    !index
        .get(house)
        .and_then(|entity| world.get::<Household>(entity))
        .map(|home| living_adults(world, home))
        .unwrap_or_default()
        .into_iter()
        .filter_map(|adult| index.get(adult))
        .any(|adult| foods.iter().any(|kind| econ::count(world, adult, kind) > 0))
}

/// One member of a household or family.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PersonView {
    /// The resident; `None` for a child, which is not an actor yet.
    pub id: Option<SimId>,
    /// The child's number; `None` for a resident.
    pub child: Option<u32>,
    pub name: String,
    pub stage: &'static str,
    pub age_days: u64,
    pub age_seasons: u64,
    /// Whole years of four seasons.
    pub age_years: u64,
    /// Lower-case profession; empty for a child.
    pub role: &'static str,
    pub alive: bool,
    /// 0 to 100000 for a child; `None` for a resident.
    pub fed: Option<i32>,
}

/// Home and family of one resident.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct HouseholdView {
    pub resident: SimId,
    pub person: PersonView,
    /// The household; `None` for somebody without a bed.
    pub house: Option<SimId>,
    pub label: String,
    pub door: Option<SimPos>,
    pub capacity: u32,
    pub free_beds: u32,
    /// Living residents of the household, this one included.
    pub members: Vec<PersonView>,
    /// Children living in the household.
    pub children_at_home: Vec<PersonView>,
    pub partner: Option<PersonView>,
    pub courting: Option<PersonView>,
    pub parents: Vec<PersonView>,
    /// Sons and daughters of this resident, grown or not, wherever they live.
    pub children: Vec<PersonView>,
    /// Calendar day (first day is 1) on which the resident becomes an elder.
    pub elder_from_day: Option<u64>,
}

fn person(world: &World, id: SimId) -> Option<PersonView> {
    let actor = world.resource::<SimIndex>().get(id)?;
    let villager = world.get::<Villager>(actor)?;
    let days = age_days(world, actor).unwrap_or(0);
    let season = season_days(world);
    Some(PersonView {
        id: Some(id),
        child: None,
        name: villager.name.clone(),
        stage: stage(world, actor).name(),
        age_days: days,
        age_seasons: days / season,
        age_years: days / (season * 4),
        role: villager.role.name(),
        alive: living(world, actor),
        fed: None,
    })
}

fn child_view(world: &World, child: &Child) -> PersonView {
    let days = age_days_at(super::calendar(world).0, child.born_day as i64);
    let season = season_days(world);
    PersonView {
        id: None,
        child: Some(child.id),
        name: child.name.clone(),
        stage: LifeStage::Child.name(),
        age_days: days,
        age_seasons: days / season,
        age_years: days / (season * 4),
        role: "",
        alive: true,
        fed: Some(child.fed),
    }
}

/// Household and family of a resident: members, ages, partner, children.
pub fn household(world: &World, resident: SimId) -> Option<HouseholdView> {
    let actor = world.resource::<SimIndex>().get(resident)?;
    let me = person(world, resident)?;
    let life = world.get::<Life>(actor);
    let house = home_of(world, resident);
    let home = house
        .and_then(|house| world.resource::<SimIndex>().get(house))
        .and_then(|entity| world.get::<Household>(entity));
    let state = world.get_resource::<Demography>();
    let mut children: Vec<PersonView> = state
        .into_iter()
        .flat_map(|state| state.children.iter())
        .filter(|child| child.parents.contains(&resident))
        .map(|child| child_view(world, child))
        .collect();
    if let Some(mut query) = world.try_query::<(&SimId, &Life)>() {
        let mut grown: Vec<SimId> = query
            .iter(world)
            .filter(|(_, life)| life.parents.contains(&resident))
            .map(|(id, _)| *id)
            .collect();
        grown.sort_unstable();
        children.extend(grown.into_iter().filter_map(|id| person(world, id)));
    }
    Some(HouseholdView {
        resident,
        person: me,
        house,
        label: home.map_or_else(String::new, |home| home.label.clone()),
        door: home.map(|home| home.door),
        capacity: home.map_or(0, |home| home.capacity),
        free_beds: house.map_or(0, |house| free_beds(world, house)),
        members: home
            .map(|home| living_adults(world, home))
            .unwrap_or_default()
            .into_iter()
            .filter_map(|id| person(world, id))
            .collect(),
        children_at_home: house
            .map(|house| children_of(world, house))
            .unwrap_or_default()
            .into_iter()
            .map(|child| child_view(world, child))
            .collect(),
        partner: life
            .and_then(|life| life.partner)
            .and_then(|id| person(world, id)),
        courting: life
            .and_then(|life| life.courting)
            .and_then(|id| person(world, id)),
        parents: life
            .map(|life| life.parents.clone())
            .unwrap_or_default()
            .into_iter()
            .filter_map(|id| person(world, id))
            .collect(),
        children,
        elder_from_day: life.zip(spec(world)).map(|(life, spec)| {
            (life.born_day + i64::from(spec.elder_age_seasons) * season_days(world) as i64).max(0)
                as u64
                + 1
        }),
    })
}

/// Vital statistics, for panels and soak reports.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Census {
    pub residents: u32,
    pub children: u32,
    pub elders: u32,
    pub couples: u32,
    pub births: u32,
    pub came_of_age: u32,
    pub pairings: u32,
    pub arrivals: u32,
    pub deaths: BTreeMap<String, u32>,
}

pub fn census(world: &World) -> Census {
    let people = residents(world);
    let state = world.get_resource::<Demography>();
    let lives: Vec<&Life> = people
        .iter()
        .filter_map(|(_, actor)| world.get::<Life>(*actor))
        .collect();
    Census {
        residents: people.len() as u32,
        children: state.map_or(0, |state| state.children.len() as u32),
        elders: lives
            .iter()
            .filter(|life| life.stage == LifeStage::Elder)
            .count() as u32,
        couples: lives.iter().filter(|life| life.partner.is_some()).count() as u32 / 2,
        births: state.map_or(0, |state| state.births),
        came_of_age: state.map_or(0, |state| state.came_of_age),
        pairings: state.map_or(0, |state| state.pairings),
        arrivals: state.map_or(0, |state| state.arrivals),
        deaths: state.map(|state| state.deaths.clone()).unwrap_or_default(),
    }
}

/// Checks family references after a load.
pub fn validate(world: &World) -> Result<(), String> {
    let Some(state) = world.get_resource::<Demography>() else {
        return Ok(());
    };
    let index = world.resource::<SimIndex>();
    let mut seen = BTreeSet::new();
    for child in &state.children {
        if !seen.insert(child.id) || child.id > state.next_child {
            return Err(format!(
                "Child {} is recorded twice or out of order",
                child.id
            ));
        }
        if index
            .get(child.home)
            .and_then(|entity| world.get::<Household>(entity))
            .is_none()
        {
            return Err(format!("Child {} lives in no household", child.id));
        }
        if !(0..=100_000).contains(&child.fed) {
            return Err(format!("Child {} has an invalid fill", child.id));
        }
    }
    if let Some(mut query) = world.try_query::<(Entity, &SimId, &Life)>() {
        for (entity, id, life) in query.iter(world) {
            if living(world, entity)
                && let Some(partner) = life.partner
                && index
                    .get(partner)
                    .and_then(|entity| world.get::<Life>(entity))
                    .is_none_or(|other| other.partner != Some(*id))
            {
                return Err(format!("Resident {} has a one-sided partnership", id.0));
            }
        }
    }
    Ok(())
}
