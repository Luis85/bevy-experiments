//! Shaping the living world (regional profile only): placing and removing
//! resource patches by recorded command, and exporting the current world as
//! world JSON in the shipped schema.
//!
//! - `PlaceResourcePatch {good, x, z, richness}`: a new patch of a good the
//!   economy's patch list knows, at `x, z` (millimetres), with
//!   `richness` percent (25 to 400) of the authored capacity. It belongs to
//!   the nearest settlement within 60 m of its centre.
//! - `RemoveResourcePatch {patch}`: removes a patch, unless it is the last
//!   one of its good in its settlement.
//! - `PlacePlot {plot, x, z, rotation}`: lays out a farm or garden plot (a
//!   catalog building with a `field`) at once, finished and empty, with its
//!   entrance at cell `x, z`, on the same ground rules as `PlaceBuilding` and
//!   never over a road. `RemovePlot {plot}` is its undo.
//! - `PlaceProp` / `RemoveProp`: see [`super::props`].
//! - [`export_world`]: the `living_world` block with the terrain, the patches,
//!   the buildings and plots standing now and the props. The host writes the
//!   file.
//!
//! Trees and rocks are not simulation data: they are view dressing drawn
//! from the terrain (`island_view::island_atmosphere`). Props that matter to
//! walking and building (barrels, fences, signposts) are; see
//! [`super::props`].
use super::ecology::{EcologicalPatch, UPDATE_TICKS};
use super::terrain::{TerrainState, TerrainSurface};
use super::world_spec::{PlacedBuilding, PlacedPatch, TerrainSpec};
use super::{LivingWorldSpec, SettlementMember, settlements};
use crate::village::ResourcePatch;
use crate::village::civic::{Building, CivicState, ConstructionSite};
use bevy_ecs::prelude::*;
use npc_framework::kernel::commands::CommandError;
use npc_framework::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::json;

/// A patch belongs to the nearest settlement whose centre is this near.
pub const PATCH_CATCHMENT_MM: i64 = 60_000;
/// Two patches stand at least this far apart.
const PATCH_SPACING_MM: i64 = 2_000;
/// Patches of one good one settlement may hold.
pub const MAX_PATCHES_PER_GOOD: usize = 4;

fn unavailable(reason: String) -> CommandError {
    CommandError::Unavailable { reason }
}

/// Patches of `kind` in `settlement`, ascending by identity.
pub fn patches_of(world: &World, settlement: SimId, kind: &str) -> Vec<(SimId, Entity)> {
    let mut out: Vec<(SimId, Entity)> = world
        .resource::<SimIndex>()
        .map
        .iter()
        .filter(|(_, entity)| {
            world
                .get::<ResourcePatch>(**entity)
                .is_some_and(|patch| patch.kind == kind)
                && settlements::member(world, **entity) == Some(settlement)
        })
        .map(|(id, entity)| (*id, *entity))
        .collect();
    out.sort_unstable_by_key(|(id, _)| *id);
    out
}

/// The settlement a new patch at `at` would belong to.
fn catchment(world: &World, at: SimPos) -> Option<SimId> {
    settlements::all(world)
        .into_iter()
        .filter_map(|(id, _)| {
            let centre = settlements::definition(world, id)?.center;
            let distance = centre.dist2(at);
            (distance <= PATCH_CATCHMENT_MM.pow(2)).then_some((distance, id))
        })
        .min()
        .map(|(_, id)| id)
}

/// Places a resource patch.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlaceResourcePatch {
    pub good: String,
    pub x: i32,
    pub z: i32,
    /// Percent of the authored capacity, 25 to 400.
    #[serde(default = "hundred")]
    pub richness: u32,
}
fn hundred() -> u32 {
    100
}
impl PlaceResourcePatch {
    /// `(settlement, capacity, gather ticks)` of the new patch.
    fn plan(&self, world: &World) -> Result<(SimId, u32, u32), String> {
        if !super::enabled(world) {
            return Err("PlaceResourcePatch requires the regional living world".into());
        }
        let def = super::economy_spec::registry(world)
            .and_then(|registry| {
                registry
                    .spec
                    .patches
                    .iter()
                    .find(|patch| patch.good == self.good)
            })
            .cloned()
            .ok_or_else(|| format!("{} does not grow in patches in this world", self.good))?;
        if !(25..=400).contains(&self.richness) || !self.richness.is_multiple_of(25) {
            return Err("richness must be 25 to 400 percent, in steps of 25".into());
        }
        let at = SimPos::new(self.x, self.z);
        let grid = world.resource::<SpatialGrid>();
        let cell = grid.cell_of(at);
        if !super::terrain::bounds(world).contains(cell) {
            return Err("the patch must be inside the map".into());
        }
        if super::terrain::cell(world, cell.x, cell.z).is_none_or(|terrain| !terrain.walkable()) {
            return Err("a patch cannot stand in water".into());
        }
        if grid.is_blocking(cell) {
            return Err("a structure stands there".into());
        }
        if world.resource::<SimIndex>().map.values().any(|entity| {
            world.get::<ResourcePatch>(*entity).is_some()
                && world
                    .get::<SimPos>(*entity)
                    .is_some_and(|pos| pos.dist2(at) < PATCH_SPACING_MM.pow(2))
        }) {
            return Err("another resource patch is within 2 m".into());
        }
        let settlement =
            catchment(world, at).ok_or("no settlement is within 60 m to work the patch")?;
        if patches_of(world, settlement, &self.good).len() >= MAX_PATCHES_PER_GOOD {
            return Err(format!(
                "a settlement holds at most {MAX_PATCHES_PER_GOOD} patches of one good"
            ));
        }
        let capacity = (u64::from(def.capacity) * u64::from(self.richness / 25) / 4).max(1) as u32;
        Ok((settlement, capacity, def.gather_ticks))
    }
    pub fn check(&self, world: &World) -> Result<(), String> {
        self.plan(world).map(|_| ())
    }
    pub fn perform(&self, world: &mut World) -> Result<SimId, String> {
        let (settlement, capacity, gather_ticks) = self.plan(world)?;
        let id = spawn_patch(
            world,
            &self.good,
            SimPos::new(self.x, self.z),
            settlement,
            capacity,
            gather_ticks,
        );
        record_effect(
            world,
            None,
            "RegionalResourcePatchPlaced",
            &json!({"patch":id,"good":self.good,"x":self.x,"z":self.z,"capacity":capacity,
                "settlement":settlement}),
        );
        Ok(id)
    }
}
impl ContentCommand for PlaceResourcePatch {
    const NAME: &'static str = "PlaceResourcePatch";
    fn validate(&self, world: &World) -> Result<(), CommandError> {
        self.check(world).map_err(unavailable)
    }
    fn apply(self, world: &mut World) {
        let _ = self.perform(world);
    }
}

/// Lays out a farm or garden plot: a catalog building with a `field`,
/// finished at once and empty, with its entrance at cell `(x, z)`.
///
/// `{"type": "PlacePlot", "plot": "garden", "x": 12, "z": -30, "rotation": 0}`
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlacePlot {
    pub plot: String,
    pub x: i32,
    pub z: i32,
    #[serde(default)]
    pub rotation: u8,
}
impl PlacePlot {
    pub fn check(&self, world: &World) -> Result<(), String> {
        if !super::enabled(world) {
            return Err("plots are placed only in the living world".into());
        }
        let def = crate::village::civic::definition(world, &self.plot)
            .filter(|def| def.field.is_some())
            .ok_or_else(|| {
                format!(
                    "`{}` is not a farm or garden plot of this catalog",
                    self.plot
                )
            })?;
        let body = crate::village::civic::plot_for(def, self.x, self.z, self.rotation);
        crate::village::civic::placement_check(world, &self.plot, self.x, self.z, self.rotation)?;
        if body.cells().any(|cell| {
            super::terrain::on_road(world, SimPos::new(cell.x * 1000 + 500, cell.z * 1000 + 500))
        }) {
            return Err("a plot cannot be laid over a road".into());
        }
        Ok(())
    }
    pub fn perform(&self, world: &mut World) -> Result<SimId, String> {
        self.check(world)?;
        let id =
            crate::village::civic::lay_out(world, &self.plot, self.x, self.z, self.rotation, true)?;
        let settlement = world
            .resource::<SimIndex>()
            .get(id)
            .and_then(|entity| world.get::<Building>(entity))
            .map(|building| building.settlement);
        record_effect(
            world,
            None,
            "LivingPlotPlaced",
            &json!({"plot":id,"building":self.plot,"x":self.x,"z":self.z,
                "rotation":self.rotation,"settlement":settlement}),
        );
        Ok(id)
    }
}
impl ContentCommand for PlacePlot {
    const NAME: &'static str = "PlacePlot";
    fn validate(&self, world: &World) -> Result<(), CommandError> {
        self.check(world).map_err(unavailable)
    }
    fn apply(self, world: &mut World) {
        let _ = self.perform(world);
    }
}

/// Removes a placed farm or garden plot: the undo of [`PlacePlot`]. Refused
/// for a founding field, and while the plot holds seed, goods or a crop, or
/// somebody works it.
///
/// `{"type": "RemovePlot", "plot": 812}`
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemovePlot {
    pub plot: SimId,
}
impl RemovePlot {
    pub fn check(&self, world: &World) -> Result<(), String> {
        if !super::enabled(world) {
            return Err("plots are placed only in the living world".into());
        }
        let building = world
            .resource::<SimIndex>()
            .get(self.plot)
            .and_then(|entity| world.get::<Building>(entity))
            .ok_or("that plot no longer exists")?;
        if crate::village::civic::definition(world, &building.building)
            .is_none_or(|def| def.field.is_none())
        {
            return Err("that is not a farm or garden plot".into());
        }
        if world
            .get_resource::<CivicState>()
            .is_some_and(|civic| civic.founded.binary_search(&self.plot).is_ok())
        {
            return Err("the founding fields stay".into());
        }
        crate::village::civic::removable(world, self.plot).map(|_| ())
    }
    pub fn perform(&self, world: &mut World) -> Result<(), String> {
        self.check(world)?;
        let building = world
            .resource::<SimIndex>()
            .get(self.plot)
            .and_then(|entity| world.get::<Building>(entity))
            .cloned()
            .expect("checked");
        crate::village::civic::remove_finished(world, self.plot)?;
        record_effect(
            world,
            None,
            "LivingPlotRemoved",
            &json!({"plot":self.plot,"building":building.building,"x":building.plot.x,
                "z":building.plot.z,"rotation":building.plot.rotation,
                "settlement":building.settlement}),
        );
        Ok(())
    }
}
impl ContentCommand for RemovePlot {
    const NAME: &'static str = "RemovePlot";
    fn validate(&self, world: &World) -> Result<(), CommandError> {
        self.check(world).map_err(unavailable)
    }
    fn apply(self, world: &mut World) {
        let _ = self.perform(world);
    }
}

/// Spawns a full resource patch with a fresh ecological history.
pub(super) fn spawn_patch(
    world: &mut World,
    good: &str,
    at: SimPos,
    settlement: SimId,
    capacity: u32,
    gather_ticks: u32,
) -> SimId {
    let now = world.resource::<SimClock>().tick;
    let (id, _) = spawn_sim(
        world,
        (
            at,
            SettlementMember { settlement },
            ResourcePatch {
                kind: good.into(),
                remaining: capacity,
                capacity,
                replenish_at: (now - now % UPDATE_TICKS) + UPDATE_TICKS,
                gather_ticks,
            },
            EcologicalPatch {
                last_update_tick: now - now % UPDATE_TICKS,
                ..EcologicalPatch::default()
            },
            Perceivable::new(crate::world::kinds::STATION),
        ),
    );
    id
}

/// Removes a resource patch.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemoveResourcePatch {
    pub patch: SimId,
}
impl RemoveResourcePatch {
    pub fn check(&self, world: &World) -> Result<(), String> {
        if !super::enabled(world) {
            return Err("RemoveResourcePatch requires the regional living world".into());
        }
        let entity = world
            .resource::<SimIndex>()
            .get(self.patch)
            .ok_or("that patch no longer exists")?;
        let patch = world
            .get::<ResourcePatch>(entity)
            .ok_or("that is not a resource patch")?;
        let settlement =
            settlements::member(world, entity).ok_or("the patch belongs to no settlement")?;
        if patches_of(world, settlement, &patch.kind).len() <= 1 {
            return Err(format!(
                "this is the settlement's last {} patch; place another first",
                patch.kind
            ));
        }
        Ok(())
    }
    pub fn perform(&self, world: &mut World) -> Result<(), String> {
        self.check(world)?;
        let entity = world
            .resource::<SimIndex>()
            .get(self.patch)
            .expect("checked");
        let kind = world
            .get::<ResourcePatch>(entity)
            .expect("checked")
            .kind
            .clone();
        // A gathering under way there ends without a yield.
        let gatherers: Vec<Entity> = world
            .query::<(Entity, &crate::village::Gathering)>()
            .iter(world)
            .filter(|(_, gathering)| gathering.patch == self.patch)
            .map(|(actor, _)| actor)
            .collect();
        for actor in gatherers {
            world
                .entity_mut(actor)
                .remove::<crate::village::Gathering>();
        }
        world.despawn(entity);
        crate::village::colony_demand_changed(world);
        record_effect(
            world,
            None,
            "RegionalResourcePatchRemoved",
            &json!({"patch":self.patch,"good":kind}),
        );
        Ok(())
    }
}
impl ContentCommand for RemoveResourcePatch {
    const NAME: &'static str = "RemoveResourcePatch";
    fn validate(&self, world: &World) -> Result<(), CommandError> {
        self.check(world).map_err(unavailable)
    }
    fn apply(self, world: &mut World) {
        let _ = self.perform(world);
    }
}

pub(super) fn surface_char(surface: TerrainSurface) -> char {
    match surface {
        TerrainSurface::Grass => 'g',
        TerrainSurface::Soil => 's',
        TerrainSurface::Stone => 't',
        TerrainSurface::Water => 'w',
    }
}
pub(super) fn surface_of(c: char) -> Option<TerrainSurface> {
    Some(match c {
        'g' => TerrainSurface::Grass,
        's' => TerrainSurface::Soil,
        't' => TerrainSurface::Stone,
        'w' => TerrainSurface::Water,
        _ => return None,
    })
}

/// The current world as the `living_world` block of world JSON in the
/// shipped schema: the authored construction and economy, plus the terrain
/// as it is now, every resource patch and every building and site placed
/// after the founding. `None` outside the regional profile.
pub fn export_spec(world: &World) -> Option<LivingWorldSpec> {
    let mut spec = super::spec(world)?.clone();
    if let Some(terrain) = world.get_resource::<TerrainState>() {
        spec.terrain = Some(TerrainSpec {
            heights: terrain.cells.iter().map(|cell| cell.height_mm).collect(),
            surfaces: terrain
                .cells
                .iter()
                .map(|cell| surface_char(cell.surface))
                .collect(),
        });
    }
    let key = |settlement: SimId| {
        settlements::definition(world, settlement)
            .map(|definition| definition.key.clone())
            .unwrap_or_default()
    };
    let mut patches: Vec<(SimId, PlacedPatch)> = world
        .resource::<SimIndex>()
        .map
        .iter()
        .filter_map(|(id, entity)| {
            let patch = world.get::<ResourcePatch>(*entity)?;
            let pos = world.get::<SimPos>(*entity)?;
            Some((
                *id,
                PlacedPatch {
                    good: patch.kind.clone(),
                    x: pos.x,
                    z: pos.z,
                    capacity: patch.capacity,
                    settlement: key(settlements::member(world, *entity)?),
                },
            ))
        })
        .collect();
    patches.sort_unstable_by_key(|(id, _)| *id);
    spec.resource_patches = patches.into_iter().map(|(_, patch)| patch).collect();
    let mut buildings: Vec<(SimId, PlacedBuilding)> = world
        .resource::<SimIndex>()
        .map
        .iter()
        .filter_map(|(id, entity)| {
            if let Some(building) = world.get::<Building>(*entity) {
                // The founding fields and the trading post are laid out by
                // the simulation at its first ticks; it lays them out again.
                let founded = world
                    .get_resource::<CivicState>()
                    .is_some_and(|civic| civic.founded.binary_search(id).is_ok());
                (!founded).then(|| {
                    (
                        *id,
                        PlacedBuilding {
                            building: building.building.clone(),
                            x: building.plot.x,
                            z: building.plot.z,
                            rotation: building.plot.rotation,
                            finished: true,
                        },
                    )
                })
            } else {
                let site = world.get::<ConstructionSite>(*entity)?;
                Some((
                    *id,
                    PlacedBuilding {
                        building: site.building.clone(),
                        x: site.plot.x,
                        z: site.plot.z,
                        rotation: site.plot.rotation,
                        finished: false,
                    },
                ))
            }
        })
        .collect();
    buildings.sort_unstable_by_key(|(id, _)| *id);
    spec.buildings = buildings
        .into_iter()
        .map(|(_, building)| building)
        .collect();
    spec.props = super::props::export(world);
    Some(spec)
}

/// [`export_spec`] as world JSON text: `{"living_world": {...}}`. Merge it
/// into the world document it came from (name, scene and the other blocks
/// are host data) before writing the file.
pub fn export_world(world: &World) -> String {
    let living = export_spec(world).map_or(serde_json::Value::Null, |spec| {
        serde_json::to_value(spec).unwrap_or_default()
    });
    serde_json::to_string_pretty(&json!({ "living_world": living })).unwrap_or_default()
}
