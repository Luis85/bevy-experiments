use crate::{
    HistoryError,
    time::{valid_at, valid_range},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Read,
    path::{Component, Path},
};
use terra_core::{GeoPosition, TileKey, WorldChunk};

const MAX_FILES: usize = 512;
pub(crate) const MAX_BYTES: u64 = 32 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoricalPackManifest {
    pub schema_version: u32,
    pub id: String,
    pub title: String,
    pub valid_from_unix: i64,
    pub valid_until_unix: Option<i64>,
    pub source_url: String,
    pub license: String,
    pub attribution: String,
    pub synthetic: bool,
    pub chunk_files: Vec<String>,
}

/// Immutable validated historical payload. Loading checks the entire bounded
/// pack once; subsequent queries never perform file or network I/O.
#[derive(Debug, Clone)]
pub struct HistoricalPack {
    pub manifest: HistoricalPackManifest,
    chunks: BTreeMap<TileKey, WorldChunk>,
}

pub(crate) fn bounded_read(path: &Path, limit: u64) -> Result<Vec<u8>, HistoryError> {
    // Reject directories/devices/FIFOs before opening: a byte limit alone would
    // not prevent an importer from blocking forever on a special file.
    if !fs::metadata(path)?.is_file() {
        return Err(HistoryError::Invalid(
            "data path must be a regular file".into(),
        ));
    }
    let file = fs::File::open(path)?;
    if file.metadata()?.len() > limit {
        return Err(HistoryError::Invalid("file byte budget exceeded".into()));
    }
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(HistoryError::Invalid("file byte budget exceeded".into()));
    }
    Ok(bytes)
}

impl HistoricalPack {
    pub fn load(manifest_path: impl AsRef<Path>) -> Result<Self, HistoryError> {
        let manifest_path = manifest_path.as_ref().canonicalize()?;
        let root = manifest_path
            .parent()
            .ok_or_else(|| HistoryError::Invalid("manifest has no parent directory".into()))?;
        let bytes = bounded_read(&manifest_path, 256 * 1024)?;
        let manifest: HistoricalPackManifest =
            serde_json::from_slice(&bytes).map_err(|e| HistoryError::Invalid(e.to_string()))?;
        if manifest.schema_version != 1 {
            return Err(HistoryError::Invalid(
                "unsupported historical pack schema".into(),
            ));
        }
        for (name, text) in [
            ("id", &manifest.id),
            ("title", &manifest.title),
            ("source_url", &manifest.source_url),
            ("license", &manifest.license),
            ("attribution", &manifest.attribution),
        ] {
            if text.trim().is_empty() || text.len() > 4096 {
                return Err(HistoryError::Invalid(format!(
                    "{name} must be non-empty and bounded"
                )));
            }
        }
        valid_range(manifest.valid_from_unix, manifest.valid_until_unix)?;
        if manifest.chunk_files.is_empty() || manifest.chunk_files.len() > MAX_FILES {
            return Err(HistoryError::Invalid(
                "a pack must contain 1..=512 chunk files".into(),
            ));
        }
        let mut names = BTreeSet::new();
        let mut chunks = BTreeMap::new();
        let mut total = bytes.len() as u64;
        for relative in &manifest.chunk_files {
            let path = Path::new(relative);
            if relative.contains(['\\', ':', '\0'])
                || path.is_absolute()
                || path
                    .components()
                    .any(|part| !matches!(part, Component::Normal(_)))
                || !names.insert(relative)
            {
                return Err(HistoryError::Invalid(
                    "chunk paths must be unique contained relative paths with forward slashes"
                        .into(),
                ));
            }
            let resolved = root.join(path).canonicalize()?;
            if !resolved.starts_with(root) || !resolved.is_file() {
                return Err(HistoryError::Invalid(
                    "chunk path escapes the pack directory or is not a file".into(),
                ));
            }
            let data = bounded_read(&resolved, MAX_BYTES.saturating_sub(total))?;
            total += data.len() as u64;
            let chunk: WorldChunk =
                serde_json::from_slice(&data).map_err(|e| HistoryError::Invalid(e.to_string()))?;
            chunk
                .validate()
                .map_err(|e| HistoryError::Invalid(e.to_string()))?;
            if chunk.sources.is_empty() {
                return Err(HistoryError::Invalid(
                    "every historical chunk requires source provenance".into(),
                ));
            }
            for source in &chunk.sources {
                for text in [
                    &source.provider,
                    &source.license,
                    &source.attribution,
                    &source.url,
                ] {
                    if text.trim().is_empty() || text.len() > 4096 {
                        return Err(HistoryError::Invalid(
                            "historical source provenance must be nonempty and bounded".into(),
                        ));
                    }
                }
            }
            if !manifest.synthetic && chunk.sources.iter().any(|source| source.synthetic) {
                return Err(HistoryError::Invalid(
                    "non-synthetic historical pack contains synthetic source data".into(),
                ));
            }
            if chunks.insert(chunk.key, chunk).is_some() {
                return Err(HistoryError::Invalid(
                    "duplicate tile key in historical pack".into(),
                ));
            }
        }
        Ok(Self { manifest, chunks })
    }

    pub fn contains_time(&self, epoch: i64) -> bool {
        valid_at(
            epoch,
            self.manifest.valid_from_unix,
            self.manifest.valid_until_unix,
        )
    }
    pub fn keys(&self) -> Vec<TileKey> {
        self.chunks.keys().copied().collect()
    }
    pub fn chunk(&self, key: TileKey, epoch: i64) -> Result<WorldChunk, HistoryError> {
        self.check_time(epoch)?;
        self.chunks
            .get(&key)
            .cloned()
            .ok_or(HistoryError::MissingChunk(key))
    }
    pub fn chunk_ref(&self, key: TileKey, epoch: i64) -> Result<&WorldChunk, HistoryError> {
        self.check_time(epoch)?;
        self.chunks.get(&key).ok_or(HistoryError::MissingChunk(key))
    }
    pub fn terrain_height(
        &self,
        position: GeoPosition,
        epoch: i64,
    ) -> Result<Option<f32>, HistoryError> {
        self.check_time(epoch)?;
        position
            .validate()
            .map_err(|e| HistoryError::Invalid(e.to_string()))?;
        Ok(self
            .chunks
            .iter()
            .rev()
            .find_map(|(_, chunk)| chunk.height_at(position)))
    }
    fn check_time(&self, epoch: i64) -> Result<(), HistoryError> {
        if self.contains_time(epoch) {
            Ok(())
        } else {
            Err(HistoryError::OutsideTime {
                requested: epoch,
                from: self.manifest.valid_from_unix,
                until: self.manifest.valid_until_unix,
            })
        }
    }
}

/// Optional bounded collection of era packs. Overlapping coverage of the same
/// tile is rejected explicitly instead of choosing a source arbitrarily.
#[derive(Debug, Clone)]
pub struct HistoricalCatalog {
    pub packs: Vec<HistoricalPack>,
}
impl HistoricalCatalog {
    pub fn new(packs: Vec<HistoricalPack>) -> Result<Self, HistoryError> {
        if packs.len() > 128 {
            return Err(HistoryError::Invalid("catalog pack budget exceeded".into()));
        }
        let mut ids = BTreeSet::new();
        for (i, pack) in packs.iter().enumerate() {
            if !ids.insert(&pack.manifest.id) {
                return Err(HistoryError::Invalid(
                    "catalog pack IDs must be unique immutable revisions".into(),
                ));
            }
            for other in packs.iter().skip(i + 1) {
                let overlap = pack.manifest.valid_from_unix
                    < other.manifest.valid_until_unix.unwrap_or(i64::MAX)
                    && other.manifest.valid_from_unix
                        < pack.manifest.valid_until_unix.unwrap_or(i64::MAX);
                if overlap && pack.chunks.keys().any(|key| other.chunks.contains_key(key)) {
                    return Err(HistoryError::Invalid(
                        "ambiguous overlapping historical coverage for the same tile".into(),
                    ));
                }
            }
        }
        Ok(Self { packs })
    }
    pub fn chunk(&self, key: TileKey, epoch: i64) -> Result<WorldChunk, HistoryError> {
        self.packs
            .iter()
            .find(|pack| pack.contains_time(epoch) && pack.chunks.contains_key(&key))
            .ok_or(HistoryError::MissingChunk(key))?
            .chunk(key, epoch)
    }
}
