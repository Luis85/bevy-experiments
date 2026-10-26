use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Read, Write},
    net::{SocketAddr, TcpListener},
    path::Path,
};

use serde::{Deserialize, Serialize};
use terra_core::{
    ActorKind, ActorState, GeoPosition, LocalFrame, TileKey, WorldChunk, WorldClock,
    synthetic_weather,
};
use terra_sandbox::{Catalog, Inventory, SandboxState};

use crate::{
    BaseWorldManifest, ClientMessage, NetError, PROTOCOL_VERSION, Result, ServerMessage,
    protocol::{surface_distance_m, validate_position},
    transport::Connection,
};

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
    actor_id: Option<u64>,
    interest: GeoPosition,
    radius_m: f64,
    last_sequence: Option<u64>,
    created_at: f64,
    last_activity: f64,
    last_movement: f64,
    tokens: f64,
    sandbox_tokens: f64,
    strikes: u8,
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
    sandbox_sequences: BTreeMap<u64, u64>,
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
    sandbox_sequences: BTreeMap<u64, u64>,
    nonplayer_actors: BTreeMap<u64, ActorState>,
    clock: WorldClock,
    tick: u64,
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
            sandbox_sequences: BTreeMap::new(),
        })
    }

    pub fn local_addr(&self) -> std::io::Result<SocketAddr> {
        self.listener.local_addr()
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
    pub fn sandbox(&self) -> &SandboxState {
        &self.sandbox
    }

    /// Explicit opt-in for an authored economy/population in the hosted world.
    pub fn enable_living_demo(&mut self, features: &[terra_core::WorldFeature]) -> Result<()> {
        // A v0.1 built-in catalog is extended only when it exactly matches the
        // prior built-in rules; custom catalogs are never silently rewritten.
        if !self.catalog.items.contains("bread") {
            let mut legacy = Catalog::builtin();
            for item in ["bread", "water", "flour"] {
                legacy.items.remove(item);
            }
            legacy.recipes.remove("bread");
            legacy.recipes.remove("flour");
            if serde_json::to_value(&legacy)? == serde_json::to_value(&self.catalog)? {
                self.catalog = Catalog::builtin();
            }
        }
        self.sandbox
            .seed_living_demo(self.config.spawn, features, &self.catalog)?;
        self.ground_living_positions(None);
        Ok(())
    }

    fn ground_living_positions(&mut self, only_player: Option<u64>) {
        let environment = self.environment.as_ref();
        let mut moved_facilities = Vec::new();
        if only_player.is_none() {
            for shop in self.sandbox.living.shops.values_mut() {
                if let Some(position) =
                    find_safe_ground(environment, shop.position, ActorKind::Human)
                {
                    moved_facilities.push((shop.position, position));
                    shop.position = position;
                }
            }
            for site in self.sandbox.living.producers.values_mut() {
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
        for unit in self
            .sandbox
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
            for road in &mut self.sandbox.living.roads {
                for point in &mut road.points {
                    *point = remap(*point);
                }
            }
        }
        for account in self.sandbox.living.accounts.values() {
            if let (Some(unit), Some(player)) = (
                self.sandbox.living.units.get(&account.active_unit),
                self.sandbox.players.get_mut(&account.player_id),
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
        self.sandbox.validate_with_catalog(&catalog)?;
        validate_starting_inventory(&catalog, &inventory)?;
        self.catalog = catalog;
        self.starting_inventory = inventory;
        Ok(())
    }

    /// Feed normalized resident chunks from the hosting world's streaming layer.
    /// This registers resources without changing the immutable source chunk.
    pub fn register_chunk(&mut self, chunk: &WorldChunk) {
        self.environment.register_chunk(chunk);
        self.sandbox.register_chunk(chunk);
        // The domain's default tree-to-wood mapping is optional for games whose
        // catalog uses different resources. Metadata remains available.
        self.sandbox
            .resource_nodes
            .retain(|_, node| self.catalog.items.contains(&node.item));
    }

    /// Drop only physical source geometry; persisted gathered-resource state
    /// remains in the domain and is not regenerated when a tile returns.
    pub fn evict_chunk(&mut self, key: TileKey) {
        self.environment.evict_chunk(key);
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
        self.sandbox
            .seed_resource(id, position, item, amount, regrow_seconds)?;
        Ok(())
    }

    /// Persist sandbox changes, player resume capabilities, clock and NPCs.
    /// The temporary file is fsynced then atomically replaces the destination.
    pub fn save_world(&self, path: impl AsRef<Path>) -> Result<()> {
        self.sandbox.validate_with_catalog(&self.catalog)?;
        validate_starting_inventory(&self.catalog, &self.starting_inventory)?;
        let saved = SavedServer {
            schema_version: 1,
            manifest: self.config.manifest.clone(),
            sandbox: self.sandbox.clone(),
            catalog: self.catalog.clone(),
            starting_inventory: self.starting_inventory.clone(),
            sessions: self.sessions.clone(),
            sandbox_sequences: self.sandbox_sequences.clone(),
            nonplayer_actors: self
                .actors
                .iter()
                .filter(|(id, _)| !self.sandbox.players.contains_key(id))
                .map(|(id, actor)| (*id, actor.clone()))
                .collect(),
            clock: self.clock,
            tick: self.tick,
        };
        let bytes = serde_json::to_vec_pretty(&saved)?;
        if bytes.len() as u64 > MAX_SAVE_BYTES {
            return Err(NetError::Protocol(
                "server save exceeds 32 MiB budget".into(),
            ));
        }
        let path = path.as_ref();
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        std::fs::create_dir_all(parent)?;
        let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
        temporary.write_all(&bytes)?;
        temporary.as_file().sync_all()?;
        temporary
            .persist(path)
            .map_err(|error| NetError::Io(error.error))?;
        Ok(())
    }

    /// Load only before accepting players. Every domain and session reference is
    /// validated before replacing live state, so rejected files are atomic.
    pub fn load_world(&mut self, path: impl AsRef<Path>) -> Result<()> {
        if !self.peers.is_empty() {
            return Err(NetError::Protocol(
                "load world before connecting clients".into(),
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
        if saved.schema_version != 1 || saved.manifest != self.config.manifest {
            return Err(NetError::Protocol(
                "save schema or base-world manifest mismatch".into(),
            ));
        }
        saved.sandbox.validate_with_catalog(&saved.catalog)?;
        validate_starting_inventory(&saved.catalog, &saved.starting_inventory)?;
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
        self.sandbox = saved.sandbox;
        self.catalog = saved.catalog;
        self.starting_inventory = saved.starting_inventory;
        self.sessions = saved.sessions;
        self.sandbox_sequences = saved.sandbox_sequences;
        self.actors = saved.nonplayer_actors;
        self.clock = saved.clock;
        self.tick = saved.tick;
        self.next_actor_id = 1;
        self.accumulator = 0.0;
        self.snapshot_accumulator = 0.0;
        Ok(())
    }

    pub fn set_environment(&mut self, environment: Box<dyn WorldEnvironment>) {
        self.environment = environment;
    }

    /// Host-only insertion for NPCs/vehicles. Client ownership remains server-held.
    pub fn insert_actor(&mut self, actor: ActorState) -> Result<()> {
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
        self.accept_connections();
        let peer_ids: Vec<_> = self.peers.keys().copied().collect();
        for id in peer_ids {
            let Some(mut peer) = self.peers.remove(&id) else {
                continue;
            };
            peer.tokens = (peer.tokens + delta * MESSAGE_TOKENS_PER_SECOND).min(MESSAGE_BURST);
            peer.sandbox_tokens = (peer.sandbox_tokens + delta * 10.0).min(20.0);
            let mut keep = true;
            match peer
                .connection
                .receive::<ClientMessage>(MAX_MESSAGES_PER_UPDATE)
            {
                Ok(messages) => {
                    for message in messages {
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
                            break;
                        }
                    }
                }
                Err(_) => keep = false,
            }
            if peer.actor_id.is_none()
                && self.real_seconds - peer.created_at > HANDSHAKE_TIMEOUT_SECONDS
            {
                keep = false;
            }
            if self.real_seconds - peer.last_activity > IDLE_TIMEOUT_SECONDS || peer.strikes >= 3 {
                keep = false;
            }
            if peer.connection.flush().is_err() {
                keep = false;
            }
            if keep {
                self.peers.insert(id, peer);
            } else {
                if let Some(actor_id) = peer.actor_id {
                    self.disconnect_player(actor_id);
                }
                report.disconnected += 1;
            }
        }

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
                self.broadcast_snapshots(&mut report);
            }
        }
        if self.accumulator >= step {
            let retained = self.accumulator % step;
            report.dropped_seconds += self.accumulator - retained;
            self.accumulator = retained;
        }
        report.tick = self.tick;
        report.clients = self
            .peers
            .values()
            .filter(|peer| peer.actor_id.is_some())
            .count();
        report.actors = self.actors.len();
        report
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
            let Ok(connection) = Connection::new(stream) else {
                continue;
            };
            let id = self.next_client_id;
            self.next_client_id = self.next_client_id.saturating_add(1);
            self.peers.insert(
                id,
                Peer {
                    connection,
                    actor_id: None,
                    interest: self.config.spawn,
                    radius_m: self.config.max_interest_radius_m.min(5_000.0),
                    last_sequence: None,
                    created_at: self.real_seconds,
                    last_activity: self.real_seconds,
                    last_movement: self.real_seconds,
                    tokens: MESSAGE_BURST,
                    sandbox_tokens: 20.0,
                    strikes: 0,
                },
            );
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
                let (actor_id, spawn, token) = if let Some(token) = resume_token {
                    let Some(&actor_id) = self.sessions.get(&token).filter(|_| valid_token(&token))
                    else {
                        return Self::error(peer, "resume", "invalid resume capability");
                    };
                    if self
                        .peers
                        .values()
                        .any(|other| other.actor_id == Some(actor_id))
                    {
                        return Self::error(peer, "connected", "player is already connected");
                    }
                    let position = self
                        .sandbox
                        .players
                        .get(&actor_id)
                        .ok_or_else(|| NetError::Protocol("saved player is missing".into()))?
                        .position;
                    (actor_id, position, token)
                } else {
                    if self.sandbox.players.len() >= terra_sandbox::MAX_PLAYERS
                        || (self.sandbox.living.enabled
                            && self
                                .sandbox
                                .living
                                .units
                                .len()
                                .saturating_add(terra_sandbox::STARTER_UNIT_COUNT)
                                > terra_sandbox::MAX_LIVING_UNITS)
                    {
                        Self::error(
                            peer,
                            "capacity",
                            "saved player capacity reached; resume an existing player",
                        )?;
                        peer.strikes = 3;
                        return Ok(());
                    }
                    let mut actor_id = self.next_actor_id;
                    while self.actors.contains_key(&actor_id)
                        || self.sandbox.players.contains_key(&actor_id)
                    {
                        actor_id = actor_id
                            .checked_add(1)
                            .ok_or_else(|| NetError::Protocol("actor IDs exhausted".into()))?;
                    }
                    let following_actor_id = actor_id
                        .checked_add(1)
                        .ok_or_else(|| NetError::Protocol("actor IDs exhausted".into()))?;
                    let offset = (client_id.saturating_sub(1) % 32) as f64 * 3.0;
                    let mut spawn = LocalFrame::new(self.config.spawn)
                        .to_geo([offset, 0.0, 0.0])
                        .unwrap_or(self.config.spawn);
                    if !self.environment.can_enter(spawn) {
                        spawn = self.config.spawn;
                    }
                    if let Some(height) = self.environment.ground_height_m(spawn).filter(|height| {
                        height.is_finite() && (-12_000.0..=100_000.0).contains(height)
                    }) {
                        spawn.altitude_m = height;
                    }
                    if !validate_position(spawn) || !self.environment.can_enter(spawn) {
                        return Self::error(
                            peer,
                            "spawn",
                            "authoritative spawn has no available terrain",
                        );
                    }
                    let Some(safe_spawn) =
                        find_safe_ground(self.environment.as_ref(), spawn, ActorKind::Human)
                    else {
                        return Self::error(
                            peer,
                            "spawn",
                            "no loaded traversable ground is available near the spawn",
                        );
                    };
                    spawn = safe_spawn;
                    let token = fresh_token()?;
                    self.sandbox.add_player_with_inventory(
                        actor_id,
                        spawn,
                        self.starting_inventory.clone(),
                        &self.catalog,
                    )?;
                    if let Err(error) = self.sandbox.ensure_living_player(actor_id) {
                        // Crew creation is transactional. A rejected registration
                        // must not mint inventory or strand a saved player/token.
                        self.sandbox.players.remove(&actor_id);
                        return Self::error(peer, "capacity", &error.to_string());
                    }
                    self.next_actor_id = following_actor_id;
                    self.sessions.insert(token.clone(), actor_id);
                    (actor_id, spawn, token)
                };
                if let Err(error) = self.sandbox.ensure_living_player(actor_id) {
                    return Self::error(peer, "capacity", &error.to_string());
                }
                self.ground_living_positions(Some(actor_id));
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
                self.actors.insert(
                    actor_id,
                    ActorState {
                        id: actor_id,
                        kind: active_kind,
                        position: spawn,
                        velocity_enu_m_s: [0.0; 3],
                        heading_deg: 0.0,
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
                if speed > 0.001
                    && let Some(account) = self.sandbox.living.accounts.get_mut(&actor_id)
                {
                    account.possessed = true;
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
                let position = self
                    .actors
                    .get(&actor_id)
                    .ok_or_else(|| NetError::Protocol("owned actor is missing".into()))?
                    .position;
                self.sandbox.set_player_position(actor_id, position)?;
                let target = match &command {
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
                match self.sandbox.apply(actor_id, command, &self.catalog) {
                    Ok(outcome) => peer
                        .connection
                        .queue(&ServerMessage::SandboxResult { sequence, outcome })?,
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
            if self.real_seconds - peer.last_movement > INPUT_TIMEOUT_SECONDS
                && let Some(actor) = peer.actor_id.and_then(|id| self.actors.get_mut(&id))
            {
                actor.velocity_enu_m_s = [0.0; 3];
            }
        }
        let mut facility_footprints = self.sandbox.living.facility_footprints();
        facility_footprints.extend(
            self.sandbox
                .structures
                .values()
                .map(|s| (s.position, s.radius_m)),
        );
        for actor in self.actors.values_mut() {
            if self
                .sandbox
                .living
                .accounts
                .get(&actor.id)
                .is_some_and(|account| !account.possessed)
            {
                continue;
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
                actor.position = position;
            } else {
                actor.position = old_position;
                actor.velocity_enu_m_s = [0.0; 3];
            }
        }
        self.tick += 1;
        for peer in self.peers.values() {
            if let Some(actor_id) = peer.actor_id
                && let Some(actor) = self.actors.get(&actor_id)
            {
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
        let previous_time = self.clock.unix_seconds;
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
        if self
            .sandbox
            .advance_living_with_environment(
                real_dt,
                self.clock.unix_seconds - previous_time,
                &self.catalog,
                |kind, old, candidate| {
                    resolve_authoritative_motion(
                        environment.as_ref(),
                        kind,
                        old,
                        candidate,
                        &facility_footprints,
                    )
                },
            )
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
        let mut disconnected = Vec::new();
        for (client_id, peer) in &mut self.peers {
            let Some(owned_actor) = peer.actor_id else {
                continue;
            };
            let mut visible: Vec<(bool, f64, &ActorState)> = self
                .actors
                .values()
                .filter_map(|actor| {
                    let is_owned = actor.id == owned_actor;
                    let distance = surface_distance_m(peer.interest, actor.position);
                    (is_owned || distance <= peer.radius_m).then_some((!is_owned, distance, actor))
                })
                .collect();
            visible.sort_by(|a, b| {
                a.0.cmp(&b.0)
                    .then_with(|| a.1.total_cmp(&b.1))
                    .then_with(|| a.2.id.cmp(&b.2.id))
            });
            let count = visible.len();
            let actors = visible
                .into_iter()
                .take(self.config.max_snapshot_actors)
                .map(|(_, _, actor)| actor.clone())
                .collect();
            let snapshot = ServerMessage::Snapshot {
                tick: self.tick,
                unix_seconds: self.clock.unix_seconds,
                actors,
                truncated: count > self.config.max_snapshot_actors,
                total_in_interest: count,
            };
            if peer
                .connection
                .queue(&snapshot)
                .and_then(|()| peer.connection.flush())
                .is_err()
            {
                disconnected.push(*client_id);
            } else {
                report.snapshots_sent += 1;
            }
            let sandbox_period = if self.sandbox.living.enabled {
                self.config.tick_hz.div_ceil(5)
            } else {
                self.config.tick_hz
            };
            if self.tick.is_multiple_of(u64::from(sandbox_period))
                && let Ok(view) = self.sandbox.snapshot_for(owned_actor, peer.radius_m)
                && peer
                    .connection
                    .queue(&ServerMessage::SandboxSnapshot {
                        tick: self.tick,
                        view,
                    })
                    .and_then(|()| peer.connection.flush())
                    .is_err()
            {
                disconnected.push(*client_id);
            }
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
        if let Some(account) = self.sandbox.living.accounts.get_mut(&player_id) {
            // Preserve identity and the selected interaction body for resume.
            // Offline RTS orders continue; stale manual velocity must not make
            // a stationary body animate or exhaust itself indefinitely.
            if account.possessed {
                account.possessed = false;
                if let Some(unit) = self.sandbox.living.units.get_mut(&account.active_unit) {
                    unit.velocity_enu_m_s = [0.; 3];
                }
            }
        }
    }

    fn send_sandbox_snapshot(&self, peer: &mut Peer, actor_id: u64) -> Result<()> {
        let view = self.sandbox.snapshot_for(actor_id, peer.radius_m)?;
        peer.connection.queue(&ServerMessage::SandboxSnapshot {
            tick: self.tick,
            view,
        })
    }
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
            let candidate = frame
                .to_geo([radius * theta.cos(), 0., radius * theta.sin()])
                .ok()?;
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
mod tests {
    use super::*;

    #[test]
    fn invalid_configuration_is_reported_before_binding() {
        let config = ServerConfig {
            tick_hz: 0,
            ..ServerConfig::default()
        };
        assert!(matches!(
            Server::bind(config),
            Err(NetError::Configuration(_))
        ));
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
}
