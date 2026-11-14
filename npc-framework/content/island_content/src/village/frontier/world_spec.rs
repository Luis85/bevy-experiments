//! Frozen, editable construction data for the regional island profile.
use crate::village::Role;
use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use npc_framework::kernel::hash::SimHashResource;
use npc_framework::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
#[serde(rename_all = "snake_case")]
pub enum Biome {
    Coast,
    Meadow,
    Woodland,
    Ridge,
    Wetland,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SiteDef {
    pub key: String,
    pub label: String,
    pub role: Role,
    pub position: SimPos,
    pub capacity: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HomeDef {
    pub key: String,
    pub label: String,
    pub door: SimPos,
    pub capacity: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SettlementDef {
    pub key: String,
    pub label: String,
    pub center: SimPos,
    pub biome: Biome,
    pub sites: Vec<SiteDef>,
    pub homes: Vec<HomeDef>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlotDef {
    pub id: String,
    pub label: String,
    pub x: i32,
    pub z: i32,
    pub width_mm: i32,
    pub depth_mm: i32,
    pub use_kind: String,
}
impl PlotDef {
    pub fn as_expansion(&self) -> crate::village::geography::ExpansionPlot<'_> {
        crate::village::geography::ExpansionPlot {
            id: &self.id,
            label: &self.label,
            x: self.x,
            z: self.z,
            width_mm: self.width_mm,
            depth_mm: self.depth_mm,
            use_kind: &self.use_kind,
        }
    }
}
#[derive(Resource, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LivingWorldSpec {
    pub version: u32,
    pub width_cells: u32,
    pub height_cells: u32,
    pub actor_start: SimPos,
    pub settlements: Vec<SettlementDef>,
    pub plots: Vec<PlotDef>,
    /// Goods, recipes, professions, patches, endowments and tuning. A file
    /// without this block gets the built-in economy.
    pub economy: super::economy_spec::EconomySpec,
    /// Resident behaviour trees, traits, schedule and places. A file without
    /// this block gets the built-in behaviour.
    pub behavior: crate::village::resident::BehaviorSpec,
    /// An exported world's terrain; without it the terrain is generated.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terrain: Option<TerrainSpec>,
    /// An exported world's resource patches; without them every settlement
    /// gets the economy's patch list.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub resource_patches: Vec<PlacedPatch>,
    /// Buildings and sites placed after the founding, laid out again when
    /// the world is built.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub buildings: Vec<PlacedBuilding>,
    /// Props placed with the Shape tools, laid out again when the world is
    /// built.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub props: Vec<PlacedProp>,
}

/// Heights (millimetres) and surfaces of every cell, row by row from the
/// map's lowest corner: `g` grass, `s` soil, `t` stone, `w` water.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TerrainSpec {
    pub heights: Vec<i32>,
    pub surfaces: String,
}

/// One resource patch of an exported world.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlacedPatch {
    pub good: String,
    pub x: i32,
    pub z: i32,
    pub capacity: u32,
    /// Key of the settlement that works it.
    pub settlement: String,
}

/// One building or site of an exported world: the catalogue key, its
/// entrance cell and quarter turns, and whether it stands finished.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlacedBuilding {
    pub building: String,
    pub x: i32,
    pub z: i32,
    pub rotation: u8,
    pub finished: bool,
}
/// One placed prop of an exported world: the catalog key, its cell and
/// quarter turns (the `PlaceProp` command's values).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlacedProp {
    pub prop: String,
    pub x: i32,
    pub z: i32,
    pub rotation: u8,
}
impl Default for LivingWorldSpec {
    fn default() -> Self {
        let settlements = [
            ("harbour", "Harbour", SimPos::new(0, -30000), Biome::Coast),
            (
                "alderwood",
                "Alderwood",
                SimPos::new(-65000, 40000),
                Biome::Woodland,
            ),
            (
                "stone_ridge",
                "Stone Ridge",
                SimPos::new(65000, 40000),
                Biome::Ridge,
            ),
        ]
        .into_iter()
        .map(|(key, label, center, biome)| {
            let roles: Vec<_> = Role::RESIDENTS
                .into_iter()
                .chain([Role::Hunter, Role::Tanner, Role::Tailor, Role::Fisher])
                .collect();
            let sites = roles
                .into_iter()
                .enumerate()
                .map(|(i, role)| SiteDef {
                    key: format!("work_{}", role.tag()),
                    label: format!("{label} {}", role.name()),
                    role,
                    position: if biome == Biome::Coast {
                        SimPos::new(
                            center.x - 30000 + (i as i32 % 7) * 10000,
                            center.z + if i < 7 { -8000 } else { 8000 },
                        )
                    } else if i == 12 {
                        SimPos::new(center.x + 26000, center.z - 8000)
                    } else if biome == Biome::Woodland {
                        SimPos::new(
                            center.x + 12000 - (i as i32 / 4) * 8000,
                            center.z - 18000 + (i as i32 % 4) * 12000,
                        )
                    } else if biome == Biome::Ridge {
                        let row = i as i32 / 4;
                        SimPos::new(
                            center.x - 18000
                                + (i as i32 % 4) * 12000
                                + [0, 4000, -4000][row as usize],
                            center.z - 12000 + row * 12000,
                        )
                    } else {
                        SimPos::new(
                            center.x - 18000 + (i as i32 % 4) * 12000,
                            center.z - 8000 + (i as i32 / 4) * 8000,
                        )
                    },
                    capacity: 2,
                })
                .collect();
            let homes = (0..14)
                .map(|i| HomeDef {
                    key: format!("home_{i:02}"),
                    label: format!("{label} cottage {}", i + 1),
                    door: if matches!(biome, Biome::Woodland | Biome::Ridge) {
                        let edge = if biome == Biome::Ridge { 30000 } else { 24000 };
                        SimPos::new(
                            center.x + if i < 7 { -edge } else { edge },
                            center.z - 24000 + (i % 7) * 8000,
                        )
                    } else {
                        SimPos::new(
                            center.x - 24000 + (i % 7) * 8000,
                            center.z + if i < 7 { -20000 } else { 21000 },
                        )
                    },
                    capacity: 2,
                })
                .collect();
            SettlementDef {
                key: key.into(),
                label: label.into(),
                center,
                biome,
                sites,
                homes,
            }
        })
        .collect();
        let plots = crate::village::geography::regional_plots()
            .iter()
            .map(|p| PlotDef {
                id: p.id.into(),
                label: p.label.into(),
                x: p.x,
                z: p.z,
                width_mm: p.width_mm,
                depth_mm: p.depth_mm,
                use_kind: p.use_kind.into(),
            })
            .collect();
        Self {
            version: 1,
            width_cells: 256,
            height_cells: 256,
            actor_start: SimPos::new(500, -43000),
            settlements,
            plots,
            economy: super::economy_spec::EconomySpec::default(),
            behavior: crate::village::resident::BehaviorSpec::default(),
            terrain: None,
            resource_patches: Vec::new(),
            buildings: Vec::new(),
            props: Vec::new(),
        }
    }
}
impl LivingWorldSpec {
    pub fn bounds(&self) -> GridBounds {
        GridBounds::centered(self.width_cells, self.height_cells)
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1 {
            return Err("Unsupported living-world construction version".into());
        }
        if !(128..=512).contains(&self.width_cells)
            || !(128..=512).contains(&self.height_cells)
            || !self.width_cells.is_multiple_of(32)
            || !self.height_cells.is_multiple_of(32)
        {
            return Err("Living-world dimensions must be multiples of 32 in 128..512 cells".into());
        }
        if !(1..=8).contains(&self.settlements.len()) {
            return Err("Living world needs 1..8 settlements".into());
        }
        let bounds = self.bounds();
        let inside =
            |p: SimPos| bounds.contains(CellCoord::new(p.x.div_euclid(1000), p.z.div_euclid(1000)));
        if !inside(self.actor_start) {
            return Err("Arrival is outside the living world".into());
        }
        let mut keys = BTreeSet::new();
        let mut plot_keys = BTreeSet::new();
        if self.plots.is_empty() || self.plots.len() > 64 {
            return Err("Living world needs 1..64 authored homestead plots".into());
        }
        for p in &self.plots {
            if !plot_keys.insert(&p.id)
                || p.id.is_empty()
                || p.label.trim().is_empty()
                || !inside(SimPos::new(p.x, p.z))
                || !(4000..=12000).contains(&p.width_mm)
                || !(4000..=12000).contains(&p.depth_mm)
                || !matches!(p.use_kind.as_str(), "housing" | "garden")
            {
                return Err("Invalid authored homestead plot".into());
            }
        }
        for settlement in &self.settlements {
            if settlement.key.is_empty()
                || settlement.label.trim().is_empty()
                || !keys.insert(&settlement.key)
                || !inside(settlement.center)
                || !(14..=32).contains(&settlement.homes.len())
                || settlement.sites.len() < 13
            {
                return Err(
                    "Settlements need unique keys, 14 homes, 13 workplaces and an in-bounds centre"
                        .into(),
                );
            }
            let centre = CellCoord::new(
                settlement.center.x.div_euclid(1000),
                settlement.center.z.div_euclid(1000),
            );
            if centre.x < bounds.min.x + 44
                || centre.z < bounds.min.z + 44
                || centre.x >= bounds.min.x + bounds.width as i32 - 44
                || centre.z >= bounds.min.z + bounds.height as i32 - 44
            {
                return Err(format!(
                    "Settlement {} needs 44 metres of map-edge clearance for its resource catchment",
                    settlement.label
                ));
            }
            let mut local = BTreeSet::new();
            let mut roles = BTreeSet::new();
            for site in &settlement.sites {
                if site.key.is_empty()
                    || !local.insert(&site.key)
                    || !inside(site.position)
                    || !(1..=8).contains(&site.capacity)
                    || site.role == Role::Visitor
                    || !roles.insert(site.role)
                    || self.economy.profession(site.role).is_none()
                {
                    return Err("Workplaces require unique keys, a resident occupation and valid position/capacity".into());
                }
            }
            // Every founder needs a workplace of its trade at the founding.
            if let Some(founder) = self
                .economy
                .seed
                .founders
                .iter()
                .find(|founder| !roles.contains(&founder.role))
            {
                return Err(format!(
                    "Settlement {} has no workplace for the founders' trade `{}`",
                    settlement.label,
                    founder.role.key()
                ));
            }
            for home in &settlement.homes {
                if home.key.is_empty()
                    || !local.insert(&home.key)
                    || !inside(home.door)
                    || !(1..=8).contains(&home.capacity)
                {
                    return Err("Homes require unique keys and valid position/capacity".into());
                }
            }
        }
        self.validate_footprints()?;
        self.validate_export(&keys)?;
        self.economy.validate()?;
        self.behavior.validate(&self.economy, bounds)
    }
    /// The exported terrain, patches and buildings, when present.
    fn validate_export(&self, settlements: &BTreeSet<&String>) -> Result<(), String> {
        let cells = (self.width_cells * self.height_cells) as usize;
        if let Some(terrain) = &self.terrain
            && (terrain.heights.len() != cells
                || terrain.surfaces.chars().count() != cells
                || terrain.heights.iter().any(|h| !(-2000..=12000).contains(h))
                || terrain
                    .surfaces
                    .chars()
                    .any(|c| super::shape::surface_of(c).is_none()))
        {
            return Err(format!(
                "terrain needs {cells} heights of -2000..12000 mm and {cells} surfaces of g, s, t or w"
            ));
        }
        let bounds = self.bounds();
        let inside = |x: i32, z: i32| {
            bounds.contains(CellCoord::new(x.div_euclid(1000), z.div_euclid(1000)))
        };
        for patch in &self.resource_patches {
            if !self
                .economy
                .patches
                .iter()
                .any(|def| def.good == patch.good)
                || !settlements.contains(&patch.settlement)
                || !inside(patch.x, patch.z)
                || !(1..=1_000_000).contains(&patch.capacity)
            {
                return Err(format!(
                    "resource patch of {} needs a patch good, a known settlement, a position inside the map and a capacity",
                    patch.good
                ));
            }
        }
        for building in &self.buildings {
            if !self
                .economy
                .buildings
                .iter()
                .any(|def| def.id == building.building)
                || building.rotation > 3
                || !bounds.contains(CellCoord::new(building.x, building.z))
            {
                return Err(format!(
                    "building {} needs a catalogue key, a rotation 0..3 and an entrance cell inside the map",
                    building.building
                ));
            }
        }
        for prop in &self.props {
            if !self.economy.props.iter().any(|def| def.id == prop.prop)
                || prop.rotation > 3
                || !bounds.contains(CellCoord::new(prop.x, prop.z))
            {
                return Err(format!(
                    "prop {} needs a catalog key, a rotation 0..3 and a cell inside the map",
                    prop.prop
                ));
            }
        }
        Ok(())
    }
    fn validate_footprints(&self) -> Result<(), String> {
        let bounds = self.bounds();
        let mut occupied = BTreeMap::new();
        let mut doors = Vec::new();
        let mut door_cells = BTreeSet::new();
        let cell_of = |p: SimPos| CellCoord::new(p.x.div_euclid(1000), p.z.div_euclid(1000));
        for s in &self.settlements {
            for site in &s.sites {
                let door = cell_of(site.position);
                if !door_cells.insert(door) {
                    return Err(format!("Workplace {} shares another entrance", site.label));
                }
                doors.push((door, site.label.as_str()));
                if !super::economy_spec::open_workplace(&self.economy, site.role) {
                    occupy(
                        &mut occupied,
                        bounds,
                        SimPos::new(
                            site.position.x,
                            site.position.z + super::settlements::WORKPLACE_OFFSET_MM,
                        ),
                        2500,
                        2500,
                        &site.label,
                    )?;
                }
            }
            for home in &s.homes {
                let door = cell_of(home.door);
                if !door_cells.insert(door) {
                    return Err(format!("Home {} shares another entrance", home.label));
                }
                doors.push((door, home.label.as_str()));
                occupy(
                    &mut occupied,
                    bounds,
                    SimPos::new(
                        home.door.x,
                        home.door.z + super::settlements::HOME_OFFSET_MM,
                    ),
                    1900,
                    2300,
                    &home.label,
                )?;
            }
            doors.push((cell_of(s.center), s.label.as_str()));
        }
        doors.push((cell_of(self.actor_start), "arrival"));
        for (cell, label) in &doors {
            if let Some(structure) = occupied.get(cell) {
                return Err(format!("Entrance {label} is blocked by {structure}"));
            }
        }
        for plot in &self.plots {
            occupy(
                &mut occupied,
                bounds,
                SimPos::new(plot.x, plot.z),
                plot.width_mm / 2,
                plot.depth_mm / 2,
                &plot.label,
            )?;
        }
        for (cell, label) in doors {
            if let Some(plot) = occupied.get(&cell) {
                return Err(format!("Entrance {label} overlaps reserved plot {plot}"));
            }
        }
        Ok(())
    }
}
fn occupy<'a>(
    occupied: &mut BTreeMap<CellCoord, &'a str>,
    bounds: GridBounds,
    center: SimPos,
    hx: i32,
    hz: i32,
    label: &'a str,
) -> Result<(), String> {
    let lo = CellCoord::new(
        (center.x - hx).div_euclid(1000),
        (center.z - hz).div_euclid(1000),
    );
    let hi = CellCoord::new(
        (center.x + hx).div_euclid(1000),
        (center.z + hz).div_euclid(1000),
    );
    if !bounds.contains(lo) || !bounds.contains(hi) {
        return Err(format!(
            "The complete footprint of {label} must fit inside the map"
        ));
    }
    for z in lo.z..=hi.z {
        for x in lo.x..=hi.x {
            if let Some(previous) = occupied.insert(CellCoord::new(x, z), label) {
                return Err(format!(
                    "Authored footprints overlap: {previous} and {label}"
                ));
            }
        }
    }
    Ok(())
}
impl SimHashResource for LivingWorldSpec {
    const NAME: &'static str = "living_world_construction_v1";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u32(self.version);
        h.u32(self.width_cells);
        h.u32(self.height_cells);
        self.actor_start.sim_hash(h);
        h.u32(self.settlements.len() as u32);
        for s in &self.settlements {
            h.str(&s.key);
            h.str(&s.label);
            s.center.sim_hash(h);
            h.u8(s.biome as u8);
            h.u32(s.sites.len() as u32);
            for site in &s.sites {
                h.str(&site.key);
                h.str(&site.label);
                site.role.sim_hash(h);
                site.position.sim_hash(h);
                h.u32(site.capacity);
            }
            h.u32(s.homes.len() as u32);
            for home in &s.homes {
                h.str(&home.key);
                h.str(&home.label);
                home.door.sim_hash(h);
                h.u32(home.capacity);
            }
        }
        h.u32(self.plots.len() as u32);
        for p in &self.plots {
            h.str(&p.id);
            h.str(&p.label);
            h.i32(p.x);
            h.i32(p.z);
            h.i32(p.width_mm);
            h.i32(p.depth_mm);
            h.str(&p.use_kind);
        }
        self.economy.sim_hash(h);
        self.behavior.sim_hash(h);
        // Exported state is hashed only when present: an authored world's
        // construction hash stays what it was.
        if let Some(terrain) = &self.terrain {
            h.str("terrain");
            h.u32(terrain.heights.len() as u32);
            for height in &terrain.heights {
                h.i32(*height);
            }
            h.str(&terrain.surfaces);
        }
        if !self.resource_patches.is_empty() {
            h.str("resource_patches");
            h.u32(self.resource_patches.len() as u32);
            for patch in &self.resource_patches {
                h.str(&patch.good);
                h.i32(patch.x);
                h.i32(patch.z);
                h.u32(patch.capacity);
                h.str(&patch.settlement);
            }
        }
        if !self.buildings.is_empty() {
            h.str("buildings");
            h.u32(self.buildings.len() as u32);
            for building in &self.buildings {
                h.str(&building.building);
                h.i32(building.x);
                h.i32(building.z);
                h.u8(building.rotation);
                h.u8(u8::from(building.finished));
            }
        }
        if !self.props.is_empty() {
            h.str("props");
            h.u32(self.props.len() as u32);
            for prop in &self.props {
                h.str(&prop.prop);
                h.i32(prop.x);
                h.i32(prop.z);
                h.u8(prop.rotation);
            }
        }
    }
    // The construction travels in the save (fix: a world chosen for New Game
    // is loaded into a running window from a snapshot, and must replace the
    // window's own construction, or its save would not match the world it
    // was made in).
    fn snapshot_fns() -> Option<npc_framework::snapshot::ResourceSnapshotFns> {
        Some(npc_framework::snapshot::ResourceSnapshotFns {
            save: save_construction,
            load: load_construction,
        })
    }
}

fn save_construction(world: &World) -> Option<serde_json::Value> {
    let spec = world.get_resource::<LivingWorldSpec>()?;
    Some(serde_json::to_value(spec).expect("a world construction serializes"))
}

/// Loads the construction and rebuilds what is derived from it: the economy
/// registry and the roles of goods.
fn load_construction(world: &mut World, value: serde_json::Value) -> Result<(), String> {
    let spec: LivingWorldSpec = serde_json::from_value(value).map_err(|e| e.to_string())?;
    let roles = crate::village::roles::from_spec(&spec.economy)?;
    world.insert_resource(super::economy_spec::EconomyRegistry::new(&spec.economy));
    world.insert_resource(crate::village::roles::WorldRoles(roles));
    world.insert_resource(spec);
    Ok(())
}
