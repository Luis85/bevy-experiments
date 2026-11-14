//! The season ledger (review fix pass, S5): what each season of the island
//! made, used, lost, bought and sold, for the season card.
//!
//! Hashed and saved although it steers nothing, so the determinism tests
//! catch accounting that depends on order. The island already keeps running
//! totals (goods produced, the boat's imports and exports, births, deaths,
//! arrivals, boat visits, buildings standing); the ledger samples them when a
//! season opens and when it closes, and a season's figure is the difference.
//! What has no running total (goods used, tools worn out, the worst
//! shortage) is counted where it happens.
use super::view::economy::{GoodSeason, SeasonSummary};
use bevy_ecs::prelude::*;
use npc_framework::kernel::hash::SimHashResource;
use npc_framework::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};

/// Seasons the ledger keeps; the oldest is dropped when a new one opens.
pub const SEASONS_KEPT: usize = 8;

/// The island's running totals at one moment.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Totals {
    pub population: u32,
    pub fund: u32,
    pub prices: BTreeMap<String, u32>,
    pub produced: BTreeMap<String, u64>,
    pub imported: BTreeMap<String, u64>,
    pub exported: BTreeMap<String, u64>,
    pub births: u32,
    pub arrivals: u32,
    pub deaths: BTreeMap<String, u32>,
    pub built: BTreeMap<String, u32>,
    pub boat_visits: u32,
}

/// One season.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeasonTally {
    /// Seasons since the founding, from 0.
    pub season: u32,
    /// The calendar season it was (`Spring` ..).
    pub name: String,
    pub start: Totals,
    /// Set when the season closes.
    pub end: Option<Totals>,
    /// Units consumed (eaten, drunk, burned, used as inputs or materials).
    pub used: BTreeMap<String, u64>,
    pub spoiled: BTreeMap<String, u64>,
    pub tools_worn: u32,
    pub worst_shortage: Option<(String, u32)>,
}

#[derive(Resource, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeasonLedger {
    pub seasons: VecDeque<SeasonTally>,
}

fn hash_map64(h: &mut Fnv64, map: &BTreeMap<String, u64>) {
    h.u32(map.len() as u32);
    for (key, n) in map {
        h.str(key);
        h.u64(*n);
    }
}
fn hash_map32(h: &mut Fnv64, map: &BTreeMap<String, u32>) {
    h.u32(map.len() as u32);
    for (key, n) in map {
        h.str(key);
        h.u32(*n);
    }
}
fn hash_totals(h: &mut Fnv64, totals: &Totals) {
    h.u32(totals.population);
    h.u32(totals.fund);
    hash_map32(h, &totals.prices);
    hash_map64(h, &totals.produced);
    hash_map64(h, &totals.imported);
    hash_map64(h, &totals.exported);
    h.u32(totals.births);
    h.u32(totals.arrivals);
    hash_map32(h, &totals.deaths);
    hash_map32(h, &totals.built);
    h.u32(totals.boat_visits);
}

impl SimHashResource for SeasonLedger {
    const NAME: &'static str = "island_season_ledger_v1";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u32(self.seasons.len() as u32);
        for tally in &self.seasons {
            h.u32(tally.season);
            h.str(&tally.name);
            hash_totals(h, &tally.start);
            h.opt(tally.end.as_ref(), hash_totals);
            hash_map64(h, &tally.used);
            hash_map64(h, &tally.spoiled);
            h.u32(tally.tools_worn);
            h.opt(tally.worst_shortage.as_ref(), |h, (good, days)| {
                h.str(good);
                h.u32(*days);
            });
        }
    }
    fn snapshot_fns() -> Option<npc_framework::snapshot::ResourceSnapshotFns> {
        Some(npc_framework::snapshot::serde_resource_fns::<Self>())
    }
}

pub fn register(app: &mut bevy_app::App) {
    app.init_resource::<SeasonLedger>()
        .register_sim_resource::<SeasonLedger>();
}

/// Seasons since the founding at the current tick.
fn season_index(world: &World) -> u32 {
    let day = world.resource::<SimClock>().tick / world.resource::<SimConfig>().day_length.max(1);
    (day / super::days_per_season(world).max(1)) as u32
}

/// The island's running totals now.
pub fn totals(world: &World) -> Totals {
    let census = super::demography::census(world);
    let merchant = world.get_resource::<super::merchant::Merchant>();
    let mut built = BTreeMap::new();
    if let Some(civic) = world.get_resource::<super::civic::CivicState>() {
        for id in civic.buildings.values().flatten() {
            if civic.founded.binary_search(id).is_ok() {
                continue;
            }
            if let Some(building) = world
                .resource::<SimIndex>()
                .get(*id)
                .and_then(|entity| world.get::<super::civic::Building>(entity))
            {
                *built.entry(building.building.clone()).or_default() += 1;
            }
        }
    }
    Totals {
        population: super::demography::residents(world)
            .into_iter()
            .filter(|(_, actor)| super::living(world, *actor))
            .count() as u32,
        fund: world
            .get_resource::<super::VillageState>()
            .map_or(0, |state| state.fund),
        prices: world
            .get_resource::<super::market::MarketState>()
            .map_or_else(BTreeMap::new, |state| {
                state
                    .commodities
                    .keys()
                    .filter_map(|good| Some((good.clone(), state.price(good)?)))
                    .collect()
            }),
        produced: world
            .get_resource::<super::VillageState>()
            .map_or_else(BTreeMap::new, |state| state.produced.clone()),
        imported: merchant.map_or_else(BTreeMap::new, |m| m.imported.clone()),
        exported: merchant.map_or_else(BTreeMap::new, |m| m.exported.clone()),
        births: census.births,
        arrivals: census.arrivals,
        deaths: census.deaths,
        built,
        boat_visits: merchant.map_or(0, |m| m.visits),
    }
}

/// Opens the first season, and a new one when the season turns (closing the
/// last). Called first in the island's tick.
pub fn tick(world: &mut World) {
    let index = season_index(world);
    if world
        .resource::<SeasonLedger>()
        .seasons
        .back()
        .is_some_and(|tally| tally.season == index)
    {
        // Once a day: the shortest stock of any village this season.
        let now = world.resource::<SimClock>().tick;
        if now.is_multiple_of(world.resource::<SimConfig>().day_length.max(1)) {
            let worst = super::frontier::settlements::all(world)
                .into_iter()
                .filter_map(|(id, _)| super::view::economy::outlook(world, id).next_shortage)
                .min_by(|a, b| (a.days_left, &a.good).cmp(&(b.days_left, &b.good)));
            if let Some(worst) = worst {
                shortage(world, &worst.good, worst.days_left);
            }
        }
        return;
    }
    let now = totals(world);
    let name = format!("{:?}", super::calendar(world).1);
    let mut ledger = world.resource_mut::<SeasonLedger>();
    if let Some(last) = ledger.seasons.back_mut() {
        last.end = Some(now.clone());
    }
    ledger.seasons.push_back(SeasonTally {
        season: index,
        name,
        start: now,
        ..SeasonTally::default()
    });
    while ledger.seasons.len() > SEASONS_KEPT {
        ledger.seasons.pop_front();
    }
}

/// Applies `change` to the open season, if the ledger is kept.
fn with_open(world: &mut World, change: impl FnOnce(&mut SeasonTally)) {
    if let Some(mut ledger) = world.get_resource_mut::<SeasonLedger>()
        && let Some(tally) = ledger.seasons.back_mut()
    {
        change(tally);
    }
}

/// Units of `good` consumed by anybody (food eaten, inputs, materials).
pub fn used(world: &mut World, good: &str, units: u32) {
    with_open(world, |tally| {
        *tally.used.entry(good.to_owned()).or_default() += u64::from(units);
    });
}
/// Units of `good` that spoiled (S2.4).
pub fn spoiled(world: &mut World, good: &str, units: u32) {
    with_open(world, |tally| {
        *tally.spoiled.entry(good.to_owned()).or_default() += u64::from(units);
    });
}
pub fn tool_worn(world: &mut World) {
    with_open(world, |tally| tally.tools_worn += 1);
}
/// Records a good's fewest days left, keeping the worst of the season.
pub fn shortage(world: &mut World, good: &str, days_left: u32) {
    with_open(world, |tally| {
        if tally
            .worst_shortage
            .as_ref()
            .is_none_or(|(_, worst)| days_left < *worst)
        {
            tally.worst_shortage = Some((good.to_owned(), days_left));
        }
    });
}

fn delta64(end: &BTreeMap<String, u64>, start: &BTreeMap<String, u64>, key: &str) -> u64 {
    end.get(key)
        .copied()
        .unwrap_or(0)
        .saturating_sub(start.get(key).copied().unwrap_or(0))
}

/// The view of one tally; an open season reads its figures so far.
pub fn summary(world: &World, tally: &SeasonTally) -> SeasonSummary {
    let live;
    let end = match &tally.end {
        Some(end) => end,
        None => {
            live = totals(world);
            &live
        }
    };
    let start = &tally.start;
    let mut keys: std::collections::BTreeSet<&String> = BTreeSet::new();
    for map in [
        &end.produced,
        &end.imported,
        &end.exported,
        &tally.used,
        &tally.spoiled,
    ] {
        keys.extend(map.keys());
    }
    keys.extend(end.prices.keys());
    let goods = keys
        .into_iter()
        .map(|good| GoodSeason {
            good: good.clone(),
            produced: delta64(&end.produced, &start.produced, good),
            eaten_or_used: tally.used.get(good).copied().unwrap_or(0),
            spoiled: tally.spoiled.get(good).copied().unwrap_or(0),
            imported: delta64(&end.imported, &start.imported, good),
            exported: delta64(&end.exported, &start.exported, good),
            price_start: start.prices.get(good).copied(),
            price_end: end.prices.get(good).copied(),
        })
        .collect();
    let counts = |end: &BTreeMap<String, u32>, start: &BTreeMap<String, u32>| {
        end.iter()
            .filter_map(|(key, n)| {
                let n = n.saturating_sub(start.get(key).copied().unwrap_or(0));
                (n > 0).then(|| (key.clone(), n))
            })
            .collect::<Vec<_>>()
    };
    SeasonSummary {
        season: tally.season,
        year: tally.season / 4 + 1,
        name: tally.name.clone(),
        population_start: start.population,
        population_end: end.population,
        births: end.births.saturating_sub(start.births),
        deaths: counts(&end.deaths, &start.deaths),
        arrivals: end.arrivals.saturating_sub(start.arrivals),
        built: counts(&end.built, &start.built),
        goods,
        tools_worn: tally.tools_worn,
        fund_start: start.fund,
        fund_end: end.fund,
        boat_visits: end.boat_visits.saturating_sub(start.boat_visits),
        worst_shortage: tally.worst_shortage.clone(),
    }
}

use std::collections::BTreeSet;
