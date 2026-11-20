//! Offline, private recovery sets for the reference local regional host.
//!
//! A verified set contains the committed database, its exact built-in catalog,
//! a portable resolved configuration, and every prepared-source dependency.
//! The final index is the publication marker. Restore always claims a new
//! directory and never opens the incident database. Hashes detect changed bytes;
//! they are not signatures, encryption, retention policy or an offsite service.
use crate::{WorldConfig, sandbox::LocalWorldSave};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};
use terra_history::PoliticsDataset;
use terra_persistence::{
    RegionArchive, RegionBackupOptions, RegionInspection, RegionStore, write_new_private_file,
};
use terra_sandbox::Catalog;
use terra_stream::source_distribution::{
    SOURCE_PACKAGE_INDEX, SourceInspectionOptions, export_source_package, install_source_package,
    verify_source_package,
};

pub const RECOVERY_INDEX: &str = "recovery-bundle.json";
const DATABASE: &str = "world.db";
const CONFIG: &str = "world-config.json";
const CATALOG: &str = "catalog.json";
const POLITICS: &str = "politics.geojson";
const MAX_INDEX_BYTES: u64 = 1024 * 1024;
const MAX_CONFIG_BYTES: u64 = 1024 * 1024;
const MAX_CATALOG_BYTES: u64 = 32 * 1024 * 1024;
const MAX_POLITICS_BYTES: u64 = 32 * 1024 * 1024;
const MAX_DATABASE_BYTES: u64 = 256 * 1024 * 1024;
const MAX_TOTAL_BYTES: u64 = 600 * 1024 * 1024;
const MAX_FILES: usize = 104;
const PROCEDURAL_REVISION: &str = "terra-procedural-v2";
type Result<T> = std::result::Result<T, String>;

/// A recovery publisher's structural outcome. Public completion markers are
/// retained after publication, including when a later flush cannot be confirmed.
#[derive(Debug)]
pub struct RecoveryPublicationError {
    message: String,
    published: bool,
}
impl RecoveryPublicationError {
    pub fn published(&self) -> bool {
        self.published
    }
}
impl std::fmt::Display for RecoveryPublicationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}", self.message)
    }
}
impl std::error::Error for RecoveryPublicationError {}
impl From<String> for RecoveryPublicationError {
    fn from(message: String) -> Self {
        Self {
            message,
            published: false,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RecoveryFile {
    pub path: PathBuf,
    pub bytes: u64,
    pub sha256: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum RecoverySource {
    Procedural {
        revision: String,
    },
    Prepared {
        package_identity: String,
        source_identity: String,
        entrypoint: PathBuf,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryPolitics {
    pub dataset_identity: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryIndex {
    pub schema_version: u32,
    pub application_version: String,
    pub source_generation: u64,
    pub region_level: u8,
    /// Canonical serialized RegionArchive, including all cold state and local
    /// host metadata. This digest never exposes the private archive itself.
    pub committed_archive_sha256: String,
    pub source: RecoverySource,
    pub politics: Option<RecoveryPolitics>,
    pub files: Vec<RecoveryFile>,
    pub total_bytes: u64,
}

/// Public receipts deliberately contain no player records or private paths.
#[derive(Clone, Debug, Serialize)]
pub struct RecoveryVerification {
    pub bundle_identity: String,
    pub source_generation: u64,
    pub region_level: u8,
    pub file_count: usize,
    pub total_bytes: u64,
    pub prepared_source: bool,
    pub political_source: bool,
    pub domain_validated: bool,
    pub local_host_metadata_validated: bool,
    pub catalog_exact: bool,
    pub source_dependencies_validated: bool,
    pub inspection: RegionInspection,
}

#[derive(Clone, Debug, Serialize)]
pub struct RecoveryRestoreReceipt {
    pub schema_version: u32,
    pub bundle_identity: String,
    pub committed_generation: u64,
    pub committed_archive_sha256: String,
    pub domain_and_local_metadata_exact: bool,
    pub source_and_catalog_exact: bool,
    pub runtime_paths_rebased: bool,
    pub original_bundle_unchanged: bool,
}

fn err(error: impl std::fmt::Display) -> String {
    format!("recovery bundle: {error}")
}

fn sha256(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

fn valid_hash(value: &str) -> bool {
    value.len() == 71
        && value.starts_with("sha256:")
        && value[7..]
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}

fn portable(path: &Path) -> Result<()> {
    let text = path.to_str().ok_or_else(|| err("non-UTF-8 member path"))?;
    if text.is_empty()
        || !text.is_ascii()
        || text.len() > 1024
        || text.contains(['\\', ':', '<', '>', '"', '|', '?', '*'])
        || text.chars().any(char::is_control)
        || path.components().count() > 18
        || path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(err(
            "member paths must be portable contained relative paths",
        ));
    }
    for part in path.components() {
        let name = part.as_os_str().to_str().unwrap();
        let stem = name.split('.').next().unwrap().to_ascii_uppercase();
        if name.ends_with(['.', ' '])
            || ["CON", "PRN", "AUX", "NUL"].contains(&stem.as_str())
            || (stem.len() == 4
                && (stem.starts_with("COM") || stem.starts_with("LPT"))
                && matches!(stem.as_bytes()[3], b'1'..=b'9'))
        {
            return Err(err("nonportable reserved member filename"));
        }
    }
    Ok(())
}

/// Like source-package maintenance, this requires quiescent input/output trees.
/// It does not defend against an administrator racing filesystem ancestors.
fn no_links(path: &Path) -> Result<PathBuf> {
    let absolute = std::path::absolute(path).map_err(err)?;
    let mut current = PathBuf::new();
    for component in absolute.components() {
        current.push(component);
        if fs::symlink_metadata(&current)
            .map_err(err)?
            .file_type()
            .is_symlink()
        {
            return Err(err(
                "symbolic links are not recovery inputs or destinations",
            ));
        }
    }
    absolute.canonicalize().map_err(err)
}

fn member(root: &Path, path: &Path) -> Result<PathBuf> {
    portable(path)?;
    let resolved = no_links(&root.join(path))?;
    let metadata = fs::metadata(&resolved).map_err(err)?;
    if !resolved.starts_with(root) || !metadata.is_file() {
        return Err(err("member escapes recovery root or is not a regular file"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.nlink() != 1 {
            return Err(err("hard-linked recovery members are not admitted"));
        }
    }
    Ok(resolved)
}

fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>> {
    let path = no_links(path)?;
    let metadata = fs::metadata(&path).map_err(err)?;
    if !metadata.is_file() || metadata.len() > limit {
        return Err(err("input is not a bounded regular file"));
    }
    let mut bytes = Vec::new();
    fs::File::open(&path)
        .map_err(err)?
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(err)?;
    if bytes.len() as u64 > limit || bytes.len() as u64 != metadata.len() {
        return Err(err("input changed or exceeded its byte bound"));
    }
    Ok(bytes)
}

pub fn read_recovery_config(path: impl AsRef<Path>) -> Result<WorldConfig> {
    let config: WorldConfig =
        serde_json::from_slice(&read_bounded(path.as_ref(), MAX_CONFIG_BYTES)?).map_err(err)?;
    config.validate().map_err(err)?;
    Ok(config)
}

fn file_hash(root: &Path, path: &Path, limit: u64) -> Result<RecoveryFile> {
    let file = member(root, path)?;
    let size = fs::metadata(&file).map_err(err)?.len();
    if size > limit {
        return Err(err("recovery member exceeds its byte limit"));
    }
    let mut input = fs::File::open(&file).map_err(err)?.take(limit + 1);
    let mut hasher = Sha256::new();
    let mut count = 0u64;
    let mut buffer = [0u8; 65_536];
    loop {
        let got = input.read(&mut buffer).map_err(err)?;
        if got == 0 {
            break;
        }
        count = count
            .checked_add(got as u64)
            .ok_or_else(|| err("member size overflow"))?;
        if count > limit {
            return Err(err("member grew beyond its limit"));
        }
        hasher.update(&buffer[..got]);
    }
    if count != size || fs::metadata(&file).map_err(err)?.len() != size {
        return Err(err("member changed while hashing"));
    }
    Ok(RecoveryFile {
        path: path.to_owned(),
        bytes: count,
        sha256: format!("sha256:{:x}", hasher.finalize()),
    })
}

struct DigestWriter {
    digest: Sha256,
    bytes: u64,
}
impl Write for DigestWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.bytes = self
            .bytes
            .checked_add(bytes.len() as u64)
            .ok_or_else(|| std::io::Error::other("archive size overflow"))?;
        if self.bytes > 128 * 1024 * 1024 {
            return Err(std::io::Error::other("archive exceeds 128 MiB"));
        }
        self.digest.update(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn archive_hash(archive: &RegionArchive) -> Result<String> {
    let mut writer = DigestWriter {
        digest: Sha256::new(),
        bytes: 0,
    };
    serde_json::to_writer(&mut writer, archive).map_err(err)?;
    Ok(format!("sha256:{:x}", writer.digest.finalize()))
}

fn check_supported(config: &WorldConfig) -> Result<()> {
    config.validate().map_err(err)?;
    if config.live_data || config.historical_pack.is_some() || config.data_epoch_unix.is_some() {
        return Err(err(
            "this recovery workflow supports offline prepared/procedural local worlds without historical packs; use explicit host recovery for other worlds",
        ));
    }
    if config.politics_file.is_some() != config.political_dataset_revision.is_some() {
        return Err(err(
            "political source requires both its local file and resolved dataset identity",
        ));
    }
    if config.heightmap_manifest.is_none() && config.base_data_revision != PROCEDURAL_REVISION {
        return Err(err("unsupported procedural source revision"));
    }
    Ok(())
}

fn read_politics(path: &Path, expected_identity: &str) -> Result<Vec<u8>> {
    let bytes = read_bounded(path, MAX_POLITICS_BYTES)?;
    let text = std::str::from_utf8(&bytes).map_err(err)?;
    let dataset = PoliticsDataset::from_geojson(text).map_err(err)?;
    if dataset.id != expected_identity {
        return Err(err(
            "political input differs from the resolved configuration and saved world identity",
        ));
    }
    Ok(bytes)
}

fn validate_local(archive: &RegionArchive, config: &WorldConfig) -> Result<()> {
    check_supported(config)?;
    if serde_json::to_value(&archive.catalog).map_err(err)?
        != serde_json::to_value(Catalog::builtin()).map_err(err)?
    {
        return Err(err(
            "the reference local recovery workflow requires its unchanged built-in catalog",
        ));
    }
    LocalWorldSave::from_regional_archive(archive.clone(), config).map_err(err)?;
    Ok(())
}

fn new_destination(path: &Path) -> Result<PathBuf> {
    let absolute = std::path::absolute(path).map_err(err)?;
    let name = absolute
        .file_name()
        .ok_or_else(|| err("destination needs a new directory name"))?;
    portable(Path::new(name))?;
    let parent = no_links(
        absolute
            .parent()
            .ok_or_else(|| err("destination needs an existing parent"))?,
    )?;
    Ok(parent.join(name))
}

fn new_private_directory(path: &Path) -> Result<PathBuf> {
    let destination = new_destination(path)?;
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(&destination).map_err(err)?;
    Ok(destination)
}

fn source_member(path: &Path) -> PathBuf {
    // Inventory names and portable configurations use one format on every OS.
    // Source-package admission separately rejects literal untrusted backslashes.
    PathBuf::from(format!("source/{}", path.to_string_lossy()))
}

fn privatize_tree(root: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut pending = vec![root.to_owned()];
        let mut count = 0;
        while let Some(path) = pending.pop() {
            count += 1;
            if count > MAX_FILES * 19 {
                return Err(err("recovery directory inventory is oversized"));
            }
            let metadata = fs::symlink_metadata(&path).map_err(err)?;
            if metadata.file_type().is_symlink() || (!metadata.is_file() && !metadata.is_dir()) {
                return Err(err("unexpected recovery link or special file"));
            }
            fs::set_permissions(
                &path,
                fs::Permissions::from_mode(if metadata.is_dir() { 0o700 } else { 0o600 }),
            )
            .map_err(err)?;
            if metadata.is_dir() {
                for entry in fs::read_dir(path).map_err(err)? {
                    pending.push(entry.map_err(err)?.path());
                }
            }
        }
    }
    #[cfg(not(unix))]
    let _ = root;
    Ok(())
}

fn normalized_config(
    config: &WorldConfig,
    source: &RecoverySource,
    root: Option<&Path>,
) -> WorldConfig {
    let mut config = config.clone();
    let path = |name: &Path| root.map_or_else(|| name.to_owned(), |root| root.join(name));
    config.region_store = Some(path(Path::new(DATABASE)));
    config.save_path = path(Path::new("manual-save.json"));
    config.cache_dir = path(Path::new("cache"));
    config.politics_file = config
        .politics_file
        .as_ref()
        .map(|_| path(Path::new(POLITICS)));
    config.heightmap_manifest = match source {
        RecoverySource::Procedural { .. } => None,
        RecoverySource::Prepared { entrypoint, .. } => Some(path(&source_member(entrypoint))),
    };
    config
}

fn json_new<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    write_new_private_file(path, &serde_json::to_vec_pretty(value).map_err(err)?).map_err(err)
}

/// Capture the last committed checkpoint, even while a host retains uncommitted
/// working state. Ask it to checkpoint first if those later changes are needed.
pub fn create_recovery_bundle(
    database: impl AsRef<Path>,
    config: &WorldConfig,
    destination: impl AsRef<Path>,
) -> Result<RecoveryVerification> {
    create_recovery_bundle_with_sync(
        database,
        config,
        destination,
        &terra_sandbox::publication::sync_directory,
    )
    .map_err(|error| error.to_string())
}

/// Structured publication outcome for wrappers that own a containing output
/// tree. `sync` is the final-marker directory-flush boundary, also used by fault
/// tests. An uncertain published result must not authorize deleting that tree.
pub fn create_recovery_bundle_with_sync(
    database: impl AsRef<Path>,
    config: &WorldConfig,
    destination: impl AsRef<Path>,
    sync: &dyn Fn(&Path) -> std::io::Result<()>,
) -> std::result::Result<RecoveryVerification, RecoveryPublicationError> {
    check_supported(config)?;
    let database = no_links(database.as_ref())?;
    terra_persistence::validate_output_path(&database, destination.as_ref()).map_err(err)?;
    let store = RegionStore::open_read_only(&database).map_err(err)?;
    // Validate the descriptor and local metadata before claiming any output.
    validate_local(&store.archive().map_err(err)?, config)?;
    let political_bytes = config
        .politics_file
        .as_ref()
        .map(|path| read_politics(path, config.political_dataset_revision.as_deref().unwrap()))
        .transpose()?;
    store.check_output_path(destination.as_ref()).map_err(err)?;
    let destination = new_private_directory(destination.as_ref())?;
    let mut published = false;
    let result: Result<RecoveryVerification> = (|| {
        store
            .backup_with_options(destination.join(DATABASE), RegionBackupOptions::default())
            .map_err(err)?;
        let snapshot = RegionStore::open_read_only(destination.join(DATABASE)).map_err(err)?;
        let archive = snapshot.archive().map_err(err)?;
        validate_local(&archive, config)?;
        let source = if let Some(manifest) = &config.heightmap_manifest {
            let exported = export_source_package(
                manifest,
                destination.join("source"),
                SourceInspectionOptions {
                    terrain_resolution: config.terrain_resolution,
                    coverage_level: config.chunk_level,
                    requested_bounds: None,
                },
            )
            .map_err(err)?;
            if exported.inspection.source_identity != config.base_data_revision {
                return Err(err(
                    "prepared source differs from the resolved configuration and saved world",
                ));
            }
            RecoverySource::Prepared {
                package_identity: exported.package_identity,
                source_identity: exported.inspection.source_identity,
                entrypoint: exported.inspection.entrypoint,
            }
        } else {
            RecoverySource::Procedural {
                revision: config.base_data_revision.clone(),
            }
        };
        let portable_config = normalized_config(config, &source, None);
        json_new(&destination.join(CONFIG), &portable_config)?;
        json_new(&destination.join(CATALOG), &archive.catalog)?;
        let politics = if let Some(bytes) = &political_bytes {
            write_new_private_file(destination.join(POLITICS), bytes).map_err(err)?;
            Some(RecoveryPolitics {
                dataset_identity: config.political_dataset_revision.clone().unwrap(),
            })
        } else {
            None
        };
        privatize_tree(&destination)?;
        let mut paths = BTreeMap::from([
            (PathBuf::from(DATABASE), MAX_DATABASE_BYTES),
            (PathBuf::from(CONFIG), MAX_CONFIG_BYTES),
            (PathBuf::from(CATALOG), MAX_CATALOG_BYTES),
        ]);
        if politics.is_some() {
            paths.insert(PathBuf::from(POLITICS), MAX_POLITICS_BYTES);
        }
        if let RecoverySource::Prepared {
            package_identity,
            source_identity,
            ..
        } = &source
        {
            let verified = verify_source_package(
                destination.join("source"),
                Some(package_identity),
                Some(source_identity),
            )
            .map_err(err)?;
            paths.insert(
                source_member(Path::new(SOURCE_PACKAGE_INDEX)),
                8 * 1024 * 1024,
            );
            for file in verified.inspection.files {
                paths.insert(source_member(&file.path), file.bytes);
            }
        }
        let files = paths
            .iter()
            .map(|(path, limit)| file_hash(&destination, path, *limit))
            .collect::<Result<Vec<_>>>()?;
        let total_bytes = files.iter().map(|file| file.bytes).sum();
        let index = RecoveryIndex {
            schema_version: 1,
            application_version: env!("CARGO_PKG_VERSION").into(),
            source_generation: archive.source_generation,
            region_level: archive.region_level,
            committed_archive_sha256: archive_hash(&archive)?,
            source,
            politics,
            files,
            total_bytes,
        };
        // The private final marker is written only after complete validation of
        // the exact snapshot and source copies. Incomplete directories reject.
        validate_contents(&destination, &index, false)?;
        terra_sandbox::publication::sync_tree(&destination, MAX_FILES * 19, MAX_TOTAL_BYTES)
            .map_err(err)?;
        let bytes = serde_json::to_vec_pretty(&index).map_err(err)?;
        terra_sandbox::publication::write_private_with_sync(
            &destination.join(RECOVERY_INDEX),
            &bytes,
            terra_sandbox::publication::PublishMode::CreateNew,
            sync,
        )
        .map_err(|error| {
            published = error.published();
            err(error)
        })?;
        published = true;
        verify_recovery_bundle(&destination, None)
    })();
    if result.is_err() && !published {
        let _ = fs::remove_dir_all(&destination);
    }
    result.map_err(|message| RecoveryPublicationError { message, published })
}

fn file_limit(path: &Path) -> Result<u64> {
    if path == Path::new(DATABASE) {
        Ok(MAX_DATABASE_BYTES)
    } else if path == Path::new(CONFIG) {
        Ok(MAX_CONFIG_BYTES)
    } else if path == Path::new(CATALOG) {
        Ok(MAX_CATALOG_BYTES)
    } else if path == Path::new(POLITICS) {
        Ok(MAX_POLITICS_BYTES)
    } else if path.starts_with("source") {
        Ok(32 * 1024 * 1024)
    } else {
        Err(err("unrecognized recovery member"))
    }
}

fn validate_inventory(root: &Path, index: &RecoveryIndex, published: bool) -> Result<()> {
    if index.schema_version != 1
        || index.source_generation == 0
        || index.application_version.is_empty()
        || index.application_version.len() > 64
        || !valid_hash(&index.committed_archive_sha256)
        || index.files.len() < 3
        || index.files.len() > MAX_FILES
        || index.total_bytes > MAX_TOTAL_BYTES
    {
        return Err(err("invalid recovery index version, provenance or bounds"));
    }
    let mut allowed = BTreeMap::<PathBuf, bool>::new();
    if published {
        allowed.insert(PathBuf::from(RECOVERY_INDEX), false);
    }
    let mut casefolded = BTreeSet::new();
    let mut total = 0u64;
    let mut previous = None;
    for file in &index.files {
        portable(&file.path)?;
        if !valid_hash(&file.sha256)
            || file.bytes > file_limit(&file.path)?
            || previous
                .as_ref()
                .is_some_and(|previous| previous >= &file.path)
            || !casefolded.insert(file.path.to_string_lossy().to_ascii_lowercase())
            || allowed.insert(file.path.clone(), false).is_some()
        {
            return Err(err("invalid, duplicated or unordered recovery member"));
        }
        previous = Some(file.path.clone());
        total = total
            .checked_add(file.bytes)
            .ok_or_else(|| err("recovery size overflow"))?;
        if total > MAX_TOTAL_BYTES {
            return Err(err("recovery set exceeds 600 MiB"));
        }
        let mut parent = file.path.parent();
        while let Some(path) = parent.filter(|path| !path.as_os_str().is_empty()) {
            if allowed.insert(path.to_owned(), true) == Some(false) {
                return Err(err("recovery member is both a file and directory"));
            }
            parent = path.parent();
        }
    }
    if total != index.total_bytes
        || [DATABASE, CONFIG, CATALOG]
            .iter()
            .any(|name| allowed.get(Path::new(name)) != Some(&false))
    {
        return Err(err("recovery inventory is incomplete or its total differs"));
    }
    let mut directories = vec![root.to_owned()];
    while let Some(directory) = directories.pop() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if fs::metadata(&directory).map_err(err)?.permissions().mode() & 0o077 != 0 {
                return Err(err("recovery directories must be private (Unix mode 0700)"));
            }
        }
        for entry in fs::read_dir(directory).map_err(err)? {
            let entry = entry.map_err(err)?;
            let path = entry.path();
            let relative = path.strip_prefix(root).map_err(err)?;
            let kind = entry.file_type().map_err(err)?;
            if kind.is_symlink()
                || (!kind.is_file() && !kind.is_dir())
                || allowed.get(relative) != Some(&kind.is_dir())
            {
                return Err(err("unindexed recovery member, link or special file"));
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if entry.metadata().map_err(err)?.permissions().mode() & 0o077 != 0 {
                    return Err(err(
                        "recovery members must be private (Unix files 0600, directories 0700)",
                    ));
                }
            }
            if kind.is_dir() {
                directories.push(path);
            }
        }
    }
    for file in &index.files {
        if file_hash(root, &file.path, file.bytes)? != *file {
            return Err(err(format!(
                "member size/hash mismatch: {}",
                file.path.display()
            )));
        }
    }
    Ok(())
}

fn validate_contents(
    root: &Path,
    index: &RecoveryIndex,
    published: bool,
) -> Result<RegionInspection> {
    validate_inventory(root, index, published)?;
    let config = read_recovery_config(member(root, Path::new(CONFIG))?)?;
    check_supported(&config)?;
    if serde_json::to_value(&config).map_err(err)?
        != serde_json::to_value(normalized_config(&config, &index.source, None)).map_err(err)?
    {
        return Err(err(
            "configuration runtime paths must stay inside the recovery set",
        ));
    }
    let includes_politics = index
        .files
        .iter()
        .any(|file| file.path == Path::new(POLITICS));
    match &index.politics {
        Some(politics)
            if includes_politics
                && config.political_dataset_revision.as_ref()
                    == Some(&politics.dataset_identity) =>
        {
            read_politics(
                &member(root, Path::new(POLITICS))?,
                &politics.dataset_identity,
            )?;
        }
        None if !includes_politics && config.politics_file.is_none() => (),
        _ => {
            return Err(err(
                "political source inventory or configured identity differs",
            ));
        }
    }
    let expected_source_files = match &index.source {
        RecoverySource::Procedural { revision } => {
            if revision != PROCEDURAL_REVISION || &config.base_data_revision != revision {
                return Err(err("procedural source identity differs"));
            }
            BTreeSet::new()
        }
        RecoverySource::Prepared {
            package_identity,
            source_identity,
            entrypoint,
        } => {
            portable(entrypoint)?;
            if entrypoint.components().count() != 1 || &config.base_data_revision != source_identity
            {
                return Err(err("invalid prepared-source entrypoint or identity"));
            }
            let verified = verify_source_package(
                root.join("source"),
                Some(package_identity),
                Some(source_identity),
            )
            .map_err(err)?;
            if &verified.inspection.entrypoint != entrypoint
                || verified.inspection.options.coverage_level != config.chunk_level
                || verified.inspection.options.terrain_resolution != config.terrain_resolution
            {
                return Err(err(
                    "prepared-source settings differ from the saved configuration",
                ));
            }
            let mut files = BTreeSet::from([source_member(Path::new(SOURCE_PACKAGE_INDEX))]);
            files.extend(
                verified
                    .inspection
                    .files
                    .into_iter()
                    .map(|file| source_member(&file.path)),
            );
            files
        }
    };
    let actual_source_files = index
        .files
        .iter()
        .filter(|file| file.path.starts_with("source"))
        .map(|file| file.path.clone())
        .collect::<BTreeSet<_>>();
    if actual_source_files != expected_source_files {
        return Err(err(
            "source dependencies differ from the recovery inventory",
        ));
    }
    let store = RegionStore::open_read_only(member(root, Path::new(DATABASE))?).map_err(err)?;
    let archive = store.archive().map_err(err)?;
    if archive.source_generation != index.source_generation
        || archive.region_level != index.region_level
        || archive_hash(&archive)? != index.committed_archive_sha256
    {
        return Err(err(
            "checkpoint generation, region level or complete state differs",
        ));
    }
    let catalog: Catalog = serde_json::from_slice(&read_bounded(
        &member(root, Path::new(CATALOG))?,
        MAX_CATALOG_BYTES,
    )?)
    .map_err(err)?;
    if serde_json::to_value(catalog).map_err(err)?
        != serde_json::to_value(&archive.catalog).map_err(err)?
    {
        return Err(err("catalog copy differs from the committed database"));
    }
    validate_local(&archive, &config)?;
    let mut inspection = store.inspect().map_err(err)?;
    // Generic RegionStore inspection deliberately leaves this requirement open.
    // This wrapper has now run the actual supported local-host validator too.
    inspection.game_metadata_requires_host_validation = false;
    Ok(inspection)
}

fn read_index(root: &Path, expected: Option<&str>) -> Result<(RecoveryIndex, String)> {
    let bytes = read_bounded(&member(root, Path::new(RECOVERY_INDEX))?, MAX_INDEX_BYTES)?;
    let identity = sha256(&bytes);
    if expected.is_some_and(|expected| expected != identity) {
        return Err(err(
            "bundle identity differs from the expected trusted receipt",
        ));
    }
    let index: RecoveryIndex = serde_json::from_slice(&bytes).map_err(err)?;
    if serde_json::to_vec_pretty(&index).map_err(err)? != bytes {
        return Err(err("recovery index is not in canonical export form"));
    }
    Ok((index, identity))
}

pub fn verify_recovery_bundle(
    directory: impl AsRef<Path>,
    expected_bundle: Option<&str>,
) -> Result<RecoveryVerification> {
    let root = no_links(directory.as_ref())?;
    if !root.is_dir() {
        return Err(err("recovery set must be a directory"));
    }
    let (index, identity) = read_index(&root, expected_bundle)?;
    let inspection = validate_contents(&root, &index, true)?;
    Ok(RecoveryVerification {
        bundle_identity: identity,
        source_generation: index.source_generation,
        region_level: index.region_level,
        file_count: index.files.len(),
        total_bytes: index.total_bytes,
        prepared_source: matches!(index.source, RecoverySource::Prepared { .. }),
        political_source: index.politics.is_some(),
        domain_validated: true,
        local_host_metadata_validated: true,
        catalog_exact: true,
        source_dependencies_validated: true,
        inspection,
    })
}

/// Restore to a newly created private directory. The emitted config uses
/// absolute contained paths, so it can be launched from another working directory.
/// No mutation or inspection of the damaged/original world is required.
pub fn restore_recovery_bundle(
    directory: impl AsRef<Path>,
    destination: impl AsRef<Path>,
    expected_bundle: Option<&str>,
) -> Result<RecoveryRestoreReceipt> {
    restore_recovery_bundle_with_sync(
        directory,
        destination,
        expected_bundle,
        &terra_sandbox::publication::sync_directory,
    )
    .map_err(|error| error.to_string())
}

/// Restore with a structural final-publication result for containing wrappers.
pub fn restore_recovery_bundle_with_sync(
    directory: impl AsRef<Path>,
    destination: impl AsRef<Path>,
    expected_bundle: Option<&str>,
    sync: &dyn Fn(&Path) -> std::io::Result<()>,
) -> std::result::Result<RecoveryRestoreReceipt, RecoveryPublicationError> {
    let source = no_links(directory.as_ref())?;
    let destination = new_destination(destination.as_ref())?;
    if destination.starts_with(&source) {
        return Err(err("restore destination must be outside the retained recovery bundle").into());
    }
    let verification = verify_recovery_bundle(&source, expected_bundle)?;
    let (index, identity) = read_index(&source, Some(&verification.bundle_identity))?;
    let config = read_recovery_config(source.join(CONFIG))?;
    let destination = new_private_directory(&destination)?;
    let mut published = false;
    let result: Result<RecoveryRestoreReceipt> = (|| {
        let store = RegionStore::open_read_only(source.join(DATABASE)).map_err(err)?;
        store
            .backup_with_options(destination.join(DATABASE), RegionBackupOptions::default())
            .map_err(err)?;
        if let RecoverySource::Prepared {
            package_identity,
            source_identity,
            ..
        } = &index.source
        {
            install_source_package(
                source.join("source"),
                destination.join("source"),
                Some(package_identity),
                Some(source_identity),
            )
            .map_err(err)?;
        }
        if let Some(politics) = &index.politics {
            let bytes = read_politics(&source.join(POLITICS), &politics.dataset_identity)?;
            let admitted = index
                .files
                .iter()
                .find(|file| file.path == Path::new(POLITICS))
                .ok_or_else(|| err("political source is missing from inventory"))?;
            if bytes.len() as u64 != admitted.bytes || sha256(&bytes) != admitted.sha256 {
                return Err(err("political source changed during restore"));
            }
            write_new_private_file(destination.join(POLITICS), &bytes).map_err(err)?;
        }
        let restored_config = normalized_config(&config, &index.source, Some(&destination));
        json_new(&destination.join(CONFIG), &restored_config)?;
        let restored = RegionStore::open_read_only(destination.join(DATABASE)).map_err(err)?;
        let archive = restored.archive().map_err(err)?;
        validate_local(&archive, &restored_config)?;
        if archive.source_generation != index.source_generation
            || archive_hash(&archive)? != index.committed_archive_sha256
        {
            return Err(err(
                "restored checkpoint is not the complete accepted generation",
            ));
        }
        json_new(&destination.join(CATALOG), &archive.catalog)?;
        privatize_tree(&destination)?;
        // A final hash pass binds unchanged original members; copied source and
        // checkpoint were independently validated before the receipt is emitted.
        validate_inventory(&source, &index, true)?;
        read_index(&source, Some(&identity))?;
        let receipt = RecoveryRestoreReceipt {
            schema_version: 1,
            bundle_identity: identity,
            committed_generation: archive.source_generation,
            committed_archive_sha256: index.committed_archive_sha256,
            domain_and_local_metadata_exact: true,
            source_and_catalog_exact: true,
            runtime_paths_rebased: true,
            original_bundle_unchanged: true,
        };
        terra_sandbox::publication::sync_tree(&destination, MAX_FILES * 19, MAX_TOTAL_BYTES)
            .map_err(err)?;
        let bytes = serde_json::to_vec_pretty(&receipt).map_err(err)?;
        terra_sandbox::publication::write_private_with_sync(
            &destination.join("restore-receipt.json"),
            &bytes,
            terra_sandbox::publication::PublishMode::CreateNew,
            sync,
        )
        .map_err(|error| {
            published = error.published();
            err(error)
        })?;
        published = true;
        Ok(receipt)
    })();
    if result.is_err() && !published {
        let _ = fs::remove_dir_all(&destination);
    }
    result.map_err(|message| RecoveryPublicationError { message, published })
}

#[cfg(test)]
#[path = "recovery_tests.rs"]
mod tests;
