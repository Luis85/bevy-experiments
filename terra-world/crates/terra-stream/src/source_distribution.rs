//! Bounded, offline distribution of prepared sources. Copies only referenced
//! members, preserves their manifest-relative names and never edits a save.
use crate::{HeightmapManifest, HeightmapSetManifest, PreparedHeightmapProvider, StreamError};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};
use terra_core::{GeoBounds, SourceRecord, TileKey};

pub const SOURCE_PACKAGE_INDEX: &str = "source-package.json";
const MAX_INDEX_BYTES: u64 = 8 * 1024 * 1024;
const MAX_TOTAL_BYTES: u64 = 256 * 1024 * 1024;
const MAX_MEMBERS: usize = 97; // set + 32 * (manifest, raster, features)
const MAX_COVERAGE_TILES: u64 = 65_536;
const NODATA_POLICY: &str = "Dense grayscale: every admitted sample is elevation. NoData masks/sentinel detection are unsupported; split or omit uncovered rectangles before export. Coverage below checks complete geographic tiles, not surveyed accuracy.";

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SourceInspectionOptions {
    pub terrain_resolution: usize,
    pub coverage_level: u8,
    /// `None` inspects the provider's navigation bounding rectangle. A concave
    /// collection can still contain missing tiles inside that rectangle.
    pub requested_bounds: Option<GeoBounds>,
}
impl Default for SourceInspectionOptions {
    fn default() -> Self {
        Self {
            terrain_resolution: 65,
            coverage_level: 15,
            requested_bounds: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SourcePackageFile {
    pub path: PathBuf,
    pub bytes: u64,
    pub blake3: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SourceBundleDescription {
    pub manifest: PathBuf,
    pub bounds: GeoBounds,
    pub revision: String,
    pub elevation: SourceRecord,
    pub features: Option<SourceRecord>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SourceCoverageReport {
    pub level: u8,
    pub requested_bounds: GeoBounds,
    pub requested_tiles: usize,
    pub fully_covered_tiles: usize,
    /// Every intersecting tile that is not completely covered. No truncation.
    pub missing_tiles: Vec<TileKey>,
    pub complete: bool,
    pub nodata_policy: String,
}

/// The complete index is deterministic for unchanged inputs and options.
/// File digests provide content identity, not a publisher signature.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SourceInspection {
    pub schema_version: u32,
    pub entrypoint: PathBuf,
    pub source_identity: String,
    pub declared_bounds: GeoBounds,
    pub options: SourceInspectionOptions,
    pub bundles: Vec<SourceBundleDescription>,
    pub files: Vec<SourcePackageFile>,
    pub total_bytes: u64,
    pub coverage: SourceCoverageReport,
}

#[derive(Clone, Debug, Serialize)]
pub struct VerifiedSourcePackage {
    /// Digest of the exact index bytes, which bind all referenced file hashes.
    pub package_identity: String,
    pub inspection: SourceInspection,
}

fn error(message: impl Into<String>) -> StreamError {
    StreamError::Payload(format!("prepared source package: {}", message.into()))
}

fn relative(path: &Path) -> Result<(), StreamError> {
    let text = path
        .to_str()
        .ok_or_else(|| error("member paths must be UTF-8"))?;
    if text.is_empty()
        || !text.is_ascii()
        || text.len() > 1024
        || text.contains(['\\', ':', '<', '>', '"', '|', '?', '*'])
        || text.chars().any(char::is_control)
        || path.components().count() > 16
        || path
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(error(
            "member paths must be portable contained relative paths",
        ));
    }
    for component in path.components() {
        let name = component.as_os_str().to_str().unwrap();
        let stem = name.split('.').next().unwrap().to_ascii_uppercase();
        let device = ["CON", "PRN", "AUX", "NUL"].contains(&stem.as_str())
            || (stem.len() == 4
                && (stem.starts_with("COM") || stem.starts_with("LPT"))
                && matches!(stem.as_bytes()[3], b'1'..=b'9'));
        if name.ends_with(['.', ' ']) || device {
            return Err(error(
                "member path contains a nonportable reserved filename",
            ));
        }
    }
    Ok(())
}

/// Serialized member names always use `/`, including on Windows. Path::join
/// is reserved for host filesystem paths because it introduces native `\\`
/// separators, which this portable format deliberately rejects on input.
fn joined_relative(base: &Path, child: &Path) -> Result<PathBuf, StreamError> {
    if !base.as_os_str().is_empty() {
        relative(base)?;
    }
    relative(child)?;
    let result = PathBuf::from(
        base.components()
            .chain(child.components())
            .map(|part| part.as_os_str().to_str().unwrap())
            .collect::<Vec<_>>()
            .join("/"),
    );
    relative(&result)?;
    Ok(result)
}

fn portable_member(path: &Path) -> Result<(), StreamError> {
    relative(path)?;
    let first = path
        .components()
        .next()
        .unwrap()
        .as_os_str()
        .to_str()
        .unwrap()
        .to_ascii_lowercase();
    if first == SOURCE_PACKAGE_INDEX || first == ".source-package.pending" {
        return Err(error(
            "member collides with a reserved publication filename or directory",
        ));
    }
    Ok(())
}

/// Reject links in every existing path component, including the supplied root.
/// The workflow requires quiescent input/output directories, not an adversarial
/// process concurrently replacing filesystem ancestors.
fn no_symlinks(path: &Path) -> Result<PathBuf, StreamError> {
    let path = std::path::absolute(path)?;
    let mut cursor = PathBuf::new();
    for part in path.components() {
        cursor.push(part);
        if fs::symlink_metadata(&cursor)?.file_type().is_symlink() {
            return Err(error(format!(
                "symbolic links are not package members: {}",
                cursor.display()
            )));
        }
    }
    Ok(path.canonicalize()?)
}
fn member(root: &Path, path: &Path) -> Result<PathBuf, StreamError> {
    relative(path)?;
    let result = no_symlinks(&root.join(path))?;
    if !result.starts_with(root) || !result.is_file() {
        return Err(error("member escapes the package or is not a regular file"));
    }
    Ok(result)
}
fn read_manifest<T: for<'a> Deserialize<'a>>(path: &Path) -> Result<T, StreamError> {
    serde_json::from_slice(&crate::heightmap::bounded_file(path, 64 * 1024)?)
        .map_err(|e| error(format!("{}: {e}", path.display())))
}

fn inventory(
    root: &Path,
    entrypoint: &Path,
) -> Result<(BTreeMap<PathBuf, u64>, Vec<SourceBundleDescription>), StreamError> {
    let entry = member(root, entrypoint)?;
    let value: serde_json::Value = read_manifest(&entry)?;
    let manifests = if value.get("kind").is_some() {
        let set: HeightmapSetManifest =
            serde_json::from_value(value).map_err(|e| error(e.to_string()))?;
        if set.bundles.is_empty() || set.bundles.len() > 32 {
            return Err(error("requires 1..32 single bundles"));
        }
        let base = entrypoint.parent().unwrap_or(Path::new(""));
        set.bundles
            .iter()
            .map(|path| {
                relative(path)?;
                joined_relative(base, path)
            })
            .collect::<Result<Vec<_>, StreamError>>()?
    } else {
        vec![entrypoint.to_owned()]
    };
    let mut files = BTreeMap::new();
    files.insert(entrypoint.to_owned(), 64 * 1024);
    let mut bundles = Vec::new();
    for path in manifests {
        let manifest: HeightmapManifest = read_manifest(&member(root, &path)?)?;
        manifest.validate()?;
        let base = path.parent().unwrap_or(Path::new(""));
        relative(&manifest.image)?;
        files.insert(joined_relative(base, &manifest.image)?, 32 * 1024 * 1024);
        if let Some(features) = &manifest.features {
            relative(&features.geojson)?;
            files.insert(
                joined_relative(base, &features.geojson)?,
                crate::heightmap_features::MAX_BYTES,
            );
        }
        files.insert(path.clone(), 64 * 1024);
        bundles.push(SourceBundleDescription {
            manifest: path,
            bounds: manifest.bounds,
            revision: manifest.revision,
            elevation: manifest.source,
            features: manifest.features.map(|f| f.source),
        });
    }
    if files.len() > MAX_MEMBERS
        || files.contains_key(Path::new(SOURCE_PACKAGE_INDEX))
        || files.contains_key(Path::new(".source-package.pending"))
    {
        return Err(error(
            "too many members or reserved source-package.json filename",
        ));
    }
    // Windows/macOS case folding must not merge files or directory names when
    // a package is moved from a case-sensitive development filesystem.
    let mut portable = BTreeMap::new();
    let mut bytes = 0_u64;
    for (path, limit) in &files {
        portable_member(path)?;
        let resolved = member(root, path)?;
        let size = fs::metadata(resolved)?.len();
        if size > *limit {
            return Err(error(format!(
                "{} exceeds its {limit}-byte bound",
                path.display()
            )));
        }
        bytes = bytes.saturating_add(size);
        let mut prefix = PathBuf::new();
        for component in path.components() {
            prefix.push(component);
            let text = prefix.to_str().unwrap().to_owned();
            if portable
                .insert(text.to_lowercase(), text.clone())
                .is_some_and(|old| old != text)
            {
                return Err(error(
                    "member names collide on a case-insensitive filesystem",
                ));
            }
        }
    }
    if bytes > MAX_TOTAL_BYTES {
        return Err(error(
            "encoded members exceed the 256 MiB distribution limit",
        ));
    }
    bundles.sort_by(|a, b| a.manifest.cmp(&b.manifest));
    Ok((files, bundles))
}

fn hash_file(root: &Path, path: &Path, limit: u64) -> Result<SourcePackageFile, StreamError> {
    let mut file = fs::File::open(member(root, path)?)?;
    let mut hash = blake3::Hasher::new();
    let mut bytes = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        bytes = bytes.saturating_add(count as u64);
        if bytes > limit {
            return Err(error("member grew beyond its admitted bound"));
        }
        hash.update(&buffer[..count]);
    }
    Ok(SourcePackageFile {
        path: path.to_owned(),
        bytes,
        blake3: format!("blake3:{}", hash.finalize().to_hex()),
    })
}
fn hashes(
    root: &Path,
    files: &BTreeMap<PathBuf, u64>,
) -> Result<Vec<SourcePackageFile>, StreamError> {
    files
        .iter()
        .map(|(path, limit)| hash_file(root, path, *limit))
        .collect()
}
fn coverage(
    provider: &PreparedHeightmapProvider,
    options: SourceInspectionOptions,
) -> Result<SourceCoverageReport, StreamError> {
    let bounds = options
        .requested_bounds
        .unwrap_or_else(|| provider.bounds());
    if ![bounds.south, bounds.north, bounds.west, bounds.east]
        .iter()
        .all(|v| v.is_finite())
        || bounds.south < -90.
        || bounds.north > 90.
        || bounds.south >= bounds.north
        || bounds.west < -180.
        || bounds.east > 180.
        || bounds.west >= bounds.east
    {
        return Err(error(
            "coverage bounds must be a valid non-wrapping geographic rectangle",
        ));
    }
    TileKey::new(options.coverage_level, 0, 0).map_err(|e| error(e.to_string()))?;
    let side = f64::from(1_u32 << options.coverage_level);
    let x0 = ((bounds.west + 180.) / 360. * side).floor() as u32;
    let x1 = ((bounds.east + 180.) / 360. * side).ceil() as u32;
    let y0 = ((90. - bounds.north) / 180. * side).floor() as u32;
    let y1 = ((90. - bounds.south) / 180. * side).ceil() as u32;
    let count = u64::from(x1 - x0) * u64::from(y1 - y0);
    if count == 0 || count > MAX_COVERAGE_TILES {
        return Err(error(
            "coverage request requires 1..65,536 tiles; choose a representable smaller extent or lower level",
        ));
    }
    let mut missing = Vec::new();
    for x in x0..x1 {
        for y in y0..y1 {
            let key =
                TileKey::new(options.coverage_level, x, y).map_err(|e| error(e.to_string()))?;
            if !provider.covers(key) {
                missing.push(key);
            }
        }
    }
    Ok(SourceCoverageReport {
        level: options.coverage_level,
        requested_bounds: bounds,
        requested_tiles: count as usize,
        fully_covered_tiles: count as usize - missing.len(),
        complete: missing.is_empty(),
        missing_tiles: missing,
        nodata_policy: NODATA_POLICY.into(),
    })
}

pub fn inspect_prepared_source(
    path: impl AsRef<Path>,
    options: SourceInspectionOptions,
) -> Result<SourceInspection, StreamError> {
    let path = no_symlinks(path.as_ref())?;
    let root = path
        .parent()
        .ok_or_else(|| error("source needs a parent directory"))?;
    let entrypoint = PathBuf::from(
        path.file_name()
            .ok_or_else(|| error("source needs a filename"))?,
    );
    let (members, bundles) = inventory(root, &entrypoint)?;
    let files = hashes(root, &members)?;
    // Same validator used by native/host admission: decoding, semantics, seam
    // continuity, connectedness and aggregate source budgets all run here.
    let provider = PreparedHeightmapProvider::open(&path, options.terrain_resolution)?;
    let coverage = coverage(&provider, options)?;
    if hashes(root, &members)? != files {
        return Err(error(
            "source changed during validation; retry with quiescent input files",
        ));
    }
    let total_bytes = files.iter().map(|f| f.bytes).sum();
    Ok(SourceInspection {
        schema_version: 1,
        entrypoint,
        source_identity: provider.source_revision().into(),
        declared_bounds: provider.bounds(),
        options,
        bundles,
        files,
        total_bytes,
        coverage,
    })
}

fn package_identity(bytes: &[u8]) -> String {
    format!("blake3:{}", blake3::hash(bytes).to_hex())
}
fn check_expected(actual: &str, expected: Option<&str>, label: &str) -> Result<(), StreamError> {
    if expected.is_some_and(|expected| expected != actual) {
        return Err(error(format!(
            "{label} identity mismatch; existing world/source identities are never rewritten"
        )));
    }
    Ok(())
}

/// Verify a received directory before pointing a host/client configuration at
/// its entrypoint. An expected package digest should come from a trusted channel.
pub fn verify_source_package(
    directory: impl AsRef<Path>,
    expected_package: Option<&str>,
    expected_source: Option<&str>,
) -> Result<VerifiedSourcePackage, StreamError> {
    let root = no_symlinks(directory.as_ref())?;
    if !root.is_dir() {
        return Err(error("package path must be a directory"));
    }
    let bytes = crate::heightmap::bounded_file(
        &member(&root, Path::new(SOURCE_PACKAGE_INDEX))?,
        MAX_INDEX_BYTES,
    )?;
    let identity = package_identity(&bytes);
    check_expected(&identity, expected_package, "package")?;
    let declared: SourceInspection =
        serde_json::from_slice(&bytes).map_err(|e| error(e.to_string()))?;
    if declared.schema_version != 1
        || declared.files.is_empty()
        || declared.files.len() > MAX_MEMBERS
        || declared.bundles.is_empty()
        || declared.bundles.len() > 32
        || declared.coverage.missing_tiles.len() > MAX_COVERAGE_TILES as usize
        || declared.total_bytes > MAX_TOTAL_BYTES
    {
        return Err(error("invalid package schema or inventory/coverage bounds"));
    }
    let mut declared_bytes = 0_u64;
    for file in &declared.files {
        portable_member(&file.path)?;
        if file.bytes > 32 * 1024 * 1024
            || file.blake3.len() != 71
            || !file.blake3.starts_with("blake3:")
            || !file.blake3[7..]
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        {
            return Err(error("invalid member byte bound or BLAKE3 digest"));
        }
        declared_bytes = declared_bytes
            .checked_add(file.bytes)
            .ok_or_else(|| error("aggregate member bytes overflow"))?;
        if declared_bytes > MAX_TOTAL_BYTES {
            return Err(error("aggregate declared member bytes exceed 256 MiB"));
        }
    }
    if declared_bytes != declared.total_bytes {
        return Err(error(
            "aggregate member bytes do not match declared total_bytes",
        ));
    }
    if serde_json::to_vec_pretty(&declared).map_err(|e| error(e.to_string()))? != bytes {
        return Err(error("package index is not in canonical export form"));
    }
    check_expected(&declared.source_identity, expected_source, "source")?;
    // Exported entrypoints are at the package root. Internal child paths retain
    // their original subdirectories, so portable rebasing changes no identity.
    relative(&declared.entrypoint)?;
    if declared.entrypoint.components().count() != 1 {
        return Err(error("entrypoint must be a root manifest"));
    }
    let mut allowed = BTreeMap::<PathBuf, bool>::new();
    allowed.insert(PathBuf::from(SOURCE_PACKAGE_INDEX), false);
    for file in &declared.files {
        relative(&file.path)?;
        if allowed.insert(file.path.clone(), false).is_some() {
            return Err(error("duplicate package member"));
        }
        let mut parent = file.path.parent();
        while let Some(path) = parent.filter(|p| !p.as_os_str().is_empty()) {
            if allowed.insert(path.to_owned(), true) == Some(false) {
                return Err(error("member is both a file and a directory"));
            }
            parent = path.parent();
        }
    }
    let mut directories = vec![root.clone()];
    while let Some(directory) = directories.pop() {
        for item in fs::read_dir(directory)? {
            let item = item?;
            let path = item.path();
            let kind = item.file_type()?;
            let relative = path
                .strip_prefix(&root)
                .map_err(|_| error("directory escaped package"))?;
            if kind.is_symlink()
                || allowed.get(relative) != Some(&kind.is_dir())
                || (!kind.is_dir() && !kind.is_file())
            {
                return Err(error(
                    "package contains an unindexed member, link or special file",
                ));
            }
            if kind.is_dir() {
                directories.push(path);
            }
        }
    }
    for file in &declared.files {
        relative(&file.path)?;
        if fs::metadata(member(&root, &file.path)?)?.len() != file.bytes
            || hash_file(&root, &file.path, file.bytes)? != *file
        {
            return Err(error(format!(
                "content hash/size mismatch for {}",
                file.path.display()
            )));
        }
    }
    let actual = inspect_prepared_source(root.join(&declared.entrypoint), declared.options)?;
    if actual != declared {
        return Err(error(
            "index does not match validated source identity, extent, provenance or exact coverage",
        ));
    }
    Ok(VerifiedSourcePackage {
        package_identity: identity,
        inspection: actual,
    })
}

fn copy_new(
    root: &Path,
    inspection: &SourceInspection,
    destination: &Path,
) -> Result<VerifiedSourcePackage, StreamError> {
    let absolute = std::path::absolute(destination)?;
    let name = absolute
        .file_name()
        .ok_or_else(|| error("destination needs a new directory name"))?;
    relative(Path::new(name))?;
    let parent = no_symlinks(
        absolute
            .parent()
            .ok_or_else(|| error("destination needs an existing parent"))?,
    )?;
    let destination = parent.join(name);
    // An exclusive directory claim prevents overwriting an existing or partial
    // package. The complete validated index is the final publication marker.
    fs::create_dir(&destination)?;
    let result = (|| {
        for file in &inspection.files {
            let source = member(root, &file.path)?;
            let target = destination.join(&file.path);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
                no_symlinks(parent)?;
            }
            let mut input = fs::File::open(source)?.take(file.bytes + 1);
            let mut output = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&target)?;
            let copied = std::io::copy(&mut input, &mut output)?;
            output.sync_all()?;
            if copied != file.bytes || hash_file(&destination, &file.path, file.bytes)? != *file {
                return Err(error(
                    "source changed while copying; destination was not admitted",
                ));
            }
        }
        let copied =
            inspect_prepared_source(destination.join(&inspection.entrypoint), inspection.options)?;
        if copied != *inspection {
            return Err(error(
                "portable copy changed source identity, members, provenance or coverage",
            ));
        }
        let bytes = serde_json::to_vec_pretty(inspection).map_err(|e| error(e.to_string()))?;
        if bytes.len() as u64 > MAX_INDEX_BYTES {
            return Err(error("package index exceeds 8 MiB"));
        }
        // Atomic marker publication; readers never admit a partially written
        // index. Both names are in the newly claimed private output directory.
        let pending = destination.join(".source-package.pending");
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&pending)?;
        output.write_all(&bytes)?;
        output.sync_all()?;
        fs::rename(&pending, destination.join(SOURCE_PACKAGE_INDEX))?;
        verify_source_package(
            &destination,
            Some(&package_identity(&bytes)),
            Some(&inspection.source_identity),
        )
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&destination);
    }
    result
}

/// Export only validated referenced members into a new self-contained directory.
pub fn export_source_package(
    path: impl AsRef<Path>,
    destination: impl AsRef<Path>,
    options: SourceInspectionOptions,
) -> Result<VerifiedSourcePackage, StreamError> {
    let path = no_symlinks(path.as_ref())?;
    let inspection = inspect_prepared_source(&path, options)?;
    copy_new(path.parent().unwrap(), &inspection, destination.as_ref())
}

/// Validate and copy a received package into a new directory. Existing saves,
/// configurations, package directories and declared source revisions stay intact.
pub fn install_source_package(
    directory: impl AsRef<Path>,
    destination: impl AsRef<Path>,
    expected_package: Option<&str>,
    expected_source: Option<&str>,
) -> Result<VerifiedSourcePackage, StreamError> {
    let root = no_symlinks(directory.as_ref())?;
    let verified = verify_source_package(&root, expected_package, expected_source)?;
    copy_new(&root, &verified.inspection, destination.as_ref())
}

#[cfg(test)]
#[path = "source_distribution_tests.rs"]
mod tests;
