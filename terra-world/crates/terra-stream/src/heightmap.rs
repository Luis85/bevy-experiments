//! Bounded import of explicitly georeferenced grayscale PNG elevation maps.
//! Raster rows run north to south; columns run west to east. Values map linearly
//! from black/min_height_m to white/max_height_m, retaining the declared datum.
use crate::{ChunkPayload, ChunkProvider, StreamError};
use image::{DynamicImage, ImageFormat, ImageReader, Limits};
use serde::{Deserialize, Serialize};
use std::{
    io::Cursor,
    path::{Path, PathBuf},
};
use terra_core::{GeoBounds, GeoPosition, HeightField, SourceRecord, TileKey, WorldChunk};

const MAX_MANIFEST_BYTES: u64 = 64 * 1024;
const MAX_IMAGE_BYTES: u64 = 32 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HeightmapManifest {
    pub schema_version: u32,
    pub image: PathBuf,
    pub bounds: GeoBounds,
    pub min_height_m: f64,
    pub max_height_m: f64,
    /// Immutable content revision, also used by the host's source identity.
    pub revision: String,
    pub source: SourceRecord,
}
impl HeightmapManifest {
    pub fn validate(&self) -> Result<(), StreamError> {
        let b = self.bounds;
        if self.schema_version != 1
            || ![
                b.south,
                b.north,
                b.west,
                b.east,
                self.min_height_m,
                self.max_height_m,
            ]
            .iter()
            .all(|x| x.is_finite())
            || b.south < -90.
            || b.north > 90.
            || b.south >= b.north
            || b.west < -180.
            || b.east > 180.
            || b.west >= b.east
            || self.min_height_m < -12_000.
            || self.max_height_m > 30_000.
            || self.min_height_m >= self.max_height_m
            || self.revision.trim().is_empty()
            || self.revision.len() > 128
            || self.source.provider.trim().is_empty()
            || self.source.license.trim().is_empty()
            || self.source.attribution.trim().is_empty()
            || self.source.url.trim().is_empty()
        {
            return Err(StreamError::Configuration(
                "invalid heightmap extent, elevation range, revision or provenance".into(),
            ));
        }
        if self.image.is_absolute()
            || self
                .image
                .components()
                .any(|c| !matches!(c, std::path::Component::Normal(_)))
        {
            return Err(StreamError::Configuration(
                "heightmap image must be a contained relative path".into(),
            ));
        }
        Ok(())
    }
}

pub struct HeightmapProvider {
    pub manifest: HeightmapManifest,
    raster: HeightField,
    resolution: usize,
}
impl HeightmapProvider {
    pub fn open(path: impl AsRef<Path>, resolution: usize) -> Result<Self, StreamError> {
        if !(2..=257).contains(&resolution) {
            return Err(StreamError::Configuration(
                "heightmap tile resolution must be 2..=257".into(),
            ));
        }
        let path = path.as_ref();
        let bytes = bounded_file(path, MAX_MANIFEST_BYTES)?;
        let manifest: HeightmapManifest =
            serde_json::from_slice(&bytes).map_err(|e| StreamError::Payload(e.to_string()))?;
        manifest.validate()?;
        let parent = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."))
            .canonicalize()?;
        let image_path = parent.join(&manifest.image).canonicalize()?;
        if !image_path.starts_with(&parent) {
            return Err(StreamError::Configuration(
                "heightmap image escapes its manifest directory".into(),
            ));
        }
        let bytes = bounded_file(&image_path, MAX_IMAGE_BYTES)?;
        let mut reader = ImageReader::with_format(Cursor::new(bytes), ImageFormat::Png);
        let mut limits = Limits::default();
        limits.max_image_width = Some(4096);
        limits.max_image_height = Some(4096);
        limits.max_alloc = Some(128 * 1024 * 1024);
        reader.limits(limits);
        let image = reader
            .decode()
            .map_err(|e| StreamError::Payload(format!("heightmap PNG: {e}")))?;
        let (width, height, normalized) = match image {
            DynamicImage::ImageLuma8(raster) => (raster.width(), raster.height(), raster.into_raw().into_iter().map(|n| n as f64 / 255.).collect::<Vec<_>>()),
            DynamicImage::ImageLuma16(raster) => (raster.width(), raster.height(), raster.into_raw().into_iter().map(|n| n as f64 / 65535.).collect::<Vec<_>>()),
            _ => return Err(StreamError::Payload("heightmap must be 8-bit or 16-bit grayscale PNG, without alpha; color imagery is not elevation".into())),
        };
        if width < 2 || height < 2 {
            return Err(StreamError::Payload(
                "heightmap needs at least two samples per axis".into(),
            ));
        }
        let span = manifest.max_height_m - manifest.min_height_m;
        let raster = HeightField {
            width: width as usize,
            height: height as usize,
            samples_m: normalized
                .into_iter()
                .map(|v| (manifest.min_height_m + v * span) as f32)
                .collect(),
        };
        raster
            .validate()
            .map_err(|e| StreamError::Payload(e.to_string()))?;
        Ok(Self {
            manifest,
            raster,
            resolution,
        })
    }

    pub fn sample_height(&self, position: GeoPosition) -> Option<f64> {
        if !self.manifest.bounds.contains(position) {
            return None;
        }
        let [u, v] = self.manifest.bounds.uv(position);
        self.raster.try_sample(u, v).ok().map(f64::from)
    }

    pub fn covers(&self, key: TileKey) -> bool {
        let b = key.bounds();
        let m = self.manifest.bounds;
        key.validate().is_ok()
            && b.west >= m.west - 1e-10
            && b.east <= m.east + 1e-10
            && b.south >= m.south - 1e-10
            && b.north <= m.north + 1e-10
    }
}
impl ChunkProvider for HeightmapProvider {
    fn load(&self, key: TileKey) -> Result<ChunkPayload, StreamError> {
        key.validate()
            .map_err(|e| StreamError::Geography(e.to_string()))?;
        if !self.covers(key) {
            return Err(StreamError::Geography(format!(
                "heightmap '{}' has no complete coverage for {}/{}/{}",
                self.manifest.revision, key.level, key.x, key.y
            )));
        }
        let bounds = key.bounds();
        let mut samples_m = Vec::with_capacity(self.resolution * self.resolution);
        for y in 0..self.resolution {
            for x in 0..self.resolution {
                let p = bounds
                    .point(
                        x as f64 / (self.resolution - 1) as f64,
                        y as f64 / (self.resolution - 1) as f64,
                        0.,
                    )
                    .map_err(|e| StreamError::Geography(e.to_string()))?;
                let [u, v] = self.manifest.bounds.uv(p);
                samples_m.push(
                    self.raster
                        .try_sample(u, v)
                        .map_err(|e| StreamError::Payload(e.to_string()))?,
                );
            }
        }
        Ok(ChunkPayload {
            chunk: WorldChunk {
                key,
                heights: HeightField {
                    width: self.resolution,
                    height: self.resolution,
                    samples_m,
                },
                features: Vec::new(),
                sources: vec![self.manifest.source.clone()],
                warnings: vec![format!(
                    "Imported heightmap {}; linear grayscale elevation mapping. Vertical datum is source-defined; no semantic features or imagery are inferred.",
                    self.manifest.revision
                )],
            },
            cache_hit: false,
        })
    }
}
fn bounded_file(path: &Path, limit: u64) -> Result<Vec<u8>, StreamError> {
    use std::io::Read;
    let metadata = std::fs::metadata(path)?;
    if !metadata.is_file() || metadata.len() > limit {
        return Err(StreamError::Payload(format!(
            "heightmap input is not a bounded regular file (limit {limit} bytes)"
        )));
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(limit + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(StreamError::Payload(
            "heightmap input grew beyond its limit".into(),
        ));
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    fn fixture() -> (PathBuf, TileKey) {
        let dir = std::env::temp_dir().join(format!(
            "terra-heightmap-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let key = TileKey::at(GeoPosition::new(48.137, 11.575, 0.).unwrap(), 15).unwrap();
        let b = key.bounds();
        let bounds = GeoBounds {
            east: b.east + (b.east - b.west),
            ..b
        };
        let pixels =
            image::ImageBuffer::<image::Luma<u16>, _>::from_raw(2, 2, vec![0, 65535, 0, 65535])
                .unwrap();
        pixels.save(dir.join("height.png")).unwrap();
        let manifest = HeightmapManifest {
            schema_version: 1,
            image: "height.png".into(),
            bounds,
            min_height_m: 100.,
            max_height_m: 300.,
            revision: "synthetic-test-v1".into(),
            source: SourceRecord {
                provider: "Authored unit fixture".into(),
                license: "MIT OR Apache-2.0".into(),
                attribution: "Synthetic gradient".into(),
                url: "https://example.invalid/heightmap-test".into(),
                retrieved_unix_seconds: 0,
                synthetic: true,
            },
        };
        std::fs::write(
            dir.join("manifest.json"),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        (dir, key)
    }
    #[test]
    fn imported_heightmaps_interpolate_share_edges_and_reject_missing_coverage() {
        let (dir, west) = fixture();
        let provider = HeightmapProvider::open(dir.join("manifest.json"), 17).unwrap();
        let east = TileKey::new(west.level, west.x + 1, west.y).unwrap();
        let a = provider.load(west).unwrap().chunk;
        let b = provider.load(east).unwrap().chunk;
        assert_eq!(a.heights.samples_m[0], 100.);
        assert_eq!(b.heights.samples_m[16], 300.);
        for row in 0..17 {
            assert_eq!(
                a.heights.samples_m[row * 17 + 16],
                b.heights.samples_m[row * 17]
            );
        }
        assert!((provider.sample_height(west.center()).unwrap() - 150.).abs() < 0.02);
        assert!(
            provider
                .load(TileKey::new(west.level, west.x, west.y + 1).unwrap())
                .is_err()
        );
        assert!(a.features.is_empty());
        assert!(a.sources[0].synthetic);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn heightmap_manifest_requires_provenance_and_contained_paths() {
        let (dir, _) = fixture();
        let file = dir.join("manifest.json");
        let mut m: HeightmapManifest =
            serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
        m.image = "../outside.png".into();
        assert!(m.validate().is_err());
        m.image = "height.png".into();
        m.source.license.clear();
        assert!(m.validate().is_err());
        m.source.license = "test".into();
        m.max_height_m = f64::NAN;
        assert!(m.validate().is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
