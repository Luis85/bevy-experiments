use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::Read,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};
use terra_bevy::{WorldConfig, render::RenderQualitySettings, rules::CharacterProfile};
use terra_core::{GeoPosition, Layer};

pub const MAX_PROFILE_BYTES: u64 = 256 * 1024;
pub const MAX_RECENT_WORLDS: usize = 24;
const START_WITNESS: &str = "world-started.json";
const SESSION_LEASE: &str = "world-session.lock";
const SESSION_LEASE_MAGIC: &[u8] = b"Terra managed session lease v1\n";

fn region_band(radius: u32) -> &'static str {
    match radius {
        0 => "Source area",
        1 => "Near",
        2 => "Local",
        3 => "Region",
        _ => "Wide",
    }
}

/// Approximate east–west and north–south dimensions of the actual geographic
/// streaming window. Longitude wraps and latitude clips just like TileKey.
fn region_span_m(focus: GeoPosition, level: u8, radius: u32) -> Option<(f64, f64)> {
    let key = terra_core::TileKey::at(focus, level).ok()?;
    let side = 1_u32 << level;
    let radius = radius.min(64);
    let columns = (radius * 2 + 1).min(side);
    let first_row = key.y.saturating_sub(radius);
    let last_row = (key.y + radius).min(side - 1);
    let rows = last_row - first_row + 1;
    let mid_latitude =
        90. - 180. * (f64::from(first_row) + f64::from(rows) * 0.5) / f64::from(side);
    let width = terra_core::MEAN_EARTH_RADIUS_M * std::f64::consts::TAU * f64::from(columns)
        / f64::from(side)
        * mid_latitude.to_radians().cos().abs();
    let height =
        terra_core::MEAN_EARTH_RADIUS_M * std::f64::consts::PI * f64::from(rows) / f64::from(side);
    Some((width, height))
}

pub(crate) fn region_coverage_label(focus: GeoPosition, level: u8, radius: u32) -> String {
    let band = region_band(radius);
    match region_span_m(focus, level, radius) {
        Some((width, height)) if width.max(height) >= 1_000. => {
            format!(
                "{band} · ≈{:.1} × {:.1} km",
                width / 1_000.,
                height / 1_000.
            )
        }
        Some((width, height)) => format!("{band} · ≈{width:.0} × {height:.0} m"),
        None => format!("{band} · Enter valid coordinates"),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Preset {
    #[default]
    Frontier,
    Heightmap,
    Historical1950,
    Historical1980,
}
impl Preset {
    pub fn next(self) -> Self {
        match self {
            Self::Frontier => Self::Heightmap,
            Self::Heightmap => Self::Historical1950,
            Self::Historical1950 => Self::Historical1980,
            Self::Historical1980 => Self::Frontier,
        }
    }
    pub fn title(self) -> &'static str {
        match self {
            Self::Frontier => "Living frontier",
            Self::Heightmap => "Heightmap valley",
            Self::Historical1950 => "Historical scenario · 1950",
            Self::Historical1980 => "Historical scenario · 1980",
        }
    }
    pub fn description(self) -> &'static str {
        match self {
            Self::Frontier => {
                "A reproducible synthetic world at real GPS coordinates. Roads, settlement, wildlife, industry and driving jobs are authored scenario content."
            }
            Self::Heightmap => {
                "An authored georeferenced heightmap with wooded hills, fields and a settlement. Local data, no downloads. The terrain is synthetic, not a survey of Munich."
            }
            Self::Historical1950 | Self::Historical1980 => {
                "A deliberately fictional dated terrain and political scenario. Its data coverage is bounded; changing the year does not reconstruct real history."
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct Draft {
    pub world_name: String,
    pub character: CharacterProfile,
    pub latitude: String,
    pub longitude: String,
    pub seed: String,
    pub preset: Preset,
    pub radius: u32,
    pub time_scale: u32,
    pub start_hour: u8,
    pub day_night: bool,
    pub seasons: bool,
    pub weather: bool,
    pub vegetation: bool,
    pub regional_storage: bool,
    pub quality: RenderQualitySettings,
    pub address: String,
    pub session_name: String,
    pub tls: bool,
    pub tls_ca: String,
    pub tls_server_name: String,
    pub allow_insecure_remote: bool,
    pub source_config: String,
}
impl Default for Draft {
    fn default() -> Self {
        Self {
            world_name: "My frontier".into(),
            character: CharacterProfile {
                name: "Alex".into(),
                body_style: 0,
                skin: 2,
                hair: 1,
                outfit: 0,
            },
            latitude: "48.137".into(),
            longitude: "11.575".into(),
            seed: "42".into(),
            preset: Preset::Frontier,
            radius: 3,
            time_scale: 60,
            start_hour: 12,
            day_night: true,
            seasons: true,
            weather: true,
            vegetation: true,
            regional_storage: true,
            quality: RenderQualitySettings::default(),
            address: "127.0.0.1:7777".into(),
            session_name: "driver".into(),
            tls: false,
            tls_ca: String::new(),
            tls_server_name: "localhost".into(),
            allow_insecure_remote: false,
            source_config: String::new(),
        }
    }
}

impl Draft {
    pub fn requested_coverage_label(&self) -> String {
        let focus = self
            .latitude
            .trim()
            .parse::<f64>()
            .ok()
            .zip(self.longitude.trim().parse::<f64>().ok())
            .and_then(|(latitude, longitude)| GeoPosition::new(latitude, longitude, 0.).ok());
        focus
            .map(|focus| {
                region_coverage_label(focus, WorldConfig::default().chunk_level, self.radius)
            })
            .unwrap_or_else(|| format!("{} · Enter valid coordinates", region_band(self.radius)))
    }
}

pub fn validate_name(value: &str, max: usize, label: &str) -> Result<(), String> {
    let count = value.trim().chars().count();
    if count == 0 || count > max || value.chars().any(char::is_control) {
        return Err(format!("{label} needs 1–{max} visible characters."));
    }
    Ok(())
}

pub fn validate_character(profile: &CharacterProfile) -> Result<(), String> {
    validate_name(&profile.name, 32, "Character name")?;
    if profile.body_style > 2 || profile.skin > 7 || profile.hair > 7 || profile.outfit > 7 {
        return Err("Choose one of the available character appearances.".into());
    }
    Ok(())
}

impl Draft {
    pub fn config(&self, project_root: &Path) -> Result<WorldConfig, String> {
        validate_name(&self.world_name, 48, "World name")?;
        validate_character(&self.character)?;
        let latitude: f64 = self
            .latitude
            .trim()
            .parse()
            .map_err(|_| "Latitude must be a number between -90 and 90.")?;
        let longitude: f64 = self
            .longitude
            .trim()
            .parse()
            .map_err(|_| "Longitude must be a number between -180 and 180.")?;
        let position = GeoPosition::new(latitude, longitude, 0.).map_err(|e| e.to_string())?;
        let seed = self
            .seed
            .trim()
            .parse::<u64>()
            .map_err(|_| "Seed must be a whole number from 0 to 18446744073709551615.")?;
        let mut config = match self.preset {
            Preset::Historical1950 => {
                read_json::<WorldConfig>(&project_root.join("configs/historical-1950.json"))?
            }
            Preset::Historical1980 => {
                read_json::<WorldConfig>(&project_root.join("configs/historical-1980.json"))?
            }
            _ => WorldConfig::default(),
        };
        config.focus = position;
        config.seed = seed;
        config.chunk_radius = self.radius;
        config.max_resident_chunks = config
            .max_resident_chunks
            .max(((self.radius * 2 + 1).pow(2)) as usize);
        config.max_queued = config.max_queued.max(config.max_resident_chunks);
        config.time_scale = f64::from(self.time_scale);
        config.day_night = self.day_night;
        config.seasons = self.seasons;
        config.live_data = false;
        config.live_weather = false;
        config.layers.set(Layer::Weather, self.weather);
        config.layers.set(Layer::Vegetation, self.vegetation);
        config.cache_dir = project_root.join(".terra-cache");
        if self.preset == Preset::Heightmap {
            config.heightmap_manifest = Some(
                project_root.join("crates/terra-stream/data/heightmap-connected/manifest.json"),
            );
        }
        if config.politics_file.is_none() && config.historical_pack.is_none() {
            config.politics_file = Some(
                project_root
                    .join("crates/terra-history/data/natural-earth-countries-2026-10-26.geojson"),
            );
        }
        for path in [
            &mut config.historical_pack,
            &mut config.heightmap_manifest,
            &mut config.politics_file,
        ] {
            if let Some(value) = path {
                if value.is_relative() {
                    *value = project_root.join(&*value);
                }
                *value = value.canonicalize().map_err(|e| {
                    format!(
                        "Required world source is unavailable: {} ({e})",
                        value.display()
                    )
                })?;
            }
        }
        if let Some(path) = &config.heightmap_manifest {
            let provider =
                terra_stream::PreparedHeightmapProvider::open(path, config.terrain_resolution)
                    .map_err(|e| e.to_string())?;
            let key = terra_core::TileKey::at(config.focus, config.chunk_level)
                .map_err(|e| e.to_string())?;
            if !provider.covers(key) {
                return Err("These coordinates are outside the bundled heightmap. Use 48.137, 11.575 or choose Living frontier.".into());
            }
            while config.chunk_radius > 0
                && !key
                    .neighbors(config.chunk_radius)
                    .iter()
                    .all(|key| provider.covers(*key))
            {
                config.chunk_radius -= 1;
            }
            config.base_data_revision = provider.source_revision().to_owned();
        }
        if let Some(path) = &config.historical_pack {
            let pack =
                terra_bevy::history::HistoricalPack::load(path).map_err(|e| e.to_string())?;
            let keys = pack.keys();
            let epoch = config
                .data_epoch_unix
                .ok_or("Historical scenario has no data epoch.")?;
            let first = pack.chunk_ref(keys[0], epoch).map_err(|e| e.to_string())?;
            config.chunk_level = keys[0].level;
            config.terrain_resolution = first.heights.width;
            let key = terra_core::TileKey::at(config.focus, config.chunk_level)
                .map_err(|e| e.to_string())?;
            pack.chunk_ref(key, epoch).map_err(|_| "These coordinates are outside this dated scenario. Use 48.137, 11.575 or choose Living frontier.")?;
            while config.chunk_radius > 0
                && !key
                    .neighbors(config.chunk_radius)
                    .iter()
                    .all(|key| keys.contains(key))
            {
                config.chunk_radius -= 1;
            }
            config.base_data_revision = pack.manifest.id;
        }
        if let Some(path) = &config.politics_file {
            config.political_dataset_revision = Some(
                terra_bevy::history::PoliticsDataset::load(path)
                    .map_err(|e| e.to_string())?
                    .id,
            );
        }
        config.validate()?;
        Ok(config)
    }

    pub fn join_arguments(&self, user_data: &Path) -> Result<Vec<String>, String> {
        let address =
            self.address.trim().parse::<SocketAddr>().map_err(
                |_| "Use an IP address and port, for example 127.0.0.1:7777 or [::1]:7777.",
            )?;
        let session = self.session_name.trim();
        if session.is_empty()
            || session.len() > 32
            || !session
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
        {
            return Err("Session name needs 1–32 letters, numbers, hyphens or underscores. Use a separate name for each player.".into());
        }
        let security = if self.tls {
            let path = Path::new(self.tls_ca.trim())
                .canonicalize()
                .map_err(|_| "Choose a readable CA certificate PEM file for this server.")?;
            let metadata = fs::metadata(&path).map_err(|e| e.to_string())?;
            if !metadata.is_file() || metadata.len() == 0 || metadata.len() > 1024 * 1024 {
                return Err(
                    "The CA certificate must be a regular PEM file from 1 byte to 1 MiB.".into(),
                );
            }
            let server_name = self.tls_server_name.trim();
            if server_name.is_empty()
                || server_name.len() > 253
                || server_name
                    .chars()
                    .any(|c| c.is_control() || c.is_whitespace())
            {
                return Err(
                    "Enter the hostname on the server certificate, for example game.example.org."
                        .into(),
                );
            }
            Some((path, server_name.to_owned()))
        } else {
            if !address.ip().is_loopback() && !self.allow_insecure_remote {
                return Err("For a remote server, enable verified TLS or explicitly allow an unencrypted trusted-network connection.".into());
            }
            None
        };
        let source_config = if self.source_config.trim().is_empty() {
            None
        } else {
            let path = Path::new(self.source_config.trim())
                .canonicalize()
                .map_err(
                    |_| "Choose a readable local WorldConfig JSON for the server's source bundle.",
                )?;
            let config: WorldConfig = read_json(&path)?;
            config.validate()?;
            if config.live_data || config.region_store.is_some() {
                return Err("A multiplayer source configuration must use fixed local source data and leave regional storage to the host.".into());
            }
            if let Some(path) = &config.heightmap_manifest {
                let provider =
                    terra_stream::PreparedHeightmapProvider::open(path, config.terrain_resolution)
                        .map_err(|e| e.to_string())?;
                let key = terra_core::TileKey::at(config.focus, config.chunk_level)
                    .map_err(|e| e.to_string())?;
                if !provider.covers(key) {
                    return Err(
                        "The source configuration's focus tile is outside its heightmap coverage."
                            .into(),
                    );
                }
            }
            if let Some(path) = &config.historical_pack {
                terra_bevy::history::HistoricalPack::load(path).map_err(|e| e.to_string())?;
            }
            if let Some(path) = &config.politics_file {
                terra_bevy::history::PoliticsDataset::load(path).map_err(|e| e.to_string())?;
            }
            Some(path)
        };
        private_directory(user_data)?;
        let sessions = user_data.join("sessions");
        private_directory(&sessions)?;
        let mut arguments = vec![
            "--play".into(),
            "--connect".into(),
            address.to_string(),
            "--session".into(),
            sessions
                .join(format!("{session}.json"))
                .to_string_lossy()
                .into_owned(),
            "--quality".into(),
            self.quality.quality.label().to_lowercase(),
            "--camp-view".into(),
            "--user-data".into(),
            user_data.to_string_lossy().into_owned(),
        ];
        if !self.quality.atmospheric_sky {
            arguments.push("--no-atmosphere".into());
        }
        if let Some(path) = source_config {
            arguments.extend(["--config".into(), path.to_string_lossy().into_owned()]);
        }
        if let Some((path, server_name)) = security {
            arguments.extend([
                "--tls-ca".into(),
                path.to_string_lossy().into_owned(),
                "--tls-server-name".into(),
                server_name,
            ]);
        } else if self.allow_insecure_remote {
            arguments.push("--allow-insecure-remote".into());
        }
        Ok(arguments)
    }

    pub fn restore_connection(&mut self, user_data: &Path) -> Option<String> {
        let path = user_data.join("last-connection.json");
        if !path.exists() {
            return None;
        }
        match read_json::<ConnectionSettings>(&path) {
            Ok(saved) => {
                self.address = saved.address;
                self.session_name = saved.session_name;
                self.tls = saved.tls;
                self.tls_ca = saved.tls_ca;
                self.tls_server_name = saved.tls_server_name;
                self.allow_insecure_remote = saved.allow_insecure_remote;
                self.source_config = saved.source_config;
                None
            }
            Err(error) => Some(format!(
                "Previous connection settings could not be read: {error}"
            )),
        }
    }

    pub fn save_connection(&self, user_data: &Path) -> Result<(), String> {
        private_directory(user_data)?;
        atomic_json(
            &user_data.join("last-connection.json"),
            &ConnectionSettings {
                address: self.address.trim().into(),
                session_name: self.session_name.trim().into(),
                tls: self.tls,
                tls_ca: self.tls_ca.trim().into(),
                tls_server_name: self.tls_server_name.trim().into(),
                allow_insecure_remote: self.allow_insecure_remote,
                source_config: self.source_config.trim().into(),
            },
        )
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConnectionSettings {
    address: String,
    session_name: String,
    tls: bool,
    tls_ca: String,
    tls_server_name: String,
    allow_insecure_remote: bool,
    #[serde(default)]
    source_config: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorldProfile {
    pub schema_version: u32,
    pub title: String,
    pub created_unix_seconds: u64,
    #[serde(default)]
    pub last_played_unix_seconds: u64,
    #[serde(default)]
    pub checkpoint_present: bool,
    pub config: WorldConfig,
    pub character: CharacterProfile,
    pub quality: RenderQualitySettings,
    pub start_hour: u8,
}
impl WorldProfile {
    pub fn read(path: &Path) -> Result<Self, String> {
        let profile: Self = read_json(path)?;
        if profile.schema_version != 1 {
            return Err("This world profile uses an unsupported version.".into());
        }
        validate_name(&profile.title, 48, "World name")?;
        validate_character(&profile.character)?;
        if profile.start_hour > 23 {
            return Err("Initial hour must be between 0 and 23.".into());
        }
        profile.config.validate()?;
        let progress = profile
            .config
            .region_store
            .as_ref()
            .unwrap_or(&profile.config.save_path);
        let started = read_start_witness(path, &profile)?.is_some();
        if (profile.checkpoint_present || started)
            && !fs::symlink_metadata(progress).is_ok_and(|metadata| {
                metadata.is_file() && !metadata.file_type().is_symlink() && metadata.len() > 0
            })
        {
            return Err(if profile.checkpoint_present {
                "This world previously saved progress, but its progress file is missing or invalid. Restore its backup before continuing; a new empty world has not been created."
            } else {
                "This world was previously started, but its progress file is missing or invalid. Its first initialization may have been interrupted. Restore a verified backup or create a separate new world; a new empty world has not been created."
            }.into());
        }
        if let Some(store) = &profile.config.region_store {
            terra_bevy::regions::validate_output_path(store, path).map_err(|e| e.to_string())?;
        }
        // A world save must never replace the profile used to locate it. The
        // same alias-aware guard protects existing hard links and symlinks.
        terra_bevy::regions::validate_output_path(path, &profile.config.save_path)
            .map_err(|e| e.to_string())?;
        Ok(profile)
    }
    pub fn source_label(&self) -> &'static str {
        if self.config.historical_pack.is_some() {
            "Dated scenario"
        } else if self.config.heightmap_manifest.is_some() {
            "Authored heightmap"
        } else if self.config.live_data {
            "Live providers"
        } else {
            "Synthetic frontier"
        }
    }
}

/// This record witnesses admission to initialization, never a successful save.
/// Publishing it before any world write closes the crash window between the
/// first committed checkpoint and a later profile/presentation-settings update.
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WorldStartWitness {
    schema_version: u32,
    profile_path: PathBuf,
    progress_path: PathBuf,
    regional: bool,
    identity: terra_bevy::sandbox::WorldIdentity,
}
impl WorldStartWitness {
    fn for_config(profile_path: &Path, config: &WorldConfig) -> Result<Self, String> {
        Ok(Self {
            schema_version: 1,
            profile_path: profile_path.canonicalize().map_err(|e| e.to_string())?,
            progress_path: std::path::absolute(
                config.region_store.as_ref().unwrap_or(&config.save_path),
            )
            .map_err(|e| e.to_string())?,
            regional: config.region_store.is_some(),
            identity: terra_bevy::sandbox::WorldIdentity::from(config),
        })
    }
}

/// The initialization witness is also a protected output boundary before it
/// exists. Export preflight and runtime writers must reserve the same pathname.
pub(crate) fn managed_start_witness_path(profile_path: &Path) -> PathBuf {
    profile_path.with_file_name(START_WITNESS)
}

fn start_witness_path(profile_path: &Path, profile: &WorldProfile) -> Result<PathBuf, String> {
    let path = managed_start_witness_path(profile_path);
    for protected in [
        Some(profile_path),
        Some(profile.config.save_path.as_path()),
        profile.config.region_store.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        terra_bevy::regions::validate_output_path(protected, &path).map_err(|_| {
            "The world initialization record would occupy a protected profile or progress file."
                .to_owned()
        })?;
    }
    Ok(path)
}

fn read_start_witness(
    profile_path: &Path,
    profile: &WorldProfile,
) -> Result<Option<WorldStartWitness>, String> {
    let path = start_witness_path(profile_path, profile)?;
    match fs::symlink_metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
        Ok(_) => (),
    }
    let witness: WorldStartWitness = read_json(&path)
        .map_err(|error| format!("The world initialization record could not be read: {error}"))?;
    if witness != WorldStartWitness::for_config(profile_path, &profile.config)? {
        return Err("The world initialization record belongs to another profile, source or progress path. Restore a verified backup or create a separate new world; existing data was not changed.".into());
    }
    Ok(Some(witness))
}

fn publish_start_witness(path: &Path, witness: &WorldStartWitness) -> Result<(), String> {
    publish_start_witness_with_sync(
        path,
        witness,
        terra_bevy::rules::publication::sync_directory,
    )
}

fn publish_start_witness_with_sync(
    path: &Path,
    witness: &WorldStartWitness,
    sync_parent: impl FnOnce(&Path) -> std::io::Result<()>,
) -> Result<(), String> {
    write_new_json_with_sync(path, witness, sync_parent).map_err(|error| {
        if error.published() {
            format!("The initialization record was published but its durability could not be confirmed: {error}. This launch did not initialize progress; keep the record and restore a verified backup or create a separate world.")
        } else {
            format!("World initialization could not be claimed exclusively: {error}. Another launch may already have started it; progress has not been initialized by this attempt.")
        }
    })
}

pub(crate) fn managed_session_lease_path(profile: &Path) -> PathBuf {
    profile.with_file_name(SESSION_LEASE)
}

/// One cooperative writer for a managed world. Keep this value alive through
/// the final save and profile update. The stable lock pathname is never unlinked:
/// closing this handle (including process death) releases OS ownership.
pub struct ManagedSessionLease {
    _file: fs::File,
    pub require_existing_progress: bool,
}

/// Coordinate a metadata or backup operation with managed play and other
/// maintenance. This guard does not admit a launch, create a start witness or
/// initialize progress. Keep it until the operation's final publication returns.
pub(super) struct ManagedMaintenanceLease {
    _file: fs::File,
}

pub(super) fn begin_managed_maintenance(
    profile_path: &Path,
    config: &WorldConfig,
) -> Result<ManagedMaintenanceLease, String> {
    Ok(ManagedMaintenanceLease {
        _file: acquire_managed_lease(profile_path, config)?,
    })
}

fn acquire_managed_lease(profile_path: &Path, config: &WorldConfig) -> Result<fs::File, String> {
    use terra_bevy::rules::publication::{PublicationError, PublishMode, write_private};
    let path = managed_session_lease_path(profile_path);
    let witness = managed_start_witness_path(profile_path);
    for protected in [
        Some(profile_path),
        Some(witness.as_path()),
        Some(config.save_path.as_path()),
        config.region_store.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        terra_bevy::regions::validate_output_path(protected, &path).map_err(|_| {
            "The managed session lock would occupy protected world data.".to_owned()
        })?;
    }
    match write_private(&path, SESSION_LEASE_MAGIC, PublishMode::CreateNew) {
        Ok(()) => (),
        Err(PublicationError::NotPublished(error))
            if error.kind() == std::io::ErrorKind::AlreadyExists =>
        {
            ()
        }
        Err(error) => {
            return Err(format!(
                "The managed session lock could not be created: {error}"
            ));
        }
    }
    let metadata = fs::symlink_metadata(&path).map_err(|error| error.to_string())?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() != SESSION_LEASE_MAGIC.len() as u64
    {
        return Err("The managed session lock is not a valid Terra lock file. Keep the world files and inspect this path before reopening.".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.nlink() != 1 {
            return Err("The managed session lock cannot be a hard-linked file.".into());
        }
    }
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
        .map_err(|error| format!("The managed session lock could not be opened: {error}"))?;
    file.try_lock().map_err(|error| match error {
        fs::TryLockError::WouldBlock => "This world is already open in another session or a maintenance operation. Close that session or wait for maintenance to finish before continuing; its progress has not been changed by this attempt.".to_owned(),
        fs::TryLockError::Error(error) => format!("The managed world could not be locked: {error}"),
    })?;
    let mut marker = Vec::new();
    (&file)
        .take(SESSION_LEASE_MAGIC.len() as u64 + 1)
        .read_to_end(&mut marker)
        .map_err(|error| error.to_string())?;
    if marker != SESSION_LEASE_MAGIC {
        return Err("The managed session lock is not a valid Terra lock file; existing files were left in place.".into());
    }
    private_permissions(&path, false)?;
    // The lock is coordination metadata, not proof of a checkpoint. Reconfirm
    // its namespace on every acquisition, including after uncertain creation.
    terra_bevy::rules::publication::sync_directory(path.parent().unwrap())
        .map_err(|error| format!("The managed session lock could not be synchronized: {error}"))?;
    Ok(file)
}

pub fn begin_managed_session(
    profile_path: &Path,
    resolved: &WorldConfig,
) -> Result<ManagedSessionLease, String> {
    let file = acquire_managed_lease(profile_path, resolved)?;
    let require_existing_progress = prepare_managed_launch(profile_path, resolved)?;
    Ok(ManagedSessionLease {
        _file: file,
        require_existing_progress,
    })
}

/// Admit one managed launch after source preparation but before the runtime can
/// write progress. `false` is a one-use in-memory first-initialization admission;
/// `true` requires an existing validated checkpoint at the actual startup barrier.
fn prepare_managed_launch(profile_path: &Path, resolved: &WorldConfig) -> Result<bool, String> {
    let profile = WorldProfile::read(profile_path)?;
    let expected = WorldStartWitness::for_config(profile_path, &profile.config)?;
    if expected != WorldStartWitness::for_config(profile_path, resolved)? {
        return Err("The prepared world differs from its selected profile. Review the source and progress paths again before starting.".into());
    }
    let previous = read_start_witness(profile_path, &profile)?.is_some();
    let progress = resolved
        .region_store
        .as_ref()
        .unwrap_or(&resolved.save_path);
    let exists = match fs::symlink_metadata(progress) {
        Ok(_) => true,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => return Err(error.to_string()),
    };
    let require_existing = previous || profile.checkpoint_present || exists;
    if require_existing {
        let catalog = terra_bevy::rules::Catalog::builtin();
        let admission = if resolved.region_store.is_some() {
            terra_bevy::regions::validate_local_checkpoint(resolved, &catalog).map(|_| ())
        } else {
            (|| {
                let metadata = fs::symlink_metadata(progress).map_err(|e| e.to_string())?;
                if !metadata.is_file() || metadata.file_type().is_symlink() {
                    return Err("Managed JSON progress must be a regular file.".into());
                }
                terra_bevy::sandbox::LocalWorldSave::read(progress, resolved, &catalog).map(|_| ())
            })()
        };
        admission.map_err(|error| format!("This managed world's existing checkpoint cannot be restored: {error}. Restore a verified backup or create a separate new world; no fresh world was initialized."))?;
    }
    if !previous {
        publish_start_witness(&start_witness_path(profile_path, &profile)?, &expected)?;
    }
    Ok(require_existing)
}

#[derive(Debug, Clone)]
pub struct RecentWorld {
    pub path: PathBuf,
    pub profile: WorldProfile,
    pub has_checkpoint_file: bool,
}

pub(super) fn has_checkpoint_file(profile: &WorldProfile) -> bool {
    fs::symlink_metadata(
        profile
            .config
            .region_store
            .as_ref()
            .unwrap_or(&profile.config.save_path),
    )
    .is_ok_and(|metadata| {
        metadata.is_file() && !metadata.file_type().is_symlink() && metadata.len() > 0
    })
}

pub fn recent_worlds(user_data: &Path) -> (Vec<RecentWorld>, Vec<String>) {
    let mut worlds = Vec::new();
    let mut warnings = Vec::new();
    let folder = user_data.join("worlds");
    let entries = match fs::read_dir(&folder) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return (worlds, warnings),
        Err(error) => {
            warnings.push(format!("Your worlds could not be read: {error}"));
            return (worlds, warnings);
        }
    };
    for (index, entry) in entries.take(257).enumerate() {
        if index == 256 {
            warnings.push("Only 256 world folders were inspected. Move archived worlds to a separate folder to show more recent worlds.".into());
            break;
        }
        let Ok(entry) = entry else {
            continue;
        };
        let path = entry.path().join("world.json");
        if !path.exists() {
            continue;
        }
        match WorldProfile::read(&path) {
            Ok(profile) => {
                let has_checkpoint_file = has_checkpoint_file(&profile);
                worlds.push(RecentWorld {
                    path,
                    profile,
                    has_checkpoint_file,
                });
            }
            Err(error) => warnings.push(format!(
                "Skipped {}: {error}",
                entry.file_name().to_string_lossy()
            )),
        }
    }
    worlds.sort_by(|a, b| {
        b.profile
            .last_played_unix_seconds
            .max(b.profile.created_unix_seconds)
            .cmp(
                &a.profile
                    .last_played_unix_seconds
                    .max(a.profile.created_unix_seconds),
            )
            .then_with(|| b.path.cmp(&a.path))
    });
    worlds.truncate(MAX_RECENT_WORLDS);
    warnings.truncate(4);
    (worlds, warnings)
}

pub fn create_world(
    draft: &Draft,
    project_root: &Path,
    user_data: &Path,
) -> Result<PathBuf, String> {
    create_world_with_sync(
        draft,
        project_root,
        user_data,
        &terra_bevy::rules::publication::sync_directory,
    )
}

fn create_world_with_sync(
    draft: &Draft,
    project_root: &Path,
    user_data: &Path,
    sync: &dyn Fn(&Path) -> std::io::Result<()>,
) -> Result<PathBuf, String> {
    let mut config = draft.config(project_root)?;
    private_directory_with_sync(user_data, sync)?;
    let folder = user_data.join("worlds");
    private_directory_with_sync(&folder, sync)?;
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "System clock is before 1970.")?;
    let mut destination = None;
    for _ in 0..16 {
        let candidate = folder.join(format!(
            "world-{}-{}-{}",
            now.as_millis(),
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        match builder.create(&candidate) {
            Ok(()) => {
                // This exclusive claim precedes profile publication. Flush its
                // entry in worlds too; syncing world.json's parent alone cannot
                // acknowledge the new containing directory.
                if let Err(error) = sync(&candidate).and_then(|()| sync(&folder)) {
                    let _ = fs::remove_dir(&candidate);
                    return Err(format!(
                        "New world was not created: its folder could not be synchronized: {error}"
                    ));
                }
                destination = Some(candidate);
                break;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(format!("Could not create a new world folder: {error}")),
        }
    }
    let destination = destination.ok_or("Could not allocate a new world folder. Try again.")?;
    let destination = destination.canonicalize().map_err(|e| e.to_string())?;
    config.region_store = draft.regional_storage.then(|| destination.join("world.db"));
    config.save_path = destination.join("world-export.json");
    let profile = WorldProfile {
        schema_version: 1,
        title: draft.world_name.trim().into(),
        created_unix_seconds: now.as_secs(),
        last_played_unix_seconds: now.as_secs(),
        checkpoint_present: false,
        config,
        character: CharacterProfile {
            name: draft.character.name.trim().into(),
            ..draft.character.clone()
        },
        quality: draft.quality,
        start_hour: draft.start_hour,
    };
    let path = destination.join("world.json");
    if let Err(error) = write_new_json_with_sync(&path, &profile, sync) {
        if !error.published() {
            let _ = fs::remove_dir(&destination);
        }
        return Err(error.to_string());
    }
    Ok(path)
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct Preferences {
    pub quality: RenderQualitySettings,
    pub playtest_reports: bool,
}
impl Preferences {
    pub fn read(user_data: &Path) -> (Self, Option<String>) {
        let path = user_data.join("preferences.json");
        if !path.exists() {
            return (Self::default(), None);
        }
        match read_json(&path) {
            Ok(value) => (value, None),
            Err(error) => (
                Self::default(),
                Some(format!(
                    "Default settings could not be read. Balanced graphics and diagnostics Off are in use. {error}"
                )),
            ),
        }
    }
    pub fn write(self, user_data: &Path) -> Result<(), String> {
        private_directory(user_data)?;
        atomic_json(&user_data.join("preferences.json"), &self)
    }

    pub fn save_quality(user_data: &Path, quality: RenderQualitySettings) -> Result<(), String> {
        let (mut preferences, warning) = Self::read(user_data);
        if let Some(warning) = warning {
            return Err(warning);
        }
        preferences.quality = quality;
        preferences.write(user_data)
    }
}

pub fn persist_settings(
    profile_path: &Path,
    quality: RenderQualitySettings,
    config: &WorldConfig,
) -> Result<(), String> {
    let mut profile = WorldProfile::read(profile_path)?;
    profile.quality = quality;
    profile.config.day_night = config.day_night;
    profile.config.seasons = config.seasons;
    profile.config.time_scale = config.time_scale;
    profile.last_played_unix_seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    profile.checkpoint_present = fs::symlink_metadata(
        config.region_store.as_ref().unwrap_or(&config.save_path),
    )
    .is_ok_and(|metadata| {
        metadata.is_file() && !metadata.file_type().is_symlink() && metadata.len() > 0
    });
    atomic_json(profile_path, &profile)
}

pub fn absolute_path(path: &Path) -> Result<PathBuf, String> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()
            .map_err(|e| e.to_string())?
            .join(path))
    }
}

pub fn validate_menu_screenshot(path: &Path) -> Result<(), String> {
    if path
        .extension()
        .and_then(|s| s.to_str())
        .is_none_or(|s| !s.eq_ignore_ascii_case("png"))
    {
        return Err("Menu screenshots require a .png destination.".into());
    }
    if let Ok(metadata) = fs::symlink_metadata(path) {
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err(
                "A menu screenshot cannot replace a symlink or non-file destination.".into(),
            );
        }
        let mut header = [0u8; 8];
        fs::File::open(path)
            .and_then(|mut file| file.read_exact(&mut header))
            .map_err(|e| e.to_string())?;
        if header != *b"\x89PNG\r\n\x1a\n" {
            return Err(
                "A menu screenshot can only replace an existing PNG image, never world data."
                    .into(),
            );
        }
    }
    Ok(())
}

pub(crate) fn read_json<T: for<'a> Deserialize<'a>>(path: &Path) -> Result<T, String> {
    let metadata = fs::symlink_metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() > MAX_PROFILE_BYTES
    {
        return Err("Profile must be a regular file smaller than 256 KiB.".into());
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(MAX_PROFILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_PROFILE_BYTES {
        return Err("Profile exceeds 256 KiB.".into());
    }
    serde_json::from_slice(&bytes).map_err(|e| format!("Invalid profile: {e}"))
}

pub(crate) fn private_directory(path: &Path) -> Result<(), String> {
    private_directory_with_sync(path, &terra_bevy::rules::publication::sync_directory)
}

pub(super) fn private_directory_with_sync(
    path: &Path,
    sync: &dyn Fn(&Path) -> std::io::Result<()>,
) -> Result<(), String> {
    if let Ok(metadata) = fs::symlink_metadata(path) {
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(format!(
                "Use a regular folder for world data: {}",
                path.display()
            ));
        }
        return Ok(());
    }
    terra_bevy::rules::publication::create_private_directories_with_sync(path, sync)
        .map_err(|e| format!("Cannot create and synchronize {}: {e}", path.display()))?;
    private_permissions(path, true)
}

pub(super) fn private_permissions(path: &Path, directory: bool) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            path,
            fs::Permissions::from_mode(if directory { 0o700 } else { 0o600 }),
        )
        .map_err(|e| e.to_string())?;
    }
    #[cfg(not(unix))]
    {
        let _ = (path, directory);
    }
    Ok(())
}

pub(super) fn write_new_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    write_new_json_with_sync(path, value, terra_bevy::rules::publication::sync_directory)
        .map_err(|error| error.to_string())
}

pub(super) fn write_new_json_with_sync<T: Serialize>(
    path: &Path,
    value: &T,
    sync: impl FnOnce(&Path) -> std::io::Result<()>,
) -> Result<(), terra_bevy::rules::publication::PublicationError> {
    use terra_bevy::rules::publication::{PublicationError, PublishMode, write_private_with_sync};
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| PublicationError::NotPublished(std::io::Error::other(error)))?;
    if bytes.len() as u64 > MAX_PROFILE_BYTES {
        return Err(PublicationError::NotPublished(std::io::Error::other(
            "Profile exceeds 256 KiB.",
        )));
    }
    write_private_with_sync(path, &bytes, PublishMode::CreateNew, sync)
}

pub(crate) fn atomic_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?;
    if bytes.len() as u64 > MAX_PROFILE_BYTES {
        return Err("Profile exceeds 256 KiB.".into());
    }
    terra_bevy::rules::publication::write_private(
        path,
        &bytes,
        terra_bevy::rules::publication::PublishMode::Replace,
    )
    .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    struct LeaseChild(std::process::Child);
    impl Drop for LeaseChild {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    fn wait_for_lease_child(child: &mut LeaseChild, signal: &Path) {
        let started = std::time::Instant::now();
        while !signal.is_file() {
            assert!(
                child.0.try_wait().unwrap().is_none(),
                "lease child exited before its signal"
            );
            assert!(
                started.elapsed() < std::time::Duration::from_secs(15),
                "lease child did not become ready"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    #[test]
    fn managed_session_lease_excludes_processes_through_final_metadata_and_releases_after_crash() {
        const CHILD: &str = "TERRA_MANAGED_LEASE_CHILD";
        const SIGNAL: &str = "TERRA_MANAGED_LEASE_SIGNAL";
        if let Some(path) = std::env::var_os(CHILD) {
            let path = PathBuf::from(path);
            let profile = WorldProfile::read(&path).unwrap();
            let lease = begin_managed_session(&path, &profile.config).unwrap();
            assert!(lease.require_existing_progress);
            let ready = PathBuf::from(std::env::var_os(SIGNAL).unwrap());
            fs::write(&ready, b"lease held").unwrap();
            let mut command = [0];
            std::io::stdin().read_exact(&mut command).unwrap();
            assert_eq!(command, [b's']);
            persist_settings(&path, profile.quality, &profile.config).unwrap();
            fs::write(
                ready.with_extension("final"),
                b"metadata saved while lease held",
            )
            .unwrap();
            std::io::stdin().read_exact(&mut command).unwrap();
            assert_eq!(command, [b'q']);
            drop(lease);
            return;
        }
        let (folder, path, profile) = managed_fixture(false);
        write_fixture_checkpoint(&profile, false);
        drop(begin_managed_session(&path, &profile.config).unwrap());
        let witness = managed_start_witness_path(&path);
        let lock = managed_session_lease_path(&path);
        let lock_bytes = fs::read(&lock).unwrap();
        let (other_folder, other_path, other_profile) = managed_fixture(true);
        write_fixture_checkpoint(&other_profile, false);
        for abrupt in [false, true] {
            let ready = folder.join(format!("ready-{abrupt}"));
            let profile_bytes = fs::read(&path).unwrap();
            let progress_bytes = fs::read(&profile.config.save_path).unwrap();
            let witness_bytes = fs::read(&witness).unwrap();
            let mut child = LeaseChild(std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "launcher::model::tests::managed_session_lease_excludes_processes_through_final_metadata_and_releases_after_crash", "--nocapture"])
                .env(CHILD, &path).env(SIGNAL, &ready)
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::null()).spawn().unwrap());
            wait_for_lease_child(&mut child, &ready);
            let started = std::time::Instant::now();
            let error = begin_managed_session(&path, &profile.config).err().unwrap();
            assert!(error.contains("already open"));
            assert!(started.elapsed() < std::time::Duration::from_secs(2));
            assert_eq!(fs::read(&path).unwrap(), profile_bytes);
            assert_eq!(fs::read(&profile.config.save_path).unwrap(), progress_bytes);
            assert_eq!(fs::read(&witness).unwrap(), witness_bytes);
            // Another managed world has independent ownership.
            drop(begin_managed_session(&other_path, &other_profile.config).unwrap());
            if abrupt {
                child.0.kill().unwrap();
                child.0.wait().unwrap();
            } else {
                child.0.stdin.as_mut().unwrap().write_all(b"s").unwrap();
                wait_for_lease_child(&mut child, &ready.with_extension("final"));
                assert!(
                    begin_managed_session(&path, &profile.config)
                        .err()
                        .unwrap()
                        .contains("already open")
                );
                assert!(WorldProfile::read(&path).unwrap().checkpoint_present);
                child.0.stdin.as_mut().unwrap().write_all(b"q").unwrap();
                assert!(child.0.wait().unwrap().success());
            }
            drop(child);
            let reopened = begin_managed_session(&path, &profile.config).unwrap();
            assert!(reopened.require_existing_progress);
            drop(reopened);
            assert_eq!(
                fs::read(&lock).unwrap(),
                lock_bytes,
                "unlocking must not remove/replace the stable inode"
            );
            assert_eq!(fs::read(&witness).unwrap(), witness_bytes);
        }
        fs::remove_dir_all(folder).unwrap();
        fs::remove_dir_all(other_folder).unwrap();
    }

    #[test]
    fn managed_lease_preserves_first_launch_witness_semantics_and_rejects_path_aliases() {
        let (folder, path, profile) = managed_fixture(false);
        let lease = begin_managed_session(&path, &profile.config).unwrap();
        assert!(!lease.require_existing_progress);
        let witness = fs::read(managed_start_witness_path(&path)).unwrap();
        assert!(
            begin_managed_session(&path, &profile.config)
                .err()
                .unwrap()
                .contains("already open")
        );
        assert!(!profile.config.save_path.exists());
        drop(lease);
        assert!(
            begin_managed_session(&path, &profile.config)
                .err()
                .unwrap()
                .contains("previously started")
        );
        assert_eq!(
            fs::read(managed_start_witness_path(&path)).unwrap(),
            witness
        );
        let lock = managed_session_lease_path(&path);
        let mut aliased = profile.config.clone();
        aliased.save_path = lock.clone();
        let original_lock = fs::read(&lock).unwrap();
        assert!(acquire_managed_lease(&path, &aliased).is_err());
        assert_eq!(fs::read(&lock).unwrap(), original_lock);
        fs::remove_dir_all(folder).unwrap();
    }

    fn managed_fixture(regional: bool) -> (PathBuf, PathBuf, WorldProfile) {
        let folder = temporary();
        private_directory(&folder).unwrap();
        let directory = folder.join("worlds/first");
        private_directory(&directory).unwrap();
        let config = WorldConfig {
            region_store: regional.then(|| directory.join("world.db")),
            save_path: directory.join("world-save.json"),
            cache_dir: directory.join("cache"),
            ..Default::default()
        };
        let profile = WorldProfile {
            schema_version: 1,
            title: "Unstarted fixture".into(),
            created_unix_seconds: 1,
            last_played_unix_seconds: 0,
            checkpoint_present: false,
            config,
            character: CharacterProfile::default(),
            quality: RenderQualitySettings::default(),
            start_hour: 12,
        };
        let path = directory.join("world.json");
        write_new_json(&path, &profile).unwrap();
        (folder, path, profile)
    }

    fn write_fixture_checkpoint(profile: &WorldProfile, abrupt: bool) {
        let config = &profile.config;
        let mut sandbox = terra_bevy::SandboxRuntime::new(config.focus);
        sandbox
            .state
            .set_character_profile(sandbox.player_id, profile.character.clone())
            .unwrap();
        let save = terra_bevy::sandbox::LocalWorldSave {
            schema_version: 1,
            identity: terra_bevy::sandbox::WorldIdentity::from(config),
            clock: terra_core::WorldClock::default(),
            player_id: sandbox.player_id,
            state: sandbox.state,
            politics: None,
        };
        save.validate_for(config, &sandbox.catalog).unwrap();
        if let Some(path) = &config.region_store {
            let archive = save.regional_archive(config, &sandbox.catalog).unwrap();
            let mut store = terra_persistence::RegionStore::open(
                path,
                archive.identity,
                &archive.catalog,
                archive.region_level,
            )
            .unwrap();
            let mut state = archive.state;
            store
                .checkpoint(&mut state, archive.metadata, &Default::default())
                .unwrap();
            if abrupt {
                // A real process exit leaves SQLite's acknowledged WAL commit
                // without normal adapter/profile handoff or connection Drop.
                std::process::exit(0);
            }
        } else {
            save.write(&config.save_path).unwrap();
            if abrupt {
                std::process::exit(0);
            }
        }
    }

    #[test]
    fn first_checkpoint_crash_cannot_turn_a_started_managed_world_into_a_fresh_world() {
        const CHILD: &str = "TERRA_MANAGED_START_CRASH_CHILD";
        if let Some(path) = std::env::var_os(CHILD) {
            let path = PathBuf::from(path);
            let profile = WorldProfile::read(&path).unwrap();
            assert!(!prepare_managed_launch(&path, &profile.config).unwrap());
            write_fixture_checkpoint(&profile, true);
            unreachable!("checkpoint child must exit without a profile handoff");
        }
        for regional in [false, true] {
            let (folder, path, profile) = managed_fixture(regional);
            let original_profile = fs::read(&path).unwrap();
            let witness_path = path.with_file_name(START_WITNESS);
            assert!(!witness_path.exists());
            assert!(!WorldProfile::read(&path).unwrap().checkpoint_present);
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "launcher::model::tests::first_checkpoint_crash_cannot_turn_a_started_managed_world_into_a_fresh_world", "--nocapture"])
                .env(CHILD, &path)
                .output().unwrap();
            assert!(
                output.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(
                fs::read(&path).unwrap(),
                original_profile,
                "no exit/settings update was needed for protection"
            );
            assert!(!WorldProfile::read(&path).unwrap().checkpoint_present);
            assert!(
                prepare_managed_launch(&path, &profile.config).unwrap(),
                "a real committed checkpoint resumes"
            );
            let witness = fs::read(&witness_path).unwrap();
            let progress = profile
                .config
                .region_store
                .as_ref()
                .unwrap_or(&profile.config.save_path);
            fs::remove_file(progress).unwrap();
            if regional {
                for suffix in ["-wal", "-shm", "-journal"] {
                    let _ =
                        fs::remove_file(PathBuf::from(format!("{}{suffix}", progress.display())));
                }
            }
            let error = prepare_managed_launch(&path, &profile.config).unwrap_err();
            assert!(error.contains("previously started") && error.contains("separate new world"));
            assert!(
                !progress.exists(),
                "rejection must not create replacement progress"
            );
            assert_eq!(fs::read(&path).unwrap(), original_profile);
            assert_eq!(fs::read(&witness_path).unwrap(), witness);
            fs::remove_dir_all(folder).unwrap();
        }
    }

    #[test]
    fn started_worlds_reject_failed_initialization_corruption_and_zero_generation() {
        for regional in [false, true] {
            for kind in ["missing", "empty", "corrupt", "zero_generation"] {
                if !regional && kind == "zero_generation" {
                    continue;
                }
                let (folder, path, profile) = managed_fixture(regional);
                assert!(!prepare_managed_launch(&path, &profile.config).unwrap());
                let progress = profile
                    .config
                    .region_store
                    .as_ref()
                    .unwrap_or(&profile.config.save_path);
                match kind {
                    "missing" => (),
                    "empty" => fs::write(progress, []).unwrap(),
                    "corrupt" => fs::write(progress, b"invalid checkpoint").unwrap(),
                    _ => {
                        let archive = terra_bevy::sandbox::LocalWorldSave {
                            schema_version: 1,
                            identity: terra_bevy::sandbox::WorldIdentity::from(&profile.config),
                            clock: terra_core::WorldClock::default(),
                            player_id: 1,
                            state: terra_bevy::SandboxRuntime::new(profile.config.focus).state,
                            politics: None,
                        }
                        .regional_archive(&profile.config, &terra_bevy::rules::Catalog::builtin())
                        .unwrap();
                        drop(
                            terra_persistence::RegionStore::open(
                                progress,
                                archive.identity,
                                &archive.catalog,
                                archive.region_level,
                            )
                            .unwrap(),
                        );
                    }
                }
                let before = fs::read(progress).ok();
                let profile_bytes = fs::read(&path).unwrap();
                assert!(
                    prepare_managed_launch(&path, &profile.config).is_err(),
                    "regional={regional}, {kind}"
                );
                assert_eq!(fs::read(progress).ok(), before);
                assert_eq!(fs::read(&path).unwrap(), profile_bytes);
                fs::remove_dir_all(folder).unwrap();
            }
        }
    }

    #[test]
    fn a_valid_legacy_checkpoint_gains_a_bound_witness_without_changing_profile_or_progress() {
        for regional in [false, true] {
            let (folder, path, profile) = managed_fixture(regional);
            let progress = profile
                .config
                .region_store
                .as_ref()
                .unwrap_or(&profile.config.save_path);
            fs::write(progress, b"invalid legacy progress").unwrap();
            assert!(prepare_managed_launch(&path, &profile.config).is_err());
            assert!(
                !path.with_file_name(START_WITNESS).exists(),
                "unvalidated legacy progress cannot gain a launch witness"
            );
            assert_eq!(fs::read(progress).unwrap(), b"invalid legacy progress");
            fs::remove_file(progress).unwrap();
            write_fixture_checkpoint(&profile, false);
            let original = fs::read(progress).unwrap();
            let original_profile = fs::read(&path).unwrap();
            assert!(prepare_managed_launch(&path, &profile.config).unwrap());
            assert_eq!(fs::read(progress).unwrap(), original);
            assert_eq!(fs::read(&path).unwrap(), original_profile);
            let witness_path = path.with_file_name(START_WITNESS);
            let witness = fs::read(&witness_path).unwrap();
            let mut renamed = WorldProfile::read(&path).unwrap();
            renamed.title = "Recognizable new name".into();
            renamed.quality.quality = renamed.quality.quality.next();
            renamed.config.time_scale = 15.;
            atomic_json(&path, &renamed).unwrap();
            assert!(prepare_managed_launch(&path, &renamed.config).unwrap());
            assert_eq!(
                fs::read(&witness_path).unwrap(),
                witness,
                "names and settings do not rewrite world identity"
            );
            let mut changed = renamed.clone();
            changed.config.seed += 1;
            atomic_json(&path, &changed).unwrap();
            assert!(
                prepare_managed_launch(&path, &changed.config)
                    .unwrap_err()
                    .contains("another profile, source or progress path")
            );
            changed = renamed;
            if regional {
                changed.config.region_store = Some(folder.join("other.db"));
            } else {
                changed.config.save_path = folder.join("other.json");
            }
            atomic_json(&path, &changed).unwrap();
            assert!(prepare_managed_launch(&path, &changed.config).is_err());
            assert_eq!(fs::read(progress).unwrap(), original);
            fs::remove_dir_all(folder).unwrap();
        }
    }

    #[test]
    fn simultaneous_first_launches_admit_only_one_initializer_and_leave_profile_unplayed() {
        let (folder, path, profile) = managed_fixture(true);
        let original = fs::read(&path).unwrap();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let threads: Vec<_> = (0..2)
            .map(|_| {
                let path = path.clone();
                let config = profile.config.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    prepare_managed_launch(&path, &config)
                })
            })
            .collect();
        let outcomes: Vec<_> = threads
            .into_iter()
            .map(|thread| thread.join().unwrap())
            .collect();
        assert_eq!(
            outcomes
                .iter()
                .filter(|result| matches!(result, Ok(false)))
                .count(),
            1
        );
        assert_eq!(outcomes.iter().filter(|result| result.is_err()).count(), 1);
        assert_eq!(fs::read(&path).unwrap(), original);
        assert!(!profile.config.region_store.as_ref().unwrap().exists());
        assert!(path.with_file_name(START_WITNESS).is_file());
        fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn failed_witness_publication_never_overwrites_files_or_authorizes_fresh_progress() {
        let (folder, path, profile) = managed_fixture(false);
        let original = fs::read(&path).unwrap();
        let witness = WorldStartWitness::for_config(&path, &profile.config).unwrap();
        let destination = path.with_file_name(START_WITNESS);
        fs::create_dir(&destination).unwrap();
        fs::write(destination.join("original"), b"keep existing destination").unwrap();
        assert!(publish_start_witness(&destination, &witness).is_err());
        assert_eq!(
            fs::read(destination.join("original")).unwrap(),
            b"keep existing destination"
        );
        assert_eq!(
            fs::read_dir(path.parent().unwrap()).unwrap().count(),
            2,
            "owned temporary was cleaned"
        );
        fs::remove_dir_all(&destination).unwrap();
        let error = publish_start_witness_with_sync(&destination, &witness, |_| {
            Err(std::io::Error::other("injected directory sync failure"))
        })
        .unwrap_err();
        assert!(error.contains("published but its durability could not be confirmed"));
        assert!(
            destination.is_file(),
            "a published witness is retained after uncertain synchronization"
        );
        assert!(prepare_managed_launch(&path, &profile.config).is_err());
        assert!(!profile.config.save_path.exists());
        assert_eq!(fs::read(&path).unwrap(), original);
        assert_eq!(fs::read_dir(path.parent().unwrap()).unwrap().count(), 2);
        fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn diagnostic_consent_defaults_off_and_survives_graphics_changes() {
        let legacy: Preferences = serde_json::from_value(serde_json::json!({
            "quality": RenderQualitySettings::default()
        }))
        .unwrap();
        assert!(!legacy.playtest_reports);
        let folder = temporary();
        assert!(!Preferences::read(&folder).0.playtest_reports);
        Preferences {
            playtest_reports: true,
            ..Default::default()
        }
        .write(&folder)
        .unwrap();
        let mut quality = RenderQualitySettings::default();
        quality.quality = quality.quality.next();
        Preferences::save_quality(&folder, quality).unwrap();
        let (saved, warning) = Preferences::read(&folder);
        assert!(warning.is_none());
        assert!(saved.playtest_reports);
        assert_eq!(saved.quality.quality, quality.quality);
        fs::write(folder.join("preferences.json"), b"not a preference file").unwrap();
        assert!(Preferences::save_quality(&folder, quality).is_err());
        assert_eq!(
            fs::read(folder.join("preferences.json")).unwrap(),
            b"not a preference file"
        );
        assert!(!Preferences::read(&folder).0.playtest_reports);
        fs::remove_dir_all(folder).unwrap();
    }
    #[test]
    fn coverage_uses_geographic_dimensions_with_latitude_and_pole_clipping() {
        let at = |latitude, longitude| GeoPosition::new(latitude, longitude, 0.).unwrap();
        let equator = region_span_m(at(0., 0.), 15, 3).unwrap();
        let north = region_span_m(at(60., 0.), 15, 3).unwrap();
        assert!((north.0 / equator.0 - 0.5).abs() < 0.001);
        assert_eq!(north.1, equator.1);
        assert_eq!(region_span_m(at(0., 179.999), 15, 3).unwrap(), equator);
        let pole = region_span_m(at(90., 0.), 15, 3).unwrap();
        assert!(pole.0 > 0. && pole.1 < equator.1);
        assert!((pole.1 / equator.1 - 4. / 7.).abs() < 0.001);
        let draft = Draft::default();
        assert!(draft.requested_coverage_label().starts_with("Region · ≈"));
        assert!(draft.requested_coverage_label().ends_with("km"));
        assert!(
            Draft {
                latitude: "-".into(),
                ..draft
            }
            .requested_coverage_label()
            .contains("Enter valid coordinates")
        );
    }

    fn root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .unwrap()
    }
    fn temporary() -> PathBuf {
        std::env::temp_dir().join(format!(
            "terra-launcher-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }
    #[test]
    fn invalid_world_input_does_not_create_or_overwrite_files() {
        let folder = temporary();
        let mut draft = Draft::default();
        draft.latitude = "NaN".into();
        assert!(create_world(&draft, &root(), &folder).is_err());
        assert!(!folder.exists());
        draft.latitude = "48.137".into();
        draft.character.name = "\n".into();
        assert!(create_world(&draft, &root(), &folder).is_err());
        assert!(!folder.exists());
        draft.character.name = "Alex".into();
        draft.preset = Preset::Heightmap;
        draft.latitude = "0".into();
        assert!(create_world(&draft, &root(), &folder).is_err());
        assert!(!folder.exists());
    }

    #[test]
    fn new_world_directory_and_profile_sync_failures_never_acknowledge_creation() {
        use std::cell::Cell;
        use terra_bevy::rules::publication::sync_directory;

        let folder = temporary();
        let draft = Draft::default();
        let original = create_world(&draft, &root(), &folder).unwrap();
        let original_bytes = fs::read(&original).unwrap();
        let worlds = folder.join("worlds");
        let observed = Cell::new(false);
        let error = create_world_with_sync(&draft, &root(), &folder, &|path| {
            if path == worlds {
                observed.set(true);
                Err(std::io::Error::other(
                    "injected new world directory sync failure",
                ))
            } else {
                sync_directory(path)
            }
        })
        .unwrap_err();
        assert!(observed.get());
        assert!(error.contains("New world was not created"));
        assert_eq!(fs::read_dir(&worlds).unwrap().count(), 1);
        assert_eq!(fs::read(&original).unwrap(), original_bytes);

        // The final profile has a different stage: a visible complete profile
        // survives an uncertain namespace acknowledgement, without success.
        observed.set(false);
        let error = create_world_with_sync(&draft, &root(), &folder, &|path| {
            if path.join("world.json").is_file() {
                observed.set(true);
                Err(std::io::Error::other(
                    "injected final world profile sync failure",
                ))
            } else {
                sync_directory(path)
            }
        })
        .unwrap_err();
        assert!(observed.get());
        assert!(error.contains("published but its durability could not be confirmed"));
        let retained = recent_worlds(&folder).0;
        assert_eq!(retained.len(), 2);
        for world in retained {
            WorldProfile::read(&world.path).unwrap();
        }
        assert_eq!(fs::read(&original).unwrap(), original_bytes);
        create_world(&draft, &root(), &folder).unwrap();
        assert_eq!(recent_worlds(&folder).0.len(), 3);
        fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn same_named_worlds_have_distinct_private_stores_and_roundtrip_the_character() {
        let folder = temporary();
        let draft = Draft::default();
        let first = create_world(&draft, &root(), &folder).unwrap();
        let original = fs::read(&first).unwrap();
        let second = create_world(&draft, &root(), &folder).unwrap();
        assert_ne!(first, second);
        assert_eq!(fs::read(&first).unwrap(), original);
        let profile = WorldProfile::read(&first).unwrap();
        assert_eq!(profile.character, draft.character);
        assert_ne!(
            profile.config.region_store,
            WorldProfile::read(&second).unwrap().config.region_store
        );
        assert!(!profile.config.region_store.as_ref().unwrap().exists());
        assert_eq!(recent_worlds(&folder).0.len(), 2);
        let checkpoint = profile.config.region_store.as_ref().unwrap();
        fs::write(checkpoint, b"existing checkpoint file").unwrap();
        persist_settings(&first, profile.quality, &profile.config).unwrap();
        assert!(WorldProfile::read(&first).unwrap().checkpoint_present);
        fs::remove_file(checkpoint).unwrap();
        assert!(
            WorldProfile::read(&first)
                .unwrap_err()
                .contains("previously saved progress")
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(first).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        fs::remove_dir_all(folder).unwrap();
    }
    #[test]
    fn join_session_names_cannot_escape_the_managed_folder() {
        let folder = temporary();
        let mut draft = Draft::default();
        draft.session_name = "../../world.db".into();
        assert!(draft.join_arguments(&folder).is_err());
        assert!(!folder.exists());
        draft.session_name = "alice-2".into();
        draft.address = "[::1]:7777".into();
        let args = draft.join_arguments(&folder).unwrap();
        assert!(args.iter().any(|s| s.ends_with("sessions/alice-2.json")));
        assert!(args.contains(&"[::1]:7777".into()));
        fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn remote_join_requires_verified_tls_or_an_explicit_insecure_choice() {
        let folder = temporary();
        let mut draft = Draft {
            address: "192.0.2.7:7777".into(),
            ..Draft::default()
        };
        assert!(draft.join_arguments(&folder).is_err());
        assert!(!folder.exists());
        draft.tls = true;
        draft.tls_ca = folder.join("missing.pem").display().to_string();
        assert!(draft.join_arguments(&folder).is_err());
        assert!(!folder.exists());
        draft.tls = false;
        draft.allow_insecure_remote = true;
        let arguments = draft.join_arguments(&folder).unwrap();
        assert!(arguments.contains(&"--allow-insecure-remote".into()));
        draft.save_connection(&folder).unwrap();
        let mut next = Draft::default();
        assert!(next.restore_connection(&folder).is_none());
        assert_eq!(next.address, draft.address);
        assert!(next.allow_insecure_remote);
        std::fs::remove_dir_all(folder).unwrap();
    }
    #[test]
    fn all_bundled_world_choices_resolve_to_valid_source_configuration() {
        for preset in [
            Preset::Frontier,
            Preset::Heightmap,
            Preset::Historical1950,
            Preset::Historical1980,
        ] {
            let draft = Draft {
                preset,
                ..Draft::default()
            };
            let config = draft.config(&root()).unwrap();
            config.validate().unwrap();
            if config.historical_pack.is_some() {
                assert!(config.data_epoch_unix.is_some());
            }
        }
    }

    #[test]
    fn join_source_configuration_is_checked_before_session_creation_and_roundtrips() {
        let folder = temporary();
        let mut draft = Draft {
            source_config: folder.join("missing.json").display().to_string(),
            ..Default::default()
        };
        assert!(draft.join_arguments(&folder).is_err());
        assert!(!folder.exists());
        let config_file = root().join("configs/heightmap-multiplayer.json");
        // The application runs from the repository root. Use absolute provider
        // paths in this test because Cargo runs unit cases from the crate folder.
        let mut config: WorldConfig = read_json(&config_file).unwrap();
        config.heightmap_manifest = Some(root().join(config.heightmap_manifest.unwrap()));
        let input = temporary();
        fs::create_dir_all(&input).unwrap();
        let file = input.join("source.json");
        fs::write(&file, serde_json::to_vec(&config).unwrap()).unwrap();
        draft.source_config = file.display().to_string();
        let arguments = draft.join_arguments(&folder).unwrap();
        assert!(
            arguments
                .windows(2)
                .any(|pair| pair[0] == "--config" && pair[1] == draft.source_config)
        );
        draft.save_connection(&folder).unwrap();
        let mut restored = Draft::default();
        assert!(restored.restore_connection(&folder).is_none());
        assert_eq!(restored.source_config, draft.source_config);
        config.region_store = Some(input.join("host.db"));
        fs::write(&file, serde_json::to_vec(&config).unwrap()).unwrap();
        assert!(draft.join_arguments(&folder).unwrap_err().contains("host"));
        assert!(!config.region_store.unwrap().exists());
        fs::remove_dir_all(folder).unwrap();
        fs::remove_dir_all(input).unwrap();
    }

    #[test]
    fn portable_world_and_screenshot_paths_cannot_replace_profiles_or_databases() {
        let folder = temporary();
        let path = create_world(
            &Draft {
                regional_storage: false,
                ..Draft::default()
            },
            &root(),
            &folder,
        )
        .unwrap();
        let mut profile = WorldProfile::read(&path).unwrap();
        assert!(profile.config.region_store.is_none());
        assert!(profile.config.save_path.ends_with("world-export.json"));
        profile.config.save_path = path.clone();
        atomic_json(&path, &profile).unwrap();
        assert!(WorldProfile::read(&path).is_err());
        let fake_image = folder.join("database.png");
        fs::write(&fake_image, b"SQLite format 3\0not a screenshot").unwrap();
        assert!(validate_menu_screenshot(&fake_image).is_err());
        assert_eq!(
            fs::read(&fake_image).unwrap(),
            b"SQLite format 3\0not a screenshot"
        );
        fs::remove_dir_all(folder).unwrap();
    }
}
