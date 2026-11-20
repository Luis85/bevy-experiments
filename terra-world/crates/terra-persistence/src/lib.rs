//! Durable regional mutable-world storage for a single local authority.
//!
//! Immutable terrain remains in the source cache. This store commits the global
//! gameplay ledger, geographic pages and host metadata in one SQLite transaction.
//! The active simulation contains only the requested dependency-closed pages.
//! Dormant environment records freeze; the retained economy keeps running.
//!
//! Aggregate limits remain. Checkpoints certify unchanged environmental groups
//! and reuse equal page bytes, while validating current ledgers and actual
//! dependent cold records. Untrusted decode validates the complete world.
//! This is one serialized authority, not a distributed or concurrent tick writer.

use rusqlite::{Connection, OpenFlags, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};
use terra_sandbox::{Catalog, RegionPage, RegionPartition, SandboxState, ValidatedRegionPartition};
use thiserror::Error;

mod worker;
pub use worker::*;
mod operations;
pub use operations::*;

pub use terra_sandbox::RegionKey;

const APPLICATION_ID: i64 = 0x54455252;
pub const STORE_SCHEMA_VERSION: i64 = 1;
pub const MAX_REGION_PAGES: usize = 4096;
pub const MAX_REQUESTED_REGIONS: usize = 1024;
pub const MAX_DOCUMENT_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_CHECKPOINT_BYTES: usize = 64 * 1024 * 1024;
const MAX_DATABASE_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("regional storage is busy; wait for the pending operation before submitting more work")]
    Busy,
    #[error("regional storage worker stopped: {0}")]
    Worker(String),
    #[error("regional store: {0}")]
    Invalid(String),
    #[error("regional store belongs to a different source world, catalog, or region level")]
    IdentityMismatch,
    #[error(
        "regional store was changed by another writer (expected generation {expected}, found {actual}); stop and reload before writing"
    )]
    Conflict { expected: u64, actual: u64 },
    #[error(transparent)]
    Domain(#[from] terra_sandbox::SandboxError),
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Publication(#[from] terra_sandbox::publication::PublicationError),
}

pub type Result<T> = std::result::Result<T, StoreError>;

/// Safe operational counters. Never includes host metadata, resume tokens,
/// account balances, private inventory, or filesystem paths.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegionStoreStatus {
    pub generation: u64,
    pub active_regions: usize,
    pub dormant_regions: usize,
    pub pinned_regions: usize,
    pub dependency_regions: usize,
    /// Page records only; retained ledger records are counted separately.
    pub active_records: usize,
    pub dormant_records: usize,
    pub global_records: usize,
    /// Serialized payload bytes in the complete last committed checkpoint.
    pub checkpoint_bytes: u64,
}

/// The complete validated world at a committed generation. Hosts must validate
/// their metadata before installing it in the live application.
pub struct StoredRegionWorld {
    pub state: SandboxState,
    pub metadata: Value,
    pub generation: u64,
}

/// One authority's handle. A second handle may inspect/read the same database,
/// but stale writes are rejected by a generation compare-and-swap inside BEGIN
/// IMMEDIATE. SQLite owns crash recovery. Do not put the database on a network
/// filesystem or copy just its main file while its WAL is live.
pub struct RegionStore {
    connection: Connection,
    path: PathBuf,
    identity: Value,
    descriptor_digest: blake3::Hash,
    catalog: Catalog,
    level: u8,
    generation: u64,
    active_regions: BTreeSet<RegionKey>,
    status: RegionStoreStatus,
    restored: bool,
    fully_materialized: bool,
    read_cache: RefCell<Option<CachedPartition>>,
    cache_hits: Cell<u64>,
    decoded_checkpoints: Cell<u64>,
    checkpoint_metrics: RegionCheckpointMetrics,
}

struct CachedPartition {
    generation: u64,
    data_version: i64,
    total_changes: u64,
    partition: ValidatedRegionPartition,
    metadata: Value,
    bytes: u64,
    encoded_pages: BTreeMap<RegionKey, EncodedPage>,
}

#[derive(Clone)]
struct EncodedPage {
    digest: String,
    payload: Arc<[u8]>,
}

/// Latest successful checkpoint work, split into measured stages. Counters never
/// contain player data. A failed attempt leaves this successful record intact.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RegionCheckpointMetrics {
    pub compose_ms: f64,
    pub select_ms: f64,
    pub encode_ms: f64,
    pub transaction_ms: f64,
    pub install_ms: f64,
    pub validation: terra_sandbox::RegionValidationWork,
    pub selected_records_validated: usize,
    pub encoded_pages: usize,
    pub encoded_page_bytes: u64,
    pub reused_pages: usize,
    pub reused_page_bytes: u64,
    pub deleted_pages: usize,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct RegionCacheStats {
    pub hits: u64,
    pub decoded_checkpoints: u64,
}

impl RegionStore {
    /// Open/create a private database. The immutable identity is game-defined
    /// JSON (normally source revision, epoch, seed, politics and host kind).
    /// An existing foreign, corrupt or incompatible database is never reset.
    pub fn open(
        path: impl AsRef<Path>,
        identity: Value,
        catalog: &Catalog,
        region_level: u8,
    ) -> Result<Self> {
        RegionKey::new(region_level, 0, 0).map_err(|e| StoreError::Invalid(e.to_string()))?;
        catalog.validate()?;
        let identity_bytes = encode(&identity)?;
        if identity_bytes.len() > 64 * 1024 || identity.is_null() {
            return Err(StoreError::Invalid(
                "invalid or oversized world identity".into(),
            ));
        }
        let catalog_bytes = encode(catalog)?;
        let path = path.as_ref();
        prepare_private_file(path)?;
        let connection = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_NO_MUTEX
                | OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )?;
        connection.busy_timeout(Duration::from_millis(250))?;
        connection.set_limit(
            rusqlite::limits::Limit::SQLITE_LIMIT_LENGTH,
            MAX_CHECKPOINT_BYTES as i32 + 8192,
        )?;
        connection.execute_batch("PRAGMA trusted_schema=OFF; PRAGMA foreign_keys=ON;")?;
        let objects: i64 = connection.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE name NOT LIKE 'sqlite_%'",
            [],
            |row| row.get(0),
        )?;
        let app_id: i64 = connection.query_row("PRAGMA application_id", [], |r| r.get(0))?;
        let schema: i64 = connection.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if objects == 0 && app_id == 0 && schema == 0 {
            connection.execute_batch(&format!(
                "BEGIN IMMEDIATE;
                 PRAGMA application_id={APPLICATION_ID};
                 PRAGMA user_version={STORE_SCHEMA_VERSION};
                 CREATE TABLE world (
                   singleton INTEGER PRIMARY KEY CHECK(singleton=1),
                   identity BLOB NOT NULL, catalog BLOB NOT NULL,
                   region_level INTEGER NOT NULL,
                   generation INTEGER NOT NULL CHECK(generation>=0),
                   global_state BLOB, metadata BLOB, digest TEXT,
                   payload_bytes INTEGER NOT NULL DEFAULT 0
                 );
                 CREATE TABLE regions (
                   level INTEGER NOT NULL, x INTEGER NOT NULL, y INTEGER NOT NULL,
                   payload BLOB NOT NULL, digest TEXT NOT NULL,
                   PRIMARY KEY(level,x,y)
                 );"
            ))?;
            connection.execute(
                "INSERT INTO world(singleton,identity,catalog,region_level,generation) VALUES(1,?1,?2,?3,0)",
                params![identity_bytes, catalog_bytes, region_level],
            )?;
            connection.execute_batch("COMMIT;")?;
        } else if app_id != APPLICATION_ID || schema != STORE_SCHEMA_VERSION || objects != 2 {
            return Err(StoreError::Invalid(
                "unsupported or foreign database schema".into(),
            ));
        }
        validate_schema(&connection)?;
        let (stored_identity, stored_catalog, stored_level) = read_descriptor(&connection)?;
        if serde_json::from_slice::<Value>(&stored_identity)? != identity
            || serde_json::from_slice::<Value>(&stored_catalog)? != serde_json::to_value(catalog)?
            || stored_level != region_level
        {
            return Err(StoreError::IdentityMismatch);
        }
        let integrity: String = connection.query_row("PRAGMA quick_check(1)", [], |r| r.get(0))?;
        if integrity != "ok" {
            return Err(StoreError::Invalid(
                "database integrity check failed".into(),
            ));
        }
        // The bundled engine must include SQLite's March 2026 WAL-reset fix.
        if rusqlite::version_number() < 3_051_003 {
            return Err(StoreError::Invalid(
                "SQLite 3.51.3 or newer is required".into(),
            ));
        }
        let mode: String = connection.query_row("PRAGMA journal_mode=WAL", [], |r| r.get(0))?;
        if !mode.eq_ignore_ascii_case("wal") {
            return Err(StoreError::Invalid("WAL journaling is unavailable".into()));
        }
        connection.execute_batch(
            "PRAGMA synchronous=FULL; PRAGMA wal_autocheckpoint=256;
             PRAGMA journal_size_limit=16777216; PRAGMA max_page_count=65536;",
        )?;
        let generation = read_generation(&connection)?;
        Ok(Self {
            connection,
            path: fs::canonicalize(path)?,
            identity,
            descriptor_digest: hash_descriptor(&stored_identity, &stored_catalog, stored_level),
            catalog: catalog.clone(),
            level: region_level,
            generation,
            active_regions: BTreeSet::new(),
            status: RegionStoreStatus {
                generation,
                ..Default::default()
            },
            restored: generation == 0,
            fully_materialized: generation == 0,
            read_cache: RefCell::new(None),
            cache_hits: Cell::new(0),
            decoded_checkpoints: Cell::new(0),
            checkpoint_metrics: RegionCheckpointMetrics::default(),
        })
    }

    pub fn is_initialized(&self) -> bool {
        self.generation != 0
    }
    pub fn region_level(&self) -> u8 {
        self.level
    }
    pub fn status(&self) -> RegionStoreStatus {
        self.status.clone()
    }
    pub fn checkpoint_metrics(&self) -> RegionCheckpointMetrics {
        self.checkpoint_metrics.clone()
    }
    pub fn cache_stats(&self) -> RegionCacheStats {
        RegionCacheStats {
            hits: self.cache_hits.get(),
            decoded_checkpoints: self.decoded_checkpoints.get(),
        }
    }

    /// Explicit inspection/scrubbing can force the next read to decode again.
    pub fn invalidate_read_cache(&self) {
        *self.read_cache.borrow_mut() = None;
    }

    /// Reject an export/log/screenshot destination that aliases this database
    /// or one of SQLite's journal files. Call before any alternate-format write.
    pub fn check_output_path(&self, output: impl AsRef<Path>) -> Result<()> {
        validate_output_path(&self.path, output.as_ref())
    }

    /// Explicitly load the newest committed generation. No simulation state is
    /// installed by this method; validate host metadata before using the result.
    pub fn restore(&mut self) -> Result<Option<StoredRegionWorld>> {
        self.restore_validated(|_| Ok(()))
    }

    /// Validate game-owned metadata and state together before changing this
    /// handle's generation or residency cursor. Use this for a live-world load:
    /// a rejected clock/session envelope must also leave future merges intact.
    pub fn restore_validated(
        &mut self,
        validate: impl FnOnce(&StoredRegionWorld) -> std::result::Result<(), String>,
    ) -> Result<Option<StoredRegionWorld>> {
        let Some((partition, metadata, generation, bytes)) = self.read_partition(false)? else {
            return Ok(None);
        };
        let state = partition.assemble()?;
        let saved = StoredRegionWorld {
            state,
            metadata,
            generation,
        };
        validate(&saved).map_err(StoreError::Invalid)?;
        self.generation = generation;
        self.active_regions = partition.pages.keys().copied().collect();
        self.restored = true;
        self.fully_materialized = true;
        self.status = RegionStoreStatus {
            generation,
            active_regions: self.active_regions.len(),
            active_records: partition.record_count(),
            global_records: partition.global_record_count(),
            checkpoint_bytes: bytes,
            ..Default::default()
        };
        Ok(Some(saved))
    }

    /// Compose current active mutations with dormant committed records. The
    /// caller can use this for legacy full-world exports without changing live
    /// residency. Stale handles and corrupt records fail without partial output.
    pub fn export(&self, active: &SandboxState) -> Result<SandboxState> {
        self.compose(active)?.assemble().map_err(Into::into)
    }

    /// Activation barrier before source registration or host edits that need
    /// global lookup. In particular, registration must never reseed a dormant
    /// depleted node. The next checkpoint can retire unrequested pages again.
    pub fn hydrate(&mut self, active: &mut SandboxState) -> Result<()> {
        if self.restored && self.fully_materialized {
            // Full residency is a live-state property, not proof that the
            // committed source is still sound. Recheck its guarded snapshot
            // before admitting registration/host edits. Unchanged checkpoints
            // reuse the read cache; external writes force complete validation.
            // Never install the stored snapshot over the caller's live edits.
            self.read_partition(true)?;
            active.require_complete()?;
            active.validate_with_catalog(&self.catalog)?;
            return Ok(());
        }
        let partition = self.compose(active)?;
        let state = partition.assemble()?;
        let keys = partition.pages.keys().copied().collect();
        self.status.active_regions = partition.pages.len();
        self.status.dormant_regions = 0;
        self.status.active_records = partition.record_count();
        self.status.dormant_records = 0;
        self.status.global_records = partition.global_record_count();
        self.status.pinned_regions = 0;
        self.status.dependency_regions = 0;
        self.active_regions = keys;
        self.fully_materialized = true;
        *active = state;
        Ok(())
    }

    /// Commit all mutable changes, host metadata and residency transition as one
    /// generation. Records moving across region boundaries are transferred in
    /// the same transaction. On every error the caller's active state, our
    /// generation and the previous committed checkpoint remain unchanged.
    pub fn checkpoint(
        &mut self,
        active: &mut SandboxState,
        metadata: Value,
        requested: &BTreeSet<RegionKey>,
    ) -> Result<RegionStoreStatus> {
        if requested.len() > MAX_REQUESTED_REGIONS {
            return Err(StoreError::Invalid(
                "requested region budget exceeded".into(),
            ));
        }
        for key in requested {
            key.validate()
                .map_err(|e| StoreError::Invalid(e.to_string()))?;
            if key.level != self.level {
                return Err(StoreError::Invalid(
                    "requested region uses another level".into(),
                ));
            }
        }
        // Observe this connection's external-write epoch before composing the
        // snapshot. Generation alone cannot detect an out-of-band edit that
        // changes stored content without advancing the authority's counter.
        let prepared_data_version: i64 =
            self.connection
                .query_row("PRAGMA data_version", [], |r| r.get(0))?;
        let mut metrics = RegionCheckpointMetrics::default();
        let stage = Instant::now();
        let partition = self.compose(active)?;
        metrics.compose_ms = stage.elapsed().as_secs_f64() * 1000.;
        metrics.validation = partition.validation_work();
        let stage = Instant::now();
        let working = partition.select(requested)?;
        metrics.select_ms = stage.elapsed().as_secs_f64() * 1000.;
        metrics.selected_records_validated = working.record_count();
        let stage = Instant::now();
        let global = encode(&partition.global)?;
        let metadata_value = metadata;
        let metadata = encode(&metadata_value)?;
        let mut payload_bytes = global.len() + metadata.len();
        let mut encoded_pages = BTreeMap::new();
        let mut changed_pages = BTreeSet::new();
        if partition.pages.len() > MAX_REGION_PAGES {
            return Err(StoreError::Invalid("region page budget exceeded".into()));
        }
        {
            // compose() has just checked generation, schema, descriptor,
            // data_version and this connection's changes. Equality against that
            // immutable proof is exact, not a caller-provided dirty flag.
            let cache = self.read_cache.borrow();
            for (key, page) in &partition.pages {
                let cached = cache.as_ref().and_then(|cache| {
                    (cache.partition.pages.get(key) == Some(page))
                        .then(|| cache.encoded_pages.get(key))
                        .flatten()
                });
                let encoded = if let Some(encoded) = cached {
                    metrics.reused_pages += 1;
                    metrics.reused_page_bytes += encoded.payload.len() as u64;
                    encoded.clone()
                } else {
                    let payload = encode(page)?;
                    metrics.encoded_pages += 1;
                    metrics.encoded_page_bytes += payload.len() as u64;
                    changed_pages.insert(*key);
                    EncodedPage {
                        digest: blake3::hash(&payload).to_hex().to_string(),
                        payload: Arc::from(payload),
                    }
                };
                payload_bytes = payload_bytes
                    .checked_add(encoded.payload.len())
                    .ok_or_else(|| StoreError::Invalid("checkpoint size overflow".into()))?;
                if payload_bytes > MAX_CHECKPOINT_BYTES {
                    return Err(StoreError::Invalid(
                        "checkpoint exceeds 64 MiB budget".into(),
                    ));
                }
                encoded_pages.insert(*key, encoded);
            }
        }
        if payload_bytes > MAX_CHECKPOINT_BYTES {
            return Err(StoreError::Invalid(
                "checkpoint exceeds 64 MiB budget".into(),
            ));
        }
        let mut digest = blake3::Hasher::new();
        digest.update(&global);
        digest.update(&metadata);
        let digest = digest.finalize().to_hex().to_string();
        let next = self
            .generation
            .checked_add(1)
            .filter(|n| *n <= i64::MAX as u64)
            .ok_or_else(|| StoreError::Invalid("generation exhausted".into()))?;
        metrics.encode_ms = stage.elapsed().as_secs_f64() * 1000.;
        let stage = Instant::now();
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        validate_schema(&tx)?;
        validate_descriptor(&tx, self.descriptor_digest)?;
        let actual = read_generation(&tx)?;
        if actual != self.generation {
            return Err(StoreError::Conflict {
                expected: self.generation,
                actual,
            });
        }
        let data_version: i64 = tx.query_row("PRAGMA data_version", [], |r| r.get(0))?;
        if data_version != prepared_data_version {
            return Err(StoreError::Invalid(
                "stored content changed while preparing checkpoint; stop and reload before writing"
                    .into(),
            ));
        }
        // Only rewrite changed pages. Regions no longer represented in the full
        // world are deleted in this same commit, including migration sources.
        let old_keys = {
            let mut statement = tx.prepare("SELECT level,x,y FROM regions")?;
            statement
                .query_map([], |r| {
                    Ok(RegionKey {
                        level: r.get(0)?,
                        x: r.get(1)?,
                        y: r.get(2)?,
                    })
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?
        };
        for key in old_keys {
            if !partition.pages.contains_key(&key) {
                metrics.deleted_pages += 1;
                tx.execute(
                    "DELETE FROM regions WHERE level=?1 AND x=?2 AND y=?3",
                    params![key.level, key.x, key.y],
                )?;
            }
        }
        for key in &changed_pages {
            let encoded = &encoded_pages[key];
            tx.execute(
                "INSERT INTO regions(level,x,y,payload,digest) VALUES(?1,?2,?3,?4,?5)
                 ON CONFLICT(level,x,y) DO UPDATE SET payload=excluded.payload,digest=excluded.digest
                 WHERE regions.digest<>excluded.digest",
                params![key.level,key.x,key.y,encoded.payload.as_ref(),encoded.digest],
            )?;
        }
        tx.execute(
            "UPDATE world SET generation=?1,global_state=?2,metadata=?3,digest=?4,payload_bytes=?5 WHERE singleton=1",
            params![next as i64,global,metadata,digest,payload_bytes as i64],
        )?;
        tx.commit()?;
        metrics.transaction_ms = stage.elapsed().as_secs_f64() * 1000.;
        let stage = Instant::now();
        let active_records = partition
            .pages
            .iter()
            .filter(|(key, _)| working.active_regions.contains(key))
            .map(|(_, p)| p.record_count())
            .sum();
        let active_page_count = partition
            .pages
            .keys()
            .filter(|key| working.active_regions.contains(key))
            .count();
        self.status = RegionStoreStatus {
            generation: next,
            active_regions: active_page_count,
            dormant_regions: partition.pages.len().saturating_sub(active_page_count),
            pinned_regions: working.pinned_regions.len(),
            dependency_regions: working.dependency_regions.len(),
            active_records,
            dormant_records: partition.record_count().saturating_sub(active_records),
            global_records: partition.global_record_count(),
            checkpoint_bytes: payload_bytes as u64,
        };
        self.generation = next;
        self.fully_materialized = self.status.dormant_regions == 0;
        self.active_regions = working.active_regions;
        *active = working.state;
        *self.read_cache.borrow_mut() = Some(CachedPartition {
            generation: next,
            data_version,
            total_changes: self.connection.total_changes(),
            partition,
            metadata: metadata_value,
            bytes: payload_bytes as u64,
            encoded_pages,
        });
        metrics.install_ms = stage.elapsed().as_secs_f64() * 1000.;
        self.checkpoint_metrics = metrics;
        Ok(self.status())
    }

    /// Create a consistent SQLite backup, including committed WAL contents.
    /// Requires a new destination and uses the online backup API; it never
    /// exports an in-flight or uncheckpointed simulation mutation.
    pub fn backup_to(&self, path: impl AsRef<Path>) -> Result<()> {
        self.backup_with_options(path, RegionBackupOptions::default())
            .map(|_| ())
    }

    fn compose(&self, active: &SandboxState) -> Result<ValidatedRegionPartition> {
        if !self.restored {
            return Err(StoreError::Invalid(
                "restore the existing checkpoint before modifying this world".into(),
            ));
        }
        match self.read_partition(true)? {
            Some((partition, _, _, _)) => Ok(partition.merge(active, &self.active_regions)?),
            None => Ok(ValidatedRegionPartition::from_state(
                active,
                self.level,
                &self.catalog,
            )?),
        }
    }

    fn read_partition(
        &self,
        check_generation: bool,
    ) -> Result<Option<(ValidatedRegionPartition, Value, u64, u64)>> {
        let tx = self.connection.unchecked_transaction()?;
        validate_schema(&tx)?;
        validate_descriptor(&tx, self.descriptor_digest)?;
        let generation = read_generation(&tx)?;
        if check_generation && generation != self.generation {
            return Err(StoreError::Conflict {
                expected: self.generation,
                actual: generation,
            });
        }
        if generation == 0 {
            let empty: bool = tx.query_row(
                "SELECT global_state IS NULL AND metadata IS NULL AND digest IS NULL AND payload_bytes=0 AND (SELECT COUNT(*) FROM regions)=0 FROM world WHERE singleton=1",
                [], |r|r.get(0),
            )?;
            if !empty {
                return Err(StoreError::Invalid(
                    "uninitialized generation contains checkpoint records".into(),
                ));
            }
            return Ok(None);
        }
        let data_version: i64 = tx.query_row("PRAGMA data_version", [], |r| r.get(0))?;
        let total_changes = self.connection.total_changes();
        if let Some(cache) = self.read_cache.borrow().as_ref() {
            if cache.generation == generation
                && cache.data_version == data_version
                && cache.total_changes == total_changes
            {
                self.cache_hits.set(self.cache_hits.get().saturating_add(1));
                return Ok(Some((
                    cache.partition.clone(),
                    cache.metadata.clone(),
                    generation,
                    cache.bytes,
                )));
            }
        }
        let (global, metadata, expected, declared_bytes): (Vec<u8>, Vec<u8>, String, i64) = tx
            .query_row(
                "SELECT global_state,metadata,digest,payload_bytes FROM world WHERE singleton=1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )?;
        let mut total = global.len() + metadata.len();
        let mut digest = blake3::Hasher::new();
        digest.update(&global);
        digest.update(&metadata);
        if global.len() > MAX_DOCUMENT_BYTES
            || metadata.len() > MAX_DOCUMENT_BYTES
            || total > MAX_CHECKPOINT_BYTES
            || digest.finalize().to_hex().as_str() != expected
        {
            return Err(StoreError::Invalid(
                "global checkpoint checksum or size is invalid".into(),
            ));
        }
        let mut partition = RegionPartition {
            level: self.level,
            global: serde_json::from_slice(&global)?,
            pages: Default::default(),
        };
        let count: i64 = tx.query_row("SELECT COUNT(*) FROM regions", [], |r| r.get(0))?;
        if count < 0 || count as usize > MAX_REGION_PAGES {
            return Err(StoreError::Invalid("stored region budget exceeded".into()));
        }
        let mut encoded_pages = BTreeMap::new();
        let mut statement =
            tx.prepare("SELECT level,x,y,payload,digest FROM regions ORDER BY level,x,y")?;
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            let key = RegionKey {
                level: row.get(0)?,
                x: row.get(1)?,
                y: row.get(2)?,
            };
            key.validate()
                .map_err(|e| StoreError::Invalid(e.to_string()))?;
            let payload: Vec<u8> = row.get(3)?;
            let expected: String = row.get(4)?;
            total = total
                .checked_add(payload.len())
                .ok_or_else(|| StoreError::Invalid("checkpoint size overflow".into()))?;
            if payload.len() > MAX_DOCUMENT_BYTES
                || total > MAX_CHECKPOINT_BYTES
                || key.level != self.level
                || blake3::hash(&payload).to_hex().as_str() != expected
            {
                return Err(StoreError::Invalid(
                    "regional payload checksum, level or size is invalid".into(),
                ));
            }
            let page: RegionPage = serde_json::from_slice(&payload)?;
            if page.key != key {
                return Err(StoreError::Invalid("regional payload key differs".into()));
            }
            insert_decoded_page(&mut partition.pages, page)?;
            encoded_pages.insert(
                key,
                EncodedPage {
                    digest: expected,
                    payload: Arc::from(payload),
                },
            );
        }
        if declared_bytes < 0 || total as u64 != declared_bytes as u64 {
            return Err(StoreError::Invalid(
                "checkpoint payload inventory differs".into(),
            ));
        }
        // Complete canonical validation is mandatory for decoded bytes. The
        // immutable proof avoids repeating that work on each guarded cache hit.
        let canonical = ValidatedRegionPartition::from_partition(partition, &self.catalog)?;
        let metadata: Value = serde_json::from_slice(&metadata)?;
        self.decoded_checkpoints
            .set(self.decoded_checkpoints.get().saturating_add(1));
        *self.read_cache.borrow_mut() = Some(CachedPartition {
            generation,
            data_version,
            total_changes,
            partition: canonical.clone(),
            metadata: metadata.clone(),
            bytes: declared_bytes as u64,
            encoded_pages,
        });
        Ok(Some((
            canonical,
            metadata,
            generation,
            declared_bytes as u64,
        )))
    }
}

fn insert_decoded_page(
    pages: &mut BTreeMap<RegionKey, RegionPage>,
    page: RegionPage,
) -> Result<()> {
    match pages.entry(page.key) {
        std::collections::btree_map::Entry::Vacant(entry) => {
            entry.insert(page);
            Ok(())
        }
        std::collections::btree_map::Entry::Occupied(_) => Err(StoreError::Invalid(
            "duplicate regional identity in checkpoint".into(),
        )),
    }
}

fn read_generation(connection: &Connection) -> Result<u64> {
    let value: Option<i64> = connection
        .query_row("SELECT generation FROM world WHERE singleton=1", [], |r| {
            r.get(0)
        })
        .optional()?;
    value
        .filter(|g| *g >= 0)
        .map(|g| g as u64)
        .ok_or_else(|| StoreError::Invalid("missing or invalid checkpoint generation".into()))
}

fn hash_descriptor(identity: &[u8], catalog: &[u8], level: u8) -> blake3::Hash {
    let mut hash = blake3::Hasher::new();
    hash.update(&(identity.len() as u64).to_le_bytes());
    hash.update(identity);
    hash.update(&(catalog.len() as u64).to_le_bytes());
    hash.update(catalog);
    hash.update(&[level]);
    hash.finalize()
}

fn validate_descriptor(connection: &Connection, expected: blake3::Hash) -> Result<()> {
    let (identity, catalog, level) = read_descriptor(connection)?;
    if hash_descriptor(&identity, &catalog, level) != expected {
        return Err(StoreError::IdentityMismatch);
    }
    Ok(())
}

fn read_descriptor(connection: &Connection) -> Result<(Vec<u8>, Vec<u8>, u8)> {
    let (identity_size, catalog_size): (i64, i64) = connection.query_row(
        "SELECT length(identity),length(catalog) FROM world WHERE singleton=1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    if !(1..=65_536).contains(&identity_size)
        || !(1..=MAX_DOCUMENT_BYTES as i64).contains(&catalog_size)
    {
        return Err(StoreError::IdentityMismatch);
    }
    let (identity, catalog, level): (Vec<u8>, Vec<u8>, u8) = connection.query_row(
        "SELECT identity,catalog,region_level FROM world WHERE singleton=1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    Ok((identity, catalog, level))
}

fn require_new_database_destination(path: &Path) -> Result<()> {
    for suffix in ["", "-wal", "-shm", "-journal"] {
        let mut name = path.as_os_str().to_os_string();
        name.push(suffix);
        match fs::symlink_metadata(Path::new(&name)) {
            Ok(_) => {
                return Err(StoreError::Invalid(
                    "destination database or SQLite sidecar already exists".into(),
                ));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

// Persistent triggers/views can change the meaning of a seemingly successful
// write. Never execute a checkpoint against an externally extended schema.
// Check inside the write transaction too, so changes made after open cannot
// silently delete pages or rewrite the ledger. Check the structural column/key
// contract rather than SQL spelling. SQLite-owned indexes are allowed, while
// foreign keys and hidden/generated columns are not part of schema 1.
fn validate_schema(connection: &Connection) -> Result<()> {
    let application: i64 = connection.query_row("PRAGMA application_id", [], |row| row.get(0))?;
    let version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if application != APPLICATION_ID || version != STORE_SCHEMA_VERSION {
        return Err(StoreError::Invalid(
            "unsupported or foreign database schema".into(),
        ));
    }
    let mut statement = connection.prepare(
        "SELECT type,name FROM sqlite_master WHERE name NOT LIKE 'sqlite_%' ORDER BY name",
    )?;
    let objects = statement
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    if objects
        != [
            ("table".into(), "regions".into()),
            ("table".into(), "world".into()),
        ]
    {
        return Err(StoreError::Invalid(
            "unexpected database schema objects; explicit migration is required".into(),
        ));
    }
    for (table, expected) in [
        (
            "world",
            &[
                ("singleton", "INTEGER", false, 1),
                ("identity", "BLOB", true, 0),
                ("catalog", "BLOB", true, 0),
                ("region_level", "INTEGER", true, 0),
                ("generation", "INTEGER", true, 0),
                ("global_state", "BLOB", false, 0),
                ("metadata", "BLOB", false, 0),
                ("digest", "TEXT", false, 0),
                ("payload_bytes", "INTEGER", true, 0),
            ][..],
        ),
        (
            "regions",
            &[
                ("level", "INTEGER", true, 1),
                ("x", "INTEGER", true, 2),
                ("y", "INTEGER", true, 3),
                ("payload", "BLOB", true, 0),
                ("digest", "TEXT", true, 0),
            ][..],
        ),
    ] {
        // table_info omits generated/hidden columns. table_xinfo exposes them,
        // and the primary-key ordinal proves region identity is truly unique.
        let mut statement = connection.prepare(&format!("PRAGMA table_xinfo({table})"))?;
        let columns = statement
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, bool>(3)?,
                    r.get::<_, i64>(5)?,
                    r.get::<_, i64>(6)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        if columns.len() != expected.len()
            || columns.iter().zip(expected).any(
                |((name, kind, not_null, primary_key, hidden), expected)| {
                    name != expected.0
                        || !kind.trim().eq_ignore_ascii_case(expected.1)
                        || *not_null != expected.2
                        || *primary_key != expected.3
                        || *hidden != 0
                },
            )
        {
            return Err(StoreError::Invalid(
                "database column or primary-key contract differs from the supported schema".into(),
            ));
        }
        let mut foreign_keys = connection.prepare(&format!("PRAGMA foreign_key_list({table})"))?;
        if foreign_keys.query([])?.next()?.is_some() {
            return Err(StoreError::Invalid(
                "database foreign keys are not part of the supported schema".into(),
            ));
        }
        // Additional UNIQUE constraints can silently REPLACE another region,
        // even though table_xinfo still reports the intended primary key.
        // SQLite-owned autoindexes are invisible to the object-name filter.
        let mut indexes = connection
            .prepare("SELECT name, \"unique\", origin, partial FROM pragma_index_list(?1)")?;
        let indexes = indexes
            .query_map([table], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, bool>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, bool>(3)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        if table == "world" {
            // singleton INTEGER PRIMARY KEY is the rowid alias, not an index.
            if !indexes.is_empty() {
                return Err(StoreError::Invalid(
                    "unexpected world index; explicit migration is required".into(),
                ));
            }
        } else {
            if indexes.len() != 1 || !indexes[0].1 || indexes[0].2 != "pk" || indexes[0].3 {
                return Err(StoreError::Invalid(
                    "unexpected regional index; only its complete primary key is supported".into(),
                ));
            }
            let mut key_columns = connection
                .prepare("SELECT name FROM pragma_index_xinfo(?1) WHERE key=1 ORDER BY seqno")?;
            let key_columns = key_columns
                .query_map([&indexes[0].0], |r| r.get::<_, Option<String>>(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            if key_columns != [Some("level".into()), Some("x".into()), Some("y".into())] {
                return Err(StoreError::Invalid(
                    "regional index differs from its supported primary key".into(),
                ));
            }
        }
    }
    let rows: i64 = connection.query_row("SELECT COUNT(*) FROM world", [], |r| r.get(0))?;
    if rows != 1 {
        return Err(StoreError::Invalid(
            "database must contain exactly one world header".into(),
        ));
    }
    Ok(())
}

fn encode(value: &impl Serialize) -> Result<Vec<u8>> {
    let bytes = serde_json::to_vec(value)?;
    if bytes.len() > MAX_DOCUMENT_BYTES {
        return Err(StoreError::Invalid("document exceeds 32 MiB budget".into()));
    }
    Ok(bytes)
}

fn prepare_private_file(path: &Path) -> Result<()> {
    if path.as_os_str().is_empty() || path.file_name().is_none() {
        return Err(StoreError::Invalid(
            "database path must name a regular file".into(),
        ));
    }
    match fs::symlink_metadata(path) {
        Ok(meta) => {
            if !meta.file_type().is_file() || meta.len() > MAX_DATABASE_BYTES {
                return Err(StoreError::Invalid(
                    "database must be a regular file within the 256 MiB budget".into(),
                ));
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            create_private_file(path)?;
        }
        Err(e) => return Err(e.into()),
    }
    Ok(())
}

fn create_private_file(path: &Path) -> Result<()> {
    terra_sandbox::publication::write_private(
        path,
        &[],
        terra_sandbox::publication::PublishMode::CreateNew,
    )?;
    Ok(())
}

/// Validate an alternate output before a host starts, without creating or
/// opening its database. Existing symlink/hardlink aliases and ordinary relative
/// aliases are rejected on Unix and Windows. Existing ancestors are resolved,
/// including directory junctions, before comparing a not-yet-created output.
/// WAL, shared memory and rollback journal destinations are reserved even when
/// those files do not currently exist. Inspection errors fail closed. Callers
/// must keep these paths unchanged; this is not a filesystem-race sandbox.
/// Unresolved Windows Unicode names that could case-alias under the same
/// existing ancestor fail closed; choose a separate existing output directory.
pub fn validate_output_path(database: &Path, output: &Path) -> Result<()> {
    let database = normalized_destination(database)?;
    let output = normalized_destination(output)?;
    let output_identity = existing_file_identity(&output)?;
    for suffix in ["", "-wal", "-shm", "-journal"] {
        let mut name = database.as_os_str().to_os_string();
        name.push(suffix);
        let protected = PathBuf::from(name);
        // Conservative folding also protects new-file ASCII case aliases on
        // common case-insensitive desktop filesystems before either exists.
        let equal = protected == output
            || protected
                .to_string_lossy()
                .eq_ignore_ascii_case(&output.to_string_lossy());
        // Retain both handles while comparing identity. On Windows the file
        // identifier is paired with its volume; path spelling cannot detect a
        // second hard-link name. Conservative identity matches are rejected.
        let protected_identity = existing_file_identity(&protected)?;
        #[cfg(windows)]
        if protected_identity.is_none()
            && output_identity.is_none()
            && unresolved_windows_alias_is_ambiguous(&protected, &output)?
        {
            return Err(StoreError::Invalid("Cannot establish distinct protected/output paths for unresolved Windows Unicode names. Choose a distinct existing output directory.".into()));
        }
        let hardlink = matches!(
            (&protected_identity, &output_identity),
            (Some(a), Some(b)) if a == b
        );
        if equal || hardlink {
            return Err(StoreError::Invalid("output path aliases the regional database or its journal; choose a different destination".into()));
        }
    }
    Ok(())
}

#[cfg(any(windows, test))]
fn unresolved_name_components_may_alias(
    left: &[std::ffi::OsString],
    right: &[std::ffi::OsString],
) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut unicode = false;
    for (a, b) in left.iter().zip(right) {
        if a.as_encoded_bytes().is_ascii() && b.as_encoded_bytes().is_ascii() {
            if !a
                .as_encoded_bytes()
                .eq_ignore_ascii_case(b.as_encoded_bytes())
            {
                return false;
            }
        } else {
            // Portable Unicode casing does not implement Windows filesystem
            // comparison. Unknown equivalence must not authorize an output.
            unicode = true;
        }
    }
    unicode
}

#[cfg(windows)]
fn unresolved_windows_alias_is_ambiguous(left: &Path, right: &Path) -> Result<bool> {
    fn unresolved(path: &Path) -> Result<(PathBuf, Vec<std::ffi::OsString>)> {
        let mut current = path;
        let mut missing = Vec::new();
        loop {
            match fs::metadata(current) {
                Ok(_) => {
                    missing.reverse();
                    return Ok((current.to_owned(), missing));
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    missing.push(
                        current
                            .file_name()
                            .ok_or_else(|| {
                                StoreError::Invalid(
                                    "Windows output has no resolvable ancestor".into(),
                                )
                            })?
                            .to_owned(),
                    );
                    current = current.parent().ok_or_else(|| {
                        StoreError::Invalid("Windows output has no resolvable ancestor".into())
                    })?;
                }
                Err(error) => return Err(error.into()),
            }
        }
    }
    let (left_parent, left_missing) = unresolved(left)?;
    let (right_parent, right_missing) = unresolved(right)?;
    if !unresolved_name_components_may_alias(&left_missing, &right_missing) {
        return Ok(false);
    }
    Ok(same_file::is_same_file(left_parent, right_parent)?)
}

fn existing_file_identity(path: &Path) -> Result<Option<same_file::Handle>> {
    match same_file::Handle::from_path(path) {
        Ok(handle) => Ok(Some(handle)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn normalized_destination(path: &Path) -> Result<PathBuf> {
    if path.as_os_str().is_empty() || path.file_name().is_none() {
        return Err(StoreError::Invalid(
            "output/database path must name a file".into(),
        ));
    }
    // This also resolves Windows drive-relative (C:world.db) and rooted paths
    // against the OS's actual current-directory rules before comparison.
    let absolute = std::path::absolute(path)?;
    let mut normalized = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::CurDir => (),
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Normal(part) => {
                #[cfg(windows)]
                let part = {
                    use std::os::windows::ffi::{OsStrExt, OsStringExt};
                    let mut units: Vec<u16> = part.encode_wide().collect();
                    // Ordinary Win32 filenames trim trailing spaces/dots. Be
                    // conservative even for a verbatim path with other rules.
                    while matches!(units.last(), Some(0x20 | 0x2e)) {
                        units.pop();
                    }
                    if units.is_empty() {
                        return Err(StoreError::Invalid(
                            "output/database path has an empty Windows filename component".into(),
                        ));
                    }
                    std::ffi::OsString::from_wide(&units)
                };
                normalized.push(part);
                match fs::symlink_metadata(&normalized) {
                    // Resolve every existing component. Windows junctions and
                    // other directory aliases need not report is_symlink().
                    Ok(_) => normalized = fs::canonicalize(&normalized)?,
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
                    Err(e) => return Err(e.into()),
                }
            }
        }
    }
    Ok(normalized)
}

#[cfg(test)]
mod tests;
