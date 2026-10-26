use terra_core::{FeatureKind, GeoPosition, TileKey};
use terra_stream::{
    StreamConfig, fetch_weather, load_chunk,
    providers::{decode_terrarium_pixel, load_terrain, parse_open_meteo, parse_overpass},
};

#[test]
#[expect(
    clippy::excessive_precision,
    reason = "retain the exact decoded Terrarium reference value in the assertion"
)]
fn documented_terrarium_encoding_decodes_exactly() {
    assert_eq!(decode_terrarium_pixel([128, 0, 0]), 0.0);
    assert_eq!(decode_terrarium_pixel([137, 219, 68]), 2523.265625);
    assert_eq!(decode_terrarium_pixel([127, 255, 128]), -0.5);
}

#[test]
fn osm_features_keep_source_ids_and_reject_invalid_geometry() {
    let features = parse_overpass(include_bytes!("fixtures/overpass.json")).unwrap();
    assert_eq!(features.len(), 4);
    assert_eq!(features[0].id, "osm:way:1");
    assert_eq!(features[0].kind, FeatureKind::Building);
    assert_eq!(features[0].height_m, 9.0);
    assert_eq!(features[2].height_m, 12.0);
    assert_eq!(features[3].kind, FeatureKind::Waterway);
    for feature in features {
        feature.validate().unwrap();
    }
    assert!(parse_overpass(br#"{"remark":"runtime error: timeout", "elements":[]}"#).is_err());
}

#[test]
fn osm_semantics_keep_shops_roofs_lanes_bridges_and_physical_units() {
    let input = br##"{"elements":[
      {"type":"node","id":20,"lat":48.137,"lon":11.575,"tags":{"shop":"bakery","name":"Authored fixture bakery","opening_hours":"Mo-Fr 08:00-18:00"}},
      {"type":"way","id":21,"tags":{"highway":"residential","width":"10 ft","lanes":"2","bridge":"yes","layer":"2","maxwidth":"1.5"},"geometry":[{"lat":48.137,"lon":11.575},{"lat":48.138,"lon":11.576}]},
      {"type":"way","id":22,"tags":{"building":"retail","shop":"general","height":"30 ft","roof:shape":"gabled","roof:height":"2.5","roof:colour":"#885533","building:material":"brick"},"geometry":[{"lat":48.137,"lon":11.575},{"lat":48.137,"lon":11.576},{"lat":48.138,"lon":11.576},{"lat":48.138,"lon":11.575},{"lat":48.137,"lon":11.575}]}
    ]}"##;
    let features = parse_overpass(input).unwrap();
    assert_eq!(features.len(), 3);
    assert_eq!(features[0].kind, FeatureKind::PointOfInterest);
    assert_eq!(features[0].properties["shop"], "bakery");
    assert_eq!(features[0].properties["opening_hours"], "Mo-Fr 08:00-18:00");
    assert!((features[1].height_m - 3.048).abs() < 0.001);
    assert_eq!(features[1].properties["bridge"], "yes");
    assert_eq!(features[1].properties["layer"], "2");
    assert!((features[2].height_m - 9.144).abs() < 0.001);
    assert_eq!(features[2].properties["roof:shape"], "gabled");
    assert_eq!(features[2].properties["roof:colour"], "#885533");
    for feature in features {
        feature.validate().unwrap();
    }
    assert!(terra_stream::providers::parse_osm_length("NaN").is_none());
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
