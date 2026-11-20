//! Opt-in durable regional state for a local authoritative game.
//!
//! Source chunks remain immutable. A successful transaction moves dependency-
//! closed mutable records between the active simulation and dormant pages.
//! Dormant records do not receive catch-up simulation. Global ownership,
//! inventory, money and logistics ledgers stay resident.
use crate::{
    PoliticalWorld, ResidentChunks, SandboxRuntime, WorldRuntime,
    sandbox::{LocalWorldSave, WorldIdentity},
};
use bevy::{app::AppExit, prelude::*};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, path::Path, time::Instant};
use terra_core::{LocalFrame, WorldClock};
use terra_history::PoliticsDataset;
pub use terra_persistence::validate_output_path;
use terra_persistence::{
    RegionArchive, RegionCompletion, RegionKey, RegionStore, RegionStoreStatus, RegionWorkKind,
    RegionWorker,
};

/// Ordering hook for game-owned exit handlers such as telemetry exports.
#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub enum RegionPersistenceSet {
    FinalCheckpoint,
}

/// Public evidence describes committed regional state, never credentials or
/// complete private inventories. Store access remains serialized by this adapter.
#[derive(Resource, Default)]
pub struct RegionalPersistence {
    pub enabled: bool,
    pub restored: bool,
    pub status: Option<RegionStoreStatus>,
    pub last_error: Option<String>,
    pub last_checkpoint_ms: f64,
    pub last_checkpoint_metrics: terra_persistence::RegionCheckpointMetrics,
    pub seconds_since_checkpoint: f64,
    pub last_checkpoint_reason: String,
    pub(crate) source_replay_pending: bool,
    store: Option<RegionWorker>,
    residency_changed: bool,
    blocked: bool,
    pending: Option<PendingRegion>,
    activation_open: bool,
    require_existing_checkpoint: bool,
    pub last_submission_ms: f64,
}

struct PendingRegion {
    id: u64,
    revision: u64,
    reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LocalRegionMetadata {
    schema_version: u32,
    clock: WorldClock,
    player_id: u64,
    politics: Option<PoliticsDataset>,
}

fn local_store_identity(config: &crate::WorldConfig) -> serde_json::Value {
    serde_json::json!({
        "adapter": "terra-bevy-local-regions-v1",
        "world": WorldIdentity::from(config),
    })
}

/// Read-only admission for a managed world that has already started. A missing,
/// corrupt or generation-zero store must not be initialized as a fresh world.
/// The receipt names only a validated committed generation, never private data.
pub fn validate_local_checkpoint(
    config: &crate::WorldConfig,
    catalog: &terra_sandbox::Catalog,
) -> Result<u64, String> {
    let path = config
        .region_store
        .as_ref()
        .ok_or("This world does not use regional checkpoints.")?;
    let store = RegionStore::open_read_only(path).map_err(|e| e.to_string())?;
    let archive = store.archive().map_err(|e| e.to_string())?;
    if serde_json::to_value(&archive.catalog).map_err(|e| e.to_string())?
        != serde_json::to_value(catalog).map_err(|e| e.to_string())?
    {
        return Err("The saved regional catalog differs from this world.".into());
    }
    let generation = archive.source_generation;
    LocalWorldSave::from_regional_archive(archive, config)?;
    Ok(generation)
}

impl LocalWorldSave {
    /// Convert a complete local save to the same pinned regional format used by
    /// the Bevy host. A matching explicit configuration guards source identity.
    pub fn regional_archive(
        &self,
        config: &crate::WorldConfig,
        catalog: &terra_sandbox::Catalog,
    ) -> Result<RegionArchive, String> {
        config.validate()?;
        self.validate_for(config, catalog)?;
        let metadata = LocalRegionMetadata {
            schema_version: 1,
            clock: self.clock,
            player_id: self.player_id,
            politics: self.politics.clone(),
        };
        Ok(RegionArchive {
            format_version: 1,
            source_generation: 0,
            identity: local_store_identity(config),
            catalog: catalog.clone(),
            region_level: config.chunk_level,
            state: self.state.clone(),
            metadata: serde_json::to_value(metadata).map_err(|e| e.to_string())?,
        })
    }

    /// Recreate the complete version-one local JSON envelope without relabeling
    /// data from another provider, era, world or host kind.
    pub fn from_regional_archive(
        archive: RegionArchive,
        config: &crate::WorldConfig,
    ) -> Result<Self, String> {
        config.validate()?;
        archive.validate().map_err(|e| e.to_string())?;
        if archive.identity != local_store_identity(config)
            || archive.region_level != config.chunk_level
        {
            return Err("regional archive belongs to a different host or world source".into());
        }
        let metadata: LocalRegionMetadata =
            serde_json::from_value(archive.metadata).map_err(|e| e.to_string())?;
        if metadata.schema_version != 1 {
            return Err("unsupported local regional metadata version".into());
        }
        let save = Self {
            schema_version: 1,
            identity: WorldIdentity::from(config),
            clock: metadata.clock,
            player_id: metadata.player_id,
            state: archive.state,
            politics: metadata.politics,
        };
        save.validate_for(config, &archive.catalog)?;
        Ok(save)
    }
}

impl RegionalPersistence {
    /// Require a valid committed world at startup instead of admitting first
    /// initialization. Set this before the app runs, after a managed launch
    /// witness has established that this is a subsequent start.
    pub fn require_existing_checkpoint(&mut self) -> Result<(), String> {
        if self.enabled || self.store.is_some() || self.pending() {
            return Err(
                "Existing-checkpoint admission must be selected before regional startup.".into(),
            );
        }
        self.require_existing_checkpoint = true;
        Ok(())
    }

    /// Guard a game-owned export destination against this database and its
    /// SQLite sidecars, including filesystem aliases. Call immediately before
    /// writing a JSON envelope obtained from [`Self::export_world`].
    pub fn check_output_path(&self, output: &Path) -> Result<(), String> {
        if let Some(store) = &self.store {
            store
                .check_output_path(output)
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    /// Back up the latest committed SQLite world, including its live WAL.
    /// The destination must be new. Pending simulation changes require a
    /// checkpoint first; use [`Self::export_world`] for an in-memory JSON export.
    pub fn backup_to(&self, path: impl AsRef<Path>) -> Result<(), String> {
        if !self.enabled || self.status.as_ref().is_none_or(|s| s.generation == 0) {
            return Err("checkpoint this regional world before creating a database backup".into());
        }
        self.store
            .as_ref()
            .ok_or("regional store has not initialized")?
            .backup_to(path)
            .map_err(|e| e.to_string())
    }

    /// Build a complete legacy JSON envelope for a game-owned backup/export UI.
    /// This read-only operation joins dormant pages with current active changes;
    /// it does not checkpoint, change residency, or overwrite the destination.
    /// Call [`LocalWorldSave::write`] on the result to choose an explicit path.
    pub fn export_world(
        &self,
        sandbox: &SandboxRuntime,
        runtime: &WorldRuntime,
        politics: &PoliticalWorld,
    ) -> Result<LocalWorldSave, String> {
        if runtime.network_authority {
            return Err("only the authoritative host can export a complete world".into());
        }
        let state = if self.enabled {
            self.store
                .as_ref()
                .ok_or("regional store has not initialized")?
                .export(&sandbox.state)
                .map_err(|e| e.to_string())?
        } else {
            sandbox.state.clone()
        };
        let save = LocalWorldSave {
            schema_version: 1,
            identity: WorldIdentity::from(&runtime.config),
            clock: runtime.clock,
            player_id: sandbox.player_id,
            state,
            politics: politics.dataset.clone(),
        };
        save.validate_for(&runtime.config, &sandbox.catalog)?;
        Ok(save)
    }

    /// Failed durable work pauses authoritative mutation until an explicit
    /// successful checkpoint/restore. Read-only world inspection stays available.
    pub fn blocked(&self) -> bool {
        self.blocked
    }

    /// A lost storage result may have committed. This session must reopen its
    /// validated checkpoint before retrying; ordinary write errors remain
    /// recoverable through the existing checkpoint/restore actions.
    pub fn requires_reopen(&self) -> bool {
        self.store
            .as_ref()
            .is_some_and(RegionWorker::requires_reopen)
    }

    /// True while bounded background storage is running. Rendering and menus
    /// remain responsive, but authority must not mutate the submitted revision.
    pub fn pending(&self) -> bool {
        self.pending.is_some()
    }

    /// A hydrated full world is also held until it is retired again, avoiding
    /// accidental simulation of dormant records between activation and commit.
    pub fn authority_paused(&self) -> bool {
        self.blocked || self.pending() || self.activation_open
    }

    pub fn status_line(&self) -> String {
        if !self.enabled {
            return "Manual JSON save · F5 save · F9 load".into();
        }
        if self.requires_reopen() {
            return "Storage worker stopped · Close and reopen this world before retrying".into();
        }
        if let Some(pending) = &self.pending {
            return format!(
                "{} · storage working · world changes waiting",
                pending.reason
            );
        }
        if self.blocked {
            return "Persistence blocked · F5 retry checkpoint · F9 restore last checkpoint".into();
        }
        match &self.status {
            Some(s) if s.generation > 0 => format!(
                "Checkpoint {} · {} active / {} dormant regions",
                s.generation, s.active_regions, s.dormant_regions
            ),
            _ => "Regional store open · first checkpoint pending".into(),
        }
    }

    pub(crate) fn residency_changed(&mut self) {
        self.residency_changed |= self.enabled;
    }

    /// Must run before immutable source registration. Otherwise a depleted
    /// resource that is currently dormant could be seeded as a new resource.
    pub(crate) fn hydrate(&mut self, sandbox: &mut SandboxRuntime) -> Result<bool, String> {
        if !self.enabled {
            return Ok(true);
        }
        if self.blocked {
            return Err(
                "regional persistence is blocked; checkpoint or restore before continuing".into(),
            );
        }
        if self.pending() {
            return Ok(false);
        }
        // Completeness describes the live view, not whether the committed
        // checkpoint is still valid. Only a batch admitted by a matching
        // hydrate/restore completion may skip another asynchronous validation.
        // Keep that admission open through all of its source/command work;
        // otherwise each resumed command would immediately queue hydration again.
        if self.activation_open {
            sandbox
                .state
                .require_complete()
                .map_err(|e| e.to_string())?;
            return Ok(true);
        }
        let start = Instant::now();
        let id = self
            .store
            .as_mut()
            .ok_or("regional store has not initialized")?
            .submit_hydrate(sandbox.state.clone())
            .map_err(|e| e.to_string())?;
        self.pending = Some(PendingRegion {
            id,
            revision: sandbox.revision,
            reason: "Loading regional state".into(),
        });
        self.activation_open = true;
        self.last_submission_ms = start.elapsed().as_secs_f64() * 1000.;
        Ok(false)
    }

    pub(crate) fn fail(&mut self, error: String, runtime: &mut WorldRuntime) {
        self.last_error = Some(error.clone());
        self.blocked = true;
        self.source_replay_pending = true;
        runtime.clock.paused = true;
        runtime.last_error = Some(format!("Regional persistence: {error}"));
    }

    pub(crate) fn checkpoint(
        &mut self,
        sandbox: &mut SandboxRuntime,
        runtime: &mut WorldRuntime,
        politics: &PoliticalWorld,
        resident: &ResidentChunks,
        reason: &str,
    ) -> Result<String, String> {
        let start = Instant::now();
        if self.pending() {
            return Ok(
                "Regional storage is already working; the current request will finish first."
                    .into(),
            );
        }
        let metadata = LocalRegionMetadata {
            schema_version: 1,
            clock: runtime.clock,
            player_id: sandbox.player_id,
            politics: politics.dataset.clone(),
        };
        validate_metadata(&metadata, runtime)?;
        if !sandbox.state.players.contains_key(&sandbox.player_id) {
            return Err("local player is absent from the active global ledger".into());
        }
        let mut requested: BTreeSet<RegionKey> = if resident.chunks.is_empty() {
            RegionKey::at(runtime.focus, runtime.config.chunk_level)
                .map_err(|e| e.to_string())?
                .neighbors(runtime.config.chunk_radius)
                .into_iter()
                .collect()
        } else {
            resident.chunks.keys().copied().collect()
        };
        // Source features may start exactly on the east/south boundary, or be
        // deliberately unclipped by a game-owned provider. Their mutable page
        // follows that anchor, which can differ from the source chunk's key.
        // Keep that page active while the source is visible so tombstones remain
        // available to rendering and do not briefly resurrect removed trees.
        requested.extend(resident.chunks.values().flat_map(|chunk| {
            chunk.features.iter().filter_map(|feature| {
                RegionKey::at(*feature.points.first()?, runtime.config.chunk_level).ok()
            })
        }));
        let metadata = serde_json::to_value(metadata).map_err(|e| e.to_string())?;
        let id = self
            .store
            .as_mut()
            .ok_or("regional store has not initialized")?
            .submit_checkpoint(sandbox.state.clone(), metadata, requested)
            .map_err(|e| e.to_string())?;
        self.pending = Some(PendingRegion {
            id,
            revision: sandbox.revision,
            reason: reason.into(),
        });
        self.last_submission_ms = start.elapsed().as_secs_f64() * 1000.;
        self.residency_changed = false;
        Ok("Checkpoint queued. World changes wait while storage finishes; the view remains available.".into())
    }

    fn install_completion(
        &mut self,
        completion: RegionCompletion,
        sandbox: &mut SandboxRuntime,
        runtime: &mut WorldRuntime,
    ) -> Result<(), String> {
        let pending = self
            .pending
            .take()
            .ok_or("unexpected regional completion")?;
        if completion.id != pending.id || sandbox.revision != pending.revision {
            return Err(
                "regional completion belongs to another world revision; restore before continuing"
                    .into(),
            );
        }
        if let Some(error) = completion.error {
            return Err(error);
        }
        sandbox.state = completion
            .state
            .ok_or("regional worker returned no state")?;
        sandbox.revision = sandbox.revision.wrapping_add(1);
        match completion.kind {
            RegionWorkKind::Hydrate => {
                // The ledger is complete for the next source/command batch.
                // Keep fixed simulation suspended until its checkpoint retires.
                self.activation_open = true;
                self.residency_changed = true;
            }
            RegionWorkKind::Checkpoint => {
                self.status = Some(completion.status);
                self.last_checkpoint_ms = completion.elapsed_ms;
                self.last_checkpoint_metrics = completion.checkpoint_metrics;
                self.last_checkpoint_reason = pending.reason;
                self.seconds_since_checkpoint = 0.;
                self.last_error = None;
                self.blocked = false;
                self.activation_open = false;
                clear_region_error(runtime);
                sandbox.dirty = false;
                let status = self.status.as_ref().unwrap();
                sandbox.last_message = format!(
                    "Checkpoint {} saved · {} active / {} dormant regions. F9 restores this checkpoint.{}",
                    status.generation,
                    status.active_regions,
                    status.dormant_regions,
                    if runtime.clock.paused {
                        " World remains paused; resume when ready."
                    } else {
                        ""
                    }
                );
            }
        }
        Ok(())
    }

    /// Blocking only at explicit startup/restore/shutdown boundaries.
    fn finish_pending(
        &mut self,
        sandbox: &mut SandboxRuntime,
        runtime: &mut WorldRuntime,
    ) -> Result<(), String> {
        let result = self
            .store
            .as_mut()
            .ok_or("regional store has not initialized")?
            .finish()
            .map_err(|e| e.to_string())?;
        if let Some(completion) = result {
            self.install_completion(completion, sandbox, runtime)?;
        }
        Ok(())
    }

    fn checkpoint_sync(
        &mut self,
        sandbox: &mut SandboxRuntime,
        runtime: &mut WorldRuntime,
        politics: &PoliticalWorld,
        resident: &ResidentChunks,
        reason: &str,
    ) -> Result<String, String> {
        self.finish_pending(sandbox, runtime)?;
        self.checkpoint(sandbox, runtime, politics, resident, reason)?;
        self.finish_pending(sandbox, runtime)?;
        Ok(sandbox.last_message.clone())
    }

    pub(crate) fn restore(
        &mut self,
        sandbox: &mut SandboxRuntime,
        runtime: &mut WorldRuntime,
        politics: &mut PoliticalWorld,
    ) -> Result<String, String> {
        self.restore_reviewed(sandbox, runtime, politics, None)
    }

    pub(crate) fn restore_reviewed(
        &mut self,
        sandbox: &mut SandboxRuntime,
        runtime: &mut WorldRuntime,
        politics: &mut PoliticalWorld,
        expected_generation: Option<u64>,
    ) -> Result<String, String> {
        self.finish_pending(sandbox, runtime)?;
        let store = self
            .store
            .as_mut()
            .ok_or("regional store has not initialized")?;
        let saved = store
            .with_store(|store| {
                store.restore_validated(|candidate| {
                    if expected_generation.is_some_and(|expected| expected != candidate.generation)
                    {
                        return Err("Checkpoint changed. Review restore again.".into());
                    }
                    let metadata: LocalRegionMetadata =
                        serde_json::from_value(candidate.metadata.clone())
                            .map_err(|e| e.to_string())?;
                    validate_metadata(&metadata, runtime)?;
                    LocalWorldSave {
                        schema_version: 1,
                        identity: WorldIdentity::from(&runtime.config),
                        clock: metadata.clock,
                        player_id: metadata.player_id,
                        state: candidate.state.clone(),
                        politics: metadata.politics,
                    }
                    .validate_for(&runtime.config, &sandbox.catalog)
                })
            })
            .map_err(|e| e.to_string())?
            .ok_or("this regional store has no checkpoint yet; press F5 to create one")?;
        let metadata: LocalRegionMetadata =
            serde_json::from_value(saved.metadata).map_err(|e| e.to_string())?;
        validate_metadata(&metadata, runtime)?;
        let save = LocalWorldSave {
            schema_version: 1,
            identity: WorldIdentity::from(&runtime.config),
            clock: metadata.clock,
            player_id: metadata.player_id,
            state: saved.state,
            politics: metadata.politics,
        };
        save.validate_for(&runtime.config, &sandbox.catalog)?;
        apply_restored_save(save, sandbox, runtime, politics);
        self.status = Some(store.status());
        self.restored = true;
        self.blocked = false;
        self.activation_open = true;
        self.last_error = None;
        clear_region_error(runtime);
        self.seconds_since_checkpoint = 0.;
        // Restore returns the complete validated world. Repartition it against
        // the currently resident source surface before the next rendered frame.
        self.residency_changed = true;
        self.source_replay_pending = true;
        Ok(format!(
            "Restored regional checkpoint {}. Clock, ownership and economy restored. Tab focuses the restored player.",
            saved.generation
        ))
    }
}

fn clear_region_error(runtime: &mut WorldRuntime) {
    if runtime
        .last_error
        .as_deref()
        .is_some_and(|e| e.starts_with("Regional persistence:"))
    {
        runtime.last_error = None;
    }
}

fn validate_metadata(metadata: &LocalRegionMetadata, runtime: &WorldRuntime) -> Result<(), String> {
    if metadata.schema_version != 1 {
        return Err("unsupported local regional metadata version".into());
    }
    if !metadata.clock.unix_seconds.is_finite()
        || !(-62135596800.0..=253402300799.0).contains(&metadata.clock.unix_seconds)
        || !metadata.clock.time_scale.is_finite()
        || !(0.0..=3600.0).contains(&metadata.clock.time_scale)
    {
        return Err("invalid regional simulation clock".into());
    }
    if metadata.politics.as_ref().map(|p| &p.id)
        != runtime.config.political_dataset_revision.as_ref()
    {
        return Err("regional political dataset identity differs".into());
    }
    if let Some(dataset) = &metadata.politics {
        dataset.validate().map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn apply_restored_save(
    save: LocalWorldSave,
    sandbox: &mut SandboxRuntime,
    runtime: &mut WorldRuntime,
    politics: &mut PoliticalWorld,
) {
    sandbox.state = save.state;
    sandbox.player_id = save.player_id;
    sandbox.revision = sandbox.revision.wrapping_add(1);
    sandbox.restore_revision = sandbox.restore_revision.wrapping_add(1);
    sandbox.dirty = false;
    runtime.clock = save.clock;
    runtime.focus = sandbox.state.players[&sandbox.player_id].position;
    runtime.frame = LocalFrame::new(runtime.focus);
    runtime.origin_revision = runtime.origin_revision.wrapping_add(1);
    politics.dataset = save.politics;
    politics.draft.clear();
    politics.drawing = false;
    politics.revision = politics.revision.wrapping_add(1);
}

pub(crate) fn initialize_regions(
    mut persistence: ResMut<RegionalPersistence>,
    mut runtime: ResMut<WorldRuntime>,
    mut sandbox: ResMut<SandboxRuntime>,
    mut politics: ResMut<PoliticalWorld>,
    resident: Res<ResidentChunks>,
    mut exit: MessageWriter<AppExit>,
) {
    let Some(path) = runtime.config.region_store.clone() else {
        return;
    };
    persistence.enabled = true;
    let result = (|| -> Result<(), String> {
        if runtime.network_authority {
            return Err("regional persistence belongs to the host; remove region_store from client configuration".into());
        }
        if persistence.require_existing_checkpoint {
            validate_local_checkpoint(&runtime.config, &sandbox.catalog).map_err(|error| {
                format!("This managed world was already started, but its committed checkpoint cannot be restored: {error}. Restore a verified backup or create a separate new world; no fresh world was initialized.")
            })?;
        }
        let identity = local_store_identity(&runtime.config);
        let store = RegionStore::open(
            &path,
            identity,
            &sandbox.catalog,
            runtime.config.chunk_level,
        )
        .map_err(|e| e.to_string())?;
        let initialized = store.is_initialized();
        persistence.status = Some(store.status());
        persistence.store = Some(RegionWorker::new(store).map_err(|e| e.to_string())?);
        if initialized {
            let message = persistence.restore(&mut sandbox, &mut runtime, &mut politics)?;
            // Restore materializes every record for cross-reference validation.
            // Repartition before the first FixedUpdate, so dormant entities do
            // not receive even one accidental catch-up step during startup.
            persistence.checkpoint_sync(
                &mut sandbox,
                &mut runtime,
                &politics,
                &resident,
                "Resume activation",
            )?;
            sandbox.last_message = message;
        } else {
            sandbox.last_message = "Regional world opened. Changes checkpoint automatically; F5 checkpoints now, F9 restores.".into();
        }
        Ok(())
    })();
    if let Err(error) = result {
        persistence.fail(error.clone(), &mut runtime);
        sandbox.last_message = format!("Cannot open regional world: {error}");
        eprintln!("Cannot open regional world: {error}");
        // A corrupt/mismatched world must never silently become a new world.
        persistence.store = None;
        exit.write(AppExit::error());
    }
}

/// Poll before FixedUpdate so completed active sets can resume without a disk
/// wait. A complete hydration remains held through its command/source batch.
pub(crate) fn poll_regions(
    mut persistence: ResMut<RegionalPersistence>,
    mut runtime: ResMut<WorldRuntime>,
    mut sandbox: ResMut<SandboxRuntime>,
) {
    if !persistence.pending() {
        return;
    }
    let result = persistence
        .store
        .as_mut()
        .ok_or_else(|| "regional store is absent".to_string())
        .and_then(|store| store.try_complete().map_err(|e| e.to_string()));
    let result = match result {
        Ok(Some(completion)) => {
            persistence.install_completion(completion, &mut sandbox, &mut runtime)
        }
        Ok(None) => return,
        Err(error) => Err(error),
    };
    if let Err(error) = result {
        persistence.pending = None;
        persistence.fail(error.clone(), &mut runtime);
        sandbox.last_message = format!("Regional operation failed. World paused: {error}");
    }
}

pub(crate) fn checkpoint_regions(
    time: Res<Time<bevy::time::Real>>,
    mut persistence: ResMut<RegionalPersistence>,
    mut runtime: ResMut<WorldRuntime>,
    mut sandbox: ResMut<SandboxRuntime>,
    politics: Res<PoliticalWorld>,
    resident: Res<ResidentChunks>,
    mut timings: ResMut<crate::WorldDiagnostics>,
) {
    if !persistence.enabled || runtime.network_authority {
        return;
    }
    persistence.seconds_since_checkpoint += time.delta_secs_f64();
    if persistence.blocked || persistence.pending() {
        return;
    }
    let reason = if persistence.residency_changed {
        "Source residency changed"
    } else if persistence.seconds_since_checkpoint >= runtime.config.region_checkpoint_seconds {
        "Automatic checkpoint"
    } else {
        return;
    };
    let start = Instant::now();
    if let Err(error) =
        persistence.checkpoint(&mut sandbox, &mut runtime, &politics, &resident, reason)
    {
        persistence.fail(error.clone(), &mut runtime);
        sandbox.last_message = format!("Checkpoint failed. World paused: {error}");
    }
    timings.record(
        crate::WorldStage::Persistence,
        start.elapsed().as_secs_f64() * 1000.,
    );
}

pub(crate) fn checkpoint_on_exit(
    mut exits: ParamSet<(MessageReader<AppExit>, MessageWriter<AppExit>)>,
    mut persistence: ResMut<RegionalPersistence>,
    mut runtime: ResMut<WorldRuntime>,
    mut sandbox: ResMut<SandboxRuntime>,
    politics: Res<PoliticalWorld>,
    resident: Res<ResidentChunks>,
    mut done: Local<bool>,
) {
    if exits.p0().read().next().is_none() || *done {
        return;
    }
    *done = true;
    if !persistence.enabled || runtime.network_authority || persistence.store.is_none() {
        return;
    }
    if let Err(error) = persistence.checkpoint_sync(
        &mut sandbox,
        &mut runtime,
        &politics,
        &resident,
        "Graceful exit",
    ) {
        persistence.fail(error.clone(), &mut runtime);
        eprintln!("Final regional checkpoint failed: {error}");
        exits.p1().write(AppExit::error());
    }
}

#[cfg(test)]
#[path = "regions_tests.rs"]
mod tests;
