//! One small, explicit provider health probe. This is not an automated live test.
use terra_core::{GeoPosition, TileKey};
use terra_stream::{StreamConfig, fetch_weather, load_chunk};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let live = std::env::args().any(|arg| arg == "--live");
    let config = StreamConfig {
        live,
        osm: !std::env::args().any(|arg| arg == "--no-osm"),
        allow_fallback: false,
        cache_dir: std::env::temp_dir().join("terra-provider-probe"),
        ..Default::default()
    };
    let position = GeoPosition::new(48.137, 11.575, 0.0)?;
    let key = TileKey::at(position, 16)?;
    let chunk = match load_chunk(key, &config) {
        Ok(chunk) => serde_json::json!({
            "ok": true,
            "key": chunk.key,
            "height_range_m": chunk.heights.min_max(),
            "feature_count": chunk.features.len(),
            "sources": chunk.sources,
            "warnings": chunk.warnings,
        }),
        Err(error) => serde_json::json!({"ok":false,"error":error.to_string()}),
    };
    let weather = match fetch_weather(position, &config) {
        Ok(sample) => serde_json::json!({"ok":true,"sample":sample}),
        Err(error) => serde_json::json!({"ok":false,"error":error.to_string()}),
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "live":live,"position":position,"chunk":chunk,"weather":weather
        }))?
    );
    Ok(())
}
