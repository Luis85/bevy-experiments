//! Geographic broad phase for regional simulation and replication.
//!
//! Cells use a mean-radius surface sphere, ignoring altitude just like
//! `GeoPosition::surface_distance_m`. This avoids longitude seams and polar
//! singularities. Exact great-circle admission follows the cheap cube lookup.
use crate::SandboxError;
use std::collections::{BTreeMap, BTreeSet};
use terra_core::{GeoPosition, MEAN_EARTH_RADIUS_M, normalize_longitude};

#[derive(Debug, Clone)]
pub struct GeoSpatialIndex<K = u64> {
    cell_size_m: f64,
    cells: BTreeMap<[i32; 3], Vec<(K, GeoPosition)>>,
    len: usize,
}

/// Work counts measure candidates actually checked, not elapsed wall time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpatialQuery<K = u64> {
    pub ids: Vec<K>,
    /// Occupied cells admitted by the broad phase, not empty bucket lookups.
    pub cells_visited: usize,
    pub candidates_tested: usize,
}

impl<K: Ord + Clone> GeoSpatialIndex<K> {
    pub fn from_positions(
        cell_size_m: f64,
        positions: impl IntoIterator<Item = (K, GeoPosition)>,
    ) -> Result<Self, SandboxError> {
        if !cell_size_m.is_finite() || !(1.0..=1_000_000.).contains(&cell_size_m) {
            return Err(SandboxError::Invalid("spatial cell size".into()));
        }
        let mut cells: BTreeMap<_, Vec<_>> = BTreeMap::new();
        let mut seen = BTreeSet::new();
        for (id, position) in positions {
            position.validate()?;
            if !seen.insert(id.clone()) {
                return Err(SandboxError::Invalid("duplicate spatial identity".into()));
            }
            let key = surface_cartesian(position).map(|v| (v / cell_size_m).floor() as i32);
            cells.entry(key).or_default().push((id, position));
        }
        Ok(Self {
            cell_size_m,
            cells,
            len: seen.len(),
        })
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn occupied_cells(&self) -> usize {
        self.cells.len()
    }

    /// Move one known entry without rebuilding or invalidating unrelated cells.
    /// `from` must match its last indexed position; stale callers fail atomically.
    /// Immutable snapshot users need not use this incremental simulation path.
    pub fn relocate(
        &mut self,
        id: &K,
        from: GeoPosition,
        to: GeoPosition,
    ) -> Result<(), SandboxError> {
        from.validate()?;
        to.validate()?;
        let old_key = surface_cartesian(from).map(|v| (v / self.cell_size_m).floor() as i32);
        let new_key = surface_cartesian(to).map(|v| (v / self.cell_size_m).floor() as i32);
        let old = self
            .cells
            .get_mut(&old_key)
            .ok_or_else(|| SandboxError::Invalid("stale spatial relocation".into()))?;
        let index = old
            .iter()
            .position(|(stored_id, p)| stored_id == id && *p == from)
            .ok_or_else(|| SandboxError::Invalid("stale spatial relocation".into()))?;
        if old_key == new_key {
            old[index].1 = to;
            return Ok(());
        }
        let (id, _) = old.remove(index);
        if old.is_empty() {
            self.cells.remove(&old_key);
        }
        self.cells.entry(new_key).or_default().push((id, to));
        Ok(())
    }

    pub fn query(
        &self,
        center: GeoPosition,
        radius_m: f64,
    ) -> Result<SpatialQuery<K>, SandboxError> {
        center.validate()?;
        if !radius_m.is_finite() || radius_m < 0. {
            return Err(SandboxError::Invalid("spatial query radius".into()));
        }
        let xyz = surface_cartesian(center);
        // Chord distance is never greater than surface distance. A small
        // numerical guard avoids dropping a point exactly on a cell boundary.
        let extent = radius_m.min(std::f64::consts::PI * MEAN_EARTH_RADIUS_M) + 0.000_01;
        let low = xyz.map(|v| ((v - extent) / self.cell_size_m).floor() as i32);
        let high = xyz.map(|v| ((v + extent) / self.cell_size_m).floor() as i32);
        let volume = (0..3).fold(1_u64, |n, a| {
            n.saturating_mul((i64::from(high[a]) - i64::from(low[a]) + 1) as u64)
        });
        let mut result = SpatialQuery {
            ids: Vec::new(),
            cells_visited: 0,
            candidates_tested: 0,
        };
        let mut inspect = |entries: &[(K, GeoPosition)]| {
            result.cells_visited += 1;
            for (id, position) in entries {
                result.candidates_tested += 1;
                if center.surface_distance_m(*position) <= radius_m {
                    result.ids.push(id.clone());
                }
            }
        };
        // Wide requests inspect occupied cells rather than iterating millions
        // of empty grid coordinates. Work never exceeds a full collection scan.
        if volume <= (self.cells.len() as u64).saturating_mul(4).min(32_768) {
            for x in low[0]..=high[0] {
                for y in low[1]..=high[1] {
                    for z in low[2]..=high[2] {
                        if let Some(entries) = self.cells.get(&[x, y, z]) {
                            inspect(entries);
                        }
                    }
                }
            }
        } else {
            for (key, entries) in &self.cells {
                if (0..3).all(|a| (low[a]..=high[a]).contains(&key[a])) {
                    inspect(entries);
                }
            }
        }
        result.ids.sort();
        Ok(result)
    }
}

fn surface_cartesian(p: GeoPosition) -> [f64; 3] {
    let (slat, clat) = p.latitude.to_radians().sin_cos();
    let (slon, clon) = normalize_longitude(p.longitude).to_radians().sin_cos();
    [
        MEAN_EARTH_RADIUS_M * clat * clon,
        MEAN_EARTH_RADIUS_M * clat * slon,
        MEAN_EARTH_RADIUS_M * slat,
    ]
}
