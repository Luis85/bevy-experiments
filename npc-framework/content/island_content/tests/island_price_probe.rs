//! Price probe for the coastal world: how long each staple stands near its
//! price ceiling over one twelve-day year, with the living-village mechanics
//! on and with them switched off in data. It separates what weather, the boat,
//! fields and demography do to prices from what the economy did before them.
//! Explicit; run in release:
//!
//! ```text
//! cargo test --locked --release -p island_content --test island_price_probe -- --ignored --nocapture
//! ```
mod life_support;

use bevy_ecs::prelude::*;
use island_content::village::{self, catalog, market, view};
use life_support::*;
use npc_framework::prelude::*;
use serde_json::{Value, json};
use std::collections::BTreeMap;

const DAY: u64 = 24_000;
const SAMPLE: u64 = 600;
const STAPLES: [&str; 5] = ["bread", catalog::WATER, "log", catalog::GRAIN, "plank"];

fn short_year(world: &mut Value) {
    let economy = economy(world);
    economy["tuning"]["days_per_season"] = json!(3);
    economy["tuning"]["winter_fuel_lead_days"] = json!(1);
}

/// The living-village mechanics off: no weather, boat, births, deaths or
/// arrivals, no founding fields, and wild grain that regrows as it used to.
fn before(world: &mut Value) {
    short_year(world);
    let economy = economy(world);
    economy["weather"]["enabled"] = json!(false);
    economy["merchant"]["enabled"] = json!(false);
    economy["demography"]["enabled"] = json!(false);
    economy["tuning"]["civic_reserve_grant_days"] = json!(0);
    economy["tuning"]["planner_crop_shortage_units"] = json!(100_000);
    for def in economy["buildings"].as_array_mut().expect("buildings") {
        if def.get("field").is_some() {
            def["field"]["founding"] = json!(false);
        }
    }
    for patch in economy["patches"].as_array_mut().expect("patches") {
        if patch["good"] == catalog::GRAIN {
            *patch = json!({"good": catalog::GRAIN, "dx": -30000, "dz": 10000,
                "capacity": 160, "gather_ticks": 180, "finite": false, "regrowth": 4,
                "winter_regrowth": 1, "harvest_loss": 40, "soil_loss": 30});
        }
    }
}

fn percent(world: &World, kind: &str) -> u32 {
    let base = catalog::price_in(world, kind).unwrap_or(1).max(1);
    market::unit_price(world, kind).unwrap_or(base) * 100 / base
}

fn probe(label: &'static str, seed: u64, days: u64, edit: fn(&mut Value)) {
    let mut app = world_for(seed, edit);
    let mut high: BTreeMap<&str, u32> = BTreeMap::new();
    let mut low: BTreeMap<&str, u32> = BTreeMap::new();
    let mut samples = 0_u32;
    while app.world().resource::<SimClock>().tick < days * DAY {
        run_ticks(app.world_mut(), SAMPLE);
        samples += 1;
        for kind in STAPLES {
            let now = percent(app.world(), kind);
            *high.entry(kind).or_default() += u32::from(now >= 450);
            *low.entry(kind).or_default() += u32::from(now <= 50);
        }
        let tick = app.world().resource::<SimClock>().tick;
        if tick.is_multiple_of(3 * DAY) {
            let prices: Vec<String> = STAPLES
                .iter()
                .map(|kind| format!("{kind}={}%", percent(app.world(), kind)))
                .collect();
            println!(
                "PRICE_PROBE {label} seed={seed} day={} season={:?} residents={} {}",
                village::calendar(app.world()).0 + 1,
                village::calendar(app.world()).1,
                view::census(app.world()).residents,
                prices.join(" ")
            );
        }
    }
    println!(
        "PRICE_PROBE_RESULT {label} seed={seed} days={days} samples={samples} \
         near_ceiling={high:?} near_floor={low:?}"
    );
}

fn no_weather(world: &mut Value) {
    short_year(world);
    economy(world)["weather"]["enabled"] = json!(false);
}

fn no_merchant(world: &mut Value) {
    short_year(world);
    economy(world)["merchant"]["enabled"] = json!(false);
}

fn no_demography(world: &mut Value) {
    short_year(world);
    economy(world)["demography"]["enabled"] = json!(false);
}

fn no_fields(world: &mut Value) {
    short_year(world);
    for def in economy(world)["buildings"]
        .as_array_mut()
        .expect("buildings")
    {
        if def.get("field").is_some() {
            def["field"]["founding"] = json!(false);
        }
    }
}

#[test]
#[ignore = "explicit one-year price probe with and without the living-village mechanics"]
fn staple_prices_with_and_without_the_living_village() {
    type Variant = (&'static str, fn(&mut Value));
    let variants: [Variant; 6] = [
        ("living", short_year),
        ("before", before),
        ("no_weather", no_weather),
        ("no_merchant", no_merchant),
        ("no_demography", no_demography),
        ("no_fields", no_fields),
    ];
    let runs: Vec<_> = variants
        .into_iter()
        .map(|(label, edit)| std::thread::spawn(move || probe(label, 7, 12, edit)))
        .collect();
    for run in runs {
        run.join().expect("probe");
    }
}

/// Who wants water, who has it and what the well keepers are doing, twice a
/// day through one year: evidence for a water price that stays high.
#[test]
#[ignore = "explicit one-year water market diagnostic"]
fn the_water_market_through_one_year() {
    use island_content::village::economy::count as econ_count;
    use island_content::village::{ResourcePatch, Role, Villager, production};
    let mut app = world_for(7, short_year);
    let half = DAY / 2;
    let mut rounds = 0_u32;
    let (mut price, mut supply, mut demand, mut low, mut traded) =
        (0_u64, 0_u64, 0_u64, 0_u64, 0_u64);
    let mut doing: BTreeMap<String, u32> = BTreeMap::new();
    let mut wanting: BTreeMap<String, u32> = BTreeMap::new();
    while app.world().resource::<SimClock>().tick < 12 * DAY {
        run_ticks(app.world_mut(), 300);
        let world = app.world();
        let now = world.resource::<SimClock>().tick;
        if let Some(sample) = world
            .resource::<market::MarketState>()
            .commodities
            .get(catalog::WATER)
            .and_then(|row| row.history.last())
        {
            rounds += 1;
            price += u64::from(sample.unit_price);
            supply += u64::from(sample.supply);
            demand += u64::from(sample.funded_demand);
            low += u64::from(sample.below_reference_bid_units);
            traded += sample.traded_units;
        }
        let mut keepers: BTreeMap<u32, (u32, u32)> = BTreeMap::new();
        for (_, actor) in all_residents(world) {
            let Some(villager) = world.get::<Villager>(actor) else {
                continue;
            };
            let place = village::frontier::settlements::member(world, actor).map_or(0, |id| id.0);
            let held = econ_count(world, actor, catalog::WATER);
            if villager.role == Role::WaterKeeper {
                let entry = keepers.entry(place).or_default();
                entry.0 += 1;
                entry.1 += held;
                *doing
                    .entry(view::activity_label(world, actor).unwrap_or("-").to_owned())
                    .or_default() += 1;
            }
            let target = production::purchase_target(world, actor, catalog::WATER);
            if target > held {
                *wanting
                    .entry(format!("{}@{place}", villager.role.name()))
                    .or_default() += target - held;
            }
        }
        if now.is_multiple_of(half) {
            let mut wells = Vec::new();
            for entity in world.resource::<SimIndex>().map.values() {
                if let Some(patch) = world.get::<ResourcePatch>(*entity)
                    && patch.kind == catalog::WATER
                {
                    wells.push(patch.remaining);
                }
            }
            let (day, season, _) = village::calendar(world);
            let n = u64::from(rounds.max(1));
            let mut want: Vec<_> = wanting.iter().collect();
            want.sort_by_key(|(_, units)| std::cmp::Reverse(**units));
            want.truncate(8);
            println!(
                "WATER day={} {season:?} weather={:?} price={} supply={} demand={} below={} traded={} \
                 keepers(place:(n,held))={keepers:?} wells={wells:?} doing={doing:?} wanting={want:?}",
                day + 1,
                view::weather(world).today,
                price * 100 / n,
                supply * 100 / n,
                demand * 100 / n,
                low * 100 / n,
                traded,
            );
            rounds = 0;
            (price, supply, demand, low, traded) = (0, 0, 0, 0, 0);
            doing.clear();
            wanting.clear();
        }
    }
}

/// Why bread, planks and grain sit at the price floor: supply on offer,
/// funded demand and units traded per market round, day by day.
#[test]
#[ignore = "explicit six-day supply and demand diagnostic for the goods at the price floor"]
fn the_goods_at_the_price_floor_through_six_days() {
    let mut app = world_for(7, |_| {});
    let goods = ["bread", "plank", catalog::GRAIN, "log"];
    let mut sums: BTreeMap<&str, [u64; 5]> = BTreeMap::new();
    while app.world().resource::<SimClock>().tick < 6 * DAY {
        run_ticks(app.world_mut(), 300);
        let world = app.world();
        let now = world.resource::<SimClock>().tick;
        for kind in goods {
            if let Some(sample) = world
                .resource::<market::MarketState>()
                .commodities
                .get(kind)
                .and_then(|row| row.history.last())
            {
                let row = sums.entry(kind).or_default();
                row[0] += 1;
                row[1] += u64::from(sample.unit_price);
                row[2] += u64::from(sample.supply);
                row[3] += u64::from(sample.funded_demand);
                row[4] += sample.traded_units;
            }
        }
        if now.is_multiple_of(DAY) {
            let makers = |kind: &str| view::producers(world, kind).len();
            println!(
                "FLOOR day={} makers bread={} plank={} grain={} log={}",
                now / DAY,
                makers("bread"),
                makers("plank"),
                makers(catalog::GRAIN),
                makers("log"),
            );
            for kind in goods {
                let row = sums.get(kind).copied().unwrap_or_default();
                let n = row[0].max(1);
                println!(
                    "FLOOR day={} {kind} base={} price_x100={} supply_x100={} demand_x100={} traded={}",
                    now / DAY,
                    catalog::price_in(world, kind).unwrap_or(0),
                    row[1] * 100 / n,
                    row[2] * 100 / n,
                    row[3] * 100 / n,
                    row[4],
                );
            }
            sums.clear();
        }
    }
}
