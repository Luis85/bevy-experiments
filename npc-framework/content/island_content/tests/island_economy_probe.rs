//! Economy probe of the shipped island (ignored; run in release): bread and
//! berry flows per season, spoilage, imports, staple advances and the work
//! share in the residents' own work block, raw and among residents who had
//! funded work. Prints one `PROBE` block per seed; asserts only the berry
//! bar (berries spoiled no more than eaten), which the S4 economy round set.
//!
//! ```sh
//! PROBE_DAYS=14 CARGO_INCREMENTAL=0 cargo test --locked --release -p island_content \
//!   --test island_economy_probe -- --ignored --nocapture
//! ```
use bevy_ecs::prelude::*;
use island_content::village::{
    self, Villager, catalog,
    ledger::SeasonLedger,
    merchant::Merchant,
    production,
    resident::{self, BlockKind},
};
use npc_framework::prelude::*;
use std::collections::BTreeMap;

/// Per season and settlement: (bread made, flour made, bread eaten, bread spoiled, grain cut).
type Villages = BTreeMap<(String, u32), [u64; 5]>;

const SLICES: usize = 16;
const SAMPLE: u64 = 250;
const BREAD: &str = "bread";
const BERRIES: &str = "village_berries";
const MEAL: &str = "village_cooked_meal";
const FLOUR: &str = "village_flour";

fn env(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|text| text.parse().ok())
        .unwrap_or(default)
}

/// Whether a running leaf's label is work: making, gathering, carrying or
/// building something, or buying its inputs.
fn working(label: &str) -> bool {
    matches!(
        label,
        "building"
            | "buying work materials"
            | "buying a tool"
            | "taking a new job"
            | "finishing the current activity"
            | "crafting at the workplace"
            | "gathering"
            | "fishing"
            | "harvesting the field"
            | "sowing the field"
            | "tending the field"
            | "carrying building materials"
            | "storing surplus goods"
            | "restocking the stall"
            | "carrying goods to the boat"
            | "helping with the hauling"
            | "hunting"
            | "working to a plan"
            | "delivering goods"
    )
}

#[derive(Default)]
struct Work {
    /// Per day slice: (in work block, working, with funded work, funded and working).
    slices: [[u64; 4]; SLICES],
    /// Labels of residents in their work block who were not working.
    idle: BTreeMap<String, (u64, u64)>,
}

fn funded(world: &World, actor: Entity) -> bool {
    let Some(person) = world.get::<Villager>(actor) else {
        return false;
    };
    catalog::outputs_for(world, person.role)
        .iter()
        .any(|kind| production::production_demand(world, actor, kind) > 0)
}

fn sample(world: &mut World, work: &mut Work) {
    let day = world.resource::<SimConfig>().day_length;
    let now = world.resource::<SimClock>().tick;
    let slice = ((now % day) * SLICES as u64 / day) as usize;
    let mut people: Vec<(SimId, Entity)> = world
        .query::<(Entity, &SimId, &Villager)>()
        .iter(world)
        .filter(|(e, _, person)| {
            person.role != village::Role::Visitor && village::living(world, *e)
        })
        .map(|(e, id, _)| (*id, e))
        .collect();
    people.sort_unstable_by_key(|(id, _)| *id);
    for (_, actor) in people {
        if !resident::in_block(world, actor, BlockKind::Work) {
            continue;
        }
        let label = village::view::activity_label(world, actor).unwrap_or("none");
        let busy = working(label);
        let has = funded(world, actor);
        let row = &mut work.slices[slice];
        row[0] += 1;
        row[1] += u64::from(busy);
        row[2] += u64::from(has);
        row[3] += u64::from(has && busy);
        if !busy {
            let entry = work.idle.entry(label.to_owned()).or_default();
            entry.0 += 1;
            entry.1 += u64::from(has);
        }
    }
}

fn settlement_of(world: &World, id: SimId) -> u32 {
    world
        .resource::<SimIndex>()
        .get(id)
        .and_then(|entity| {
            production::economic_settlement(world, entity)
                .or_else(|| village::frontier::settlements::member(world, entity))
        })
        .map_or(0, |settlement| settlement.0)
}

/// Adds the journal's crafts, gathers and spoilage since `from` and the
/// meals of a closed bucket; returns the tick read to.
fn village_flows(world: &mut World, from: u64, villages: &mut Villages) -> u64 {
    let now = world.resource::<SimClock>().tick;
    let season = format!("{:?}", village::calendar(world).1);
    let world: &World = world;
    let journal = world.resource::<Journal>();
    for entry in journal
        .entries
        .iter()
        .filter(|entry| entry.tick >= from && entry.tick < now)
    {
        let Some(unit) = entry.unit else {
            if let JournalKind::Effect {
                effect,
                applied: true,
            } = &entry.kind
            {
                let value = serde_json::to_value(effect).unwrap_or_default();
                if value["type"] == "VillageSpoiled" && value["good"] == BREAD {
                    let holder = SimId(value["holder"].as_u64().unwrap_or(0) as u32);
                    let row = villages
                        .entry((season.clone(), settlement_of(world, holder)))
                        .or_default();
                    row[3] += value["units"].as_u64().unwrap_or(0);
                }
            }
            continue;
        };
        match &entry.kind {
            JournalKind::CraftDone { recipe, .. } => {
                let col = match recipe.as_str() {
                    "village_bake" => 0,
                    "village_mill" => 1,
                    _ => continue,
                };
                let row = villages
                    .entry((season.clone(), settlement_of(world, unit)))
                    .or_default();
                // One batch: four loaves or one flour (the shipped recipes).
                row[col] += if col == 0 { 4 } else { 1 };
            }
            JournalKind::Effect {
                effect,
                applied: true,
            } => {
                let value = serde_json::to_value(effect).unwrap_or_default();
                if value["type"] == "VillageSpoiled" && value["good"] == BREAD {
                    let row = villages
                        .entry((season.clone(), settlement_of(world, unit)))
                        .or_default();
                    row[3] += value["units"].as_u64().unwrap_or(0);
                }
                if value["type"] == "VillageGathered" && value["kind"] == "village_grain" {
                    let row = villages
                        .entry((season.clone(), settlement_of(world, unit)))
                        .or_default();
                    row[4] += 1;
                }
            }
            _ => {}
        }
    }
    // Meals: the bucket that closed at `now` (buckets close every eighth of a day).
    if now.is_multiple_of(village::stock::bucket_ticks(world))
        && let Some(round) = world.resource::<village::stock::FoodEaten>().rounds.last()
    {
        for (settlement, goods) in round {
            let row = villages.entry((season.clone(), settlement.0)).or_default();
            row[2] += u64::from(goods.get(BREAD).copied().unwrap_or(0));
        }
    }
    now
}

/// `PROBE_WATCH=<good>`: four times a day, the quote, each settlement's
/// stock on offer and target, and who holds the good.
fn watch_good(world: &World, seed: u64, kind: &str) {
    let now = world.resource::<SimClock>().tick;
    let quote = world
        .resource::<village::market::MarketState>()
        .commodities
        .get(kind)
        .map_or(0, |row| row.reference_milli);
    let mut line = format!("WATCH seed={seed} tick={now} {kind} quote_milli={quote}");
    for (settlement, _) in village::frontier::settlements::all(world) {
        line.push_str(&format!(
            " | s{} offer {} target {} use {}",
            settlement.0,
            village::stock::on_offer(world, settlement, kind),
            village::stock::target(world, settlement, kind),
            village::stock::daily_use(world, settlement, kind),
        ));
    }
    let mut holders = Vec::new();
    for (id, entity) in &world.resource::<SimIndex>().map {
        let held = village::economy::count(world, *entity, kind);
        let stored = world
            .get::<village::civic::Depot>(*entity)
            .map_or(0, |depot| depot.stock(kind));
        if held + stored > 0 {
            let who = world
                .get::<Villager>(*entity)
                .map_or("store".to_owned(), |person| person.role.key().to_owned());
            holders.push(format!("{}:{who}={}", id.0, held + stored));
        }
    }
    holders.sort();
    let plan = world.resource::<production::ProductionPlan>();
    let planned: Vec<String> = plan
        .production
        .iter()
        .filter_map(|(id, goods)| {
            let units = goods.get(kind).copied().filter(|units| *units > 0)?;
            Some(format!("{}={units}", id.0))
        })
        .collect();
    let orders: Vec<String> = village::frontier::settlements::all(world)
        .into_iter()
        .map(|(settlement, _)| {
            format!(
                "s{}:{}",
                settlement.0,
                village::civic::material_orders(world, settlement)
                    .get(kind)
                    .copied()
                    .unwrap_or(0)
            )
        })
        .collect();
    // Who wants the good and who makes it.
    let mut wanters = Vec::new();
    let mut makers = Vec::new();
    let mut people: Vec<(SimId, Entity)> = world
        .resource::<SimIndex>()
        .map
        .iter()
        .map(|(id, entity)| (*id, *entity))
        .collect();
    people.sort_unstable_by_key(|(id, _)| *id);
    for (id, actor) in people {
        let Some(person) = world.get::<Villager>(actor) else {
            continue;
        };
        if !village::living(world, actor) || person.role == village::Role::Visitor {
            continue;
        }
        let settlement = village::frontier::settlements::member(world, actor).map_or(0, |s| s.0);
        let count = village::economy::count(world, actor, kind);
        let wanted = production::purchase_target(world, actor, kind);
        let coatless = kind == catalog::WARM_CLOTHES && !village::living::has_coat(world, actor);
        if wanted > count || coatless {
            wanters.push(format!(
                "{}:{}@s{settlement}/plan s{} coins {} wants {wanted} has {count}{}",
                id.0,
                person.role.key(),
                production::economic_settlement(world, actor).map_or(0, |s| s.0),
                world.get::<village::Wallet>(actor).map_or(0, |w| w.coins),
                if coatless { " coatless" } else { "" }
            ));
        }
        if catalog::outputs_for(world, person.role).contains(&kind) {
            makers.push(format!(
                "{}:{}@s{settlement}/plan s{} demand {} sells {} coins {} doing {}",
                id.0,
                person.role.key(),
                production::economic_settlement(world, actor).map_or(0, |s| s.0),
                production::production_demand(world, actor, kind),
                village::market::available_to_sell(world, actor, kind),
                world.get::<village::Wallet>(actor).map_or(0, |w| w.coins),
                village::view::activity_label(world, actor).unwrap_or("none"),
            ));
        }
    }
    println!(
        "{line} | holders {} | planned {} | site orders {} | wanters {} | makers {}",
        holders.join(" "),
        planned.join(" "),
        orders.join(" "),
        wanters.join("; "),
        makers.join("; ")
    );
}

/// Building sites and births held for a bed, from the journal and a sample
/// of the open sites (S4 housing round).
#[derive(Default)]
struct Housing {
    /// Completed and cancelled sites, by building kind.
    completed: BTreeMap<String, u64>,
    cancelled: BTreeMap<String, u64>,
    /// `VillageBirthDeferred` by reason `no_bed` / `no_home_affordable`.
    no_bed: u64,
    no_home: u64,
    /// Ticks each site stood open short of materials.
    waiting: BTreeMap<u32, u64>,
    /// Units delivered to sites, and those carried by a resident of another settlement.
    delivered: BTreeMap<String, u64>,
    across: BTreeMap<String, u64>,
    read_to: u64,
}

fn site_settlement(world: &World, site: u32) -> Option<SimId> {
    let entity = world.resource::<SimIndex>().get(SimId(site))?;
    world
        .get::<village::civic::ConstructionSite>(entity)
        .map(|site| site.settlement)
        .or_else(|| {
            world
                .get::<village::civic::Building>(entity)
                .map(|building| building.settlement)
        })
}

fn housing_flows(world: &mut World, housing: &mut Housing) {
    let now = world.resource::<SimClock>().tick;
    let from = housing.read_to;
    housing.read_to = now;
    let world: &World = world;
    for site in village::civic::sites(world) {
        if site
            .cost
            .iter()
            .any(|(kind, units)| site.delivered.get(kind).copied().unwrap_or(0) < *units)
        {
            *housing.waiting.entry(site.id.0).or_default() += SAMPLE;
        }
    }
    let journal = world.resource::<Journal>();
    for entry in journal
        .entries
        .iter()
        .filter(|entry| entry.tick >= from && entry.tick < now)
    {
        let JournalKind::Effect {
            effect,
            applied: true,
        } = &entry.kind
        else {
            continue;
        };
        let value = serde_json::to_value(effect).unwrap_or_default();
        match value["type"].as_str() {
            Some("VillageBuildingCompleted") => {
                *housing
                    .completed
                    .entry(value["kind"].as_str().unwrap_or("?").to_owned())
                    .or_default() += 1;
            }
            Some("VillageSiteCancelled") => {
                let kind =
                    village::civic::definition(world, value["building"].as_str().unwrap_or(""))
                        .map_or("prop", |def| def.kind.name());
                *housing.cancelled.entry(kind.to_owned()).or_default() += 1;
            }
            Some("VillageBirthDeferred") => match value["reason"].as_str() {
                Some("no_bed") => housing.no_bed += 1,
                Some("no_home_affordable") => housing.no_home += 1,
                _ => {}
            },
            Some("VillageSiteDelivery") => {
                let kind = value["kind"].as_str().unwrap_or("?").to_owned();
                let units = value["quantity"].as_u64().unwrap_or(0);
                *housing.delivered.entry(kind.clone()).or_default() += units;
                let site = value["site"].as_u64().unwrap_or(0) as u32;
                let hauler = entry
                    .unit
                    .and_then(|id| world.resource::<SimIndex>().get(id))
                    .and_then(|actor| village::civic::home_settlement(world, actor));
                if hauler.is_some() && hauler != site_settlement(world, site) {
                    *housing.across.entry(kind).or_default() += units;
                }
            }
            _ => {}
        }
    }
}

fn get(map: &BTreeMap<String, u64>, key: &str) -> u64 {
    map.get(key).copied().unwrap_or(0)
}

fn probe(seed: u64, days: u64) -> (String, u64, u64) {
    let mut app = island_content::new_sim_app(seed, "living_world");
    let day = app.world().resource::<SimConfig>().day_length;
    let mut work = Work::default();
    let end = days * day;
    let watch = std::env::var("PROBE_WATCH").ok();
    // `PROBE_VILLAGES=1`: bread and flour made, eaten and spoiled per
    // settlement and season, read from the journal (it never feeds the sim).
    let villages_on = std::env::var("PROBE_VILLAGES").is_ok();
    let mut villages = Villages::new();
    if villages_on {
        let world = app.world_mut();
        npc_framework::kernel::journal::set_journal(world, true, false);
        world.resource_mut::<Journal>().capacity = 400_000;
    }
    let mut read_to = 0_u64;
    // The housing counters read the journal (it never feeds the sim).
    let mut housing = Housing::default();
    {
        let world = app.world_mut();
        npc_framework::kernel::journal::set_journal(world, true, false);
        let mut journal = world.resource_mut::<Journal>();
        journal.capacity = journal.capacity.max(400_000);
    }
    while app.world().resource::<SimClock>().tick < end {
        run_ticks(app.world_mut(), SAMPLE);
        sample(app.world_mut(), &mut work);
        housing_flows(app.world_mut(), &mut housing);
        if villages_on {
            read_to = village_flows(app.world_mut(), read_to, &mut villages);
        }
        let now = app.world().resource::<SimClock>().tick;
        if let Some(kind) = watch.as_deref()
            && now.is_multiple_of(day / 4)
        {
            watch_good(app.world(), seed, kind);
        }
    }
    let world = app.world();
    let mut out = String::new();
    let ledger = world.resource::<SeasonLedger>();
    let mut used = BTreeMap::<String, u64>::new();
    let mut spoiled = BTreeMap::<String, u64>::new();
    let now_totals = village::ledger::totals(world);
    for tally in &ledger.seasons {
        let end = tally.end.clone().unwrap_or_else(|| now_totals.clone());
        let d = |map_end: &BTreeMap<String, u64>, map_start: &BTreeMap<String, u64>, k: &str| {
            get(map_end, k).saturating_sub(get(map_start, k))
        };
        let su = |k: &str| tally.used.get(k).copied().unwrap_or(0);
        let ss = |k: &str| tally.spoiled.get(k).copied().unwrap_or(0);
        out.push_str(&format!(
            "PROBE seed={seed} season={} {}: bread made {} eaten {} spoiled {} imported {} (+flour {}); \
             berries gathered {} eaten {} spoiled {} exported {}; meals made {} eaten {} spoiled {}; \
             grain {} flour {}\n",
            tally.season,
            tally.name,
            d(&end.produced, &tally.start.produced, BREAD),
            su(BREAD),
            ss(BREAD),
            d(&end.imported, &tally.start.imported, BREAD),
            d(&end.imported, &tally.start.imported, FLOUR),
            d(&end.produced, &tally.start.produced, BERRIES),
            su(BERRIES),
            ss(BERRIES),
            d(&end.exported, &tally.start.exported, BERRIES),
            d(&end.produced, &tally.start.produced, MEAL),
            su(MEAL),
            ss(MEAL),
            d(&end.produced, &tally.start.produced, "village_grain"),
            d(&end.produced, &tally.start.produced, FLOUR),
        ));
        for (k, v) in &tally.used {
            *used.entry(k.clone()).or_default() += v;
        }
        for (k, v) in &tally.spoiled {
            *spoiled.entry(k.clone()).or_default() += v;
        }
    }
    for ((season, settlement), [bread, flour, eaten, spoiled, grain]) in &villages {
        out.push_str(&format!(
            "PROBE seed={seed} village s{settlement} {season}: bread made {bread} eaten {eaten} spoiled {spoiled} \
             (eaten+spoiled {}); flour made {flour}; grain cut {grain}
",
            eaten + spoiled
        ));
    }
    let merchant = world.resource::<Merchant>();
    let state = world.resource::<village::VillageState>();
    out.push_str(&format!(
        "PROBE seed={seed} total: bread made {} eaten {} spoiled {} imported {} flour imported {}; \
         berries gathered {} eaten {} spoiled {} exported {}; advances {} repaid {}\n",
        get(&state.produced, BREAD),
        get(&used, BREAD),
        get(&spoiled, BREAD),
        get(&merchant.imported, BREAD),
        get(&merchant.imported, FLOUR),
        get(&state.produced, BERRIES),
        get(&used, BERRIES),
        get(&spoiled, BERRIES),
        get(&merchant.exported, BERRIES),
        world
            .get_resource::<production::StapleAdvances>()
            .map_or(0, |a| a.advanced),
        world
            .get_resource::<production::StapleAdvances>()
            .map_or(0, |a| a.repaid),
    ));
    // Year totals of chosen goods (`PROBE_GOODS`, comma separated) from the
    // ledger, and the days some village was short of food by season name.
    let goods = std::env::var("PROBE_GOODS")
        .unwrap_or_else(|_| "bread,village_turnip,village_egg,village_grain".into());
    for kind in goods
        .split(',')
        .map(str::trim)
        .filter(|kind| !kind.is_empty())
    {
        out.push_str(&format!(
            "PROBE_GOOD seed={seed} {kind}: made {} eaten_or_used {} spoiled {} exported {} imported {}\n",
            get(&state.produced, kind),
            get(&used, kind),
            get(&spoiled, kind),
            get(&merchant.exported, kind),
            get(&merchant.imported, kind),
        ));
    }
    let mut short = BTreeMap::<String, u32>::new();
    for tally in &ledger.seasons {
        for (what, days) in &tally.short_days {
            *short.entry(format!("{}:{what}", tally.name)).or_default() += days;
        }
    }
    let census = village::view::census(world);
    out.push_str(&format!(
        "PROBE_LIFE seed={seed} residents={} children={} deaths={:?} short_days={short:?}\n",
        census.residents, census.children, census.deaths,
    ));
    let civic = world.resource::<village::civic::CivicState>();
    let longest = housing
        .waiting
        .iter()
        .max_by_key(|(id, ticks)| (**ticks, std::cmp::Reverse(**id)));
    let open: Vec<String> = village::civic::sites(world)
        .into_iter()
        .map(|site| {
            let short: Vec<String> = site
                .cost
                .iter()
                .filter_map(|(kind, units)| {
                    let have = site.delivered.get(kind).copied().unwrap_or(0);
                    (have < *units).then(|| format!("{kind} {have}/{units}"))
                })
                .collect();
            format!(
                "{}@s{} [{}]",
                site.building,
                site.settlement.0,
                short.join(" ")
            )
        })
        .collect();
    out.push_str(&format!(
        "PROBE seed={seed} housing: completed {} cancelled {} (by kind: completed {:?} cancelled {:?}); \
         births held no_bed {} no_home_affordable {}; site wait days total {} longest {}; \
         delivered {:?} across villages {:?}; open {}\n",
        civic.completed,
        civic.cancelled,
        housing.completed,
        housing.cancelled,
        housing.no_bed,
        housing.no_home,
        housing.waiting.values().sum::<u64>() / day,
        longest.map_or("-".to_owned(), |(id, ticks)| format!("{} (site {id})", ticks / day)),
        housing.delivered,
        housing.across,
        open.join("; "),
    ));
    let pct = |a: u64, b: u64| (a * 100).checked_div(b).unwrap_or(0);
    let raw: Vec<String> = work
        .slices
        .iter()
        .map(|row| {
            if row[0] == 0 {
                "  -".into()
            } else {
                format!("{:3}", pct(row[1], row[0]))
            }
        })
        .collect();
    let funded: Vec<String> = work
        .slices
        .iter()
        .map(|row| {
            if row[2] == 0 {
                "  -".into()
            } else {
                format!("{:3}", pct(row[3], row[2]))
            }
        })
        .collect();
    let sum = |i: usize, range: std::ops::Range<usize>| -> u64 {
        work.slices[range].iter().map(|row| row[i]).sum()
    };
    out.push_str(&format!(
        "PROBE seed={seed} work share by slice (raw)    {} total={} afternoon(5-9)={}\n",
        raw.join(" "),
        pct(sum(1, 0..SLICES), sum(0, 0..SLICES)),
        pct(sum(1, 5..10), sum(0, 5..10)),
    ));
    out.push_str(&format!(
        "PROBE seed={seed} work share by slice (funded) {} total={} afternoon(5-9)={} funded_of_block={}\n",
        funded.join(" "),
        pct(sum(3, 0..SLICES), sum(2, 0..SLICES)),
        pct(sum(3, 5..10), sum(2, 5..10)),
        pct(sum(2, 0..SLICES), sum(0, 0..SLICES)),
    ));
    let block = sum(0, 0..SLICES).max(1);
    let mut idle: Vec<_> = work.idle.iter().collect();
    idle.sort_by(|a, b| b.1.0.cmp(&a.1.0).then_with(|| a.0.cmp(b.0)));
    let idle: Vec<String> = idle
        .into_iter()
        .take(8)
        .map(|(label, (n, f))| {
            format!("{label} {}% ({}% funded)", n * 100 / block, f * 100 / block)
        })
        .collect();
    out.push_str(&format!(
        "PROBE seed={seed} not working in the work block: {}\n",
        idle.join(", ")
    ));
    (out, get(&used, BERRIES), get(&spoiled, BERRIES))
}

#[test]
#[ignore = "economy probe over PROBE_DAYS (default 14) on PROBE_SEEDS (default 7,11,23); run in release"]
fn economy_probe_berries_spoil_no_more_than_are_eaten() {
    let days = env("PROBE_DAYS", 14);
    let seeds: Vec<u64> = std::env::var("PROBE_SEEDS")
        .ok()
        .map(|text| {
            text.split(',')
                .filter_map(|s| s.trim().parse().ok())
                .collect()
        })
        .unwrap_or_else(|| vec![7, 11, 23]);
    let runs: Vec<_> = seeds
        .into_iter()
        .map(|seed| std::thread::spawn(move || probe(seed, days)))
        .collect();
    let mut failures = Vec::new();
    for run in runs {
        let (text, eaten, spoiled) = run.join().expect("probe thread");
        print!("{text}");
        if spoiled > eaten {
            failures.push(format!("berries spoiled {spoiled} > eaten {eaten}"));
        }
    }
    assert!(failures.is_empty(), "{failures:?}");
}
