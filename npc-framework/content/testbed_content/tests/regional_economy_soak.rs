//! Unassisted regional survival and circulation on the unchanged simulation clock.
//! The extended audit is explicitly run by release/playtest validation, not silently
//! substituted with a short scenario. Failure artifacts are headless snapshots.
use bevy_app::App;
use bevy_ecs::prelude::*;
use npc_framework::prelude::*;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use testbed_content::village::{
    self, CoinPurse, ResourcePatch, VillageState, Villager, Vitality, Wallet, catalog, colony,
    economy,
    frontier::{
        SettlementMember,
        ecology::{EcologicalPatch, FULL_BIOMASS, HabitatEcology},
        labor::{Employment, ResidentProfile},
        settlements,
    },
};

const SEED: u64 = 7;
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
}

impl Audit {
    fn new(world: &World) -> Self {
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
        }
    }

    fn sample(&mut self, world: &World) -> Result<(), String> {
        let now = world.resource::<SimClock>().tick;
        if world.resource::<SimConfig>().day_length != 24_000 {
            return Err("The audit must keep the ordinary 24000-tick day".into());
        }
        let people = residents(world);
        let current: BTreeSet<_> = people.iter().map(|(id, _)| *id).collect();
        if current != self.identities || current.len() != 42 {
            return Err("Founding resident identities changed; replacement is not survival".into());
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
                }
            }
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
            let minimum: BTreeMap<_, _> = NEEDS.into_iter().map(|need| {
                (need, people.iter().map(|(_, actor)| economy::need(world, *actor, need)).min().unwrap_or(0))
            }).collect();
            json!({"key":settlements::definition(world,id).map(|region|&region.key),"residents":people.len(),
                "housed":people.iter().filter(|(resident,_)|colony::home_of(world,*resident).is_some()).count(),
                "coated":people.iter().filter(|(_,actor)|village::living::has_coat(world,*actor)).count(),
                "minimum_needs":minimum,"roles":roles})
        }).collect();
        json!({"seed":SEED,"tick":world.resource::<SimClock>().tick,"calendar":village::calendar(world),
            "currency":money(world),"residents":self.identities.len(),"regions":regions,
            "transactions":state.transactions,"produced":state.produced,"social_events":world.resource::<colony::ColonyState>().social_events,
            "minimum_needs_so_far":self.minimum_needs,"critical_actor_samples":self.critical_actor_samples,
            "observed_job_changes":self.observed_job_changes,"maximum_observed_blocked_ticks":self.max_blocked.0,
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

fn fail(app: &mut App, audit: &Audit, reason: &str) -> ! {
    let tick = app.world().resource::<SimClock>().tick;
    let detail = diagnostics(app.world(), reason, audit);
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("content crate belongs to the workspace")
        .join("target")
        .join("regional-audit")
        .join(format!("seed{SEED}-pid{}-tick{tick}", std::process::id()));
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

fn run_audit(end: u64) -> (App, Audit) {
    let mut app = testbed_content::new_sim_app(SEED, "living_world");
    let mut audit = Audit::new(app.world());
    assert_eq!(audit.currency, 42 * u64::from(village::NPC_SEED_COINS));
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
    let (mut app, audit) = run_audit(7_200);
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

/// Run explicitly: cargo test --locked -p testbed_content --test regional_economy_soak
/// extended_regional_economy_survives_fourteen_normal_days -- --ignored --nocapture
#[test]
#[ignore = "explicit 336000-tick regional survival, production and ecology release audit"]
fn extended_regional_economy_survives_fourteen_normal_days() {
    let (mut app, audit) = run_audit(END);
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
        if flow.get(catalog::WARM_CLOTHES).copied().unwrap_or(0) == 0 {
            fail(
                &mut app,
                &audit,
                &format!("Warm garment production stopped during {label}"),
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
        json!({"ok":true,"seed":SEED,"ticks":END})
    );
}
