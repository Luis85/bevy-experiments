//! Explicit offline migration and online-backup tooling. Portable archives
//! include private host metadata; public inspection returns counters only.
use crate::{MAX_DOCUMENT_BYTES, RegionStore, Result, STORE_SCHEMA_VERSION, StoreError};
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeSet,
    fs,
    io::Read,
    path::Path,
    time::{Duration, Instant},
};
use terra_sandbox::{Catalog, SandboxState};

pub const MAX_REGION_ARCHIVE_BYTES: usize = 128 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegionArchive {
    pub format_version: u32,
    pub source_generation: u64,
    pub identity: Value,
    pub catalog: Catalog,
    pub region_level: u8,
    pub state: SandboxState,
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegionInspection {
    pub database_schema: i64,
    pub generation: u64,
    pub region_level: u8,
    pub nonempty_regions: usize,
    pub paged_records: usize,
    pub global_records: usize,
    pub checkpoint_payload_bytes: u64,
    pub checkpoint_limit_bytes: u64,
    pub checkpoint_remaining_bytes: u64,
    pub region_limit: usize,
    pub players: usize,
    pub living_bodies: usize,
    pub resources: usize,
    pub structures: usize,
    pub domain_validated: bool,
    pub game_metadata_requires_host_validation: bool,
}

/// A bound on online backup retries. Continuous source churn or lock contention
/// cannot leave an operator waiting indefinitely. Progress is copied in small
/// batches; validation and disk sync remain explicit maintenance work.
#[derive(Debug, Clone, Copy)]
pub struct RegionBackupOptions {
    pub max_copy_time: Duration,
    pub pages_per_step: u32,
}
impl Default for RegionBackupOptions {
    fn default() -> Self {
        Self {
            max_copy_time: Duration::from_secs(30),
            pages_per_step: 128,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegionBackupReceipt {
    /// Read from the verified snapshot, not a stale pre-copy source inspection.
    pub generation: u64,
    pub checkpoint_payload_bytes: u64,
    pub nonempty_regions: usize,
    pub copied_pages: i32,
    pub copy_steps: u64,
    pub elapsed_ms: f64,
    pub domain_validated: bool,
    pub game_metadata_requires_host_validation: bool,
}

impl RegionStore {
    /// Copy committed WAL state into a private temporary destination, validate
    /// that exact snapshot, sync it, then atomically publish a new file. A failed
    /// copy/validation leaves no final destination and never rewrites the source.
    /// The copy retry deadline is bounded; full domain validation follows it.
    pub fn backup_with_options(
        &self,
        path: impl AsRef<Path>,
        options: RegionBackupOptions,
    ) -> Result<RegionBackupReceipt> {
        self.backup_with_options_and_sync(
            path.as_ref(),
            options,
            terra_sandbox::publication::sync_directory,
        )
    }

    pub(crate) fn backup_with_options_and_sync(
        &self,
        path: &Path,
        options: RegionBackupOptions,
        sync: impl FnOnce(&Path) -> std::io::Result<()>,
    ) -> Result<RegionBackupReceipt> {
        if options.max_copy_time.is_zero()
            || options.max_copy_time > Duration::from_secs(300)
            || !(1..=4096).contains(&options.pages_per_step)
        {
            return Err(StoreError::Invalid(
                "backup requires a copy deadline in (0, 300] seconds and 1..4096 pages per step"
                    .into(),
            ));
        }
        let path = path.as_ref();
        self.check_output_path(path)?;
        crate::require_new_database_destination(path)?;
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        terra_sandbox::publication::create_private_directories(parent)?;
        let temporary = tempfile::Builder::new()
            .prefix(".terra-backup-")
            .tempdir_in(parent)?;
        let snapshot = temporary.path().join("snapshot.db");
        crate::create_private_file(&snapshot)?;
        let started = Instant::now();
        let (copied_pages, copy_steps) = {
            let mut destination = Connection::open_with_flags(
                &snapshot,
                OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NOFOLLOW,
            )?;
            destination.busy_timeout(Duration::from_millis(20))?;
            let backup = rusqlite::backup::Backup::new(&self.connection, &mut destination)?;
            let mut steps = 0u64;
            loop {
                if started.elapsed() >= options.max_copy_time {
                    return Err(StoreError::Invalid("backup copy deadline exceeded; no destination was published. Pause source writes or release the database lock, then retry to a new path".into()));
                }
                let outcome = backup.step(options.pages_per_step as i32)?;
                steps = steps.saturating_add(1);
                match outcome {
                    rusqlite::backup::StepResult::Done => {
                        break (backup.progress().pagecount, steps);
                    }
                    rusqlite::backup::StepResult::More => (),
                    // SQLite BUSY/LOCKED are retryable, but only until the deadline.
                    _ => std::thread::sleep(Duration::from_millis(5)),
                }
            }
        };
        let evidence = {
            // Compare the pinned descriptor, then decode every committed page.
            // This also rejects corruption changed behind an existing handle.
            let verified = Self::open(&snapshot, self.identity.clone(), &self.catalog, self.level)?;
            let evidence = verified.inspect()?;
            let mode: String =
                verified
                    .connection
                    .query_row("PRAGMA journal_mode=DELETE", [], |row| row.get(0))?;
            if !mode.eq_ignore_ascii_case("delete") {
                return Err(StoreError::Invalid("backup could not consolidate its private snapshot journal; no destination was published".into()));
            }
            evidence
        };
        // The shared publisher retains a visible backup if directory sync fails
        // and distinguishes that outcome from an untouched destination.
        crate::require_new_database_destination(path)?;
        terra_sandbox::publication::link_private_file_with_sync(&snapshot, path, sync)?;
        Ok(RegionBackupReceipt {
            generation: evidence.generation,
            checkpoint_payload_bytes: evidence.checkpoint_payload_bytes,
            nonempty_regions: evidence.nonempty_regions,
            copied_pages,
            copy_steps,
            elapsed_ms: started.elapsed().as_secs_f64() * 1000.,
            domain_validated: evidence.domain_validated,
            game_metadata_requires_host_validation: true,
        })
    }

    /// Open an existing store using its recorded identity/catalog. This never
    /// intentionally creates a missing source or guesses a catalog. The normal
    /// open path then rechecks schema, identity, limits and SQLite integrity.
    pub fn open_existing(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let descriptor = Self::open_read_only(path)?;
        let identity = descriptor.identity.clone();
        let catalog = descriptor.catalog.clone();
        let level = descriptor.level;
        drop(descriptor);
        Self::open(path, identity, &catalog, level)
    }

    /// Open a maintenance reader without changing the source journal mode or
    /// database content. WAL-aware reads include the current committed snapshot.
    /// SQLite may access its ordinary shared-memory sidecar; query_only also
    /// prevents accidental writes through this handle.
    pub fn open_read_only(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let file = fs::symlink_metadata(path)?;
        if !file.is_file()
            || file.file_type().is_symlink()
            || file.len() > crate::MAX_DATABASE_BYTES
        {
            return Err(StoreError::Invalid(
                "source database must be a bounded regular file".into(),
            ));
        }
        let connection = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )?;
        connection.set_limit(
            rusqlite::limits::Limit::SQLITE_LIMIT_LENGTH,
            crate::MAX_CHECKPOINT_BYTES as i32 + 8192,
        )?;
        connection.busy_timeout(Duration::from_millis(250))?;
        connection.execute_batch("PRAGMA trusted_schema=OFF; PRAGMA query_only=ON;")?;
        let application: i64 = connection.query_row("PRAGMA application_id", [], |r| r.get(0))?;
        let schema: i64 = connection.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if application != crate::APPLICATION_ID || schema != STORE_SCHEMA_VERSION {
            return Err(StoreError::Invalid(
                "unsupported or foreign database schema".into(),
            ));
        }
        crate::validate_schema(&connection)?;
        let (identity_length, catalog_length): (i64, i64) = connection.query_row(
            "SELECT length(identity),length(catalog) FROM world WHERE singleton=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        if !(1..=65_536).contains(&identity_length)
            || !(1..=MAX_DOCUMENT_BYTES as i64).contains(&catalog_length)
        {
            return Err(StoreError::Invalid(
                "database descriptor exceeds bounds".into(),
            ));
        }
        let (identity, catalog, level): (Vec<u8>, Vec<u8>, u8) = connection.query_row(
            "SELECT identity,catalog,region_level FROM world WHERE singleton=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        let identity_value: Value = serde_json::from_slice(&identity)?;
        let catalog_value: Catalog = serde_json::from_slice(&catalog)?;
        if identity_value.is_null() {
            return Err(StoreError::Invalid("null world identity".into()));
        }
        catalog_value.validate()?;
        crate::RegionKey::new(level, 0, 0).map_err(|e| StoreError::Invalid(e.to_string()))?;
        let integrity: String = connection.query_row("PRAGMA quick_check(1)", [], |r| r.get(0))?;
        if integrity != "ok" {
            return Err(StoreError::Invalid(
                "database integrity check failed".into(),
            ));
        }
        if rusqlite::version_number() < 3_051_003 {
            return Err(StoreError::Invalid(
                "SQLite 3.51.3 or newer is required".into(),
            ));
        }
        let generation = crate::read_generation(&connection)?;
        Ok(Self {
            connection,
            path: fs::canonicalize(path)?,
            identity: identity_value,
            descriptor_digest: crate::hash_descriptor(&identity, &catalog, level),
            catalog: catalog_value,
            level,
            generation,
            active_regions: BTreeSet::new(),
            status: crate::RegionStoreStatus {
                generation,
                ..Default::default()
            },
            restored: generation == 0,
            fully_materialized: generation == 0,
            read_cache: std::cell::RefCell::new(None),
            cache_hits: std::cell::Cell::new(0),
            decoded_checkpoints: std::cell::Cell::new(0),
            checkpoint_metrics: crate::RegionCheckpointMetrics::default(),
        })
    }

    /// Verify a complete checkpoint and report bounded, nonsecret evidence.
    pub fn inspect(&self) -> Result<RegionInspection> {
        self.invalidate_read_cache();
        let (partition, _, generation, bytes) = self
            .read_partition(false)?
            .ok_or_else(|| StoreError::Invalid("store has no committed checkpoint".into()))?;
        let state = partition.assemble_all(&self.catalog)?;
        Ok(RegionInspection {
            database_schema: STORE_SCHEMA_VERSION,
            generation,
            region_level: self.region_level(),
            nonempty_regions: partition.pages.len(),
            paged_records: partition.record_count(),
            global_records: partition.global_record_count(),
            checkpoint_payload_bytes: bytes,
            checkpoint_limit_bytes: crate::MAX_CHECKPOINT_BYTES as u64,
            checkpoint_remaining_bytes: (crate::MAX_CHECKPOINT_BYTES as u64).saturating_sub(bytes),
            region_limit: crate::MAX_REGION_PAGES,
            players: state.players.len(),
            living_bodies: state.living.units.len(),
            resources: state.resource_nodes.len(),
            structures: state.structures.len(),
            domain_validated: true,
            game_metadata_requires_host_validation: true,
        })
    }

    /// Export the last committed world, including dormant records and private
    /// game metadata. The authority should checkpoint first to include live work.
    pub fn archive(&self) -> Result<RegionArchive> {
        let (partition, metadata, generation, _) = self
            .read_partition(false)?
            .ok_or_else(|| StoreError::Invalid("store has no committed checkpoint".into()))?;
        Ok(RegionArchive {
            format_version: 1,
            source_generation: generation,
            identity: self.identity.clone(),
            catalog: self.catalog.clone(),
            region_level: self.level,
            state: partition.assemble_all(&self.catalog)?,
            metadata,
        })
    }
}

impl RegionArchive {
    pub fn validate(&self) -> Result<()> {
        if self.format_version != 1
            || self.identity.is_null()
            || serde_json::to_vec(&self.identity)?.len() > 65_536
        {
            return Err(StoreError::Invalid("unsupported regional archive".into()));
        }
        crate::RegionKey::new(self.region_level, 0, 0)
            .map_err(|e| StoreError::Invalid(e.to_string()))?;
        self.catalog.validate()?;
        self.state.require_complete()?;
        self.state.validate_with_catalog(&self.catalog)?;
        Ok(())
    }
    pub fn read(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let metadata = fs::symlink_metadata(path)?;
        if !metadata.is_file()
            || metadata.file_type().is_symlink()
            || metadata.len() > MAX_REGION_ARCHIVE_BYTES as u64
        {
            return Err(StoreError::Invalid(
                "archive must be a bounded regular file".into(),
            ));
        }
        let mut bytes = Vec::new();
        fs::File::open(path)?
            .take(MAX_REGION_ARCHIVE_BYTES as u64 + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() > MAX_REGION_ARCHIVE_BYTES {
            return Err(StoreError::Invalid("archive is oversized".into()));
        }
        let archive: Self = serde_json::from_slice(&bytes)?;
        archive.validate()?;
        Ok(archive)
    }
    pub fn write_new(&self, path: impl AsRef<Path>) -> Result<()> {
        self.validate()?;
        let bytes = serde_json::to_vec_pretty(self)?;
        write_new_private_file(path, &bytes)
    }
    /// Build and verify in an isolated temporary store, then create the final
    /// destination exclusively through SQLite backup. The source generation is
    /// provenance; the new database starts at generation one. Host identities
    /// and resume capabilities remain in the preserved metadata.
    pub fn import_new(&self, path: impl AsRef<Path>) -> Result<crate::RegionStoreStatus> {
        self.validate()?;
        let path = path.as_ref();
        crate::require_new_database_destination(path)?;
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        terra_sandbox::publication::create_private_directories(parent)?;
        let temporary = tempfile::Builder::new()
            .prefix(".terra-import-")
            .tempdir_in(parent)?;
        let mut store = RegionStore::open(
            temporary.path().join("world.db"),
            self.identity.clone(),
            &self.catalog,
            self.region_level,
        )?;
        let mut state = self.state.clone();
        let status = store.checkpoint(&mut state, self.metadata.clone(), &BTreeSet::new())?;
        store.backup_to(path)?;
        Ok(status)
    }
}

/// Publish an explicit private export atomically without replacing any existing
/// destination, including symlinks. Call `validate_output_path` first when a
/// live database is involved. This helper never writes into that database.
pub fn write_new_private_file(path: impl AsRef<Path>, bytes: &[u8]) -> Result<()> {
    if bytes.len() > MAX_REGION_ARCHIVE_BYTES {
        return Err(StoreError::Invalid("export is oversized".into()));
    }
    terra_sandbox::publication::write_private(
        path.as_ref(),
        bytes,
        terra_sandbox::publication::PublishMode::CreateNew,
    )?;
    Ok(())
}
