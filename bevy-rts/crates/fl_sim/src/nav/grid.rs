//! [`NavGrid`]: per-class blocking, clearance and path cost of every cell (ADR-0014), built
//! from the Hashed [`TerrainGrid`] and the content. `Derived` `"core.nav_grid"`.

use bevy_ecs::resource::Resource;
use fl_content::{ContentDb, MoveClass, ResolvedObject, TerrainDef};
use fl_math::{Fx, FxVec2};
use serde::{Deserialize, Serialize};

use crate::movement::MovementRules;
use crate::terrain::{CellRect, TerrainGrid};
use crate::units::{Footprint, HullDef, Mobility, MobilityDef};

/// A movement profile: a class and a size in cells (ADR-0014). Navigation data is kept per
/// profile that the content's units use.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct MoveProfile {
    pub class: MoveClass,
    /// The side, in cells, of the square the unit covers: `1..=Path.max_clearance`.
    pub size: u8,
}

/// The size of a unit `width` metres wide on `cell`-metre cells: `ceil(width / cell)`,
/// clamped to `1..=max_clearance`. No width is size 1.
fn size_of(width: Option<Fx>, cell: Fx, max_clearance: u8) -> u8 {
    let max = max_clearance.max(1);
    let (Some(width), Ok(cell)) = (width, u64::try_from(cell.raw())) else {
        return 1;
    };
    if cell == 0 {
        return 1;
    }
    let cells = u64::try_from(width.raw().max(0))
        .unwrap_or(0)
        .div_ceil(cell);
    u8::try_from(cells.clamp(1, u64::from(max))).expect("clamped to a u8")
}

/// The profile of a unit with `mobility` and `footprint` on `cell`-metre cells:
/// `size = ceil(footprint width / cell)` in `1..=max_clearance` (1 without a footprint).
pub fn profile_of(
    mobility: &Mobility,
    footprint: Option<&Footprint>,
    cell: Fx,
    max_clearance: u8,
) -> MoveProfile {
    MoveProfile {
        class: mobility.class,
        size: size_of(footprint.map(|f| f.width), cell, max_clearance),
    }
}

/// The profile a unit of `def` spawns with, if it has `Mobility`.
pub(crate) fn def_profile(
    content: &ContentDb,
    def: &ResolvedObject,
    cell: Fx,
    max_clearance: u8,
) -> Option<MoveProfile> {
    let registry = content.registry();
    let mobility = def.get::<MobilityDef>(registry)?;
    let width = def.get::<HullDef>(registry).map(|hull| hull.footprint.x);
    Some(MoveProfile {
        class: mobility.class,
        size: size_of(width, cell, max_clearance),
    })
}

/// Every profile of the content's concrete units with `Mobility`, sorted and unique.
pub fn content_profiles(content: &ContentDb, cell: Fx, max_clearance: u8) -> Vec<MoveProfile> {
    let mut profiles: Vec<MoveProfile> = content
        .units()
        .iter()
        .filter_map(|def| def_profile(content, def, cell, max_clearance))
        .collect();
    profiles.sort_unstable();
    profiles.dedup();
    profiles
}

/// The path cost of a road cell: a cell costs `ceil(COST_ROAD / factor)`.
pub const COST_ROAD: u16 = 100;

/// `clamp(pass + (1 − pass) × terrain_factor, min, 1)`, with `Fx` operations in the order
/// ADR-0014 pins: `ONE − pass`, times the terrain factor, plus `pass`, then the clamp.
fn factor(pass: Fx, terrain_factor: Fx, min: Fx) -> Fx {
    ((Fx::ONE - pass) * terrain_factor + pass).clamp(min, Fx::ONE)
}

/// `ceil(COST_ROAD / factor)` on raw integers: `((COST_ROAD << 32) + raw − 1) / raw`.
/// `factor` is at least `Movement.min_speed_frac ≥ COST_ROAD / 65535`, so it fits a `u16`.
fn cost_of(factor: Fx) -> u16 {
    let raw = i128::from(factor.raw().max(1));
    let cost = ((i128::from(COST_ROAD) << 32) + raw - 1) / raw;
    u16::try_from(cost).unwrap_or(u16::MAX)
}

/// One terrain kind as navigation sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Kind {
    pass: Fx,
    blocks: [bool; 4],
}

/// Open ground, and any palette name the content does not define: pass 1, blocks nothing.
const OPEN_KIND: Kind = Kind {
    pass: Fx::ONE,
    blocks: [false; 4],
};

/// The navigation data of every cell, per movement class: a blocked flag, a clearance and a
/// path cost. `Derived` `"core.nav_grid"`, rebuilt by core's rebuild (`rebuild_core` →
/// [`rebuild_nav`](crate::nav::rebuild_nav)) and updated in place by
/// [`update_rect`](Self::update_rect) when the terrain changes.
///
/// - **Blocked:** the cell's terrain lists the class in `blocks`, or the height step to a
///   4-neighbour inside the map exceeds `Movement.max_slope_m[class]`. Cells off the map are
///   blocked; the map edge itself blocks nothing.
/// - **Clearance:** the largest `k ≤ max_clearance` for which the `k × k` square anchored at
///   the cell, toward +x and +y, is unblocked: `0` if blocked, else
///   `1 + min(c(x+1, y), c(x, y+1), c(x+1, y+1))`, capped, with cells off the map at 0.
/// - **Cost:** `ceil(COST_ROAD / factor)` with the class's `Movement.path_terrain_factor`.
#[derive(Resource, Clone, Debug)]
pub struct NavGrid {
    cells_x: u32,
    cells_y: u32,
    /// Cell side in metres.
    cell: Fx,
    max_clearance: u8,
    min_speed_frac: Fx,
    profiles: Vec<MoveProfile>,
    /// One per palette entry of the terrain.
    kinds: Vec<Kind>,
    /// The terrain's palette index per cell ([`TerrainGrid::OPEN`] for open ground).
    types: Vec<u8>,
    /// Per class ([`MoveClass::index`]), per cell.
    blocked: [Vec<bool>; 4],
    clearance: [Vec<u8>; 4],
    cost: [Vec<u16>; 4],
    /// Observe-only: +1 per update, 0 after a build.
    version: u64,
}

/// Every field but `version`, which is observe-only: a loaded or freshly built grid equals
/// the live one.
impl PartialEq for NavGrid {
    fn eq(&self, other: &Self) -> bool {
        self.cells_x == other.cells_x
            && self.cells_y == other.cells_y
            && self.cell == other.cell
            && self.max_clearance == other.max_clearance
            && self.min_speed_frac == other.min_speed_frac
            && self.profiles == other.profiles
            && self.kinds == other.kinds
            && self.types == other.types
            && self.blocked == other.blocked
            && self.clearance == other.clearance
            && self.cost == other.cost
    }
}

impl Eq for NavGrid {}

impl NavGrid {
    /// The navigation data of `terrain` (with `cell`-metre cells) for `content`'s terrain
    /// kinds and `rules`, for `profiles`, with clearances capped at `max_clearance`
    /// (`Path.max_clearance`).
    pub fn build(
        terrain: &TerrainGrid,
        content: &ContentDb,
        rules: &MovementRules,
        profiles: &[MoveProfile],
        max_clearance: u8,
        cell: Fx,
    ) -> Self {
        let n = terrain.cells_x as usize * terrain.cells_y as usize;
        let mut grid = NavGrid {
            cells_x: terrain.cells_x,
            cells_y: terrain.cells_y,
            cell,
            max_clearance: max_clearance.max(1),
            min_speed_frac: rules.min_speed_frac,
            profiles: profiles.to_vec(),
            kinds: Vec::new(),
            types: vec![TerrainGrid::OPEN; n],
            blocked: std::array::from_fn(|_| vec![false; n]),
            clearance: std::array::from_fn(|_| vec![0; n]),
            cost: std::array::from_fn(|_| vec![0; n]),
            version: 0,
        };
        grid.profiles.sort_unstable();
        grid.profiles.dedup();
        if n > 0 {
            let all = CellRect {
                min: (0, 0),
                max: (terrain.cells_x - 1, terrain.cells_y - 1),
            };
            grid.recompute(terrain, content, rules, all, all);
        }
        grid
    }

    /// Recompute kinds (whole palette), types, blocking and cost in `cells`, then clearance
    /// in `clear`, which must contain `cells` and every cell whose clearance square reaches
    /// into it.
    fn recompute(
        &mut self,
        terrain: &TerrainGrid,
        content: &ContentDb,
        rules: &MovementRules,
        cells: CellRect,
        clear: CellRect,
    ) {
        let defs = content.kind::<TerrainDef>();
        self.kinds = terrain
            .palette
            .iter()
            .map(|name| match defs.get(name) {
                Some(def) => Kind {
                    pass: def.pass,
                    blocks: MoveClass::ALL.map(|class| def.blocks.contains(&class)),
                },
                None => OPEN_KIND,
            })
            .collect();
        let slopes = MoveClass::ALL.map(|class| rules.max_slope_m.get(&class).copied());
        let path_factors = MoveClass::ALL.map(|class| {
            rules
                .path_terrain_factor
                .get(&class)
                .copied()
                .unwrap_or(Fx::ONE)
        });

        for y in cells.min.1..=cells.max.1 {
            for x in cells.min.0..=cells.max.0 {
                let i = self.index(x, y);
                let t = terrain.type_at(x, y);
                self.types[i] = t;
                let kind = self.kind(t);
                let step = max_step(terrain, x, y);
                for class in MoveClass::ALL {
                    let c = class.index();
                    let too_steep = slopes[c].is_some_and(|max| step > max);
                    self.blocked[c][i] = kind.blocks[c] || too_steep;
                    self.cost[c][i] =
                        cost_of(factor(kind.pass, path_factors[c], self.min_speed_frac));
                }
            }
        }

        // From +x/+y toward −x/−y, so every neighbour read is current.
        for y in (clear.min.1..=clear.max.1).rev() {
            for x in (clear.min.0..=clear.max.0).rev() {
                let i = self.index(x, y);
                for c in 0..MoveClass::ALL.len() {
                    self.clearance[c][i] = if self.blocked[c][i] {
                        0
                    } else {
                        let next = self
                            .clearance_raw(c, x + 1, y)
                            .min(self.clearance_raw(c, x, y + 1))
                            .min(self.clearance_raw(c, x + 1, y + 1));
                        (next + 1).min(self.max_clearance)
                    };
                }
            }
        }
    }

    fn index(&self, x: u32, y: u32) -> usize {
        y as usize * self.cells_x as usize + x as usize
    }

    fn kind(&self, t: u8) -> Kind {
        self.kinds.get(usize::from(t)).copied().unwrap_or(OPEN_KIND)
    }

    /// The clearance of class index `c` at `(x, y)`; 0 off the map.
    fn clearance_raw(&self, c: usize, x: u32, y: u32) -> u8 {
        if x < self.cells_x && y < self.cells_y {
            self.clearance[c][self.index(x, y)]
        } else {
            0
        }
    }

    /// The grid size in cells, `(cells_x, cells_y)`.
    pub fn cells(&self) -> (u32, u32) {
        (self.cells_x, self.cells_y)
    }

    /// The cell side in metres.
    pub fn cell_m(&self) -> Fx {
        self.cell
    }

    /// The largest clearance stored (`Path.max_clearance`).
    pub fn max_clearance(&self) -> u8 {
        self.max_clearance
    }

    /// The cell containing `pos`, clamped into the grid.
    pub fn cell_of(&self, pos: FxVec2) -> (u32, u32) {
        let axis = |v: Fx, count: u32| -> u32 {
            let cell = self.cell.raw().max(1);
            let index = v
                .raw()
                .div_euclid(cell)
                .clamp(0, i64::from(count.max(1)) - 1);
            u32::try_from(index).expect("clamped into 0..count")
        };
        (axis(pos.x, self.cells_x), axis(pos.y, self.cells_y))
    }

    /// Whether `class` cannot enter cell `(x, y)`; cells off the map are blocked.
    pub fn blocked(&self, class: MoveClass, x: u32, y: u32) -> bool {
        if x < self.cells_x && y < self.cells_y {
            self.blocked[class.index()][self.index(x, y)]
        } else {
            true
        }
    }

    /// The clearance of `class` at `(x, y)`; 0 off the map.
    pub fn clearance(&self, class: MoveClass, x: u32, y: u32) -> u8 {
        self.clearance_raw(class.index(), x, y)
    }

    /// Whether a unit of profile `p` can stand with its centre cell at `(x, y)`: false off
    /// the map; else the clearance at the anchor `(x − (size − 1) / 2, y − (size − 1) / 2)`
    /// is at least `p.size` (ADR-0014; the anchor is `(x, y)` itself for size ≤ 2).
    pub fn passable(&self, p: MoveProfile, x: i64, y: i64) -> bool {
        if x < 0 || y < 0 || x >= i64::from(self.cells_x) || y >= i64::from(self.cells_y) {
            return false;
        }
        let back = (i64::from(p.size.max(1)) - 1) / 2;
        let (ax, ay) = (x - back, y - back);
        let (Ok(ax), Ok(ay)) = (u32::try_from(ax), u32::try_from(ay)) else {
            return false;
        };
        self.clearance(p.class, ax, ay) >= p.size.max(1)
    }

    /// The path cost of `(x, y)` for `class`: `ceil(COST_ROAD / factor)`, factor =
    /// `clamp(pass + (1 − pass) × path_terrain_factor[class], min_speed_frac, 1)`.
    ///
    /// # Panics
    /// If `(x, y)` is off the map.
    pub fn cost(&self, class: MoveClass, x: u32, y: u32) -> u16 {
        assert!(
            x < self.cells_x && y < self.cells_y,
            "cell ({x}, {y}) is off the map"
        );
        self.cost[class.index()][self.index(x, y)]
    }

    /// The fraction of road speed a unit with `terrain_factor` keeps on `(x, y)` (spec §7):
    /// `clamp(pass + (1 − pass) × terrain_factor, Movement.min_speed_frac, 1)`. A cell off the
    /// map counts as open ground.
    pub fn speed_factor(&self, terrain_factor: Fx, x: u32, y: u32) -> Fx {
        let t = if x < self.cells_x && y < self.cells_y {
            self.types[self.index(x, y)]
        } else {
            TerrainGrid::OPEN
        };
        factor(self.kind(t).pass, terrain_factor, self.min_speed_frac)
    }

    /// Bring the grid up to date after the terrain of `rect` changed: blocking and cost for
    /// `rect` grown by 1 (the slope to a neighbour), clearance for that rect grown by
    /// `max_clearance − 1` toward −x/−y. Returns the rect recomputed (clamped to the map; it
    /// may hold unchanged cells) and bumps the version.
    ///
    /// # Panics
    /// If `terrain` is not the size of the grid.
    pub fn update_rect(
        &mut self,
        terrain: &TerrainGrid,
        content: &ContentDb,
        rules: &MovementRules,
        rect: CellRect,
    ) -> CellRect {
        assert_eq!(
            (terrain.cells_x, terrain.cells_y),
            (self.cells_x, self.cells_y),
            "the terrain is not the size of the nav grid"
        );
        self.version += 1;
        if self.cells_x == 0 || self.cells_y == 0 {
            return rect;
        }
        let (last_x, last_y) = (self.cells_x - 1, self.cells_y - 1);
        let cells = CellRect {
            min: (rect.min.0.saturating_sub(1), rect.min.1.saturating_sub(1)),
            max: (
                rect.max.0.saturating_add(1).min(last_x),
                rect.max.1.saturating_add(1).min(last_y),
            ),
        };
        let reach = u32::from(self.max_clearance) - 1;
        let clear = CellRect {
            min: (
                cells.min.0.saturating_sub(reach),
                cells.min.1.saturating_sub(reach),
            ),
            max: cells.max,
        };
        self.recompute(terrain, content, rules, cells, clear);
        clear
    }

    /// The profiles the grid was built for, sorted.
    pub fn profiles(&self) -> &[MoveProfile] {
        &self.profiles
    }

    /// Observe-only: how many updates since the last build.
    pub fn version(&self) -> u64 {
        self.version
    }
}

/// The largest height step from `(x, y)` to a 4-neighbour inside the map.
fn max_step(terrain: &TerrainGrid, x: u32, y: u32) -> Fx {
    let here = terrain.height_at(x, y);
    let mut step = Fx::ZERO;
    let neighbours = [
        x.checked_sub(1).map(|nx| (nx, y)),
        (x + 1 < terrain.cells_x).then_some((x + 1, y)),
        y.checked_sub(1).map(|ny| (x, ny)),
        (y + 1 < terrain.cells_y).then_some((x, y + 1)),
    ];
    for (nx, ny) in neighbours.into_iter().flatten() {
        step = step.max((terrain.height_at(nx, ny) - here).abs());
    }
    step
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn factor_clamps_into_min_and_one() {
        let min = Fx::from_ratio(1, 20);
        assert_eq!(factor(Fx::ZERO, Fx::ZERO, min), min);
        assert_eq!(factor(Fx::ONE, Fx::HALF, min), Fx::ONE);
        assert_eq!(factor(Fx::HALF, -Fx::ONE, min), min);
    }

    #[test]
    fn cost_is_a_ceiling() {
        assert_eq!(cost_of(Fx::ONE), COST_ROAD);
        assert_eq!(cost_of(Fx::HALF), 200);
        assert_eq!(cost_of(Fx::from_ratio(3, 4)), 134);
    }

    #[test]
    fn size_rounds_up_and_clamps() {
        let cell = Fx::from_int(4);
        assert_eq!(size_of(None, cell, 3), 1);
        assert_eq!(size_of(Some(Fx::from_ratio(3, 5)), cell, 3), 1);
        assert_eq!(size_of(Some(Fx::from_int(4)), cell, 3), 1);
        assert_eq!(size_of(Some(Fx::from_int(7)), cell, 3), 2);
        assert_eq!(size_of(Some(Fx::from_int(40)), cell, 3), 3);
    }
}
