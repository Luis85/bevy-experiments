//! Unassisted regional survival and circulation on the unchanged simulation clock.
//! The extended audit is explicitly run by release/playtest validation, not silently
//! substituted with a short scenario. Failure artifacts are headless snapshots.
use bevy_app::App;
use bevy_ecs::prelude::*;
use island_content::village::{
    self, CoinPurse, ResourcePatch, VillageState, Villager, Vitality, Wallet, catalog, colony,
    economy,
    frontier::{
        SettlementMember,
        ecology::{EcologicalPatch, FULL_BIOMASS, HabitatEcology},
        labor::{Employment, ResidentProfile},
        settlements,
    },
};
use npc_framework::prelude::*;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

const SEED: u64 = 7;
/// Seeds of the acceptance table: every bar holds on each.
const SEEDS: [u64; 3] = [7, 11, 23];
const SAMPLE: u64 = 300;
const CHECKPOINT: u64 = 12_000;
const END: u64 = 336_000;
const WINTER_LATE_START: u64 = 144_000;
const WINTER_END: u64 = 216_000;
const FINAL_LATE_START: u64 = 264_000;
const NEEDS: [&str; 4] = ["Hunger", "Thirst", "Energy", "Warmth"];

fn residents(world: &World) -> Vec<(SimId, Entity)> {
    world
        .resource::<SimIndex>()
        .map
        .iter()
        .filter(|(_, entity)| world.get::<ResidentProfile>(**entity).is_some())
        .map(|(id, entity)| (*id, *entity))
        .collect()
}

fn money(world: &World) -> u64 {
    world
        .resource::<SimIndex>()
        .map
        .values()
        .map(|entity| {
            u64::from(
                world
                    .get::<Wallet>(*entity)
                    .map_or(0, |wallet| wallet.coins),
            ) + u64::from(
                world
                    .get::<CoinPurse>(*entity)
                    .map_or(0, |purse| purse.coins),
            )
        })
        .sum::<u64>()
        + u64::from(world.resource::<VillageState>().fund)
        // The merchant boat's purse is the other half of the closed system.
        + u64::from(village::merchant::purse(world))
        + village::live::escrow(world)
}

/// What the civic planner and the overseer have built, and what the stores hold.
fn civic_report(world: &World) -> Value {
    let civic = world.resource::<village::civic::CivicState>();
    let label = |id: SimId| settlements::definition(world, id).map(|region| region.key.clone());
    let built: Vec<_> = village::civic::buildings(world)
        .into_iter()
        .map(|building| {
            json!({"building":building.building,"settlement":label(building.settlement),
            "completed_at":building.completed_at})
        })
        .collect();
    let open: Vec<_> = village::civic::sites(world)
        .into_iter()
        .map(|site| {
            json!({"building":site.building,"settlement":label(site.settlement),
            "delivered":site.delivered,"cost":site.cost,"work_done":site.work_done,
            "civic":site.civic,"reason":site.reason})
        })
        .collect();
    let stores: Vec<_> = village::civic::depot_views(world)
        .into_iter()
        .map(|store| {
            json!({"label":store.label,"used":store.used,"capacity":store.capacity,
            "sold_units":store.sold_units,"turnover":store.turnover})
        })
        .collect();
    let planners: BTreeMap<_, _> = civic
        .planners
        .iter()
        .map(|(id, planner)| {
            (
                label(*id).unwrap_or_default(),
                json!({"homeless":planner.homeless,"free_beds":planner.free_beds,
                    "surplus_units":planner.surplus_units,"last_decision":planner.last_decision}),
            )
        })
        .collect();
    json!({"completed":civic.completed,"cancelled":civic.cancelled,"spent":civic.spent,
        "held_for_building":village::civic::fund_hold(world),"built":built,"open_sites":open,
        "stores":stores,"planners":planners})
}

fn finished_food(produced: &BTreeMap<String, u64>) -> u64 {
    ["bread", catalog::COOKED_MEAL]
        .into_iter()
        .map(|kind| produced.get(kind).copied().unwrap_or(0))
        .sum()
}

fn delta(before: &BTreeMap<String, u64>, after: &BTreeMap<String, u64>) -> BTreeMap<String, u64> {
    after
        .iter()
        .map(|(kind, count)| {
            (
                kind.clone(),
                count.saturating_sub(before.get(kind).copied().unwrap_or(0)),
            )
        })
        .collect()
}

struct Audit {
    seed: u64,
    /// Residents already reported for a need below 15000.
    traced: BTreeSet<(SimId, &'static str)>,
    /// Per trade: samples of a resident of it, and samples of physical work
    /// (gathering, crafting, hunting, farming, building, restoring).
    work: BTreeMap<String, (u64, u64)>,
    /// Per need, the lowest value among living residents at each sample: the
    /// series behind the 5th percentile of the acceptance table.
    lowest_series: BTreeMap<&'static str, Vec<i32>>,
    identities: BTreeSet<SimId>,
    currency: u64,
    initial_patches: BTreeMap<SimId, u32>,
    previous_ore: BTreeMap<SimId, u32>,
    previous_ecology: BTreeMap<SimId, (u64, u64)>,
    assignments: BTreeMap<SimId, (village::Role, SimId)>,
    observed_job_changes: u64,
    minimum_needs: BTreeMap<&'static str, i32>,
    critical_actor_samples: BTreeMap<&'static str, u64>,
    max_blocked: (u32, SimId, u64),
    winter_before: BTreeMap<String, u64>,
    winter_after: BTreeMap<String, u64>,
    final_before: BTreeMap<String, u64>,
    final_transactions: u64,
    /// Per good: (samples, at the lowest quote the market allows, at or below
    /// half the base price).
    prices: BTreeMap<&'static str, (u64, u64, u64)>,
    /// Per good: samples above the base price, and the summed quote.
    dear: BTreeMap<&'static str, (u64, u64)>,
    /// Per good: summed supply and funded demand of the last market round.
    flows: BTreeMap<&'static str, (u64, u64)>,
}

/// Goods whose quotes the audit watches for oversupply.
const WATCHED: [&str; 7] = [
    "bread",
    "plank",
    catalog::GRAIN,
    catalog::WATER,
    catalog::COOKED_MEAL,
    "log",
    catalog::FLOUR,
];

/// Merges `patch` into `target` (JSON merge patch: objects merge, the rest replaces).
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

/// The shipped world, or (`REGIONAL_ECONOMY_PATCH`, a JSON merge patch of the
/// world's economy block) a variant of it for an explicit A/B audit.
fn audit_app(seed: u64) -> App {
    let economy = std::env::var("REGIONAL_ECONOMY_PATCH").ok();
    // `REGIONAL_AI_PATCH`: a merge patch of the world's `island_ai` policy.
    let ai = std::env::var("REGIONAL_AI_PATCH").ok();
    // `REGIONAL_WORLD_PATCH_FILE`: a merge patch of the whole world document.
    let whole = std::env::var("REGIONAL_WORLD_PATCH_FILE").ok();
    if economy.is_none() && ai.is_none() && whole.is_none() {
        return island_content::new_sim_app(seed, "living_world");
    }
    let mut doc = island_content::scenario::scenario_doc("living_world").expect("scenario");
    if let Some(text) = economy {
        let patch: Value = serde_json::from_str(&text).expect("REGIONAL_ECONOMY_PATCH is JSON");
        println!("REGIONAL_AUDIT_PATCH {patch}");
        merge(&mut doc.world["construction"]["world"]["preset"]["living_world"]["economy"], &patch);
    }
    if let Some(text) = ai {
        let patch: Value = serde_json::from_str(&text).expect("REGIONAL_AI_PATCH is JSON");
        println!("REGIONAL_AUDIT_AI_PATCH {patch}");
        merge(&mut doc.world["island_ai"], &patch);
    }
    if let Some(path) = whole {
        let text = std::fs::read_to_string(&path).expect("world patch file");
        let patch: Value = serde_json::from_str(&text).expect("world patch is JSON");
        println!("REGIONAL_AUDIT_WORLD_PATCH {path}");
        merge(&mut doc.world, &patch);
    }
    island_content::scenario::load_scenario(&doc, seed)
        .unwrap_or_else(|error| panic!("{error}"))
        .app
}

impl Audit {
    fn new(world: &World, seed: u64) -> Self {
        let identities = residents(world).into_iter().map(|(id, _)| id).collect();
        let patches: BTreeMap<_, _> = world
            .resource::<SimIndex>()
            .map
            .iter()
            .filter_map(|(id, entity)| Some((*id, world.get::<ResourcePatch>(*entity)?.remaining)))
            .collect();
        let ore = world
            .resource::<SimIndex>()
            .map
            .iter()
            .filter_map(|(id, entity)| {
                let patch = world.get::<ResourcePatch>(*entity)?;
                (patch.kind == "ore").then_some((*id, patch.remaining))
            })
            .collect();
        Self {
            seed,
            traced: BTreeSet::new(),
            work: BTreeMap::new(),
            lowest_series: NEEDS.into_iter().map(|need| (need, Vec::new())).collect(),
            identities,
            currency: money(world),
            initial_patches: patches,
            previous_ore: ore,
            previous_ecology: BTreeMap::new(),
            assignments: BTreeMap::new(),
            observed_job_changes: 0,
            minimum_needs: NEEDS.into_iter().map(|need| (need, 100_000)).collect(),
            critical_actor_samples: NEEDS.into_iter().map(|need| (need, 0)).collect(),
            max_blocked: (0, SimId(0), 0),
            winter_before: BTreeMap::new(),
            winter_after: BTreeMap::new(),
            final_before: BTreeMap::new(),
            final_transactions: 0,
            prices: WATCHED.into_iter().map(|kind| (kind, (0, 0, 0))).collect(),
            flows: WATCHED.into_iter().map(|kind| (kind, (0, 0))).collect(),
            dear: WATCHED.into_iter().map(|kind| (kind, (0, 0))).collect(),
        }
    }

    fn sample(&mut self, world: &World) -> Result<(), String> {
        let now = world.resource::<SimClock>().tick;
        if world.resource::<SimConfig>().day_length != 24_000 {
            return Err("The audit must keep the ordinary 24000-tick day".into());
        }
        let people = residents(world);
        for need in NEEDS {
            let lowest = people
                .iter()
                .map(|(_, actor)| economy::need(world, *actor, need))
                .min()
                .unwrap_or(100_000);
            self.lowest_series
                .get_mut(need)
                .expect("declared need")
                .push(lowest);
        }
        let current: BTreeSet<_> = people.iter().map(|(id, _)| *id).collect();
        // Settlers may land by boat; no founder may vanish, and (below) nobody
        // at all may die in these fourteen ordinary days.
        if !self.identities.is_subset(&current) || self.identities.len() != 42 {
            return Err(
                "A founding resident identity vanished; replacement is not survival".into(),
            );
        }
        if money(world) != self.currency {
            return Err(format!(
                "Currency drift: expected {}, found {}",
                self.currency,
                money(world)
            ));
        }
        for (id, actor) in people {
            let person = world
                .get::<Villager>(actor)
                .ok_or_else(|| format!("Resident {} lost its life component", id.0))?;
            let vitality = world
                .get::<Vitality>(actor)
                .ok_or_else(|| format!("{} lost vitality", person.name))?;
            if vitality.died_at.is_some() || vitality.health <= 0 {
                return Err(format!(
                    "{} ({}) died or has zero health at tick {now}",
                    person.name, id.0
                ));
            }
            let house_id = colony::home_of(world, id)
                .ok_or_else(|| format!("{} is unhoused at tick {now}", person.name))?;
            let house = world
                .resource::<SimIndex>()
                .get(house_id)
                .ok_or("Assigned household vanished")?;
            let household = world
                .get::<colony::Household>(house)
                .ok_or("Assigned home is not a household")?;
            if person.home != household.door
                || !household.residents.contains(&id)
                || settlements::member(world, actor) != settlements::member(world, house)
            {
                return Err(format!(
                    "{} has inconsistent regional home membership",
                    person.name
                ));
            }
            for need in NEEDS {
                let value = economy::need(world, actor, need);
                let minimum = self.minimum_needs.get_mut(need).expect("declared need");
                *minimum = (*minimum).min(value);
                if value < 15_000 {
                    *self
                        .critical_actor_samples
                        .get_mut(need)
                        .expect("declared need") += 1;
                    // Who fell this low, doing what, where: the first time.
                    if self.traced.insert((id, need)) {
                        println!(
                            "REGIONAL_LOW seed={} tick={now} {} ({}) {need}={value} role={}                              coat={} home_m={} inside={:?} activity={:?} decision={:?} coins={}                              weather={:?} calendar={:?}",
                            self.seed,
                            person.name,
                            id.0,
                            person.role.name(),
                            village::living::has_coat(world, actor),
                            world
                                .get::<SimPos>(actor)
                                .map_or(-1, |here| here.dist2(person.home).isqrt() / 1000),
                            village::view::inside(world, id),
                            village::view::activity_label(world, actor),
                            village::behavior::decision(world, actor),
                            world.get::<Wallet>(actor).map_or(0, |wallet| wallet.coins),
                            village::view::weather(world).today,
                            village::calendar(world),
                        );
                    }
                }
            }
            let doing = village::activity(world, actor).0;
            let tally = self.work.entry(person.role.name().to_owned()).or_default();
            tally.0 += 1;
            tally.1 += u64::from(matches!(
                doing,
                "gathering" | "crafting" | "hunting" | "farming" | "building" | "restoring"
            ));
            let assignment = (person.role, person.workplace);
            if self
                .assignments
                .insert(id, assignment)
                .is_some_and(|old| old != assignment)
            {
                self.observed_job_changes += 1;
            }
            if world.get::<MoveIntent>(actor).is_some()
                && let Some(MoveStatus::Blocked { ticks }) = world.get::<MoveStatus>(actor)
                && *ticks > self.max_blocked.0
            {
                self.max_blocked = (*ticks, id, now);
            }
        }
        if self.max_blocked.0 >= 1200 {
            return Err(format!(
                "Resident {} remained blocked for {} ticks (observed at {})",
                self.max_blocked.1.0, self.max_blocked.0, self.max_blocked.2
            ));
        }
        let patches: Vec<_> = world
            .resource::<SimIndex>()
            .map
            .iter()
            .filter_map(|(id, entity)| {
                world
                    .get::<ResourcePatch>(*entity)
                    .map(|patch| (*id, *entity, patch))
            })
            .collect();
        if patches.len() != self.initial_patches.len() {
            return Err("Resource site identities changed".into());
        }
        for (id, entity, patch) in patches {
            let initial = *self
                .initial_patches
                .get(&id)
                .ok_or("Unexpected resource site")?;
            let ecology = world
                .get::<EcologicalPatch>(entity)
                .ok_or("Resource site lost ecological history")?;
            if patch.remaining > patch.capacity
                || ecology.biomass_milli > FULL_BIOMASS
                || ecology.soil_milli > FULL_BIOMASS
                || ecology.disturbance_milli > FULL_BIOMASS
                || ecology.last_update_tick > now
            {
                return Err(format!(
                    "Resource site {} has invalid stock or ecological state",
                    id.0
                ));
            }
            let available = u64::from(initial)
                .checked_add(ecology.regrown_units)
                .ok_or("Resource history overflow")?;
            if available.checked_sub(ecology.harvested_units) != Some(u64::from(patch.remaining)) {
                return Err(format!(
                    "{} site {} broke stock = initial + regrowth - harvest",
                    patch.kind, id.0
                ));
            }
            let history = (ecology.harvested_units, ecology.regrown_units);
            if self
                .previous_ecology
                .insert(id, history)
                .is_some_and(|old| history.0 < old.0 || history.1 < old.1)
            {
                return Err(format!(
                    "Ecological history moved backwards for site {}",
                    id.0
                ));
            }
            if patch.kind == "ore" {
                let previous = self
                    .previous_ore
                    .insert(id, patch.remaining)
                    .ok_or("Unexpected ore deposit")?;
                if patch.remaining > previous || ecology.regrown_units != 0 {
                    return Err(format!("Finite ore deposit {} regenerated", id.0));
                }
            }
        }
        let floor_percent = village::frontier::economy_spec::tuning(world)
            .map_or(0, |tuning| tuning.price_floor_percent);
        for kind in WATCHED {
            let (Some(base), Some(row)) = (
                catalog::price_in(world, kind),
                world
                    .resource::<village::market::MarketState>()
                    .commodities
                    .get(kind),
            ) else {
                continue;
            };
            let lowest = (base * floor_percent * 10).max(1000);
            let tally = self.prices.get_mut(kind).expect("watched good");
            tally.0 += 1;
            tally.1 += u64::from(row.reference_milli <= lowest);
            tally.2 += u64::from(row.reference_milli * 2 <= base * 1000);
            let dear = self.dear.get_mut(kind).expect("watched good");
            dear.0 += u64::from(row.reference_milli > base * 1000);
            dear.1 += u64::from(row.reference_milli);
            if let Some(last) = row.history.last() {
                let flow = self.flows.get_mut(kind).expect("watched good");
                flow.0 += u64::from(last.supply);
                flow.1 += u64::from(last.funded_demand);
            }
        }
        let state = world.resource::<VillageState>();
        match now {
            WINTER_LATE_START => self.winter_before = state.produced.clone(),
            WINTER_END => self.winter_after = state.produced.clone(),
            FINAL_LATE_START => {
                self.final_before = state.produced.clone();
                self.final_transactions = state.transactions;
            }
            _ => {}
        }
        if now.is_multiple_of(CHECKPOINT) {
            village::integrity::validate_colony(world)?;
        }
        // The mechanism behind a raw good's price: every 2400 ticks, the
        // quote, the last round's supply, funded demand and trades, and the
        // stock on offer against the island's target.
        if now.is_multiple_of(2_400) {
            for kind in ["log", catalog::WATER] {
                let market = world.resource::<village::market::MarketState>();
                let Some(row) = market.commodities.get(kind) else {
                    continue;
                };
                let last = row.history.last();
                let offer: u32 = settlements::all(world)
                    .into_iter()
                    .map(|(id, _)| village::stock::on_offer(world, id, kind))
                    .sum();
                println!(
                    "PRICE_TRACE seed={} tick={now} kind={kind} reference_milli={} supply={}                      demand={} traded={} on_offer={offer} island_target={}",
                    self.seed,
                    row.reference_milli,
                    last.map_or(0, |sample| sample.supply),
                    last.map_or(0, |sample| sample.funded_demand),
                    last.map_or(0, |sample| sample.traded_units),
                    village::stock::island_target(world, kind),
                );
            }
        }
        Ok(())
    }

    fn progress(&self, world: &World) -> Value {
        let state = world.resource::<VillageState>();
        let regions: Vec<_> = settlements::all(world).into_iter().map(|(id, _)| {
            let people: Vec<_> = residents(world).into_iter().filter(|(_, actor)| settlements::member(world, *actor) == Some(id)).collect();
            let mut roles = BTreeMap::new();
            for (_, actor) in &people {
                if let Some(person) = world.get::<Villager>(*actor) { *roles.entry(person.role.name()).or_insert(0_u32) += 1; }
            }
            // Residents may work at another settlement's site, so staffing is
            // also reported by the settlement of the workplace.
            let mut workplaces = BTreeMap::new();
            for (_, actor) in residents(world) {
                if let Some(person) = world.get::<Villager>(actor)
                    && world.resource::<SimIndex>().get(person.workplace)
                        .and_then(|site| settlements::member(world, site)) == Some(id)
                {
                    *workplaces.entry(person.role.name()).or_insert(0_u32) += 1;
                }
            }
            let minimum: BTreeMap<_, _> = NEEDS.into_iter().map(|need| {
                (need, people.iter().map(|(_, actor)| economy::need(world, *actor, need)).min().unwrap_or(0))
            }).collect();
            json!({"key":settlements::definition(world,id).map(|region|&region.key),"residents":people.len(),
                "housed":people.iter().filter(|(resident,_)|colony::home_of(world,*resident).is_some()).count(),
                "coated":people.iter().filter(|(_,actor)|village::living::has_coat(world,*actor)).count(),
                "minimum_needs":minimum,"roles":roles,"workplace_roles":workplaces})
        }).collect();
        json!({"seed":self.seed,"tick":world.resource::<SimClock>().tick,"calendar":village::calendar(world),
            "currency":money(world),"founders":self.identities.len(),"residents":residents(world).len(),
            "census":village::view::census(world),"weather":village::view::weather(world).today,
            "weather_days":world.resource::<village::weather::WeatherState>().days,
            "boat":{"purse":village::merchant::purse(world),"visits":village::view::boat(world).visits,
                "berth":village::view::boat(world).berth,"dock":village::view::boat(world).dock,
                "duty_paid":world.resource::<village::merchant::Merchant>().duty_paid,
                "coins_traded":world.resource::<village::merchant::Merchant>().coins_paid
                    + world.resource::<village::merchant::Merchant>().coins_earned,
                "exported":world.resource::<village::merchant::Merchant>().exported,
                "imported":world.resource::<village::merchant::Merchant>().imported},
            "fields":village::view::fields(world).iter().map(|field| json!({"settlement":field.settlement,
                "harvested":field.harvested_units,"lost_plots":field.lost_plots,"seed":field.seed_stock,
                "stages":field.plots.iter().map(|plot| plot.stage).collect::<Vec<_>>()})).collect::<Vec<_>>(),
            "regions":regions,
            "fund":state.fund,"civic":civic_report(world),
            "transactions":state.transactions,"produced":state.produced,"social_events":world.resource::<colony::ColonyState>().social_events,
            "minimum_needs_so_far":self.minimum_needs,"critical_actor_samples":self.critical_actor_samples,
            "observed_job_changes":self.observed_job_changes,"price_samples":self.prices.iter().map(|(kind,(n,floor,half))|
                (*kind,json!({"samples":n,"at_floor":floor,"at_or_below_half_base":half,
                "mean_supply":self.flows[kind].0/(*n).max(1),"mean_demand":self.flows[kind].1/(*n).max(1),
                "above_base":self.dear[kind].0,"mean_price_milli":self.dear[kind].1/(*n).max(1),
                "price":village::market::unit_price(world,kind)}))).collect::<BTreeMap<_,_>>(),
            "maximum_observed_blocked_ticks":self.max_blocked.0,
            "ore_remaining":self.previous_ore.values().map(|count|u64::from(*count)).sum::<u64>()})
    }
}

fn diagnostics(world: &World, reason: &str, audit: &Audit) -> Value {
    let people: Vec<_> = residents(world).into_iter().map(|(id, actor)| {
        let needs: BTreeMap<_, _> = NEEDS.into_iter().map(|need| (need, economy::need(world, actor, need))).collect();
        let inventory: Vec<_> = economy::free_items(world, actor).into_iter().map(|(item_id, item)|
            json!({"id":item_id,"kind":item.kind,"quantity":item.stack})).collect();
        let plan = world.resource::<village::production::ProductionPlan>();
        json!({"id":id,"person":world.get::<Villager>(actor),"vitality":world.get::<Vitality>(actor),
            "position":world.get::<SimPos>(actor),"needs":needs,"wallet":world.get::<Wallet>(actor),
            "home":colony::home_of(world,id),"heating":colony::home_heating(world,actor),
            "activity":village::activity(world,actor),"decision":village::behavior::decision(world,actor),
            "job":village::production::job_summary(world,actor),"employment":world.get::<Employment>(actor),
            "move_intent":format!("{:?}",world.get::<MoveIntent>(actor)),"move_status":format!("{:?}",world.get::<MoveStatus>(actor)),
            "route_recovery":world.get::<village::routing::RouteRecovery>(actor),"inventory":inventory,
            "procurement":plan.procurement.get(&id),"wanted":plan.wanted.get(&id),"production":plan.production.get(&id),
            "craft":world.get::<npc_framework::crafting::Crafting>(actor),"gather":world.get::<village::Gathering>(actor),
            "hunt":world.get::<village::hunting::Hunting>(actor)})
    }).collect();
    let patches: Vec<_> = world.resource::<SimIndex>().map.iter().filter_map(|(id, entity)| {
        Some(json!({"id":id,"patch":world.get::<ResourcePatch>(*entity)?,"ecology":world.get::<EcologicalPatch>(*entity),
            "region":world.get::<SettlementMember>(*entity),"position":world.get::<SimPos>(*entity)}))
    }).collect();
    let habitats: Vec<_> = world
        .resource::<SimIndex>()
        .map
        .iter()
        .filter_map(|(id, entity)| {
            Some(
                json!({"id":id,"ecology":world.get::<HabitatEcology>(*entity)?,
            "habitat":world.get::<village::hunting::WildlifeHabitat>(*entity)}),
            )
        })
        .collect();
    json!({"reason":reason,"progress":audit.progress(world),"people":people,"patches":patches,"habitats":habitats})
}

/// Units of every good held in the packs of residents who have no use for
/// it (`market::no_use`): stock that never reaches the trade using it.
fn stuck_goods(world: &World, seed: u64) {
    let mut stuck: BTreeMap<&str, u32> = BTreeMap::new();
    for (_, actor) in residents(world) {
        for kind in catalog::trade_kinds(world) {
            let count = village::economy::count(world, actor, kind);
            if count > 0 && village::market::no_use(world, actor, kind) {
                *stuck.entry(kind).or_default() += count;
            }
        }
    }
    println!(
        "STUCK_GOODS seed={seed} total={} {stuck:?}",
        stuck.values().sum::<u32>()
    );
}

fn fail(app: &mut App, audit: &Audit, reason: &str) -> ! {
    stuck_goods(app.world(), audit.seed);
    let tick = app.world().resource::<SimClock>().tick;
    let detail = diagnostics(app.world(), reason, audit);
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("content crate belongs to the workspace")
        .join("target")
        .join("regional-audit")
        .join(format!(
            "seed{}-pid{}-tick{tick}",
            audit.seed,
            std::process::id()
        ));
    let artifacts = (|| -> Result<(), String> {
        std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
        let snapshot = npc_framework::snapshot::save(app.world_mut())?;
        npc_framework::snapshot::write_atomic(&directory.join("failure.world.json"), &snapshot)
            .map_err(|error| error.to_string())?;
        std::fs::write(
            directory.join("diagnostics.json"),
            serde_json::to_vec_pretty(&detail).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        Ok(())
    })();
    eprintln!("REGIONAL_AUDIT_FAILURE {detail}");
    panic!(
        "Regional audit failed at tick {tick}: {reason}. Headless artifacts: {} ({artifacts:?})",
        directory.display()
    );
}

fn run_audit(seed: u64, end: u64) -> (App, Audit) {
    let mut app = audit_app(seed);
    let mut audit = Audit::new(app.world(), seed);
    assert_eq!(
        audit.currency,
        42 * u64::from(village::NPC_SEED_COINS) + u64::from(village::merchant::purse(app.world()))
    );
    if let Err(reason) = audit.sample(app.world()) {
        fail(&mut app, &audit, &reason);
    }
    println!("REGIONAL_AUDIT_BEGIN {}", audit.progress(app.world()));
    while app.world().resource::<SimClock>().tick < end {
        let previous = app.world().resource::<SimClock>().tick;
        let step = SAMPLE.min(end - previous);
        run_ticks(app.world_mut(), step);
        if app.world().resource::<SimClock>().tick != previous + step {
            fail(
                &mut app,
                &audit,
                "The simulation clock did not advance by the requested ordinary ticks",
            );
        }
        if let Err(reason) = audit.sample(app.world()) {
            fail(&mut app, &audit, &reason);
        }
        let now = app.world().resource::<SimClock>().tick;
        if now.is_multiple_of(CHECKPOINT) || now == end {
            println!("REGIONAL_AUDIT_CHECKPOINT {}", audit.progress(app.world()));
        }
    }
    (app, audit)
}

#[test]
fn regional_first_shift_preserves_people_homes_currency_and_starts_physical_work() {
    let (mut app, audit) = run_audit(SEED, 7_200);
    let state = app.world().resource::<VillageState>();
    // The finite initial bread covers this early shift. Requiring new bread
    // here would reward gratuitous overproduction rather than needs-driven work.
    // Continued finished food is required in the extended late-season windows.
    if state.transactions == 0 || !state.produced.values().any(|count| *count > 0) {
        fail(
            &mut app,
            &audit,
            "The first shift completed without physical trade and production",
        );
    }
}

/// Run explicitly: cargo test --locked -p island_content --test regional_economy_soak
/// extended_regional_economy_survives_fourteen_normal_days -- --ignored --nocapture
#[test]
#[ignore = "explicit 336000-tick regional survival, production and ecology release audit"]
fn extended_regional_economy_survives_fourteen_normal_days() {
    println!("REGIONAL_AUDIT_ROW {}", fourteen_days(SEED));
}

/// The acceptance table over seeds 7, 11 and 23 (fourteen days each, in
/// parallel threads): for Hunger, Thirst and Warmth the absolute low and the
/// 5th percentile of the per-sample lowest resident need, job changes and the
/// share of samples at the price floor. Bars: p5 at least 30000 (Hunger,
/// Thirst) and 20000 (Warmth), absolute low at least 15000, at most 150 job
/// changes, bread, grain, plank and log at their floor in under 25% of samples.
#[test]
#[ignore = "explicit 336000-tick audit on seeds 7, 11 and 23; run in release"]
fn fourteen_days_on_three_seeds_meet_the_acceptance_bars() {
    let runs: Vec<(u64, Result<Value, String>)> = SEEDS
        .into_iter()
        .map(|seed| (seed, std::thread::spawn(move || fourteen_days(seed))))
        .collect::<Vec<_>>()
        .into_iter()
        .map(|(seed, run)| {
            (
                seed,
                run.join().map_err(|error| {
                    error
                        .downcast_ref::<String>()
                        .cloned()
                        .or_else(|| error.downcast_ref::<&str>().map(|text| (*text).to_owned()))
                        .unwrap_or_default()
                }),
            )
        })
        .collect();
    println!(
        "\nACCEPTANCE_TABLE\n| seed | Hunger low / p5 | Thirst low / p5 | Warmth low / p5 | Energy low / p5 | job changes | bread / grain / plank / log at floor | verdict |\n|---|---|---|---|---|---|---|---|"
    );
    let mut failures = Vec::new();
    for (seed, run) in &runs {
        let row = match run {
            Ok(row) => row,
            Err(reason) => {
                println!("| {seed} | failed: {reason} | | | | | | no |");
                failures.push(format!("seed {seed} failed its audit"));
                continue;
            }
        };
        let need = |name: &str| {
            (
                row["low"][name].as_i64().unwrap_or(0),
                row["p5"][name].as_i64().unwrap_or(0),
            )
        };
        let floor = |kind: &str| row["at_floor_percent"][kind].as_u64().unwrap_or(100);
        let jobs = row["job_changes"].as_u64().unwrap_or(u64::MAX);
        let mut why = Vec::new();
        for (name, bar) in [("Hunger", 30_000), ("Thirst", 30_000), ("Warmth", 20_000)] {
            let (low, p5) = need(name);
            if p5 < bar {
                why.push(format!("{name} p5 {p5} < {bar}"));
            }
            if low < 15_000 {
                why.push(format!("{name} low {low} < 15000"));
            }
        }
        if jobs > 150 {
            why.push(format!("{jobs} job changes"));
        }
        for kind in ["bread", catalog::GRAIN, "plank", "log"] {
            if floor(kind) >= 25 {
                why.push(format!("{kind} at floor {}%", floor(kind)));
            }
        }
        let cell = |name: &str| {
            let (low, p5) = need(name);
            format!("{low} / {p5}")
        };
        println!(
            "| {seed} | {} | {} | {} | {} | {jobs} | {}% / {}% / {}% / {}% | {} |",
            cell("Hunger"),
            cell("Thirst"),
            cell("Warmth"),
            cell("Energy"),
            floor("bread"),
            floor(catalog::GRAIN),
            floor("plank"),
            floor("log"),
            if why.is_empty() {
                "ok".to_owned()
            } else {
                why.join("; ")
            }
        );
        failures.extend(
            why.into_iter()
                .map(|reason| format!("seed {seed}: {reason}")),
        );
    }
    assert!(failures.is_empty(), "acceptance bars: {failures:?}");
}

/// The fifth percentile of a series (the value below which 5% of it lies).
fn percentile_5(series: &[i32]) -> i32 {
    let mut sorted = series.to_vec();
    sorted.sort_unstable();
    sorted.get(sorted.len() * 5 / 100).copied().unwrap_or(0)
}

/// Fourteen ordinary days on `seed`; panics on a failed invariant and returns
/// the acceptance row.
fn fourteen_days(seed: u64) -> Value {
    let (mut app, audit) = run_audit(seed, END);
    let state = app.world().resource::<VillageState>();
    let winter = delta(&audit.winter_before, &audit.winter_after);
    let final_flow = delta(&audit.final_before, &state.produced);
    let final_trades = state.transactions.saturating_sub(audit.final_transactions);
    println!(
        "REGIONAL_AUDIT_EVIDENCE {}",
        json!({"progress":audit.progress(app.world()),
        "winter_window":[WINTER_LATE_START,WINTER_END],"winter_production":winter,
        "final_window":[FINAL_LATE_START,END],"final_production":final_flow,
        "final_transactions":final_trades,"sample_interval_ticks":SAMPLE})
    );
    for (label, flow) in [("late winter", &winter), ("final three days", &final_flow)] {
        if finished_food(flow) == 0 {
            fail(
                &mut app,
                &audit,
                &format!("Finished food production stopped during {label}"),
            );
        }
        // Garments are made to order. In late winter somebody always needs
        // one; in the last, spring days a coat hardly wears (not at all
        // indoors), so no garment made is a failure only while somebody goes
        // without a coat or has an order for one that nobody has filled.
        let wanted = residents(app.world())
            .into_iter()
            .filter(|(_, actor)| {
                !village::living::has_coat(app.world(), *actor)
                    || village::production::purchase_target(
                        app.world(),
                        *actor,
                        catalog::WARM_CLOTHES,
                    ) > village::economy::count(app.world(), *actor, catalog::WARM_CLOTHES)
            })
            .count();
        if flow.get(catalog::WARM_CLOTHES).copied().unwrap_or(0) == 0
            && (label == "late winter" || wanted > 0)
        {
            fail(
                &mut app,
                &audit,
                &format!(
                    "Warm garment production stopped during {label} ({wanted} residents want one)"
                ),
            );
        }
    }
    if final_trades == 0 {
        fail(
            &mut app,
            &audit,
            "Physical trades stopped during the final three days",
        );
    }
    let regrown = app
        .world()
        .resource::<SimIndex>()
        .map
        .values()
        .filter_map(|entity| app.world().get::<EcologicalPatch>(*entity))
        .map(|state| state.regrown_units)
        .sum::<u64>();
    if regrown == 0 {
        fail(
            &mut app,
            &audit,
            "Renewable ecology never restored any harvested stock",
        );
    }
    println!(
        "REGIONAL_AUDIT_COMPLETE {}",
        json!({"ok":true,"seed":seed,"ticks":END})
    );
    trade_audit(app.world(), seed, &audit);
    acceptance_row(&audit)
}

/// For every trade: residents holding it at the end and the units of its
/// outputs made in the run (an output two trades share counts for both).
fn trade_audit(world: &World, seed: u64, audit: &Audit) {
    let Some(registry) = village::frontier::economy_spec::registry(world) else {
        return;
    };
    let produced = &world.resource::<VillageState>().produced;
    stuck_goods(world, seed);
    let people = residents(world);
    let mut rows = Vec::new();
    for profession in &registry.spec.professions {
        let workers = people
            .iter()
            .filter(|(_, actor)| {
                world
                    .get::<Villager>(*actor)
                    .is_some_and(|person| person.role == profession.role)
            })
            .count();
        let output: u64 = profession
            .outputs
            .iter()
            .map(|good| produced.get(good).copied().unwrap_or(0))
            .sum();
        // Hours of physical work per worker and day: the share of samples
        // spent working, of a 24-hour day.
        let (samples, working) = audit
            .work
            .get(profession.role.name())
            .copied()
            .unwrap_or_default();
        rows.push(json!({"trade":profession.role.name(),"workers":workers,
            "outputs":profession.outputs,"produced":output,
            "work_tenths_of_hours_per_worker_day":working * 240 / samples.max(1)}));
    }
    println!("TRADE_AUDIT seed={seed} {}", Value::Array(rows));
}

fn acceptance_row(audit: &Audit) -> Value {
    let p5: BTreeMap<_, _> = audit
        .lowest_series
        .iter()
        .map(|(need, series)| (*need, percentile_5(series)))
        .collect();
    let at_floor: BTreeMap<_, _> = audit
        .prices
        .iter()
        .map(|(kind, (samples, floor, _))| (*kind, floor * 100 / (*samples).max(1)))
        .collect();
    json!({"seed":audit.seed,"low":audit.minimum_needs,"p5":p5,
        "job_changes":audit.observed_job_changes,"at_floor_percent":at_floor,
        "samples":audit.lowest_series["Hunger"].len()})
}
