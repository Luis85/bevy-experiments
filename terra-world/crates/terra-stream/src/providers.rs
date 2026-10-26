//! Built-in provider adapters. Parsing functions are public for offline fixtures
//! and importer tools. External layers can implement [`crate::ChunkProvider`].
use crate::{StreamConfig, StreamError, cache, procedural_chunk};
use image::RgbImage;
use reqwest::blocking::{Client, Response};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Read,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};
use terra_core::{
    FeatureKind, GeoPosition, HeightField, SourceRecord, TileKey, WeatherSample, WorldChunk,
    WorldClock, WorldFeature, synthetic_weather,
};

const MERCATOR_MAX_LAT: f64 = 85.051_128_779_806_6;
#[derive(Default)]
struct OverpassBudget {
    last: Option<Instant>,
    requests: usize,
    downloaded_bytes: usize,
}
static OVERPASS_GATE: OnceLock<Mutex<OverpassBudget>> = OnceLock::new();

fn client(config: &StreamConfig, url: &str) -> Result<Client, StreamError> {
    let mut builder = Client::builder()
        .user_agent("TerraWorld/0.2 (open-data development demo; bounded requests)")
        .timeout(Duration::from_secs(config.request_timeout_seconds))
        .connect_timeout(Duration::from_secs(config.request_timeout_seconds.min(8)))
        .redirect(reqwest::redirect::Policy::limited(3));
    let ca_path = config
        .ca_cert_path
        .clone()
        .or_else(|| std::env::var_os("SSL_CERT_FILE").map(std::path::PathBuf::from));
    if let Some(path) = ca_path {
        let metadata = std::fs::metadata(&path)?;
        if metadata.len() > 2 * 1024 * 1024 {
            return Err(StreamError::Configuration("CA bundle exceeds 2 MiB".into()));
        }
        let pem = std::fs::read(path)?;
        let certificates = reqwest::Certificate::from_pem_bundle(&pem)?;
        if certificates.is_empty() {
            return Err(StreamError::Configuration(
                "CA bundle contains no certificates".into(),
            ));
        }
        for certificate in certificates {
            builder = builder.add_root_certificate(certificate);
        }
    }
    // Local fixture/self-hosted development services must not be routed through
    // a process-wide outbound proxy. Keep normal proxy support for public URLs.
    let loopback = reqwest::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_owned))
        .is_some_and(|host| {
            host == "localhost"
                || host
                    .trim_matches(['[', ']'])
                    .parse::<std::net::IpAddr>()
                    .is_ok_and(|ip| ip.is_loopback())
        });
    if loopback {
        builder = builder.no_proxy();
    }
    Ok(builder.build()?)
}

fn body_limited(response: Response, config: &StreamConfig) -> Result<Vec<u8>, StreamError> {
    let response = response.error_for_status()?;
    if response
        .content_length()
        .is_some_and(|size| size > config.max_response_bytes as u64)
    {
        return Err(StreamError::Payload(
            "HTTP response exceeds configured byte budget".into(),
        ));
    }
    let mut bytes = Vec::new();
    response
        .take(config.max_response_bytes as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > config.max_response_bytes {
        return Err(StreamError::Payload(
            "HTTP response exceeds configured byte budget".into(),
        ));
    }
    Ok(bytes)
}

fn get(url: &str, config: &StreamConfig) -> Result<Vec<u8>, StreamError> {
    let client = client(config, url)?;
    let mut last = None;
    for attempt in 0..2 {
        match client
            .get(url)
            .send()
            .map_err(StreamError::from)
            .and_then(|r| body_limited(r, config))
        {
            Ok(bytes) => return Ok(bytes),
            Err(error) => {
                let retry = matches!(&error,StreamError::Http(e) if e.is_timeout() || e.is_connect() || e.status().is_some_and(|s| s.is_server_error()));
                if !retry || attempt == 1 {
                    return Err(error);
                }
                last = Some(error);
                std::thread::sleep(Duration::from_millis(400));
            }
        }
    }
    Err(last.unwrap_or_else(|| StreamError::Payload("request retry loop exhausted".into())))
}

fn chunk_cache_key(key: TileKey, config: &StreamConfig) -> String {
    format!(
        "chunk-v4/{}/{}/{}/{}/{}/{}/{}/{}/{}",
        key.level,
        key.x,
        key.y,
        config.terrain_resolution,
        config.seed,
        config.osm,
        config.terrain_url,
        config.overpass_url,
        config.allow_fallback
    )
}

/// Synchronously load a chunk. Use [`crate::ChunkStreamer`] in frame-based apps.
/// This may perform network and disk I/O; it is meant for workers and CLIs.
pub fn load_chunk(key: TileKey, config: &StreamConfig) -> Result<WorldChunk, StreamError> {
    load_chunk_cached(key, config).map(|(chunk, _)| chunk)
}

pub(crate) fn load_chunk_cached(
    key: TileKey,
    config: &StreamConfig,
) -> Result<(WorldChunk, bool), StreamError> {
    config.validate()?;
    key.validate()
        .map_err(|e| StreamError::Geography(e.to_string()))?;
    if !config.live {
        return Ok((
            procedural_chunk(key, config.terrain_resolution, config.seed),
            false,
        ));
    }
    let cache_key = chunk_cache_key(key, config);
    if let Some(data) = cache::read(config, &cache_key, config.chunk_cache_ttl_seconds) {
        if let Ok(chunk) = serde_json::from_slice::<WorldChunk>(&data) {
            if chunk.key == key
                && chunk.validate().is_ok()
                && chunk.heights.width == config.terrain_resolution
                && chunk.heights.height == config.terrain_resolution
                && !chunk.sources.is_empty()
                && chunk.sources.iter().all(|source| {
                    !source.synthetic
                        && cache::now()
                            .checked_sub(source.retrieved_unix_seconds)
                            .is_some_and(|age| {
                                age >= 0 && age as u64 <= config.chunk_cache_ttl_seconds
                            })
                })
            {
                return Ok((chunk, true));
            }
        }
    }
    // Missing semantic data stays missing in live mode. Never plant fictional
    // buildings over real terrain and pass them off as surveyed objects.
    let mut chunk = WorldChunk {
        key,
        heights: HeightField {
            width: 0,
            height: 0,
            samples_m: vec![],
        },
        features: vec![],
        sources: vec![],
        warnings: vec![],
    };
    match load_terrain_timed(key, config) {
        Ok((heights, retrieved_unix_seconds)) => {
            chunk.heights = heights;
            chunk.sources.push(SourceRecord {
                provider:"Mapzen Terrain Tiles / AWS Open Data".into(),
                license:"Source-specific open-data terms; see Tilezen attribution manifest".into(),
                attribution:"Mapzen Terrain Tiles and contributing elevation datasets; https://github.com/tilezen/joerd/blob/master/docs/attribution.md".into(),
                url:config.terrain_url.clone(),
                retrieved_unix_seconds,synthetic:false,
            });
            chunk.warnings.push("Elevation source vertical datum is retained without a geoid transform. Rendered height approximates WGS84 ellipsoidal altitude; not survey/navigation precision.".into());
        }
        Err(error) if config.allow_fallback => {
            let fallback = procedural_chunk(key, config.terrain_resolution, config.seed);
            chunk.heights = fallback.heights;
            chunk.sources.extend(fallback.sources);
            chunk
                .warnings
                .push(format!("TERRAIN FALLBACK — synthetic heightfield: {error}"));
        }
        Err(error) => return Err(error),
    }
    if config.osm {
        match load_osm(key, config) {
            Ok(mut features) => {
                let b = key.bounds();
                for feature in &mut features {
                    for point in &mut feature.points {
                        let u = (point.longitude - b.west) / (b.east - b.west);
                        let v = (b.north - point.latitude) / (b.north - b.south);
                        point.altitude_m = chunk.heights.sample(u, v) as f64;
                    }
                }
                chunk.features = features;
                chunk.sources.push(SourceRecord {
                    provider: "OpenStreetMap contributors via Overpass".into(),
                    license: "Open Database License (ODbL) 1.0".into(),
                    attribution:
                        "© OpenStreetMap contributors — https://www.openstreetmap.org/copyright"
                            .into(),
                    url: config.overpass_url.clone(),
                    retrieved_unix_seconds: cache::now(),
                    synthetic: false,
                });
                chunk.warnings.push("OSM importer covers point/way geometry. Relations, multipolygon holes, tunnels and complete road topology require a production importer.".into());
            }
            Err(error) if config.allow_fallback => chunk
                .warnings
                .push(format!("OSM unavailable; semantic layer omitted: {error}")),
            Err(error) => return Err(error),
        }
    }
    // Do not persist failures for a week. A failed provider is retried when the
    // chunk is next loaded; successful raw terrain tiles remain reusable.
    let degraded = chunk.sources.iter().any(|s| s.synthetic)
        || chunk
            .warnings
            .iter()
            .any(|w| w.starts_with("OSM unavailable"));
    if !degraded {
        let data = serde_json::to_vec(&chunk).map_err(|e| StreamError::Payload(e.to_string()))?;
        if let Err(error) = cache::write(config, &cache_key, &data) {
            chunk.warnings.push(format!(
                "Chunk cache write failed; in-memory data remains valid: {error}"
            ));
        }
    }
    Ok((chunk, false))
}

/// Decode the exact Terrarium RGB fixed-point representation (metres).
pub fn decode_terrarium_pixel(rgb: [u8; 3]) -> f32 {
    rgb[0] as f32 * 256.0 + rgb[1] as f32 + rgb[2] as f32 / 256.0 - 32768.0
}

fn mercator_coordinate(latitude: f64, longitude: f64, zoom: u8) -> (f64, f64) {
    let count = (1u32 << zoom) as f64;
    let lat = latitude
        .clamp(-MERCATOR_MAX_LAT, MERCATOR_MAX_LAT)
        .to_radians();
    let x = (longitude + 180.0) / 360.0 * count;
    let y = (1.0 - (lat.tan() + 1.0 / lat.cos()).ln() / std::f64::consts::PI) * 0.5 * count;
    (
        x.clamp(0.0, count - f64::EPSILON * count * 2.0),
        y.clamp(0.0, count - f64::EPSILON * count * 2.0),
    )
}

/// Resample Web Mercator source pixels into a geographic grid. Caps source zoom
/// and fanout; polar tiles fail explicitly rather than silently duplicating 85°N.
pub fn load_terrain(key: TileKey, config: &StreamConfig) -> Result<HeightField, StreamError> {
    load_terrain_timed(key, config).map(|(heights, _)| heights)
}

fn load_terrain_timed(
    key: TileKey,
    config: &StreamConfig,
) -> Result<(HeightField, i64), StreamError> {
    config.validate()?;
    key.validate()
        .map_err(|e| StreamError::Geography(e.to_string()))?;
    if !config.live {
        return Ok((
            procedural_chunk(key, config.terrain_resolution, config.seed).heights,
            0,
        ));
    }
    let bounds = key.bounds();
    if bounds.north > MERCATOR_MAX_LAT || bounds.south < -MERCATOR_MAX_LAT {
        return Err(StreamError::Geography("Terrarium's Mercator coverage stops at ±85.0511°; use a polar/COG provider for this tile".into()));
    }
    let resolution = config.terrain_resolution;
    let mut zoom = key.level.min(15);
    let sample_points = |z: u8| -> Vec<(u32, u32, f64, f64)> {
        (0..resolution)
            .flat_map(|row| {
                (0..resolution).map(move |col| {
                    let latitude = bounds.north
                        - (bounds.north - bounds.south) * row as f64 / (resolution - 1) as f64;
                    let longitude = bounds.west
                        + (bounds.east - bounds.west) * col as f64 / (resolution - 1) as f64;
                    let (x, y) = mercator_coordinate(latitude, longitude, z);
                    (
                        x.floor() as u32,
                        y.floor() as u32,
                        (x - x.floor()) * 256.0,
                        (y - y.floor()) * 256.0,
                    )
                })
            })
            .collect()
    };
    let (points, addresses) = loop {
        let points = sample_points(zoom);
        let addresses: BTreeSet<_> = points.iter().map(|&(x, y, _, _)| (x, y)).collect();
        if addresses.len() <= 12 || zoom == 0 {
            break (points, addresses);
        }
        zoom -= 1;
    };
    let mut tiles = BTreeMap::<(u32, u32), RgbImage>::new();
    let mut oldest_retrieved = cache::now();
    for (x, y) in addresses {
        let url = format!(
            "{}/{zoom}/{x}/{y}.png",
            config.terrain_url.trim_end_matches('/')
        );
        let cache_key = format!("terrarium-v1:{url}");
        let (bytes, retrieved) =
            match cache::read_timed(config, &cache_key, config.chunk_cache_ttl_seconds) {
                Some(entry) => entry,
                None => {
                    let bytes = get(&url, config)?;
                    // Validate before persisting. Decompression allocations are bounded.
                    decode_terrain_png(&bytes)?;
                    let _ = cache::write(config, &cache_key, &bytes);
                    (bytes, cache::now())
                }
            };
        oldest_retrieved = oldest_retrieved.min(retrieved);
        tiles.insert((x, y), decode_terrain_png(&bytes)?);
    }
    let samples_m = points
        .into_iter()
        .map(|(x, y, px, py)| {
            let tile = &tiles[&(x, y)];
            // Source pixels are centres; consistent global sampling on either side of
            // a geographic chunk boundary avoids per-chunk seams. Edge replication
            // within the source tile is an acknowledged half-pixel approximation.
            let sx = (px - 0.5).clamp(0.0, 255.0);
            let sy = (py - 0.5).clamp(0.0, 255.0);
            let ix = sx.floor() as u32;
            let iy = sy.floor() as u32;
            let fx = (sx - ix as f64) as f32;
            let fy = (sy - iy as f64) as f32;
            let at = |x, y| decode_terrarium_pixel(tile.get_pixel(x, y).0);
            let a = at(ix, iy) * (1.0 - fx) + at((ix + 1).min(255), iy) * fx;
            let b = at(ix, (iy + 1).min(255)) * (1.0 - fx)
                + at((ix + 1).min(255), (iy + 1).min(255)) * fx;
            a * (1.0 - fy) + b * fy
        })
        .collect::<Vec<_>>();
    if samples_m
        .iter()
        .any(|h| !h.is_finite() || *h < -12_000.0 || *h > 10_000.0)
    {
        return Err(StreamError::Payload(
            "terrain contains NoData or implausible heights".into(),
        ));
    }
    Ok((
        HeightField {
            width: resolution,
            height: resolution,
            samples_m,
        },
        oldest_retrieved,
    ))
}

fn decode_terrain_png(bytes: &[u8]) -> Result<RgbImage, StreamError> {
    let mut reader =
        image::ImageReader::with_format(std::io::Cursor::new(bytes), image::ImageFormat::Png);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(256);
    limits.max_image_height = Some(256);
    limits.max_alloc = Some(2 * 1024 * 1024);
    reader.limits(limits);
    let image = reader
        .decode()
        .map_err(|e| StreamError::Payload(format!("terrain PNG: {e}")))?;
    if image.width() != 256 || image.height() != 256 {
        return Err(StreamError::Payload(
            "Terrarium tile must be 256×256 pixels".into(),
        ));
    }
    Ok(image.into_rgb8())
}

fn load_osm(key: TileKey, config: &StreamConfig) -> Result<Vec<WorldFeature>, StreamError> {
    if key.level < 13 {
        return Err(StreamError::Geography(
            "public Overpass requests are restricted to detail levels ≥13".into(),
        ));
    }
    let b = key.bounds();
    let bbox = format!("{},{},{},{}", b.south, b.west, b.north, b.east);
    let query = format!(
        "[out:json][timeout:15][maxsize:8388608];(way[building]({bbox});way[building:part]({bbox});way[highway]({bbox});way[natural=water]({bbox});way[waterway]({bbox});way[natural=wood]({bbox});way[landuse~\"forest|farmland|farmyard|meadow|grass|orchard|vineyard\"]({bbox});node[natural=tree]({bbox});node[shop]({bbox});node[amenity]({bbox});node[craft]({bbox});node[tourism]({bbox}););out tags geom;"
    );
    // One request in flight across all instances and threads in this process.
    let mut budget = OVERPASS_GATE
        .get_or_init(|| Mutex::new(OverpassBudget::default()))
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    if budget.requests >= config.overpass_request_budget {
        return Err(StreamError::Configuration("Overpass session request budget exhausted; use a prepared world pack or your own data service for extensive exploration".into()));
    }
    if budget
        .downloaded_bytes
        .saturating_add(config.max_response_bytes)
        > 256 * 1024 * 1024
    {
        return Err(StreamError::Configuration(
            "Overpass session download budget exhausted (256 MiB); use your own world data service"
                .into(),
        ));
    }
    if let Some(previous) = budget.last {
        let elapsed = previous.elapsed();
        if elapsed < Duration::from_secs(2) {
            std::thread::sleep(Duration::from_secs(2) - elapsed);
        }
    }
    budget.requests += 1;
    let response = client(config, &config.overpass_url)?
        .post(&config.overpass_url)
        .form(&[("data", query)])
        .send();
    // Keep the gate until the entire body is received to enforce single-flight.
    let bytes = response
        .map_err(StreamError::from)
        .and_then(|r| body_limited(r, config));
    budget.downloaded_bytes = budget
        .downloaded_bytes
        .saturating_add(bytes.as_ref().map_or(config.max_response_bytes, Vec::len));
    budget.last = Some(Instant::now());
    drop(budget);
    parse_overpass(&bytes?).map(|features| crate::clip::to_tile(features, key))
}

/// Import tagged OSM points and ways. Relations are deliberately not flattened
/// into false polygons. Closed land-use/building/water areas, open roads and
/// waterways, trees and semantic shop/amenity nodes are kept with their tags.
pub fn parse_overpass(bytes: &[u8]) -> Result<Vec<WorldFeature>, StreamError> {
    let root: Value =
        serde_json::from_slice(bytes).map_err(|e| StreamError::Payload(e.to_string()))?;
    if let Some(remark) = root.get("remark").and_then(Value::as_str) {
        return Err(StreamError::Payload(format!(
            "Overpass returned a partial/error response: {remark}"
        )));
    }
    let elements = root
        .get("elements")
        .and_then(Value::as_array)
        .ok_or_else(|| StreamError::Payload("Overpass elements array missing".into()))?;
    if elements.len() > 50_000 {
        return Err(StreamError::Payload("OSM element budget exceeded".into()));
    }
    let mut output = Vec::new();
    let mut vertices = 0usize;
    for element in elements {
        let kind_name = element.get("type").and_then(Value::as_str).unwrap_or("");
        if !matches!(kind_name, "node" | "way") {
            continue;
        }
        let Some(id) = element.get("id").and_then(Value::as_u64) else {
            continue;
        };
        let Some(tags) = element.get("tags").and_then(Value::as_object) else {
            continue;
        };
        let tag = |name: &str| tags.get(name).and_then(Value::as_str).unwrap_or("");
        let kind = if kind_name == "node"
            && (!tag("shop").is_empty()
                || !tag("amenity").is_empty()
                || !tag("craft").is_empty()
                || !tag("tourism").is_empty())
        {
            FeatureKind::PointOfInterest
        } else if (!tag("building").is_empty() && tag("building") != "no")
            || (!tag("building:part").is_empty() && tag("building:part") != "no")
        {
            FeatureKind::Building
        } else if !tag("highway").is_empty() {
            FeatureKind::Road
        } else if tag("natural") == "water" || tag("waterway") == "riverbank" {
            FeatureKind::Water
        } else if !tag("waterway").is_empty() {
            FeatureKind::Waterway
        } else if tag("natural") == "tree" {
            FeatureKind::Tree
        } else if tag("natural") == "wood" || tag("landuse") == "forest" {
            FeatureKind::Forest
        } else if matches!(
            tag("landuse"),
            "farmland" | "farmyard" | "orchard" | "vineyard"
        ) {
            FeatureKind::Farm
        } else if matches!(tag("landuse"), "meadow" | "grass") {
            FeatureKind::Grass
        } else {
            continue;
        };
        let coordinate = |value: &Value| -> Option<GeoPosition> {
            GeoPosition::new(
                value.get("lat")?.as_f64()?,
                value.get("lon")?.as_f64()?,
                0.0,
            )
            .ok()
        };
        let points = if kind_name == "node" {
            coordinate(element).into_iter().collect::<Vec<_>>()
        } else {
            element
                .get("geometry")
                .and_then(Value::as_array)
                .and_then(|g| g.iter().map(coordinate).collect::<Option<Vec<_>>>())
                .unwrap_or_default()
        };
        if points.is_empty() || (kind_name == "way" && points.len() < 2) {
            continue;
        }
        if matches!(
            kind,
            FeatureKind::Building
                | FeatureKind::Water
                | FeatureKind::Forest
                | FeatureKind::Farm
                | FeatureKind::Grass
        ) && (points.len() < 4 || points.first() != points.last())
        {
            continue;
        }
        vertices = vertices.saturating_add(points.len());
        if vertices > 500_000 {
            return Err(StreamError::Payload(
                "OSM geometry vertex budget exceeded".into(),
            ));
        }
        let parsed_height =
            parse_osm_length(tag("height")).filter(|h| h.is_finite() && *h > 0.0 && *h < 2000.0);
        let levels = tag("building:levels")
            .parse::<f32>()
            .ok()
            .filter(|v| v.is_finite() && *v > 0.0 && *v < 400.0);
        let default_height = match kind {
            FeatureKind::Building => levels.unwrap_or(2.0) * 3.0,
            FeatureKind::Tree | FeatureKind::Forest => 8.0,
            FeatureKind::Road => match tag("highway") {
                "motorway" | "trunk" => 10.5,
                "primary" | "secondary" => 7.0,
                "tertiary" | "residential" => 6.0,
                "footway" | "path" | "cycleway" | "steps" => 2.0,
                _ => 4.0,
            },
            FeatureKind::Waterway => match tag("waterway") {
                "river" => 14.0,
                "canal" => 8.0,
                "ditch" | "drain" => 1.5,
                _ => 3.0,
            },
            _ => 0.0,
        };
        let mut properties: BTreeMap<String, String> = tags
            .iter()
            .filter_map(|(k, v)| v.as_str().map(|v| (k.clone(), v.to_string())))
            .collect();
        properties.insert("osm_type".into(), kind_name.into());
        properties.insert(
            "geometry".into(),
            if kind_name == "node" {
                "point"
            } else if points.first() == points.last() {
                "polygon"
            } else {
                "line"
            }
            .into(),
        );
        if parsed_height.is_none() && matches!(kind, FeatureKind::Building | FeatureKind::Tree) {
            properties.insert(
                "height_source".into(),
                "inferred/default; not measured".into(),
            );
        }
        let height_m = if matches!(kind, FeatureKind::Road | FeatureKind::Waterway) {
            // Legacy WorldFeature height_m stores the rendered width for linear features.
            let lanes = tag("lanes")
                .parse::<f32>()
                .ok()
                .filter(|n| n.is_finite() && (1.0..=12.0).contains(n));
            let measured = parse_osm_length(tag("width")).filter(|n| (0.3..=100.0).contains(n));
            if measured.is_none() {
                properties.insert(
                    "width_source".into(),
                    "inferred from lanes/class; not measured".into(),
                );
            }
            measured
                .or_else(|| {
                    if kind == FeatureKind::Road {
                        lanes.map(|n| n * 3.1)
                    } else {
                        None
                    }
                })
                .unwrap_or(default_height)
        } else {
            parsed_height.unwrap_or(default_height)
        };
        output.push(WorldFeature {
            id: format!("osm:{kind_name}:{id}"),
            kind,
            points,
            height_m,
            properties,
        });
    }
    Ok(output)
}

/// OSM lengths use metres unless a supported unit is explicit. Restrictions such
/// as `maxwidth` are deliberately not used as physical geometry dimensions.
pub fn parse_osm_length(input: &str) -> Option<f32> {
    let value = input.trim();
    let (digits, factor) = if let Some(s) = value.strip_suffix(" ft") {
        (s, 0.3048)
    } else if let Some(s) = value.strip_suffix(" cm") {
        (s, 0.01)
    } else if let Some(s) = value.strip_suffix('m') {
        (s, 1.0)
    } else {
        (value, 1.0)
    };
    let n = digits.trim().parse::<f32>().ok()? * factor;
    n.is_finite().then_some(n)
}

/// Fetch current live conditions, cached per 0.05° cell for ten minutes by default.
/// "Current" refers to provider wall-clock time, independent of the game clock.
/// To replay historical dates inject recorded weather through game state instead.
pub fn fetch_weather(
    position: GeoPosition,
    config: &StreamConfig,
) -> Result<WeatherSample, StreamError> {
    config.validate()?;
    GeoPosition::new(position.latitude, position.longitude, position.altitude_m)
        .map_err(|e| StreamError::Geography(e.to_string()))?;
    if !config.live || !config.weather {
        let clock = WorldClock {
            unix_seconds: cache::now() as f64,
            ..Default::default()
        };
        return Ok(synthetic_weather(&position, &clock, config.seed));
    }
    let latitude = (position.latitude * 20.0).round() / 20.0;
    let longitude = terra_core::normalize_longitude(
        (terra_core::normalize_longitude(position.longitude) * 20.0).round() / 20.0,
    );
    let mut url = reqwest::Url::parse(&config.weather_url)
        .map_err(|e| StreamError::Configuration(e.to_string()))?;
    url.query_pairs_mut()
        .append_pair("latitude", &latitude.to_string())
        .append_pair("longitude", &longitude.to_string())
        .append_pair(
            "current",
            "temperature_2m,precipitation,cloud_cover,wind_speed_10m,wind_direction_10m",
        )
        .append_pair("wind_speed_unit", "ms")
        .append_pair("timeformat", "unixtime")
        .append_pair("timezone", "UTC");
    let url = url.to_string();
    let key = format!("weather-v2:{url}");
    if let Some(bytes) = cache::read(config, &key, config.weather_cache_ttl_seconds) {
        if let Ok(sample) = parse_open_meteo(&bytes) {
            return Ok(sample);
        }
    }
    let bytes = get(&url, config)?;
    let sample = parse_open_meteo(&bytes)?;
    let _ = cache::write(config, &key, &bytes);
    Ok(sample)
}

/// Strictly parse expected provider units; absent/null fields are errors, not
/// fabricated zero weather. Current precipitation is converted from the reported
/// interval accumulation into a per-hour equivalent rate.
pub fn parse_open_meteo(bytes: &[u8]) -> Result<WeatherSample, StreamError> {
    let root: Value =
        serde_json::from_slice(bytes).map_err(|e| StreamError::Payload(e.to_string()))?;
    let units = root
        .get("current_units")
        .ok_or_else(|| StreamError::Payload("weather units missing".into()))?;
    for (name, expected) in [
        ("time", "unixtime"),
        ("interval", "seconds"),
        ("temperature_2m", "°C"),
        ("precipitation", "mm"),
        ("cloud_cover", "%"),
        ("wind_speed_10m", "m/s"),
        ("wind_direction_10m", "°"),
    ] {
        if units.get(name).and_then(Value::as_str) != Some(expected) {
            return Err(StreamError::Payload(format!(
                "unexpected unit for {name}; expected {expected}"
            )));
        }
    }
    let current = root
        .get("current")
        .ok_or_else(|| StreamError::Payload("current weather missing".into()))?;
    let number = |name: &str| {
        current
            .get(name)
            .and_then(Value::as_f64)
            .filter(|v| v.is_finite())
            .ok_or_else(|| {
                StreamError::Payload(format!("weather field {name} missing/null/non-finite"))
            })
    };
    let interval = number("interval")?;
    if !(1.0..=3600.0).contains(&interval) {
        return Err(StreamError::Payload(
            "unexpected current weather interval".into(),
        ));
    }
    let temperature = number("temperature_2m")?;
    let precipitation = number("precipitation")?;
    let cloud = number("cloud_cover")?;
    let wind = number("wind_speed_10m")?;
    let direction = number("wind_direction_10m")?;
    if !(-100.0..=70.0).contains(&temperature)
        || !(0.0..=500.0).contains(&precipitation)
        || !(0.0..=100.0).contains(&cloud)
        || !(0.0..=200.0).contains(&wind)
        || !(0.0..=360.0).contains(&direction)
    {
        return Err(StreamError::Payload(
            "weather value outside physical input limits".into(),
        ));
    }
    let precipitation_rate = precipitation * 3600.0 / interval;
    if precipitation_rate > 1_000.0 {
        return Err(StreamError::Payload(
            "derived precipitation rate exceeds input limit".into(),
        ));
    }
    let timestamp = current
        .get("time")
        .and_then(Value::as_i64)
        .ok_or_else(|| StreamError::Payload("weather Unix timestamp missing".into()))?;
    Ok(WeatherSample {
        temperature_c: temperature as f32,
        precipitation_mm_h: precipitation_rate as f32,
        cloud_cover: (cloud / 100.0) as f32,
        wind_speed_m_s: wind as f32,
        wind_direction_deg: direction as f32,
        observed_unix_seconds: timestamp,
        synthetic: false,
    })
}
