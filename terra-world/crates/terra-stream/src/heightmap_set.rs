//! Connected, bounded collections of explicitly prepared elevation bundles.
//! No interpolation bridges uncovered space. Neighboring rasters must agree at
//! every seam knot before a provider is admitted into play or server authority.
use crate::{ChunkPayload, ChunkProvider, HeightmapProvider, StreamError};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    path::{Component, Path, PathBuf},
};
use terra_core::{GeoBounds, GeoPosition, HeightField, SourceRecord, TileKey, WorldChunk};

const MAX_MAPS: usize = 32;
const MAX_SAMPLES: usize = 16_777_216;
const MAX_SET_FEATURES: usize = 8192;
const MAX_SET_VERTICES: usize = 131_072;
const MAX_SET_PROPERTIES: usize = 65_536;
const MAX_SET_PROPERTY_BYTES: usize = 8 * 1024 * 1024;

fn default_tolerance() -> f64 {
    0.05
}

/// A manifest of adjacent *single* heightmap manifests, relative to this file.
/// Collections cannot contain collections or overlapping raster interiors.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HeightmapSetManifest {
    pub schema_version: u32,
    pub kind: String,
    pub revision: String,
    pub bundles: Vec<PathBuf>,
    /// Maximum declared elevation difference at any shared edge knot, 0..2 m.
    #[serde(default = "default_tolerance")]
    pub seam_tolerance_m: f64,
}

pub struct HeightmapSet {
    maps: Vec<HeightmapProvider>,
    bounds: GeoBounds,
    source_revision: String,
    resolution: usize,
}

/// Common source entry point for the native launcher, host and SDK clients.
/// Legacy single-bundle identities are deliberately unchanged.
pub enum PreparedHeightmapProvider {
    Single(HeightmapProvider),
    Set(HeightmapSet),
}
impl PreparedHeightmapProvider {
    pub fn open(path: impl AsRef<Path>, resolution: usize) -> Result<Self, StreamError> {
        let path = path.as_ref();
        let bytes = crate::heightmap::bounded_file(path, 64 * 1024)?;
        let value: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|e| StreamError::Payload(e.to_string()))?;
        if value.get("kind").is_none() {
            return HeightmapProvider::open(path, resolution).map(Self::Single);
        }
        let manifest: HeightmapSetManifest = serde_json::from_value(value)
            .map_err(|e| StreamError::Payload(format!("heightmap set: {e}")))?;
        if manifest.kind != "heightmap_set"
            || manifest.schema_version != 1
            || manifest.revision.trim().is_empty()
            || manifest.revision.len() > 128
            || manifest.revision.chars().any(char::is_control)
            || !(1..=MAX_MAPS).contains(&manifest.bundles.len())
            || !manifest.seam_tolerance_m.is_finite()
            || !(0.0..=2.0).contains(&manifest.seam_tolerance_m)
        {
            return Err(error(
                "requires schema 1, kind heightmap_set, a revision, 1..32 bundles and a finite seam tolerance in 0..2 m",
            ));
        }
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."))
            .canonicalize()?;
        let mut paths = BTreeSet::new();
        let mut maps = Vec::new();
        let mut samples = 0usize;
        let mut feature_bytes = 0usize;
        for relative in &manifest.bundles {
            if relative.as_os_str().is_empty()
                || relative.is_absolute()
                || relative
                    .components()
                    .any(|c| !matches!(c, Component::Normal(_)))
            {
                return Err(error("bundle paths must be contained relative paths"));
            }
            let resolved = parent.join(relative).canonicalize()?;
            if !resolved.starts_with(&parent) || !paths.insert(resolved.clone()) {
                return Err(error(
                    "bundle path escapes its set or duplicates another entry",
                ));
            }
            let map = HeightmapProvider::open(resolved, resolution)?;
            samples = samples.saturating_add(map.raster.samples_m.len());
            feature_bytes = feature_bytes.saturating_add(map.features.resident_property_bytes());
            if samples > MAX_SAMPLES || feature_bytes > 32 * 1024 * 1024 {
                return Err(error(
                    "combined source budget exceeded (16777216 elevation samples / 32 MiB normalized feature text)",
                ));
            }
            maps.push(map);
        }
        // File listing order is not world identity or a seam-selection policy.
        maps.sort_by(|a, b| a.content_digest.cmp(&b.content_digest));
        let mut connected = vec![false; maps.len()];
        let mut edges = vec![Vec::new(); maps.len()];
        for i in 0..maps.len() {
            for j in i + 1..maps.len() {
                if validate_pair(&maps[i], &maps[j], manifest.seam_tolerance_m)? {
                    edges[i].push(j);
                    edges[j].push(i);
                }
            }
        }
        let mut frontier = vec![0];
        while let Some(i) = frontier.pop() {
            if connected[i] {
                continue;
            }
            connected[i] = true;
            frontier.extend(edges[i].iter().copied());
        }
        if connected.contains(&false) {
            return Err(error(
                "bundle coverage must be connected through shared edges; isolated or corner-only bundles are unsupported",
            ));
        }
        let bounds = maps
            .iter()
            .map(|m| m.manifest.bounds)
            .reduce(|a, b| GeoBounds {
                south: a.south.min(b.south),
                north: a.north.max(b.north),
                west: a.west.min(b.west),
                east: a.east.max(b.east),
            })
            .expect("nonempty maps validated");
        let mut identity = blake3::Hasher::new_derive_key("Terra connected heightmap set v1");
        identity.update(&(manifest.revision.len() as u64).to_le_bytes());
        identity.update(manifest.revision.as_bytes());
        identity.update(&manifest.seam_tolerance_m.to_le_bytes());
        for map in &maps {
            identity.update(map.content_digest.as_bytes());
        }
        Ok(Self::Set(HeightmapSet {
            maps,
            bounds,
            resolution,
            source_revision: format!("heightmap-set-v1:{}", identity.finalize().to_hex()),
        }))
    }
    pub fn source_revision(&self) -> &str {
        match self {
            Self::Single(m) => m.source_revision(),
            Self::Set(s) => &s.source_revision,
        }
    }
    /// Bounding rectangle for navigation only. Use covers() for actual coverage;
    /// a connected set can still have concave boundaries or holes.
    pub fn bounds(&self) -> GeoBounds {
        match self {
            Self::Single(m) => m.manifest.bounds,
            Self::Set(s) => s.bounds,
        }
    }
    pub fn sample_height(&self, position: GeoPosition) -> Option<f64> {
        match self {
            Self::Single(m) => m.sample_height(position),
            Self::Set(s) => s.maps.iter().find_map(|m| m.sample_height(position)),
        }
    }
    pub fn covers(&self, key: TileKey) -> bool {
        if key.validate().is_err() {
            return false;
        }
        match self {
            Self::Single(m) => m.covers(key),
            Self::Set(s) => covers_rectangle(&s.maps, key.bounds()),
        }
    }
    /// An actually covered source tile, never the potentially empty center of
    /// the collection's bounding rectangle. Used only for a new-world default.
    pub fn covered_focus(&self, level: u8) -> Option<GeoPosition> {
        TileKey::new(level, 0, 0).ok()?;
        let bounds: Vec<_> = match self {
            Self::Single(m) => vec![m.manifest.bounds],
            Self::Set(s) => s.maps.iter().map(|m| m.manifest.bounds).collect(),
        };
        // Preserve the established default when a child center has full tile
        // coverage. Offset strips can instead cover a tile jointly while every
        // child center falls in an incomplete tile outside their shared span.
        if let Some(center) = bounds
            .iter()
            .filter_map(|bounds| bounds.point(0.5, 0.5, 0.).ok())
            .find(|p| TileKey::at(*p, level).is_ok_and(|k| self.covers(k)))
        {
            return Some(center);
        }
        // Grid coverage changes only next to a source boundary. Check the
        // cross-product of those candidate rows/columns instead of scanning an
        // arbitrarily large geographic extent. 32 maps yield at most 192 of
        // each; neighbors include the dateline and clip at the poles.
        let mut columns = BTreeSet::new();
        let mut rows = BTreeSet::new();
        for bounds in bounds {
            for u in [0., 1.] {
                for v in [0., 1.] {
                    let corner = TileKey::at(bounds.point(u, v, 0.).ok()?, level).ok()?;
                    for neighbor in corner.neighbors(1) {
                        columns.insert(neighbor.x);
                        rows.insert(neighbor.y);
                    }
                }
            }
        }
        for x in columns {
            for &y in &rows {
                let key = TileKey::new(level, x, y).ok()?;
                if self.covers(key) {
                    return Some(key.center());
                }
            }
        }
        None
    }
}
impl ChunkProvider for PreparedHeightmapProvider {
    fn load(&self, key: TileKey) -> Result<ChunkPayload, StreamError> {
        let Self::Set(set) = self else {
            return match self {
                Self::Single(map) => map.load(key),
                _ => unreachable!(),
            };
        };
        if !self.covers(key) {
            return Err(StreamError::Geography(format!(
                "heightmap set has no complete coverage for {}/{}/{}; uncovered space has no synthetic fallback",
                key.level, key.x, key.y
            )));
        }
        let bounds = key.bounds();
        let mut samples_m = Vec::with_capacity(set.resolution * set.resolution);
        for y in 0..set.resolution {
            for x in 0..set.resolution {
                let p = bounds
                    .point(
                        x as f64 / (set.resolution - 1) as f64,
                        y as f64 / (set.resolution - 1) as f64,
                        0.,
                    )
                    .map_err(|e| StreamError::Geography(e.to_string()))?;
                samples_m.push(
                    self.sample_height(p)
                        .ok_or_else(|| error("coverage changed during tile sampling"))?
                        as f32,
                );
            }
        }
        let mut features = Vec::new();
        let mut sources: Vec<SourceRecord> = Vec::new();
        let mut vertices = 0usize;
        let mut properties = 0usize;
        let mut property_bytes = 0usize;
        for map in &set.maps {
            let b = map.manifest.bounds;
            if b.east < bounds.west
                || b.west > bounds.east
                || b.north < bounds.south
                || b.south > bounds.north
            {
                continue;
            }
            let mut fragments = map.features.clipped(key)?;
            for part in &mut fragments {
                let mut identity = blake3::Hasher::new_derive_key("Terra heightmap set feature v1");
                identity.update(map.content_digest.as_bytes());
                identity.update(part.id.as_bytes());
                part.id = format!("bundle:{}", identity.finalize().to_hex());
                part.properties
                    .insert("bundle_revision".into(), map.manifest.revision.clone());
                for point in &mut part.points {
                    point.altitude_m = self
                        .sample_height(*point)
                        .ok_or_else(|| error("semantic vertex is outside collection coverage"))?;
                }
                vertices = vertices.saturating_add(part.points.len());
                properties = properties.saturating_add(part.properties.len());
                property_bytes = property_bytes.saturating_add(
                    part.properties
                        .iter()
                        .map(|(k, v)| k.len() + v.len())
                        .sum::<usize>(),
                );
            }
            if features.len().saturating_add(fragments.len()) > MAX_SET_FEATURES
                || vertices > MAX_SET_VERTICES
                || properties > MAX_SET_PROPERTIES
                || property_bytes > MAX_SET_PROPERTY_BYTES
            {
                return Err(error(
                    "combined tile feature/vertex/property budget exceeded",
                ));
            }
            features.extend(fragments);
            for source in std::iter::once(&map.manifest.source)
                .chain(map.manifest.features.iter().map(|f| &f.source))
            {
                if !sources.contains(source) {
                    sources.push(source.clone());
                }
            }
        }
        Ok(ChunkPayload {
            cache_hit: false,
            chunk: WorldChunk {
                key, heights: HeightField { width: set.resolution, height: set.resolution, samples_m },
                features, sources,
                warnings: vec!["Imported connected heightmap set; shared edges verified. Elevation and semantics retain each bundle's attribution. Uncovered tiles are unavailable.".into()],
            },
        })
    }
}
fn error(message: impl Into<String>) -> StreamError {
    StreamError::Configuration(format!("heightmap set: {}", message.into()))
}

/// Divide the requested rectangle at every raster boundary. Coverage is exact
/// over all resulting open cells and their outer edges, not a sparse sample or
/// a bounding-box test. Tiny real holes are rejected rather than epsilon-filled.
fn covers_rectangle(maps: &[HeightmapProvider], bounds: GeoBounds) -> bool {
    let mut xs = vec![bounds.west, bounds.east];
    let mut ys = vec![bounds.south, bounds.north];
    for m in maps {
        let b = m.manifest.bounds;
        xs.extend(
            [b.west, b.east]
                .into_iter()
                .filter(|v| *v > bounds.west && *v < bounds.east),
        );
        ys.extend(
            [b.south, b.north]
                .into_iter()
                .filter(|v| *v > bounds.south && *v < bounds.north),
        );
    }
    xs.sort_by(f64::total_cmp);
    xs.dedup();
    ys.sort_by(f64::total_cmp);
    ys.dedup();
    xs.windows(2).all(|x| {
        ys.windows(2).all(|y| {
            maps.iter().any(|m| {
                let b = m.manifest.bounds;
                b.west <= x[0] && b.east >= x[1] && b.south <= y[0] && b.north >= y[1]
            })
        })
    })
}

/// Bilinear rasters are piecewise linear along an edge. Checking both sets of
/// sample knots plus overlap endpoints bounds the difference over the *whole*
/// seam, including a narrow spike missed by evenly spaced fixed probes.
fn validate_pair(
    a: &HeightmapProvider,
    b: &HeightmapProvider,
    tolerance: f64,
) -> Result<bool, StreamError> {
    let (x, y) = (a.manifest.bounds, b.manifest.bounds);
    if x.west < y.east && x.east > y.west && x.south < y.north && x.north > y.south {
        return Err(error(format!(
            "raster interiors overlap: {} and {}",
            a.manifest.revision, b.manifest.revision
        )));
    }
    let (vertical, fixed, start, end) = if (x.east == y.west || y.east == x.west)
        && x.south.max(y.south) < x.north.min(y.north)
    {
        (
            true,
            if x.east == y.west { x.east } else { y.east },
            x.south.max(y.south),
            x.north.min(y.north),
        )
    } else if (x.north == y.south || y.north == x.south) && x.west.max(y.west) < x.east.min(y.east)
    {
        (
            false,
            if x.north == y.south { x.north } else { y.north },
            x.west.max(y.west),
            x.east.min(y.east),
        )
    } else {
        return Ok(false);
    };
    let mut knots = vec![start, end];
    for map in [a, b] {
        let bounds = map.manifest.bounds;
        let (lo, hi, count) = if vertical {
            (bounds.south, bounds.north, map.raster.height)
        } else {
            (bounds.west, bounds.east, map.raster.width)
        };
        knots.extend(
            (0..count)
                .map(|i| lo + (hi - lo) * i as f64 / (count - 1) as f64)
                .filter(|v| *v > start && *v < end),
        );
    }
    for knot in knots {
        let point = if vertical {
            GeoPosition::new(knot, fixed, 0.)
        } else {
            GeoPosition::new(fixed, knot, 0.)
        }
        .map_err(|e| StreamError::Geography(e.to_string()))?;
        let difference = (a
            .sample_height(point)
            .ok_or_else(|| error("invalid seam coverage"))?
            - b.sample_height(point)
                .ok_or_else(|| error("invalid seam coverage"))?)
        .abs();
        if difference > tolerance {
            return Err(error(format!(
                "elevation seam mismatch between {} and {}: {difference:.4} m exceeds {tolerance:.4} m",
                a.manifest.revision, b.manifest.revision
            )));
        }
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::HeightmapManifest;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "terra-heightmap-set-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }
        fn map(
            &self,
            name: &str,
            bounds: GeoBounds,
            width: u32,
            height: u32,
            pixels: Vec<u16>,
        ) -> PathBuf {
            let image_path = format!("{name}.png");
            image::ImageBuffer::<image::Luma<u16>, _>::from_raw(width, height, pixels)
                .unwrap()
                .save(self.0.join(&image_path))
                .unwrap();
            let manifest = HeightmapManifest {
                schema_version: 1,
                image: image_path.into(),
                bounds,
                min_height_m: 100.,
                max_height_m: 200.,
                revision: name.into(),
                source: SourceRecord {
                    provider: "Original test map".into(),
                    license: "MIT OR Apache-2.0".into(),
                    attribution: "Synthetic test only".into(),
                    url: "urn:terra:test:heightmap-set".into(),
                    retrieved_unix_seconds: 0,
                    synthetic: true,
                },
                features: None,
            };
            let relative = PathBuf::from(format!("{name}.json"));
            std::fs::write(
                self.0.join(&relative),
                serde_json::to_vec(&manifest).unwrap(),
            )
            .unwrap();
            relative
        }
        fn set(&self, bundles: Vec<PathBuf>) -> PathBuf {
            let path = self.0.join("set.json");
            std::fs::write(
                &path,
                serde_json::to_vec(&HeightmapSetManifest {
                    schema_version: 1,
                    kind: "heightmap_set".into(),
                    revision: "test-set".into(),
                    bundles,
                    seam_tolerance_m: 0.01,
                })
                .unwrap(),
            )
            .unwrap();
            path
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn key() -> TileKey {
        TileKey::at(GeoPosition::new(48.137, 11.575, 0.).unwrap(), 15).unwrap()
    }

    #[test]
    fn connected_sources_cover_a_tile_crossing_the_join_and_pin_actual_bytes() {
        let f = Fixture::new();
        let key = key();
        let bounds = key.bounds();
        let middle = (bounds.west + bounds.east) * 0.5;
        let west = f.map(
            "west",
            GeoBounds {
                east: middle,
                ..bounds
            },
            2,
            2,
            vec![0, 32768, 0, 32768],
        );
        let east = f.map(
            "east",
            GeoBounds {
                west: middle,
                ..bounds
            },
            2,
            2,
            vec![32768, 65535, 32768, 65535],
        );
        let path = f.set(vec![west.clone(), east.clone()]);
        let provider = PreparedHeightmapProvider::open(&path, 17).unwrap();
        assert!(provider.covers(key));
        let chunk = provider.load(key).unwrap().chunk;
        chunk.validate().unwrap();
        assert_eq!(chunk.heights.samples_m[0], 100.);
        assert_eq!(chunk.heights.samples_m[16], 200.);
        assert!((chunk.heights.samples_m[8] - 150.).abs() < 0.01);
        assert!(!provider.covers(TileKey::new(key.level, key.x + 1, key.y).unwrap()));
        assert!(
            provider
                .sample_height(GeoPosition::new(0., 0., 0.).unwrap())
                .is_none()
        );
        f.set(vec![east.clone(), west]);
        assert_eq!(
            provider.source_revision(),
            PreparedHeightmapProvider::open(&path, 17)
                .unwrap()
                .source_revision()
        );
        // A raster-only child still contributes its bytes to the collection ID.
        image::ImageBuffer::<image::Luma<u16>, _>::from_raw(2, 2, vec![32768, 64000, 32768, 64000])
            .unwrap()
            .save(f.0.join("east.png"))
            .unwrap();
        assert_ne!(
            provider.source_revision(),
            PreparedHeightmapProvider::open(&path, 17)
                .unwrap()
                .source_revision()
        );
        let single = PreparedHeightmapProvider::open(f.0.join(east), 17).unwrap();
        assert_eq!(single.source_revision(), "east");
    }

    #[test]
    fn covered_focus_finds_joint_coverage_when_every_child_center_is_unusable() {
        for key in [key(), TileKey::new(15, (1 << 15) - 1, key().y).unwrap()] {
            let f = Fixture::new();
            let b = key.bounds();
            let split = (b.west + b.east) * 0.5;
            let height = b.north - b.south;
            let west = f.map(
                "west-offset",
                GeoBounds {
                    east: split,
                    south: b.south - 2. * height,
                    ..b
                },
                2,
                2,
                vec![0; 4],
            );
            let east = f.map(
                "east-offset",
                GeoBounds {
                    west: split,
                    north: b.north + 2. * height,
                    ..b
                },
                2,
                2,
                vec![0; 4],
            );
            let path = f.set(vec![west.clone(), east.clone()]);
            let provider = PreparedHeightmapProvider::open(&path, 17).unwrap();
            let PreparedHeightmapProvider::Set(set) = &provider else {
                unreachable!();
            };
            assert!(set.maps.iter().all(|map| {
                let center = map.manifest.bounds.point(0.5, 0.5, 0.).unwrap();
                !provider.covers(TileKey::at(center, key.level).unwrap())
            }));
            assert!(provider.covers(key));
            let focus = provider.covered_focus(key.level).unwrap();
            assert_eq!(TileKey::at(focus, key.level).unwrap(), key);
            assert!(provider.sample_height(focus).is_some());
            provider.load(key).unwrap().chunk.validate().unwrap();
            assert!(provider.covered_focus(0).is_none());
            assert!(provider.covered_focus(255).is_none());
            f.set(vec![east, west]);
            assert_eq!(
                PreparedHeightmapProvider::open(path, 17)
                    .unwrap()
                    .covered_focus(key.level),
                Some(focus)
            );
        }
    }

    #[test]
    fn whole_seam_checks_find_a_narrow_knot_mismatch() {
        let f = Fixture::new();
        let b = key().bounds();
        let split = (b.west + b.east) * 0.5;
        let a = f.map("flat", GeoBounds { east: split, ..b }, 2, 2, vec![0; 4]);
        let mut pixels = vec![0; 2 * 257];
        pixels[2 * 103] = 10000;
        let z = f.map("spike", GeoBounds { west: split, ..b }, 2, 257, pixels);
        let error = PreparedHeightmapProvider::open(f.set(vec![a, z]), 17)
            .err()
            .unwrap()
            .to_string();
        assert!(error.contains("seam mismatch"), "{error}");
    }

    #[test]
    fn connected_concave_coverage_does_not_fill_its_missing_quadrant() {
        let f = Fixture::new();
        let key = key();
        let b = key.bounds();
        let mx = (b.west + b.east) * 0.5;
        let my = (b.south + b.north) * 0.5;
        let nw = f.map(
            "nw",
            GeoBounds {
                east: mx,
                south: my,
                ..b
            },
            2,
            2,
            vec![0; 4],
        );
        let ne = f.map(
            "ne",
            GeoBounds {
                west: mx,
                south: my,
                ..b
            },
            2,
            2,
            vec![0; 4],
        );
        let sw = f.map(
            "sw",
            GeoBounds {
                east: mx,
                north: my,
                ..b
            },
            2,
            2,
            vec![0; 4],
        );
        let provider = PreparedHeightmapProvider::open(f.set(vec![nw, ne, sw]), 17).unwrap();
        assert_eq!(provider.bounds(), b);
        assert!(!provider.covers(key));
        assert!(provider.load(key).is_err());
        assert!(
            provider
                .sample_height(b.point(0.75, 0.75, 0.).unwrap())
                .is_none()
        );
        assert!(provider.covered_focus(key.level + 1).is_some());
    }

    #[test]
    fn overlap_disconnection_nested_collections_and_path_escape_fail() {
        let f = Fixture::new();
        let b = key().bounds();
        let a = f.map("a", b, 2, 2, vec![0; 4]);
        let z = f.map("overlap", b, 2, 2, vec![0; 4]);
        assert!(
            PreparedHeightmapProvider::open(f.set(vec![a.clone(), z]), 17)
                .err()
                .unwrap()
                .to_string()
                .contains("overlap")
        );
        let z = f.map(
            "isolated",
            GeoBounds {
                west: b.east + 0.001,
                east: b.east + 0.002,
                ..b
            },
            2,
            2,
            vec![0; 4],
        );
        assert!(
            PreparedHeightmapProvider::open(f.set(vec![a.clone(), z]), 17)
                .err()
                .unwrap()
                .to_string()
                .contains("connected")
        );
        assert!(PreparedHeightmapProvider::open(f.set(vec![a.clone(), a]), 17).is_err());
        assert!(
            PreparedHeightmapProvider::open(f.set(vec!["../outside.json".into()]), 17).is_err()
        );
        assert!(PreparedHeightmapProvider::open(f.set(vec!["set.json".into()]), 17).is_err());
    }

    #[test]
    fn bundled_connected_valley_loads_all_tiles_across_its_shared_edge() {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("data/heightmap-connected/manifest.json");
        let provider = PreparedHeightmapProvider::open(path, 17).unwrap();
        let b = provider.bounds();
        let nw = TileKey::at(
            GeoPosition::new(b.north - 1e-9, b.west + 1e-9, 0.).unwrap(),
            15,
        )
        .unwrap();
        let se = TileKey::at(
            GeoPosition::new(b.south + 1e-9, b.east - 1e-9, 0.).unwrap(),
            15,
        )
        .unwrap();
        let mut count = 0;
        for y in nw.y..=se.y {
            let mut previous: Option<WorldChunk> = None;
            for x in nw.x..=se.x {
                let key = TileKey::new(15, x, y).unwrap();
                assert!(provider.covers(key));
                let chunk = provider.load(key).unwrap().chunk;
                chunk.validate().unwrap();
                for feature in &chunk.features {
                    assert!(feature.id.len() <= 128);
                    assert!(feature.properties.contains_key("source_id"));
                    assert!(feature.properties.contains_key("bundle_revision"));
                }
                if let Some(west) = &previous {
                    for row in 0..17 {
                        assert_eq!(
                            west.heights.samples_m[row * 17 + 16],
                            chunk.heights.samples_m[row * 17]
                        );
                    }
                }
                previous = Some(chunk);
                count += 1;
            }
        }
        assert_eq!(count, 81);
    }

    #[test]
    fn a_subpixel_gap_cannot_pass_union_coverage() {
        let f = Fixture::new();
        let b = key().bounds();
        let mx = (b.west + b.east) * 0.5;
        let my = (b.south + b.north) * 0.5;
        // Northern bridge keeps the set connected. The thin southern slit is
        // narrower than a 257-sample tile, but still blocks complete coverage.
        let n = f.map("north", GeoBounds { south: my, ..b }, 2, 2, vec![0; 4]);
        let w = f.map(
            "west",
            GeoBounds {
                east: mx,
                north: my,
                ..b
            },
            2,
            2,
            vec![0; 4],
        );
        let e = f.map(
            "east",
            GeoBounds {
                west: mx + 1e-11,
                north: my,
                ..b
            },
            2,
            2,
            vec![0; 4],
        );
        let provider = PreparedHeightmapProvider::open(f.set(vec![n, w, e]), 17).unwrap();
        assert!(!provider.covers(key()));
    }
}
