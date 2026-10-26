use std::path::{Path, PathBuf};
use std::process::Command;

use fl_catalog::base_mod_dir;
use fl_cli::{MapPreset, data_check, generate};
use fl_content::{ContentDb, MapDef, MapLayers, MoveClass, TerrainDef};
use fl_sim::{MobilityDef, load_content};

fn maps_dir() -> PathBuf {
    base_mod_dir().join("maps")
}

fn terrain_at(layers: &MapLayers, x: u32, y: u32) -> &str {
    &layers.palette[usize::from(layers.types[layers.index(x, y)])]
}

fn height_raw(layers: &MapLayers, x: u32, y: u32) -> u16 {
    layers.heights[layers.index(x, y)]
}

fn assert_png_matches(preset: MapPreset) {
    let (terrain, layers) = generate(preset);
    let dir = tempfile::tempdir().expect("temp dir");
    fl_content::write_layers(&terrain, dir.path(), &layers).expect("write layers");
    for name in [&terrain.height_png, &terrain.terrain_png] {
        let made = std::fs::read(dir.path().join(name)).expect("generated file");
        let committed = std::fs::read(maps_dir().join(name))
            .unwrap_or_else(|e| panic!("committed {name} is missing: {e}"));
        assert!(made == committed, "{name} differs from the committed PNG");
    }
}

#[test]
fn testbed_layers_are_generated() {
    assert_png_matches(MapPreset::Testbed);
}

#[test]
fn scale_layers_are_generated() {
    assert_png_matches(MapPreset::Scale);
}

#[test]
fn testbed_layout_matches_plan() {
    let (terrain, layers) = generate(MapPreset::Testbed);
    assert_eq!((layers.cells_x, layers.cells_y), (250, 250));
    for (x, y, id) in [
        (71, 124, "base.bridge"),
        (181, 125, "base.bridge"),
        (10, 124, "base.water"),
        (73, 124, "base.water"),
        (71, 100, "base.road"),
        (10, 61, "base.road"),
        (25, 150, "base.forest"),
        (210, 160, "base.forest"),
        (110, 40, "base.deep_forest"),
        (140, 210, "base.field"),
        (220, 70, "base.swamp"),
        (0, 0, "base.grass"),
        (249, 249, "base.grass"),
    ] {
        assert_eq!(terrain_at(&layers, x, y), id, "cell ({x}, {y})");
    }
    // Heights over (-4, 16) m: raw = (h + 4) / 20 * 65535.
    let zero = height_raw(&layers, 0, 0);
    assert_eq!(zero, 13107, "0 m");
    assert_eq!(
        height_raw(&layers, 71, 124),
        zero,
        "bridge cells stay at 0 m"
    );
    assert!(
        height_raw(&layers, 10, 124) < zero,
        "the river bed is below 0 m"
    );
    assert_eq!(height_raw(&layers, 10, 124), 6554, "-2 m");
    assert_eq!(
        height_raw(&layers, 60, 200),
        52428,
        "the hill peaks at 12 m"
    );
    assert!(height_raw(&layers, 60, 215) < 52428 && height_raw(&layers, 60, 215) > zero);
    assert_eq!(
        height_raw(&layers, 85, 200),
        zero,
        "the hill ends at its radius"
    );
    assert_eq!(
        height_raw(&layers, 141, 150),
        13107 + 19661,
        "the ridge is a 6 m step"
    );
    assert_eq!(height_raw(&layers, 139, 150), zero);
    assert_eq!(terrain.height_m.0, fl_math::Fx::from_int(-4));
    assert_eq!(terrain.height_m.1, fl_math::Fx::from_int(16));
}

#[test]
fn scale_layout_matches_plan() {
    let (_, layers) = generate(MapPreset::Scale);
    assert_eq!((layers.cells_x, layers.cells_y), (500, 500));
    assert_eq!(terrain_at(&layers, 0, 161), "base.water");
    assert_eq!(terrain_at(&layers, 499, 332), "base.water");
    assert_eq!(terrain_at(&layers, 31, 161), "base.bridge");
    assert_eq!(terrain_at(&layers, 93, 332), "base.bridge");
    assert_eq!(terrain_at(&layers, 60, 161), "base.water");
    assert!(layers.heights.iter().all(|&h| h == 0));
    let forest = layers
        .types
        .iter()
        .filter(|&&t| layers.palette[usize::from(t)] == "base.forest")
        .count();
    assert!(forest > 0 && forest <= 120 * 64, "{forest} forest cells");
}

fn load_base() -> std::sync::Arc<ContentDb> {
    load_content(&fl_catalog::features(), &[base_mod_dir()]).expect("base mod loads")
}

#[test]
fn testbed_scenario_units_on_passable_terrain() {
    let content = load_base();
    let scenario = content
        .kind::<fl_content::ScenarioDef>()
        .get("base.testbed")
        .expect("base.testbed");
    let map = content
        .kind::<MapDef>()
        .get(&scenario.map)
        .expect("the scenario's map");
    let layers = map.layers.as_ref().expect("the testbed map has layers");
    let terrains = content.kind::<TerrainDef>();
    assert!(!scenario.units.is_empty());
    for unit in &scenario.units {
        let def = content
            .unit(content.unit_id(&unit.unit).expect("unit id"))
            .expect("unit");
        let class: Option<MoveClass> = def.get::<MobilityDef>(content.registry()).map(|m| m.class);
        let cell = map.cell_m.raw();
        let x = u32::try_from(unit.pos.x.raw() / cell).expect("x");
        let y = u32::try_from(unit.pos.y.raw() / cell).expect("y");
        let id = terrain_at(layers, x, y);
        let terrain = terrains.get(id).expect("terrain def");
        if let Some(class) = class {
            assert!(
                !terrain.blocks.contains(&class),
                "{} ({class:?}) stands on {id} at ({x}, {y})",
                unit.unit
            );
        }
    }
}

#[test]
fn base_content_loads_terrain() {
    let content = load_base();
    let terrains = content.kind::<TerrainDef>();
    assert_eq!(terrains.len(), 8);
    for (preset, id) in [
        (MapPreset::Testbed, "base.testbed_1km"),
        (MapPreset::Scale, "base.scale_2km"),
    ] {
        let map = content.kind::<MapDef>().get(id).expect("map");
        let (terrain, layers) = generate(preset);
        assert_eq!(map.terrain.as_ref(), Some(&terrain), "{id} terrain");
        assert_eq!(map.layers.as_ref(), Some(&layers), "{id} layers");
        for name in &layers.palette {
            assert!(terrains.get(name).is_some(), "{name}");
        }
    }
    let report = data_check(&[base_mod_dir()]).expect("data-check");
    assert_eq!(report.units, 10);
    assert!(report.kinds.contains(&("terrain", 8)), "{:?}", report.kinds);
    let status = Command::new(env!("CARGO_BIN_EXE_fl_cli"))
        .arg("data-check")
        .arg(base_mod_dir())
        .status()
        .expect("run fl_cli");
    assert_eq!(status.code(), Some(0));
}

fn map_gen_json(preset: &str, out: &Path) -> serde_json::Value {
    let output = Command::new(env!("CARGO_BIN_EXE_fl_cli"))
        .args(["map-gen", preset, "--json", "--out"])
        .arg(out)
        .output()
        .expect("run fl_cli");
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    serde_json::from_slice(&output.stdout).expect("one JSON document")
}

#[test]
fn map_gen_cli_json() {
    let dir = tempfile::tempdir().expect("temp dir");
    let doc = map_gen_json("testbed", dir.path());
    assert_eq!(doc["files"].as_array().expect("files").len(), 2);
    assert_eq!(doc["cells"], serde_json::json!([250, 250]));
    for file in doc["files"].as_array().expect("files") {
        assert!(Path::new(file.as_str().expect("path")).is_file(), "{file}");
    }
    let (terrain, _) = generate(MapPreset::Testbed);
    let committed = std::fs::read(maps_dir().join(&terrain.terrain_png)).expect("committed");
    assert_eq!(
        std::fs::read(dir.path().join(&terrain.terrain_png)).expect("written"),
        committed
    );
}

#[test]
fn map_gen_cli_missing_dir_fails() {
    let dir = tempfile::tempdir().expect("temp dir");
    let output = Command::new(env!("CARGO_BIN_EXE_fl_cli"))
        .args(["map-gen", "scale", "--out"])
        .arg(dir.path().join("nope"))
        .output()
        .expect("run fl_cli");
    assert_eq!(output.status.code(), Some(1));
}
