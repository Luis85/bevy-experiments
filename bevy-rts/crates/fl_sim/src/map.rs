//! The map's extent. Sim coordinates are metres: `x` east, `y` north, origin at a corner.

use bevy_ecs::resource::Resource;
use fl_math::{Fx, FxVec2};
use serde::{Deserialize, Serialize};

use crate::hash::{SimHash, StateHasher};

/// The playable area `[0, size)` on both axes, split into square cells. Hashed `"core.map"`.
#[derive(Resource, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MapInfo {
    pub size: FxVec2,
    /// Cell side in metres.
    pub cell: Fx,
    pub cells_x: u32,
    pub cells_y: u32,
}

impl MapInfo {
    /// A flat map of `size` metres with `cell`-metre cells; a partial last cell counts.
    ///
    /// # Panics
    /// If `cell` or a side of `size` is not positive.
    pub fn flat(size: FxVec2, cell: Fx) -> Self {
        assert!(cell > Fx::ZERO, "map cell size {cell} must be positive");
        let cells = |side: Fx| -> u32 {
            assert!(side > Fx::ZERO, "map side {side} must be positive");
            let count = side
                .raw()
                .unsigned_abs()
                .div_ceil(cell.raw().unsigned_abs());
            u32::try_from(count).expect("cell count fits in u32")
        };
        MapInfo {
            size,
            cell,
            cells_x: cells(size.x),
            cells_y: cells(size.y),
        }
    }

    /// Whether `p` is on the map: `0 ≤ p < size` on both axes.
    pub fn contains(&self, p: FxVec2) -> bool {
        p.x >= Fx::ZERO && p.y >= Fx::ZERO && p.x < self.size.x && p.y < self.size.y
    }

    /// The nearest point on the map: each axis clamped into `[0, size − EPSILON]`.
    pub fn clamp(&self, p: FxVec2) -> FxVec2 {
        let max = FxVec2::new(self.size.x - Fx::EPSILON, self.size.y - Fx::EPSILON);
        p.clamp_to_rect(FxVec2::ZERO, max)
    }
}

impl SimHash for MapInfo {
    fn sim_hash(&self, h: &mut StateHasher) {
        self.size.sim_hash(h);
        self.cell.sim_hash(h);
        self.cells_x.sim_hash(h);
        self.cells_y.sim_hash(h);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(x: i32, y: i32) -> FxVec2 {
        FxVec2::new(Fx::from_int(x), Fx::from_int(y))
    }

    #[test]
    fn flat_counts_cells_rounding_up() {
        let map = MapInfo::flat(v(1000, 998), Fx::from_int(4));
        assert_eq!((map.cells_x, map.cells_y), (250, 250));
    }

    #[test]
    fn contains_is_half_open() {
        let map = MapInfo::flat(v(100, 50), Fx::from_int(4));
        assert!(map.contains(v(0, 0)));
        assert!(map.contains(FxVec2::new(Fx::from_int(100) - Fx::EPSILON, Fx::ZERO)));
        assert!(!map.contains(v(100, 0)));
        assert!(!map.contains(v(0, 50)));
        assert!(!map.contains(FxVec2::new(-Fx::EPSILON, Fx::ZERO)));
    }

    #[test]
    fn clamp_lands_inside() {
        let map = MapInfo::flat(v(100, 50), Fx::from_int(4));
        let clamped = map.clamp(v(500, -20));
        assert!(map.contains(clamped));
        assert_eq!(
            clamped,
            FxVec2::new(Fx::from_int(100) - Fx::EPSILON, Fx::ZERO)
        );
        assert_eq!(map.clamp(v(10, 10)), v(10, 10));
    }
}
