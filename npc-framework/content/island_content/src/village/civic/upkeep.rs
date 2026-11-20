//! Upkeep and fire: buildings wear, may burn, and are repaired through the
//! ordinary site path.
//!
//! Every building whose definition has an `upkeep` block carries an
//! [`Upkeep`]: its condition in permille of a sound building, saved and
//! hashed. Once a day each wears by its kind's rate (faster on a day of
//! storm or snow); below `economy.upkeep.repair_below_permille` it needs
//! repair and the planner opens a repair site (`SiteKind::Repair`) that is
//! supplied, paid and built like any other; below `worn_below_permille` it
//! works worse (slower work, a colder home, a smaller store) and at 0 it is
//! unusable until repaired.
//!
//! A building with a fire risk (`upkeep.fire`) may catch fire on a seeded
//! day at a seeded hour, more likely when worn. A fire takes condition at
//! once and while it burns, may spread to a building whose door is near,
//! and residents nearby come to fight it (`life.fight_fire`): each one at
//! the door slows the damage and the spread and helps put it out, faster
//! with water in the pack. Nothing here reads a clock but `SimClock`, and
//! every roll is `sim_rand` of the world seed, the building and the day or
//! tick.
use super::model::{Building, CivicState, ConstructionSite, Depot, Goods, Plot};
use crate::village::colony::Household;
use crate::village::frontier::building_spec::{BuildingDef, BuildingKind};
use crate::village::frontier::life_spec::UpkeepSpec;
use crate::village::frontier::{Settlement, SettlementMember, economy_spec};
use crate::village::{VillagePlace, Villager, economy as econ, living};
use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use npc_framework::kernel::hash::SimHashResource;
use npc_framework::kernel::rng::sim_rand;
use npc_framework::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeMap;

/// A sound building's condition.
pub const SOUND: u32 = 1_000;
/// A firefighter stands within this distance of the burning building's door.
pub const FIGHT_REACH_MM: i64 = 4_000;
/// Ticks between two steps of a burning fire: damage, fighting, spread.
pub const FIRE_STEP_TICKS: u64 = 100;

const CONDITION_PURPOSE: u32 = 0x5550_4b01;
const FIRE_PURPOSE: u32 = 0x5550_4b02;
const FIRE_HOUR_PURPOSE: u32 = 0x5550_4b03;
const SPREAD_PURPOSE: u32 = 0x5550_4b04;

/// A fire burning in a building.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub struct Fire {
    pub started: u64,
    /// The tick it burns out on its own.
    pub ends: u64,
    /// Fighter-ticks spent on it so far.
    pub fought: u32,
    /// Condition it has taken so far.
    pub damage: u32,
    /// The most firefighters at the door at once.
    pub fighters: u32,
    /// The building it spread from, if it did.
    pub from: Option<SimId>,
}

/// The upkeep of one building: its condition and any fire.
#[derive(Component, Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
#[reflect(Component)]
pub struct Upkeep {
    /// Key of the building's definition (`economy.buildings`).
    pub building: String,
    /// Permille of a sound building: 0 is unusable, 1000 sound.
    pub condition: u32,
    /// A fire rolled for today that has not broken out yet.
    pub ignites_at: Option<u64>,
    pub fire: Option<Fire>,
    /// Since when the building has been unusable (condition 0).
    pub unusable_since: Option<u64>,
}
impl SimHash for Upkeep {
    const NAME: &'static str = "island_building_upkeep_v1";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.str(&self.building);
        h.u32(self.condition);
        h.opt(self.ignites_at.as_ref(), |h, tick| h.u64(*tick));
        h.opt(self.fire.as_ref(), |h, fire| {
            h.u64(fire.started);
            h.u64(fire.ends);
            h.u32(fire.fought);
            h.u32(fire.damage);
            h.u32(fire.fighters);
            h.opt(fire.from.as_ref(), |h, id| h.u32(id.0));
        });
        h.opt(self.unusable_since.as_ref(), |h, tick| h.u64(*tick));
    }
}

/// The island's upkeep bookkeeping: the last day worn, fires waiting to
/// break out and burning, and the running tallies.
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub struct UpkeepState {
    /// The calendar day the last daily pass ran on.
    pub day: Option<u64>,
    /// Fires rolled for today, `(tick, building)` ascending.
    pub pending: Vec<(u64, SimId)>,
    /// Buildings on fire, ascending.
    pub burning: Vec<SimId>,
    /// Fires that broke out, of them spread from a neighbour, put out by
    /// firefighters, and that left their building unusable.
    pub fires: u64,
    pub spread: u64,
    pub put_out: u64,
    pub burned_out: u64,
    /// Repairs finished.
    pub repairs: u64,
    /// The longest a building has stood unusable until repaired, ticks.
    pub longest_unusable: u64,
}
impl SimHashResource for UpkeepState {
    const NAME: &'static str = "island_upkeep_state_v1";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.opt(self.day.as_ref(), |h, day| h.u64(*day));
        h.u32(self.pending.len() as u32);
        for (tick, id) in &self.pending {
            h.u64(*tick);
            h.u32(id.0);
        }
        h.u32(self.burning.len() as u32);
        for id in &self.burning {
            h.u32(id.0);
        }
        for value in [
            self.fires,
            self.spread,
            self.put_out,
            self.burned_out,
            self.repairs,
            self.longest_unusable,
        ] {
            h.u64(value);
        }
    }
    fn snapshot_fns() -> Option<npc_framework::snapshot::ResourceSnapshotFns> {
        Some(npc_framework::snapshot::serde_resource_fns::<Self>())
    }
}

/// The world's upkeep rules, when they are on.
pub fn spec(world: &World) -> Option<&UpkeepSpec> {
    economy_spec::registry(world)
        .map(|registry| &registry.spec.upkeep)
        .filter(|spec| spec.enabled)
}

fn definition<'a>(world: &'a World, key: &str) -> Option<&'a BuildingDef> {
    super::definition(world, key)
}

/// The definition a founding building stands for: a home of its size (else
/// the first home), the first workplace of its trade.
fn founding_definition(world: &World, entity: Entity) -> Option<String> {
    let defs = super::catalog(world);
    if let Some(home) = world.get::<Household>(entity) {
        let homes = || {
            defs.iter()
                .filter(|def| matches!(def.kind, BuildingKind::Home { .. }))
        };
        return homes()
            .find(|def| matches!(def.kind, BuildingKind::Home { beds } if beds == home.capacity))
            .or_else(|| homes().next())
            .map(|def| def.id.clone());
    }
    let place = world.get::<VillagePlace>(entity)?;
    defs.iter()
        .find(|def| {
            matches!(&def.kind, BuildingKind::Workplace { profession, .. } if *profession == place.role)
        })
        .map(|def| def.id.clone())
}

/// A seeded starting condition for a building that stands at the founding:
/// from `founding_min_permille` up to sound.
fn founding_condition(world: &World, id: SimId) -> u32 {
    let low = spec(world).map_or(SOUND, |spec| spec.founding_min_permille.min(SOUND));
    let seed = world.resource::<WorldSeed>().0;
    low + (sim_rand(seed, id, 0, CONDITION_PURPOSE) % u64::from(SOUND - low + 1)) as u32
}

/// Gives the authored homes and workplaces their upkeep; call once after
/// the settlements exist. Inserts components only: no identity moves.
pub(super) fn spawn(world: &mut World) {
    if spec(world).is_none() {
        return;
    }
    let mut founding: Vec<(SimId, Entity)> = world
        .query_filtered::<(Entity, &SimId), (
            With<SettlementMember>,
            Or<(With<Household>, With<VillagePlace>)>,
            Without<Building>,
            Without<Upkeep>,
        )>()
        .iter(world)
        .map(|(entity, id)| (*id, entity))
        .collect();
    founding.sort_unstable_by_key(|(id, _)| *id);
    for (id, entity) in founding {
        let Some(key) = founding_definition(world, entity) else {
            continue;
        };
        if definition(world, &key).is_none_or(|def| def.upkeep.is_none()) {
            continue;
        }
        let condition = founding_condition(world, id);
        world.entity_mut(entity).insert(Upkeep {
            building: key,
            condition,
            ignites_at: None,
            fire: None,
            unusable_since: None,
        });
    }
}

/// Gives a finished building its upkeep: sound when built, seeded when it
/// stands from the founding.
pub(super) fn established(world: &mut World, entity: Entity, key: &str, founding: bool) {
    if spec(world).is_none() || definition(world, key).is_none_or(|def| def.upkeep.is_none()) {
        return;
    }
    let condition = match world.get::<SimId>(entity) {
        Some(id) if founding => founding_condition(world, *id),
        _ => SOUND,
    };
    world.entity_mut(entity).insert(Upkeep {
        building: key.to_owned(),
        condition,
        ignites_at: None,
        fire: None,
        unusable_since: None,
    });
}

/// Every building with upkeep, ascending by identity.
fn all(world: &mut World) -> Vec<(SimId, Entity)> {
    let mut out: Vec<(SimId, Entity)> = world
        .query::<(Entity, &SimId, &Upkeep)>()
        .iter(world)
        .map(|(entity, id, _)| (*id, entity))
        .collect();
    out.sort_unstable_by_key(|(id, _)| *id);
    out
}

/// Called once per tick by `civic::tick`: the daily pass on a new day, fires
/// that break out now, and every hundredth tick the fires that burn.
pub(super) fn tick(world: &mut World, now: u64) {
    if spec(world).is_none() || !world.contains_resource::<UpkeepState>() {
        return;
    }
    let day = crate::village::calendar(world).0;
    if world.resource::<UpkeepState>().day != Some(day) {
        daily(world, day, now);
    }
    loop {
        let due = {
            let state = world.resource::<UpkeepState>();
            state.pending.first().filter(|(at, _)| *at <= now).copied()
        };
        let Some((_, id)) = due else { break };
        world.resource_mut::<UpkeepState>().pending.remove(0);
        let Some(entity) = world.resource::<SimIndex>().get(id) else {
            continue;
        };
        if let Some(mut upkeep) = world.get_mut::<Upkeep>(entity) {
            upkeep.ignites_at = None;
        }
        ignite(world, id, entity, now, None);
    }
    if now.is_multiple_of(FIRE_STEP_TICKS) && !world.resource::<UpkeepState>().burning.is_empty() {
        burn(world, now);
    }
}

/// Wears every building by a day (from the second day on) and rolls the
/// day's fires.
fn daily(world: &mut World, day: u64, now: u64) {
    let first = world.resource::<UpkeepState>().day.is_none();
    world.resource_mut::<UpkeepState>().day = Some(day);
    let Some(spec) = spec(world).cloned() else {
        return;
    };
    let weather = crate::village::weather::today(world);
    let bad = matches!(
        weather,
        crate::village::frontier::life_spec::Weather::Storm
            | crate::village::frontier::life_spec::Weather::Snow
    );
    let season = crate::village::calendar(world).1;
    let seed = world.resource::<WorldSeed>().0;
    let day_length = world.resource::<SimConfig>().day_length.max(1);
    let day_start = now - now % day_length;
    let mut pending = Vec::new();
    for (id, entity) in all(world) {
        let Some(upkeep) = world.get::<Upkeep>(entity).cloned() else {
            continue;
        };
        let Some(rules) = definition(world, &upkeep.building).and_then(|def| def.upkeep.clone())
        else {
            continue;
        };
        if !first {
            let wear = if bad {
                rules.wear_per_day * rules.bad_weather_percent / 100
            } else {
                rules.wear_per_day
            };
            set_condition(
                world,
                id,
                entity,
                upkeep.condition.saturating_sub(wear),
                now,
            );
        }
        // Today's fire, if the dice say so: a building that stands, is not
        // burning, and has its hearth, oven or forge going this season.
        let condition = world.get::<Upkeep>(entity).map_or(0, |u| u.condition);
        let Some(fire) = rules.fire.as_ref() else {
            continue;
        };
        if condition == 0
            || upkeep.fire.is_some()
            || !(fire.seasons.is_empty() || fire.seasons.contains(&season))
        {
            continue;
        }
        let chance = if condition < spec.worn_below_permille {
            fire.daily_permille * spec.fire_worn_percent / 100
        } else {
            fire.daily_permille
        };
        if sim_rand(seed, id, day, FIRE_PURPOSE) % 1_000 >= u64::from(chance) {
            continue;
        }
        // Between six in the morning and ten at night, while it is in use.
        let from = day_length / 4;
        let span = (day_length * 11 / 12).saturating_sub(from).max(1);
        let at = (day_start + from + sim_rand(seed, id, day, FIRE_HOUR_PURPOSE) % span).max(now);
        if let Some(mut upkeep) = world.get_mut::<Upkeep>(entity) {
            upkeep.ignites_at = Some(at);
        }
        pending.push((at, id));
    }
    pending.sort_unstable();
    world.resource_mut::<UpkeepState>().pending = pending;
}

/// Sets a building's condition and what follows from it: when it became
/// unusable, and the room its store withholds.
fn set_condition(world: &mut World, id: SimId, entity: Entity, condition: u32, now: u64) {
    let Some(spec) = spec(world).cloned() else {
        return;
    };
    let Some(mut upkeep) = world.get_mut::<Upkeep>(entity) else {
        return;
    };
    let condition = condition.min(SOUND);
    if upkeep.condition != condition {
        upkeep.condition = condition;
    }
    let broke = condition == 0 && upkeep.unusable_since.is_none();
    if broke {
        upkeep.unusable_since = Some(now);
    }
    if let Some(mut depot) = world.get_mut::<Depot>(entity) {
        let withheld = if condition == 0 {
            depot.capacity
        } else if condition < spec.worn_below_permille {
            depot.capacity - depot.capacity * spec.worn_storage_percent.min(100) / 100
        } else {
            0
        };
        if depot.withheld != withheld {
            depot.withheld = withheld;
        }
    }
    if broke {
        let (label, what, settlement, pos) = describe(world, entity);
        record_effect(
            world,
            None,
            "VillageBuildingUnusable",
            &json!({"building":id,"label":label,"what":what,"settlement":settlement,
                "pos":[pos.x, pos.z]}),
        );
    }
}

/// A building's name, what it is in words, its settlement and its place.
fn describe(world: &World, entity: Entity) -> (String, String, Option<SimId>, SimPos) {
    let settlement = world
        .get::<SettlementMember>(entity)
        .map(|member| member.settlement);
    let what = world
        .get::<Upkeep>(entity)
        .and_then(|upkeep| definition(world, &upkeep.building))
        .map_or_else(|| "building".to_owned(), |def| def.display.to_lowercase());
    let label = label(world, entity);
    let pos = world.get::<SimPos>(entity).copied().unwrap_or_default();
    (label, what, settlement, pos)
}

/// The name players read for a building: its own label, or for an authored
/// workplace the settlement and the building ("Alderwood bakery").
pub fn label(world: &World, entity: Entity) -> String {
    if let Some(building) = world.get::<Building>(entity) {
        return building.label.clone();
    }
    if let Some(home) = world.get::<Household>(entity) {
        return home.label.clone();
    }
    let place = world
        .get::<SettlementMember>(entity)
        .and_then(|member| world.resource::<SimIndex>().get(member.settlement))
        .and_then(|settlement| world.get::<Settlement>(settlement))
        .map(|settlement| settlement.label.clone())
        .unwrap_or_default();
    let what = world
        .get::<Upkeep>(entity)
        .and_then(|upkeep| definition(world, &upkeep.building))
        .map(|def| def.display.to_lowercase())
        .or_else(|| {
            world
                .get::<VillagePlace>(entity)
                .map(|place| place.label.clone())
        })
        .unwrap_or_else(|| "building".to_owned());
    if place.is_empty() {
        what
    } else {
        format!("{place} {what}")
    }
}

/// A fire breaks out: it takes `fire_flash_permille` at once and burns
/// until it is put out or `fire_burn_ticks` have passed.
fn ignite(world: &mut World, id: SimId, entity: Entity, now: u64, from: Option<SimId>) {
    let Some(spec) = spec(world).cloned() else {
        return;
    };
    let Some(upkeep) = world.get::<Upkeep>(entity).cloned() else {
        return;
    };
    if upkeep.fire.is_some() || upkeep.condition == 0 {
        return;
    }
    let flash = spec.fire_flash_permille.min(upkeep.condition);
    if let Some(mut upkeep) = world.get_mut::<Upkeep>(entity) {
        upkeep.fire = Some(Fire {
            started: now,
            ends: now.saturating_add(spec.fire_burn_ticks),
            fought: 0,
            damage: flash,
            fighters: 0,
            from,
        });
    }
    set_condition(world, id, entity, upkeep.condition - flash, now);
    {
        let mut state = world.resource_mut::<UpkeepState>();
        if let Err(at) = state.burning.binary_search(&id) {
            state.burning.insert(at, id);
        }
        state.fires += 1;
        if from.is_some() {
            state.spread += 1;
        }
    }
    let (label, what, settlement, pos) = describe(world, entity);
    let kind = definition(world, &upkeep.building).map_or("", |def| def.kind.name());
    record_effect(
        world,
        None,
        "VillageFire",
        &json!({"building":id,"label":label,"what":what,"kind":kind,"settlement":settlement,
            "pos":[pos.x, pos.z],"condition":upkeep.condition - flash,"from":from}),
    );
}

/// Residents fighting the fire at `door` now: living, awake and within
/// [`FIGHT_REACH_MM`] of it, ascending by identity.
fn fighters(world: &mut World, door: SimPos) -> Vec<(SimId, Entity)> {
    let mut out: Vec<(SimId, Entity)> = world
        .query::<(Entity, &SimId, &SimPos, &Villager)>()
        .iter(world)
        .filter(|(_, _, pos, _)| pos.dist2(door) <= FIGHT_REACH_MM * FIGHT_REACH_MM)
        .map(|(entity, id, _, _)| (*id, entity))
        .collect();
    out.retain(|(_, entity)| {
        living(world, *entity) && !crate::village::indoors::asleep(world, *entity)
    });
    out.sort_unstable_by_key(|(id, _)| *id);
    out
}

/// One step of every burning fire: firefighters work, the fire takes its
/// damage (less for every fighter), may spread, and goes out when fought
/// enough, burnt out or the building is gone.
fn burn(world: &mut World, now: u64) {
    let Some(spec) = spec(world).cloned() else {
        return;
    };
    let seed = world.resource::<WorldSeed>().0;
    let drink = crate::village::roles::roles(world).drink;
    let burning = world.resource::<UpkeepState>().burning.clone();
    for id in burning {
        let Some(entity) = world.resource::<SimIndex>().get(id) else {
            world
                .resource_mut::<UpkeepState>()
                .burning
                .retain(|b| *b != id);
            continue;
        };
        let Some(door) = world.get::<SimPos>(entity).copied() else {
            continue;
        };
        // Past its time the fire has burnt out: no more damage.
        let expired = world
            .get::<Upkeep>(entity)
            .and_then(|upkeep| upkeep.fire.as_ref())
            .is_some_and(|fire| now >= fire.ends);
        let crew = if expired {
            Vec::new()
        } else {
            fighters(world, door)
        };
        let mut work = 0_u32;
        for (_, fighter) in &crew {
            let water = econ::count(world, *fighter, drink) > 0
                && econ::consume(world, *fighter, drink, 1).is_ok();
            work += if water {
                FIRE_STEP_TICKS as u32 * spec.fire_water_percent / 100
            } else {
                FIRE_STEP_TICKS as u32
            };
        }
        let relief = 100 + spec.fire_fighter_relief_percent * crew.len() as u32;
        let Some(upkeep) = world.get::<Upkeep>(entity).cloned() else {
            continue;
        };
        let Some(mut fire) = upkeep.fire.clone() else {
            world
                .resource_mut::<UpkeepState>()
                .burning
                .retain(|b| *b != id);
            continue;
        };
        let damage = if expired {
            0
        } else {
            (spec.fire_damage_per_100_ticks * 100 / relief).min(upkeep.condition)
        };
        fire.fought = fire.fought.saturating_add(work);
        fire.damage = fire.damage.saturating_add(damage);
        fire.fighters = fire.fighters.max(crew.len() as u32);
        let condition = upkeep.condition - damage;
        set_condition(world, id, entity, condition, now);
        // The spread: each standing neighbour near the door may catch.
        let chance = if expired {
            0
        } else {
            u64::from(spec.fire_spread_permille * 100 / relief)
        };
        if chance > 0 {
            let reach = i64::from(spec.fire_spread_m) * 1_000;
            let neighbours: Vec<(SimId, Entity)> = all(world)
                .into_iter()
                .filter(|(other, entity)| {
                    *other != id
                        && world
                            .get::<Upkeep>(*entity)
                            .is_some_and(|upkeep| upkeep.fire.is_none() && upkeep.condition > 0)
                        && world
                            .get::<SimPos>(*entity)
                            .is_some_and(|pos| pos.dist2(door) <= reach * reach)
                })
                .collect();
            for (other, neighbour) in neighbours {
                if sim_rand(seed, other, now, SPREAD_PURPOSE) % 1_000 < chance {
                    ignite(world, other, neighbour, now, Some(id));
                }
            }
        }
        let out = fire.fought >= spec.fire_fight_ticks || now >= fire.ends || condition == 0;
        if !out {
            if let Some(mut upkeep) = world.get_mut::<Upkeep>(entity) {
                upkeep.fire = Some(fire);
            }
            continue;
        }
        if let Some(mut upkeep) = world.get_mut::<Upkeep>(entity) {
            upkeep.fire = None;
        }
        let put_out = fire.fought >= spec.fire_fight_ticks && condition > 0;
        {
            let mut state = world.resource_mut::<UpkeepState>();
            state.burning.retain(|b| *b != id);
            state.put_out += u64::from(put_out);
            state.burned_out += u64::from(condition == 0);
        }
        let (label, what, settlement, pos) = describe(world, entity);
        record_effect(
            world,
            None,
            "VillageFireOut",
            &json!({"building":id,"label":label,"what":what,"settlement":settlement,
                "pos":[pos.x, pos.z],"damage":fire.damage,"condition":condition,
                "fighters":fire.fighters,"put_out":put_out,"burned_out":condition == 0,
                "ticks":now - fire.started}),
        );
    }
}

/// The burning building nearest to `actor` within `fire_reach_m`, with the
/// place to fight it from: its door. Pure.
pub fn fire_to_fight(world: &World, actor: Entity) -> Option<(SimId, SimPos)> {
    let spec = spec(world)?;
    let state = world.get_resource::<UpkeepState>()?;
    if state.burning.is_empty() {
        return None;
    }
    let here = *world.get::<SimPos>(actor)?;
    let reach = i64::from(spec.fire_reach_m) * 1_000;
    state
        .burning
        .iter()
        .filter_map(|id| {
            let door = *world.get::<SimPos>(world.resource::<SimIndex>().get(*id)?)?;
            let distance = door.dist2(here);
            (distance <= reach * reach).then_some((distance, *id, door))
        })
        .min_by_key(|(distance, id, _)| (*distance, *id))
        .map(|(_, id, door)| (id, door))
}

/// Starts a fire now, for tests and tools: the same as one the dice set.
pub fn start_fire(world: &mut World, building: SimId) -> Result<(), String> {
    let entity = world
        .resource::<SimIndex>()
        .get(building)
        .ok_or("no such building")?;
    let upkeep = world
        .get::<Upkeep>(entity)
        .ok_or("that building has no upkeep")?;
    if upkeep.fire.is_some() {
        return Err("it is burning already".into());
    }
    if upkeep.condition == 0 {
        return Err("nothing is left to burn".into());
    }
    let now = world.resource::<SimClock>().tick;
    ignite(world, building, entity, now, None);
    Ok(())
}

/// Sets a building's condition, for tests and tools.
pub fn set_condition_of(world: &mut World, building: SimId, condition: u32) -> Result<(), String> {
    let entity = world
        .resource::<SimIndex>()
        .get(building)
        .ok_or("no such building")?;
    if world.get::<Upkeep>(entity).is_none() {
        return Err("that building has no upkeep".into());
    }
    let now = world.resource::<SimClock>().tick;
    set_condition(world, building, entity, condition, now);
    Ok(())
}

/// A building's condition in permille; `None` for one without upkeep.
pub fn condition(world: &World, entity: Entity) -> Option<u32> {
    world.get::<Upkeep>(entity).map(|upkeep| upkeep.condition)
}

/// Whether a building can be used: it has no upkeep, or a condition above 0.
pub fn usable(world: &World, entity: Entity) -> bool {
    world
        .get::<Upkeep>(entity)
        .is_none_or(|upkeep| upkeep.condition > 0)
}

/// [`usable`] by identity.
pub fn usable_id(world: &World, building: SimId) -> bool {
    world
        .resource::<SimIndex>()
        .get(building)
        .is_none_or(|entity| usable(world, entity))
}

/// Whether a building is worn: below `worn_below_permille`.
pub fn worn(world: &World, entity: Entity) -> bool {
    match (spec(world), world.get::<Upkeep>(entity)) {
        (Some(spec), Some(upkeep)) => upkeep.condition < spec.worn_below_permille,
        _ => false,
    }
}

/// Work time at a workplace in percent: `worn_work_percent` when worn.
pub fn work_percent(world: &World, station: SimId) -> u32 {
    match (spec(world), world.resource::<SimIndex>().get(station)) {
        (Some(spec), Some(entity)) if worn(world, entity) => spec.worn_work_percent,
        _ => 100,
    }
}

/// How well a home shelters and its hearth warms, percent: 100 sound, the
/// worn share when worn, 0 when unusable.
pub fn shelter_percent(world: &World, home: Entity) -> u32 {
    match (spec(world), world.get::<Upkeep>(home)) {
        (Some(_), Some(upkeep)) if upkeep.condition == 0 => 0,
        (Some(spec), Some(upkeep)) if upkeep.condition < spec.worn_below_permille => {
            spec.worn_warmth_percent
        }
        _ => 100,
    }
}

/// Whether a building is on fire.
pub fn burning(world: &World, entity: Entity) -> bool {
    world
        .get::<Upkeep>(entity)
        .is_some_and(|upkeep| upkeep.fire.is_some())
}

/// The building to repair next in a settlement: the one in the worst
/// condition below `repair_below_permille` that is not burning and has no
/// repair site yet, with its materials and work. `(building, cost, work
/// ticks, unusable)`.
pub(super) fn repair_candidate(
    world: &mut World,
    settlement: SimId,
) -> Option<(SimId, Goods, u32, bool)> {
    let spec = spec(world)?.clone();
    let mending: Vec<SimId> = world
        .resource::<CivicState>()
        .sites
        .get(&settlement)
        .into_iter()
        .flatten()
        .filter_map(|site| {
            world
                .get::<ConstructionSite>(world.resource::<SimIndex>().get(*site)?)?
                .repairs()
        })
        .collect();
    let mut wanted: Vec<(u32, SimId, String)> = world
        .query::<(&SimId, &Upkeep, &SettlementMember)>()
        .iter(world)
        .filter(|(id, upkeep, member)| {
            member.settlement == settlement
                && upkeep.condition < spec.repair_below_permille
                && upkeep.fire.is_none()
                && !mending.contains(id)
        })
        .map(|(id, upkeep, _)| (upkeep.condition, *id, upkeep.building.clone()))
        .collect();
    wanted.sort_unstable_by_key(|(condition, id, _)| (*condition, *id));
    let (condition, id, key) = wanted.into_iter().next()?;
    let def = definition(world, &key)?;
    let (cost, work) = repair_bill(def, condition)?;
    Some((id, cost, work, condition == 0))
}

/// The materials and work a repair from `condition` back to sound takes:
/// `repair_percent` of the building's cost and work, times the damage.
pub fn repair_bill(def: &BuildingDef, condition: u32) -> Option<(Goods, u32)> {
    let rules = def.upkeep.as_ref()?;
    let damage = u64::from(SOUND.saturating_sub(condition));
    let share = |amount: u32| {
        (u64::from(amount) * u64::from(rules.repair_percent) * damage).div_ceil(100 * 1_000) as u32
    };
    let mut cost: Goods = def
        .cost
        .iter()
        .map(|stack| (stack.good.clone(), share(stack.count)))
        .filter(|(_, count)| *count > 0)
        .collect();
    if cost.is_empty() {
        let first = def.cost.first()?;
        cost.insert(first.good.clone(), 1);
    }
    Some((cost, share(def.work_ticks).max(1)))
}

/// Opens the repair site of a building: on the cell at its door, with the
/// materials and work of [`repair_bill`], paid by the civic fund like any
/// civic site. Returns the site.
pub(super) fn open_repair(
    world: &mut World,
    building: SimId,
    cost: Goods,
    work_ticks: u32,
    reason: &str,
) -> Result<SimId, String> {
    let entity = world
        .resource::<SimIndex>()
        .get(building)
        .ok_or("the building is gone")?;
    let upkeep = world
        .get::<Upkeep>(entity)
        .cloned()
        .ok_or("the building has no upkeep")?;
    let settlement = world
        .get::<SettlementMember>(entity)
        .map(|member| member.settlement)
        .ok_or("the building belongs to no settlement")?;
    let door = *world
        .get::<SimPos>(entity)
        .ok_or("the building has no place")?;
    let (x, z) = (door.x.div_euclid(1000), door.z.div_euclid(1000));
    let plot = Plot {
        x,
        z,
        rotation: 0,
        lo_x: x,
        lo_z: z,
        hi_x: x,
        hi_z: z,
        solid: false,
    };
    let label = format!("Repairs to {}", label(world, entity));
    let now = world.resource::<SimClock>().tick;
    let (site, _) = spawn_sim(
        world,
        (
            plot.door(),
            ConstructionSite {
                building: upkeep.building.clone(),
                label: label.clone(),
                settlement,
                plot,
                blocker: None,
                cost: cost.clone(),
                delivered: Goods::new(),
                incoming: BTreeMap::new(),
                work_ticks,
                work_done: 0,
                placed_at: now,
                civic: true,
                reason: reason.into(),
                paid: 0,
                builder: None,
                builder_until: 0,
                last_delivery_at: None,
                kind: super::model::SiteKind::Repair { target: building },
            },
            SettlementMember { settlement },
            Perceivable::new(crate::world::kinds::STATION),
        ),
    );
    world
        .resource_mut::<CivicState>()
        .sites
        .entry(settlement)
        .or_default()
        .push(site);
    super::sites::raise_building_levy(world);
    crate::village::colony_demand_changed(world);
    record_effect(
        world,
        None,
        "VillageSitePlaced",
        &json!({"site":site,"building":upkeep.building,"label":label,"settlement":settlement,
            "cell":[x,z],"rotation":0,"civic":true,"reason":reason,"cost":cost,
            "work_ticks":work_ticks,"repair":building,"condition":upkeep.condition}),
    );
    Ok(site)
}

/// A finished repair: the building is sound again. Called by the site
/// path when the last stage of a repair site is done.
pub(super) fn repaired(world: &mut World, site: &ConstructionSite, site_id: SimId) {
    let Some(target) = site.repairs() else {
        return;
    };
    let now = world.resource::<SimClock>().tick;
    let Some(entity) = world.resource::<SimIndex>().get(target) else {
        return;
    };
    let Some(before) = world.get::<Upkeep>(entity).cloned() else {
        return;
    };
    set_condition(world, target, entity, SOUND, now);
    let unusable_for = before.unusable_since.map(|since| now.saturating_sub(since));
    if let Some(mut upkeep) = world.get_mut::<Upkeep>(entity) {
        upkeep.unusable_since = None;
    }
    {
        let mut state = world.resource_mut::<UpkeepState>();
        state.repairs += 1;
        if let Some(ticks) = unusable_for {
            state.longest_unusable = state.longest_unusable.max(ticks);
        }
    }
    let (label, what, settlement, pos) = describe(world, entity);
    record_effect(
        world,
        None,
        "VillageBuildingRepaired",
        &json!({"site":site_id,"building":target,"label":label,"what":what,
            "settlement":settlement,"pos":[pos.x, pos.z],"from":before.condition,
            "paid":site.paid,"unusable_ticks":unusable_for,"site_label":site.label}),
    );
}

/// What a building card shows of a building's upkeep.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct UpkeepView {
    pub condition: u32,
    /// Below `repair_below_permille`: the planner will open a repair site.
    pub needs_repair: bool,
    /// Below `worn_below_permille`: it works worse.
    pub worn: bool,
    pub unusable: bool,
    pub burning: bool,
    /// The open repair site, if any.
    pub repair_site: Option<SimId>,
}

/// A building's upkeep for a card or a tool: one component and the
/// settlement's open sites. `None` for a building without upkeep.
pub fn view(world: &World, building: SimId) -> Option<UpkeepView> {
    let spec = spec(world)?;
    let entity = world.resource::<SimIndex>().get(building)?;
    let upkeep = world.get::<Upkeep>(entity)?;
    let settlement = world.get::<SettlementMember>(entity)?.settlement;
    let repair_site = world
        .get_resource::<CivicState>()?
        .sites
        .get(&settlement)
        .into_iter()
        .flatten()
        .copied()
        .find(|site| {
            world
                .resource::<SimIndex>()
                .get(*site)
                .and_then(|site| world.get::<ConstructionSite>(site))
                .and_then(ConstructionSite::repairs)
                == Some(building)
        });
    Some(UpkeepView {
        condition: upkeep.condition,
        needs_repair: upkeep.condition < spec.repair_below_permille,
        worn: upkeep.condition < spec.worn_below_permille,
        unusable: upkeep.condition == 0,
        burning: upkeep.fire.is_some(),
        repair_site,
    })
}

/// The island's upkeep at a glance: for the ledger and the soak reports.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct UpkeepReport {
    pub buildings: u32,
    /// Mean condition, permille.
    pub mean_condition: u32,
    pub needs_repair: u32,
    pub worn: u32,
    pub unusable: u32,
    pub burning: u32,
    pub fires: u64,
    pub spread: u64,
    pub put_out: u64,
    pub burned_out: u64,
    pub repairs: u64,
    /// The longest a building stood unusable, ticks: finished spans and the
    /// one still running, whichever is longer.
    pub longest_unusable: u64,
}

/// [`UpkeepReport`] of the whole island. A scan of the buildings with
/// upkeep: call it when asked, never every frame.
pub fn report(world: &World) -> UpkeepReport {
    let (Some(spec), Some(state)) = (spec(world), world.get_resource::<UpkeepState>()) else {
        return UpkeepReport::default();
    };
    let now = world.resource::<SimClock>().tick;
    let mut out = UpkeepReport {
        fires: state.fires,
        spread: state.spread,
        put_out: state.put_out,
        burned_out: state.burned_out,
        repairs: state.repairs,
        longest_unusable: state.longest_unusable,
        burning: state.burning.len() as u32,
        ..UpkeepReport::default()
    };
    let mut total = 0_u64;
    let Some(mut query) = world.try_query::<&Upkeep>() else {
        return out;
    };
    for upkeep in query.iter(world) {
        out.buildings += 1;
        total += u64::from(upkeep.condition);
        out.needs_repair += u32::from(upkeep.condition < spec.repair_below_permille);
        out.worn += u32::from(upkeep.condition < spec.worn_below_permille);
        out.unusable += u32::from(upkeep.condition == 0);
        if let Some(since) = upkeep.unusable_since {
            out.longest_unusable = out.longest_unusable.max(now.saturating_sub(since));
        }
    }
    out.mean_condition = (total / u64::from(out.buildings.max(1))) as u32;
    out
}

/// Checks after a load: every condition within range, and the fire lists
/// name buildings that burn or wait to.
pub fn validate(world: &World) -> Result<(), String> {
    let Some(state) = world.get_resource::<UpkeepState>() else {
        return Ok(());
    };
    let index = world.resource::<SimIndex>();
    for id in &state.burning {
        if index
            .get(*id)
            .and_then(|entity| world.get::<Upkeep>(entity))
            .is_none_or(|upkeep| upkeep.fire.is_none())
        {
            return Err(format!("Building {} is listed as burning but is not", id.0));
        }
    }
    if let Some(mut query) = world.try_query::<&Upkeep>() {
        for upkeep in query.iter(world) {
            if upkeep.condition > SOUND {
                return Err("A building's condition is above sound".into());
            }
        }
    }
    Ok(())
}
