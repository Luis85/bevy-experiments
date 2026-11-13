//! Finite stocks, gradual regeneration, habitat carrying capacity and restoration.
use super::{SettlementMember, settlements};
use crate::village::{
    ResourcePatch, Season, Vitality, colony,
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
pub fn spawn(world: &mut World, ids: &mut crate::IslandIds) {
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
    let exported = super::spec(world)
        .map(|spec| spec.resource_patches.clone())
        .unwrap_or_default();
    for (settlement, _) in settlements::all(world) {
        let def = settlements::definition(world, settlement)
            .expect("settlement definition")
            .clone();
        let at = def.center;
        let patches = super::economy_spec::registry(world)
            .expect("regional economy")
            .spec
            .patches
            .clone();
        // An exported world brings its own patches, in their order.
        let mut named = BTreeSet::new();
        for patch in exported.iter().filter(|patch| patch.settlement == def.key) {
            let gather_ticks = patch_def(world, &patch.good).map_or(100, |d| d.gather_ticks);
            let id = super::shape::spawn_patch(
                world,
                &patch.good,
                SimPos::new(patch.x, patch.z),
                settlement,
                patch.capacity,
                gather_ticks,
            );
            let name = if named.insert(patch.good.clone()) {
                format!("{}_patch_{}", def.key, patch.good)
            } else {
                format!("{}_patch_{}_{}", def.key, patch.good, id.0)
            };
            ids.name(name, id);
        }
        for patch in patches.iter().filter(|patch| {
            exported.is_empty() && (patch.biomes.is_empty() || patch.biomes.contains(&def.biome))
        }) {
            let (kind, capacity, gather_ticks) =
                (patch.good.as_str(), patch.capacity, patch.gather_ticks);
            let position = patch
                .at_site
                .and_then(|role| def.sites.iter().find(|site| site.role == role))
                .map_or(SimPos::new(at.x + patch.dx, at.z + patch.dz), |site| {
                    site.position
                });
            // A fishing ground lies at the water: the dry cell beside water
            // nearest to the authored spot (the fisher's site).
            let position = if kind == crate::village::catalog::FISH {
                shore_near(world, position).unwrap_or(position)
            } else {
                position
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
/// Within 40 m of `near`, the centre of the walkable cell beside a water cell
/// that is nearest to it (ties: lower z, then lower x). `None` without water.
fn shore_near(world: &World, near: SimPos) -> Option<SimPos> {
    const REACH_CELLS: i32 = 40;
    let (cx, cz) = (near.x.div_euclid(1000), near.z.div_euclid(1000));
    let water =
        |x: i32, z: i32| super::terrain::cell(world, x, z).is_some_and(|cell| !cell.walkable());
    let mut best: Option<(i64, i32, i32)> = None;
    for z in cz - REACH_CELLS..=cz + REACH_CELLS {
        for x in cx - REACH_CELLS..=cx + REACH_CELLS {
            let dry = super::terrain::cell(world, x, z).is_some_and(|cell| cell.walkable());
            if !dry
                || ![(0, -1), (-1, 0), (1, 0), (0, 1)]
                    .iter()
                    .any(|(dx, dz)| water(x + dx, z + dz))
            {
                continue;
            }
            let at = SimPos::new(x * 1000 + 500, z * 1000 + 500);
            let key = (at.dist2(near), z, x);
            if best.is_none_or(|old| key < old) {
                best = Some(key);
            }
        }
    }
    best.map(|(_, z, x)| SimPos::new(x * 1000 + 500, z * 1000 + 500))
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
    // Never within 1.5 m of another animal (`wildlife::SPACING_MM`).
    let at = crate::village::wildlife::birth_spot(world, home, SimPos::new(home.x + x, home.z + z));
    let (_, entity) = spawn_sim(
        world,
        (
            at,
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
/// Regrowth and harvest parameters of the patches of one good.
fn patch_def(world: &World, kind: &str) -> Option<super::economy_spec::PatchDef> {
    super::economy_spec::registry(world)?
        .spec
        .patches
        .iter()
        .find(|patch| patch.good == kind)
        .cloned()
}
pub fn harvested(world: &mut World, patch: SimId, quantity: u32) {
    let Some(e) = world.resource::<SimIndex>().get(patch) else {
        return;
    };
    let kind = world
        .get::<ResourcePatch>(e)
        .map(|p| p.kind.clone())
        .unwrap_or_default();
    let (loss, soil_loss) =
        patch_def(world, &kind).map_or((40, 0), |patch| (patch.harvest_loss, patch.soil_loss));
    if let Some(mut state) = world.get_mut::<EcologicalPatch>(e) {
        state.harvested_units = state.harvested_units.saturating_add(u64::from(quantity));
        state.biomass_milli = state
            .biomass_milli
            .saturating_sub(quantity.saturating_mul(loss));
        state.disturbance_milli = state
            .disturbance_milli
            .saturating_add(quantity.saturating_mul(loss / 2))
            .min(FULL_BIOMASS);
        state.soil_milli = state
            .soil_milli
            .saturating_sub(quantity.saturating_mul(soil_loss));
    }
}
/// Adds up to `units` to a patch from outside its own regrowth (rain on a
/// well), keeping the ecological ledger consistent. Returns the units added.
pub fn replenish(world: &mut World, patch: Entity, units: u32) -> u32 {
    let Some(mut stock) = world.get_mut::<ResourcePatch>(patch) else {
        return 0;
    };
    let added = units.min(stock.capacity.saturating_sub(stock.remaining));
    stock.remaining += added;
    if let Some(mut state) = world.get_mut::<EcologicalPatch>(patch) {
        state.regrown_units = state.regrown_units.saturating_add(u64::from(added));
    }
    added
}
pub fn tick(world: &mut World) {
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
        let definition = patch_def(world, &patch.kind);
        if !definition.as_ref().is_some_and(|patch| patch.finite) {
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
            let base = definition.as_ref().map_or(4, |patch| patch.regrowth);
            let seasonal = definition
                .as_ref()
                .and_then(|patch| patch.winter_regrowth)
                .filter(|_| season == Season::Winter)
                .unwrap_or(base);
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
    // The wild animals only, ascending by id (the index's order).
    let Some(mut query) = world.try_query::<(Entity, &SimId, &Wildlife, &Vitality)>() else {
        return Vec::new();
    };
    let mut animals: Vec<(SimId, Entity)> = query
        .iter(world)
        .filter(|(_, _, animal, vitality)| animal.habitat == habitat && vitality.died_at.is_none())
        .map(|(entity, id, ..)| (*id, entity))
        .collect();
    animals.sort_unstable_by_key(|(id, _)| *id);
    animals
}
pub fn harvestable_prey(world: &World, target: SimId) -> bool {
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
                        .is_some_and(|r| r.kind == crate::village::roles::roles(world).timber))
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
pub fn work_inputs(world: &World, kind: EcologyWorkKind) -> Vec<(&'static str, u32)> {
    let roles = crate::village::roles::roles(world);
    match kind {
        EcologyWorkKind::PlantTree => vec![(roles.timber, 1), (roles.drink, 1)],
        EcologyWorkKind::RestoreHabitat => vec![(roles.fiber, 2), (roles.drink, 1)],
    }
}
pub fn check_work(
    world: &World,
    actor: Entity,
    patch: SimId,
    kind: EcologyWorkKind,
) -> Result<(), String> {
    if !crate::village::living(world, actor)
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
    if matches!(kind, EcologyWorkKind::PlantTree)
        && source.kind != crate::village::roles::roles(world).timber
    {
        return Err("Plant cuttings at a woodland resource site".into());
    }
    if matches!(kind, EcologyWorkKind::RestoreHabitat)
        && (source.kind == crate::village::roles::roles(world).ore
            || source.kind == crate::village::roles::roles(world).drink)
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
            let definition = patch_def(world, &patch.kind)
                .ok_or("Regional resource kind differs from construction")?;
            let (capacity, duration) = (&definition.capacity, &definition.gather_ticks);
            let settlement = settlements::member(world, *e)
                .filter(|id| settlements::definition(world, *id).is_some())
                .ok_or("Regional resource has no valid settlement catchment")?;
            // Several patches of a good may serve a settlement (placed by
            // `PlaceResourcePatch` or exported): each holds a richness of 25%
            // to 400% of the authored capacity, in steps of 25%, and keeps the
            // authored gathering time. Any other capacity is tampering.
            sources.insert((settlement, patch.kind.as_str()));
            let rich = (1..=16_u64).any(|quarters| {
                (u64::from(*capacity) * quarters / 4).max(1) == u64::from(patch.capacity)
            });
            if !rich || patch.gather_ticks != *duration {
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
    // At least one patch of every listed good serves every settlement (an
    // exported world brings its own list).
    let exported = super::spec(world).is_some_and(|spec| !spec.resource_patches.is_empty());
    let missing = !exported
        && settlements::all(world).into_iter().any(|(id, _)| {
            settlements::definition(world, id).is_some_and(|settlement| {
                super::economy_spec::registry(world).is_some_and(|registry| {
                    registry.spec.patches.iter().any(|patch| {
                        (patch.biomes.is_empty() || patch.biomes.contains(&settlement.biome))
                            && !sources.contains(&(id, patch.good.as_str()))
                    })
                })
            })
        });
    if missing {
        return Err("Regional resource identities are incomplete".into());
    }
    Ok(())
}
