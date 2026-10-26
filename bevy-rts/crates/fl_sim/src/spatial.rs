//! The spatial index (spec §5.7): a uniform grid of sorted per-cell id lists.
//!
//! [`SpatialGrid`] and [`SpatialCell`] are `Derived` state: a component hook and one tracking
//! system keep them current, and [`rebuild_spatial`] makes them from [`MapInfo`] and every
//! [`Pose`], which is also how a load restores them. Neither is hashed or saved.

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::lifecycle::HookContext;
use bevy_ecs::resource::Resource;
use bevy_ecs::system::{Commands, Query, Res, ResMut};
use bevy_ecs::world::{DeferredWorld, World};
use fl_content::RulesSection;
use fl_math::{Fx, FxVec2};
use fl_protocol::SimId;
use serde::{Deserialize, Serialize};

use crate::build::Content;
use crate::builder::SimBuilder;
use crate::classify::Class;
use crate::counters::SimCounters;
use crate::ids::{SimEntity, SimIndex};
use crate::map::MapInfo;
use crate::schedule::PerceiveSet;
use crate::units::Pose;

/// `rules.ron` section `"Spatial"`.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SpatialRules {
    /// Side of a spatial-index cell, in metres.
    pub cell_m: Fx,
}

impl RulesSection for SpatialRules {}

/// A uniform grid over the map. Each cell holds the ids of the entities inside it, sorted
/// ascending, so the grid never depends on the order entities were inserted in. Cells are
/// numbered row by row: `cell = row · cols + col`. `Derived` `"core.spatial_grid"`.
#[derive(Resource, Debug, Clone, PartialEq, Eq)]
pub struct SpatialGrid {
    cell: Fx,
    cols: u32,
    rows: u32,
    cells: Vec<Vec<SimId>>,
    len: usize,
}

impl SpatialGrid {
    /// An empty grid of `cell`-metre cells covering `map`; a partial last cell counts.
    ///
    /// # Panics
    /// If `cell` is not positive.
    pub fn new(map: &MapInfo, cell: Fx) -> Self {
        assert!(cell > Fx::ZERO, "spatial cell size {cell} must be positive");
        let count = |side: Fx| -> u32 {
            let n = side
                .raw()
                .max(1)
                .unsigned_abs()
                .div_ceil(cell.raw().unsigned_abs());
            u32::try_from(n).expect("cell count fits in u32")
        };
        let cols = count(map.size.x);
        let rows = count(map.size.y);
        SpatialGrid {
            cell,
            cols,
            rows,
            cells: vec![Vec::new(); cols as usize * rows as usize],
            len: 0,
        }
    }

    /// The column or row of coordinate `v` along an axis of `count` cells, clamped into the
    /// grid.
    fn axis(&self, v: Fx, count: u32) -> u32 {
        let index = v
            .raw()
            .div_euclid(self.cell.raw())
            .clamp(0, i64::from(count) - 1);
        u32::try_from(index).expect("clamped into 0..count")
    }

    /// The cell containing `pos`. Positions outside the grid clamp to its edge cells, so a
    /// position exactly on the far edge (`pos == size`) maps to the last cell.
    pub fn cell_of(&self, pos: FxVec2) -> u32 {
        self.axis(pos.y, self.rows) * self.cols + self.axis(pos.x, self.cols)
    }

    /// Add `id` to `cell`, keeping the cell sorted. A no-op if it is already there.
    ///
    /// # Panics
    /// If `cell` is not a cell of this grid.
    pub fn insert(&mut self, id: SimId, cell: u32) {
        let members = &mut self.cells[cell as usize];
        if let Err(at) = members.binary_search(&id) {
            members.insert(at, id);
            self.len += 1;
        }
    }

    /// Remove `id` from `cell`; whether it was there.
    ///
    /// # Panics
    /// If `cell` is not a cell of this grid.
    pub fn remove(&mut self, id: SimId, cell: u32) -> bool {
        let members = &mut self.cells[cell as usize];
        match members.binary_search(&id) {
            Ok(at) => {
                members.remove(at);
                self.len -= 1;
                true
            }
            Err(_) => false,
        }
    }

    /// Move `id` from cell `from` to cell `to`.
    ///
    /// # Panics
    /// If `id` is not in `from`.
    pub fn relocate(&mut self, id: SimId, from: u32, to: u32) {
        assert!(self.remove(id, from), "{id:?} is not in cell {from}");
        self.insert(id, to);
    }

    /// The ids in `cell`, ascending.
    ///
    /// # Panics
    /// If `cell` is not a cell of this grid.
    pub fn members(&self, cell: u32) -> &[SimId] {
        &self.cells[cell as usize]
    }

    /// Broad phase: `out` becomes the ids in every cell overlapping the bounding box of the
    /// circle `(center, radius)`, sorted ascending and unique: a superset of the ids inside
    /// the circle. Returns the number of candidates, for `"spatial.query_candidates"`.
    pub fn query_radius(&self, center: FxVec2, radius: Fx, out: &mut Vec<SimId>) -> u32 {
        out.clear();
        let r = radius.max(Fx::ZERO);
        let (col0, col1) = (
            self.axis(center.x - r, self.cols),
            self.axis(center.x + r, self.cols),
        );
        let (row0, row1) = (
            self.axis(center.y - r, self.rows),
            self.axis(center.y + r, self.rows),
        );
        for row in row0..=row1 {
            for col in col0..=col1 {
                out.extend_from_slice(&self.cells[(row * self.cols + col) as usize]);
            }
        }
        // Each id is in exactly one cell, so sorting is enough to make `out` unique.
        out.sort_unstable();
        u32::try_from(out.len()).expect("fewer than u32::MAX candidates")
    }

    /// Columns of cells.
    pub fn cols(&self) -> u32 {
        self.cols
    }

    /// Rows of cells.
    pub fn rows(&self) -> u32 {
        self.rows
    }

    /// How many ids the grid holds.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Whether the grid holds no ids.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

/// The grid cell an entity is filed under. `Derived` `"core.spatial_cell"`. Removing it (or
/// despawning the entity) takes the entity out of the [`SpatialGrid`].
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
#[component(on_remove = unfile_from_grid)]
pub struct SpatialCell(pub u32);

/// Index maintenance: drop the entity from the grid when its [`SpatialCell`] goes.
fn unfile_from_grid(mut world: DeferredWorld, ctx: HookContext) {
    let Some(cell) = world.get::<SpatialCell>(ctx.entity).map(|c| c.0) else {
        return;
    };
    let Some(id) = world.get::<SimEntity>(ctx.entity).map(SimEntity::id) else {
        return;
    };
    if let Some(mut grid) = world.get_resource_mut::<SpatialGrid>() {
        let was_filed = grid.remove(id, cell);
        debug_assert!(was_filed, "{id:?} was not filed in cell {cell}");
    }
}

/// File the new entity `id` into the grid and give it its [`SpatialCell`], at spawn time, so
/// the grid is whole the moment the spawn lands (a save taken right after loads to the same
/// grid). [`track_spatial`] only relocates afterwards. No-op without a grid or a [`Pose`].
pub(crate) fn file_spawn(world: &mut World, id: SimId) {
    let Some(entity) = world.resource::<SimIndex>().get(id) else {
        return;
    };
    let Some(pos) = world.get::<Pose>(entity).map(|pose| pose.pos) else {
        return;
    };
    let Some(mut grid) = world.get_resource_mut::<SpatialGrid>() else {
        return;
    };
    let cell = grid.cell_of(pos);
    grid.insert(id, cell);
    world.entity_mut(entity).insert(SpatialCell(cell));
    // Setup spawns happen outside a tick, where the next tick clears the counters anyway.
    if let Some(mut counters) = world.get_resource_mut::<SimCounters>() {
        counters.add("spatial.inserts", 1);
    }
}

/// Insert a new, empty grid for the [`MapInfo`] in `world`, with the `"Spatial"` rules' cell
/// size; without a map, remove the grid. Setup calls this once the map is in place.
pub(crate) fn init_spatial(world: &mut World) {
    let Some(map) = world.get_resource::<MapInfo>() else {
        world.remove_resource::<SpatialGrid>();
        return;
    };
    let cell = world.resource::<Content>().0.rules::<SpatialRules>().cell_m;
    let grid = SpatialGrid::new(map, cell);
    world.insert_resource(grid);
}

/// Make the grid and every [`SpatialCell`] from [`MapInfo`] and the [`Pose`]s, in `SimIndex`
/// order. Part of core's `Derived` rebuild (`rebuild_core`), run after a load.
pub fn rebuild_spatial(world: &mut World) {
    init_spatial(world);
    let entities: Vec<(SimId, Entity)> = world.resource::<SimIndex>().iter().collect();
    for (id, entity) in entities {
        let Some(pos) = world.get::<Pose>(entity).map(|pose| pose.pos) else {
            continue;
        };
        let Some(grid) = world.get_resource::<SpatialGrid>() else {
            return;
        };
        let cell = grid.cell_of(pos);
        // Replacing a SpatialCell does not run `on_remove`, so the fresh grid is not touched.
        world.entity_mut(entity).insert(SpatialCell(cell));
        world.resource_mut::<SpatialGrid>().insert(id, cell);
    }
}

/// File every entity under the cell of its pose, in `SimIndex` order: new entities get a
/// [`SpatialCell`] (a fallback: spawns file themselves), and entities whose cell changed move.
/// No change detection: it checks every entity's cell, which is cheap and cannot miss a write.
///
/// Counts `"spatial.checked"` (every indexed entity the scan visits), `"spatial.inserts"` and
/// `"spatial.relocations"`.
fn track_spatial(
    index: Res<SimIndex>,
    grid: Option<ResMut<SpatialGrid>>,
    mut counters: ResMut<SimCounters>,
    mut poses: Query<(&Pose, Option<&mut SpatialCell>)>,
    mut commands: Commands,
) {
    let Some(mut grid) = grid else {
        return;
    };
    let (mut inserts, mut relocations) = (0, 0);
    for (id, entity) in index.iter() {
        let Ok((pose, cell)) = poses.get_mut(entity) else {
            continue;
        };
        let to = grid.cell_of(pose.pos);
        match cell {
            None => {
                grid.insert(id, to);
                commands.entity(entity).insert(SpatialCell(to));
                inserts += 1;
            }
            Some(mut cell) if cell.0 != to => {
                grid.relocate(id, cell.0, to);
                cell.0 = to;
                relocations += 1;
            }
            Some(_) => {}
        }
    }
    counters.add("spatial.checked", index.len() as u64);
    counters.add("spatial.inserts", inserts);
    counters.add("spatial.relocations", relocations);
}

/// Registers the `"Spatial"` rules section, the `Derived` grid and cells (core's one rebuild,
/// `rebuild_core`, makes both with [`rebuild_spatial`]), and the tracker.
pub(crate) fn build(sim: &mut SimBuilder) {
    sim.register_rules_section::<SpatialRules>("Spatial")
        .register_resource::<SpatialGrid>("core.spatial_grid", Class::Derived)
        .register_component::<SpatialCell>("core.spatial_cell", Class::Derived)
        .add_systems(PerceiveSet::Spatial, track_spatial);
}
