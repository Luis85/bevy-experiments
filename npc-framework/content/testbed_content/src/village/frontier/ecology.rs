//! Finite stocks, gradual regeneration, habitat carrying capacity and restoration.
use super::{SettlementMember, settlements};
use crate::village::{
    ResourcePatch, Season, Vitality, catalog, colony,
    hunting::{Wildlife, WildlifeHabitat},
};
use bevy_app::App;
use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use npc_framework::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeSet;

pub const UPDATE_TICKS: u64 = 600;
pub const FULL_BIOMASS: u32 = 100_000;
const PATCH_DEFS: [(&str, i32, i32, u32, u32); 7] = [
    (catalog::GRAIN, -30000, 10000, 160, 180),
    ("log", -32000, 0, 240, 240),
    ("ore", 38000, 10000, 12000, 300),
    (catalog::WATER, 0, 0, 160, 100),
    (catalog::FIBER, -31000, 17000, 160, 120),
    (catalog::BERRIES, 38000, 0, 160, 80),
    (catalog::FISH, 38000, -8000, 80, 220),
];
const BIRTH_OFFSETS_MM: [(i32, i32); 8] = [
    (0, 0),
    (-1500, 0),
    (1500, 0),
    (0, -1500),
    (0, 1500),
    (-1500, -1500),
    (1500, 1500),
    (-1500, 1500),
];
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub struct GrowingCutting {
    pub matures_at: u64,
    pub biomass: u32,
}
#[derive(Component, Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub struct EcologicalPatch {
    pub biomass_milli: u32,
    pub soil_milli: u32,
    pub disturbance_milli: u32,
    pub last_update_tick: u64,
    pub pending_growth: Vec<GrowingCutting>,
    pub harvested_units: u64,
    pub regrown_units: u64,
    pub planted_units: u64,
    pub restored_units: u64,
}
impl Default for EcologicalPatch {
    fn default() -> Self {
        Self {
            biomass_milli: FULL_BIOMASS,
            soil_milli: FULL_BIOMASS,
            disturbance_milli: 0,
            last_update_tick: 0,
            pending_growth: Vec::new(),
            harvested_units: 0,
            regrown_units: 0,
            planted_units: 0,
            restored_units: 0,
        }
    }
}
impl SimHash for EcologicalPatch {
    const NAME: &'static str = "living_world_ecological_patch_v1";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u32(self.biomass_milli);
        h.u32(self.soil_milli);
        h.u32(self.disturbance_milli);
        h.u64(self.last_update_tick);
        h.u32(self.pending_growth.len() as u32);
        for p in &self.pending_growth {
            h.u64(p.matures_at);
            h.u32(p.biomass);
        }
        h.u64(self.harvested_units);
        h.u64(self.regrown_units);
        h.u64(self.planted_units);
        h.u64(self.restored_units);
    }
}
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub struct HabitatEcology {
    pub next_birth: u64,
    pub births: u64,
}
impl SimHash for HabitatEcology {
    const NAME: &'static str = "living_world_habitat_ecology_v1";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u64(self.next_birth);
        h.u64(self.births);
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EcologyWorkKind {
    PlantTree,
    RestoreHabitat,
}
pub fn register(app: &mut App) {
    app.register_sim_component::<EcologicalPatch>()
        .register_sim_component::<HabitatEcology>();
}
pub fn spawn(world: &mut World, ids: &mut crate::TestbedIds) {
    let day = world.resource::<SimConfig>().day_length.max(1);
    let grid = world.resource::<SpatialGrid>();
    let blocked: BTreeSet<_> = world
        .resource::<SimIndex>()
        .map
        .values()
        .filter_map(|e| {
            Some(grid.footprint_cells(*world.get::<SimPos>(*e)?, world.get::<GridBlocker>(*e)?.0))
        })
        .flatten()
        .collect();
    for (settlement, _) in settlements::all(world) {
        let def = settlements::definition(world, settlement)
            .expect("settlement definition")
            .clone();
        let at = def.center;
        for (kind, dx, dz, capacity, gather_ticks) in PATCH_DEFS {
            let position = if kind == catalog::WATER {
                def.sites
                    .iter()
                    .find(|site| site.role == crate::village::Role::WaterKeeper)
                    .expect("authored well")
                    .position
            } else {
                SimPos::new(at.x + dx, at.z + dz)
            };
            let position = clear_resource_position(world, position, &blocked);
            let (id, _) = spawn_sim(
                world,
                (
                    position,
                    SettlementMember { settlement },
                    ResourcePatch {
                        kind: kind.into(),
                        remaining: capacity,
                        capacity,
                        replenish_at: UPDATE_TICKS,
                        gather_ticks,
                    },
                    EcologicalPatch::default(),
                    Perceivable::new(crate::world::kinds::STATION),
                ),
            );
            ids.name(format!("{}_patch_{kind}", def.key), id);
        }
        let home = SimPos::new(at.x - 28000, at.z + 28000);
        let (habitat, _) = spawn_sim(
            world,
            (
                home,
                SettlementMember { settlement },
                WildlifeHabitat {
                    home,
                    capacity: 8,
                    renew_at: day / 8,
                },
                HabitatEcology {
                    next_birth: (day / 8).max(1),
                    births: 0,
                },
                Perceivable::new(crate::world::kinds::STATION),
            ),
        );
        ids.name(format!("{}_habitat", def.key), habitat);
        for n in 0..8 {
            spawn_animal(world, habitat, home, n);
        }
    }
}
fn clear_resource_position(
    world: &World,
    nominal: SimPos,
    blocked: &BTreeSet<CellCoord>,
) -> SimPos {
    let grid = world.resource::<SpatialGrid>();
    let cell = grid.cell_of(nominal);
    let free =
        |c: CellCoord| grid.bounds().contains(c) && !grid.is_blocking(c) && !blocked.contains(&c);
    if free(cell) {
        return nominal;
    }
    let mut candidates = Vec::new();
    for dz in -6..=6 {
        for dx in -6..=6 {
            let c = CellCoord::new(cell.x + dx, cell.z + dz);
            if free(c) {
                let at = grid.center(c);
                candidates.push((at.dist2(nominal), c.x, c.z, at));
            }
        }
    }
    candidates.sort_by_key(|(distance, x, z, _)| (*distance, *x, *z));
    candidates.first().map_or(nominal, |(_, _, _, at)| *at)
}
fn spawn_animal(world: &mut World, habitat: SimId, home: SimPos, index: u32) {
    let (x, z) = BIRTH_OFFSETS_MM[index as usize % BIRTH_OFFSETS_MM.len()];
    let (_, entity) = spawn_sim(
        world,
        (
            SimPos::new(home.x + x, home.z + z),
            Wildlife { habitat },
            Vitality::default(),
            Perceivable::new(crate::world::kinds::UNIT),
        ),
    );
    if let Some(settlement) = world
        .resource::<SimIndex>()
        .get(habitat)
        .and_then(|e| settlements::member(world, e))
    {
        world
            .entity_mut(entity)
            .insert(SettlementMember { settlement });
    }
}
fn birth_position_available(world: &World, at: SimPos) -> bool {
    let grid = world.resource::<SpatialGrid>();
    let cell = grid.cell_of(at);
    if !grid.bounds().contains(cell)
        || grid.static_cell(cell).flags.blocking
        || super::terrain::cell(world, cell.x, cell.z).is_none_or(|terrain| !terrain.walkable())
    {
        return false;
    }
    // A structure may have completed earlier in this Effects pass, before the
    // derived grid's next rebuild. Inspect current authoritative footprints.
    !world.resource::<SimIndex>().map.values().any(|entity| {
        let (Some(position), Some(blocker)) = (
            world.get::<SimPos>(*entity),
            world.get::<GridBlocker>(*entity),
        ) else {
            return false;
        };
        let (lo, hi) = grid.footprint_cell_rect(*position, blocker.0);
        (lo.x..=hi.x).contains(&cell.x) && (lo.z..=hi.z).contains(&cell.z)
    })
}
pub fn harvested(world: &mut World, patch: SimId, quantity: u32) {
    let Some(e) = world.resource::<SimIndex>().get(patch) else {
        return;
    };
    let kind = world
        .get::<ResourcePatch>(e)
        .map(|p| p.kind.clone())
        .unwrap_or_default();
    let loss = if kind == "log" {
        125
    } else if kind == catalog::WATER || kind == "ore" {
        0
    } else {
        40
    };
    if let Some(mut state) = world.get_mut::<EcologicalPatch>(e) {
        state.harvested_units = state.harvested_units.saturating_add(u64::from(quantity));
        state.biomass_milli = state
            .biomass_milli
            .saturating_sub(quantity.saturating_mul(loss));
        state.disturbance_milli = state
            .disturbance_milli
            .saturating_add(quantity.saturating_mul(loss / 2))
            .min(FULL_BIOMASS);
        if kind == catalog::GRAIN {
            state.soil_milli = state.soil_milli.saturating_sub(quantity.saturating_mul(30));
        }
    }
}
pub fn tick(world: &mut World) {
    if !super::enabled(world) {
        return;
    }
    let now = world.resource::<SimClock>().tick;
    if !now.is_multiple_of(UPDATE_TICKS) {
        return;
    }
    let (_, season, _) = crate::village::calendar(world);
    let patches: Vec<_> = world
        .resource::<SimIndex>()
        .map
        .iter()
        .filter_map(|(id, e)| {
            Some((
                *id,
                *e,
                world.get::<ResourcePatch>(*e)?.clone(),
                world.get::<EcologicalPatch>(*e)?.clone(),
            ))
        })
        .collect();
    for (_, e, mut patch, mut state) in patches {
        if now <= state.last_update_tick {
            continue;
        }
        let rounds = (now - state.last_update_tick) / UPDATE_TICKS;
        state.last_update_tick = now;
        let matured: u32 = state
            .pending_growth
            .iter()
            .filter(|p| p.matures_at <= now)
            .map(|p| p.biomass)
            .sum();
        state.pending_growth.retain(|p| p.matures_at > now);
        state.biomass_milli = state
            .biomass_milli
            .saturating_add(matured)
            .min(FULL_BIOMASS);
        if patch.kind != "ore" {
            let natural = (rounds.min(u64::from(u32::MAX)) as u32).saturating_mul(150);
            state.biomass_milli = state
                .biomass_milli
                .saturating_add(natural)
                .min(FULL_BIOMASS);
            state.soil_milli = state
                .soil_milli
                .saturating_add(natural / 2)
                .min(FULL_BIOMASS);
            state.disturbance_milli = state.disturbance_milli.saturating_sub(natural / 2);
            let base: u32 = if patch.kind == catalog::WATER {
                10
            } else if patch.kind == "log" {
                4
            } else if patch.kind == catalog::FISH {
                2
            } else {
                4
            };
            let seasonal = if patch.kind == catalog::GRAIN && season == Season::Winter {
                1
            } else {
                base
            };
            let growth = u128::from(seasonal)
                * u128::from(rounds)
                * u128::from(state.biomass_milli)
                * u128::from(state.soil_milli)
                / 10_000_000_000;
            let available = patch.capacity.saturating_sub(patch.remaining);
            let grown = growth.min(u128::from(available)) as u32;
            patch.remaining += grown;
            state.regrown_units = state.regrown_units.saturating_add(u64::from(grown));
        }
        patch.replenish_at = now.saturating_add(UPDATE_TICKS);
        world.entity_mut(e).insert((patch, state));
    }
}
fn living_animals(world: &World, habitat: SimId) -> Vec<(SimId, Entity)> {
    world
        .resource::<SimIndex>()
        .map
        .iter()
        .filter(|(_, e)| {
            world
                .get::<Wildlife>(**e)
                .is_some_and(|a| a.habitat == habitat)
                && world
                    .get::<Vitality>(**e)
                    .is_some_and(|v| v.died_at.is_none())
        })
        .map(|(id, e)| (*id, *e))
        .collect()
}
pub fn harvestable_prey(world: &World, target: SimId) -> bool {
    if !super::enabled(world) {
        return true;
    }
    let Some(animal) = world
        .resource::<SimIndex>()
        .get(target)
        .and_then(|e| world.get::<Wildlife>(e))
    else {
        return false;
    };
    // Two adult animals are the explicitly protected breeding refuge. This
    // policy is shared by native players and resident hunting actions.
    living_animals(world, animal.habitat).len() > 2
}
pub fn wildlife_tick(world: &mut World) {
    if !super::enabled(world) {
        return;
    }
    let now = world.resource::<SimClock>().tick;
    let interval = (world.resource::<SimConfig>().day_length / 8).max(1);
    let habitats: Vec<_> = world
        .resource::<SimIndex>()
        .map
        .iter()
        .filter_map(|(id, e)| {
            Some((
                *id,
                *e,
                *world.get::<WildlifeHabitat>(*e)?,
                *world.get::<HabitatEcology>(*e)?,
            ))
        })
        .collect();
    for (id, e, mut habitat, mut state) in habitats {
        if now < state.next_birth {
            continue;
        }
        let settlement = settlements::member(world, e);
        let biomass = world
            .resource::<SimIndex>()
            .map
            .values()
            .filter_map(|p| {
                (settlements::member(world, *p) == settlement
                    && world
                        .get::<ResourcePatch>(*p)
                        .is_some_and(|r| r.kind == "log"))
                .then(|| world.get::<EcologicalPatch>(*p).map(|s| s.biomass_milli))
                .flatten()
            })
            .min()
            .unwrap_or(FULL_BIOMASS);
        habitat.capacity = (2 + biomass.saturating_mul(6) / FULL_BIOMASS).min(8);
        let animals = living_animals(world, id);
        if animals.len() >= 2 && animals.len() < (habitat.capacity as usize) {
            let occupied: Vec<_> = animals
                .iter()
                .filter_map(|(_, e)| world.get::<SimPos>(*e).copied())
                .collect();
            if let Some(index) = BIRTH_OFFSETS_MM.iter().position(|(x, z)| {
                let at = SimPos::new(habitat.home.x + x, habitat.home.z + z);
                !occupied.contains(&at) && birth_position_available(world, at)
            }) {
                spawn_animal(world, id, habitat.home, index as u32);
                state.births = state.births.saturating_add(1);
                record_effect(
                    world,
                    Some(id),
                    "WildlifeBorn",
                    &json!({"capacity":habitat.capacity,"population":animals.len()+1}),
                );
            }
        }
        state.next_birth = now.saturating_add(interval);
        habitat.renew_at = state.next_birth;
        world.entity_mut(e).insert((habitat, state));
    }
}
pub fn work_inputs(kind: EcologyWorkKind) -> Vec<(&'static str, u32)> {
    match kind {
        EcologyWorkKind::PlantTree => vec![("log", 1), (catalog::WATER, 1)],
        EcologyWorkKind::RestoreHabitat => vec![(catalog::FIBER, 2), (catalog::WATER, 1)],
    }
}
pub fn check_work(
    world: &World,
    actor: Entity,
    patch: SimId,
    kind: EcologyWorkKind,
) -> Result<(), String> {
    if !super::enabled(world)
        || !crate::village::living(world, actor)
        || world.get::<colony::AwaitingArrival>(actor).is_some()
    {
        return Err("Restoration requires a living island resident".into());
    }
    let e = world
        .resource::<SimIndex>()
        .get(patch)
        .ok_or("Resource site is missing")?;
    let source = world
        .get::<ResourcePatch>(e)
        .ok_or("Choose a resource habitat")?;
    let state = world
        .get::<EcologicalPatch>(e)
        .ok_or("This site has no ecological state")?;
    if matches!(kind, EcologyWorkKind::PlantTree) && source.kind != "log" {
        return Err("Plant cuttings at a woodland resource site".into());
    }
    if matches!(kind, EcologyWorkKind::RestoreHabitat)
        && (source.kind == "ore" || source.kind == catalog::WATER)
    {
        return Err("Restore a woodland, field, forage patch or fishing habitat".into());
    }
    if state.pending_growth.len() >= 32 {
        return Err("This habitat already has enough seedlings maturing".into());
    }
    if state.biomass_milli == FULL_BIOMASS
        && state.soil_milli == FULL_BIOMASS
        && state.disturbance_milli == 0
    {
        return Err("This habitat is healthy; restore a disturbed site".into());
    }
    Ok(())
}
/// Called by the shared timed action only after its reserved inputs were consumed.
/// This domain completion never grants items or bypasses the action lifecycle.
pub fn complete_work(
    world: &mut World,
    actor: SimId,
    patch: SimId,
    kind: EcologyWorkKind,
) -> Result<(), String> {
    let person = world
        .resource::<SimIndex>()
        .get(actor)
        .ok_or("Resident is missing")?;
    check_work(world, person, patch, kind)?;
    let e = world
        .resource::<SimIndex>()
        .get(patch)
        .ok_or("Resource site is missing")?;
    let now = world.resource::<SimClock>().tick;
    let day = world.resource::<SimConfig>().day_length;
    let mut state = world
        .get_mut::<EcologicalPatch>(e)
        .expect("validated habitat");
    match kind {
        EcologyWorkKind::PlantTree => {
            state.pending_growth.push(GrowingCutting {
                matures_at: now.saturating_add(day),
                biomass: 5000,
            });
            state.planted_units = state.planted_units.saturating_add(1);
        }
        EcologyWorkKind::RestoreHabitat => {
            state.soil_milli = state.soil_milli.saturating_add(8000).min(FULL_BIOMASS);
            state.disturbance_milli = state.disturbance_milli.saturating_sub(8000);
            state.pending_growth.push(GrowingCutting {
                matures_at: now.saturating_add(day / 2),
                biomass: 3000,
            });
            state.restored_units = state.restored_units.saturating_add(1);
        }
    }
    let delay = match kind {
        EcologyWorkKind::PlantTree => day,
        EcologyWorkKind::RestoreHabitat => day / 2,
    };
    record_effect(
        world,
        Some(actor),
        "HabitatRestored",
        &json!({"patch":patch,"work":kind,"matures_after_ticks":delay}),
    );
    Ok(())
}
pub fn validate(world: &World) -> Result<(), String> {
    let now = world.resource::<SimClock>().tick;
    let mut sources = BTreeSet::new();
    for e in world.resource::<SimIndex>().map.values() {
        if let Some(patch) = world.get::<ResourcePatch>(*e) {
            let (_, _, _, capacity, duration) = PATCH_DEFS
                .iter()
                .find(|(kind, _, _, _, _)| *kind == patch.kind)
                .ok_or("Regional resource kind differs from construction")?;
            let settlement = settlements::member(world, *e)
                .filter(|id| settlements::definition(world, *id).is_some())
                .ok_or("Regional resource has no valid settlement catchment")?;
            if patch.capacity != *capacity
                || patch.gather_ticks != *duration
                || !sources.insert((settlement, patch.kind.as_str()))
            {
                return Err("Regional resource identity differs from construction".into());
            }
            let state = world
                .get::<EcologicalPatch>(*e)
                .ok_or("Regional resource ecology is missing")?;
            if patch.remaining > patch.capacity
                || state.biomass_milli > FULL_BIOMASS
                || state.soil_milli > FULL_BIOMASS
                || state.disturbance_milli > FULL_BIOMASS
                || state.last_update_tick > now
                || state.pending_growth.len() > 32
                || state.pending_growth.iter().any(|p| {
                    !matches!(p.biomass, 3000 | 5000) || p.matures_at <= state.last_update_tick
                })
                || state
                    .pending_growth
                    .iter()
                    .filter(|p| p.biomass == 5000)
                    .count() as u64
                    > state.planted_units
                || state
                    .pending_growth
                    .iter()
                    .filter(|p| p.biomass == 3000)
                    .count() as u64
                    > state.restored_units
                || u64::from(patch.capacity)
                    .checked_add(state.regrown_units)
                    .and_then(|n| n.checked_sub(state.harvested_units))
                    != Some(u64::from(patch.remaining))
            {
                return Err("Regional resource stock or ecological history is inconsistent".into());
            }
        }
        if let Some(habitat) = world.get::<WildlifeHabitat>(*e)
            && (!(2..=8).contains(&habitat.capacity) || world.get::<HabitatEcology>(*e).is_none())
        {
            return Err("Regional wildlife ecology is missing or invalid".into());
        }
    }
    if sources.len() != settlements::all(world).len() * PATCH_DEFS.len() {
        return Err("Regional resource identities are incomplete".into());
    }
    Ok(())
}
