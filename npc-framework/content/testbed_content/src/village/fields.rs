//! Crop fields worked by residents (regional profile only).
//!
//! A field is a building with the `field` attribute: an open granary with
//! plots around it. Farmers sow a fallow plot in a sowing season, tend what
//! grows and harvest what is ripe, each as timed work through the resident
//! tree (`life.sow`, `life.tend`, `life.harvest`). Growth follows the weather
//! and stops in winter, when every plot lies fallow. Part of each harvest
//! stays in the field as seed for the same plot, so a field keeps itself in
//! seed; the rest goes into the farmer's pack like any gathered good.
use super::civic::{Building, CivicState};
use super::frontier::economy_spec;
use super::frontier::life_spec::CropSpec;
use super::frontier::settlements;
use super::{Role, Season, Villager, ai, economy as econ, living, production};
use bevy_app::App;
use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use npc_framework::interaction::Item;
use npc_framework::kernel::hash::SimHashResource;
use npc_framework::kernel::rng::sim_rand;
use npc_framework::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeMap;

/// Plots grow in rounds of this many ticks.
pub const GROWTH_ROUND_TICKS: u64 = 200;
const BLIGHT_PURPOSE: u32 = 0x4649_4c01;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
#[serde(rename_all = "snake_case")]
pub enum PlotStage {
    Fallow,
    Growing,
    Ripe,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
#[serde(rename_all = "snake_case")]
pub enum FieldTask {
    Sow,
    Tend,
    Harvest,
}
impl FieldTask {
    pub fn name(self) -> &'static str {
        match self {
            Self::Sow => "sow",
            Self::Tend => "tend",
            Self::Harvest => "harvest",
        }
    }
}

/// One plot of a field.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub struct FieldPlot {
    pub stage: PlotStage,
    /// Growing ticks accumulated since sowing, at 100% growth.
    pub growth: u32,
    pub sown_at: u64,
    pub tended_at: u64,
    /// The farmer walking to or working on the plot, until `claimed_until`.
    pub claimed_by: Option<SimId>,
    pub claimed_until: u64,
}
impl FieldPlot {
    fn fallow() -> Self {
        Self {
            stage: PlotStage::Fallow,
            growth: 0,
            sown_at: 0,
            tended_at: 0,
            claimed_by: None,
            claimed_until: 0,
        }
    }
    fn free_for(&self, actor: SimId, now: u64) -> bool {
        self.claimed_by.is_none_or(|holder| holder == actor) || self.claimed_until <= now
    }
}

/// The plots of a field building. The entity is also a [`Building`] and a
/// store: the granary.
#[derive(Component, Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
#[reflect(Component)]
pub struct Field {
    /// Key of the crop in `economy.crops`.
    pub crop: String,
    pub plots: Vec<FieldPlot>,
    /// Seed kept in the field for the next sowing, in units of the seed good.
    pub seed_stock: u32,
    pub sowings: u64,
    pub harvested_units: u64,
    /// Plots lost to winter or blight before they were harvested.
    pub lost_plots: u64,
}
impl SimHash for Field {
    const NAME: &'static str = "regional_field_v1";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.str(&self.crop);
        h.u32(self.plots.len() as u32);
        for plot in &self.plots {
            h.u8(plot.stage as u8);
            h.u32(plot.growth);
            h.u64(plot.sown_at);
            h.u64(plot.tended_at);
            h.opt(plot.claimed_by.as_ref(), |h, id| h.u32(id.0));
            h.u64(plot.claimed_until);
        }
        h.u32(self.seed_stock);
        h.u64(self.sowings);
        h.u64(self.harvested_units);
        h.u64(self.lost_plots);
    }
}

/// One piece of field work in progress.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
#[component(storage = "SparseSet")]
#[reflect(Component)]
pub struct FieldWork {
    pub field: SimId,
    pub plot: u32,
    pub task: FieldTask,
    pub ends: u64,
}
impl SimHash for FieldWork {
    const NAME: &'static str = "regional_field_work_v1";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u32(self.field.0);
        h.u32(self.plot);
        h.u8(self.task as u8);
        h.u64(self.ends);
    }
}

/// Fields by settlement, ascending by identity, and what was founded.
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub struct FieldState {
    pub fields: BTreeMap<SimId, Vec<SimId>>,
    /// The founding fields were laid out (or could not be).
    pub founded: bool,
    /// Winter already cleared the plots of this calendar year.
    pub fallow_year: Option<u64>,
}
impl SimHashResource for FieldState {
    const NAME: &'static str = "regional_field_state_v1";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u32(self.fields.len() as u32);
        for (settlement, ids) in &self.fields {
            h.u32(settlement.0);
            h.u32(ids.len() as u32);
            for id in ids {
                h.u32(id.0);
            }
        }
        h.u8(u8::from(self.founded));
        h.opt(self.fallow_year.as_ref(), |h, year| h.u64(*year));
    }
    fn snapshot_fns() -> Option<npc_framework::snapshot::ResourceSnapshotFns> {
        Some(npc_framework::snapshot::serde_resource_fns::<Self>())
    }
}

/// Regional profile only.
pub fn register(app: &mut App) {
    app.init_resource::<FieldState>()
        .register_sim_resource::<FieldState>()
        .register_sim_component::<Field>()
        .register_sim_component::<FieldWork>();
}

fn crop<'a>(world: &'a World, key: &str) -> Option<&'a CropSpec> {
    economy_spec::registry(world)?.spec.crop(key)
}

/// Fields of a settlement, ascending by identity.
pub fn fields_of(world: &World, settlement: SimId) -> Vec<(SimId, Entity)> {
    let Some(state) = world.get_resource::<FieldState>() else {
        return Vec::new();
    };
    state
        .fields
        .get(&settlement)
        .into_iter()
        .flatten()
        .filter_map(|id| Some((*id, world.resource::<SimIndex>().get(*id)?)))
        .collect()
}

/// Called when a building with the `field` attribute is finished or founded.
pub(super) fn established(
    world: &mut World,
    entity: Entity,
    plots: u32,
    crop_key: &str,
    seed: u32,
) {
    let Some(building) = world.get::<Building>(entity) else {
        return;
    };
    let settlement = building.settlement;
    let Some(id) = world.get::<SimId>(entity).copied() else {
        return;
    };
    world.entity_mut(entity).insert(Field {
        crop: crop_key.into(),
        plots: (0..plots).map(|_| FieldPlot::fallow()).collect(),
        seed_stock: seed,
        sowings: 0,
        harvested_units: 0,
        lost_plots: 0,
    });
    let mut state = world.resource_mut::<FieldState>();
    let list = state.fields.entry(settlement).or_default();
    list.push(id);
    list.sort_unstable();
}

/// Seed of `kind` lying in fields: goods in nobody's pack.
pub fn seed_in_custody(world: &World, kind: &str) -> u32 {
    let Some(state) = world.get_resource::<FieldState>() else {
        return 0;
    };
    let index = world.resource::<SimIndex>();
    state
        .fields
        .values()
        .flatten()
        .filter_map(|id| world.get::<Field>(index.get(*id)?))
        .filter(|field| crop(world, &field.crop).is_some_and(|crop| crop.seed.good == kind))
        .map(|field| field.seed_stock)
        .sum()
}

/// The settlement whose fields a resident works: the one of the workplace.
fn farm_settlement(world: &World, actor: Entity) -> Option<SimId> {
    let person = world.get::<Villager>(actor)?;
    if person.role != super::roles::roles(world).sower || !living(world, actor) {
        return None;
    }
    production::economic_settlement(world, actor).or_else(|| settlements::member(world, actor))
}

/// Ticks left before winter begins, from now.
fn ticks_before_winter(world: &World) -> u64 {
    let day_length = world.resource::<SimConfig>().day_length.max(1);
    let now = world.resource::<SimClock>().tick;
    let season_days = super::days_per_season(world);
    let (day, season, _) = super::calendar(world);
    if season == Season::Winter {
        return 0;
    }
    let winter_day = (day / (season_days * 4)) * season_days * 4 + season_days * 3;
    (winter_day.saturating_sub(day)) * day_length - now % day_length
}

/// Units of a crop's harvest good a settlement holds (its residents' packs
/// and its stores) plus what its sown plots will yield.
pub fn crop_stock(world: &World, settlement: SimId, crop: &CropSpec) -> u32 {
    let kind = crop.harvest.good.as_str();
    let carried: u32 = super::demography::members(world, settlement)
        .into_iter()
        .map(|(_, actor)| econ::count(world, actor, kind))
        .sum();
    let stored: u32 = super::civic::depots(world, settlement)
        .into_iter()
        .filter_map(|(_, entity)| world.get::<super::civic::Depot>(entity))
        .map(|depot| depot.stock(kind))
        .sum();
    let growing: u32 = fields_of(world, settlement)
        .into_iter()
        .filter_map(|(_, entity)| world.get::<Field>(entity))
        .filter(|field| field.crop == crop.key)
        .map(|field| {
            field
                .plots
                .iter()
                .filter(|plot| plot.stage != PlotStage::Fallow)
                .count() as u32
                * crop.harvest.count
        })
        .sum();
    carried + stored + growing
}

/// Whether the settlement has enough of the crop in hand and in the ground:
/// farmers then leave plots fallow instead of growing a glut.
fn stocked(world: &World, settlement: SimId, crop: &CropSpec) -> bool {
    crop.stock_per_resident > 0
        && crop_stock(world, settlement, crop)
            >= super::demography::members(world, settlement).len() as u32 * crop.stock_per_resident
}

/// The plot this farmer would work now for `task`: `(field, entity, plot)`.
pub fn target(world: &World, actor: Entity, task: FieldTask) -> Option<(SimId, Entity, u32)> {
    let settlement = farm_settlement(world, actor)?;
    let me = *world.get::<SimId>(actor)?;
    let now = world.resource::<SimClock>().tick;
    let season = super::calendar(world).1;
    for (id, entity) in fields_of(world, settlement) {
        let Some(field) = world.get::<Field>(entity) else {
            continue;
        };
        let Some(crop) = crop(world, &field.crop) else {
            continue;
        };
        let wanted = match task {
            FieldTask::Sow => {
                if !crop.sow_seasons.contains(&season)
                    || ticks_before_winter(world) < u64::from(crop.growth_ticks)
                    || (field.seed_stock < crop.seed.count
                        && econ::count(world, actor, &crop.seed.good) < crop.seed.count)
                    || !field
                        .plots
                        .iter()
                        .any(|plot| plot.stage == PlotStage::Fallow)
                    || stocked(world, settlement, crop)
                {
                    continue;
                }
                PlotStage::Fallow
            }
            FieldTask::Tend => PlotStage::Growing,
            FieldTask::Harvest => {
                let carried = crop.harvest.count.saturating_sub(crop.seed.count).max(1);
                if econ::fit(
                    world,
                    actor,
                    &[Item::new(&crop.harvest.good).stack(carried)],
                )
                .is_err()
                {
                    continue;
                }
                PlotStage::Ripe
            }
        };
        if world
            .get::<SimPos>(entity)
            .is_some_and(|pos| super::routing::avoiding(world, actor, *pos, Some(id)))
        {
            continue;
        }
        let plot = field.plots.iter().position(|plot| {
            plot.stage == wanted
                && plot.free_for(me, now)
                && (task != FieldTask::Tend
                    || now >= plot.tended_at.saturating_add(crop.tend_interval_ticks))
        });
        if let Some(plot) = plot {
            return Some((id, entity, plot as u32));
        }
    }
    None
}

/// The resident step behind `life.sow`, `life.tend` and `life.harvest`: walk
/// to the field, then start the timed work.
pub(super) fn step(world: &mut World, actor: Entity, task: FieldTask) -> Result<(), &'static str> {
    if world
        .get::<Villager>(actor)
        .is_none_or(|person| person.role != super::roles::roles(world).sower)
    {
        return Err("only a farmer works the fields");
    }
    let (field_id, entity, plot) = target(world, actor, task).ok_or(match task {
        FieldTask::Sow => "no fallow plot can be sown now, or no seed is at hand",
        FieldTask::Tend => "no growing plot needs tending",
        FieldTask::Harvest => "no plot is ripe, or the pack is full",
    })?;
    let me = *world.get::<SimId>(actor).ok_or("no identity")?;
    let now = world.resource::<SimClock>().tick;
    let (good, base) = {
        let field = world.get::<Field>(entity).ok_or("the field is gone")?;
        let crop = crop(world, &field.crop).ok_or("the field grows an unknown crop")?;
        (
            crop.harvest.good.clone(),
            match task {
                FieldTask::Sow => crop.sow_ticks,
                FieldTask::Tend => crop.tend_ticks,
                FieldTask::Harvest => crop.harvest_ticks,
            },
        )
    };
    production::intent(world, actor, task.name(), &good, "", Some(field_id));
    let hold = now.saturating_add(
        economy_spec::tuning(world).map_or(1200, |tuning| tuning.haul_reservation_ticks),
    );
    claim(world, entity, plot, me, hold);
    if !ai::approach_work(world, actor, entity) {
        return Ok(());
    }
    if super::behavior::busy(world, actor) {
        return Err("already working");
    }
    let ends =
        now.saturating_add(u64::from(living::gather_ticks(world, actor, &good, base)).max(1));
    world
        .entity_mut(actor)
        .remove::<MoveIntent>()
        .insert(FieldWork {
            field: field_id,
            plot,
            task,
            ends,
        });
    claim(world, entity, plot, me, ends);
    Ok(())
}

fn claim(world: &mut World, field: Entity, plot: u32, worker: SimId, until: u64) {
    if let Some(mut state) = world.get_mut::<Field>(field)
        && let Some(slot) = state.plots.get_mut(plot as usize)
        && (slot.claimed_by != Some(worker) || slot.claimed_until != until)
    {
        slot.claimed_by = Some(worker);
        slot.claimed_until = until;
    }
}

/// Stops field work without crediting it and frees the plot.
pub fn cancel(world: &mut World, actor: Entity) {
    // A read first: profiles without fields never register the type.
    if world.get::<FieldWork>(actor).is_none() {
        return;
    }
    let Some(work) = world.entity_mut(actor).take::<FieldWork>() else {
        return;
    };
    let me = world.get::<SimId>(actor).copied();
    if let Some(mut field) = world
        .resource::<SimIndex>()
        .get(work.field)
        .and_then(|entity| world.get_mut::<Field>(entity))
        && let Some(plot) = field.plots.get_mut(work.plot as usize)
        && plot.claimed_by == me
    {
        plot.claimed_by = None;
        plot.claimed_until = 0;
    }
}

fn finish(world: &mut World, id: SimId, actor: Entity, work: FieldWork, now: u64) {
    let Some(entity) = world.resource::<SimIndex>().get(work.field) else {
        return;
    };
    let Some(field) = world.get::<Field>(entity).cloned() else {
        return;
    };
    let Some(crop) = crop(world, &field.crop).cloned() else {
        return;
    };
    let Some(plot) = field.plots.get(work.plot as usize).cloned() else {
        return;
    };
    if !econ::near(world, actor, entity) {
        return;
    }
    match work.task {
        FieldTask::Sow => {
            if plot.stage != PlotStage::Fallow {
                return;
            }
            // Seed kept in the field first, else seed from the farmer's pack.
            let from_field = field.seed_stock >= crop.seed.count;
            if !from_field && econ::consume(world, actor, &crop.seed.good, crop.seed.count).is_err()
            {
                return;
            }
            let mut state = world.get_mut::<Field>(entity).expect("field");
            if from_field {
                state.seed_stock -= crop.seed.count;
            }
            state.sowings += 1;
            state.plots[work.plot as usize] = FieldPlot {
                stage: PlotStage::Growing,
                growth: 0,
                sown_at: now,
                tended_at: now,
                claimed_by: None,
                claimed_until: 0,
            };
            record_effect(
                world,
                Some(id),
                "VillageFieldSown",
                &json!({"field":work.field,"plot":work.plot,"crop":crop.key,
                    "seed":crop.seed.good,"seed_from_field":from_field}),
            );
        }
        FieldTask::Tend => {
            if plot.stage != PlotStage::Growing {
                return;
            }
            let mut state = world.get_mut::<Field>(entity).expect("field");
            let slot = &mut state.plots[work.plot as usize];
            slot.tended_at = now;
            slot.claimed_by = None;
            slot.claimed_until = 0;
        }
        FieldTask::Harvest => {
            if plot.stage != PlotStage::Ripe {
                return;
            }
            // The plot's own seed stays in the field when seed and harvest
            // are the same good; the farmer carries the rest away.
            let kept = if crop.seed.good == crop.harvest.good {
                crop.seed.count.min(crop.harvest.count.saturating_sub(1))
            } else {
                0
            };
            let carried = crop.harvest.count - kept;
            if econ::fit(
                world,
                actor,
                &[Item::new(&crop.harvest.good).stack(carried)],
            )
            .is_err()
            {
                return;
            }
            {
                let mut state = world.get_mut::<Field>(entity).expect("field");
                state.seed_stock = state.seed_stock.saturating_add(kept);
                state.harvested_units = state
                    .harvested_units
                    .saturating_add(u64::from(crop.harvest.count));
                state.plots[work.plot as usize] = FieldPlot::fallow();
            }
            econ::grant(world, actor, Item::new(&crop.harvest.good).stack(carried));
            living::wear_gather_tool(world, actor, id, &crop.harvest.good);
            econ::produce(world, &crop.harvest.good, crop.harvest.count);
            production::completed(world, id, &crop.harvest.good, carried);
            record_effect(
                world,
                Some(id),
                "VillageFieldHarvested",
                &json!({"field":work.field,"plot":work.plot,"crop":crop.key,
                    "kind":crop.harvest.good,"quantity":carried,"seed_kept":kept}),
            );
        }
    }
}

fn found(world: &mut World) {
    let Some(def) = economy_spec::registry(world).and_then(|registry| {
        registry
            .spec
            .buildings
            .iter()
            .find(|def| def.field.as_ref().is_some_and(|field| field.founding))
            .cloned()
    }) else {
        return;
    };
    let Some(field) = def.field.clone() else {
        return;
    };
    let seed = crop(world, &field.crop).map_or(0, |crop| crop.seed.count * field.plots);
    for (settlement, _) in settlements::all(world) {
        // Beside the farm when the ground allows, else anywhere near the centre.
        let farm = settlements::workplace(world, settlement, super::roles::roles(world).sower)
            .and_then(|id| world.resource::<SimIndex>().get(id))
            .and_then(|entity| world.get::<SimPos>(entity).copied());
        if let Some(id) = super::civic::found_building(
            world,
            &def.id,
            settlement,
            farm,
            "founded with the village",
        ) && let Some(entity) = world.resource::<SimIndex>().get(id)
            && let Some(mut state) = world.get_mut::<Field>(entity)
        {
            state.seed_stock = seed;
        }
    }
    found_coat_makers(world);
}

/// Living world (`tuning.found_coat_workshops`): each settlement is founded
/// with the workplace of the trade that makes coats already built, and each
/// of its coat makers with one batch of the inputs of the first coat recipe
/// (one shelf of coat materials). Read from data: the building whose trade
/// outputs coats, the recipe that makes them.
fn found_coat_makers(world: &mut World) {
    if !economy_spec::tuning(world).is_some_and(|tuning| tuning.found_coat_workshops) {
        return;
    }
    let garment = super::roles::roles(world).garment;
    let Some(registry) = economy_spec::registry(world) else {
        return;
    };
    let makers: Vec<Role> = registry
        .spec
        .professions
        .iter()
        .filter(|profession| profession.outputs.iter().any(|good| good == garment))
        .map(|profession| profession.role)
        .collect();
    let workshop = registry
        .spec
        .buildings
        .iter()
        .find_map(|def| match &def.kind {
            super::frontier::building_spec::BuildingKind::Workplace { profession, .. }
                if makers.contains(profession) =>
            {
                Some(def.id.clone())
            }
            _ => None,
        });
    let materials: Vec<(String, u32)> = registry
        .spec
        .recipes
        .iter()
        .find(|recipe| recipe.outputs.iter().any(|out| out.good == garment))
        .map(|recipe| {
            recipe
                .inputs
                .iter()
                .map(|input| (input.good.clone(), input.count))
                .collect()
        })
        .unwrap_or_default();
    for (settlement, _) in settlements::all(world) {
        if let Some(workshop) = &workshop {
            let _ = super::civic::found_building(
                world,
                workshop,
                settlement,
                None,
                "founded with the village so the first winter has coats",
            );
        }
        let mut members = super::demography::members(world, settlement);
        members.sort_unstable_by_key(|(id, _)| *id);
        for (_, actor) in members {
            if !world
                .get::<Villager>(actor)
                .is_some_and(|person| makers.contains(&person.role))
            {
                continue;
            }
            for (good, count) in &materials {
                super::economy::grant(
                    world,
                    actor,
                    npc_framework::interaction::Item::new(good).stack(*count),
                );
            }
        }
    }
}

/// Once per tick in the regional Effects pass, after resident actions.
pub fn tick(world: &mut World) {
    if !world.contains_resource::<FieldState>() {
        return;
    }
    let now = world.resource::<SimClock>().tick;
    if !world.resource::<FieldState>().founded && now >= 1 {
        world.resource_mut::<FieldState>().founded = true;
        found(world);
    }
    if world
        .resource::<FieldState>()
        .fields
        .values()
        .all(Vec::is_empty)
    {
        return;
    }
    let mut workers: Vec<_> = world
        .query::<(Entity, &SimId, &FieldWork)>()
        .iter(world)
        .map(|(entity, id, work)| (*id, entity, *work))
        .collect();
    workers.sort_unstable_by_key(|(id, _, _)| *id);
    for (id, actor, work) in workers {
        if !living(world, actor) {
            cancel(world, actor);
            continue;
        }
        if work.ends > now {
            continue;
        }
        cancel(world, actor);
        finish(world, id, actor, work, now);
    }
    if !now.is_multiple_of(GROWTH_ROUND_TICKS) {
        return;
    }
    let (day, season, _) = super::calendar(world);
    let year = day / (super::days_per_season(world) * 4);
    let fields: Vec<(SimId, Entity)> = {
        let index = world.resource::<SimIndex>();
        world
            .resource::<FieldState>()
            .fields
            .values()
            .flatten()
            .filter_map(|id| Some((*id, index.get(*id)?)))
            .collect()
    };
    if season == Season::Winter {
        if world.resource::<FieldState>().fallow_year != Some(year) {
            world.resource_mut::<FieldState>().fallow_year = Some(year);
            for (id, entity) in fields {
                let Some(mut field) = world.get_mut::<Field>(entity) else {
                    continue;
                };
                let lost = field
                    .plots
                    .iter()
                    .filter(|plot| plot.stage != PlotStage::Fallow)
                    .count() as u64;
                for plot in &mut field.plots {
                    *plot = FieldPlot::fallow();
                }
                field.lost_plots += lost;
                record_effect(
                    world,
                    None,
                    "VillageFieldFallow",
                    &json!({"field":id,"lost_plots":lost,"reason":"winter"}),
                );
            }
        }
        return;
    }
    let percent = u64::from(super::weather::growth_percent(world));
    if percent == 0 {
        return;
    }
    for (id, entity) in fields {
        let Some(field) = world.get::<Field>(entity) else {
            continue;
        };
        let Some(crop) = crop(world, &field.crop).cloned() else {
            continue;
        };
        if !field
            .plots
            .iter()
            .any(|plot| plot.stage == PlotStage::Growing)
        {
            continue;
        }
        let mut ripened = 0;
        let mut field = world.get_mut::<Field>(entity).expect("field");
        for plot in &mut field.plots {
            if plot.stage != PlotStage::Growing {
                continue;
            }
            let care = if now < plot.tended_at.saturating_add(crop.tend_interval_ticks * 2) {
                100
            } else {
                u64::from(crop.untended_growth_percent)
            };
            let grown = (GROWTH_ROUND_TICKS * percent * care / 10_000) as u32;
            plot.growth = plot.growth.saturating_add(grown);
            if plot.growth >= crop.growth_ticks {
                plot.growth = crop.growth_ticks;
                plot.stage = PlotStage::Ripe;
                ripened += 1;
            }
        }
        if ripened > 0 {
            record_effect(
                world,
                None,
                "VillageFieldRipe",
                &json!({"field":id,"plots":ripened,"crop":crop.key}),
            );
        }
    }
}

/// A blight destroys a seeded share of the growing plots of every field.
pub(super) fn blight(world: &mut World, day: u64) {
    let Some(loss) =
        economy_spec::registry(world).map(|registry| registry.spec.weather.blight_loss_percent)
    else {
        return;
    };
    let seed = world.resource::<WorldSeed>().0;
    let fields: Vec<(SimId, Entity)> = {
        let index = world.resource::<SimIndex>();
        world
            .get_resource::<FieldState>()
            .into_iter()
            .flat_map(|state| state.fields.values().flatten())
            .filter_map(|id| Some((*id, index.get(*id)?)))
            .collect()
    };
    for (id, entity) in fields {
        let Some(mut field) = world.get_mut::<Field>(entity) else {
            continue;
        };
        let mut lost = 0_u64;
        for (index, plot) in field.plots.iter_mut().enumerate() {
            if plot.stage == PlotStage::Fallow {
                continue;
            }
            let roll = sim_rand(seed, id, day * 64 + index as u64, BLIGHT_PURPOSE) % 100;
            if roll < u64::from(loss) {
                *plot = FieldPlot::fallow();
                lost += 1;
            }
        }
        field.lost_plots += lost;
        if lost > 0 {
            record_effect(
                world,
                None,
                "VillageFieldFallow",
                &json!({"field":id,"lost_plots":lost,"reason":"blight"}),
            );
        }
    }
}

/// Whether every plot of the settlement's fields is in use (or it has none):
/// more land, not more labour, is what a grain shortage then calls for.
pub fn land_is_short(world: &World, settlement: SimId) -> bool {
    fields_of(world, settlement).into_iter().all(|(_, entity)| {
        world.get::<Field>(entity).is_none_or(|field| {
            field
                .plots
                .iter()
                .all(|plot| plot.stage != PlotStage::Fallow)
        })
    })
}

/// Goods of the settlement's field crops: `(harvest good)`, ascending, unique.
pub fn crop_goods(world: &World) -> Vec<String> {
    let mut goods: Vec<String> = economy_spec::registry(world)
        .map(|registry| {
            registry
                .spec
                .crops
                .iter()
                .map(|crop| crop.harvest.good.clone())
                .collect()
        })
        .unwrap_or_default();
    goods.sort();
    goods.dedup();
    goods
}

/// One plot, for drawing and for panels.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PlotView {
    pub index: u32,
    /// `fallow`, `sown`, `growing` or `ripe`.
    pub stage: &'static str,
    /// Empty for a fallow plot.
    pub crop: String,
    /// 0 to 1000: how far the crop has grown.
    pub growth_permille: u32,
    pub sown_at: Option<u64>,
    /// The plot has been tended recently enough to grow at full speed.
    pub tended: bool,
    pub worker: Option<SimId>,
}

/// One field with its plots.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FieldView {
    pub id: SimId,
    pub label: String,
    pub settlement: SimId,
    pub plot: super::civic::Plot,
    pub door: SimPos,
    pub crop: String,
    pub crop_display: String,
    pub plots: Vec<PlotView>,
    pub seed_stock: u32,
    pub harvested_units: u64,
    pub lost_plots: u64,
    /// Fields lie fallow in winter.
    pub fallow_season: bool,
}

/// Every field with its plots, ascending by identity.
pub fn views(world: &World) -> Vec<FieldView> {
    let Some(state) = world.get_resource::<FieldState>() else {
        return Vec::new();
    };
    let now = world.resource::<SimClock>().tick;
    let winter = super::calendar(world).1 == Season::Winter;
    let mut ids: Vec<SimId> = state.fields.values().flatten().copied().collect();
    ids.sort_unstable();
    ids.into_iter()
        .filter_map(|id| {
            let entity = world.resource::<SimIndex>().get(id)?;
            let field = world.get::<Field>(entity)?;
            let building = world.get::<Building>(entity)?;
            let crop = crop(world, &field.crop);
            let total = crop.map_or(1, |crop| crop.growth_ticks.max(1));
            let interval = crop.map_or(u64::MAX / 4, |crop| crop.tend_interval_ticks);
            Some(FieldView {
                id,
                label: building.label.clone(),
                settlement: building.settlement,
                plot: building.plot,
                door: building.plot.door(),
                crop: field.crop.clone(),
                crop_display: crop.map_or_else(String::new, |crop| crop.display.clone()),
                plots: field
                    .plots
                    .iter()
                    .enumerate()
                    .map(|(index, plot)| {
                        let permille =
                            (u64::from(plot.growth) * 1000 / u64::from(total)).min(1000) as u32;
                        PlotView {
                            index: index as u32,
                            stage: match plot.stage {
                                PlotStage::Fallow => "fallow",
                                PlotStage::Ripe => "ripe",
                                PlotStage::Growing if permille < 150 => "sown",
                                PlotStage::Growing => "growing",
                            },
                            crop: if plot.stage == PlotStage::Fallow {
                                String::new()
                            } else {
                                field.crop.clone()
                            },
                            growth_permille: permille,
                            sown_at: (plot.stage != PlotStage::Fallow).then_some(plot.sown_at),
                            tended: plot.stage == PlotStage::Growing
                                && now < plot.tended_at.saturating_add(interval * 2),
                            worker: plot.claimed_by.filter(|_| plot.claimed_until > now),
                        }
                    })
                    .collect(),
                seed_stock: field.seed_stock,
                harvested_units: field.harvested_units,
                lost_plots: field.lost_plots,
                fallow_season: winter,
            })
        })
        .collect()
}

/// Checks the field index against the entities it names, after a load.
pub fn validate(world: &World) -> Result<(), String> {
    let Some(state) = world.get_resource::<FieldState>() else {
        return Ok(());
    };
    let index = world.resource::<SimIndex>();
    for (settlement, ids) in &state.fields {
        for id in ids {
            let entity = index.get(*id);
            let field = entity.and_then(|entity| world.get::<Field>(entity));
            let building = entity.and_then(|entity| world.get::<Building>(entity));
            if field.is_none() || building.is_none_or(|building| building.settlement != *settlement)
            {
                return Err(format!("Field {} is missing or misfiled", id.0));
            }
            if field.is_some_and(|field| crop(world, &field.crop).is_none()) {
                return Err(format!("Field {} grows an unknown crop", id.0));
            }
        }
    }
    let _ = world.get_resource::<CivicState>();
    Ok(())
}
