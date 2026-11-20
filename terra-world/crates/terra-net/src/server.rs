use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    io::Read,
    net::{SocketAddr, TcpListener},
    path::Path,
};

use serde::{Deserialize, Serialize};
use terra_core::{
    ActorKind, ActorState, GeoPosition, LocalFrame, TileKey, WorldChunk, WorldClock,
    synthetic_weather,
};
use terra_persistence::{
    RegionArchive, RegionCompletion, RegionStore, RegionStoreStatus, RegionWorkKind, RegionWorker,
};
use terra_sandbox::{
    Catalog, EcologyScheduleConfig, EcologyScheduleStats, EcologyScheduler, GeoSpatialIndex,
    Inventory, SandboxInterestIndex, SandboxState,
};

use crate::{
    BaseWorldManifest, ClientMessage, NetError, PROTOCOL_VERSION, Result, ServerMessage,
    protocol::{surface_distance_m, validate_position},
    replication::{PeerReplication, ReplicationStats},
    transport::Connection,
};

#[path = "operator.rs"]
mod operator;
use operator::OperatorState;
pub use operator::{OperatorAction, OperatorAuditEntry, OperatorReceipt, PrincipalStatus};

const MAX_MESSAGES_PER_UPDATE: usize = 32;
const MESSAGE_TOKENS_PER_SECOND: f64 = 60.0;
const MESSAGE_BURST: f64 = 120.0;
const MAX_STEPS_PER_UPDATE: usize = 8;
const INPUT_TIMEOUT_SECONDS: f64 = 1.0;
const HANDSHAKE_TIMEOUT_SECONDS: f64 = 10.0;
const IDLE_TIMEOUT_SECONDS: f64 = 30.0;
const MAX_SAVE_BYTES: u64 = 32 * 1024 * 1024;

/// Surface queries for game-specific authoritative collision/terrain services.
/// Implementations must not perform blocking HTTP: use resident data or a pack.
pub trait WorldEnvironment: Send + Sync {
    fn ground_height_m(&self, position: GeoPosition) -> Option<f64>;
    /// Surface beneath one wheel. Legacy custom providers retain neutral paved
    /// handling; source providers override material and semantic exclusions.
    fn vehicle_ground_sample(
        &self,
        position: GeoPosition,
    ) -> Option<terra_core::VehicleGroundSample> {
        if !self.can_enter(position) {
            return None;
        }
        self.resolve_movement(ActorKind::Vehicle, position, position)
            .filter(|p| validate_position(*p) && self.can_enter(*p))
            .map(|p| terra_core::VehicleGroundSample {
                height_m: p.altitude_m,
                surface: terra_core::VehicleSurface::Paved,
            })
    }
    /// Hosting packs can prevent actors from entering uncovered geography.
    fn can_enter(&self, _position: GeoPosition) -> bool {
        true
    }
    fn water_height_m(&self, _position: GeoPosition) -> Option<f64> {
        Some(0.0)
    }
    /// Called when the host's source streamer integrates or evicts geometry.
    fn register_chunk(&mut self, _chunk: &WorldChunk) {}
    fn evict_chunk(&mut self, _key: TileKey) {}
    /// Override for a game's memory-only collision and navigation queries.
    /// The default supports terrain/coverage only; the reference CLI uses
    /// ResidentSurface for water, slopes, bridges and source footprints.
    fn resolve_movement(
        &self,
        kind: ActorKind,
        _from: GeoPosition,
        mut candidate: GeoPosition,
    ) -> Option<GeoPosition> {
        if !validate_position(candidate) || !self.can_enter(candidate) {
            return None;
        }
        let height = if kind == ActorKind::Ship {
            self.water_height_m(candidate)
        } else {
            self.ground_height_m(candidate)
        }?;
        if !height.is_finite() || !(-12_000.0..=99_997.0).contains(&height) {
            return None;
        }
        candidate.altitude_m = if is_airborne(&kind) {
            candidate.altitude_m.max(height + 3.)
        } else {
            height
        };
        Some(candidate)
    }
    fn allows_placement(&self, position: GeoPosition, _radius_m: f64, _footprint: bool) -> bool {
        validate_position(position)
            && self.can_enter(position)
            && self
                .ground_height_m(position)
                .is_some_and(|h| h.is_finite() && (position.altitude_m - h).abs() <= 3.)
    }
    /// Small sample radius used by a model-sized vehicle envelope. Providers
    /// with source colliders should override this memory-only point query.
    fn vehicle_support(&self, position: GeoPosition) -> Option<GeoPosition> {
        self.resolve_movement(ActorKind::Vehicle, position, position)
    }
    /// Model-height-aware admission. Existing custom providers retain their
    /// support implementation; source colliders override this to check roofs.
    fn vehicle_support_with_clearance(
        &self,
        position: GeoPosition,
        _body_height_m: f64,
    ) -> Option<GeoPosition> {
        self.vehicle_support(position)
    }
}

/// Explicit simple reference environment; no road navigation or collision shapes.
pub struct FlatEnvironment {
    pub ground_height_m: f64,
    pub water_height_m: f64,
}

impl WorldEnvironment for FlatEnvironment {
    fn ground_height_m(&self, _position: GeoPosition) -> Option<f64> {
        Some(self.ground_height_m)
    }
    fn water_height_m(&self, _position: GeoPosition) -> Option<f64> {
        Some(self.water_height_m)
    }
}

#[derive(Clone, Debug)]
pub struct ServerConfig {
    pub bind_addr: SocketAddr,
    pub tick_hz: u32,
    pub snapshot_hz: u32,
    pub max_clients: usize,
    /// Startup host policy. False requires an existing valid saved capability.
    /// It is deliberately independent of source/save identity and never grants
    /// a new guest identity when a supplied capability is invalid.
    pub allow_new_players: bool,
    pub max_actors: usize,
    pub max_snapshot_actors: usize,
    pub max_interest_radius_m: f64,
    pub spawn: GeoPosition,
    pub manifest: BaseWorldManifest,
    /// Calendar scale is independent of the fixed-step actor movement rate.
    pub calendar_time_scale: f64,
    pub initial_unix_seconds: Option<f64>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind_addr: SocketAddr::from(([127, 0, 0, 1], 7777)),
            tick_hz: 20,
            snapshot_hz: 10,
            max_clients: 16,
            allow_new_players: true,
            max_actors: 10_000,
            max_snapshot_actors: 128,
            max_interest_radius_m: 20_000.0,
            spawn: GeoPosition::new(48.137, 11.575, 0.0).expect("constant valid position"),
            manifest: BaseWorldManifest::default(),
            calendar_time_scale: 60.0,
            initial_unix_seconds: None,
        }
    }
}

impl ServerConfig {
    fn validate(&self) -> Result<()> {
        let valid = (1..=120).contains(&self.tick_hz)
            && (1..=self.tick_hz).contains(&self.snapshot_hz)
            && (1..=terra_sandbox::MAX_PLAYERS).contains(&self.max_clients)
            && self.max_actors >= self.max_clients
            && self.max_actors <= 100_000
            && (1..=512).contains(&self.max_snapshot_actors)
            && self.max_interest_radius_m.is_finite()
            && (10.0..=100_000.0).contains(&self.max_interest_radius_m)
            && validate_position(self.spawn)
            && self.calendar_time_scale.is_finite()
            && (0.0..=86_400.0).contains(&self.calendar_time_scale)
            && self
                .initial_unix_seconds
                .is_none_or(|value| value.is_finite() && value.abs() <= 1e12)
            && !self.manifest.world_id.is_empty()
            && self.manifest.world_id.len() <= 256
            && self.manifest.base_data_revision.len() <= 512
            && self.manifest.terrain_provider.len() <= 256;
        let valid = valid
            && self.manifest.chunk_level <= 20
            && (2..=257).contains(&self.manifest.terrain_resolution)
            && self
                .manifest
                .data_epoch_unix
                .is_none_or(|epoch| (-1_000_000_000_000..=1_000_000_000_000).contains(&epoch))
            && self
                .manifest
                .political_dataset_revision
                .as_ref()
                .is_none_or(|revision| !revision.is_empty() && revision.len() <= 256);
        if valid {
            Ok(())
        } else {
            Err(NetError::Configuration(
                "invalid rate, budget, position, calendar scale or manifest".into(),
            ))
        }
    }
}

struct Peer {
    connection: Connection,
    inbox: VecDeque<ClientMessage>,
    read_failed: bool,
    actor_id: Option<u64>,
    interest: GeoPosition,
    radius_m: f64,
    last_sequence: Option<u64>,
    created_at: f64,
    last_activity: f64,
    last_movement: f64,
    drive_input: Option<(u64, terra_core::DriveInput)>,
    tokens: f64,
    sandbox_tokens: f64,
    strikes: u8,
    replication: PeerReplication,
    /// One coalesced request to rebuild the latest sandbox view. No retained
    /// serialized snapshots accumulate behind a slow reader.
    sandbox_snapshot_due: bool,
}

#[derive(Default, Debug, Clone)]
pub struct ServerReport {
    pub tick: u64,
    pub clients: usize,
    pub actors: usize,
    pub messages: usize,
    pub disconnected: usize,
    pub fixed_steps: usize,
    pub dropped_seconds: f64,
    pub snapshots_sent: usize,
    pub domain_errors: usize,
    pub replication: ReplicationStats,
    /// Exact-distance candidates tested by the shared principal-actor AOI index.
    pub interest_candidates_tested: usize,
    /// A failed regional checkpoint stops simulation advancement for
    /// this update. The host must resolve/retry or stop; committed data is intact.
    pub persistence_error: Option<String>,
    /// A storage request is outstanding; the call did not wait for SQLite.
    pub persistence_pending: bool,
    pub storage_work_ms: f64,
    /// Reliable input batches deferred until a maximum response frame fits.
    pub deferred_command_batches: usize,
    /// Application/TLS queued bytes for the most backlogged current peer.
    pub max_peer_pending_bytes: usize,
    /// Largest recorded application/TLS queue for any current peer.
    pub peak_peer_pending_bytes: usize,
}

/// Call `update` from an application loop. It performs bounded nonblocking I/O,
/// then advances actors at a fixed tick and sends interest-filtered snapshots.
pub struct Server {
    config: ServerConfig,
    listener: TcpListener,
    peers: BTreeMap<u64, Peer>,
    actors: BTreeMap<u64, ActorState>,
    environment: Box<dyn WorldEnvironment>,
    clock: WorldClock,
    tick: u64,
    next_client_id: u64,
    next_actor_id: u64,
    real_seconds: f64,
    accumulator: f64,
    snapshot_accumulator: f64,
    sandbox: SandboxState,
    catalog: Catalog,
    starting_inventory: Inventory,
    sessions: BTreeMap<String, u64>,
    operator: OperatorState,
    sandbox_sequences: BTreeMap<u64, u64>,
    ecology_scheduler: EcologyScheduler,
    region_store: Option<RegionWorker>,
    /// Each source tile retains its own key and feature-anchor regions. Shared
    /// anchors remain requested until every contributing source tile evicts.
    source_regions: BTreeMap<TileKey, BTreeSet<TileKey>>,
    region_checkpoint_pending: bool,
    region_hydrated_for_input: bool,
    region_last_error: Option<String>,
    region_last_work_ms: f64,
    region_hydration_failed: bool,
    region_source_activation: bool,
    closing_peers: Vec<Peer>,
    #[cfg(feature = "tls")]
    tls: Option<crate::ServerTlsConfig>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SavedServer {
    schema_version: u32,
    manifest: BaseWorldManifest,
    sandbox: SandboxState,
    catalog: Catalog,
    #[serde(default = "default_starting_inventory")]
    starting_inventory: Inventory,
    sessions: BTreeMap<String, u64>,
    #[serde(default)]
    operator: OperatorState,
    sandbox_sequences: BTreeMap<u64, u64>,
    nonplayer_actors: BTreeMap<u64, ActorState>,
    clock: WorldClock,
    tick: u64,
}

/// Private host control state committed in the same transaction as all domain
/// partitions. Resume capabilities never appear in public store diagnostics.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SavedServerControl {
    schema_version: u32,
    manifest: BaseWorldManifest,
    catalog: Catalog,
    #[serde(default = "default_starting_inventory")]
    starting_inventory: Inventory,
    sessions: BTreeMap<String, u64>,
    #[serde(default)]
    operator: OperatorState,
    sandbox_sequences: BTreeMap<u64, u64>,
    nonplayer_actors: BTreeMap<u64, ActorState>,
    clock: WorldClock,
    tick: u64,
}

impl SavedServerControl {
    fn with_sandbox(self, sandbox: SandboxState) -> SavedServer {
        SavedServer {
            schema_version: self.schema_version,
            manifest: self.manifest,
            sandbox,
            catalog: self.catalog,
            starting_inventory: self.starting_inventory,
            sessions: self.sessions,
            operator: self.operator,
            sandbox_sequences: self.sandbox_sequences,
            nonplayer_actors: self.nonplayer_actors,
            clock: self.clock,
            tick: self.tick,
        }
    }
}

impl Server {
    pub fn bind(config: ServerConfig) -> Result<Self> {
        config.validate()?;
        let listener = TcpListener::bind(config.bind_addr)?;
        listener.set_nonblocking(true)?;
        let environment = Box::new(FlatEnvironment {
            ground_height_m: config.spawn.altitude_m,
            water_height_m: 0.0,
        });
        let mut clock = WorldClock {
            time_scale: config.calendar_time_scale,
            ..WorldClock::default()
        };
        if let Some(time) = config.initial_unix_seconds {
            clock.unix_seconds = time;
        }
        Ok(Self {
            config,
            listener,
            peers: BTreeMap::new(),
            actors: BTreeMap::new(),
            environment,
            clock,
            tick: 0,
            next_client_id: 1,
            next_actor_id: 1,
            real_seconds: 0.0,
            accumulator: 0.0,
            snapshot_accumulator: 0.0,
            sandbox: SandboxState::default(),
            catalog: Catalog::builtin(),
            starting_inventory: default_starting_inventory(),
            sessions: BTreeMap::new(),
            operator: OperatorState::default(),
            sandbox_sequences: BTreeMap::new(),
            ecology_scheduler: EcologyScheduler::default(),
            region_store: None,
            source_regions: BTreeMap::new(),
            region_checkpoint_pending: false,
            region_hydrated_for_input: false,
            region_last_error: None,
            region_last_work_ms: 0.,
            region_hydration_failed: false,
            region_source_activation: false,
            closing_peers: Vec::new(),
            #[cfg(feature = "tls")]
            tls: None,
        })
    }

    pub fn ecology_schedule_stats(&self) -> &EcologyScheduleStats {
        self.ecology_scheduler.stats()
    }

    /// Host policy only; changing it resets transient decision scheduling.
    pub fn set_ecology_schedule(&mut self, config: EcologyScheduleConfig) -> Result<()> {
        self.ecology_scheduler = EcologyScheduler::new(config)?;
        Ok(())
    }

    pub fn local_addr(&self) -> std::io::Result<SocketAddr> {
        self.listener.local_addr()
    }
    /// Require verified TLS records on every accepted socket. Certificate/key
    /// parsing happens before binding; handshakes progress within the regular
    /// bounded nonblocking transport loop and admission timeout.
    #[cfg(feature = "tls")]
    pub fn bind_tls(config: ServerConfig, tls: crate::ServerTlsConfig) -> Result<Self> {
        let mut server = Self::bind(config)?;
        server.tls = Some(tls);
        Ok(server)
    }
    pub fn tick(&self) -> u64 {
        self.tick
    }
    pub fn clock(&self) -> &WorldClock {
        &self.clock
    }
    pub fn actors(&self) -> &BTreeMap<u64, ActorState> {
        &self.actors
    }
    pub fn config(&self) -> &ServerConfig {
        &self.config
    }
    /// Current working set. With a regional store attached this may contain
    /// dormant reservations; do not treat it as a complete portable world.
    /// `save_world` exports the validated active and dormant state together.
    pub fn sandbox(&self) -> &SandboxState {
        &self.sandbox
    }

    /// Attach a local transactional regional store before accepting players.
    /// Existing data is validated and restored as one complete host state. A
    /// new store starts empty until the first explicit checkpoint succeeds.
    /// Its geographic level is pinned to the immutable source manifest.
    pub fn open_region_store(&mut self, path: impl AsRef<Path>) -> Result<RegionStoreStatus> {
        if self.region_store.is_some() || !self.peers.is_empty() {
            return Err(NetError::Configuration(
                "open a regional store once, before connecting clients".into(),
            ));
        }
        let identity = serde_json::json!({
            "kind": "terra-server",
            "schema_version": 1,
            "manifest": self.config.manifest,
        });
        let mut store = RegionStore::open(
            path,
            identity,
            &self.catalog,
            self.config.manifest.chunk_level,
        )
        .map_err(persistence_error)?;
        let restored = store
            .restore_validated(|candidate| {
                let control: SavedServerControl =
                    serde_json::from_value(candidate.metadata.clone())
                        .map_err(|error| error.to_string())?;
                if serde_json::to_value(&control.catalog).map_err(|error| error.to_string())?
                    != serde_json::to_value(&self.catalog).map_err(|error| error.to_string())?
                {
                    return Err("stored host catalog differs from the pinned store catalog".into());
                }
                self.validate_saved_world(&control.with_sandbox(candidate.state.clone()))
                    .map_err(|error| error.to_string())
            })
            .map_err(persistence_error)?;
        if let Some(restored) = restored {
            let control: SavedServerControl = serde_json::from_value(restored.metadata)?;
            let saved = control.with_sandbox(restored.state);
            self.install_saved_world(saved);
        }
        let status = store.status();
        self.region_store = Some(RegionWorker::new(store).map_err(persistence_error)?);
        // Restore materializes the complete logical world for validation. Even
        // API hosts that immediately step must repartition it before simulation.
        self.region_checkpoint_pending = true;
        Ok(status)
    }

    /// Offline migration of an already validated complete server world. Builds
    /// a temporary checkpoint and publishes a new database exclusively; never
    /// replaces a source, changes identities or attaches a second authority.
    pub fn import_region_store_new(&self, path: impl AsRef<Path>) -> Result<RegionStoreStatus> {
        if self.region_store.is_some() || !self.peers.is_empty() {
            return Err(NetError::Configuration(
                "import a complete world before attaching a store or accepting players".into(),
            ));
        }
        self.validate_saved_world(&self.saved_control().with_sandbox(self.sandbox.clone()))?;
        RegionArchive {
            format_version: 1, source_generation: 0,
            identity: serde_json::json!({ "kind": "terra-server", "schema_version": 1, "manifest": self.config.manifest }),
            catalog: self.catalog.clone(), region_level: self.config.manifest.chunk_level,
            state: self.sandbox.clone(), metadata: serde_json::to_value(self.saved_control())?,
        }.import_new(path).map_err(persistence_error)
    }

    /// Prepare a retained, bounded source-event batch without disk work on the
    /// tick thread. Keep the events until this returns true, then register them
    /// and call `request_region_checkpoint`. A pending batch freezes authority.
    pub fn prepare_source_batch(&mut self) -> Result<bool> {
        if self.persistence_pending() {
            return Ok(false);
        }
        if self.region_store.is_none() || !self.sandbox.is_partial() {
            self.region_source_activation = false;
            return Ok(true);
        }
        self.region_store
            .as_mut()
            .unwrap()
            .submit_hydrate(self.sandbox.clone())
            .map_err(persistence_error)?;
        self.region_source_activation = true;
        Ok(false)
    }

    /// Durably commit the global ledger, regional records and private host
    /// metadata together, then retire regions outside source residency and the
    /// domain dependency closure. On failure, live state is retained unchanged.
    /// This synchronous bounded reference operation belongs at a host batch
    /// boundary, not inside a real-time simulation system.
    pub fn checkpoint_regions(&mut self) -> Result<RegionStoreStatus> {
        self.finish_region_work()?;
        let metadata = serde_json::to_value(self.saved_control())?;
        let requested = self
            .source_regions
            .values()
            .flat_map(|keys| keys.iter().copied())
            .collect();
        let store = self
            .region_store
            .as_mut()
            .ok_or_else(|| NetError::Configuration("no regional store is attached".into()))?;
        let result = store
            .with_store(|store| store.checkpoint(&mut self.sandbox, metadata, &requested))
            .map_err(persistence_error);
        if result.is_ok() {
            self.region_checkpoint_pending = false;
            self.region_hydrated_for_input = false;
            self.region_source_activation = false;
            self.region_hydration_failed = false;
            self.region_last_error = None;
        }
        result
    }

    /// Public counts and last committed generation; no inventories, capabilities
    /// or file contents are exposed. Passive simulation may be newer than this
    /// checkpoint until the host commits again.
    pub fn region_store_status(&self) -> Option<RegionStoreStatus> {
        self.region_store.as_ref().map(RegionWorker::status)
    }

    /// Consistent online backup of the last committed generation, including WAL
    /// contents. The destination must be new. Call `checkpoint_regions` first
    /// when the backup should include the latest in-memory simulation changes.
    pub fn backup_region_store(&self, path: impl AsRef<Path>) -> Result<()> {
        self.region_store
            .as_ref()
            .ok_or_else(|| NetError::Configuration("no regional store is attached".into()))?
            .backup_to(path)
            .map_err(persistence_error)
    }

    fn hydrate_regions_for_input(&mut self) -> Result<()> {
        if let Some(store) = &mut self.region_store {
            self.region_checkpoint_pending = true;
            if self.sandbox.is_partial() {
                // Ordinary network input is hydrated asynchronously before
                // dispatch. This path is for explicit host maintenance APIs.
                store
                    .with_store(|store| store.hydrate(&mut self.sandbox))
                    .map_err(persistence_error)?;
            }
        }
        Ok(())
    }

    pub fn persistence_pending(&self) -> bool {
        self.region_store
            .as_ref()
            .is_some_and(RegionWorker::pending)
    }

    fn require_region_idle(&self) -> Result<()> {
        if self.persistence_pending() {
            return Err(persistence_error(
                "storage is working; defer host mutation until the next completed update",
            ));
        }
        Ok(())
    }

    /// Queue a checkpoint for the next nonblocking update barrier. Repeated
    /// requests coalesce; no additional world snapshots are buffered.
    pub fn request_region_checkpoint(&mut self) -> Result<()> {
        if self.region_store.is_none() {
            return Err(NetError::Configuration(
                "no regional store is attached".into(),
            ));
        }
        self.region_checkpoint_pending = true;
        Ok(())
    }

    fn install_region_completion(&mut self, completion: RegionCompletion) -> Result<()> {
        self.region_last_work_ms = completion.elapsed_ms;
        if let Some(error) = completion.error {
            self.region_hydration_failed = completion.kind == RegionWorkKind::Hydrate;
            self.region_last_error = Some(error.clone());
            return Err(persistence_error(error));
        }
        self.sandbox = completion
            .state
            .ok_or_else(|| persistence_error("storage returned no world"))?;
        self.region_last_error = None;
        self.region_hydration_failed = false;
        match completion.kind {
            RegionWorkKind::Hydrate => {
                self.region_hydrated_for_input = true;
            }
            RegionWorkKind::Checkpoint => {
                self.region_checkpoint_pending = false;
                self.region_hydrated_for_input = false;
            }
        }
        Ok(())
    }

    fn finish_region_work(&mut self) -> Result<()> {
        if let Some(store) = &mut self.region_store {
            if let Some(completion) = store.finish().map_err(persistence_error)? {
                self.install_region_completion(completion)?;
            }
        }
        Ok(())
    }

    fn regional_commit_barrier(&mut self, report: &mut ServerReport) -> bool {
        if let Some(store) = &mut self.region_store {
            match store.try_complete().map_err(persistence_error) {
                Ok(Some(completion)) => {
                    if let Err(error) = self.install_region_completion(completion) {
                        report.persistence_error = Some(error.to_string());
                        report.domain_errors += 1;
                        return false;
                    }
                }
                Ok(None) => {}
                Err(error) => {
                    self.region_last_error = Some(error.to_string());
                    report.persistence_error = self.region_last_error.clone();
                    return false;
                }
            }
        }
        if self.persistence_pending() {
            report.persistence_pending = true;
            report.persistence_error = self.region_last_error.clone();
            report.storage_work_ms = self.region_last_work_ms;
            return false;
        }
        if self.region_hydration_failed {
            let result = self
                .region_store
                .as_mut()
                .ok_or_else(|| persistence_error("regional store is absent"))
                .and_then(|store| {
                    store
                        .submit_hydrate(self.sandbox.clone())
                        .map_err(persistence_error)
                });
            report.persistence_pending = result.is_ok();
            report.persistence_error = self
                .region_last_error
                .clone()
                .or_else(|| result.err().map(|error| error.to_string()));
            return false;
        }
        if self.region_checkpoint_pending && !self.region_hydrated_for_input {
            let metadata = serde_json::to_value(self.saved_control());
            let requested = self
                .source_regions
                .values()
                .flat_map(|keys| keys.iter().copied())
                .collect();
            let result = metadata.map_err(NetError::from).and_then(|metadata| {
                self.region_store
                    .as_mut()
                    .ok_or_else(|| persistence_error("regional store is absent"))?
                    .submit_checkpoint(self.sandbox.clone(), metadata, requested)
                    .map_err(persistence_error)
            });
            if let Err(error) = result {
                report.persistence_error = Some(error.to_string());
                report.domain_errors += 1;
                report.tick = self.tick;
                report.clients = self
                    .peers
                    .values()
                    .filter(|peer| peer.actor_id.is_some())
                    .count();
                report.actors = self.actors.len();
                return false;
            }
            report.persistence_pending = true;
            report.persistence_error = self.region_last_error.clone();
            return false;
        }
        true
    }

    fn saved_control(&self) -> SavedServerControl {
        SavedServerControl {
            schema_version: 1,
            manifest: self.config.manifest.clone(),
            catalog: self.catalog.clone(),
            starting_inventory: self.starting_inventory.clone(),
            sessions: self.sessions.clone(),
            operator: self.operator.clone(),
            sandbox_sequences: self.sandbox_sequences.clone(),
            nonplayer_actors: self
                .actors
                .iter()
                .filter(|(id, _)| !self.sandbox.players.contains_key(id))
                .map(|(id, actor)| (*id, actor.clone()))
                .collect(),
            clock: self.clock.clone(),
            tick: self.tick,
        }
    }

    /// Explicit opt-in for an authored economy/population in the hosted world.
    pub fn enable_living_demo(&mut self, features: &[terra_core::WorldFeature]) -> Result<()> {
        self.require_region_idle()?;
        self.upgrade_reference_catalog()?;
        self.hydrate_regions_for_input()?;
        self.sandbox
            .seed_living_demo(self.config.spawn, features, &self.catalog)?;
        self.ground_living_positions(None);
        self.sandbox.ensure_transport_demo(&self.catalog)?;
        self.ensure_ecology()?;
        Ok(())
    }

    /// Extend only exact published reference catalogs. Custom rules remain
    /// unchanged, including catalogs that intentionally omit new resource goods.
    fn upgrade_reference_catalog(&mut self) -> Result<()> {
        if self.catalog.items.contains("berries") {
            return Ok(());
        }
        let current = Catalog::builtin();
        let mut previous = current.clone();
        for item in [
            "berries",
            "fiber",
            "fish",
            "iron_ore",
            "iron_ingot",
            "cooked_fish",
            "fishing_rod",
            "iron_tool",
        ] {
            previous.items.remove(item);
        }
        for recipe in ["fishing_rod", "cooked_fish", "iron_ingot", "iron_tool"] {
            previous.recipes.remove(recipe);
        }
        let installed = serde_json::to_value(&self.catalog)?;
        let is_v2 = serde_json::to_value(&previous)? == installed;
        for item in ["bread", "water", "flour"] {
            previous.items.remove(item);
        }
        for recipe in ["bread", "flour"] {
            previous.recipes.remove(recipe);
        }
        if is_v2 || serde_json::to_value(&previous)? == installed {
            if self.region_store.is_some() {
                return Err(NetError::Configuration(
                    "upgrade a legacy reference catalog before attaching its regional store".into(),
                ));
            }
            self.catalog = current;
        }
        Ok(())
    }

    /// Add the optional ecology layer to a loaded earlier living world without
    /// resetting or moving any existing player, facility, stock or resource.
    fn ensure_ecology(&mut self) -> Result<()> {
        if !self.sandbox.living.enabled
            || !self.sandbox.ecology.config.enabled
            || self
                .sandbox
                .ecology
                .seeded_regions
                .contains("ecology:starter:v1")
        {
            return Ok(());
        }
        self.upgrade_reference_catalog()?;
        let mut obstacles: Vec<_> = self
            .sandbox
            .structures
            .values()
            .map(|s| (s.position, s.radius_m))
            .collect();
        obstacles.extend(self.sandbox.living.facility_footprints());
        let environment = self.environment.as_ref();
        let origin = self.sandbox.living.origin.unwrap_or(self.config.spawn);
        self.sandbox
            .seed_ecology_near_with_environment(origin, &self.catalog, |p| {
                ecology_ground(environment, p, &obstacles)
            })?;
        Ok(())
    }

    fn ground_living_positions(&mut self, only_player: Option<u64>) {
        Self::ground_living_state(self.environment.as_ref(), &mut self.sandbox, only_player);
    }

    fn ground_living_state(
        environment: &dyn WorldEnvironment,
        sandbox: &mut SandboxState,
        only_player: Option<u64>,
    ) {
        let mut moved_facilities = Vec::new();
        if only_player.is_none() {
            for shop in sandbox.living.shops.values_mut() {
                if let Some(position) =
                    find_safe_ground(environment, shop.position, ActorKind::Human)
                {
                    moved_facilities.push((shop.position, position));
                    shop.position = position;
                }
            }
            for site in sandbox.living.producers.values_mut() {
                if let Some(position) =
                    find_safe_ground(environment, site.position, ActorKind::Human)
                {
                    moved_facilities.push((site.position, position));
                    site.position = position;
                }
            }
        }
        let remap = |position: GeoPosition| {
            moved_facilities
                .iter()
                .find(|(old, _)| old.surface_distance_m(position) < 0.5)
                .map(|(_, new)| *new)
                .unwrap_or_else(|| {
                    let mut p = position;
                    if let Some(height) = environment.ground_height_m(p) {
                        p.altitude_m = height;
                    }
                    p
                })
        };
        for unit in sandbox
            .living
            .units
            .values_mut()
            .filter(|u| only_player.is_none_or(|id| u.owner == Some(id)))
        {
            if let Some(position) = find_safe_ground(environment, unit.position, unit.kind) {
                unit.position = position;
            }
            unit.home = remap(unit.home);
            for point in unit.path.iter_mut().chain(&mut unit.patrol) {
                *point = remap(*point);
            }
        }
        if only_player.is_none() {
            for road in &mut sandbox.living.roads {
                for point in &mut road.points {
                    *point = remap(*point);
                }
            }
        }
        for account in sandbox.living.accounts.values() {
            if let (Some(unit), Some(player)) = (
                sandbox.living.units.get(&account.active_unit),
                sandbox.players.get_mut(&account.player_id),
            ) {
                player.position = unit.position;
            }
        }
    }

    pub fn set_catalog(&mut self, catalog: Catalog) -> Result<()> {
        self.set_catalog_and_starting_inventory(catalog, self.starting_inventory.clone())
    }

    /// Game-owned policy for new players. Both values are checked against the
    /// current world before either is changed; existing inventories are retained.
    pub fn set_catalog_and_starting_inventory(
        &mut self,
        catalog: Catalog,
        inventory: Inventory,
    ) -> Result<()> {
        self.require_region_idle()?;
        if self.region_store.is_some()
            && serde_json::to_value(&catalog)? != serde_json::to_value(&self.catalog)?
        {
            return Err(NetError::Configuration(
                "an attached regional store pins its catalog; migrate through an explicit export"
                    .into(),
            ));
        }
        self.sandbox.validate_with_catalog(&catalog)?;
        validate_starting_inventory(&catalog, &inventory)?;
        self.catalog = catalog;
        self.starting_inventory = inventory;
        Ok(())
    }

    /// Feed normalized resident chunks from the hosting world's streaming layer.
    /// This registers resources without changing the immutable source chunk.
    pub fn register_chunk(&mut self, chunk: &WorldChunk) {
        if let Err(error) = self.try_register_chunk(chunk) {
            eprintln!("Source chunk was not admitted: {error}");
        }
    }

    /// Fallible source registration for durable hosts. Persisted mutations are
    /// restored before providers may seed resources, protecting depletion and
    /// destruction tombstones when a source tile returns.
    pub fn try_register_chunk(&mut self, chunk: &WorldChunk) -> Result<()> {
        self.require_region_idle()?;
        chunk
            .key
            .validate()
            .map_err(|error| NetError::Configuration(error.to_string()))?;
        if chunk.key.level != self.config.manifest.chunk_level {
            return Err(NetError::Configuration(
                "source chunk level differs from the host manifest".into(),
            ));
        }
        let mut regions = BTreeSet::from([chunk.key]);
        for feature in &chunk.features {
            if let Some(position) = feature.points.first() {
                if let Ok(key) = TileKey::at(*position, self.config.manifest.chunk_level) {
                    regions.insert(key);
                }
            }
        }
        let mut requested = regions.clone();
        for (key, anchors) in &self.source_regions {
            if *key != chunk.key {
                requested.extend(anchors);
            }
        }
        if self.region_store.is_some() && requested.len() > terra_persistence::MAX_REQUESTED_REGIONS
        {
            return Err(NetError::Configuration(
                "resident source tiles and feature anchors exceed the regional request budget"
                    .into(),
            ));
        }
        if let Some(store) = &mut self.region_store {
            if self.sandbox.is_partial() {
                // Explicit host maintenance compatibility. Runtime source
                // streamers call prepare_source_batch before entering here.
                store
                    .with_store(|store| store.hydrate(&mut self.sandbox))
                    .map_err(persistence_error)?;
            }
            self.region_checkpoint_pending = true;
        }
        self.environment.register_chunk(chunk);
        self.source_regions.insert(chunk.key, regions);
        self.sandbox.register_chunk(chunk);
        // Resource/habitat definitions are authored overlays. They retain
        // depletion and are never used as evidence of observed animal counts.
        if let Err(error) = self.sandbox.register_ecology_chunk(chunk, &self.catalog) {
            eprintln!("Ecology chunk was not admitted: {error}");
        }
        // The domain's default tree-to-wood mapping is optional for games whose
        // catalog uses different resources. Metadata remains available.
        self.sandbox
            .resource_nodes
            .retain(|_, node| self.catalog.items.contains(&node.item));
        Ok(())
    }

    /// Drop only physical source geometry; persisted gathered-resource state
    /// remains in the domain and is not regenerated when a tile returns.
    pub fn evict_chunk(&mut self, key: TileKey) {
        if let Err(error) = self.try_evict_chunk(key) {
            eprintln!("Source eviction was deferred: {error}");
        }
    }

    pub fn try_evict_chunk(&mut self, key: TileKey) -> Result<()> {
        self.require_region_idle()?;
        self.environment.evict_chunk(key);
        self.source_regions.remove(&key);
        self.region_checkpoint_pending |= self.region_store.is_some();
        Ok(())
    }

    pub fn seed_resource(
        &mut self,
        id: String,
        position: GeoPosition,
        item: &str,
        amount: u32,
        regrow_seconds: f64,
    ) -> Result<()> {
        if !self.catalog.items.contains(item) {
            return Err(NetError::Configuration(
                "resource item is absent from the active catalog".into(),
            ));
        }
        if let Some(store) = &mut self.region_store {
            store
                .with_store(|store| store.hydrate(&mut self.sandbox))
                .map_err(persistence_error)?;
            self.region_checkpoint_pending = true;
        }
        self.sandbox
            .seed_resource(id, position, item, amount, regrow_seconds)?;
        Ok(())
    }

    /// Persist sandbox changes, player resume capabilities, clock and NPCs.
    /// The temporary file is fsynced then atomically replaces the destination.
    pub fn save_world(&self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        if let Some(store) = &self.region_store {
            store.check_output_path(path).map_err(persistence_error)?;
        }
        let sandbox = match &self.region_store {
            Some(store) => store.export(&self.sandbox).map_err(persistence_error)?,
            None => self.sandbox.clone(),
        };
        sandbox.validate_with_catalog(&self.catalog)?;
        sandbox.require_complete()?;
        validate_starting_inventory(&self.catalog, &self.starting_inventory)?;
        let saved = self.saved_control().with_sandbox(sandbox);
        let bytes = serde_json::to_vec_pretty(&saved)?;
        if bytes.len() as u64 > MAX_SAVE_BYTES {
            return Err(NetError::Protocol(
                "server save exceeds 32 MiB budget".into(),
            ));
        }
        terra_sandbox::publication::write_private(
            path,
            &bytes,
            terra_sandbox::publication::PublishMode::Replace,
        )?;
        Ok(())
    }

    /// Complete private server JSON export to a new path for operations tools.
    /// Host metadata is validated with the current terrain/provider contract.
    pub fn export_world_new(&self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        let sandbox = match &self.region_store {
            Some(store) => {
                store.check_output_path(path).map_err(persistence_error)?;
                store.export(&self.sandbox).map_err(persistence_error)?
            }
            None => self.sandbox.clone(),
        };
        let saved = self.saved_control().with_sandbox(sandbox);
        self.validate_saved_world(&saved)?;
        let bytes = serde_json::to_vec_pretty(&saved)?;
        if bytes.len() as u64 > MAX_SAVE_BYTES {
            return Err(NetError::Protocol(
                "server save exceeds 32 MiB budget".into(),
            ));
        }
        terra_persistence::write_new_private_file(path, &bytes).map_err(persistence_error)
    }

    /// Load only before accepting players. Every domain and session reference is
    /// validated before replacing live state, so rejected files are atomic.
    pub fn load_world(&mut self, path: impl AsRef<Path>) -> Result<()> {
        if !self.peers.is_empty() || self.region_store.is_some() {
            return Err(NetError::Protocol(
                "load world before connecting clients or attaching a regional store".into(),
            ));
        }
        if !std::fs::metadata(path.as_ref())?.is_file() {
            return Err(NetError::Protocol(
                "server save path must be a regular file".into(),
            ));
        }
        let file = std::fs::File::open(path)?;
        let mut bytes = Vec::new();
        file.take(MAX_SAVE_BYTES + 1).read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MAX_SAVE_BYTES {
            return Err(NetError::Protocol(
                "server save exceeds 32 MiB budget".into(),
            ));
        }
        let saved: SavedServer = serde_json::from_slice(&bytes)?;
        self.validate_saved_world(&saved)?;
        self.install_saved_world(saved);
        Ok(())
    }

    fn validate_saved_world(&self, saved: &SavedServer) -> Result<()> {
        if saved.schema_version != 1 || saved.manifest != self.config.manifest {
            return Err(NetError::Protocol(
                "save schema or base-world manifest mismatch".into(),
            ));
        }
        saved.sandbox.validate_with_catalog(&saved.catalog)?;
        saved.sandbox.require_complete()?;
        validate_starting_inventory(&saved.catalog, &saved.starting_inventory)?;
        saved.operator.validate(&saved.sandbox)?;
        let mut principals = BTreeSet::new();
        if saved.sessions.len() != saved.sandbox.players.len()
            || !saved.sessions.iter().all(|(token, id)| {
                valid_token(token)
                    && saved.sandbox.players.contains_key(id)
                    && principals.insert(*id)
            })
            || !saved
                .sandbox_sequences
                .keys()
                .all(|id| saved.sandbox.players.contains_key(id))
            || !saved.sandbox.players.values().all(|player| {
                validate_position(player.position) && self.environment.can_enter(player.position)
            })
            || !saved.clock.unix_seconds.is_finite()
            || saved.clock.unix_seconds.abs() > 1e12
            || !saved.clock.time_scale.is_finite()
            || !(0.0..=86_400.0).contains(&saved.clock.time_scale)
            || saved.nonplayer_actors.len() > self.config.max_actors
            || saved.tick > u64::MAX - 1_000_000
            || !saved.nonplayer_actors.iter().all(|(id, actor)| {
                *id != 0
                    && *id == actor.id
                    && !saved.sandbox.players.contains_key(id)
                    && validate_position(actor.position)
                    && self.environment.can_enter(actor.position)
                    && actor.validate().is_ok()
            })
        {
            return Err(NetError::Protocol(
                "save contains invalid ownership, clock or actor state".into(),
            ));
        }
        Ok(())
    }

    fn install_saved_world(&mut self, saved: SavedServer) {
        self.ecology_scheduler.reset();
        self.sandbox = saved.sandbox;
        // No network peer survives a host process restart. Freeze ownership
        // before the first regional checkpoint or simulation step, including
        // legacy saves written before explicit owner dormancy existed.
        let owners: Vec<_> = self.sandbox.players.keys().copied().collect();
        for owner in owners {
            self.sandbox
                .set_player_dormant(owner, true)
                .expect("validated saved principal exists");
        }
        self.catalog = saved.catalog;
        self.starting_inventory = saved.starting_inventory;
        self.sessions = saved.sessions;
        self.operator = saved.operator;
        self.sandbox_sequences = saved.sandbox_sequences;
        self.actors = saved.nonplayer_actors;
        self.clock = saved.clock;
        self.tick = saved.tick;
        self.next_actor_id = 1;
        self.accumulator = 0.0;
        self.snapshot_accumulator = 0.0;
        self.region_checkpoint_pending = false;
    }

    pub fn set_environment(&mut self, environment: Box<dyn WorldEnvironment>) {
        self.environment = environment;
    }

    /// Host-only insertion for NPCs/vehicles. Client ownership remains server-held.
    pub fn insert_actor(&mut self, actor: ActorState) -> Result<()> {
        self.require_region_idle()?;
        if actor.id == 0
            || !validate_position(actor.position)
            || !self.environment.can_enter(actor.position)
            || !actor.velocity_enu_m_s.iter().all(|value| value.is_finite())
            || !actor.heading_deg.is_finite()
        {
            return Err(NetError::Protocol("invalid actor state".into()));
        }
        if self.sandbox.players.contains_key(&actor.id) {
            return Err(NetError::Protocol(
                "cannot replace a client-owned actor".into(),
            ));
        }
        if !self.actors.contains_key(&actor.id) && self.actors.len() >= self.config.max_actors {
            return Err(NetError::Protocol("server actor budget exceeded".into()));
        }
        self.actors.insert(actor.id, actor);
        Ok(())
    }

    pub fn remove_actor(&mut self, actor_id: u64) -> Result<Option<ActorState>> {
        self.require_region_idle()?;
        if self.sandbox.players.contains_key(&actor_id) {
            return Err(NetError::Protocol(
                "cannot remove a connected player's actor".into(),
            ));
        }
        Ok(self.actors.remove(&actor_id))
    }

    pub fn update(&mut self, real_delta_seconds: f64) -> ServerReport {
        let delta = if real_delta_seconds.is_finite() {
            real_delta_seconds.max(0.0)
        } else {
            0.0
        };
        self.real_seconds += delta;
        let mut report = ServerReport::default();
        if !self.regional_commit_barrier(&mut report) {
            report.dropped_seconds = delta;
            self.finish_report(&mut report);
            return report;
        }
        if self.region_source_activation {
            // The source owner gets a chance to install its retained batch
            // before this hydrated world is retired or receives simulation.
            report.dropped_seconds = delta;
            self.finish_report(&mut report);
            return report;
        }
        // Prior batch successes are now durable. Flush before admitting another
        // batch so a continuous input stream cannot starve acknowledgements.
        self.flush_responses(&mut report);
        self.flush_deferred_sandbox_snapshots(&mut report);
        // Input applies to the next fixed step. A completed hydration is held
        // for its queued command batch; dormant records never receive a step.
        if !self.region_hydrated_for_input {
            self.advance_fixed_time(delta, &mut report);
        }
        self.accept_connections();
        for peer in self.peers.values_mut() {
            if peer.inbox.is_empty() && !peer.read_failed {
                match peer
                    .connection
                    .receive::<ClientMessage>(MAX_MESSAGES_PER_UPDATE)
                {
                    Ok(messages) => peer.inbox.extend(messages),
                    Err(_) => peer.read_failed = true,
                }
            }
        }
        let needs_complete = self.sandbox.is_partial()
            && self.peers.values().any(|peer| {
                peer.inbox
                    .iter()
                    .any(|message| self.message_requires_hydration(peer, message))
            });
        if needs_complete {
            let result = self
                .region_store
                .as_mut()
                .ok_or_else(|| persistence_error("partial world has no storage authority"))
                .and_then(|store| {
                    store
                        .submit_hydrate(self.sandbox.clone())
                        .map_err(persistence_error)
                });
            match result {
                Ok(_) => {
                    report.persistence_pending = true;
                }
                Err(error) => {
                    self.region_hydration_failed = true;
                    self.region_last_error = Some(error.to_string());
                    report.persistence_error = self.region_last_error.clone();
                }
            }
            self.finish_report(&mut report);
            return report;
        }
        let peer_ids: Vec<_> = self.peers.keys().copied().collect();
        for id in peer_ids {
            let Some(mut peer) = self.peers.remove(&id) else {
                continue;
            };
            peer.tokens = (peer.tokens + delta * MESSAGE_TOKENS_PER_SECOND).min(MESSAGE_BURST);
            peer.sandbox_tokens = (peer.sandbox_tokens + delta * 10.0).min(20.0);
            let mut keep = !peer.read_failed;
            while keep {
                if !peer.inbox.is_empty() && !peer.connection.has_frame_headroom() {
                    // Keep this bounded, already-decoded batch for a later
                    // update. No command is accepted without response capacity.
                    report.deferred_command_batches += 1;
                    break;
                }
                let Some(message) = peer.inbox.pop_front() else {
                    break;
                };
                report.messages += 1;
                if peer.tokens < 1.0 {
                    let _ = Self::error(&mut peer, "rate_limit", "message rate exceeded");
                    keep = false;
                    break;
                }
                peer.tokens -= 1.0;
                peer.last_activity = self.real_seconds;
                if self.handle_message(id, &mut peer, message).is_err() {
                    keep = false;
                }
            }
            if peer.actor_id.is_none()
                && self.real_seconds - peer.created_at > HANDSHAKE_TIMEOUT_SECONDS
            {
                keep = false;
            }
            if self.real_seconds - peer.last_activity > IDLE_TIMEOUT_SECONDS || peer.strikes >= 3 {
                keep = false;
            }
            report
                .replication
                .accumulate(&std::mem::take(&mut peer.replication.pending));
            if keep {
                self.peers.insert(id, peer);
            } else {
                if let Some(actor_id) = peer.actor_id {
                    self.disconnect_player(actor_id);
                }
                self.closing_peers.push(peer);
                report.disconnected += 1;
            }
        }
        if self.region_hydrated_for_input {
            self.region_checkpoint_pending = true;
            self.region_hydrated_for_input = false;
        }
        if self.regional_commit_barrier(&mut report) {
            self.flush_responses(&mut report);
        }
        self.finish_report(&mut report);
        report
    }

    fn finish_report(&self, report: &mut ServerReport) {
        report.tick = self.tick;
        report.clients = self
            .peers
            .values()
            .filter(|peer| peer.actor_id.is_some())
            .count();
        report.actors = self.actors.len();
        report.persistence_pending |= self.persistence_pending();
        report.storage_work_ms = self.region_last_work_ms;
        for peer in self.peers.values() {
            let stats = peer.connection.stats();
            report.max_peer_pending_bytes = report.max_peer_pending_bytes.max(stats.pending_bytes);
            report.peak_peer_pending_bytes =
                report.peak_peer_pending_bytes.max(stats.peak_pending_bytes);
        }
    }

    fn flush_deferred_sandbox_snapshots(&mut self, report: &mut ServerReport) {
        let ids: Vec<_> = self
            .peers
            .iter()
            .filter_map(|(id, peer)| {
                (peer.sandbox_snapshot_due && peer.connection.has_snapshot_headroom())
                    .then_some(*id)
            })
            .collect();
        for id in ids {
            let Some(mut peer) = self.peers.remove(&id) else {
                continue;
            };
            let Some(owner) = peer.actor_id else {
                self.peers.insert(id, peer);
                continue;
            };
            let result = self.send_sandbox_snapshot(&mut peer, owner);
            report
                .replication
                .accumulate(&std::mem::take(&mut peer.replication.pending));
            if result.is_ok() {
                self.peers.insert(id, peer);
            } else {
                self.disconnect_player(owner);
                report.disconnected += 1;
            }
        }
    }

    fn flush_responses(&mut self, report: &mut ServerReport) {
        for mut peer in self.closing_peers.drain(..) {
            let _ = peer.connection.flush();
        }
        let failed: Vec<_> = self
            .peers
            .iter_mut()
            .filter_map(|(id, peer)| peer.connection.flush().is_err().then_some(*id))
            .collect();
        for id in failed {
            if let Some(peer) = self.peers.remove(&id) {
                if let Some(actor_id) = peer.actor_id {
                    self.disconnect_player(actor_id);
                }
                report.disconnected += 1;
            }
        }
    }

    fn advance_fixed_time(&mut self, delta: f64, report: &mut ServerReport) {
        let accepted_delta = delta.min(0.25);
        report.dropped_seconds += delta - accepted_delta;
        self.accumulator += accepted_delta;
        let step = 1.0 / f64::from(self.config.tick_hz);
        while self.accumulator + f64::EPSILON >= step && report.fixed_steps < MAX_STEPS_PER_UPDATE {
            self.accumulator -= step;
            report.domain_errors += self.fixed_step(step);
            report.fixed_steps += 1;
            self.snapshot_accumulator +=
                f64::from(self.config.snapshot_hz) / f64::from(self.config.tick_hz);
            if self.snapshot_accumulator + f64::EPSILON >= 1.0 {
                self.snapshot_accumulator -= 1.0;
                self.broadcast_snapshots(report);
            }
        }
        if self.accumulator >= step {
            let retained = self.accumulator % step;
            report.dropped_seconds += self.accumulator - retained;
            self.accumulator = retained;
        }
    }

    fn accept_connections(&mut self) {
        for _ in 0..16 {
            let (stream, _) = match self.listener.accept() {
                Ok(value) => value,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(_) => break,
            };
            if self.peers.len() >= self.config.max_clients {
                continue;
            }
            #[cfg(feature = "tls")]
            let connection = match &self.tls {
                Some(tls) => Connection::new_tls_server(stream, tls),
                None => Connection::new(stream),
            };
            #[cfg(not(feature = "tls"))]
            let connection = Connection::new(stream);
            let Ok(connection) = connection else {
                continue;
            };
            let id = self.next_client_id;
            self.next_client_id = self.next_client_id.saturating_add(1);
            self.peers.insert(
                id,
                Peer {
                    connection,
                    inbox: VecDeque::new(),
                    read_failed: false,
                    actor_id: None,
                    interest: self.config.spawn,
                    radius_m: self.config.max_interest_radius_m.min(5_000.0),
                    last_sequence: None,
                    created_at: self.real_seconds,
                    last_activity: self.real_seconds,
                    last_movement: self.real_seconds,
                    drive_input: None,
                    tokens: MESSAGE_BURST,
                    sandbox_tokens: 20.0,
                    strikes: 0,
                    replication: PeerReplication::default(),
                    sandbox_snapshot_due: false,
                },
            );
        }
    }

    /// Cheap admission checks precede any whole-world activation. Unauthenticated
    /// commands, incompatible handshakes and replayed sequences cannot force a
    /// storage job or wake cold pages just to receive their rejection.
    fn message_requires_hydration(&self, peer: &Peer, message: &ClientMessage) -> bool {
        match message {
            ClientMessage::Hello {
                protocol,
                interest,
                resume_token,
                expected_manifest,
            } => {
                peer.actor_id.is_none()
                    && *protocol == PROTOCOL_VERSION
                    && validate_position(*interest)
                    && self.actors.len() < self.config.max_actors
                    && (resume_token.is_some() || self.config.allow_new_players)
                    && (resume_token.is_some()
                        || (self.sandbox.players.len() < terra_sandbox::MAX_PLAYERS
                            && (!self.sandbox.living.enabled
                                || self
                                    .sandbox
                                    .living_unit_count()
                                    .saturating_add(terra_sandbox::STARTER_UNIT_COUNT)
                                    <= terra_sandbox::MAX_LIVING_UNITS)))
                    && expected_manifest
                        .as_ref()
                        .is_none_or(|m| self.config.manifest.matches_data(m))
                    && resume_token.as_ref().is_none_or(|token| {
                        valid_token(token)
                            && self.sessions.get(token).is_some_and(|owner| {
                                !self.operator.suspended.contains(owner)
                                    && !self
                                        .peers
                                        .values()
                                        .any(|other| other.actor_id == Some(*owner))
                            })
                    })
            }
            ClientMessage::Sandbox { sequence, .. } => {
                *sequence > 0
                    && *sequence < u64::MAX
                    && peer.actor_id.is_some_and(|owner| {
                        self.sandbox_sequences
                            .get(&owner)
                            .is_none_or(|previous| sequence > previous)
                    })
            }
            _ => false,
        }
    }

    fn handle_message(
        &mut self,
        client_id: u64,
        peer: &mut Peer,
        message: ClientMessage,
    ) -> Result<()> {
        match message {
            ClientMessage::Hello {
                protocol,
                interest,
                resume_token,
                expected_manifest,
            } => {
                if peer.actor_id.is_some() {
                    return Self::error(peer, "hello_repeated", "handshake already completed");
                }
                if protocol != PROTOCOL_VERSION {
                    Self::error(peer, "version", "unsupported protocol version")?;
                    peer.strikes = 3;
                    return Ok(());
                }
                if !validate_position(interest) {
                    return Self::error(peer, "position", "invalid geographic interest");
                }
                if expected_manifest
                    .as_ref()
                    .is_some_and(|expected| !self.config.manifest.matches_data(expected))
                {
                    Self::error(
                        peer,
                        "world_mismatch",
                        "server base-world data does not match the requested manifest",
                    )?;
                    peer.strikes = 3;
                    return Ok(());
                }
                if self.actors.len() >= self.config.max_actors {
                    Self::error(peer, "capacity", "server actor budget exceeded")?;
                    peer.strikes = 3;
                    return Ok(());
                }
                if resume_token
                    .as_ref()
                    .is_some_and(|token| !valid_token(token) || !self.sessions.contains_key(token))
                {
                    return Self::error(peer, "resume", "invalid resume capability");
                }
                if resume_token
                    .as_ref()
                    .and_then(|token| self.sessions.get(token))
                    .is_some_and(|owner| self.operator.suspended.contains(owner))
                {
                    Self::error(
                        peer,
                        "access",
                        "this principal is suspended by the host operator",
                    )?;
                    peer.strikes = 3;
                    return Ok(());
                }
                // Repeat the cheap preflight inside the authoritative handler:
                // skipping async hydration is insufficient if a rejected Hello
                // can fall through to the explicit synchronous host boundary.
                if resume_token
                    .as_ref()
                    .and_then(|token| self.sessions.get(token))
                    .is_some_and(|owner| {
                        self.peers
                            .values()
                            .any(|other| other.actor_id == Some(*owner))
                    })
                {
                    return Self::error(peer, "connected", "player is already connected");
                }
                if resume_token.is_none() && !self.config.allow_new_players {
                    Self::error(
                        peer,
                        "registration_closed",
                        "this host accepts existing saved players only; obtain a session through the host's enrollment process",
                    )?;
                    peer.strikes = 3;
                    return Ok(());
                }
                if resume_token.is_none()
                    && (self.sandbox.players.len() >= terra_sandbox::MAX_PLAYERS
                        || (self.sandbox.living.enabled
                            && self
                                .sandbox
                                .living_unit_count()
                                .saturating_add(terra_sandbox::STARTER_UNIT_COUNT)
                                > terra_sandbox::MAX_LIVING_UNITS))
                {
                    Self::error(
                        peer,
                        "capacity",
                        "saved player capacity reached; resume an existing player",
                    )?;
                    peer.strikes = 3;
                    return Ok(());
                }
                if let Err(error) = self.hydrate_regions_for_input() {
                    return Self::error(peer, "storage", &error.to_string());
                }
                let (actor_id, spawn, token) = if let Some(token) = resume_token {
                    let Some(&actor_id) = self.sessions.get(&token).filter(|_| valid_token(&token))
                    else {
                        return Self::error(peer, "resume", "invalid resume capability");
                    };
                    let position = self
                        .sandbox
                        .players
                        .get(&actor_id)
                        .ok_or_else(|| NetError::Protocol("saved player is missing".into()))?
                        .position;
                    (actor_id, position, token)
                } else {
                    let prepared = match self.prepare_principal(client_id.saturating_sub(1)) {
                        Ok(prepared) => prepared,
                        Err(error) => return Self::error(peer, "registration", &error.to_string()),
                    };
                    self.sandbox = prepared.sandbox;
                    self.next_actor_id = prepared.next_actor_id;
                    self.sessions
                        .insert(prepared.token.clone(), prepared.player_id);
                    (prepared.player_id, prepared.spawn, prepared.token)
                };
                if let Err(error) = self.sandbox.ensure_living_player(actor_id) {
                    return Self::error(peer, "capacity", &error.to_string());
                }
                // Restored bodies keep their exact saved positions; only a new
                // principal passes the complete supported placement search.
                if let Err(error) = self.sandbox.ensure_transport_demo(&self.catalog) {
                    return Self::error(peer, "transport", &error.to_string());
                }
                if let Err(error) = self.sandbox.set_player_dormant(actor_id, false) {
                    return Self::error(peer, "storage", &error.to_string());
                }
                let spawn = self
                    .sandbox
                    .living
                    .active_unit(actor_id)
                    .map_or(spawn, |u| u.position);
                let active_kind = self
                    .sandbox
                    .living
                    .active_unit(actor_id)
                    .map_or(ActorKind::Human, |u| u.kind);
                let active_heading = self
                    .sandbox
                    .living
                    .active_unit(actor_id)
                    .map_or(0., |u| u.heading_deg);
                self.actors.insert(
                    actor_id,
                    ActorState {
                        id: actor_id,
                        kind: active_kind,
                        position: spawn,
                        velocity_enu_m_s: [0.0; 3],
                        heading_deg: active_heading,
                    },
                );
                peer.actor_id = Some(actor_id);
                peer.interest = interest;
                peer.connection.queue(&ServerMessage::Welcome {
                    protocol: PROTOCOL_VERSION,
                    client_id,
                    actor_id,
                    manifest: self.config.manifest.clone(),
                    tick: self.tick,
                    unix_seconds: self.clock.unix_seconds,
                    time_scale: self.clock.time_scale,
                    paused: self.clock.paused,
                    tick_hz: self.config.tick_hz,
                    spawn,
                    resume_token: token,
                    next_sandbox_sequence: self
                        .sandbox_sequences
                        .get(&actor_id)
                        .map_or(1, |value| value.saturating_add(1)),
                })?;
                self.send_sandbox_snapshot(peer, actor_id)
            }
            ClientMessage::Move {
                sequence,
                velocity_enu_m_s,
            } => {
                let Some(actor_id) = peer.actor_id else {
                    return Self::error(peer, "handshake", "hello required");
                };
                if peer
                    .last_sequence
                    .is_some_and(|previous| sequence <= previous)
                {
                    return Self::error(peer, "sequence", "stale movement sequence");
                }
                if !velocity_enu_m_s.iter().all(|value| value.is_finite()) {
                    return Self::error(peer, "velocity", "movement values must be finite");
                }
                if let Some(vehicle_id) = self
                    .sandbox
                    .living
                    .active_unit(actor_id)
                    .map(|u| u.id)
                    .filter(|id| self.sandbox.transport.vehicles.contains_key(id))
                {
                    if velocity_enu_m_s.iter().any(|v| v.abs() > 0.0001) {
                        return Self::error(
                            peer,
                            "drive_required",
                            "road vehicles require steering and pedal input",
                        );
                    }
                    peer.drive_input = Some((vehicle_id, terra_core::DriveInput::stopped()));
                    peer.last_sequence = Some(sequence);
                    peer.last_movement = self.real_seconds;
                    return Ok(());
                }
                peer.drive_input = None;
                let Some(actor) = self.actors.get_mut(&actor_id) else {
                    return Self::error(peer, "actor_missing", "owned actor is unavailable");
                };
                let speed = velocity_enu_m_s
                    .iter()
                    .map(|component| component * component)
                    .sum::<f64>()
                    .sqrt();
                let allowed = self.sandbox.living.active_unit(actor_id).map_or_else(
                    || speed_limit(&actor.kind),
                    |unit| unit.base_speed_limit_m_s(),
                );
                if !speed.is_finite() || speed > allowed {
                    return Self::error(peer, "speed", "movement exceeds actor speed limit");
                }
                if !is_airborne(&actor.kind) && velocity_enu_m_s[2].abs() > f64::EPSILON {
                    return Self::error(peer, "vertical_movement", "surface actor cannot fly");
                }
                actor.velocity_enu_m_s = velocity_enu_m_s;
                // Snapshot-delayed clients may request their prior needs cap.
                // Validate the fixed intent cap, then enforce current needs here.
                if let Some(unit) = self.sandbox.living.active_unit(actor_id) {
                    let cap = unit.speed_limit_m_s();
                    if speed > cap {
                        for v in &mut actor.velocity_enu_m_s {
                            *v *= cap / speed;
                        }
                    }
                }
                if speed > 0.001 {
                    if let Some(account) = self.sandbox.living.accounts.get_mut(&actor_id) {
                        account.possessed = true;
                    }
                }
                if speed > 0.01 {
                    actor.heading_deg = velocity_enu_m_s[0]
                        .atan2(velocity_enu_m_s[1])
                        .to_degrees()
                        .rem_euclid(360.0) as f32;
                }
                peer.last_sequence = Some(sequence);
                peer.last_movement = self.real_seconds;
                Ok(())
            }
            ClientMessage::Drive { sequence, input } => {
                let Some(actor_id) = peer.actor_id else {
                    return Self::error(peer, "handshake", "hello required");
                };
                if peer
                    .last_sequence
                    .is_some_and(|previous| sequence <= previous)
                {
                    return Self::error(peer, "sequence", "stale movement sequence");
                }
                if input.validate().is_err() {
                    return Self::error(
                        peer,
                        "drive_input",
                        "pedals and steering must be finite normalized values",
                    );
                }
                let Some(account) = self
                    .sandbox
                    .living
                    .accounts
                    .get(&actor_id)
                    .filter(|a| a.possessed)
                else {
                    return Self::error(
                        peer,
                        "possession",
                        "take control of an owned vehicle first",
                    );
                };
                let Some(vehicle) = self
                    .sandbox
                    .transport
                    .vehicles
                    .get(&account.active_unit)
                    .filter(|v| v.owner == actor_id)
                else {
                    return Self::error(
                        peer,
                        "vehicle",
                        "the active body is not your registered road vehicle",
                    );
                };
                peer.drive_input = Some((vehicle.unit_id, input));
                peer.last_sequence = Some(sequence);
                peer.last_movement = self.real_seconds;
                Ok(())
            }
            ClientMessage::Focus { position, radius_m } => {
                if peer.actor_id.is_none() {
                    return Self::error(peer, "handshake", "hello required");
                }
                if !validate_position(position)
                    || !radius_m.is_finite()
                    || !(10.0..=self.config.max_interest_radius_m).contains(&radius_m)
                {
                    return Self::error(peer, "interest", "invalid interest position or radius");
                }
                peer.interest = position;
                peer.radius_m = radius_m;
                Ok(())
            }
            ClientMessage::Sandbox { sequence, command } => {
                let Some(actor_id) = peer.actor_id else {
                    return Self::error(peer, "handshake", "hello required");
                };
                if peer.sandbox_tokens < 1.0 {
                    return Self::error(peer, "sandbox_rate", "sandbox command rate exceeded");
                }
                peer.sandbox_tokens -= 1.0;
                if sequence == 0 || sequence == u64::MAX {
                    return peer.connection.queue(&ServerMessage::SandboxRejected {
                        sequence,
                        message: "sandbox sequence must be positive and below u64::MAX".into(),
                    });
                }
                if self
                    .sandbox_sequences
                    .get(&actor_id)
                    .is_some_and(|previous| sequence <= *previous)
                {
                    return peer.connection.queue(&ServerMessage::SandboxRejected {
                        sequence,
                        message: "stale sandbox sequence".into(),
                    });
                }
                self.sandbox_sequences.insert(actor_id, sequence);
                if let Err(error) = self.hydrate_regions_for_input() {
                    return peer.connection.queue(&ServerMessage::SandboxRejected {
                        sequence,
                        message: error.to_string(),
                    });
                }
                let position = self
                    .actors
                    .get(&actor_id)
                    .ok_or_else(|| NetError::Protocol("owned actor is missing".into()))?
                    .position;
                self.sandbox.set_player_position(actor_id, position)?;
                let target = match &command {
                    terra_sandbox::Command::Settlement {
                        action: terra_sandbox::SettlementCommand::Found { center, .. },
                    } => Some((*center, 0., false)),
                    terra_sandbox::Command::ClaimPlot { center, .. } => Some((*center, 0., false)),
                    terra_sandbox::Command::Build {
                        position,
                        blueprint,
                    } => Some((
                        *position,
                        self.catalog
                            .blueprints
                            .get(blueprint)
                            .map_or(0., |b| b.radius_m),
                        true,
                    )),
                    terra_sandbox::Command::Plant { position, .. } => Some((*position, 1., true)),
                    _ => None,
                };
                if let Some((target, radius, footprint)) = target {
                    let clear = !footprint
                        || self
                            .sandbox
                            .living
                            .facility_footprints()
                            .iter()
                            .all(|(p, r)| p.surface_distance_m(target) >= r + radius);
                    if !clear || !self.environment.allows_placement(target, radius, footprint) {
                        return peer.connection.queue(&ServerMessage::SandboxRejected {
                            sequence,
                            message: "placement requires a known terrain surface, dry gentle ground, and clear source/facility footprints".into(),
                        });
                    }
                }
                let prior_control = self
                    .sandbox
                    .living
                    .accounts
                    .get(&actor_id)
                    .map(|a| (a.active_unit, a.possessed));
                let takeover = matches!(
                    &command,
                    terra_sandbox::Command::Living {
                        action: terra_sandbox::LivingCommand::Possess { .. }
                            | terra_sandbox::LivingCommand::Release
                            | terra_sandbox::LivingCommand::SelectActive { .. }
                            | terra_sandbox::LivingCommand::StopUnits { .. }
                            | terra_sandbox::LivingCommand::Rest { .. }
                    } | terra_sandbox::Command::Transport {
                        action: terra_sandbox::TransportCommand::EnterVehicle { .. }
                            | terra_sandbox::TransportCommand::ExitVehicle
                            | terra_sandbox::TransportCommand::SetAutopilot { .. }
                    }
                );
                let outcome = match command {
                    terra_sandbox::Command::Living {
                        action:
                            terra_sandbox::LivingCommand::MoveUnits {
                                unit_ids,
                                destination,
                            },
                    } => {
                        let environment = self.environment.as_ref();
                        self.sandbox.move_units_with_environment(
                            actor_id,
                            unit_ids,
                            destination,
                            &self.catalog,
                            |kind, previous, point| {
                                if !environment.can_enter(point) {
                                    return None;
                                }
                                environment
                                    .resolve_movement(kind, previous, point)
                                    .filter(|p| validate_position(*p) && environment.can_enter(*p))
                            },
                        )
                    }
                    terra_sandbox::Command::Transport {
                        action: terra_sandbox::TransportCommand::ExitVehicle,
                    } => {
                        let environment = self.environment.as_ref();
                        self.sandbox
                            .exit_vehicle_with_ground(actor_id, &self.catalog, |point| {
                                if !environment.can_enter(point) {
                                    return None;
                                }
                                environment
                                    .resolve_movement(ActorKind::Human, point, point)
                                    .filter(|p| validate_position(*p) && environment.can_enter(*p))
                            })
                    }
                    terra_sandbox::Command::Transport {
                        action:
                            action @ (terra_sandbox::TransportCommand::BuyVehicle { .. }
                            | terra_sandbox::TransportCommand::RentVehicle { .. }),
                    } => {
                        let environment = self.environment.as_ref();
                        self.sandbox.deliver_vehicle_with_ground(
                            actor_id,
                            action,
                            &self.catalog,
                            |point, height| {
                                if !environment.can_enter(point) {
                                    return None;
                                }
                                environment
                                    .vehicle_support_with_clearance(point, height)
                                    .filter(|p| validate_position(*p) && environment.can_enter(*p))
                            },
                        )
                    }
                    command => self.sandbox.apply(actor_id, command, &self.catalog),
                };
                match outcome {
                    Ok(outcome) => {
                        if takeover
                            || prior_control
                                != self
                                    .sandbox
                                    .living
                                    .accounts
                                    .get(&actor_id)
                                    .map(|a| (a.active_unit, a.possessed))
                        {
                            peer.drive_input = None;
                        }
                        peer.connection
                            .queue(&ServerMessage::SandboxResult { sequence, outcome })?;
                    }
                    Err(error) => peer.connection.queue(&ServerMessage::SandboxRejected {
                        sequence,
                        message: error.to_string(),
                    })?,
                }
                if let (Some(unit), Some(actor)) = (
                    self.sandbox.living.active_unit(actor_id),
                    self.actors.get_mut(&actor_id),
                ) {
                    actor.kind = unit.kind;
                    actor.position = unit.position;
                    actor.heading_deg = unit.heading_deg;
                    actor.velocity_enu_m_s = unit.velocity_enu_m_s;
                }
                self.send_sandbox_snapshot(peer, actor_id)
            }
            ClientMessage::Resync {} => {
                let Some(actor_id) = peer.actor_id else {
                    return Self::error(peer, "handshake", "hello required");
                };
                if peer
                    .replication
                    .last_resync_at
                    .is_some_and(|last| self.real_seconds - last < 1.)
                {
                    return Self::error(
                        peer,
                        "resync_rate",
                        "replication resync is limited to once per second",
                    );
                }
                peer.replication.last_resync_at = Some(self.real_seconds);
                peer.replication.pending.resync_requests += 1;
                peer.replication.reset_baselines();
                self.send_sandbox_snapshot(peer, actor_id)
            }
            ClientMessage::Ping { nonce } => peer.connection.queue(&ServerMessage::Pong {
                nonce,
                tick: self.tick,
            }),
        }
    }

    fn error(peer: &mut Peer, code: &str, message: &str) -> Result<()> {
        peer.strikes = peer.strikes.saturating_add(1);
        peer.connection.queue(&ServerMessage::Error {
            code: code.into(),
            message: message.into(),
        })
    }

    fn fixed_step(&mut self, step: f64) -> usize {
        let mut errors = 0;
        for peer in self.peers.values() {
            if self.real_seconds - peer.last_movement > INPUT_TIMEOUT_SECONDS {
                if let Some(actor) = peer.actor_id.and_then(|id| self.actors.get_mut(&id)) {
                    actor.velocity_enu_m_s = [0.0; 3];
                }
            }
        }
        let mut facility_footprints = self.sandbox.living.facility_footprints();
        facility_footprints.extend(
            self.sandbox
                .structures
                .values()
                .map(|s| (s.position, s.radius_m)),
        );
        let mut contacts = match terra_sandbox::LivingContacts::new(&self.sandbox) {
            Ok(contacts) => contacts,
            Err(_) => return 1,
        };
        for actor in self.actors.values_mut() {
            if self.clock.paused {
                continue;
            }
            if self
                .sandbox
                .living
                .accounts
                .get(&actor.id)
                .is_some_and(|account| !account.possessed)
            {
                continue;
            }
            if let Some(unit) = self.sandbox.living.active_unit(actor.id).cloned() {
                if let Some(vehicle) = self.sandbox.transport.vehicles.get(&unit.id).cloned() {
                    let mut dynamics = vehicle.dynamics;
                    let config = self.sandbox.transport.vehicle_dynamics_config(unit.id);
                    let support = config.and_then(|config| {
                        terra_core::sample_vehicle_ground(
                            actor.position,
                            f64::from(actor.heading_deg),
                            config.wheelbase_m,
                            vehicle.model.dimensions_m()[0] * 0.86,
                            |p| {
                                self.environment
                                    .vehicle_ground_sample(p)
                                    .map(|s| self.sandbox.living.apply_authored_road_surface(p, s))
                            },
                        )
                        .ok()
                    });
                    let cap = self
                        .sandbox
                        .vehicle_speed_limit_m_s(unit.id)
                        .unwrap_or(0.)
                        .min(unit.speed_limit_m_s());
                    if cap <= 0. || config.is_none() || support.is_none() {
                        actor.velocity_enu_m_s = [0.; 3];
                        dynamics.stop();
                        if vehicle.fuel_l <= 0. {
                            dynamics.engine_rpm = 0.;
                        }
                        if self
                            .sandbox
                            .set_controlled_motion(
                                actor.id,
                                actor.position,
                                [0.; 3],
                                actor.heading_deg,
                            )
                            .is_err()
                            || contacts
                                .update(unit.id, actor.position, actor.heading_deg, [0.; 3])
                                .is_err()
                        {
                            errors += 1;
                        }
                    } else {
                        let input = self
                            .peers
                            .values()
                            .find(|p| p.actor_id == Some(actor.id))
                            .filter(|p| {
                                self.real_seconds - p.last_movement <= INPUT_TIMEOUT_SECONDS
                            })
                            .and_then(|p| p.drive_input)
                            .filter(|(id, _)| *id == unit.id)
                            .map_or_else(terra_core::DriveInput::stopped, |(_, input)| input);
                        let mut config = config.unwrap();
                        let support = support.unwrap();
                        config.max_speed_m_s = cap.max(1.);
                        let weather = synthetic_weather(
                            &actor.position,
                            &self.clock,
                            self.config.manifest.seed,
                        );
                        match terra_core::drive_actor(
                            actor,
                            &mut dynamics,
                            &config,
                            input,
                            step,
                            support.grip_scale(terra_core::road_weather_grip(
                                weather.precipitation_mm_h,
                                weather.temperature_c,
                            )),
                            support.pitch_radians,
                        ) {
                            Ok(mut candidate) => {
                                let old = actor.position;
                                let resolved = resolve_vehicle_motion(
                                    self.environment.as_ref(),
                                    old,
                                    candidate.position,
                                    actor.heading_deg,
                                    candidate.heading_deg,
                                    vehicle.model.dimensions_m(),
                                    &facility_footprints,
                                );
                                let mut impact = 0.;
                                if let Some(position) = resolved {
                                    candidate.position = position;
                                    match contacts.resolve(
                                        unit.id,
                                        old,
                                        actor.heading_deg,
                                        candidate.position,
                                        candidate.heading_deg,
                                        candidate.velocity_enu_m_s,
                                    ) {
                                        Ok(admitted) => {
                                            candidate.position = admitted.position;
                                            candidate.heading_deg = admitted.heading_deg;
                                            if admitted.contact.is_some() {
                                                candidate.velocity_enu_m_s = [0.; 3];
                                                dynamics.stop();
                                                impact = admitted.impact_speed_m_s;
                                            }
                                            *actor = candidate;
                                        }
                                        Err(_) => {
                                            actor.velocity_enu_m_s = [0.; 3];
                                            dynamics.stop();
                                            errors += 1;
                                        }
                                    }
                                } else {
                                    // Missing or forbidden terrain beneath any
                                    // part of the model stops admission without
                                    // inventing a collision against unloaded data.
                                    let covered = terra_core::vehicle_support_points(
                                        candidate.position,
                                        f64::from(candidate.heading_deg),
                                        vehicle.model.dimensions_m(),
                                    )
                                    .ok()
                                    .is_some_and(|points| {
                                        points.into_iter().all(|point| {
                                            self.environment.can_enter(point)
                                                && self
                                                    .environment
                                                    .ground_height_m(point)
                                                    .is_some_and(|height| {
                                                        validate_position(GeoPosition {
                                                            altitude_m: height,
                                                            ..point
                                                        })
                                                    })
                                        })
                                    });
                                    if covered {
                                        impact = dynamics.speed_m_s.abs();
                                    }
                                    actor.velocity_enu_m_s = [0.; 3];
                                    dynamics.stop();
                                }
                                if contacts
                                    .update(
                                        unit.id,
                                        actor.position,
                                        actor.heading_deg,
                                        actor.velocity_enu_m_s,
                                    )
                                    .is_err()
                                {
                                    errors += 1;
                                }
                                // Accounting (including an attached trailer)
                                // reads the admitted pose, not last frame's body.
                                if self
                                    .sandbox
                                    .set_controlled_motion(
                                        actor.id,
                                        actor.position,
                                        actor.velocity_enu_m_s,
                                        actor.heading_deg,
                                    )
                                    .is_err()
                                {
                                    errors += 1;
                                }
                                if self
                                    .sandbox
                                    .record_vehicle_motion(
                                        actor.id,
                                        unit.id,
                                        old.surface_distance_m(actor.position),
                                        step,
                                        impact,
                                    )
                                    .is_err()
                                {
                                    errors += 1;
                                }
                            }
                            Err(_) => {
                                actor.velocity_enu_m_s = [0.; 3];
                                dynamics.stop();
                                errors += 1;
                            }
                        }
                    }
                    self.sandbox
                        .transport
                        .vehicles
                        .get_mut(&unit.id)
                        .unwrap()
                        .dynamics = dynamics;
                    continue;
                }
            }
            if let Some(unit) = self.sandbox.living.active_unit(actor.id) {
                let cap = unit.speed_limit_m_s();
                let speed = actor.speed_m_s();
                if speed > cap {
                    for v in &mut actor.velocity_enu_m_s {
                        *v *= cap / speed;
                    }
                }
            }
            let old_position = actor.position;
            let resolved = actor.advance(step).ok().and_then(|()| {
                resolve_authoritative_motion(
                    self.environment.as_ref(),
                    actor.kind,
                    old_position,
                    actor.position,
                    &facility_footprints,
                )
            });
            if let Some(position) = resolved {
                if let Some(unit) = self.sandbox.living.active_unit(actor.id) {
                    match contacts.resolve(
                        unit.id,
                        old_position,
                        unit.heading_deg,
                        position,
                        actor.heading_deg,
                        actor.velocity_enu_m_s,
                    ) {
                        Ok(admitted) => {
                            actor.position = admitted.position;
                            actor.heading_deg = admitted.heading_deg;
                            if admitted.contact.is_some() {
                                actor.velocity_enu_m_s = [0.; 3];
                            }
                            if contacts
                                .update(
                                    unit.id,
                                    actor.position,
                                    actor.heading_deg,
                                    actor.velocity_enu_m_s,
                                )
                                .is_err()
                            {
                                errors += 1;
                            }
                        }
                        Err(_) => {
                            actor.position = old_position;
                            actor.velocity_enu_m_s = [0.; 3];
                            errors += 1;
                        }
                    }
                } else {
                    actor.position = position;
                }
            } else {
                actor.position = old_position;
                actor.velocity_enu_m_s = [0.0; 3];
            }
        }
        self.tick += 1;
        for peer in self.peers.values() {
            if let Some(actor_id) = peer.actor_id {
                if let Some(actor) = self.actors.get(&actor_id) {
                    let result = if self
                        .sandbox
                        .living
                        .accounts
                        .get(&actor_id)
                        .is_some_and(|account| account.possessed)
                    {
                        self.sandbox.set_controlled_motion(
                            actor_id,
                            actor.position,
                            actor.velocity_enu_m_s,
                            actor.heading_deg,
                        )
                    } else if !self.sandbox.living.enabled {
                        self.sandbox.set_player_position(actor_id, actor.position)
                    } else {
                        Ok(())
                    };
                    if result.is_err() {
                        errors += 1;
                    }
                }
            }
        }
        let previous_time = self.clock.unix_seconds;
        if self.ensure_ecology().is_err() {
            errors += 1;
        }
        self.clock.advance(step);
        let weather = synthetic_weather(&self.config.spawn, &self.clock, self.config.manifest.seed);
        if self
            .sandbox
            .advance(self.clock.unix_seconds - previous_time, &weather)
            .is_err()
        {
            errors += 1;
        }
        let environment = &self.environment;
        let real_dt = if self.clock.paused { 0.0 } else { step };
        let sim_dt = self.clock.unix_seconds - previous_time;
        let hour = (self.clock.utc_hour() + self.config.spawn.longitude / 15.).rem_euclid(24.);
        let connected: BTreeSet<_> = self.peers.values().filter_map(|p| p.actor_id).collect();
        let observers: Vec<_> = self
            .sandbox
            .living
            .units
            .values()
            .filter(|u| u.owner.is_some_and(|owner| connected.contains(&owner)))
            .map(|u| u.position)
            .collect();
        if self
            .sandbox
            .advance_ecology_scheduled_with_environment(
                real_dt,
                sim_dt,
                hour,
                &mut self.ecology_scheduler,
                &observers,
                |p| ecology_ground(environment.as_ref(), p, &facility_footprints),
            )
            .is_err()
        {
            errors += 1;
        }
        if self
            .sandbox
            .advance_living_with_vehicle_environment(
                real_dt,
                self.clock.unix_seconds - previous_time,
                &self.catalog,
                true,
                &|kind, old, candidate| {
                    resolve_authoritative_motion(
                        environment.as_ref(),
                        kind,
                        old,
                        candidate,
                        &facility_footprints,
                    )
                },
                terra_sandbox::AutonomousVehicleEnvironment {
                    sample: &|p| environment.vehicle_ground_sample(p),
                    resolve: &|from, to, dimensions| {
                        resolve_vehicle_motion(
                            environment.as_ref(),
                            from.position,
                            to.position,
                            from.heading_deg,
                            to.heading_deg,
                            dimensions,
                            &facility_footprints,
                        )
                    },
                    weather_grip: terra_core::road_weather_grip(
                        weather.precipitation_mm_h,
                        weather.temperature_c,
                    ),
                },
            )
            .is_err()
        {
            errors += 1;
        }
        if self
            .sandbox
            .advance_settlements(sim_dt, &self.catalog)
            .is_err()
        {
            errors += 1;
        }
        for (id, actor) in &mut self.actors {
            if let Some(unit) = self.sandbox.living.active_unit(*id) {
                actor.kind = unit.kind;
                actor.position = unit.position;
                actor.velocity_enu_m_s = unit.velocity_enu_m_s;
                actor.heading_deg = unit.heading_deg;
            }
        }
        errors
    }

    fn broadcast_snapshots(&mut self, report: &mut ServerReport) {
        // Rebuild immutable spatial indices once for this entire fanout. The
        // borrowed sandbox index cannot become stale while views are assembled.
        let actor_index = match GeoSpatialIndex::from_positions(
            1_000.,
            self.actors.values().map(|a| (a.id, a.position)),
        ) {
            Ok(index) => index,
            Err(_) => {
                report.domain_errors += 1;
                return;
            }
        };
        let keyframe_ticks = u64::from(self.config.tick_hz) * 10;
        let sandbox_period = if self.sandbox.living.enabled {
            self.config.tick_hz.div_ceil(5)
        } else {
            self.config.tick_hz
        };
        let sandbox_index = if self.tick.is_multiple_of(u64::from(sandbox_period)) {
            match SandboxInterestIndex::new(&self.sandbox) {
                Ok(index) => Some(index),
                Err(_) => {
                    report.domain_errors += 1;
                    return;
                }
            }
        } else {
            None
        };
        let mut disconnected = BTreeSet::new();
        for (client_id, peer) in &mut self.peers {
            let Some(owned_actor) = peer.actor_id else {
                continue;
            };
            if !peer.connection.has_snapshot_headroom() {
                report.replication.backpressure_deferrals += 1;
                peer.sandbox_snapshot_due |= sandbox_index.is_some();
                continue;
            }
            // This fanout runs immediately after a completed fixed step and
            // before receiving the next input batch. The acknowledged sequence
            // therefore belongs to the pose being published, not future input.
            if let Some(primary) = self.actors.get(&owned_actor) {
                let unit = self.sandbox.living.active_unit(owned_actor);
                let vehicle = unit.and_then(|unit| self.sandbox.transport.vehicles.get(&unit.id));
                let actor = unit.map_or_else(
                    || primary.clone(),
                    |unit| ActorState {
                        id: unit.id,
                        kind: unit.kind,
                        position: unit.position,
                        velocity_enu_m_s: unit.velocity_enu_m_s,
                        heading_deg: unit.heading_deg,
                    },
                );
                let cap =
                    unit.map_or_else(|| speed_limit(&actor.kind), |unit| unit.speed_limit_m_s());
                let maximum_speed_m_s = vehicle.map_or(cap, |vehicle| {
                    self.sandbox
                        .vehicle_speed_limit_m_s(vehicle.unit_id)
                        .unwrap_or(0.)
                        .min(cap)
                });
                let state = crate::prediction::MotionSnapshot {
                    player_id: owned_actor,
                    tick: self.tick,
                    sequence: peer.last_sequence,
                    actor,
                    movement_allowed: !self.clock.paused
                        && self
                            .sandbox
                            .living
                            .accounts
                            .get(&owned_actor)
                            .is_none_or(|account| account.possessed),
                    maximum_speed_m_s,
                    vehicle: vehicle.and_then(|vehicle| {
                        self.sandbox
                            .transport
                            .vehicle_dynamics_config(vehicle.unit_id)
                            .map(|config| crate::prediction::PredictedVehicle {
                                config,
                                dynamics: vehicle.dynamics,
                                dimensions_m: vehicle.model.dimensions_m(),
                            })
                    }),
                };
                if state.validate().is_err()
                    || peer
                        .connection
                        .queue(&ServerMessage::MotionState { state })
                        .is_err()
                {
                    disconnected.insert(*client_id);
                    continue;
                }
            }
            let query = match actor_index.query(peer.interest, peer.radius_m) {
                Ok(query) => query,
                Err(_) => {
                    disconnected.insert(*client_id);
                    continue;
                }
            };
            report.interest_candidates_tested += query.candidates_tested;
            let mut ids = query.ids;
            if !ids.contains(&owned_actor) {
                ids.push(owned_actor);
            }
            let mut visible: Vec<_> = ids
                .into_iter()
                .filter_map(|id| self.actors.get(&id))
                .map(|actor| {
                    (
                        actor.id != owned_actor,
                        surface_distance_m(peer.interest, actor.position),
                        actor,
                    )
                })
                .collect();
            let count = visible.len();
            let compare = |a: &(bool, f64, &ActorState), b: &(bool, f64, &ActorState)| {
                a.0.cmp(&b.0)
                    .then_with(|| a.1.total_cmp(&b.1))
                    .then_with(|| a.2.id.cmp(&b.2.id))
            };
            if visible.len() > self.config.max_snapshot_actors {
                visible.select_nth_unstable_by(self.config.max_snapshot_actors, compare);
                visible.truncate(self.config.max_snapshot_actors);
            }
            visible.sort_by(compare);
            let actors = visible
                .into_iter()
                .take(self.config.max_snapshot_actors)
                .map(|(_, _, a)| a.clone())
                .collect();
            let result = peer
                .replication
                .actors(
                    self.tick,
                    self.clock.unix_seconds,
                    actors,
                    count > self.config.max_snapshot_actors,
                    count,
                    keyframe_ticks,
                )
                .and_then(|message| peer.connection.queue(&message));
            if result.is_err() {
                disconnected.insert(*client_id);
            } else {
                report.snapshots_sent += 1;
            }
            if let Some(sandbox_index) = &sandbox_index {
                if !peer.connection.has_snapshot_headroom() {
                    peer.sandbox_snapshot_due = true;
                    report.replication.backpressure_deferrals += 1;
                } else {
                    let result = self
                        .sandbox
                        .snapshot_for_indexed(owned_actor, peer.radius_m, sandbox_index)
                        .map_err(NetError::from)
                        .and_then(|view| peer.replication.sandbox(self.tick, view, keyframe_ticks))
                        .and_then(|message| peer.connection.queue(&message));
                    if result.is_err() {
                        disconnected.insert(*client_id);
                    } else {
                        peer.sandbox_snapshot_due = false;
                    }
                }
            }
            if peer.connection.flush().is_err() {
                disconnected.insert(*client_id);
            }
            report
                .replication
                .accumulate(&std::mem::take(&mut peer.replication.pending));
        }
        for id in disconnected {
            if let Some(peer) = self.peers.remove(&id) {
                if let Some(actor_id) = peer.actor_id {
                    self.disconnect_player(actor_id);
                }
                report.disconnected += 1;
            }
        }
    }

    fn disconnect_player(&mut self, player_id: u64) {
        self.actors.remove(&player_id);
        self.region_checkpoint_pending |= self.region_store.is_some();
        // Personal orders, needs, vehicle work and company payroll freeze.
        // The next checkpoint retires owned bodies/homes with their regions;
        // ownership/capabilities stay in the bounded private global ledger.
        let _ = self.sandbox.set_player_dormant(player_id, true);
    }

    fn send_sandbox_snapshot(&self, peer: &mut Peer, actor_id: u64) -> Result<()> {
        peer.sandbox_snapshot_due = true;
        if !peer.connection.has_snapshot_headroom() {
            peer.replication.pending.backpressure_deferrals += 1;
            return Ok(());
        }
        let view = self.sandbox.snapshot_for(actor_id, peer.radius_m)?;
        let message =
            peer.replication
                .sandbox(self.tick, view, u64::from(self.config.tick_hz) * 10)?;
        peer.connection.queue(&message)?;
        peer.sandbox_snapshot_due = false;
        Ok(())
    }
}

fn persistence_error(error: impl std::fmt::Display) -> NetError {
    NetError::Persistence(error.to_string())
}

fn ecology_ground(
    environment: &dyn WorldEnvironment,
    candidate: GeoPosition,
    obstacles: &[(GeoPosition, f64)],
) -> Option<GeoPosition> {
    let grounded = environment
        .resolve_movement(ActorKind::Animal, candidate, candidate)
        .filter(|p| validate_position(*p))?;
    if !environment.allows_placement(grounded, 0.4, true)
        || obstacles
            .iter()
            .any(|(p, r)| p.surface_distance_m(grounded) < r + 0.4)
    {
        return None;
    }
    Some(grounded)
}

fn find_safe_ground(
    environment: &dyn WorldEnvironment,
    position: GeoPosition,
    kind: ActorKind,
) -> Option<GeoPosition> {
    let frame = LocalFrame::new(position);
    for radius in [0., 3., 7., 15., 30., 60., 100.] {
        for angle in 0..if radius == 0. { 1 } else { 12 } {
            let theta = angle as f64 * std::f64::consts::TAU / 12.;
            // Preserve an already safe GPS anchor exactly. Even a zero ENU
            // roundtrip can introduce small coordinate drift on each resume.
            let candidate = if radius == 0. {
                position
            } else {
                frame
                    .to_geo([radius * theta.cos(), 0., radius * theta.sin()])
                    .ok()?
            };
            if let Some(position) = environment
                .resolve_movement(kind, candidate, candidate)
                .filter(|p| validate_position(*p))
            {
                return Some(position);
            }
        }
    }
    None
}

/// Apply the same bounded wall sliding used by the local reference controller.
/// A failed candidate never commits movement or completes a logistics arrival.
fn resolve_authoritative_motion(
    environment: &dyn WorldEnvironment,
    kind: ActorKind,
    old: GeoPosition,
    candidate: GeoPosition,
    footprints: &[(GeoPosition, f64)],
) -> Option<GeoPosition> {
    // Coverage boundaries are not obstacles to slide around. Honor a host's
    // explicit geographic admission rule before trying local wall steering.
    if !environment.can_enter(candidate) {
        return None;
    }
    let accept = |candidate| {
        if !validate_position(candidate)
            || !terra_sandbox::facility_path_clear(kind, old, candidate, footprints)
        {
            return None;
        }
        environment
            .resolve_movement(kind, old, candidate)
            .filter(|p| validate_position(*p))
    };
    if let Some(position) = accept(candidate) {
        return Some(position);
    }
    if !matches!(
        kind,
        ActorKind::Human | ActorKind::Animal | ActorKind::Creature | ActorKind::Vehicle
    ) {
        return None;
    }
    let frame = LocalFrame::new(old);
    let delta = frame.to_local(candidate);
    for trial in [
        [delta[0], 0., 0.],
        [0., 0., delta[2]],
        [delta[2] * 0.7, 0., -delta[0] * 0.7],
        [-delta[2] * 0.7, 0., delta[0] * 0.7],
    ] {
        if trial[0].hypot(trial[2]) < 0.0001 {
            continue;
        }
        if let Some(valid) = frame.to_geo(trial).ok().and_then(accept) {
            return Some(valid);
        }
    }
    None
}

fn resolve_vehicle_motion(
    environment: &dyn WorldEnvironment,
    old: GeoPosition,
    candidate: GeoPosition,
    old_heading: f32,
    heading: f32,
    dimensions: [f64; 3],
    footprints: &[(GeoPosition, f64)],
) -> Option<GeoPosition> {
    if !validate_position(old)
        || !validate_position(candidate)
        || !environment.can_enter(old)
        || !environment.can_enter(candidate)
    {
        return None;
    }
    if !terra_sandbox::facility_path_clear(ActorKind::Vehicle, old, candidate, footprints) {
        return None;
    }
    let frame = LocalFrame::new(old);
    let delta = frame.to_local(candidate);
    let angle = (f64::from(heading - old_heading) + 540.).rem_euclid(360.) - 180.;
    let corner_radius = dimensions[0].hypot(dimensions[2]) * 0.5;
    let required = ((old.surface_distance_m(candidate) + corner_radius * angle.abs().to_radians())
        / 0.25)
        .ceil() as usize;
    if required > 512 {
        return None;
    }
    let count = required.max(1);
    let mut previous =
        terra_core::vehicle_support_points(old, f64::from(old_heading), dimensions).ok()?;
    let mut result = old;
    for n in 1..=count {
        let point = frame
            .to_geo(delta.map(|x| x * n as f64 / count as f64))
            .ok()?;
        let points = terra_core::vehicle_support_points(
            point,
            f64::from(old_heading) + angle * n as f64 / count as f64,
            dimensions,
        )
        .ok()?;
        for (&from, &p) in previous.iter().zip(&points) {
            if !validate_position(p)
                || !environment.can_enter(p)
                || !terra_sandbox::facility_path_clear(ActorKind::Human, from, p, footprints)
            {
                return None;
            }
            let support = environment.vehicle_support_with_clearance(p, dimensions[1])?;
            if !validate_position(support) || !environment.can_enter(support) {
                return None;
            }
        }
        previous = points;
        result = environment.vehicle_support_with_clearance(point, dimensions[1])?;
        if !validate_position(result) || !environment.can_enter(result) {
            return None;
        }
    }
    Some(result)
}

fn valid_token(token: &str) -> bool {
    token.len() == 64 && token.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn default_starting_inventory() -> Inventory {
    [("wood", 30), ("stone", 20), ("wheat_seed", 8)]
        .into_iter()
        .map(|(id, count)| (id.to_owned(), count))
        .collect()
}

fn validate_starting_inventory(catalog: &Catalog, inventory: &Inventory) -> Result<()> {
    if inventory.len() > terra_sandbox::MAX_INVENTORY_ITEMS
        || inventory.iter().any(|(id, count)| {
            !catalog.items.contains(id) || *count > terra_sandbox::MAX_ITEM_COUNT
        })
    {
        return Err(NetError::Configuration("starter inventory must use bounded items from the active catalog; set catalog and inventory together for a custom game".into()));
    }
    Ok(())
}

fn fresh_token() -> Result<String> {
    let mut entropy = [0_u8; 32];
    getrandom::fill(&mut entropy)
        .map_err(|error| NetError::Protocol(format!("OS randomness unavailable: {error}")))?;
    Ok(entropy.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn is_airborne(kind: &ActorKind) -> bool {
    matches!(
        kind,
        ActorKind::Helicopter | ActorKind::Balloon | ActorKind::Airplane
    )
}

fn speed_limit(kind: &ActorKind) -> f64 {
    match kind {
        ActorKind::Human => 12.0,
        ActorKind::Animal => 30.0,
        ActorKind::Creature => 50.0,
        ActorKind::Vehicle => 80.0,
        ActorKind::Ship => 35.0,
        ActorKind::Helicopter => 120.0,
        ActorKind::Balloon => 25.0,
        ActorKind::Airplane => 350.0,
    }
}

#[cfg(test)]
#[path = "server_backpressure_tests.rs"]
mod backpressure_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sandbox_walking_dispatch_uses_coverage_before_shortening_a_route() {
        struct WalkingSurface {
            origin: GeoPosition,
            radius_m: f64,
        }
        impl WorldEnvironment for WalkingSurface {
            fn ground_height_m(&self, _: GeoPosition) -> Option<f64> {
                Some(100.)
            }
            fn can_enter(&self, point: GeoPosition) -> bool {
                self.origin.surface_distance_m(point) < self.radius_m
            }
        }
        let mut server = Server::bind(ServerConfig {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            ..Default::default()
        })
        .unwrap();
        server.enable_living_demo(&[]).unwrap();
        let _socket = std::net::TcpStream::connect(server.local_addr().unwrap()).unwrap();
        server.accept_connections();
        let mut peer = server.peers.remove(&1).unwrap();
        server
            .handle_message(
                1,
                &mut peer,
                ClientMessage::Hello {
                    protocol: PROTOCOL_VERSION,
                    interest: server.config.spawn,
                    resume_token: None,
                    expected_manifest: None,
                },
            )
            .unwrap();
        let player = peer.actor_id.unwrap();
        let avatar = server.sandbox.living.accounts[&player].avatar_unit;
        let mut origin = LocalFrame::new(server.config.spawn)
            .to_geo([2_000., 0., 2_000.])
            .unwrap();
        origin.altitude_m = 100.;
        let frame = LocalFrame::new(origin);
        let destination = frame.to_geo([50., 0., 0.]).unwrap();
        server
            .sandbox
            .living
            .units
            .get_mut(&avatar)
            .unwrap()
            .position = origin;
        server.sandbox.players.get_mut(&player).unwrap().position = origin;
        server.actors.get_mut(&player).unwrap().position = origin;
        server.sandbox.living.roads = vec![terra_sandbox::RoadRoute {
            id: "test:network-walking-perimeter".into(),
            points: [
                [-100., -80.],
                [100., -80.],
                [100., 100.],
                [-100., 100.],
                [-100., -80.],
            ]
            .into_iter()
            .map(|p| frame.to_geo([p[0], 0., p[1]]).unwrap())
            .collect(),
            width_m: 6.,
            rules: terra_sandbox::RoadRules::default(),
        }];
        let order = |sequence| ClientMessage::Sandbox {
            sequence,
            command: terra_sandbox::Command::Living {
                action: terra_sandbox::LivingCommand::MoveUnits {
                    unit_ids: vec![avatar],
                    destination,
                },
            },
        };
        server.set_environment(Box::new(WalkingSurface {
            origin,
            radius_m: 0.1,
        }));
        server.handle_message(1, &mut peer, order(1)).unwrap();
        assert!(server.sandbox.living.units[&avatar].path.len() > 1);
        assert_eq!(server.sandbox.living.units[&avatar].position, origin);
        server.set_environment(Box::new(WalkingSurface {
            origin,
            radius_m: 250.,
        }));
        server.handle_message(1, &mut peer, order(2)).unwrap();
        let unit = &server.sandbox.living.units[&avatar];
        assert_eq!(
            unit.position, origin,
            "the authority plans without moving the body"
        );
        assert_eq!(unit.path.len(), 1);
        assert!(unit.path[0].surface_distance_m(destination) < 1e-6);
        assert_eq!(unit.path[0].altitude_m, 100.);
        assert_eq!(server.actors[&player].position, origin);
        server
            .sandbox
            .validate_with_catalog(&server.catalog)
            .unwrap();
    }

    #[test]
    fn sandbox_exit_dispatch_rejects_uncovered_dismount_then_places_clear() {
        struct ExitSurface {
            origin: GeoPosition,
            radius_m: f64,
        }
        impl WorldEnvironment for ExitSurface {
            fn ground_height_m(&self, _: GeoPosition) -> Option<f64> {
                Some(self.origin.altitude_m)
            }
            fn can_enter(&self, point: GeoPosition) -> bool {
                self.origin.surface_distance_m(point) < self.radius_m
            }
        }
        let mut server = Server::bind(ServerConfig {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            ..Default::default()
        })
        .unwrap();
        server.enable_living_demo(&[]).unwrap();
        let _socket = std::net::TcpStream::connect(server.local_addr().unwrap()).unwrap();
        server.accept_connections();
        let mut peer = server.peers.remove(&1).unwrap();
        server
            .handle_message(
                1,
                &mut peer,
                ClientMessage::Hello {
                    protocol: PROTOCOL_VERSION,
                    interest: server.config.spawn,
                    resume_token: None,
                    expected_manifest: None,
                },
            )
            .unwrap();
        let player = peer.actor_id.unwrap();
        let vehicle = server
            .sandbox
            .transport
            .vehicles
            .values()
            .find(|v| v.owner == player)
            .unwrap()
            .unit_id;
        let mut origin = LocalFrame::new(server.config.spawn)
            .to_geo([2_000., 0., 2_000.])
            .unwrap();
        origin.altitude_m = 100.;
        server
            .sandbox
            .living
            .units
            .get_mut(&vehicle)
            .unwrap()
            .position = origin;
        server.sandbox.set_player_position(player, origin).unwrap();
        server
            .sandbox
            .apply(
                player,
                terra_sandbox::Command::Transport {
                    action: terra_sandbox::TransportCommand::EnterVehicle {
                        vehicle_id: vehicle,
                    },
                },
                &server.catalog,
            )
            .unwrap();
        let actor = server.actors.get_mut(&player).unwrap();
        actor.kind = ActorKind::Vehicle;
        actor.position = origin;
        actor.velocity_enu_m_s = [0.; 3];
        let before = server.sandbox.clone();
        server.set_environment(Box::new(ExitSurface {
            origin,
            radius_m: 0.4,
        }));
        let exit = |sequence| ClientMessage::Sandbox {
            sequence,
            command: terra_sandbox::Command::Transport {
                action: terra_sandbox::TransportCommand::ExitVehicle,
            },
        };
        server.handle_message(1, &mut peer, exit(1)).unwrap();
        assert_eq!(
            server.sandbox, before,
            "a center-only source patch cannot authorize dismount"
        );
        assert_eq!(server.actors[&player].kind, ActorKind::Vehicle);
        server.set_environment(Box::new(ExitSurface {
            origin,
            radius_m: 20.,
        }));
        server.handle_message(1, &mut peer, exit(2)).unwrap();
        let account = &server.sandbox.living.accounts[&player];
        let avatar = &server.sandbox.living.units[&account.avatar_unit];
        assert_eq!(account.active_unit, account.avatar_unit);
        assert_eq!(server.sandbox.spawn_obstruction(avatar).unwrap(), None);
        assert_eq!(server.actors[&player].kind, ActorKind::Human);
        assert_eq!(server.actors[&player].position, avatar.position);
        assert!(server.sandbox.transport.vehicles[&vehicle].parking_brake);
        server
            .sandbox
            .validate_with_catalog(&server.catalog)
            .unwrap();

        let garage = server
            .sandbox
            .transport
            .facilities
            .values()
            .find(|f| f.kind == terra_sandbox::TransportFacilityKind::Garage)
            .unwrap()
            .id;
        server
            .sandbox
            .transport
            .facilities
            .get_mut(&garage)
            .unwrap()
            .position = origin;
        let before_delivery = server.sandbox.clone();
        let rent = |sequence| ClientMessage::Sandbox {
            sequence,
            command: terra_sandbox::Command::Transport {
                action: terra_sandbox::TransportCommand::RentVehicle {
                    model: terra_sandbox::VehicleModel::Compact,
                    facility_id: garage,
                },
            },
        };
        server.set_environment(Box::new(ExitSurface {
            origin,
            radius_m: 0.4,
        }));
        server.handle_message(1, &mut peer, rent(3)).unwrap();
        assert_eq!(server.sandbox, before_delivery);
        server.set_environment(Box::new(ExitSurface {
            origin,
            radius_m: 20.,
        }));
        server.handle_message(1, &mut peer, rent(4)).unwrap();
        let rental = server
            .sandbox
            .transport
            .vehicles
            .keys()
            .find(|id| !before_delivery.transport.vehicles.contains_key(id))
            .copied()
            .unwrap();
        assert_eq!(
            server
                .sandbox
                .spawn_obstruction(&server.sandbox.living.units[&rental])
                .unwrap(),
            None
        );
        assert_eq!(
            server.sandbox.living.units[&rental].position.altitude_m,
            100.
        );
        server
            .sandbox
            .validate_with_catalog(&server.catalog)
            .unwrap();
    }

    /// A provider can implement point support and geographic coverage as
    /// separate concerns. The authority must still honor both contracts.
    struct CustomVehicleSupport {
        origin: GeoPosition,
        east_edge_longitude: f64,
        corner_return: Option<GeoPosition>,
    }

    impl CustomVehicleSupport {
        fn new(origin: GeoPosition, east_edge_m: f64, corner_return: Option<GeoPosition>) -> Self {
            Self {
                origin,
                east_edge_longitude: LocalFrame::new(origin)
                    .to_geo([east_edge_m, 0., 0.])
                    .unwrap()
                    .longitude,
                corner_return,
            }
        }
    }

    impl WorldEnvironment for CustomVehicleSupport {
        fn ground_height_m(&self, _: GeoPosition) -> Option<f64> {
            Some(self.origin.altitude_m)
        }

        fn can_enter(&self, position: GeoPosition) -> bool {
            // Deliberately separate from support and altitude validation.
            position.longitude <= self.east_edge_longitude
        }

        fn vehicle_support(&self, position: GeoPosition) -> Option<GeoPosition> {
            let east_m = LocalFrame::new(self.origin).to_local(position)[0];
            if east_m.abs() > 0.5 {
                if let Some(returned) = self.corner_return {
                    return Some(returned);
                }
            }
            Some(position)
        }
    }

    #[test]
    fn vehicle_envelope_respects_coverage_with_custom_point_support() {
        let origin = ServerConfig::default().spawn;
        let candidate = LocalFrame::new(origin).to_geo([3., 0., 0.]).unwrap();
        let dimensions = [2., 2., 4.];
        let covered = CustomVehicleSupport::new(origin, 100., None);
        assert!(
            resolve_vehicle_motion(&covered, origin, candidate, 0., 0., dimensions, &[]).is_some()
        );

        for (east_edge, center_inside) in [(2., false), (3.5, true)] {
            let environment = CustomVehicleSupport::new(origin, east_edge, None);
            assert_eq!(environment.can_enter(candidate), center_inside);
            // The provider's support override deliberately accepts this point.
            // Coverage must reject both a crossing center and an uncovered
            // model corner when the center itself remains inside the world.
            assert_eq!(environment.vehicle_support(candidate), Some(candidate));
            assert!(
                resolve_vehicle_motion(&environment, origin, candidate, 0., 0., dimensions, &[])
                    .is_none(),
                "vehicle crossed the {east_edge} m coverage boundary"
            );
        }
    }

    #[test]
    fn vehicle_envelope_rejects_invalid_or_uncovered_returned_corner_support() {
        let origin = ServerConfig::default().spawn;
        let frame = LocalFrame::new(origin);
        let candidate = frame.to_geo([0., 0., -2.]).unwrap();
        let dimensions = [2., 2., 4.];
        let covered = CustomVehicleSupport::new(origin, 10., None);
        assert!(
            resolve_vehicle_motion(&covered, origin, candidate, 0., 0., dimensions, &[]).is_some()
        );

        let bad_altitude = |altitude_m| GeoPosition {
            altitude_m,
            ..origin
        };
        for (label, returned) in [
            ("NaN altitude", bad_altitude(f64::NAN)),
            ("infinite altitude", bad_altitude(f64::INFINITY)),
            ("finite out-of-range altitude", bad_altitude(100_001.)),
            (
                "outside host coverage",
                frame.to_geo([20., 0., 0.]).unwrap(),
            ),
        ] {
            let environment = CustomVehicleSupport::new(origin, 10., Some(returned));
            assert!(environment.can_enter(candidate));
            assert_eq!(environment.vehicle_support(candidate), Some(candidate));
            // Center support remains valid throughout this northbound move;
            // only the provider's returned corner support is defective.
            assert!(
                resolve_vehicle_motion(&environment, origin, candidate, 0., 0., dimensions, &[])
                    .is_none(),
                "admitted {label} from a corner support query"
            );
        }
    }

    #[test]
    fn vehicle_envelope_missing_coverage_stops_without_collision_wear() {
        #[derive(Clone, Copy, Debug)]
        enum CornerFailure {
            Missing,
            InvalidHeight(f64),
            Forbidden,
            Obstacle,
        }

        struct BoundaryEnvironment {
            origin: GeoPosition,
            failure: CornerFailure,
        }

        impl BoundaryEnvironment {
            fn corner(&self, position: GeoPosition) -> bool {
                LocalFrame::new(self.origin).to_local(position)[0].abs() > 0.5
            }
        }

        impl WorldEnvironment for BoundaryEnvironment {
            fn ground_height_m(&self, position: GeoPosition) -> Option<f64> {
                if self.corner(position) {
                    match self.failure {
                        CornerFailure::Missing => return None,
                        CornerFailure::InvalidHeight(height) => return Some(height),
                        _ => {}
                    }
                }
                Some(self.origin.altitude_m)
            }

            fn can_enter(&self, position: GeoPosition) -> bool {
                !(self.corner(position) && matches!(self.failure, CornerFailure::Forbidden))
            }

            fn vehicle_support(&self, position: GeoPosition) -> Option<GeoPosition> {
                if self.corner(position) && matches!(self.failure, CornerFailure::Obstacle) {
                    None
                } else {
                    self.resolve_movement(ActorKind::Vehicle, position, position)
                }
            }
        }

        for failure in [
            CornerFailure::Missing,
            CornerFailure::InvalidHeight(f64::NAN),
            CornerFailure::InvalidHeight(f64::INFINITY),
            CornerFailure::InvalidHeight(100_001.),
            CornerFailure::Forbidden,
            CornerFailure::Obstacle,
        ] {
            let mut server = Server::bind(ServerConfig {
                bind_addr: "127.0.0.1:0".parse().unwrap(),
                calendar_time_scale: 0.,
                ..Default::default()
            })
            .unwrap();
            server.sandbox.add_player(1, server.config.spawn).unwrap();
            server.enable_living_demo(&[]).unwrap();
            let vehicle_id = server
                .sandbox
                .transport
                .vehicles
                .values()
                .find(|vehicle| vehicle.owner == 1)
                .unwrap()
                .unit_id;
            let account = server.sandbox.living.accounts.get_mut(&1).unwrap();
            account.active_unit = vehicle_id;
            account.possessed = true;
            // Keep authored facilities away from this isolated support test.
            let mut origin = LocalFrame::new(server.config.spawn)
                .to_geo([2_000., 0., 2_000.])
                .unwrap();
            origin.altitude_m = 0.;
            server
                .sandbox
                .set_controlled_motion(1, origin, [0.; 3], 0.)
                .unwrap();
            let vehicle = server
                .sandbox
                .transport
                .vehicles
                .get_mut(&vehicle_id)
                .unwrap();
            vehicle.dynamics.speed_m_s = 5.;
            let condition = vehicle.condition;
            // Install the admitted principal as Hello does. insert_actor is
            // intentionally reserved for independent host-created NPC records.
            server.actors.insert(
                1,
                ActorState {
                    id: 1,
                    kind: ActorKind::Vehicle,
                    position: origin,
                    velocity_enu_m_s: [0., 5., 0.],
                    heading_deg: 0.,
                },
            );
            server.set_environment(Box::new(BoundaryEnvironment { origin, failure }));

            assert_eq!(server.fixed_step(0.05), 0, "{failure:?}");
            let vehicle = &server.sandbox.transport.vehicles[&vehicle_id];
            assert_eq!(server.actors[&1].position, origin, "{failure:?}");
            assert_eq!(vehicle.dynamics.speed_m_s, 0., "{failure:?}");
            assert_eq!(vehicle.odometer_m, 0., "blocked motion added mileage");
            if matches!(failure, CornerFailure::Obstacle) {
                assert!(
                    vehicle.condition < condition,
                    "known collision lost its damage"
                );
            } else {
                assert_eq!(
                    vehicle.condition, condition,
                    "{failure:?} charged collision wear"
                );
            }
        }
    }

    #[test]
    fn invalid_configuration_is_reported_before_binding() {
        let mut config = ServerConfig::default();
        config.tick_hz = 0;
        assert!(matches!(
            Server::bind(config),
            Err(NetError::Configuration(_))
        ));
    }

    #[test]
    fn authoritative_manual_vehicle_stops_at_dynamic_body_but_not_an_overpass() {
        for overhead in [false, true] {
            let mut server = Server::bind(ServerConfig {
                bind_addr: "127.0.0.1:0".parse().unwrap(),
                calendar_time_scale: 0.,
                ..Default::default()
            })
            .unwrap();
            server.sandbox.add_player(1, server.config.spawn).unwrap();
            server.enable_living_demo(&[]).unwrap();
            let vehicle_id = server
                .sandbox
                .transport
                .vehicles
                .values()
                .find(|v| v.owner == 1)
                .unwrap()
                .unit_id;
            let account = server.sandbox.living.accounts.get_mut(&1).unwrap();
            account.active_unit = vehicle_id;
            account.possessed = true;
            let mut origin = LocalFrame::new(server.config.spawn)
                .to_geo([2_000., 0., 2_000.])
                .unwrap();
            origin.altitude_m = 0.025;
            server
                .sandbox
                .set_controlled_motion(1, origin, [0., 10., 0.], 0.)
                .unwrap();
            server
                .sandbox
                .transport
                .vehicles
                .get_mut(&vehicle_id)
                .unwrap()
                .dynamics
                .speed_m_s = 10.;
            let mut pedestrian = server
                .sandbox
                .living
                .units
                .values()
                .find(|u| u.owner.is_none() && u.kind == ActorKind::Human)
                .unwrap()
                .clone();
            pedestrian.position = LocalFrame::new(origin)
                .to_geo([0., if overhead { 5. } else { 0. }, -4.])
                .unwrap();
            pedestrian.velocity_enu_m_s = [0.; 3];
            pedestrian.order = terra_sandbox::UnitOrder::Idle;
            pedestrian.patrol.clear();
            server
                .sandbox
                .living
                .units
                .insert(pedestrian.id, pedestrian);
            server.actors.insert(
                1,
                ActorState {
                    id: 1,
                    kind: ActorKind::Vehicle,
                    position: origin,
                    velocity_enu_m_s: [0., 10., 0.],
                    heading_deg: 0.,
                },
            );
            assert_eq!(server.fixed_step(0.25), 0);
            let distance = origin.surface_distance_m(server.actors[&1].position);
            if overhead {
                assert!(
                    distance > 1.7,
                    "overhead body blocked road movement: {distance}"
                );
                assert!(server.sandbox.transport.vehicles[&vehicle_id].condition > 0.99);
            } else {
                assert!(
                    (0.5..1.5).contains(&distance),
                    "did not stop at body footprint: {distance}"
                );
                assert_eq!(server.actors[&1].velocity_enu_m_s, [0.; 3]);
                assert!(server.sandbox.transport.vehicles[&vehicle_id].condition < 0.99);
            }
        }
    }

    #[test]
    fn fixed_step_catches_up_with_a_strict_budget() {
        let mut server = Server::bind(ServerConfig {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            tick_hz: 120,
            ..ServerConfig::default()
        })
        .unwrap();
        let report = server.update(10.0);
        assert_eq!(report.fixed_steps, MAX_STEPS_PER_UPDATE);
        assert!(report.dropped_seconds > 9.9);
        assert_eq!(server.tick(), MAX_STEPS_PER_UPDATE as u64);
    }

    #[test]
    fn actor_budget_and_invalid_values_are_enforced() {
        let mut server = Server::bind(ServerConfig {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            max_actors: 1,
            max_clients: 1,
            ..ServerConfig::default()
        })
        .unwrap();
        let actor = ActorState {
            id: 10,
            kind: ActorKind::Animal,
            position: server.config.spawn,
            velocity_enu_m_s: [0.0; 3],
            heading_deg: 0.0,
        };
        server.insert_actor(actor.clone()).unwrap();
        let mut second = actor;
        second.id = 11;
        assert!(server.insert_actor(second.clone()).is_err());
        second.position.latitude = f64::NAN;
        assert!(server.insert_actor(second).is_err());
    }
    #[test]
    fn living_registration_rejection_leaves_no_player_account_or_session() {
        for exhausted_ids in [false, true] {
            let mut server = Server::bind(ServerConfig {
                bind_addr: "127.0.0.1:0".parse().unwrap(),
                ..ServerConfig::default()
            })
            .unwrap();
            server.enable_living_demo(&[]).unwrap();
            if exhausted_ids {
                server.sandbox.living.next_id = u64::MAX - 2;
            } else {
                let template = server
                    .sandbox
                    .living
                    .units
                    .values()
                    .find(|u| u.owner.is_none())
                    .unwrap()
                    .clone();
                while server.sandbox.living.units.len()
                    <= terra_sandbox::MAX_LIVING_UNITS - terra_sandbox::STARTER_UNIT_COUNT
                {
                    let mut unit = template.clone();
                    unit.id = server.sandbox.living.next_id;
                    server.sandbox.living.next_id += 1;
                    server.sandbox.living.units.insert(unit.id, unit);
                }
            }
            let before = serde_json::to_value(&server.sandbox).unwrap();
            let _socket = std::net::TcpStream::connect(server.local_addr().unwrap()).unwrap();
            server.accept_connections();
            let mut peer = server.peers.remove(&1).unwrap();
            server
                .handle_message(
                    1,
                    &mut peer,
                    ClientMessage::Hello {
                        protocol: PROTOCOL_VERSION,
                        interest: server.config.spawn,
                        resume_token: None,
                        expected_manifest: None,
                    },
                )
                .unwrap();
            assert!(peer.actor_id.is_none());
            assert!(server.sessions.is_empty());
            assert!(server.actors.is_empty());
            assert_eq!(server.next_actor_id, 1);
            assert_eq!(serde_json::to_value(&server.sandbox).unwrap(), before);
        }
    }

    #[test]
    fn constructed_structure_blocks_fast_authoritative_actor_motion() {
        let mut server = Server::bind(ServerConfig {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            ..ServerConfig::default()
        })
        .unwrap();
        let origin = server.config.spawn;
        let frame = LocalFrame::new(origin);
        server.sandbox.structures.insert(
            1,
            terra_sandbox::Structure {
                id: 1,
                owner: 1,
                faction_owner: None,
                blueprint: "cabin".into(),
                position: frame.to_geo([4., 0., 0.]).unwrap(),
                hitpoints: 100,
                max_hitpoints: 100,
                radius_m: 2.,
                housing: true,
                workbench: false,
            },
        );
        server
            .insert_actor(ActorState {
                id: 99,
                kind: ActorKind::Vehicle,
                position: origin,
                velocity_enu_m_s: [80., 0., 0.],
                heading_deg: 90.,
            })
            .unwrap();
        server.fixed_step(0.1);
        let position = server.actors[&99].position;
        assert!(frame.to_local(position)[0] < 1.);
        assert!(terra_sandbox::facility_path_clear(
            ActorKind::Vehicle,
            origin,
            position,
            &[(frame.to_geo([4., 0., 0.]).unwrap(), 2.)]
        ));
    }
    #[test]
    fn model_sized_vehicle_sweep_honors_source_underpass_clearance_and_legacy_provider() {
        let origin = GeoPosition::new(48.137, 11.575, 100.025).unwrap();
        let frame = LocalFrame::new(origin);
        let key = TileKey::at(origin, 15).unwrap();
        let points = [[-6., 0., -8.], [6., 0., -8.], [6., 0., 8.], [-6., 0., 8.]]
            .map(|offset| {
                let mut point = frame.to_geo(offset).unwrap();
                point.altitude_m = 0.;
                point
            })
            .to_vec();
        let chunk = WorldChunk {
            key,
            heights: terra_core::HeightField::constant(100.),
            features: vec![terra_core::WorldFeature {
                id: "overhead-part".into(),
                kind: terra_core::FeatureKind::Building,
                points,
                height_m: 8.,
                properties: [("min_height".into(), "2.4".into())].into(),
            }],
            sources: vec![],
            warnings: vec![],
        };
        let mut surface = crate::ResidentSurface::new(15);
        surface.register_chunk(&chunk);
        let end = frame.to_geo([2., 0., 0.]).unwrap();
        assert!(
            resolve_vehicle_motion(&surface, origin, end, 90., 90., [1.8, 1.55, 3.8], &[])
                .is_some()
        );
        assert!(
            resolve_vehicle_motion(&surface, origin, end, 90., 90., [2.5, 3.25, 8.], &[]).is_none()
        );
        let legacy = CustomVehicleSupport::new(origin, 100., None);
        assert!(
            resolve_vehicle_motion(&legacy, origin, end, 90., 90., [1.8, 1.55, 3.8], &[]).is_some()
        );
    }
    #[test]
    fn vehicle_source_sweep_cannot_jump_a_thin_wall_between_old_one_metre_samples() {
        let origin = GeoPosition::new(48.137, 11.575, 100.025).unwrap();
        let frame = LocalFrame::new(origin);
        let key = TileKey::at(origin, 15).unwrap();
        let chunk = WorldChunk {
            key,
            heights: terra_core::HeightField::constant(100.),
            features: vec![terra_core::WorldFeature {
                id: "thin-wall".into(),
                kind: terra_core::FeatureKind::Building,
                points: [
                    [2.52, 0., -8.],
                    [2.54, 0., -8.],
                    [2.54, 0., 8.],
                    [2.52, 0., 8.],
                ]
                .map(|p| frame.to_geo(p).unwrap())
                .to_vec(),
                height_m: 4.,
                properties: Default::default(),
            }],
            sources: vec![],
            warnings: vec![],
        };
        let mut surface = crate::ResidentSurface::new(15);
        surface.register_chunk(&chunk);
        let dimensions = [2., 2., 4.];
        assert!(
            terra_core::vehicle_support_points(origin, 90., dimensions)
                .unwrap()
                .into_iter()
                .all(|p| surface.vehicle_support_with_clearance(p, 2.).is_some())
        );
        let end = frame.to_geo([3., 0., 0.]).unwrap();
        assert!(resolve_vehicle_motion(&surface, origin, end, 90., 90., dimensions, &[]).is_none());
    }
}
