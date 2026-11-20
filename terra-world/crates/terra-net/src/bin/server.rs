use std::{
    error::Error,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use terra_core::{ActorKind, GeoPosition, LocalFrame, TileKey, WorldChunk, WorldClock};
use terra_history::{HistoricalPack, PoliticsDataset, parse_date};
use terra_net::{ResidentSurface, Server, ServerConfig, WorldEnvironment};
use terra_stream::{
    ChunkProvider, ChunkStreamer, PreparedHeightmapProvider, StreamConfig, StreamEvent,
};

#[derive(Clone)]
enum ReferenceEnvironment {
    Procedural {
        seed: u64,
        surface: ResidentSurface,
    },
    Historical {
        pack: Arc<HistoricalPack>,
        epoch: i64,
        surface: ResidentSurface,
    },
    Heightmap {
        provider: Arc<PreparedHeightmapProvider>,
        surface: ResidentSurface,
    },
}

impl ReferenceEnvironment {
    fn surface(&self) -> &ResidentSurface {
        match self {
            Self::Procedural { surface, .. }
            | Self::Historical { surface, .. }
            | Self::Heightmap { surface, .. } => surface,
        }
    }
    fn surface_mut(&mut self) -> &mut ResidentSurface {
        match self {
            Self::Procedural { surface, .. }
            | Self::Historical { surface, .. }
            | Self::Heightmap { surface, .. } => surface,
        }
    }
}

impl WorldEnvironment for ReferenceEnvironment {
    fn vehicle_ground_sample(
        &self,
        position: GeoPosition,
    ) -> Option<terra_core::VehicleGroundSample> {
        self.surface().vehicle_ground_sample(position)
    }
    fn vehicle_support_with_clearance(
        &self,
        position: GeoPosition,
        body_height_m: f64,
    ) -> Option<GeoPosition> {
        self.surface()
            .vehicle_support_with_clearance(position, body_height_m)
    }
    fn vehicle_support(&self, position: GeoPosition) -> Option<GeoPosition> {
        self.surface().vehicle_support(position)
    }
    fn ground_height_m(&self, position: GeoPosition) -> Option<f64> {
        match self {
            Self::Procedural { seed, surface } => surface
                .terrain_height_m(position)
                .or_else(|| Some(terra_stream::procedural_height(position, *seed) as f64)),
            Self::Historical { pack, epoch, .. } => pack
                .terrain_height(position, *epoch)
                .ok()
                .flatten()
                .map(f64::from),
            Self::Heightmap { provider, .. } => provider.sample_height(position),
        }
    }

    fn can_enter(&self, position: GeoPosition) -> bool {
        self.ground_height_m(position).is_some()
    }
    fn register_chunk(&mut self, chunk: &WorldChunk) {
        self.surface_mut().register_chunk(chunk);
    }
    fn evict_chunk(&mut self, key: TileKey) {
        self.surface_mut().evict_chunk(key);
    }
    fn water_height_m(&self, position: GeoPosition) -> Option<f64> {
        self.surface()
            .sample(position)
            .filter(|s| s.water)
            .map(|s| s.height_m.max(0.) + 0.35)
    }
    fn resolve_movement(
        &self,
        kind: ActorKind,
        from: GeoPosition,
        candidate: GeoPosition,
    ) -> Option<GeoPosition> {
        self.surface().resolve_movement(kind, from, candidate)
    }
    fn allows_placement(&self, position: GeoPosition, radius_m: f64, footprint: bool) -> bool {
        self.surface()
            .allows_placement(position, radius_m, footprint)
    }
}

/// Keep admission terrain resident even when every connected player is far
/// away. Player centers precede their wider rings under a tight cache budget.
fn stream_interest_keys(spawn: TileKey, centers: Vec<TileKey>) -> Vec<TileKey> {
    let mut keys = spawn.neighbors(1);
    keys.extend(centers.iter().copied());
    for center in centers {
        keys.extend(center.neighbors(1));
    }
    keys
}

fn main() {
    if let Err(error) = run() {
        eprintln!("terra-server: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let mut config = ServerConfig::default();
    let mut ticks = None::<u64>;
    let mut save = None::<PathBuf>;
    let mut load = None::<PathBuf>;
    let mut import_json = None::<PathBuf>;
    let mut export_json = None::<PathBuf>;
    let mut tls_cert = None::<PathBuf>;
    let mut tls_key = None::<PathBuf>;
    let mut allow_insecure_remote = false;
    let mut region_store = None::<PathBuf>;
    let mut world_pack = None::<PathBuf>;
    let mut heightmap = None::<PathBuf>;
    let mut politics_file = None::<PathBuf>;
    let mut epoch = None::<i64>;
    let mut living = true;
    let mut explicit_level = false;
    let mut explicit_resolution = false;
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        let mut value = || {
            args.next()
                .ok_or_else(|| format!("missing value for {argument}"))
        };
        match argument.as_str() {
            "--no-living" => living = false,
            "--resume-only" => config.allow_new_players = false,
            "--bind" => config.bind_addr = value()?.parse()?,
            "--ticks" => ticks = Some(value()?.parse()?),
            "--lat" => config.spawn.latitude = value()?.parse()?,
            "--lon" => config.spawn.longitude = value()?.parse()?,
            "--seed" => config.manifest.seed = value()?.parse()?,
            "--level" => {
                config.manifest.chunk_level = value()?.parse()?;
                explicit_level = true;
            }
            "--resolution" => {
                config.manifest.terrain_resolution = value()?.parse()?;
                explicit_resolution = true;
            }
            "--save" => save = Some(value()?.into()),
            "--load" => load = Some(value()?.into()),
            "--import-json" => import_json = Some(value()?.into()),
            "--export-json" => export_json = Some(value()?.into()),
            "--tls-cert" => tls_cert = Some(value()?.into()),
            "--tls-key" => tls_key = Some(value()?.into()),
            "--allow-insecure-remote" => allow_insecure_remote = true,
            "--region-store" => region_store = Some(value()?.into()),
            "--world-pack" => world_pack = Some(value()?.into()),
            "--heightmap" => heightmap = Some(value()?.into()),
            "--epoch" | "--date" => epoch = Some(parse_date(&value()?)?),
            "--politics" => politics_file = Some(value()?.into()),
            "--help" | "-h" => {
                println!(
                    "Terra authoritative LAN server\n\n  --bind ADDRESS:PORT  default 127.0.0.1:7777\n  --ticks N            stop after N new simulation ticks\n  --lat N --lon N      authoritative spawn (default Munich)\n  --seed N             immutable world seed (default 42)\n  --level N            procedural tile level (default 15)\n  --resolution N       procedural grid resolution (default 65)\n  --world-pack FILE    versioned historical pack manifest\n  --heightmap FILE     immutable georeferenced PNG/semantic bundle manifest\n  --epoch DATE         required for pack: Unix seconds or YYYY-MM-DD\n  --politics FILE      dated political GeoJSON, shared by all clients\n  --no-living          disable authored economy and population\n  --resume-only        admit existing saved capabilities; disable new guests\n  --tls-cert PATH      PEM certificate chain (requires --tls-key)\n  --tls-key PATH       matching PEM private key\n  --allow-insecure-remote  explicitly allow non-loopback plaintext\n  --save PATH          atomic save every 30s and on clean shutdown\n  --load PATH          load a matching versioned server save\n  --import-json PATH   validate server JSON into a NEW --region-store; exit\n  --export-json PATH   export existing --region-store to NEW server JSON; exit\n  --region-store PATH  create/resume transactional regional SQLite state\n                       checkpoint input/source batches, every 30s and on exit\n                       --save additionally exports the complete world\n\nUses bounded TCP JSON, 20 Hz simulation and 10 Hz actor snapshots.\nDefault terrain is the explicitly synthetic offline reference surface.\nHistorical terrain requires a matching dated local pack, with no fallback.\nHeightmaps require matching local bundle identity on every native client.\nResident map geometry governs dry land, slopes, bridge decks and footprints.\nTLS verifies server identity and encrypts capabilities; accounts and rigid-body physics remain game-owned."
                );
                return Ok(());
            }
            _ => return Err(format!("unknown argument: {argument}").into()),
        }
    }
    if tls_cert.is_some() != tls_key.is_some() {
        return Err("--tls-cert and --tls-key must be supplied together".into());
    }
    if heightmap.is_some() && (world_pack.is_some() || epoch.is_some()) {
        return Err("--heightmap is an explicit immutable source; do not combine it with --world-pack or --epoch".into());
    }
    if !config.bind_addr.ip().is_loopback() && tls_cert.is_none() && !allow_insecure_remote {
        return Err("non-loopback plaintext bind requires --allow-insecure-remote; use --tls-cert and --tls-key to protect player capabilities".into());
    }
    #[cfg(not(feature = "tls"))]
    if tls_cert.is_some() {
        return Err("this build does not enable terra-net/tls".into());
    }
    #[cfg(feature = "tls")]
    let tls = match (tls_cert, tls_key) {
        (Some(certificate), Some(key)) => Some(terra_net::ServerTlsConfig::from_pem_files(
            certificate,
            key,
        )?),
        _ => None,
    };
    if load.is_some() && region_store.is_some() {
        return Err("--load and --region-store are separate restore modes; use --import-json PATH --region-store NEW_DATABASE for explicit migration".into());
    }
    if import_json.is_some() {
        if load.is_some() || save.is_some() || export_json.is_some() || region_store.is_none() {
            return Err("--import-json requires --region-store NEW_DATABASE and cannot be combined with --load or --save".into());
        }
        if std::fs::symlink_metadata(region_store.as_ref().unwrap()).is_ok() {
            return Err("import destination already exists; use a new database path".into());
        }
    }
    if let Some(output) = &export_json {
        if load.is_some() || save.is_some() || region_store.is_none() {
            return Err("--export-json requires --region-store EXISTING_DATABASE and cannot be combined with --load or --save".into());
        }
        if !std::fs::symlink_metadata(region_store.as_ref().unwrap())?.is_file() {
            return Err("export source must be an existing database".into());
        }
        terra_persistence::validate_output_path(region_store.as_ref().unwrap(), output)?;
    }
    // A JSON export must never replace the live database or one of SQLite's
    // companion files. Check aliases before binding or opening any world data.
    if let (Some(database), Some(output)) = (&region_store, &save) {
        terra_persistence::validate_output_path(database, output)?;
    }
    config.spawn = GeoPosition::new(config.spawn.latitude, config.spawn.longitude, 0.0)?;
    let environment = if let Some(path) = heightmap {
        let provider = Arc::new(PreparedHeightmapProvider::open(
            path,
            config.manifest.terrain_resolution,
        )?);
        let spawn_key = TileKey::at(config.spawn, config.manifest.chunk_level)?;
        if !provider.covers(spawn_key) {
            return Err("heightmap does not cover the complete spawn tile; choose coordinates inside its coverage".into());
        }
        config.manifest.terrain_provider = "heightmap".into();
        config.manifest.base_data_revision = provider.source_revision().to_owned();
        ReferenceEnvironment::Heightmap {
            provider,
            surface: ResidentSurface::new(config.manifest.chunk_level),
        }
    } else {
        match (world_pack, epoch) {
        (Some(path), Some(epoch)) => {
            let pack = Arc::new(HistoricalPack::load(path)?);
            if !pack.contains_time(epoch) {
                return Err("requested epoch is outside the historical pack validity interval".into());
            }
            let keys = pack.keys();
            let first = keys.first().ok_or("historical pack has no terrain tiles")?;
            let resolution = pack.chunk_ref(*first, epoch)?.heights.width;
            for key in &keys {
                let chunk = pack.chunk_ref(*key, epoch)?;
                if key.level != first.level || chunk.heights.width != resolution || chunk.heights.height != resolution {
                    return Err("reference server requires one tile level and uniform square terrain grids per historical pack".into());
                }
            }
            if (explicit_level && config.manifest.chunk_level != first.level)
                || (explicit_resolution && config.manifest.terrain_resolution != resolution) {
                return Err("explicit level or resolution differs from the historical pack".into());
            }
            config.manifest.chunk_level = first.level;
            config.manifest.terrain_resolution = resolution;
            config.manifest.terrain_provider = "historical-pack".into();
            config.manifest.base_data_revision = pack.manifest.id.clone();
            config.manifest.data_epoch_unix = Some(epoch);
            config.initial_unix_seconds = Some(epoch as f64);
            ReferenceEnvironment::Historical { pack, epoch, surface:ResidentSurface::new(first.level) }
        }
        (None, None) => ReferenceEnvironment::Procedural { seed: config.manifest.seed, surface:ResidentSurface::new(config.manifest.chunk_level) },
        (Some(_), None) => return Err("--world-pack requires an explicit --epoch".into()),
        (None, Some(_)) => return Err("--epoch requires --world-pack; current procedural terrain cannot be labelled historical data".into()),
    }
    };
    if let Some(path) = politics_file {
        let dataset = PoliticsDataset::load(path)?;
        let mut calendar = config
            .initial_unix_seconds
            .unwrap_or(WorldClock::default().unix_seconds) as i64;
        if config.manifest.data_epoch_unix.is_none() && calendar < dataset.valid_from_unix {
            calendar = dataset.valid_from_unix;
            config.initial_unix_seconds = Some(calendar as f64);
        }
        if !dataset.contains_time(calendar) {
            return Err("political dataset does not cover the selected starting epoch".into());
        }
        config.manifest.political_dataset_revision = Some(dataset.id);
    }
    config.spawn.altitude_m = environment
        .ground_height_m(config.spawn)
        .ok_or("authoritative spawn is outside historical terrain coverage")?;
    let spawn = config.spawn;
    #[cfg(feature = "tls")]
    let mut server = match tls {
        Some(tls) => Server::bind_tls(config, tls)?,
        None => Server::bind(config)?,
    };
    #[cfg(not(feature = "tls"))]
    let mut server = Server::bind(config)?;
    server.set_environment(Box::new(environment.clone()));
    if let Some(path) = load {
        server.load_world(path)?;
    }
    if let Some(path) = import_json {
        // Reuse the actual host validator, including manifest, private resume
        // capabilities, clock, terrain coverage and replay sequences. No demo
        // seeding or simulation runs during migration.
        server.load_world(path)?;
        let destination = region_store.as_ref().unwrap();
        server.import_region_store_new(destination)?;
        println!(
            "Imported validated server world into {}",
            destination.display()
        );
        return Ok(());
    }
    if let Some(path) = &region_store {
        server.open_region_store(path)?;
    }
    if let Some(path) = export_json {
        server.export_world_new(&path)?;
        println!(
            "Exported complete validated server world to {}",
            path.display()
        );
        return Ok(());
    }
    let mut living_features = Vec::new();
    match &environment {
        ReferenceEnvironment::Procedural { seed, .. } => {
            let key = TileKey::at(spawn, server.config().manifest.chunk_level)?;
            let chunk = terra_stream::procedural_chunk(
                key,
                server.config().manifest.terrain_resolution,
                *seed,
            );
            living_features.extend(chunk.features.clone());
            server.try_register_chunk(&chunk)?;
        }
        ReferenceEnvironment::Historical { pack, epoch, .. } => {
            for key in pack.keys() {
                let chunk = pack.chunk_ref(key, *epoch)?;
                living_features.extend(chunk.features.clone());
                server.try_register_chunk(chunk)?;
            }
        }
        ReferenceEnvironment::Heightmap { provider, .. } => {
            let center = TileKey::at(spawn, server.config().manifest.chunk_level)?;
            // Register covered neighbours before admission/seeding, so crew and
            // facilities at a tile edge use actual resident source support.
            for key in center
                .neighbors(1)
                .into_iter()
                .filter(|key| provider.covers(*key))
            {
                let chunk = provider.load(key)?.chunk;
                living_features.extend(chunk.features.clone());
                server.try_register_chunk(&chunk)?;
            }
        }
    }
    if living {
        server.enable_living_demo(&living_features)?;
    }
    // Source registration follows the union of connected player areas. Surface
    // queries remain resident/analytic; no data request blocks a server tick.
    let stream_config = StreamConfig {
        live: false,
        cache_dir: ".terra-server-cache".into(),
        max_resident: 256,
        max_queued: 256,
        max_in_flight: 2,
        terrain_resolution: server.config().manifest.terrain_resolution,
        seed: server.config().manifest.seed,
        ..Default::default()
    };
    let mut streamer = match &environment {
        ReferenceEnvironment::Procedural { .. } => Some(ChunkStreamer::new(stream_config)?),
        ReferenceEnvironment::Heightmap { provider, .. } => Some(ChunkStreamer::with_provider(
            stream_config,
            provider.clone(),
        )?),
        ReferenceEnvironment::Historical { .. } => None,
    };
    for (id, item, local) in [
        ("authored:spawn:wood", "wood", [6.0, 0.0, 0.0]),
        ("authored:spawn:stone", "stone", [-6.0, 0.0, 0.0]),
    ] {
        if !server.sandbox().resource_nodes.contains_key(id) {
            let mut position = LocalFrame::new(spawn).to_geo(local)?;
            let Some(height) = environment.ground_height_m(position) else {
                continue;
            };
            position.altitude_m = height;
            server.seed_resource(
                id.into(),
                position,
                item,
                100,
                if item == "wood" { 60.0 } else { 0.0 },
            )?;
        }
    }
    if region_store.is_some() {
        server.checkpoint_regions()?;
    }
    println!(
        "Player admission: {}",
        if server.config().allow_new_players {
            "new guests and saved players"
        } else {
            "existing saved players only"
        }
    );
    let initial_tick = server.tick();
    let running = Arc::new(AtomicBool::new(true));
    let signal = running.clone();
    ctrlc::set_handler(move || signal.store(false, Ordering::Relaxed))?;
    println!(
        "Terra server listening on {} (world {}, seed {}, provider {}, revision {}, data epoch {:?})",
        server.local_addr()?,
        server.config().manifest.world_id,
        server.config().manifest.seed,
        server.config().manifest.terrain_provider,
        server.config().manifest.base_data_revision,
        server.config().manifest.data_epoch_unix
    );
    let mut last = Instant::now();
    let mut last_save = Instant::now();
    let mut last_checkpoint = Instant::now();
    let mut last_report = Instant::now();
    let mut replication_window = terra_net::ReplicationStats::default();
    let mut interest_candidates = 0usize;
    let mut deferred_command_batches = 0usize;
    let mut peak_pending_bytes = 0usize;
    let mut last_interest = Instant::now() - Duration::from_secs(1);
    let mut source_events = Vec::new();
    while running.load(Ordering::Relaxed)
        && ticks.is_none_or(|count| server.tick() - initial_tick < count)
    {
        let now = Instant::now();
        let report = server.update(now.duration_since(last).as_secs_f64());
        if let Some(error) = &report.persistence_error {
            return Err(error.clone().into());
        }
        last = now;
        replication_window.accumulate(&report.replication);
        interest_candidates += report.interest_candidates_tested;
        deferred_command_batches += report.deferred_command_batches;
        peak_pending_bytes = peak_pending_bytes.max(report.peak_peer_pending_bytes);
        if let Some(streamer) = &mut streamer {
            let mut residency_changed = false;
            if last_interest.elapsed() >= Duration::from_millis(200) {
                let centers: Vec<_> = server
                    .actors()
                    .values()
                    .filter(|actor| server.sandbox().players.contains_key(&actor.id))
                    .filter_map(|actor| {
                        TileKey::at(actor.position, server.config().manifest.chunk_level).ok()
                    })
                    .collect();
                let mut keys = stream_interest_keys(
                    TileKey::at(spawn, server.config().manifest.chunk_level)?,
                    centers,
                );
                if let ReferenceEnvironment::Heightmap { provider, .. } = &environment {
                    keys.retain(|key| provider.covers(*key));
                }
                streamer.request_tiles(keys)?;
                last_interest = now;
            }
            // Keep completed source events in the streamer's bounded queue while
            // the storage snapshot owns the authority revision.
            if source_events.is_empty() && !server.persistence_pending() {
                source_events = streamer.poll(2);
            }
            let integrate = !source_events.is_empty() && server.prepare_source_batch()?;
            for event in if integrate {
                std::mem::take(&mut source_events)
            } else {
                Vec::new()
            } {
                match event {
                    StreamEvent::Loaded(chunk) => {
                        server.try_register_chunk(&chunk)?;
                        residency_changed = true;
                    }
                    StreamEvent::Failed { error, .. } => {
                        eprintln!("Source chunk unavailable: {error}")
                    }
                    StreamEvent::Evicted(key) => {
                        server.try_evict_chunk(key)?;
                        residency_changed = true;
                    }
                }
            }
            // A source batch is the handoff boundary: activate before seeding,
            // commit all host/domain records, then retire dependency-safe pages.
            if residency_changed && region_store.is_some() {
                server.request_region_checkpoint()?;
                last_checkpoint = now;
            }
        }
        if last_report.elapsed() >= Duration::from_secs(1) {
            let storage = server.region_store_status().map_or_else(
                || "storage=memory".to_string(),
                |status| format!(
                    "storage=regional generation={} active_regions={} dormant_regions={} pinned_regions={} active_records={} dormant_records={}",
                    status.generation, status.active_regions, status.dormant_regions,
                    status.pinned_regions, status.active_records, status.dormant_records,
                ),
            );
            println!(
                "tick={} clients={} actors={} unix={:.3} replication_bytes={} full_equivalent_bytes={} keyframes={} deltas={} snapshot_deferrals={} command_batch_deferrals={} pending_bytes={} peak_pending_bytes={} aoi_candidates={} {}",
                report.tick,
                report.clients,
                report.actors,
                server.clock().unix_seconds,
                replication_window.wire_bytes,
                replication_window.full_snapshot_bytes,
                replication_window.keyframes,
                replication_window.deltas,
                replication_window.backpressure_deferrals,
                deferred_command_batches,
                report.max_peer_pending_bytes,
                peak_pending_bytes,
                interest_candidates,
                storage,
            );
            replication_window = terra_net::ReplicationStats::default();
            interest_candidates = 0;
            deferred_command_batches = 0;
            peak_pending_bytes = 0;
            last_report = now;
        }
        if region_store.is_some() && last_checkpoint.elapsed() >= Duration::from_secs(30) {
            server.request_region_checkpoint()?;
            last_checkpoint = now;
        }
        if !server.persistence_pending() && last_save.elapsed() >= Duration::from_secs(30) {
            if let Some(path) = &save {
                server.save_world(path)?;
            }
            last_save = now;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    if region_store.is_some() {
        server.checkpoint_regions()?;
    }
    if let Some(path) = &save {
        server.save_world(path)?;
    }
    println!(
        "Server stopped at tick {} with {} persisted players",
        server.tick(),
        server.sandbox().players.len()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};

    #[test]
    fn reference_environment_preserves_wheel_material_and_body_clearance() {
        let key = TileKey::at(GeoPosition::new(48.137, 11.575, 10.).unwrap(), 15).unwrap();
        let origin = GeoPosition {
            altitude_m: 10.,
            ..key.center()
        };
        let frame = LocalFrame::new(origin);
        let mut chunk = WorldChunk {
            key,
            heights: terra_core::HeightField::constant(10.),
            features: vec![terra_core::WorldFeature {
                id: "fixture:gravel-road".into(),
                kind: terra_core::FeatureKind::Road,
                points: vec![
                    frame.to_geo([-20., 0., 0.]).unwrap(),
                    frame.to_geo([20., 0., 0.]).unwrap(),
                ],
                height_m: 8.,
                properties: BTreeMap::from([("surface".into(), "gravel".into())]),
            }],
            sources: Vec::new(),
            warnings: Vec::new(),
        };
        let mut environment = ReferenceEnvironment::Procedural {
            seed: 42,
            surface: ResidentSurface::new(15),
        };
        environment.register_chunk(&chunk);
        let sample = environment.vehicle_ground_sample(origin).unwrap();
        assert_eq!(sample.surface, terra_core::VehicleSurface::Gravel);
        assert_eq!(sample.height_m, 10.);
        chunk.features.push(terra_core::WorldFeature {
            id: "fixture:overhead".into(),
            kind: terra_core::FeatureKind::Building,
            points: [[-8., -8.], [8., -8.], [8., 8.], [-8., 8.]]
                .map(|p| frame.to_geo([p[0], 0., p[1]]).unwrap())
                .to_vec(),
            height_m: 8.,
            properties: BTreeMap::from([("min_height".into(), "240 cm".into())]),
        });
        environment.register_chunk(&chunk);
        assert!(
            environment
                .vehicle_support_with_clearance(origin, 1.55)
                .is_some()
        );
        assert!(
            environment
                .vehicle_support_with_clearance(origin, 3.25)
                .is_none()
        );
        environment.evict_chunk(key);
        assert!(environment.vehicle_ground_sample(origin).is_none());
    }

    #[test]
    fn spawn_residency_is_preserved_when_all_connected_players_are_far_away() {
        let spawn = TileKey::at(GeoPosition::new(48.137, 11.575, 0.).unwrap(), 15).unwrap();
        let players: Vec<_> = (0..16)
            .map(|n| {
                TileKey::at(
                    GeoPosition::new(30. + n as f64, 40. + n as f64 * 5., 0.).unwrap(),
                    15,
                )
                .unwrap()
            })
            .collect();
        let requested = stream_interest_keys(spawn, players.clone());
        // ChunkStreamer preserves first occurrence and admits up to256 keys.
        let mut seen = BTreeSet::new();
        let admitted: BTreeSet<_> = requested
            .into_iter()
            .filter(|key| seen.insert(*key))
            .take(256)
            .collect();
        assert!(spawn.neighbors(1).iter().all(|key| admitted.contains(key)));
        assert!(players.iter().all(|key| admitted.contains(key)));
        assert!(admitted.len() <= 17 * 9);
    }
}
