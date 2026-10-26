use serde_json::{Value, json};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
use terra_core::{GeoBounds, GeoPosition, HeightField, SourceRecord, TileKey, WorldChunk};
use terra_history::{HistoricalCatalog, HistoricalPack, HistoryError, PoliticsDataset, parse_date};

fn metadata() -> Value {
    json!({"id":"authored-test-v1","title":"Synthetic test only","valid_from_unix":0,"valid_until_unix":100,"source_url":"urn:terra:test","license":"MIT OR Apache-2.0","attribution":"Authored synthetic fixture, not geopolitical evidence","synthetic":true})
}
fn feature(id: &str, coordinates: Value) -> Value {
    json!({"type":"Feature","properties":{"id":id,"name":id,"country":"TEST","kind":"scenario_territory","claimants":["Author A","Author B"],"status":"fictional","faction_id":7,"custom_note":"preserve me"},"geometry":{"type":"Polygon","coordinates":coordinates}})
}
fn outer() -> Value {
    json!([[
        [0.0, 0.0],
        [10.0, 0.0],
        [10.0, 10.0],
        [0.0, 10.0],
        [0.0, 0.0]
    ]])
}
fn collection(features: Vec<Value>) -> Value {
    json!({"type":"FeatureCollection","terra":metadata(),"features":features})
}
fn read(value: &Value) -> Result<PoliticsDataset, HistoryError> {
    PoliticsDataset::from_geojson(&value.to_string())
}
fn at(lat: f64, lon: f64) -> GeoPosition {
    GeoPosition::new(lat, lon, 0.0).unwrap()
}

#[test]
fn dates_are_utc_and_calendar_checked() {
    assert_eq!(parse_date("1970-01-01").unwrap(), 0);
    assert_eq!(parse_date("1969-12-31").unwrap(), -86400);
    assert_eq!(parse_date("2026-10-26").unwrap(), 1792972800);
    assert!(parse_date("2000-02-29").is_ok());
    for date in [
        "1900-02-29",
        "2026-02-30",
        "0000-01-01",
        "01/02/2026",
        "2026-13-01",
    ] {
        assert!(parse_date(date).is_err(), "{date}");
    }
}

#[test]
fn holes_and_antimeridian_membership_are_preserved() {
    let coordinates = json!([
        [
            [0.0, 0.0],
            [10.0, 0.0],
            [10.0, 10.0],
            [0.0, 10.0],
            [0.0, 0.0]
        ],
        [[4.0, 4.0], [4.0, 6.0], [6.0, 6.0], [6.0, 4.0], [4.0, 4.0]]
    ]);
    let dateline = json!([[
        [179.0, -1.0],
        [-179.0, -1.0],
        [-179.0, 1.0],
        [179.0, 1.0],
        [179.0, -1.0]
    ]]);
    let dataset = read(&collection(vec![
        feature("with-hole", coordinates),
        feature("dateline", dateline),
    ]))
    .unwrap();
    assert_eq!(dataset.at(at(2.0, 2.0), 1).unwrap().len(), 1);
    assert!(dataset.at(at(5.0, 5.0), 1).unwrap().is_empty());
    assert_eq!(dataset.at(at(0.0, 179.5), 1).unwrap()[0].id, "dateline");
    assert_eq!(dataset.at(at(0.0, -179.5), 1).unwrap()[0].id, "dateline");
    assert!(dataset.at(at(0.0, 170.0), 1).unwrap().is_empty());
    let visible = dataset
        .visible(
            GeoBounds {
                south: -0.5,
                west: 179.8,
                north: 0.5,
                east: -179.8,
            },
            1,
        )
        .unwrap();
    assert_eq!(visible.len(), 1);
}

#[test]
fn temporal_switch_is_half_open_and_overlap_claims_are_not_arbitrated() {
    let mut earlier = feature("earlier", outer());
    earlier["properties"]["valid_until_unix"] = 50.into();
    let mut later = feature("later", outer());
    later["properties"]["valid_from_unix"] = 50.into();
    let dataset = read(&collection(vec![earlier, later])).unwrap();
    assert_eq!(dataset.at(at(2.0, 2.0), 49).unwrap()[0].id, "earlier");
    assert_eq!(dataset.at(at(2.0, 2.0), 50).unwrap()[0].id, "later");
    assert!(matches!(
        dataset.at(at(2.0, 2.0), 100),
        Err(HistoryError::OutsideTime { .. })
    ));
    let overlapping = read(&collection(vec![
        feature("claim-a", outer()),
        feature("claim-b", outer()),
    ]))
    .unwrap();
    assert_eq!(overlapping.at(at(2.0, 2.0), 50).unwrap().len(), 2);
}

#[test]
fn exports_keep_holes_properties_faction_links_and_provenance() {
    let coordinates = json!([
        [
            [0.0, 0.0],
            [10.0, 0.0],
            [10.0, 10.0],
            [0.0, 10.0],
            [0.0, 0.0]
        ],
        [[4.0, 4.0], [4.0, 6.0], [6.0, 6.0], [6.0, 4.0], [4.0, 4.0]]
    ]);
    let dataset = read(&collection(vec![feature("editable", coordinates)])).unwrap();
    let output = dataset.to_geojson().unwrap();
    let reloaded = PoliticsDataset::from_geojson(&output).unwrap();
    assert_eq!(reloaded.features[0].polygons, dataset.features[0].polygons);
    assert_eq!(reloaded.features[0].faction_id, Some(7));
    assert_eq!(
        reloaded.features[0].properties["custom_note"],
        "preserve me"
    );
    assert_eq!(reloaded.attribution, dataset.attribution);
    assert_eq!(reloaded.to_geojson().unwrap(), output);
}

#[test]
fn malformed_or_undated_politics_is_rejected() {
    let mut input = collection(vec![feature("same", outer()), feature("same", outer())]);
    assert!(read(&input).unwrap_err().to_string().contains("unique"));
    input["features"].as_array_mut().unwrap().pop();
    input["features"][0]["geometry"]["coordinates"][0]
        .as_array_mut()
        .unwrap()
        .pop();
    assert!(read(&input).unwrap_err().to_string().contains("closed"));
    input = collection(vec![feature("one", outer())]);
    input.as_object_mut().unwrap().remove("terra");
    assert!(read(&input).is_err());
    input = collection(vec![feature("one", outer())]);
    input["features"][0]["properties"]["valid_from_unix"] = (-1).into();
    assert!(read(&input).is_err());
    input = collection(vec![feature("one", outer())]);
    input["terra"]["schema_version"] = "1".into();
    assert!(read(&input).is_err());
}

#[test]
fn edited_geometry_and_view_bounds_are_checked_before_export_or_query() {
    let mut dataset = read(&collection(vec![feature("one", outer())])).unwrap();
    dataset.features[0].polygons[0].rings[0][1].longitude = 361.0;
    assert!(dataset.to_geojson().is_err());
    assert!(
        dataset
            .visible(
                GeoBounds {
                    south: 0.0,
                    west: -181.0,
                    north: 10.0,
                    east: 10.0
                },
                1
            )
            .is_err()
    );
}

#[test]
fn bundled_synthetic_eras_switch_geometry_and_faction_territories() {
    let data = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data");
    let earlier = HistoricalPack::load(data.join("synthetic-era-a/manifest.json")).unwrap();
    let later = HistoricalPack::load(data.join("synthetic-era-b/manifest.json")).unwrap();
    assert!(earlier.manifest.synthetic && later.manifest.synthetic);
    assert_eq!(earlier.keys().len(), 25);
    assert_eq!(earlier.keys(), later.keys());
    let switch = parse_date("1980-01-01").unwrap();
    let key = TileKey::at(at(48.137, 11.575), 15).unwrap();
    let old = earlier.chunk(key, switch - 1).unwrap();
    let new = later.chunk(key, switch).unwrap();
    assert!(new.features.len() > old.features.len());
    assert!(earlier.chunk(key, switch).is_err());
    let politics = PoliticsDataset::load(data.join("synthetic-politics-eras.geojson")).unwrap();
    assert!(politics.synthetic);
    let before = politics.at(at(48.137, 11.575), switch - 1).unwrap();
    let after = politics.at(at(48.137, 11.575), switch).unwrap();
    assert_eq!(before.len(), 1);
    assert_eq!(after.len(), 1);
    assert_ne!(before[0].id, after[0].id);
    assert!(after[0].faction_id.is_some());
}

#[test]
fn included_natural_earth_snapshot_is_current_provenanced_and_queries_germany() {
    let dataset = PoliticsDataset::load(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("data/natural-earth-countries-2026-10-26.geojson"),
    )
    .unwrap();
    assert_eq!(dataset.features.len(), 177);
    assert!(!dataset.synthetic);
    let countries = dataset.at(at(48.137, 11.575), 1792972800).unwrap();
    assert!(countries.iter().any(|c| c.country == "DEU"));
    assert!(
        dataset
            .at(at(48.137, 11.575), parse_date("1950-01-01").unwrap())
            .is_err()
    );
    let exported = dataset.to_geojson().unwrap();
    let reloaded = PoliticsDataset::from_geojson(&exported).unwrap();
    assert_eq!(reloaded.features.len(), 177);
}

static SERIAL: AtomicU64 = AtomicU64::new(0);
struct PackFixture {
    root: PathBuf,
    manifest: Value,
}
impl PackFixture {
    fn new(id: &str, from: i64, until: i64) -> Self {
        let root = std::env::temp_dir().join(format!(
            "terra-history-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root).unwrap();
        let key = TileKey::at(at(48.137, 11.575), 15).unwrap();
        let chunk = WorldChunk {
            key,
            heights: HeightField::constant(100.0),
            features: vec![],
            sources: vec![SourceRecord {
                provider: "authored fixture".into(),
                license: "MIT OR Apache-2.0".into(),
                attribution: "synthetic, not history".into(),
                url: "urn:terra:test".into(),
                retrieved_unix_seconds: 0,
                synthetic: true,
            }],
            warnings: vec!["Authored example, not a reconstruction".into()],
        };
        std::fs::write(root.join("chunk.json"), serde_json::to_vec(&chunk).unwrap()).unwrap();
        let manifest = json!({"schema_version":1,"id":id,"title":"Synthetic historical test","valid_from_unix":from,"valid_until_unix":until,"source_url":"urn:terra:test","license":"MIT OR Apache-2.0","attribution":"Authored test, not historical truth","synthetic":true,"chunk_files":["chunk.json"]});
        Self { root, manifest }
    }
    fn load(&self) -> Result<HistoricalPack, HistoryError> {
        let path = self.root.join("manifest.json");
        std::fs::write(&path, self.manifest.to_string()).unwrap();
        HistoricalPack::load(path)
    }
}
impl Drop for PackFixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn packs_refuse_wrong_epoch_or_missing_chunk_without_current_fallback() {
    let fixture = PackFixture::new("v1", 0, 100);
    let pack = fixture.load().unwrap();
    let key = pack.keys()[0];
    assert!(pack.chunk(key, 0).is_ok());
    assert!(pack.chunk(key, 99).is_ok());
    assert!(matches!(
        pack.chunk(key, 100),
        Err(HistoryError::OutsideTime { .. })
    ));
    assert!(matches!(
        pack.chunk(
            TileKey {
                level: 15,
                x: 0,
                y: 0
            },
            1
        ),
        Err(HistoryError::MissingChunk(_))
    ));
    assert_eq!(
        pack.terrain_height(at(48.137, 11.575), 1).unwrap(),
        Some(100.0)
    );
    assert_eq!(pack.terrain_height(at(-40.0, 120.0), 1).unwrap(), None);
}

#[test]
fn pack_paths_duplicate_keys_and_false_provenance_are_rejected() {
    let mut fixture = PackFixture::new("v1", 0, 100);
    fixture.manifest["chunk_files"] = json!(["../escape.json"]);
    assert!(fixture.load().is_err());
    fixture.manifest["chunk_files"] = json!(["/tmp/absolute.json"]);
    assert!(fixture.load().is_err());
    fixture.manifest["chunk_files"] = json!(["chunk.json", "second.json"]);
    std::fs::copy(
        fixture.root.join("chunk.json"),
        fixture.root.join("second.json"),
    )
    .unwrap();
    assert!(
        fixture
            .load()
            .unwrap_err()
            .to_string()
            .contains("duplicate")
    );
    fixture.manifest["chunk_files"] = json!(["chunk.json"]);
    fixture.manifest["synthetic"] = false.into();
    assert!(fixture.load().is_err());
}

#[test]
fn pack_file_count_and_bytes_are_bounded() {
    let mut fixture = PackFixture::new("v1", 0, 100);
    fixture.manifest["chunk_files"] =
        json!((0..513).map(|i| format!("{i}.json")).collect::<Vec<_>>());
    assert!(fixture.load().unwrap_err().to_string().contains("512"));
    fixture.manifest["chunk_files"] = json!(["chunk.json"]);
    std::fs::OpenOptions::new()
        .write(true)
        .open(fixture.root.join("chunk.json"))
        .unwrap()
        .set_len(32 * 1024 * 1024 + 1)
        .unwrap();
    assert!(fixture.load().unwrap_err().to_string().contains("budget"));
}

#[test]
fn catalogs_accept_adjacent_eras_and_reject_ambiguous_coverage() {
    let first = PackFixture::new("first", 0, 50);
    let second = PackFixture::new("second", 50, 100);
    let overlap = PackFixture::new("overlap", 49, 100);
    let key = first.load().unwrap().keys()[0];
    let catalog =
        HistoricalCatalog::new(vec![first.load().unwrap(), second.load().unwrap()]).unwrap();
    assert!(catalog.chunk(key, 50).is_ok());
    assert!(HistoricalCatalog::new(vec![first.load().unwrap(), overlap.load().unwrap()]).is_err());
}

#[cfg(unix)]
#[test]
fn pack_symlinks_cannot_escape_root() {
    let mut fixture = PackFixture::new("v1", 0, 100);
    let other = PackFixture::new("outside", 0, 100);
    std::os::unix::fs::symlink(
        other.root.join("chunk.json"),
        fixture.root.join("linked.json"),
    )
    .unwrap();
    fixture.manifest["chunk_files"] = json!(["linked.json"]);
    assert!(fixture.load().unwrap_err().to_string().contains("escapes"));
}
