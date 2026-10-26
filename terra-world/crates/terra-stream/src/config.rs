use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Runtime budgets and provider settings. All limits are checked at construction.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct StreamConfig {
    /// Network use must be explicitly enabled; offline is fully functional.
    pub live: bool,
    pub cache_dir: PathBuf,
    pub max_resident: usize,
    pub max_in_flight: usize,
    pub max_queued: usize,
    /// Samples along each geographic tile side, including both shared edges.
    pub terrain_resolution: usize,
    pub osm: bool,
    pub weather: bool,
    pub seed: u64,
    /// For chunk requests: substitute synthetic terrain or omit failed OSM.
    /// `fetch_weather` always reports live-provider errors to its caller.
    pub allow_fallback: bool,
    pub request_timeout_seconds: u64,
    /// Cap each HTTP body before parsing, including when Content-Length is absent.
    pub max_response_bytes: usize,
    pub max_disk_cache_bytes: u64,
    pub chunk_cache_ttl_seconds: u64,
    pub weather_cache_ttl_seconds: u64,
    /// Per-process safety budget for public Overpass demo use. A production data
    /// service should distribute prepared regional packages instead.
    pub overpass_request_budget: usize,
    /// Configurable for a self-hosted/contracted production endpoint.
    pub overpass_url: String,
    pub weather_url: String,
    /// Base directory containing z/x/y.png, not a template string.
    pub terrain_url: String,
    /// Additional trusted CA bundle for an explicitly configured TLS proxy or
    /// private data service. Never disables normal certificate verification.
    /// When absent, the standard SSL_CERT_FILE environment variable is honored.
    pub ca_cert_path: Option<PathBuf>,
}

impl Default for StreamConfig {
    fn default() -> Self {
        Self {
            live: false,
            cache_dir: PathBuf::from(".terra-cache"),
            max_resident: 49,
            max_in_flight: 2,
            max_queued: 64,
            terrain_resolution: 33,
            osm: true,
            weather: true,
            seed: 42,
            allow_fallback: true,
            request_timeout_seconds: 20,
            max_response_bytes: 8 * 1024 * 1024,
            max_disk_cache_bytes: 256 * 1024 * 1024,
            chunk_cache_ttl_seconds: 7 * 24 * 3600,
            weather_cache_ttl_seconds: 600,
            overpass_request_budget: 256,
            overpass_url: "https://overpass-api.de/api/interpreter".into(),
            weather_url: "https://api.open-meteo.com/v1/forecast".into(),
            terrain_url: "https://s3.amazonaws.com/elevation-tiles-prod/terrarium".into(),
            ca_cert_path: None,
        }
    }
}

impl StreamConfig {
    pub fn validate(&self) -> Result<(), StreamError> {
        let problem = if !(1..=4096).contains(&self.max_resident) {
            Some("max_resident must be in 1..=4096")
        } else if !(1..=8).contains(&self.max_in_flight) {
            Some("max_in_flight must be in 1..=8")
        } else if !(1..=4096).contains(&self.max_queued) {
            Some("max_queued must be in 1..=4096")
        } else if !(2..=257).contains(&self.terrain_resolution) {
            Some("terrain_resolution must be in 2..=257")
        } else if !(1..=120).contains(&self.request_timeout_seconds) {
            Some("request_timeout_seconds must be in 1..=120")
        } else if !(1024..=64 * 1024 * 1024).contains(&self.max_response_bytes) {
            Some("max_response_bytes must be in 1024..=67108864")
        } else if self.max_disk_cache_bytes < self.max_response_bytes as u64 {
            Some("disk cache budget must be at least max_response_bytes")
        } else if !(1..=10_000).contains(&self.overpass_request_budget) {
            Some("overpass_request_budget must be in 1..=10000")
        } else {
            None
        };
        if let Some(problem) = problem {
            return Err(StreamError::Configuration(problem.into()));
        }
        if self.live {
            for (name, url) in [
                ("terrain_url", &self.terrain_url),
                ("overpass_url", &self.overpass_url),
                ("weather_url", &self.weather_url),
            ] {
                let parsed = reqwest::Url::parse(url)
                    .map_err(|_| StreamError::Configuration(format!("invalid {name}")))?;
                if !matches!(parsed.scheme(), "https" | "http") {
                    return Err(StreamError::Configuration(format!(
                        "{name} must use HTTP or HTTPS"
                    )));
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum StreamError {
    #[error("invalid streaming configuration: {0}")]
    Configuration(String),
    #[error("invalid geographic request: {0}")]
    Geography(String),
    #[error("provider request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("provider payload is invalid: {0}")]
    Payload(String),
    #[error("cache operation failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("stream worker disconnected")]
    WorkerDisconnected,
}
