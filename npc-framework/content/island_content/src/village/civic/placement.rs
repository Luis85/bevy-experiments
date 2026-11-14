//! Where a building may stand: one pure query shared by the view's placement
//! ghost, the recorded command and the civic planner.
use super::model::{Building, CivicState, ConstructionSite, Goods, Plot};
use crate::village::frontier::building_spec::{BuildSurface, BuildingDef};
use crate::village::frontier::terrain::{self, TerrainSurface};
use crate::village::frontier::{Settlement, SettlementMember, economy_spec};
use crate::village::{ResourcePatch, VillagePlace, colony::Household, homestead};
use bevy_ecs::prelude::*;
use npc_framework::kernel::commands::CommandError;
use npc_framework::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// A building belongs to the nearest settlement and must stand this close to it.
pub const MAX_SETTLEMENT_DISTANCE_MM: i32 = 48_000;
/// Open sites one settlement may have at a time.
pub const MAX_OPEN_SITES: usize = 4;
/// Cells kept free around a resource site.
const PATCH_CLEARANCE: i32 = 3;
/// A waterfront building stands this many cells from the sea at most.
pub const COAST_REACH_CELLS: i32 = 6;

/// Whether a cell is open sea: water at the depth of the sea (ponds and
/// wetland pools are shallow).
pub fn sea(world: &World, x: i32, z: i32) -> bool {
    terrain::cell(world, x, z)
        .is_some_and(|cell| cell.surface == TerrainSurface::Water && cell.height_mm <= -500)
}

fn near_sea(world: &World, plot: &Plot) -> bool {
    let reach = COAST_REACH_CELLS;
    (plot.lo_z - reach..=plot.hi_z + reach)
        .any(|z| (plot.lo_x - reach..=plot.hi_x + reach).any(|x| sea(world, x, z)))
}

/// The body of `def` with its entrance at cell `(x, z)` after `rotation`
/// quarter turns: towards +z, +x, -z or -x of the entrance.
pub fn plot_for(def: &BuildingDef, x: i32, z: i32, rotation: u8) -> Plot {
    let width = def.footprint.width as i32;
    let depth = def.footprint.depth as i32;
    let (left, right) = ((width - 1) / 2, width / 2);
    let rotation = rotation % 4;
    let (lo_x, lo_z, hi_x, hi_z) = match rotation {
        0 => (x - left, z + 1, x + right, z + depth),
        1 => (x + 1, z - left, x + depth, z + right),
        2 => (x - right, z - depth, x + left, z - 1),
        _ => (x - depth, z - right, x - 1, z + left),
    };
    Plot {
        x,
        z,
        rotation,
        lo_x,
        lo_z,
        hi_x,
        hi_z,
        solid: def.solid,
    }
}

/// Everything already on the ground that a new building must respect. Built
/// once for a batch of candidate checks.
pub struct Survey {
    /// Cells of other bodies and reserved plots, with what holds them.
    reserved: BTreeMap<CellCoord, &'static str>,
    /// Solid bodies whose blocker may not be in the grid yet.
    solid: BTreeSet<CellCoord>,
    /// Entrances and public points that must stay open.
    entrances: BTreeSet<CellCoord>,
    /// Cells where somebody stands or something lies right now. Read from the
    /// current positions, not the spatial index, which is one move behind.
    occupied: BTreeSet<CellCoord>,
    patches: Vec<CellCoord>,
    settlements: Vec<(SimId, SimPos)>,
    open_sites: BTreeMap<SimId, usize>,
    /// Open prop sites, which have their own allowance.
    prop_sites: BTreeMap<SimId, usize>,
}

fn cell_of(pos: SimPos) -> CellCoord {
    CellCoord::new(pos.x.div_euclid(1000), pos.z.div_euclid(1000))
}

impl Survey {
    pub fn new(world: &World) -> Self {
        let mut survey = Self {
            reserved: BTreeMap::new(),
            solid: BTreeSet::new(),
            entrances: BTreeSet::new(),
            occupied: BTreeSet::new(),
            patches: Vec::new(),
            settlements: Vec::new(),
            open_sites: BTreeMap::new(),
            prop_sites: BTreeMap::new(),
        };
        if let Some(spec) = crate::village::frontier::spec(world) {
            survey.entrances.insert(cell_of(spec.actor_start));
        }
        for (id, entity) in &world.resource::<SimIndex>().map {
            let Some(pos) = world.get::<SimPos>(*entity).copied() else {
                continue;
            };
            if let Some(settlement) = world.get::<Settlement>(*entity) {
                survey.settlements.push((*id, settlement.center));
                survey.entrances.insert(cell_of(settlement.center));
            }
            if world.get::<VillagePlace>(*entity).is_some() {
                survey.entrances.insert(cell_of(pos));
            }
            if world.get::<crate::village::Villager>(*entity).is_some()
                || world
                    .get::<crate::village::colony::AwaitingArrival>(*entity)
                    .is_some()
                || world
                    .get::<crate::village::hunting::Wildlife>(*entity)
                    .is_some()
                || world
                    .get::<npc_framework::interaction::Item>(*entity)
                    .is_some()
            {
                survey.occupied.insert(cell_of(pos));
            }
            if let Some(home) = world.get::<Household>(*entity) {
                survey.entrances.insert(cell_of(home.door));
            }
            if world.get::<ResourcePatch>(*entity).is_some()
                || world
                    .get::<crate::village::hunting::WildlifeHabitat>(*entity)
                    .is_some()
            {
                survey.patches.push(cell_of(pos));
            }
            if world.get::<homestead::HomesteadPlot>(*entity).is_some()
                && let Some(definition) = homestead::definition(world, *entity)
            {
                let lo = cell_of(SimPos::new(
                    definition.x - definition.width_mm / 2,
                    definition.z - definition.depth_mm / 2,
                ));
                let hi = cell_of(SimPos::new(
                    definition.x + definition.width_mm / 2,
                    definition.z + definition.depth_mm / 2,
                ));
                for z in lo.z..=hi.z {
                    for x in lo.x..=hi.x {
                        survey
                            .reserved
                            .insert(CellCoord::new(x, z), "a reserved homestead plot");
                    }
                }
            }
            // A placed prop holds its cells like a building.
            if let Some(prop) = world.get::<crate::village::frontier::props::Prop>(*entity) {
                for cell in prop.plot.cells() {
                    survey.reserved.insert(cell, "a prop");
                    if prop.plot.solid {
                        survey.solid.insert(cell);
                    }
                }
            }
            let plot = world
                .get::<ConstructionSite>(*entity)
                .map(|site| {
                    let open = if site.is_prop() {
                        &mut survey.prop_sites
                    } else {
                        &mut survey.open_sites
                    };
                    *open.entry(site.settlement).or_default() += 1;
                    site.plot
                })
                .or_else(|| world.get::<Building>(*entity).map(|building| building.plot));
            if let Some(plot) = plot {
                survey.entrances.insert(CellCoord::new(plot.x, plot.z));
                for cell in plot.cells() {
                    survey.reserved.insert(cell, "another building");
                    if plot.solid {
                        survey.solid.insert(cell);
                    }
                }
            }
        }
        survey
    }

    /// The settlement a building at `door` would belong to.
    fn settlement(&self, door: SimPos) -> Option<(SimId, i64)> {
        self.settlements
            .iter()
            .map(|(id, center)| (*id, center.dist2(door)))
            .min_by_key(|(id, distance)| (*distance, *id))
    }

    /// Every rule except connectivity. Cheap enough to scan candidates with.
    pub fn check_fast(
        &self,
        world: &World,
        def: &BuildingDef,
        x: i32,
        z: i32,
        rotation: u8,
    ) -> Result<(Plot, SimId), String> {
        let (plot, settlement) = self.check_ground(world, def, x, z, rotation)?;
        if self.open_sites.get(&settlement).copied().unwrap_or(0) >= MAX_OPEN_SITES {
            return Err(
                "Four building sites are already open here. Wait for one to finish or cancel one."
                    .to_string(),
            );
        }
        Ok((plot, settlement))
    }

    /// [`Self::check_fast`] without the open-site allowance: the ground,
    /// the neighbours and the settlement in reach (props use it too).
    pub fn check_ground(
        &self,
        world: &World,
        def: &BuildingDef,
        x: i32,
        z: i32,
        rotation: u8,
    ) -> Result<(Plot, SimId), String> {
        cell_on_map(world, x, z)?;
        if rotation > 3 {
            return Err("rotation must be 0..=3 quarter turns".into());
        }
        let plot = plot_for(def, x, z, rotation);
        let grid = world.resource::<SpatialGrid>();
        let bounds = terrain::bounds(world);
        let door = CellCoord::new(x, z);
        if !bounds.contains(door) || plot.cells().any(|cell| !bounds.contains(cell)) {
            return Err("the footprint leaves the map".into());
        }
        let name = &def.display;
        let Some(ground) = terrain::cell(world, x, z) else {
            return Err("the entrance is outside the terrain".into());
        };
        if !ground.walkable() {
            return Err("the entrance is on water".into());
        }
        if grid.is_blocking(door) || self.solid.contains(&door) {
            return Err("the entrance is blocked by a building".into());
        }
        if let Some(holder) = self.reserved.get(&door) {
            return Err(format!("the entrance lies inside {holder}"));
        }
        let (mut low, mut high) = (ground.height_mm, ground.height_mm);
        for cell in plot.cells() {
            let Some(value) = terrain::cell(world, cell.x, cell.z) else {
                return Err("the footprint leaves the terrain".into());
            };
            let surface = match value.surface {
                TerrainSurface::Grass => BuildSurface::Grass,
                TerrainSurface::Soil => BuildSurface::Soil,
                TerrainSurface::Stone => BuildSurface::Stone,
                TerrainSurface::Water => {
                    return Err(format!(
                        "{name} cannot stand on water at [{}, {}]",
                        cell.x, cell.z
                    ));
                }
            };
            if !def.placement.surfaces.contains(&surface) {
                let ground = match surface {
                    BuildSurface::Grass => "grass",
                    BuildSurface::Soil => "a road or bare soil",
                    BuildSurface::Stone => "stone",
                };
                return Err(format!(
                    "{name} cannot stand on {ground} at [{}, {}]",
                    cell.x, cell.z
                ));
            }
            low = low.min(value.height_mm);
            high = high.max(value.height_mm);
        }
        if (high - low) as u32 > def.placement.max_slope_mm {
            return Err(format!(
                "the ground is too steep: {} mm across the footprint, at most {} mm",
                high - low,
                def.placement.max_slope_mm
            ));
        }
        let margin = def.placement.clearance as i32;
        for cz in plot.lo_z - margin..=plot.hi_z + margin {
            for cx in plot.lo_x - margin..=plot.hi_x + margin {
                let cell = CellCoord::new(cx, cz);
                let inside = plot.contains(cx, cz);
                if let Some(holder) = self.reserved.get(&cell) {
                    return Err(if inside {
                        format!("the footprint overlaps {holder}")
                    } else {
                        format!("too close to {holder}; keep {margin} cell(s) free")
                    });
                }
                let built = bounds
                    .index(cell)
                    .is_some_and(|index| grid.blockers_at(index) > 0);
                if built {
                    return Err(if inside {
                        "the footprint overlaps a solid building".into()
                    } else {
                        format!("too close to a solid building; keep {margin} cell(s) free")
                    });
                }
                if cell != door && self.entrances.contains(&cell) {
                    return Err(if inside {
                        "the footprint covers an entrance or a public place".into()
                    } else {
                        "too close to an entrance or a public place".into()
                    });
                }
            }
        }
        if self.patches.iter().any(|patch| {
            patch.x >= plot.lo_x - PATCH_CLEARANCE
                && patch.x <= plot.hi_x + PATCH_CLEARANCE
                && patch.z >= plot.lo_z - PATCH_CLEARANCE
                && patch.z <= plot.hi_z + PATCH_CLEARANCE
        }) {
            return Err("too close to a resource site".into());
        }
        if plot.solid {
            for cell in plot.cells() {
                if self.occupied.contains(&cell) {
                    return Err(format!(
                        "somebody or something is in the way at [{}, {}]",
                        cell.x, cell.z
                    ));
                }
            }
        }
        let Some((settlement, distance)) = self.settlement(plot.door()) else {
            return Err("there is no settlement to build for".into());
        };
        if distance > i64::from(MAX_SETTLEMENT_DISTANCE_MM).pow(2)
            && !(def.placement.coast && near_sea(world, &plot))
        {
            return Err(format!(
                "too far from any settlement; build within {} m of a centre",
                MAX_SETTLEMENT_DISTANCE_MM / 1000
            ));
        }
        Ok((plot, settlement))
    }

    /// The entrance must be reachable on foot from the arrival point, and a
    /// solid body must not cut off any ground that was reachable before.
    pub fn check_reachable(&self, world: &World, plot: &Plot) -> Result<(), String> {
        let grid = world.resource::<SpatialGrid>();
        let bounds = terrain::bounds(world);
        let Some(spec) = crate::village::frontier::spec(world) else {
            return Err("placement needs the regional living world".into());
        };
        let origin = cell_of(spec.actor_start);
        let flood = |with_body: bool| -> Vec<bool> {
            let mut reached = vec![false; bounds.len()];
            let passable = |cell: CellCoord| {
                bounds.contains(cell)
                    && !grid.is_blocking(cell)
                    && !self.solid.contains(&cell)
                    && !(with_body && plot.solid && plot.contains(cell.x, cell.z))
            };
            let mut queue = VecDeque::new();
            if passable(origin)
                && let Some(index) = bounds.index(origin)
            {
                reached[index as usize] = true;
                queue.push_back(origin);
            }
            while let Some(cell) = queue.pop_front() {
                for (dx, dz) in [(0, -1), (-1, 0), (1, 0), (0, 1)] {
                    let next = CellCoord::new(cell.x + dx, cell.z + dz);
                    if passable(next)
                        && let Some(index) = bounds.index(next)
                        && !reached[index as usize]
                    {
                        reached[index as usize] = true;
                        queue.push_back(next);
                    }
                }
            }
            reached
        };
        let after = flood(true);
        let door = bounds
            .index(CellCoord::new(plot.x, plot.z))
            .ok_or("the entrance is outside the map")?;
        if !after[door as usize] {
            return Err("the entrance cannot be reached on foot from the settlements".into());
        }
        if plot.solid {
            let before = flood(false);
            for (index, was) in before.iter().enumerate() {
                let cell = bounds.coord(index as u32);
                if *was && !after[index] && !plot.contains(cell.x, cell.z) {
                    return Err(format!(
                        "the building would cut off the ground at [{}, {}]",
                        cell.x, cell.z
                    ));
                }
            }
        }
        Ok(())
    }
}

fn definition<'a>(world: &'a World, building: &str) -> Result<&'a BuildingDef, String> {
    let registry =
        economy_spec::registry(world).ok_or("building requires the regional living world")?;
    registry.spec.building(building).ok_or_else(|| {
        format!(
            "unknown building `{building}`; known buildings: {}",
            registry
                .spec
                .buildings
                .iter()
                .map(|def| def.id.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )
    })
}

/// The cell `(x, z)` when it lies on the island's map; checked before any
/// arithmetic on a commanded cell (`plot_for` is safe after it: the map and
/// every footprint are small).
pub fn cell_on_map(world: &World, x: i32, z: i32) -> Result<CellCoord, String> {
    let cell = CellCoord::new(x, z);
    if terrain::bounds(world).contains(cell) {
        Ok(cell)
    } else {
        Err("That spot is outside the island.".into())
    }
}

/// Pure placement test for the view's ghost, the command and the planner:
/// `Ok` when `building` may be placed with its entrance at cell `(x, z)`,
/// otherwise the reason in words. Never changes the world.
pub fn placement_check(
    world: &World,
    building: &str,
    x: i32,
    z: i32,
    rotation: u8,
) -> Result<(), String> {
    let def = definition(world, building)?;
    let survey = Survey::new(world);
    let (plot, _) = survey.check_fast(world, def, x, z, rotation)?;
    survey.check_reachable(world, &plot)
}

/// The ground rules of a building (or a prop described as one) without the
/// open-site allowance: terrain, neighbours, the settlement in reach and
/// reachability. `Ok` gives the body and the settlement.
pub fn ground_check(
    world: &World,
    def: &BuildingDef,
    x: i32,
    z: i32,
    rotation: u8,
) -> Result<(Plot, SimId), String> {
    let survey = Survey::new(world);
    let (plot, settlement) = survey.check_ground(world, def, x, z, rotation)?;
    survey.check_reachable(world, &plot)?;
    Ok((plot, settlement))
}

/// The body a building would cover, for drawing a ghost. `None` for an
/// unknown building.
pub fn placement_plot(world: &World, building: &str, x: i32, z: i32, rotation: u8) -> Option<Plot> {
    cell_on_map(world, x, z).ok()?;
    definition(world, building)
        .ok()
        .map(|def| plot_for(def, x, z, rotation))
}

/// Opens a construction site. Shared by the command and the planner; the
/// caller's validation is repeated here, so a stale request changes nothing.
pub(crate) fn place(
    world: &mut World,
    building: &str,
    x: i32,
    z: i32,
    rotation: u8,
    civic: bool,
    reason: &str,
) -> Result<SimId, String> {
    place_site(world, building, x, z, rotation, civic, reason, true)
}

/// [`place`]; a founding building is not announced as a new site.
#[allow(clippy::too_many_arguments)]
pub(super) fn place_site(
    world: &mut World,
    building: &str,
    x: i32,
    z: i32,
    rotation: u8,
    civic: bool,
    reason: &str,
    announce: bool,
) -> Result<SimId, String> {
    let def = definition(world, building)?.clone();
    let survey = Survey::new(world);
    let (plot, settlement) = survey.check_fast(world, &def, x, z, rotation)?;
    survey.check_reachable(world, &plot)?;
    Ok(open_site(
        world,
        &def,
        plot,
        settlement,
        civic,
        reason,
        announce,
        super::model::SiteKind::Building,
    ))
}

/// Prop sites one settlement may have open at a time.
pub const MAX_OPEN_PROP_SITES: usize = 8;

/// Opens the construction site of a prop (described as a building, see
/// `props::as_building`), its entrance at `(x, z)`: the prop's own ground
/// rules, the prop-site allowance, then the ordinary site path.
pub(crate) fn place_prop_site(
    world: &mut World,
    def: &BuildingDef,
    x: i32,
    z: i32,
    rotation: u8,
    kind: super::model::SiteKind,
) -> Result<SimId, String> {
    let survey = Survey::new(world);
    let (plot, settlement) = survey.check_ground(world, def, x, z, rotation)?;
    survey.check_reachable(world, &plot)?;
    if survey.prop_sites.get(&settlement).copied().unwrap_or(0) >= MAX_OPEN_PROP_SITES {
        return Err(
            "Eight small things already wait to be built here. Let the builders catch up first."
                .into(),
        );
    }
    Ok(open_site(
        world,
        def,
        plot,
        settlement,
        false,
        "laid out by the player",
        true,
        kind,
    ))
}

/// Spawns a construction site on a checked plot.
#[allow(clippy::too_many_arguments)]
fn open_site(
    world: &mut World,
    def: &BuildingDef,
    plot: Plot,
    settlement: SimId,
    civic: bool,
    reason: &str,
    announce: bool,
    kind: super::model::SiteKind,
) -> SimId {
    let (building, x, z, rotation) = (def.id.as_str(), plot.x, plot.z, plot.rotation);
    let place = world
        .resource::<SimIndex>()
        .get(settlement)
        .and_then(|entity| world.get::<Settlement>(entity))
        .map_or_else(String::new, |settlement| settlement.label.clone());
    let state = world.resource::<CivicState>();
    let same = state
        .sites
        .get(&settlement)
        .into_iter()
        .flatten()
        .chain(state.buildings.get(&settlement).into_iter().flatten())
        .filter(|id| {
            world
                .resource::<SimIndex>()
                .get(**id)
                .is_some_and(|entity| {
                    world
                        .get::<ConstructionSite>(entity)
                        .map(|site| site.building.as_str())
                        .or_else(|| {
                            world
                                .get::<Building>(entity)
                                .map(|building| building.building.as_str())
                        })
                        == Some(building)
                })
        })
        .count();
    let label = format!("{place} {} {}", def.display.to_lowercase(), same + 1);
    let blocker = plot.solid.then(|| {
        let width = plot.hi_x - plot.lo_x + 1;
        let depth = plot.hi_z - plot.lo_z + 1;
        let footprint = Footprint {
            half_x_mm: width * 500 - 1,
            half_z_mm: depth * 500 - 1,
        };
        spawn_sim(
            world,
            (
                SimPos::new(
                    (plot.lo_x + plot.hi_x + 1) * 500,
                    (plot.lo_z + plot.hi_z + 1) * 500,
                ),
                GridBlocker(footprint),
                GridOccluder(footprint),
            ),
        )
        .0
    });
    let cost: Goods = def
        .cost
        .iter()
        .map(|stack| (stack.good.clone(), stack.count))
        .collect();
    let now = world.resource::<SimClock>().tick;
    let (site, _) = spawn_sim(
        world,
        (
            plot.door(),
            ConstructionSite {
                building: def.id.clone(),
                label: label.clone(),
                settlement,
                plot,
                blocker,
                cost: cost.clone(),
                delivered: Goods::new(),
                incoming: BTreeMap::new(),
                work_ticks: def.work_ticks,
                work_done: 0,
                placed_at: now,
                civic,
                reason: reason.into(),
                paid: 0,
                builder: None,
                builder_until: 0,
                last_delivery_at: None,
                kind,
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
    if !announce {
        return site;
    }
    // Whoever placed it, the village pays: raise what the fund lacks now, so
    // the site is supplied without waiting for the next levy day.
    super::sites::raise_building_levy(world);
    crate::village::colony_demand_changed(world);
    record_effect(
        world,
        None,
        "VillageSitePlaced",
        &json!({"site":site,"building":def.id,"label":label,"settlement":settlement,
            "cell":[x,z],"rotation":rotation,"civic":civic,"reason":reason,"cost":cost,
            "work_ticks":def.work_ticks}),
    );
    site
}

/// Recorded placement of a building by the overseer or a player:
/// `{"type":"PlaceBuilding","building":"cottage","x":12,"z":-34,"rotation":0}`.
/// `x`, `z` are the entrance cell (metres); `rotation` is 0..=3 quarter turns.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlaceBuilding {
    pub building: String,
    pub x: i32,
    pub z: i32,
    #[serde(default)]
    pub rotation: u8,
}
impl PlaceBuilding {
    pub fn check(&self, world: &World) -> Result<(), String> {
        if !world.contains_resource::<CivicState>() {
            return Err("PlaceBuilding requires the regional living world".into());
        }
        placement_check(world, &self.building, self.x, self.z, self.rotation)
    }
    /// Returns the new site's identity.
    pub fn perform(&self, world: &mut World) -> Result<SimId, String> {
        self.check(world)?;
        place(
            world,
            &self.building,
            self.x,
            self.z,
            self.rotation,
            false,
            "placed by the overseer",
        )
    }
}
impl ContentCommand for PlaceBuilding {
    const NAME: &'static str = "PlaceBuilding";
    fn validate(&self, world: &World) -> Result<(), CommandError> {
        self.check(world)
            .map_err(|reason| CommandError::Unavailable { reason })
    }
    fn apply(self, world: &mut World) {
        let _ = self.perform(world);
    }
}

/// Recorded cancellation of an unfinished site: `{"type":"CancelBuilding","site":812}`.
/// Delivered materials go to a nearby store, or onto the ground at the entrance.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CancelBuilding {
    pub site: SimId,
}
impl CancelBuilding {
    pub fn check(&self, world: &World) -> Result<(), String> {
        world
            .get_resource::<SimIndex>()
            .and_then(|index| index.get(self.site))
            .and_then(|entity| world.get::<ConstructionSite>(entity))
            .map(|_| ())
            .ok_or_else(|| "that is not an unfinished building site".into())
    }
    pub fn perform(&self, world: &mut World) -> Result<(), String> {
        self.check(world)?;
        super::sites::cancel(world, self.site, "cancelled by the overseer");
        Ok(())
    }
}
impl ContentCommand for CancelBuilding {
    const NAME: &'static str = "CancelBuilding";
    fn validate(&self, world: &World) -> Result<(), CommandError> {
        self.check(world)
            .map_err(|reason| CommandError::Unavailable { reason })
    }
    fn apply(self, world: &mut World) {
        let _ = self.perform(world);
    }
}
