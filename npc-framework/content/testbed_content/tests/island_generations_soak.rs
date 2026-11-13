//! Multi-year soak of the coastal world and its tick rate as the village
//! grows. Both tests are explicit; run them in release:
//!
//! ```text
//! cargo test --locked --release -p testbed_content --test island_generations_soak -- --ignored --nocapture
//! ```
//!
//! The soak shortens the year to twelve days (`days_per_season: 3`), so three
//! years are 864000 ticks a seed. Ages are counted in seasons, so every life
//! shortens with the year; the settings counted in days are scaled alongside
//! in [`compressed`].

// determinism: wall-clock timing of the test harness only (ticks per second
// as `f64`); never sim state.
#![allow(
    clippy::disallowed_methods,
    clippy::disallowed_types,
    clippy::float_arithmetic
)]

mod life_support;

use bevy_app::App;
use bevy_ecs::prelude::*;
use life_support::*;
use npc_framework::interaction::Item;
use npc_framework::prelude::*;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use testbed_content::village::{
    self, Season, VillageState, Villager, Vitality, Wallet, catalog, civic, colony, demography,
    economy as econ, fields,
    frontier::{labor, life_spec::Shock},
    market, merchant, view, weather,
};

const DAY: u64 = 24_000;
const SEASON_DAYS: u64 = 3;
const SAMPLE: u64 = 600;
const NEEDS: [&str; 4] = ["Hunger", "Thirst", "Energy", "Warmth"];

/// A twelve-day year. What the data counts in days keeps its share of a
/// season: the boat calls every two days instead of four, a couple waits four
/// days between births instead of ten and courts for one day instead of two.
fn compressed(world: &mut Value) {
    let economy = economy(world);
    economy["tuning"]["days_per_season"] = json!(SEASON_DAYS);
    economy["tuning"]["winter_fuel_lead_days"] = json!(1);
    economy["demography"]["birth_cooldown_days"] = json!(4);
    economy["demography"]["courtship_days"] = json!(1);
    economy["merchant"]["interval_days"] = json!(2);
    economy["merchant"]["first_visit_day"] = json!(1);
}

#[derive(Default)]
struct Run {
    seed: u64,
    coins: u64,
    /// One row per season: what a reader needs to see the village live.
    rows: Vec<String>,
    min_needs: BTreeMap<&'static str, i32>,
    least_people: u32,
    most_people: u32,
    /// Deaths of want outside a harsh winter: `(tick, cause, name)`.
    failures: Vec<(u64, String, String)>,
    /// Deaths of cold or want in a harsh winter.
    harsh: Vec<(u64, String, String)>,
    seen_dead: std::collections::BTreeSet<SimId>,
    worst_price: (u32, String),
    /// Samples taken, and samples in which a staple stood within a tenth of
    /// the price ceiling, by good.
    samples: u32,
    at_ceiling: BTreeMap<&'static str, u32>,
    /// Residents already reported for a need below a tenth.
    warned: std::collections::BTreeSet<(SimId, &'static str)>,
    traced: Vec<SimId>,
    /// Ticks in a year of this run.
    year: u64,
    hungry_children: std::collections::BTreeSet<u32>,
    /// Samples of a housed resident resting outside its home: in a bed's rest
    /// (never allowed) and collapsed from exhaustion.
    asleep_outside: u32,
    collapsed_outside: u32,
    /// Samples of a resident indoors, and of one asleep.
    indoors: u64,
    asleep: u64,
    /// Building and relief levies seen in the journal: `(count, coins)`.
    building_levies: (u32, u64),
    relief_levies: (u32, u64),
    /// Boat duty paid, from the journal: `(count, coins)`.
    duties: (u32, u64),
    /// Samples with the fund above the reserve, and the fund at each day's end.
    fund_above_reserve: u32,
    fund_days: Vec<u32>,
    last_journal_tick: u64,
    /// The journal ring dropped entries between two samples.
    journal_overflows: u32,
    /// Samples in which a good stood at the lowest quote the market allows.
    at_floor: BTreeMap<&'static str, u32>,
    /// Per need, the lowest value among living residents at each sample.
    lowest_series: BTreeMap<&'static str, Vec<i32>>,
    /// Role and workplace of each resident at the last sample, and changes.
    assignments: BTreeMap<SimId, (village::Role, SimId)>,
    job_changes: u64,
    /// Couple-days a birth was deferred, by the gate that stopped it.
    births_deferred: BTreeMap<String, u32>,
    /// Beds rented at inns, the civic top-ups of keepers' wages and the coats
    /// the fund paid for: `(count, coins)`.
    inn_nights: u32,
    /// Inn guests by their trade: who sleeps at the inns.
    inn_guests: BTreeMap<String, u32>,
    service_wages: (u32, u64),
    coat_grants: (u32, u64),
    /// Civic sites by id: building, tick commissioned, tick of the delivery
    /// that supplied it, tick completed.
    sites: BTreeMap<u64, (String, u64, Option<u64>, Option<u64>)>,
    /// The civic fund's journaled flows since the last season row, by
    /// source: income positive, spending negative.
    fund_flows: BTreeMap<&'static str, i64>,
    /// The fund at the last season row, for the untracked remainder.
    fund_at_row: i64,
    /// Changes of trade, `"From->To"`.
    trade_moves: BTreeMap<String, u32>,
    /// Boat orders closed: filled, part-filled, expired with nothing.
    boat_orders: (u32, u32, u32),
    /// Coats bought by residents (units).
    coats_sold: u64,
}

/// Merges `patch` into `target` (JSON merge patch).
fn merge(target: &mut Value, patch: &Value) {
    match (target, patch) {
        (Value::Object(target), Value::Object(patch)) => {
            for (key, value) in patch {
                merge(target.entry(key.clone()).or_insert(Value::Null), value);
            }
        }
        (target, patch) => *target = patch.clone(),
    }
}

/// `GENERATIONS_ECONOMY_PATCH`, a JSON merge patch of the economy block, for
/// an explicit A/B run of the same binary.
fn patched(world: &mut Value) {
    if let Ok(text) = std::env::var("GENERATIONS_ECONOMY_PATCH") {
        let patch: Value = serde_json::from_str(&text).expect("GENERATIONS_ECONOMY_PATCH is JSON");
        println!("GENERATIONS_PATCH {patch}");
        merge(economy(world), &patch);
    }
    // `GENERATIONS_WORLD_PATCH_FILE`: a JSON merge patch of the whole world
    // document (the AI policy, the resident tree, ...), for a bisection.
    if let Ok(path) = std::env::var("GENERATIONS_WORLD_PATCH_FILE") {
        let text = std::fs::read_to_string(&path).expect("world patch file");
        let patch: Value = serde_json::from_str(&text).expect("world patch is JSON");
        println!("GENERATIONS_WORLD_PATCH {path}");
        merge(world, &patch);
    }
}

const STAPLES: [&str; 4] = ["bread", catalog::WATER, "log", catalog::GRAIN];
/// Goods whose share of samples at the price floor the run reports.
const FLOOR_WATCHED: [&str; 4] = ["bread", catalog::GRAIN, "plank", "log"];

/// The fifth percentile of a series.
fn percentile_5(series: &[i32]) -> i32 {
    let mut sorted = series.to_vec();
    sorted.sort_unstable();
    sorted.get(sorted.len() * 5 / 100).copied().unwrap_or(0)
}

fn price_percent(world: &World, kind: &str) -> u32 {
    let base = catalog::price_in(world, kind).unwrap_or(1).max(1);
    market::unit_price(world, kind).unwrap_or(base) * 100 / base
}

impl Run {
    fn sample(&mut self, app: &mut App) {
        let world = app.world();
        let now = world.resource::<SimClock>().tick;
        assert_eq!(
            money(world),
            self.coins,
            "seed {} tick {now}: coins drifted (island plus the merchant's purse)",
            self.seed
        );
        let harsh = weather::shock(world) == Some(Shock::HarshWinter);
        for (id, actor) in all_residents(world) {
            let vitality = world.get::<Vitality>(actor).expect("vitality");
            if vitality.died_at.is_some() {
                if self.seen_dead.insert(id) {
                    let life = world.get::<demography::Life>(actor).expect("life");
                    let name = world
                        .get::<village::Villager>(actor)
                        .map_or_else(String::new, |person| person.name.clone());
                    if life.cause_of_death != "old age" {
                        let entry = (now, life.cause_of_death.clone(), name);
                        if harsh {
                            self.harsh.push(entry);
                        } else {
                            self.failures.push(entry);
                        }
                    }
                }
                continue;
            }
            let person = world.get::<village::Villager>(actor).expect("villager");
            let assignment = (person.role, person.workplace);
            if let Some(old) = self.assignments.insert(id, assignment)
                && old != assignment
            {
                self.job_changes += 1;
                if old.0 != assignment.0 {
                    *self
                        .trade_moves
                        .entry(format!("{:?}->{:?}", old.0, assignment.0))
                        .or_default() += 1;
                }
            }
            let inside = view::inside(world, id);
            self.indoors += u64::from(inside.is_some());
            self.asleep += u64::from(view::repose(world, actor) == view::Repose::Asleep);
            // A rented bed at an inn is indoors too (ADR 0041).
            if person.resting_until > now
                && colony::home_of(world, id).is_some()
                && inside != colony::home_of(world, id)
                && !inside.is_some_and(|building| village::indoors::is_inn(world, building))
            {
                // A bed's rest (the night's sleep or a nap) against the slow
                // ground rest of exhaustion.
                if village::indoors::asleep(world, actor)
                    || person.rest_rate > village::GROUND_REST_RATE
                {
                    self.asleep_outside += 1;
                    if self.asleep_outside <= 12 {
                        println!(
                            "GENERATIONS_ASLEEP_OUTSIDE seed={} tick={now} {} ({}) rate={}                              inside={inside:?} home={:?} from_home_m={} doing={:?}",
                            self.seed,
                            person.name,
                            id.0,
                            person.rest_rate,
                            colony::home_of(world, id),
                            world
                                .get::<SimPos>(actor)
                                .map_or(0, |at| at.dist2(person.home).isqrt() / 1000),
                            view::activity_label(world, actor),
                        );
                    }
                } else {
                    self.collapsed_outside += 1;
                }
            }
            // A resident in want is followed for a while: what it is doing,
            // why, and how far from home.
            if NEEDS
                .iter()
                .any(|need| econ::need(world, actor, need) < 15_000)
                && self.traced.iter().filter(|seen| **seen == id).count() < 24
            {
                self.traced.push(id);
                let here = world.get::<SimPos>(actor).copied();
                let home = person.home;
                let decision = view::latest_decision(world, actor);
                println!(
                    "GENERATIONS_TRACE seed={} tick={now} {} ({}) needs={:?} coins={} doing={:?} branch={:?} action={:?} from_home_m={:?} inside={inside:?} resting={} moving={}",
                    self.seed,
                    person.name,
                    id.0,
                    NEEDS.map(|need| econ::need(world, actor, need) / 1000),
                    world.get::<Wallet>(actor).map_or(0, |wallet| wallet.coins),
                    view::activity_label(world, actor),
                    decision.as_ref().map(|d| d.branch.last().cloned()),
                    decision
                        .as_ref()
                        .map(|d| format!("{} {}", d.action, d.param)),
                    here.map(|pos| pos.dist2(home).isqrt() / 1000),
                    person.resting_until > now,
                    world.get::<MoveIntent>(actor).is_some(),
                );
            }
            for need in NEEDS {
                let value = econ::need(world, actor, need);
                let lowest = self.min_needs.entry(need).or_insert(100_000);
                *lowest = (*lowest).min(value);
                if value < 10_000 && self.warned.insert((id, need)) {
                    let person = world.get::<village::Villager>(actor).expect("villager");
                    let food: u32 = ["bread", catalog::COOKED_MEAL, catalog::BERRIES]
                        .iter()
                        .map(|kind| econ::count(world, actor, kind))
                        .sum();
                    println!(
                        "GENERATIONS_LOW seed={} tick={now} {} ({}) {need}={value} role={} coins={} food={food} stage={:?} housed={} weather={:?}",
                        self.seed,
                        person.name,
                        id.0,
                        person.role.name(),
                        world.get::<Wallet>(actor).map_or(0, |wallet| wallet.coins),
                        demography::stage(world, actor),
                        colony::home_of(world, id).is_some(),
                        weather::today(world),
                    );
                }
            }
        }
        // Entries of the ticks run since the last sample: `[last, now)`. The
        // clock advances at the end of a tick, so a tick that is a multiple of
        // the sample interval belongs to the window it starts (the boat sails
        // at such a tick: reading `(last, now]` dropped every duty entry).
        if world
            .resource::<Journal>()
            .entries
            .front()
            .is_some_and(|entry| entry.tick > self.last_journal_tick && self.last_journal_tick > 0)
        {
            self.journal_overflows += 1;
        }
        for entry in &world.resource::<Journal>().entries {
            if entry.tick < self.last_journal_tick {
                continue;
            }
            if let JournalKind::Effect {
                effect,
                applied: true,
            } = &entry.kind
            {
                let coins = |key: &str| effect.get(key).and_then(Value::as_u64).unwrap_or(0);
                match effect.get("type").and_then(Value::as_str) {
                    Some("VillageBuildingLevy") => {
                        self.building_levies.0 += 1;
                        self.building_levies.1 += coins("raised");
                        *self.fund_flows.entry("building_levy").or_default() +=
                            coins("raised") as i64;
                    }
                    Some("VillageReliefLevy") => {
                        self.relief_levies.0 += 1;
                        self.relief_levies.1 += coins("raised");
                    }
                    Some("VillageCivicLevy") => {
                        let mut book = |source: &'static str, coins: u64, sign: i64| {
                            *self.fund_flows.entry(source).or_default() += sign * coins as i64;
                        };
                        book("levy", coins("contributed"), 1);
                        book("relief_levy", coins("relief_levy"), 1);
                        book("floor_grants", coins("floor_grants"), -1);
                        book("dividend", coins("dividend"), -1);
                    }
                    Some("VillageCivicProject") => {
                        let site = effect.get("site").and_then(Value::as_u64).unwrap_or(0);
                        let building = effect
                            .get("building")
                            .and_then(Value::as_str)
                            .unwrap_or("?")
                            .to_string();
                        match effect.get("decision").and_then(Value::as_str) {
                            Some("commissioned") => {
                                self.sites.insert(site, (building, entry.tick, None, None));
                            }
                            Some("completed") => {
                                if let Some(record) = self.sites.get_mut(&site) {
                                    record.3 = Some(entry.tick);
                                }
                            }
                            _ => {}
                        }
                    }
                    Some("VillageSiteDelivery") => {
                        *self.fund_flows.entry("site_materials").or_default() -=
                            coins("coins") as i64;
                        let site = effect.get("site").and_then(Value::as_u64).unwrap_or(0);
                        if effect.get("supplied").and_then(Value::as_bool) == Some(true)
                            && let Some(record) = self.sites.get_mut(&site)
                        {
                            record.2.get_or_insert(entry.tick);
                        }
                    }
                    Some("VillageBedRented") => {
                        self.inn_nights += 1;
                        let role = entry
                            .unit
                            .and_then(|id| world.resource::<SimIndex>().get(id))
                            .and_then(|guest| world.get::<Villager>(guest))
                            .map_or_else(|| "?".to_string(), |guest| format!("{:?}", guest.role));
                        *self.inn_guests.entry(role).or_default() += 1;
                    }
                    Some("VillageServiceWage") => {
                        *self.fund_flows.entry("service_wages").or_default() -=
                            coins("paid") as i64;
                        self.service_wages.0 += 1;
                        self.service_wages.1 += coins("paid");
                    }
                    Some("VillageCoatGrant") => {
                        *self.fund_flows.entry("coat_grants").or_default() -= coins("coins") as i64;
                        self.coat_grants.0 += 1;
                        self.coat_grants.1 += coins("coins");
                    }
                    Some("VillageBirthDeferred") => {
                        let reason = effect
                            .get("reason")
                            .and_then(Value::as_str)
                            .unwrap_or("?")
                            .to_owned();
                        *self.births_deferred.entry(reason).or_default() += 1;
                    }
                    Some("VillageBoatOrderClosed") => {
                        let (units, delivered) = (coins("units"), coins("delivered"));
                        if delivered >= units {
                            self.boat_orders.0 += 1;
                        } else if delivered > 0 {
                            self.boat_orders.1 += 1;
                        } else {
                            self.boat_orders.2 += 1;
                        }
                    }
                    Some("VillageTrade")
                        if effect.get("kind").and_then(Value::as_str)
                            == Some(catalog::WARM_CLOTHES) =>
                    {
                        self.coats_sold += coins("quantity");
                    }
                    Some("VillageBoatDuty") => {
                        *self.fund_flows.entry("boat_duty").or_default() += coins("coins") as i64;
                        self.duties.0 += 1;
                        self.duties.1 += coins("coins");
                    }
                    _ => {}
                }
            }
        }
        self.last_journal_tick = now;
        let fund = world.resource::<VillageState>().fund;
        self.fund_above_reserve += u32::from(fund > civic::reserve(world));
        if now.is_multiple_of(DAY) {
            self.fund_days.push(fund);
        }
        for need in NEEDS {
            let lowest = all_residents(world)
                .into_iter()
                .filter(|(_, actor)| village::living(world, *actor))
                .map(|(_, actor)| econ::need(world, actor, need))
                .min()
                .unwrap_or(100_000);
            self.lowest_series.entry(need).or_default().push(lowest);
        }
        let floor_percent = village::frontier::economy_spec::tuning(world)
            .map_or(0, |tuning| tuning.price_floor_percent);
        for kind in FLOOR_WATCHED {
            let (Some(base), Some(row)) = (
                catalog::price_in(world, kind),
                world
                    .resource::<market::MarketState>()
                    .commodities
                    .get(kind),
            ) else {
                continue;
            };
            let lowest = (base * floor_percent * 10).max(1000);
            *self.at_floor.entry(kind).or_default() += u32::from(row.reference_milli <= lowest);
        }
        let people = demography::population(world);
        self.least_people = self.least_people.min(people);
        self.most_people = self.most_people.max(people);
        self.samples += 1;
        for kind in STAPLES {
            let percent = price_percent(world, kind);
            if percent > self.worst_price.0 {
                self.worst_price = (percent, kind.into());
            }
            if percent >= 450 {
                *self.at_ceiling.entry(kind).or_default() += 1;
            }
        }
        for child in &world.resource::<demography::Demography>().children {
            if child.unfed_ticks > 0 && self.hungry_children.insert(child.id) {
                let parents: Vec<String> = child
                    .parents
                    .iter()
                    .filter_map(|id| world.resource::<SimIndex>().get(*id))
                    .map(|actor| {
                        let food: u32 = ["bread", catalog::COOKED_MEAL, catalog::BERRIES]
                            .iter()
                            .map(|kind| econ::count(world, actor, kind))
                            .sum();
                        format!(
                            "{}:alive={} coins={} food={food}",
                            world
                                .get::<village::Villager>(actor)
                                .map_or("?", |person| person.role.name()),
                            village::living(world, actor),
                            world.get::<Wallet>(actor).map_or(0, |wallet| wallet.coins),
                        )
                    })
                    .collect();
                println!(
                    "GENERATIONS_HUNGRY_CHILD seed={} tick={now} {} unfed_ticks={} fund={} parents={parents:?}",
                    self.seed,
                    child.name,
                    child.unfed_ticks,
                    world.resource::<VillageState>().fund
                );
            }
        }
    }

    fn row(&mut self, app: &mut App) {
        let world = app.world();
        let now = world.resource::<SimClock>().tick;
        let (day, season, _) = village::calendar(world);
        // Coats ahead of winter: who goes without one at each season's start.
        let people = all_residents(world)
            .into_iter()
            .filter(|(_, actor)| village::living(world, *actor))
            .collect::<Vec<_>>();
        let coatless = people
            .iter()
            .filter(|(_, actor)| {
                !village::living::has_coat(world, *actor)
                    && econ::count(world, *actor, catalog::WARM_CLOTHES) == 0
            })
            .count();
        // The fund's flows of the season by source; `other` is what no
        // journaled flow explains (stall sales of civic stock, imports bought,
        // arrival grants, children fed).
        let fund_now = i64::from(world.resource::<VillageState>().fund);
        let mut flows = std::mem::take(&mut self.fund_flows);
        let tracked: i64 = flows.values().sum();
        flows.insert("other", fund_now - self.fund_at_row - tracked);
        self.fund_at_row = fund_now;
        println!(
            "GENERATIONS_FUND seed={} day={} {season:?} fund={fund_now} flows={flows:?}",
            self.seed,
            day + 1
        );
        println!(
            "GENERATIONS_SEASON seed={} day={} {season:?} residents={} coatless={coatless}              warm_clothes_made={} fund={}",
            self.seed,
            day + 1,
            people.len(),
            world
                .resource::<VillageState>()
                .produced
                .get(catalog::WARM_CLOTHES)
                .copied()
                .unwrap_or(0),
            world.resource::<VillageState>().fund,
        );
        let census = view::census(world);
        let civic = world.resource::<civic::CivicState>();
        let boat = world.resource::<merchant::Merchant>();
        let all_fields = view::fields(world);
        let harvested: u64 = all_fields.iter().map(|field| field.harvested_units).sum();
        let housed = all_residents(world)
            .into_iter()
            .filter(|(id, actor)| {
                village::living(world, *actor) && colony::home_of(world, *id).is_some()
            })
            .count();
        // Island trade so far, units and coins, the boat's included.
        let trade = world
            .resource::<market::MarketState>()
            .commodities
            .values()
            .fold((0_u64, 0_u64), |(units, coins), row| {
                (units + row.total_units, coins + row.total_turnover)
            });
        let deaths: Vec<String> = census
            .deaths
            .iter()
            .map(|(cause, count)| format!("{cause}:{count}"))
            .collect();
        self.rows.push(format!(
            "| {} | {} {season:?} | {} | {} | {} | {housed} | {} | {} | {} | {} | {} | {} | {}/{} | {}/{} | {} | {} | {}% {}% {}% {}% | {}/{} | {} |",
            now / self.year.max(1) + 1,
            day + 1,
            census.residents,
            census.children,
            census.elders,
            census.couples,
            census.births,
            if deaths.is_empty() { "-".into() } else { deaths.join(" ") },
            census.arrivals,
            civic.completed,
            all_fields.len(),
            harvested,
            world
                .resource::<VillageState>()
                .produced
                .get("bread")
                .copied()
                .unwrap_or(0),
            boat.exported.values().sum::<u64>(),
            boat.imported.values().sum::<u64>(),
            boat.purse,
            world.resource::<VillageState>().fund,
            price_percent(world, "bread"),
            price_percent(world, catalog::GRAIN),
            price_percent(world, catalog::WATER),
            price_percent(world, "log"),
            trade.0,
            trade.1,
            boat.coins_paid + boat.coins_earned,
        ));
    }
}

fn soak(seed: u64, years: u64) -> Run {
    soak_on(seed, years, SEASON_DAYS, compressed)
}

/// `season_days` must be the `days_per_season` the edit leaves in the world.
fn soak_on(seed: u64, years: u64, season_days: u64, edit: fn(&mut Value)) -> Run {
    let mut app = world_for(seed, |world| {
        edit(world);
        patched(world);
    });
    let mut run = Run {
        seed,
        coins: money(app.world()),
        least_people: u32::MAX,
        year: DAY * season_days * 4,
        ..Default::default()
    };
    assert_eq!(
        run.coins,
        42 * 200 + u64::from(merchant::purse(app.world())),
        "founders' coins plus the purse"
    );
    let season = DAY * season_days;
    let end = years * 4 * season;
    run.row(&mut app);
    while app.world().resource::<SimClock>().tick < end {
        run_ticks(app.world_mut(), SAMPLE);
        run.sample(&mut app);
        let now = app.world().resource::<SimClock>().tick;
        if now.is_multiple_of(season) {
            run.row(&mut app);
            village::integrity::validate_colony(app.world())
                .unwrap_or_else(|error| panic!("seed {seed} tick {now}: {error}"));
            println!("GENERATIONS seed={seed} {}", run.rows.last().expect("row"));
        }
    }
    let world = app.world();
    // How long civic sites waited for their materials and for their work,
    // in tenths of a day; an open site waits still.
    let end_tick = world.resource::<SimClock>().tick;
    let tenths = |from: u64, to: u64| to.saturating_sub(from) * 10 / DAY;
    for (site, (building, commissioned, supplied, completed)) in &run.sites {
        println!(
            "GENERATIONS_SITE seed={seed} site={site} building={building} day={}              supplied_after_tenths={} built_after_tenths={}",
            commissioned / DAY + 1,
            supplied.map_or_else(
                || format!("open>{}", tenths(*commissioned, end_tick)),
                |at| tenths(*commissioned, at).to_string()
            ),
            completed.map_or_else(
                || format!("open>{}", tenths(*commissioned, end_tick)),
                |at| tenths(*commissioned, at).to_string()
            ),
        );
    }
    let census = view::census(world);
    let civic = world.resource::<civic::CivicState>();
    let boat = world.resource::<merchant::Merchant>();
    let state = world.resource::<weather::WeatherState>();
    println!(
        "GENERATIONS_RESULT seed={seed} years={years} residents={} children={} elders={} \
         couples={} births={} came_of_age={} pairings={} arrivals={} deaths={:?} built={} \
         cancelled={} civic_spent={} fields={} boat_visits={} boat_delays={} exported={:?} \
         imported={:?} coins_paid={} coins_earned={} purse={} fund={} reserve={} \
         transactions={} min_needs={:?} people_min={} people_max={} weather_days={:?} \
         shocks={:?} worst_price={:?} samples={} near_ceiling_samples={:?} social_events={} indoors_samples={} asleep_samples={} asleep_outside={} collapsed_outside={} want_deaths={:?} harsh_winter_deaths={:?}          building_levies={:?} relief_levies={:?} boat_duty={:?} fund_above_reserve_samples={} fund_by_day={:?} \
         boat_duty_paid={} boat_volume_coins={} journal_overflows={} at_floor_samples={:?} \
         need_p5={:?} job_changes={} produced={:?} births_deferred={:?} inn_nights={} inn_guests={:?} service_wages={:?} coat_grants={:?} innkeepers={}          boat_orders={:?} coats_sold={} trade_moves={:?}",
        census.residents,
        census.children,
        census.elders,
        census.couples,
        census.births,
        census.came_of_age,
        census.pairings,
        census.arrivals,
        census.deaths,
        civic.completed,
        civic.cancelled,
        civic.spent,
        view::fields(world).len(),
        boat.visits,
        boat.delays,
        boat.exported,
        boat.imported,
        boat.coins_paid,
        boat.coins_earned,
        boat.purse,
        world.resource::<VillageState>().fund,
        civic::reserve(world),
        world.resource::<VillageState>().transactions,
        run.min_needs,
        run.least_people,
        run.most_people,
        state.days,
        state.shocks,
        run.worst_price,
        run.samples,
        run.at_ceiling,
        world.resource::<colony::ColonyState>().social_events,
        run.indoors,
        run.asleep,
        run.asleep_outside,
        run.collapsed_outside,
        run.failures,
        run.harsh,
        run.building_levies,
        run.relief_levies,
        run.duties,
        run.fund_above_reserve,
        run.fund_days,
        boat.duty_paid,
        boat.coins_paid + boat.coins_earned,
        run.journal_overflows,
        run.at_floor,
        run.lowest_series
            .iter()
            .map(|(need, series)| (*need, percentile_5(series)))
            .collect::<BTreeMap<_, _>>(),
        run.job_changes,
        world.resource::<VillageState>().produced,
        run.births_deferred,
        run.inn_nights,
        run.inn_guests,
        run.service_wages,
        run.coat_grants,
        all_residents(world)
            .into_iter()
            .filter(|(_, actor)| {
                world
                    .get::<village::Villager>(*actor)
                    .is_some_and(|person| person.role == village::Role::Innkeeper)
            })
            .count(),
        run.boat_orders,
        run.coats_sold,
        run.trade_moves,
    );
    run
}

fn verdict(run: &Run) {
    let seed = run.seed;
    assert!(
        run.failures.is_empty(),
        "seed {seed}: deaths of want in a normal season: {:?}",
        run.failures
    );
    assert_eq!(
        run.asleep_outside, 0,
        "seed {seed}: a resident with a bed slept outside its home"
    );
    assert!(
        run.least_people >= 30,
        "seed {seed}: the population fell to {}",
        run.least_people
    );
    // A hard winter may drive a staple to its ceiling for a while; a price
    // that stays there has run away. The bound is a quarter of the run: with
    // every living-village mechanic switched off, logs already stand near the
    // ceiling in 17% of a year (`tests/island_price_probe.rs`).
    for (kind, samples) in &run.at_ceiling {
        assert!(
            *samples * 4 <= run.samples,
            "seed {seed}: the price of {kind} stood at its ceiling in {samples} of {} samples",
            run.samples
        );
    }
}

/// Three in-game years on three seeds. Reports population over time, births,
/// deaths by cause, arrivals, buildings, fields, boat trade, the fund and the
/// lowest needs; fails on a death of want in a normal season, a population
/// collapse or a price that runs to its ceiling.
#[test]
#[ignore = "explicit three-year (864000-tick) soak on seeds 7, 11 and 23; run in release"]
fn the_village_lives_through_three_years_on_three_seeds() {
    let runs: Vec<Run> = [7, 11, 23]
        .into_iter()
        .map(|seed| std::thread::spawn(move || soak(seed, 3)))
        .collect::<Vec<_>>()
        .into_iter()
        .map(|run| run.join().expect("soak thread"))
        .collect();
    for run in &runs {
        println!(
            "\nGENERATIONS_TABLE seed={}\n| year | day season | residents | children | elders | \
             housed | couples | births | deaths | arrivals | built | fields | grain/bread made | \
             boat out/in | purse | fund | bread grain water log price |\n|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|",
            run.seed
        );
        for row in &run.rows {
            println!("{row}");
        }
    }
    for run in &runs {
        verdict(run);
    }
}

/// Three years of the shipped calendar (28 days a year, 2,016,000 ticks) on
/// three seeds. Hours in release; run explicitly.
#[test]
#[ignore = "explicit three-year soak on the shipped 28-day year, seeds 7, 11 and 23; run in release"]
fn the_village_lives_through_three_shipped_years_on_three_seeds() {
    fn shipped(_: &mut Value) {}
    let started = std::time::Instant::now();
    let runs: Vec<Run> = [7, 11, 23]
        .into_iter()
        .map(|seed| std::thread::spawn(move || soak_on(seed, 3, 7, shipped)))
        .collect::<Vec<_>>()
        .into_iter()
        .map(|run| run.join().expect("soak thread"))
        .collect();
    println!("GENERATIONS_WALL seconds={}", started.elapsed().as_secs());
    for run in &runs {
        println!(
            "
GENERATIONS_TABLE seed={}",
            run.seed
        );
        for row in &run.rows {
            println!("{row}");
        }
    }
    for run in &runs {
        verdict(run);
    }
}

/// One year on one seed: the quickest whole-year check while tuning.
#[test]
#[ignore = "explicit three-year (864000-tick) soak on seed 7 alone; run in release"]
fn the_village_lives_through_three_years_on_seed_7() {
    verdict(&soak(7, 3));
}

/// One year of the shipped calendar (28 days) on seeds 7, 11 and 23, in
/// parallel threads.
#[test]
#[ignore = "explicit one-year soak on the shipped 28-day year, seeds 7, 11 and 23; run in release"]
fn the_village_lives_through_one_shipped_year_on_three_seeds() {
    fn shipped(_: &mut Value) {}
    let runs: Vec<Run> = [7, 11, 23]
        .into_iter()
        .map(|seed| std::thread::spawn(move || soak_on(seed, 1, 7, shipped)))
        .collect::<Vec<_>>()
        .into_iter()
        .map(|run| run.join().expect("soak thread"))
        .collect();
    for run in &runs {
        println!("\nGENERATIONS_TABLE seed={}", run.seed);
        for row in &run.rows {
            println!("{row}");
        }
    }
    for run in &runs {
        verdict(run);
    }
}

/// One year of the shipped calendar (28 days) on seed 7.
#[test]
#[ignore = "explicit one-year soak on the shipped 28-day year, seed 7; run in release"]
fn the_village_lives_through_one_shipped_year_on_seed_7() {
    fn shipped(_: &mut Value) {}
    let run = soak_on(7, 1, 7, shipped);
    for row in &run.rows {
        println!("{row}");
    }
    verdict(&run);
}

/// The worst year the data can make: a blight in every growing season and a
/// harsh winter, from the first season on. The village may lose people; it
/// must not collapse, and every death is reported.
#[test]
#[ignore = "explicit one-year soak with a forced blight and harsh winter, seed 7; run in release"]
fn the_village_meets_a_forced_blight_and_harsh_winter() {
    fn forced(world: &mut Value) {
        compressed(world);
        let weather = &mut economy(world)["weather"];
        weather["blight_permille"] = json!(1000);
        weather["harsh_winter_permille"] = json!(1000);
        weather["drought_permille"] = json!(0);
        weather["shock_grace_seasons"] = json!(0);
    }
    let run = soak_on(7, 1, SEASON_DAYS, forced);
    for row in &run.rows {
        println!("{row}");
    }
    println!(
        "FORCED_SHOCKS people_min={} want_deaths={:?} harsh_winter_deaths={:?} min_needs={:?}",
        run.least_people, run.failures, run.harsh, run.min_needs
    );
    assert_eq!(run.asleep_outside, 0);
    assert!(
        run.least_people >= 30,
        "the village collapsed to {}",
        run.least_people
    );
}

#[test]
#[ignore = "explicit one-year (288000-tick) soak on seed 7; run in release"]
fn the_village_lives_through_one_year_on_seed_7() {
    verdict(&soak(7, 1));
}

/// A world with room for `people` residents and that many living in it.
fn crowd(people: usize) -> App {
    let mut app = world_with(|world| {
        for settlement in world["living_world"]["settlements"]
            .as_array_mut()
            .expect("settlements")
        {
            for home in settlement["homes"].as_array_mut().expect("homes") {
                home["capacity"] = json!(4);
            }
            for site in settlement["sites"].as_array_mut().expect("sites") {
                site["capacity"] = json!(4);
            }
        }
        // The headcount is the variable: nobody is born, dies or lands.
        economy(world)["demography"]["enabled"] = json!(false);
    });
    let places = ["harbour", "alderwood", "stone_ridge"].map(|key| settlement(app.world(), key));
    let mut index = 0;
    while all_residents(app.world()).len() < people {
        let place = places[index % 3];
        let id = labor::settle_newcomer(app.world_mut(), place, &format!("Settler {index}"))
            .expect("a free seat");
        let actor = entity(app.world(), id);
        // The founders' endowment, so newcomers live like founders.
        app.world_mut()
            .get_mut::<Wallet>(actor)
            .expect("wallet")
            .coins = 200;
        for kind in ["bread", catalog::WATER, "log"] {
            econ::grant(app.world_mut(), actor, Item::new(kind).stack(2));
        }
        index += 1;
    }
    for place in places {
        civic::house_homeless(app.world_mut(), place);
    }
    app
}

/// Ticks per second at 42, about 80 and about 120 residents. The cost of a
/// tick may grow with the village, but no faster than the village does.
#[test]
#[ignore = "explicit tick-rate measurement at 42, 81 and 120 residents; run in release"]
fn the_tick_rate_scales_no_worse_than_linearly_with_population() {
    const WARMUP: u64 = 1_200;
    const TICKS: u64 = 3_600;
    let mut rates = Vec::new();
    for people in [42, 81, 120] {
        let mut app = crowd(people);
        run_ticks(app.world_mut(), WARMUP);
        let start = std::time::Instant::now();
        run_ticks(app.world_mut(), TICKS);
        let seconds = start.elapsed().as_secs_f64();
        let alive = all_residents(app.world())
            .into_iter()
            .filter(|(_, actor)| village::living(app.world(), *actor))
            .count();
        let rate = TICKS as f64 / seconds;
        let micros = seconds * 1e6 / TICKS as f64;
        println!(
            "SCALE residents={people} alive={alive} ticks={TICKS} seconds={seconds:.2} \
             ticks_per_second={rate:.0} micros_per_tick={micros:.0} \
             micros_per_resident_tick={:.1} fields={} season={:?}",
            micros / people as f64,
            fields::views(app.world()).len(),
            Season::Autumn,
        );
        assert_eq!(alive, people, "somebody died during the measurement");
        rates.push((people as f64, micros));
    }
    let (base_people, base) = rates[0];
    for (people, micros) in &rates[1..] {
        let linear = base * people / base_people;
        assert!(
            *micros <= linear * 1.25,
            "{people} residents cost {micros:.0} us a tick; linear growth from {base_people} \
             residents allows {linear:.0} us (+25%)"
        );
    }
}

/// Where a tick's time goes, by system: the costliest systems at 42 and at
/// 120 residents, in microseconds per tick.
#[test]
#[ignore = "explicit per-system profile at 42 and 120 residents; run in release"]
fn the_cost_of_a_tick_by_system() {
    use npc_framework::perf::{MechanicTags, PerfCollector, install_global};
    const TICKS: u64 = 2_400;
    let collector = PerfCollector::new(TICKS as usize);
    let mut tags = MechanicTags::default_npc();
    testbed_content::perf_tags(&mut tags);
    collector.set_tags(tags);
    assert!(
        install_global(&collector),
        "a global subscriber was already set"
    );
    for people in [42, 120] {
        let mut app = crowd(people);
        for _ in 0..600 {
            run_ticks(app.world_mut(), 1);
            collector.end_frame();
        }
        collector.reset();
        for _ in 0..TICKS {
            run_ticks(app.world_mut(), 1);
            collector.end_frame();
        }
        let report = collector.report_top(14);
        let total: f64 = report.systems.iter().map(|system| system.total_us).sum();
        println!(
            "PROFILE residents={people} top_systems_total_us_per_tick={:.0}",
            total / TICKS as f64
        );
        for system in &report.systems {
            println!(
                "PROFILE residents={people} {:>8.0} us/tick  p95 {:>8.0} us  {} [{}]",
                system.total_us / TICKS as f64,
                system.p95_us,
                system.name,
                system.mechanic
            );
        }
    }
}

/// Ticks per second at about 45 and about 60 residents: the sizes the
/// shipped year reaches.
#[test]
#[ignore = "explicit tick-rate measurement at 45 and 60 residents; run in release"]
fn the_tick_rate_at_45_and_60_residents() {
    const WARMUP: u64 = 1_200;
    const TICKS: u64 = 3_600;
    for people in [45, 60] {
        let mut app = crowd(people);
        run_ticks(app.world_mut(), WARMUP);
        let start = std::time::Instant::now();
        run_ticks(app.world_mut(), TICKS);
        let seconds = start.elapsed().as_secs_f64();
        println!(
            "TICK_RATE residents={people} ticks={TICKS} seconds={seconds:.2} \
             ticks_per_second={:.0} micros_per_tick={:.0}",
            TICKS as f64 / seconds,
            seconds * 1e6 / TICKS as f64
        );
    }
}
