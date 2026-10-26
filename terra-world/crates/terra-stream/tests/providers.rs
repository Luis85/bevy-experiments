use terra_core::{FeatureKind, GeoPosition, TileKey};
use terra_stream::{
    StreamConfig, fetch_weather, load_chunk,
    providers::{decode_terrarium_pixel, load_terrain, parse_open_meteo, parse_overpass},
};

#[test]
fn documented_terrarium_encoding_decodes_exactly() {
    assert_eq!(decode_terrarium_pixel([128, 0, 0]), 0.0);
    assert_eq!(decode_terrarium_pixel([137, 219, 68]), 2523.265625);
    assert_eq!(decode_terrarium_pixel([127, 255, 128]), -0.5);
}

#[test]
fn osm_features_keep_source_ids_and_reject_invalid_geometry() {
    let features = parse_overpass(include_bytes!("fixtures/overpass.json")).unwrap();
    assert_eq!(features.len(), 3);
    assert_eq!(features[0].id, "osm:way:1");
    assert_eq!(features[0].kind, FeatureKind::Building);
    assert_eq!(features[0].height_m, 9.0);
    assert_eq!(features[2].height_m, 12.0);
    for feature in features {
        feature.validate().unwrap();
    }
    assert!(parse_overpass(br#"{"remark":"runtime error: timeout", "elements":[]}"#).is_err());
}

#[test]
fn weather_keeps_units_and_timestamp_without_inventing_missing_values() {
    let raw = include_bytes!("fixtures/weather.json");
    let weather = parse_open_meteo(raw).unwrap();
    assert_eq!(weather.temperature_c, 22.5);
    assert!((weather.cloud_cover - 0.45).abs() < 0.0001);
    assert_eq!(weather.precipitation_mm_h, 1.0);
    assert_eq!(weather.observed_unix_seconds, 1782043200);
    assert!(!weather.synthetic);
    let mut value: serde_json::Value = serde_json::from_slice(raw).unwrap();
    value["current_units"]["wind_speed_10m"] = "km/h".into();
    assert!(parse_open_meteo(&serde_json::to_vec(&value).unwrap()).is_err());
    value["current_units"]["wind_speed_10m"] = "m/s".into();
    value["current_units"]["interval"] = "minutes".into();
    assert!(parse_open_meteo(&serde_json::to_vec(&value).unwrap()).is_err());
    value["current_units"]["interval"] = "seconds".into();
    value["current_units"]["time"] = "days since 1970".into();
    assert!(parse_open_meteo(&serde_json::to_vec(&value).unwrap()).is_err());
    value["current_units"]["time"] = "unixtime".into();
    value["current"]["temperature_2m"] = serde_json::Value::Null;
    assert!(parse_open_meteo(&serde_json::to_vec(&value).unwrap()).is_err());
}

#[test]
fn offline_mode_requires_no_cache_directory_or_network() {
    let config = StreamConfig {
        cache_dir: "/not/a/writable/cache".into(),
        ..Default::default()
    };
    let position = GeoPosition::new(48.137, 11.575, 0.0).unwrap();
    let key = TileKey::at(position, 15).unwrap();
    let chunk = load_chunk(key, &config).unwrap();
    chunk.validate().unwrap();
    assert!(chunk.sources.iter().all(|s| s.synthetic));
    assert!(fetch_weather(position, &config).unwrap().synthetic);
}

#[test]
fn polar_terrain_does_not_silently_clamp_to_mercator_edge() {
    let config = StreamConfig {
        live: true,
        ..Default::default()
    };
    let key = TileKey::at(GeoPosition::new(89.0, 10.0, 0.0).unwrap(), 14).unwrap();
    let result = load_terrain(key, &config).unwrap_err().to_string();
    assert!(result.contains("85.0511"));
}
