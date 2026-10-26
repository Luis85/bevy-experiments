//! Navigation (spec §5.7, §7; ADR-0014, ADR-0015): the [`NavGrid`] built from the terrain, the
//! HPA* graphs over it ([`HpaGraphs`]), the flow fields of group moves ([`FlowCache`]), the
//! `"Path"` rules, core's one `Derived` rebuild ([`rebuild_core`]), the terrain-change hook
//! ([`on_terrain_changed`]) and the `core.passability`, `core.terrain_at` and `core.clusters`
//! debug queries. Grid search ([`astar()`], [`dijkstra_from`], [`dijkstra_seeded`],
//! [`nearest_passable`]) and line of sight ([`line_clear`], [`smooth`]) work on the grid with
//! integer costs and pinned tie-breaks; [`plan_path`] plans a path request with them.
//!
//! Every navigation cache is `Derived`: a pure function of the Hashed terrain and the content,
//! rebuilt from scratch on load, so it never changes an outcome (ADR-0015).

pub mod astar;
pub mod flow;
mod grid;
pub mod hpa;
pub mod los;

use bevy_ecs::schedule::IntoScheduleConfigs;
use bevy_ecs::world::{Mut, World};
use fl_content::{ContentDb, MoveClass, RulesSection};
use fl_protocol::SimId;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

pub use astar::{
    CellXY, DistanceField, SearchStats, astar, cell_centre, dijkstra_from, dijkstra_seeded,
    edge_cost, nearest_passable, octile,
};
pub use flow::{FlowCache, FlowDir, FlowWork};
pub(crate) use grid::def_profile;
pub use grid::{COST_ROAD, MoveProfile, NavGrid, content_profiles, profile_of};
pub use hpa::{ClusterId, HpaGraph, HpaGraphs, PathPlan, PathWork, PlanStatus, plan_path};
pub(crate) use los::step_clear;
pub use los::{line_clear, smooth};

use crate::build::Content;
use crate::builder::SimBuilder;
use crate::classify::Class;
use crate::counters::SimCounters;
use crate::ids::SimIndex;
use crate::map::MapInfo;
use crate::movement::MovementRules;
use crate::terrain::{CellRect, TerrainGrid};
use crate::units::{Footprint, Mobility};

/// `rules.ron` section `"Path"`; every value is a guess until tuned (spec §7).
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PathRules {
    /// The side of an HPA* cluster, in cells.
    pub cluster_cells: u32,
    /// The work (nodes expanded plus line-of-sight cells) path requests may spend per tick.
    pub budget_per_tick: u32,
    /// The largest unit size, in cells, that navigation data distinguishes.
    pub max_clearance: u8,
    /// How far a blocked goal snaps to the nearest passable cell, in cells.
    pub snap_radius_cells: u32,
    /// The fewest same-profile units a group move needs to share a flow field.
    pub flow_min_group: u32,
    /// The flow fields kept cached between ticks: at a tick start the cache evicts the least
    /// recently used down to this many, but never a field read in the previous tick (R124).
    pub flow_cache_max: u32,
    /// The ceiling on cached flow fields, at least `flow_cache_max` (base: 4 ×): at a tick start
    /// the cache evicts the least recently used down to this many even among the fields read in
    /// the previous tick (R124).
    pub flow_cache_hard_max: u32,
}

impl RulesSection for PathRules {
    fn link(&self, ctx: &mut fl_content::LinkCtx<'_>) {
        let counts = [
            ("cluster_cells", self.cluster_cells),
            ("budget_per_tick", self.budget_per_tick),
            ("max_clearance", u32::from(self.max_clearance)),
            ("flow_min_group", self.flow_min_group),
            ("flow_cache_max", self.flow_cache_max),
        ];
        for (field, value) in counts {
            if value < 1 {
                ctx.error(field, "must be at least 1");
            }
        }
        if self.flow_cache_hard_max < self.flow_cache_max {
            ctx.error(
                "flow_cache_hard_max",
                format!(
                    "{} must be at least flow_cache_max ({})",
                    self.flow_cache_hard_max, self.flow_cache_max
                ),
            );
        }
    }
}

/// Make the [`NavGrid`] from the [`TerrainGrid`], the [`MapInfo`] cell size and the content's
/// `"Movement"` and `"Path"` rules, for every profile of the content's units, then the
/// [`HpaGraphs`] over it (`Path.cluster_cells`) and an empty [`FlowCache`]; without terrain or
/// a map, remove the grid and the graphs and empty the flow cache. Part of core's `Derived`
/// rebuild ([`rebuild_core`]).
pub fn rebuild_nav(world: &mut World) {
    let (Some(terrain), Some(map)) = (
        world.get_resource::<TerrainGrid>(),
        world.get_resource::<MapInfo>(),
    ) else {
        world.remove_resource::<NavGrid>();
        world.remove_resource::<HpaGraphs>();
        reset_flow_cache(world);
        return;
    };
    let grid = nav_for(&world.resource::<Content>().0, terrain, map);
    install_nav(world, grid);
}

/// Insert `grid` and every navigation cache built from it: the [`HpaGraphs`]
/// (`Path.cluster_cells`), then an empty [`FlowCache`] (`Path.flow_cache_max`). The one place
/// navigation data is installed, for a live setup
/// ([`setup_scenario_def`](crate::setup_scenario_def)) and a load ([`rebuild_nav`]) alike, so
/// the two always equal (R53, R101).
pub(crate) fn install_nav(world: &mut World, grid: NavGrid) {
    let cluster_cells = world
        .resource::<Content>()
        .0
        .rules::<PathRules>()
        .cluster_cells;
    let hpa = HpaGraphs::build(&grid, cluster_cells);
    world.insert_resource(grid);
    world.insert_resource(hpa);
    reset_flow_cache(world);
}

/// Replace the [`FlowCache`] with an empty one keeping `Path.flow_cache_max` entries between
/// ticks and at most `Path.flow_cache_hard_max`, keeping its
/// [`RunConfig::flow_cache_cold`](crate::RunConfig::flow_cache_cold) flag.
fn reset_flow_cache(world: &mut World) {
    let rules = world.resource::<Content>().0.rules::<PathRules>();
    let (max, hard_max) = (rules.flow_cache_max, rules.flow_cache_hard_max);
    let cold = world
        .get_resource::<FlowCache>()
        .is_some_and(FlowCache::is_cold);
    world.insert_resource(FlowCache::new(max, hard_max, cold));
}

/// The [`NavGrid`] of `terrain` on `map` for `content`'s rules and units' profiles.
pub(crate) fn nav_for(content: &ContentDb, terrain: &TerrainGrid, map: &MapInfo) -> NavGrid {
    let max_clearance = content.rules::<PathRules>().max_clearance;
    let profiles = content_profiles(content, map.cell, max_clearance);
    NavGrid::build(
        terrain,
        content,
        content.rules::<MovementRules>(),
        &profiles,
        max_clearance,
        map.cell,
    )
}

/// Core's one `Derived` rebuild (R74), registered in
/// [`CoreFeature::build`](crate::CoreFeature): the spatial index
/// ([`rebuild_spatial`](crate::rebuild_spatial)), then the navigation data ([`rebuild_nav`]:
/// the grid, the HPA* graphs and an empty flow cache).
pub(crate) fn rebuild_core(world: &mut World) {
    crate::spatial::rebuild_spatial(world);
    rebuild_nav(world);
}

/// The one hook a terrain change goes through (ADR-0015), after [`TerrainGrid`] changed in
/// `rect`: updates the [`NavGrid`] ([`NavGrid::update_rect`]) and counts the cells it
/// recomputed as `"nav.cells_updated"`, then rebuilds the HPA* clusters that touches
/// ([`HpaGraphs::invalidate`]) and counts them as `"path.clusters_invalidated"`, drops the
/// flow fields of every profile whose graph rebuilt a cluster
/// ([`FlowCache::clear_profile`]; any change inside the map touches every profile's graph,
/// R100), then sends every unit whose remaining path crosses a rebuilt cluster back to
/// planning ([`invalidate_paths`](crate::path::invalidate_paths), journaled `"replanned"`).
/// Units on a flow field keep it: their next read rebuilds it from the new terrain (R84). No-op
/// without terrain or a grid.
pub(crate) fn on_terrain_changed(world: &mut World, rect: CellRect) {
    let content = world.resource::<Content>().0.clone();
    // Borrow the terrain next to the grid; never copy it (up to 4096² cells, R89).
    let updated = world
        .try_resource_scope(|world, terrain: Mut<TerrainGrid>| {
            world.try_resource_scope(|_, mut grid: Mut<NavGrid>| {
                grid.update_rect(&terrain, &content, content.rules::<MovementRules>(), rect)
            })
        })
        .flatten();
    let Some(updated) = updated else {
        return;
    };
    world
        .resource_mut::<SimCounters>()
        .add("nav.cells_updated", updated.area());
    let rebuilt = world
        .try_resource_scope(|world, mut hpa: Mut<HpaGraphs>| {
            world
                .get_resource::<NavGrid>()
                .map(|grid| hpa.invalidate(grid, updated))
        })
        .flatten();
    if let Some(rebuilt) = rebuilt {
        world
            .resource_mut::<SimCounters>()
            .add("path.clusters_invalidated", rebuilt.len() as u64);
        if !rebuilt.is_empty() {
            // `invalidate` returns the union over the profiles, and every profile's graph
            // rebuilds the clusters the rect touches (R100): each profile's fields are stale.
            let profiles = world.resource::<NavGrid>().profiles().to_vec();
            let mut cache = world.resource_mut::<FlowCache>();
            for p in profiles {
                cache.clear_profile(p);
            }
        }
        let rects: Vec<CellRect> = {
            let hpa = world.resource::<HpaGraphs>();
            rebuilt
                .iter()
                .filter_map(|&id| hpa.cluster_rect(id))
                .collect()
        };
        crate::path::invalidate_paths(world, &rects);
    }
}

/// The class names as JSON object keys (`"Foot"`, ...), in [`MoveClass::ALL`] order; also
/// the `"class"` of every debug query's profile.
pub(crate) fn class_name(class: MoveClass) -> String {
    format!("{class:?}")
}

/// The [`NavGrid`], or the error every navigation query gives without one.
fn nav_grid(world: &World) -> Result<&NavGrid, String> {
    world
        .get_resource::<NavGrid>()
        .ok_or_else(|| "no map is set up: there is no navigation grid".to_owned())
}

/// The profile `args` names: `{"class": "Tracked", "size": 1}` or `{"unit": 7}`.
fn query_profile(world: &World, grid: &NavGrid, args: &Value) -> Result<MoveProfile, String> {
    if let Some(unit) = args.get("unit") {
        let id = unit
            .as_u64()
            .and_then(|id| u32::try_from(id).ok())
            .ok_or_else(|| format!("\"unit\" must be a sim id, not {unit}"))?;
        let entity = world
            .resource::<SimIndex>()
            .get(SimId(id))
            .ok_or_else(|| format!("no unit #{id}"))?;
        let mobility = world
            .get::<Mobility>(entity)
            .ok_or_else(|| format!("unit #{id} cannot move"))?;
        let footprint = world.get::<Footprint>(entity);
        return Ok(profile_of(
            mobility,
            footprint,
            grid.cell_m(),
            grid.max_clearance(),
        ));
    }
    let class = args
        .get("class")
        .ok_or_else(|| "give {\"class\": <class>, \"size\": <n>} or {\"unit\": <id>}".to_owned())?;
    let class: MoveClass = serde_json::from_value(class.clone())
        .map_err(|_| format!("unknown movement class {class}"))?;
    let size = match args.get("size") {
        None => 1,
        Some(size) => size
            .as_u64()
            .and_then(|size| u8::try_from(size).ok())
            .filter(|size| (1..=grid.max_clearance()).contains(size))
            .ok_or_else(|| {
                format!(
                    "\"size\" must be in 1..={}, not {size}",
                    grid.max_clearance()
                )
            })?,
    };
    Ok(MoveProfile { class, size })
}

/// `core.passability`: args `{"class": "Tracked", "size": 1}` (size defaults to 1) or
/// `{"unit": 7}` → `{"class", "size", "cells": [x, y], "version": n, "blocked": [[start,
/// len], ..]}`, the row-major runs (index `y × cells_x + x`) of cells where the profile is not
/// passable.
fn passability(world: &World, args: &Value) -> Result<Value, String> {
    let grid = nav_grid(world)?;
    let profile = query_profile(world, grid, args)?;
    let (cells_x, cells_y) = grid.cells();
    let mut runs: Vec<[u64; 2]> = Vec::new();
    let mut index = 0u64;
    for y in 0..cells_y {
        for x in 0..cells_x {
            if !grid.passable(profile, i64::from(x), i64::from(y)) {
                match runs.last_mut() {
                    Some([start, len]) if *start + *len == index => *len += 1,
                    _ => runs.push([index, 1]),
                }
            }
            index += 1;
        }
    }
    Ok(json!({
        "class": class_name(profile.class),
        "size": profile.size,
        "cells": [cells_x, cells_y],
        "version": grid.version(),
        "blocked": runs,
    }))
}

/// `core.clusters`: args as `core.passability` takes them → `{"cluster_cells": n,
/// "clusters": [cx, cy], "entrances": [[x, y], ..], "nodes": n}`, the HPA* graph of the
/// profile: its entrance cells cluster by cluster (`ClusterId` order), each sorted by `(y,
/// x)`, and its node count. A profile no unit of the content moves with has no graph: an
/// error.
fn clusters(world: &World, args: &Value) -> Result<Value, String> {
    let grid = nav_grid(world)?;
    let profile = query_profile(world, grid, args)?;
    let graph = world
        .get_resource::<HpaGraphs>()
        .and_then(|hpa| hpa.graph(profile))
        .ok_or_else(|| {
            format!(
                "no navigation graph for {} size {}: no unit of the content moves with it",
                class_name(profile.class),
                profile.size
            )
        })?;
    let (cx, cy) = graph.clusters();
    let entrances: Vec<[u32; 2]> = (0..cx * cy)
        .flat_map(|id| graph.entrances(ClusterId(id)).iter().map(|&(x, y)| [x, y]))
        .collect();
    let cluster_cells = world.resource::<HpaGraphs>().cluster_cells();
    Ok(json!({
        "cluster_cells": cluster_cells,
        "clusters": [cx, cy],
        "entrances": entrances,
        "nodes": graph.node_count(),
    }))
}

/// `core.terrain_at`: args `{"cell": [x, y]}` → `{"cell": [x, y], "terrain": id | null,
/// "height_raw": i64, "blocked": {"Foot": bool, ..}, "clearance": {..}, "cost": {..}}`, the
/// height as its raw `Fx`.
fn terrain_at(world: &World, args: &Value) -> Result<Value, String> {
    let grid = nav_grid(world)?;
    let terrain = world
        .get_resource::<TerrainGrid>()
        .ok_or_else(|| "no map is set up: there is no terrain".to_owned())?;
    let cell = args
        .get("cell")
        .ok_or_else(|| "give {\"cell\": [x, y]}".to_owned())?;
    let (x, y): (u32, u32) = serde_json::from_value(cell.clone())
        .map_err(|_| format!("\"cell\" must be [x, y] in cells, not {cell}"))?;
    let (cells_x, cells_y) = grid.cells();
    if x >= cells_x || y >= cells_y {
        return Err(format!(
            "cell [{x}, {y}] is off the {cells_x}x{cells_y} map"
        ));
    }
    let mut blocked = Map::new();
    let mut clearance = Map::new();
    let mut cost = Map::new();
    for class in MoveClass::ALL {
        blocked.insert(class_name(class), json!(grid.blocked(class, x, y)));
        clearance.insert(class_name(class), json!(grid.clearance(class, x, y)));
        cost.insert(class_name(class), json!(grid.cost(class, x, y)));
    }
    Ok(json!({
        "cell": [x, y],
        "terrain": terrain.terrain_at(x, y),
        "height_raw": terrain.height_at(x, y).raw(),
        "blocked": blocked,
        "clearance": clearance,
        "cost": cost,
    }))
}

/// The `"Path"` rules, the Hashed [`TerrainGrid`] (with its `fill_missing` for M0 saves), the
/// Derived [`NavGrid`], [`HpaGraphs`] and [`FlowCache`] (rebuilt by [`rebuild_core`], which
/// `CoreFeature::build` registers; the flow cache is always present, so it carries
/// `RunConfig::flow_cache_cold` from the build), the navigation debug queries, and the flow
/// cache's tick start (`begin_flow_tick`, core slot of `InputSet::Housekeeping`, R124).
pub(crate) fn build(sim: &mut SimBuilder) {
    sim.app.init_resource::<FlowCache>();
    sim.register_rules_section::<PathRules>("Path")
        .register_resource::<TerrainGrid>(
            "core.terrain",
            Class::Hashed(
                crate::classify::Codec::new(1).fill_missing(crate::terrain::fill_terrain),
            ),
        )
        .register_resource::<NavGrid>("core.nav_grid", Class::Derived)
        .register_resource::<HpaGraphs>("core.nav_hpa", Class::Derived)
        .register_resource::<FlowCache>("core.path_flow_cache", Class::Derived)
        .register_debug_query("core.clusters", clusters)
        .register_debug_query("core.passability", passability)
        .register_debug_query("core.terrain_at", terrain_at)
        .add_systems(
            crate::InputSet::Housekeeping,
            flow::begin_flow_tick.after(crate::classify::reset_transients),
        );
}
