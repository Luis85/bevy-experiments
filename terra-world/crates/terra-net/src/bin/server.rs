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
use terra_stream::{ChunkStreamer, StreamConfig, StreamEvent};

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
}

impl ReferenceEnvironment {
    fn surface(&self) -> &ResidentSurface {
        match self {
            Self::Procedural { surface, .. } | Self::Historical { surface, .. } => surface,
        }
    }
    fn surface_mut(&mut self) -> &mut ResidentSurface {
        match self {
            Self::Procedural { surface, .. } | Self::Historical { surface, .. } => surface,
        }
    }
}

impl WorldEnvironment for ReferenceEnvironment {
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
    let mut world_pack = None::<PathBuf>;
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
            "--world-pack" => world_pack = Some(value()?.into()),
            "--epoch" | "--date" => epoch = Some(parse_date(&value()?)?),
            "--politics" => politics_file = Some(value()?.into()),
            "--help" | "-h" => {
                println!(
                    "Terra authoritative LAN server\n\n  --bind ADDRESS:PORT  default 127.0.0.1:7777\n  --ticks N            stop after N new simulation ticks\n  --lat N --lon N      authoritative spawn (default Munich)\n  --seed N             immutable world seed (default 42)\n  --level N            procedural tile level (default 15)\n  --resolution N       procedural grid resolution (default 65)\n  --world-pack FILE    versioned historical pack manifest\n  --epoch DATE         required for pack: Unix seconds or YYYY-MM-DD\n  --politics FILE      dated political GeoJSON, shared by all clients\n  --no-living          disable authored economy and population\n  --save PATH          atomic save every 30s and on clean shutdown\n  --load PATH          load a matching versioned server save\n\nUses bounded TCP JSON, 20 Hz simulation and 10 Hz actor snapshots.\nDefault terrain is the explicitly synthetic offline reference surface.\nHistorical terrain requires a matching dated local pack, with no fallback.\nResident map geometry governs dry land, slopes, bridge decks and footprints.\nThis reference server has no TLS, account system or rigid-body physics engine."
                );
                return Ok(());
            }
            _ => return Err(format!("unknown argument: {argument}").into()),
        }
    }
    config.spawn = GeoPosition::new(config.spawn.latitude, config.spawn.longitude, 0.0)?;
    let environment = match (world_pack, epoch) {
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
    let mut server = Server::bind(config)?;
    server.set_environment(Box::new(environment.clone()));
    if let Some(path) = load {
        server.load_world(path)?;
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
            server.register_chunk(&chunk);
        }
        ReferenceEnvironment::Historical { pack, epoch, .. } => {
            for key in pack.keys() {
                let chunk = pack.chunk_ref(key, *epoch)?;
                living_features.extend(chunk.features.clone());
                server.register_chunk(chunk);
            }
        }
    }
    if living && !server.sandbox().living.enabled {
        server.enable_living_demo(&living_features)?;
    }
    // Source registration follows the union of connected player areas. Surface
    // queries remain resident/analytic; no data request blocks a server tick.
    let mut streamer = if matches!(environment, ReferenceEnvironment::Procedural { .. }) {
        Some(ChunkStreamer::new(StreamConfig {
            live: false,
            cache_dir: ".terra-server-cache".into(),
            max_resident: 256,
            max_queued: 256,
            max_in_flight: 2,
            terrain_resolution: server.config().manifest.terrain_resolution,
            seed: server.config().manifest.seed,
            ..Default::default()
        })?)
    } else {
        None
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
    let mut last_report = Instant::now();
    let mut last_interest = Instant::now() - Duration::from_secs(1);
    while running.load(Ordering::Relaxed)
        && ticks.is_none_or(|count| server.tick() - initial_tick < count)
    {
        let now = Instant::now();
        let report = server.update(now.duration_since(last).as_secs_f64());
        last = now;
        if let Some(streamer) = &mut streamer {
            if last_interest.elapsed() >= Duration::from_millis(200) {
                let centers: Vec<_> = server
                    .actors()
                    .values()
                    .filter(|actor| server.sandbox().players.contains_key(&actor.id))
                    .filter_map(|actor| {
                        TileKey::at(actor.position, server.config().manifest.chunk_level).ok()
                    })
                    .collect();
                let keys = stream_interest_keys(
                    TileKey::at(spawn, server.config().manifest.chunk_level)?,
                    centers,
                );
                streamer.request_tiles(keys)?;
                last_interest = now;
            }
            for event in streamer.poll(2) {
                match event {
                    StreamEvent::Loaded(chunk) => server.register_chunk(&chunk),
                    StreamEvent::Failed { error, .. } => {
                        eprintln!("Source chunk unavailable: {error}")
                    }
                    StreamEvent::Evicted(key) => server.evict_chunk(key),
                }
            }
        }
        if last_report.elapsed() >= Duration::from_secs(1) {
            println!(
                "tick={} clients={} actors={} unix={:.3}",
                report.tick,
                report.clients,
                report.actors,
                server.clock().unix_seconds
            );
            last_report = now;
        }
        if last_save.elapsed() >= Duration::from_secs(30) {
            if let Some(path) = &save {
                server.save_world(path)?;
            }
            last_save = now;
        }
        std::thread::sleep(Duration::from_millis(2));
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
    use std::collections::BTreeSet;

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
