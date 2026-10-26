use terra_core::*;

fn geo(lat: f64, lon: f64, altitude: f64) -> GeoPosition {
    GeoPosition::new(lat, lon, altitude).unwrap()
}

#[test]
fn wgs84_roundtrips_across_world_and_altitudes() {
    for lat in [-90.0, -89.9999, -65.0, -0.001, 0.0, 48.137, 89.9999, 90.0] {
        for lon in [-180.0, -77.4, 0.0, 151.2, 179.9999] {
            for altitude in [-11_000.0, 0.0, 523.4, 100_000.0, 35_786_000.0] {
                let p = geo(lat, lon, altitude);
                let q = GeoPosition::from_ecef(p.to_ecef()).unwrap();
                assert!((p.latitude - q.latitude).abs() < 1e-8, "{p:?} -> {q:?}");
                if lat.abs() < 90.0 {
                    assert!(normalize_longitude(p.longitude - q.longitude).abs() < 1e-8);
                }
                assert!((p.altitude_m - q.altitude_m).abs() < 1e-4);
            }
        }
    }
}

#[test]
fn coordinate_validation_and_wrap() {
    assert!(geo(0.0, 540.0, 0.0).longitude == -180.0);
    assert!(GeoPosition::new(91.0, 0.0, 0.0).is_err());
    assert!(GeoPosition::new(0.0, f64::NAN, 0.0).is_err());
    assert!(GeoPosition::from_ecef([0.0; 3]).is_err());
    assert!(
        (geo(0.0, 179.999, 0.0).surface_distance_m(geo(0.0, -179.999, 0.0)) - 222.39).abs() < 0.1
    );
}

#[test]
fn local_frame_axes_and_roundtrip() {
    let frame = LocalFrame::new(geo(48.137, 11.575, 520.0));
    let north = frame.to_local(geo(48.138, 11.575, 520.0));
    let east = frame.to_local(geo(48.137, 11.576, 520.0));
    let up = frame.to_local(geo(48.137, 11.575, 620.0));
    assert!(north[2] < -110.0 && east[0] > 70.0 && up[1] > 99.99);
    let point = geo(48.145, 11.6, 725.4);
    let restored = frame.to_geo(frame.to_local(point)).unwrap();
    assert!(point.surface_distance_m(restored) < 1e-5);
    assert!((restored.altitude_m - point.altitude_m).abs() < 1e-5);
}

#[test]
fn tiles_cover_poles_dateline_and_hierarchy() {
    let west = TileKey::at(geo(0.0, -180.0, 0.0), 3).unwrap();
    assert_eq!(west.x, 0);
    assert_eq!(west, TileKey::at(geo(0.0, 180.0, 0.0), 3).unwrap());
    assert!(west.neighbors(1).iter().any(|k| k.x == 7));
    let north = TileKey::at(geo(90.0, 0.0, 0.0), 3).unwrap();
    assert_eq!(north.y, 0);
    assert_eq!(north.neighbors(1).len(), 6);
    let south = TileKey::at(geo(-90.0, 0.0, 0.0), 3).unwrap();
    assert_eq!(south.y, 7);
    for child in west.children().unwrap() {
        assert_eq!(child.parent(), Some(west));
    }
    assert_eq!(TileKey::new(0, 0, 0).unwrap().neighbors(64).len(), 1);
    assert!(
        TileKey::new(MAX_TILE_LEVEL, 0, 0)
            .unwrap()
            .children()
            .is_err()
    );
    assert!(TileKey::new(255, 0, 0).is_err());
    assert!(TileKey::new(2, 4, 0).is_err());
}

#[test]
fn height_field_interpolation_orientation_and_invalid_data() {
    let field = HeightField {
        width: 2,
        height: 2,
        samples_m: vec![0.0, 10.0, 20.0, 30.0],
    };
    assert_eq!(field.sample(0.5, 0.5), 15.0);
    assert_eq!(field.sample(0.0, 0.0), 0.0);
    assert_eq!(field.sample(0.0, 1.0), 20.0);
    assert_eq!(field.sample(2.0, -1.0), 10.0);
    let malformed = HeightField {
        width: usize::MAX,
        height: 2,
        samples_m: vec![],
    };
    assert!(malformed.validate().is_err());
    assert!(malformed.try_sample(0.5, 0.5).is_err());
    assert_eq!(malformed.sample(0.5, 0.5), 0.0);
    let missing = HeightField::constant(f32::NAN);
    assert!(missing.validate().is_err());
}

#[test]
fn calendar_handles_epoch_negative_time_and_leap_day() {
    let mut clock = WorldClock {
        unix_seconds: 0.0,
        time_scale: 1.0,
        paused: false,
    };
    assert_eq!(clock.date(), (1970, 1, 1));
    assert_eq!(clock.day_of_year(), 1);
    clock.unix_seconds = -1.0;
    assert_eq!(clock.date(), (1969, 12, 31));
    assert_eq!(clock.day_of_year(), 365);
    clock.unix_seconds = 1_709_164_800.0; // 2024-02-29T00:00Z
    assert_eq!(clock.date(), (2024, 2, 29));
    assert_eq!(clock.day_of_year(), 60);
    assert_eq!(clock.days_in_year(), 366);
    assert_eq!(WorldClock::default().date(), (2026, 6, 21));
    assert!(!is_leap_year(1900));
    assert!(is_leap_year(2000));
}

#[test]
fn clock_pauses_and_rejects_bad_delta() {
    let mut clock = WorldClock::default();
    let before = clock.unix_seconds;
    clock.advance(2.0);
    assert_eq!(clock.unix_seconds, before + 120.0);
    clock.paused = true;
    clock.advance(1_000.0);
    assert_eq!(clock.unix_seconds, before + 120.0);
    clock.paused = false;
    clock.advance(f64::NAN);
    assert_eq!(clock.unix_seconds, before + 120.0);
}

#[test]
fn solar_obeys_day_night_longitude_and_polar_seasons() {
    let mut clock = WorldClock::default();
    let north = geo(80.0, 0.0, 0.0);
    assert!(solar_position(&north, &clock).elevation_deg > 0.0);
    clock.unix_seconds += 12.0 * 3_600.0;
    assert!(solar_position(&north, &clock).elevation_deg > 0.0); // midnight sun
    let equator = geo(0.0, 0.0, 0.0);
    assert!(solar_position(&equator, &clock).elevation_deg < -60.0);
    assert!(solar_position(&geo(0.0, 180.0, 0.0), &clock).elevation_deg > 60.0);
    for lat in [-90.0, 90.0] {
        let sun = solar_position(&geo(lat, 0.0, 0.0), &clock);
        assert!(sun.azimuth_deg.is_finite() && sun.elevation_deg.is_finite());
    }
}

#[test]
fn seasons_are_opposite_and_weather_is_seeded() {
    let clock = WorldClock::default();
    assert_eq!(season(&geo(45.0, 0.0, 0.0), &clock), Season::Summer);
    assert_eq!(season(&geo(-45.0, 0.0, 0.0), &clock), Season::Winter);
    let pos = geo(48.137, 11.575, 520.0);
    let a = synthetic_weather(&pos, &clock, 7);
    assert_eq!(a, synthetic_weather(&pos, &clock, 7));
    assert_ne!(a, synthetic_weather(&pos, &clock, 8));
    a.validate().unwrap();
    assert!(a.synthetic);
}

#[test]
fn weather_wind_is_from_not_toward_and_biomes_are_explicit_heuristics() {
    let wind = WeatherSample {
        wind_speed_m_s: 10.0,
        wind_direction_deg: 0.0,
        ..Default::default()
    };
    assert_eq!(wind.wind_enu_m_s(), [-0.0, -10.0, 0.0]);
    assert_eq!(
        classify_biome(&geo(0.0, 0.0, 10.0), 28.0, 2_000.0),
        Biome::TropicalForest
    );
    assert_eq!(
        classify_biome(&geo(30.0, 0.0, 500.0), 25.0, 50.0),
        Biome::Desert
    );
}

#[test]
fn actors_cross_dateline_preserve_altitude_and_reject_bad_input() {
    let mut actor = ActorState {
        id: 1,
        kind: ActorKind::Airplane,
        position: geo(0.0, 179.999, 1_000.0),
        velocity_enu_m_s: [250.0, 0.0, 3.0],
        heading_deg: 0.0,
    };
    actor.advance(1.0).unwrap();
    assert!(actor.position.longitude < -179.0);
    assert_eq!(actor.position.altitude_m, 1_003.0);
    assert_eq!(actor.heading_deg, 90.0);
    let old = actor.clone();
    assert!(actor.advance(f64::INFINITY).is_err());
    assert_eq!(old, actor);
    actor.position = geo(89.999, 10.0, 0.0);
    actor.velocity_enu_m_s = [0.0, 250.0, 0.0];
    actor.advance(1.0).unwrap();
    actor.validate().unwrap();
    assert!(actor.position.latitude < 90.0);
}

#[test]
fn domain_transport_roundtrip_preserves_provenance_and_layers() {
    let chunk = WorldChunk {
        key: TileKey::at(geo(48.0, 11.0, 0.0), 14).unwrap(),
        heights: HeightField::constant(50.0),
        features: vec![],
        sources: vec![SourceRecord {
            provider: "test".into(),
            license: "CC0".into(),
            attribution: "test".into(),
            url: "https://example.com".into(),
            retrieved_unix_seconds: 1,
            synthetic: false,
        }],
        warnings: vec![],
    };
    let restored: WorldChunk =
        serde_json::from_str(&serde_json::to_string(&chunk).unwrap()).unwrap();
    restored.validate().unwrap();
    assert_eq!(chunk, restored);
    assert!(!chunk.is_synthetic());
    let mut layers = LayerMask::all();
    layers.set(Layer::Roads, false);
    assert!(!layers.contains(Layer::Roads) && layers.contains(Layer::Terrain));
}
