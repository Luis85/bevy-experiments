//! Terrain content (ADR-0014): `*.terrain.ron`, PNG map layers decoded through the sidecar
//! hook, the 255-kind cap and the rules-section link hook. Every mod is written into a temp
//! dir; no binary fixture is committed.

use std::fs::{self, File};
use std::io::BufWriter;
use std::path::{Path, PathBuf};

use fl_content::{
    ContentDb, ContentErrors, DefRegistry, LinkCtx, MapDef, MapLayers, MapTerrain, MoveClass,
    RulesSection, TerrainDef, decode_layers, load_dirs, write_layers,
};
use fl_math::Fx;
use proptest::prelude::*;
use serde::{Deserialize, Serialize};
use tempfile::TempDir;

type Rgb = (u8, u8, u8);

const RED: Rgb = (255, 0, 0);
const BLUE: Rgb = (0, 0, 255);
const MAGENTA: Rgb = (255, 0, 255);
const GRASS: &str = "t.grass";
const WATER: &str = "t.water";
/// The test maps' cell side, in metres.
const CELL_M: u32 = 4;

// ---------------------------------------------------------------- helpers

/// A `*.terrain.ron` file's text.
fn terrain_ron(id: &str, pass: &str, blocks: &str) -> String {
    format!(
        "TerrainDef(id: \"{id}\", name: \"{id}\", pass: {pass}, blocks: [{blocks}], \
         color: (0.2, 0.5, 0.2))"
    )
}

/// Grass (open) and water (blocks every class).
fn grass_water() -> Vec<String> {
    vec![
        terrain_ron(GRASS, "1.0", ""),
        terrain_ron(WATER, "0.0", "Foot, Wheeled, HalfTrack, Tracked"),
    ]
}

/// Red is grass, blue is water.
fn terrain_with(legend: &[(Rgb, &str)]) -> MapTerrain {
    MapTerrain {
        height_png: "h.png".into(),
        terrain_png: "t.png".into(),
        height_m: (Fx::ZERO, Fx::from_int(100)),
        legend: legend
            .iter()
            .map(|(c, id)| (*c, (*id).to_owned()))
            .collect(),
    }
}

fn terrain() -> MapTerrain {
    terrain_with(&[(RED, GRASS), (BLUE, WATER)])
}

/// `MapTerrain` as RON, for a map file.
fn terrain_text(terrain: &MapTerrain) -> String {
    let legend: Vec<String> = terrain
        .legend
        .iter()
        .map(|((r, g, b), id)| format!("(({r}, {g}, {b}), \"{id}\")"))
        .collect();
    format!(
        "(height_png: \"{}\", terrain_png: \"{}\", height_m: ({}, {}), legend: [{}])",
        terrain.height_png,
        terrain.terrain_png,
        terrain.height_m.0,
        terrain.height_m.1,
        legend.join(", ")
    )
}

/// A `t.map` of `cells` with `extra` fields appended (`, terrain: Some(…)`).
fn map_ron(cells: (u32, u32), extra: &str) -> String {
    format!(
        "MapDef(id: \"t.map\", name: \"Map\", size_m: ({}.0, {}.0), cell_m: {CELL_M}.0, \
         ground_color: (0.3, 0.5, 0.2){extra})",
        cells.0 * CELL_M,
        cells.1 * CELL_M
    )
}

fn map_with_terrain(cells: (u32, u32), terrain: &MapTerrain) -> String {
    map_ron(
        cells,
        &format!(", terrain: Some({})", terrain_text(terrain)),
    )
}

/// Layers over `palette` with `types(x, y)` and every height `0`.
fn layers(cells: (u32, u32), palette: &[&str], types: impl Fn(u32, u32) -> u8) -> MapLayers {
    let mut out = MapLayers {
        cells_x: cells.0,
        cells_y: cells.1,
        palette: palette.iter().map(|s| (*s).to_owned()).collect(),
        types: Vec::new(),
        heights: vec![0; (cells.0 * cells.1) as usize],
    };
    for y in 0..cells.1 {
        for x in 0..cells.0 {
            out.types.push(types(x, y));
        }
    }
    out
}

/// Mod `t` in `dir`: `terrains`, `maps/t.map.ron` holding `map_ron`, and `layers` written
/// next to the map with [`write_layers`].
fn write_mod(
    dir: &Path,
    terrains: &[String],
    map_ron: &str,
    layers: Option<(&MapTerrain, &MapLayers)>,
) {
    fs::write(dir.join("mod.ron"), "Mod(id: \"t\", version: \"1.0.0\")").unwrap();
    fs::create_dir_all(dir.join("terrain")).unwrap();
    for (i, text) in terrains.iter().enumerate() {
        fs::write(dir.join(format!("terrain/k{i:03}.terrain.ron")), text).unwrap();
    }
    let maps = dir.join("maps");
    fs::create_dir_all(&maps).unwrap();
    fs::write(maps.join("t.map.ron"), map_ron).unwrap();
    if let Some((terrain, layers)) = layers {
        write_layers(terrain, &maps, layers).unwrap();
    }
}

fn load(dir: &Path) -> Result<ContentDb, ContentErrors> {
    load_dirs(DefRegistry::with_builtins(), &[dir.to_path_buf()])
}

fn load_ok(dir: &Path) -> ContentDb {
    load(dir).unwrap_or_else(|errs| panic!("should load:\n{errs}"))
}

fn load_err(dir: &Path) -> ContentErrors {
    match load(dir) {
        Ok(_) => panic!("should fail to load"),
        Err(errs) => errs,
    }
}

/// Asserts that some error's display contains every one of `needles`.
fn assert_error(errs: &ContentErrors, needles: &[&str]) {
    assert!(
        errs.0.iter().any(|e| {
            let text = e.to_string();
            needles.iter().all(|n| text.contains(n))
        }),
        "no error contains all of {needles:?}; errors:\n{errs}"
    );
}

/// Asserts that some error sits at field `path` and its message contains `needle`.
fn assert_error_at(errs: &ContentErrors, path: &str, needle: &str) {
    assert!(
        errs.0
            .iter()
            .any(|e| e.path == path && e.message.contains(needle)),
        "no error at {path:?} containing {needle:?}; errors:\n{errs}"
    );
}

fn map(db: &ContentDb) -> &MapDef {
    db.kind::<MapDef>().get("t.map").expect("t.map loaded")
}

/// A PNG of `color` type and `depth` from raw rows, north row first. Indexed images get a
/// one-entry palette (red).
fn write_png(
    path: &Path,
    size: (u32, u32),
    color: png::ColorType,
    depth: png::BitDepth,
    data: &[u8],
) {
    let file = BufWriter::new(File::create(path).unwrap());
    let mut encoder = png::Encoder::new(file, size.0, size.1);
    encoder.set_color(color);
    encoder.set_depth(depth);
    if color == png::ColorType::Indexed {
        encoder.set_palette(vec![255, 0, 0]);
    }
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(data).unwrap();
    writer.finish().unwrap();
}

fn write_rgb(path: &Path, size: (u32, u32), pixel: impl Fn(u32, u32) -> Rgb) {
    let mut data = Vec::new();
    for py in 0..size.1 {
        for px in 0..size.0 {
            let (r, g, b) = pixel(px, py);
            data.extend([r, g, b]);
        }
    }
    write_png(path, size, png::ColorType::Rgb, png::BitDepth::Eight, &data);
}

fn write_gray16(path: &Path, size: (u32, u32), value: u16) {
    let data: Vec<u8> = (0..size.0 * size.1)
        .flat_map(|_| value.to_be_bytes())
        .collect();
    write_png(
        path,
        size,
        png::ColorType::Grayscale,
        png::BitDepth::Sixteen,
        &data,
    );
}

fn temp() -> TempDir {
    tempfile::tempdir().unwrap()
}

// ---------------------------------------------------------------- loading

#[test]
fn terrain_kind_loads() {
    let dir = temp();
    let terrain = terrain();
    let layers = layers((4, 3), &[GRASS, WATER], |x, _| u8::from(x == 3));
    write_mod(
        dir.path(),
        &grass_water(),
        &map_with_terrain((4, 3), &terrain),
        Some((&terrain, &layers)),
    );
    let db = load_ok(dir.path());
    let kinds = db.kind::<TerrainDef>();
    assert_eq!(kinds.len(), 2);
    let water = kinds.get(WATER).unwrap();
    assert_eq!(water.pass, Fx::ZERO);
    assert_eq!(water.blocks, MoveClass::ALL);
    assert_eq!(kinds.get(GRASS).unwrap().pass, Fx::ONE);
    let map = map(&db);
    assert_eq!(map.terrain.as_ref(), Some(&terrain));
    assert_eq!(map.layers.as_ref(), Some(&layers));
}

#[test]
fn move_class_order_and_index() {
    assert_eq!(
        MoveClass::ALL,
        [
            MoveClass::Foot,
            MoveClass::Wheeled,
            MoveClass::HalfTrack,
            MoveClass::Tracked
        ]
    );
    for (i, class) in MoveClass::ALL.into_iter().enumerate() {
        assert_eq!(class.index(), i);
    }
}

#[test]
fn layers_are_north_up() {
    let dir = temp();
    let terrain = terrain();
    write_mod(
        dir.path(),
        &grass_water(),
        &map_with_terrain((4, 3), &terrain),
        None,
    );
    let maps = dir.path().join("maps");
    write_rgb(&maps.join("t.png"), (4, 3), |px, py| {
        if (px, py) == (1, 0) { RED } else { BLUE }
    });
    write_gray16(&maps.join("h.png"), (4, 3), 0);
    let db = load_ok(dir.path());
    let layers = map(&db).layers.as_ref().unwrap();
    assert_eq!(layers.palette, [GRASS, WATER]);
    // Pixel row 0 is the north edge, the map's top row y = cells_y - 1.
    assert_eq!(layers.index(1, 2), 2 * 4 + 1);
    for y in 0..3 {
        for x in 0..4 {
            let expected = if (x, y) == (1, 2) { 0 } else { 1 };
            assert_eq!(
                layers.types[layers.index(x, y)],
                expected,
                "cell ({x}, {y})"
            );
        }
    }
}

#[test]
fn layer_size_mismatch_is_error() {
    let dir = temp();
    let terrain = terrain();
    write_mod(
        dir.path(),
        &grass_water(),
        &map_with_terrain((4, 3), &terrain),
        None,
    );
    let maps = dir.path().join("maps");
    write_rgb(&maps.join("t.png"), (4, 2), |_, _| RED);
    write_gray16(&maps.join("h.png"), (4, 3), 0);
    let errs = load_err(dir.path());
    assert_error_at(
        &errs,
        "t.map/terrain.terrain_png",
        "expected 4x3 pixels, found 4x2",
    );
    assert_error(&errs, &["t.png", "expected 4x3 pixels, found 4x2"]);

    // A height PNG of the wrong size is reported at its own field.
    write_rgb(&maps.join("t.png"), (4, 3), |_, _| RED);
    write_gray16(&maps.join("h.png"), (3, 3), 0);
    let errs = load_err(dir.path());
    assert_error_at(
        &errs,
        "t.map/terrain.height_png",
        "expected 4x3 pixels, found 3x3",
    );
}

#[test]
fn unknown_legend_colour_is_error() {
    let dir = temp();
    let terrain = terrain();
    write_mod(
        dir.path(),
        &grass_water(),
        &map_with_terrain((4, 3), &terrain),
        None,
    );
    let maps = dir.path().join("maps");
    write_rgb(&maps.join("t.png"), (4, 3), |px, py| {
        if (px, py) == (2, 1) { MAGENTA } else { RED }
    });
    write_gray16(&maps.join("h.png"), (4, 3), 0);
    let errs = load_err(dir.path());
    assert_error(
        &errs,
        &["t.png", "pixel (2, 1) colour #ff00ff is not in the legend"],
    );
    assert_error_at(&errs, "t.map/terrain.terrain_png", "colour #ff00ff");
}

#[test]
fn legend_naming_unknown_terrain_is_error() {
    let dir = temp();
    let terrain = terrain_with(&[(RED, GRASS), (BLUE, "t.nope")]);
    let layers = layers((4, 3), &[GRASS, "t.nope"], |_, _| 0);
    write_mod(
        dir.path(),
        &grass_water(),
        &map_with_terrain((4, 3), &terrain),
        Some((&terrain, &layers)),
    );
    let errs = load_err(dir.path());
    assert_error_at(&errs, "t.map/terrain.legend[1]", "t.nope");
}

#[test]
fn heights_accept_16_and_8_bit() {
    let dir = temp();
    let terrain = terrain();
    write_rgb(&dir.path().join("t.png"), (2, 1), |_, _| RED);

    let h16: Vec<u8> = [65535u16, 0].iter().flat_map(|v| v.to_be_bytes()).collect();
    let h = dir.path().join("h.png");
    write_png(
        &h,
        (2, 1),
        png::ColorType::Grayscale,
        png::BitDepth::Sixteen,
        &h16,
    );
    let layers = decode_layers(&terrain, dir.path(), (2, 1)).unwrap();
    assert_eq!(layers.heights, [65535, 0]);

    write_png(
        &h,
        (2, 1),
        png::ColorType::Grayscale,
        png::BitDepth::Eight,
        &[255, 1],
    );
    let layers = decode_layers(&terrain, dir.path(), (2, 1)).unwrap();
    assert_eq!(layers.heights, [65535, 257]);
}

/// What `rules_hash_changes_with_one_pixel` changes from the base mod.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Flip {
    Nothing,
    TerrainPixel,
    HeightPixel,
    LegendColour,
}

#[test]
fn rules_hash_changes_with_one_pixel() {
    let hash = |flip: Flip| {
        let dir = temp();
        let terrain = if flip == Flip::LegendColour {
            terrain_with(&[((254, 0, 0), GRASS), (BLUE, WATER)])
        } else {
            terrain()
        };
        let mut layers = layers((4, 3), &[GRASS, WATER], |x, y| {
            u8::from(flip == Flip::TerrainPixel && (x, y) == (3, 1))
        });
        if flip == Flip::HeightPixel {
            let i = layers.index(2, 2);
            layers.heights[i] = 1;
        }
        write_mod(
            dir.path(),
            &grass_water(),
            &map_with_terrain((4, 3), &terrain),
            Some((&terrain, &layers)),
        );
        load_ok(dir.path()).rules_hash()
    };
    let base = hash(Flip::Nothing);
    assert_eq!(base, hash(Flip::Nothing));
    for flip in [Flip::TerrainPixel, Flip::HeightPixel, Flip::LegendColour] {
        assert_ne!(base, hash(flip), "{flip:?}");
    }
}

/// A map with terrain whose grid would exceed 4096 cells per side (R89).
fn huge_grid_errors(size_m: &str, cell_m: &str) -> ContentErrors {
    let dir = temp();
    let map = format!(
        "MapDef(id: \"t.map\", name: \"Map\", size_m: ({size_m}, {size_m}), cell_m: {cell_m}, \
         ground_color: (0.3, 0.5, 0.2), terrain: Some({}))",
        terrain_text(&terrain())
    );
    write_mod(dir.path(), &grass_water(), &map, None);
    load_err(dir.path())
}

#[test]
fn grid_over_4096_cells_per_side_is_error() {
    // 8 m / 2^-10 m = 8192 cells per side: an exact divisor, so only the cap stops it.
    let errs = huge_grid_errors("8.0", "0.0009765625");
    assert_error_at(&errs, "t.map/cell_m", "8192 cells");
    assert_error_at(&errs, "t.map/cell_m", "4096");
    // The sidecar skipped the decode: no PNG was opened (none exists).
    assert!(errs.0.iter().all(|e| !e.message.contains(".png")), "{errs}");
    // One error per axis, both at the cell side.
    assert_eq!(errs.0.len(), 2, "{errs}");
    assert!(errs.0.iter().all(|e| e.path == "t.map/cell_m"), "{errs}");

    let errs = huge_grid_errors("4.0", "0.0001");
    assert_error_at(&errs, "t.map/cell_m", "4096");
    assert!(errs.0.iter().all(|e| !e.message.contains(".png")), "{errs}");

    // 4096 cells per side is still allowed.
    let dir = temp();
    let map = "MapDef(id: \"t.map\", name: \"Map\", size_m: (4096.0, 4.0), cell_m: 1.0, \
               ground_color: (0.3, 0.5, 0.2))";
    write_mod(dir.path(), &grass_water(), map, None);
    load_ok(dir.path());
}

#[test]
fn decode_layers_rejects_oversized_grids() {
    let dir = temp();
    for cells in [(4097, 1), (1, 4097), (0, 1)] {
        let err = decode_layers(&terrain(), dir.path(), cells).unwrap_err();
        assert!(err.contains("4096"), "{cells:?}: {err}");
    }
}

#[test]
fn png_paths_must_stay_next_to_the_map() {
    let outside = temp();
    let absolute = outside.path().join("h.png");
    write_gray16(&absolute, (4, 3), 0);
    for (field, name) in [
        ("terrain_png", "../t.png"),
        ("terrain_png", "sub/../../t.png"),
        ("height_png", absolute.to_str().unwrap()),
        ("height_png", ""),
        // Not portable: a separator or drive prefix on Windows only.
        ("terrain_png", "sub\\t.png"),
        ("terrain_png", "c:t.png"),
    ] {
        let dir = temp();
        let mut terrain = terrain();
        let layers = layers((4, 3), &[GRASS, WATER], |_, _| 0);
        write_mod(
            dir.path(),
            &grass_water(),
            &map_ron((4, 3), ""),
            Some((&terrain, &layers)),
        );
        match field {
            "terrain_png" => terrain.terrain_png = name.into(),
            _ => terrain.height_png = name.into(),
        }
        // RON strings escape backslashes (Windows absolute paths).
        let map = map_with_terrain((4, 3), &terrain).replace('\\', "\\\\");
        fs::write(dir.path().join("maps/t.map.ron"), map).unwrap();
        let errs = load_err(dir.path());
        assert_error_at(&errs, &format!("t.map/terrain.{field}"), "relative");
        assert_eq!(errs.0.len(), 1, "{name:?}: {errs}");

        let err = decode_layers(&terrain, &dir.path().join("maps"), (4, 3)).unwrap_err();
        assert!(
            err.contains(field) && err.contains("relative"),
            "{name:?}: {err}"
        );
    }
    // A subdirectory below the map's directory is fine.
    let dir = temp();
    let mut terrain = terrain();
    terrain.terrain_png = "layers/t.png".into();
    terrain.height_png = "./layers/h.png".into();
    let layers = layers((4, 3), &[GRASS, WATER], |_, _| 1);
    fs::create_dir_all(dir.path().join("maps/layers")).unwrap();
    write_mod(
        dir.path(),
        &grass_water(),
        &map_with_terrain((4, 3), &terrain),
        Some((&terrain, &layers)),
    );
    assert_eq!(map(&load_ok(dir.path())).layers.as_ref(), Some(&layers));
}

#[test]
fn map_without_terrain_has_no_layers() {
    let dir = temp();
    write_mod(dir.path(), &grass_water(), &map_ron((4, 3), ""), None);
    let db = load_ok(dir.path());
    assert_eq!(map(&db).terrain, None);
    assert_eq!(map(&db).layers, None);
}

#[test]
fn layers_cannot_be_written_in_ron() {
    let dir = temp();
    let extra = ", layers: Some((cells_x: 1, cells_y: 1, palette: [], types: [0], heights: [0]))";
    write_mod(dir.path(), &grass_water(), &map_ron((1, 1), extra), None);
    let errs = load_err(dir.path());
    assert_error(&errs, &["t.map.ron", "`layers`"]);
}

#[test]
fn pass_out_of_range_is_error() {
    let dir = temp();
    let terrains = [
        terrain_ron("t.mud", "1.5", ""),
        terrain_ron("t.tar", "-0.1", ""),
    ];
    write_mod(dir.path(), &terrains, &map_ron((1, 1), ""), None);
    let errs = load_err(dir.path());
    assert_error_at(&errs, "t.mud/pass", "[0, 1]");
    assert_error_at(&errs, "t.tar/pass", "[0, 1]");
}

#[test]
fn duplicate_blocks_is_error() {
    let dir = temp();
    let terrains = [terrain_ron("t.swamp", "0.5", "Tracked, Foot, Tracked")];
    write_mod(dir.path(), &terrains, &map_ron((1, 1), ""), None);
    let errs = load_err(dir.path());
    assert_error_at(&errs, "t.swamp/blocks", "Tracked");
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    #[test]
    fn layers_round_trip(
        cells_x in 1u32..16,
        cells_y in 1u32..16,
        seed in proptest::collection::vec((0u8..3, any::<u16>()), 256),
    ) {
        let dir = temp();
        let terrain = terrain_with(&[(RED, GRASS), ((0, 200, 0), "t.forest"), (BLUE, WATER)]);
        let n = (cells_x * cells_y) as usize;
        let layers = MapLayers {
            cells_x,
            cells_y,
            palette: vec!["t.forest".into(), GRASS.into(), WATER.into()],
            types: seed.iter().take(n).map(|(t, _)| *t).collect(),
            heights: seed.iter().take(n).map(|(_, h)| *h).collect(),
        };
        write_layers(&terrain, dir.path(), &layers).unwrap();
        let decoded = decode_layers(&terrain, dir.path(), (cells_x, cells_y)).unwrap();
        prop_assert_eq!(decoded, layers);
    }
}

// ---------------------------------------------------------------- PNG formats

#[test]
fn unsupported_png_formats_are_errors() {
    use png::BitDepth::{Eight, Four, Sixteen};
    use png::ColorType::{Grayscale, GrayscaleAlpha, Indexed, Rgb};

    let dir = temp();
    let terrain = terrain();
    let t = dir.path().join("t.png");
    let h = dir.path().join("h.png");
    let good_terrain = |path: &PathBuf| write_rgb(path, (2, 1), |_, _| RED);
    let good_height = |path: &PathBuf| write_gray16(path, (2, 1), 0);
    let check = |file: &str, colour: &str, depth: &str| {
        let err = decode_layers(&terrain, dir.path(), (2, 1)).unwrap_err();
        for needle in [
            file,
            &format!("colour type {colour}"),
            &format!("bit depth {depth}"),
        ] {
            assert!(err.contains(needle), "{needle:?} missing from: {err}");
        }
    };

    good_height(&h);
    write_png(&t, (2, 1), Indexed, Eight, &[0, 0]);
    check("t.png", "Indexed", "8");
    write_png(&t, (2, 1), GrayscaleAlpha, Eight, &[0, 255, 0, 255]);
    check("t.png", "GrayscaleAlpha", "8");
    write_png(&t, (2, 1), Grayscale, Four, &[0]);
    check("t.png", "Grayscale", "4");
    write_png(&t, (2, 1), Grayscale, Eight, &[0, 0]);
    check("t.png", "Grayscale", "8");
    write_png(
        &t,
        (2, 1),
        Rgb,
        Sixteen,
        &[255, 255, 0, 0, 0, 0, 255, 255, 0, 0, 0, 0],
    );
    check("t.png", "Rgb", "16");

    good_terrain(&t);
    write_png(&h, (2, 1), Rgb, Eight, &[0, 0, 0, 0, 0, 0]);
    check("h.png", "Rgb", "8");
    write_png(&h, (2, 1), Grayscale, Four, &[0]);
    check("h.png", "Grayscale", "4");
    write_png(&h, (2, 1), GrayscaleAlpha, Eight, &[0, 255, 0, 255]);
    check("h.png", "GrayscaleAlpha", "8");
    write_png(&h, (2, 1), GrayscaleAlpha, Sixteen, &[0; 8]);
    check("h.png", "GrayscaleAlpha", "16");
    write_png(&h, (2, 1), Indexed, Eight, &[0, 0]);
    check("h.png", "Indexed", "8");

    // RGBA is accepted only with every alpha 255.
    good_height(&h);
    write_png(
        &t,
        (2, 1),
        png::ColorType::Rgba,
        Eight,
        &[255, 0, 0, 255, 255, 0, 0, 255],
    );
    let layers = decode_layers(&terrain, dir.path(), (2, 1)).unwrap();
    assert_eq!(layers.types, [0, 0]);
    write_png(
        &t,
        (2, 1),
        png::ColorType::Rgba,
        Eight,
        &[255, 0, 0, 255, 255, 0, 0, 128],
    );
    let err = decode_layers(&terrain, dir.path(), (2, 1)).unwrap_err();
    assert!(err.contains("t.png") && err.contains("alpha 128"), "{err}");
}

/// A zlib stream holding `data` in one stored (uncompressed) deflate block.
fn zlib_stored(data: &[u8]) -> Vec<u8> {
    let len = u16::try_from(data.len()).unwrap();
    let mut out = vec![0x78, 0x01, 0x01];
    out.extend(len.to_le_bytes());
    out.extend((!len).to_le_bytes());
    out.extend(data);
    let (mut a, mut b) = (1u32, 0u32);
    for &byte in data {
        a = (a + u32::from(byte)) % 65521;
        b = (b + a) % 65521;
    }
    out.extend(((b << 16) | a).to_be_bytes());
    out
}

#[test]
fn ancillary_colour_chunks_are_ignored() {
    let olive: Rgb = (128, 128, 0);
    let terrain = terrain_with(&[(olive, GRASS), ((10, 20, 30), WATER)]);
    let pixel = |px: u32, _| if px == 0 { olive } else { (10, 20, 30) };

    let plain = temp();
    write_rgb(&plain.path().join("t.png"), (2, 1), pixel);
    write_gray16(&plain.path().join("h.png"), (2, 1), 7);
    let expected = decode_layers(&terrain, plain.path(), (2, 1)).unwrap();

    let tagged = temp();
    write_gray16(&tagged.path().join("h.png"), (2, 1), 7);
    let file = BufWriter::new(File::create(tagged.path().join("t.png")).unwrap());
    let mut encoder = png::Encoder::new(file, 2, 1);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    // Gamma 1/2.2 (scaled by 100000), sRGB perceptual, and an ICC profile: a decoder that
    // applied any of them would change the samples.
    let mut iccp = b"fake profile\0\0".to_vec();
    iccp.extend(zlib_stored(&[7; 64]));
    for (name, data) in [
        (*b"gAMA", 45455u32.to_be_bytes().to_vec()),
        (*b"sRGB", vec![0]),
        (*b"iCCP", iccp),
    ] {
        writer
            .write_chunk(png::chunk::ChunkType(name), &data)
            .unwrap();
    }
    writer.write_image_data(&[128, 128, 0, 10, 20, 30]).unwrap();
    writer.finish().unwrap();

    assert_eq!(
        decode_layers(&terrain, tagged.path(), (2, 1)).unwrap(),
        expected
    );
    assert_eq!(expected.types, [0, 1]);
}

#[test]
fn duplicate_legend_colour_is_error() {
    let dir = temp();
    let terrain = terrain_with(&[(RED, GRASS), (RED, WATER)]);
    let layers = layers((4, 3), &[GRASS, WATER], |_, _| 0);
    write_mod(
        dir.path(),
        &grass_water(),
        &map_with_terrain((4, 3), &terrain),
        Some((&terrain, &layers)),
    );
    let errs = load_err(dir.path());
    assert_error_at(&errs, "t.map/terrain.legend[1]", "#ff0000");
    // Only the link pass reports it: the sidecar does not decode an ambiguous legend.
    assert_eq!(errs.0.len(), 1, "{errs}");
}

// ---------------------------------------------------------------- caps and rules links

#[test]
fn too_many_terrain_kinds_is_error() {
    let dir = temp();
    let terrains: Vec<String> = (0..256)
        .map(|i| terrain_ron(&format!("t.k{i:03}"), "1.0", ""))
        .collect();
    write_mod(dir.path(), &terrains, &map_ron((1, 1), ""), None);
    let errs = load_err(dir.path());
    assert_error(&errs, &["k255.terrain.ron", "256 terrain kinds", "255"]);

    let ok = temp();
    write_mod(ok.path(), &terrains[..255], &map_ron((1, 1), ""), None);
    assert_eq!(load_ok(ok.path()).kind::<TerrainDef>().len(), 255);
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TestLimits {
    max_units: u32,
}

impl RulesSection for TestLimits {
    fn link(&self, ctx: &mut LinkCtx<'_>) {
        if self.max_units == 0 {
            ctx.error("max_units", "must be at least 1");
        }
    }
}

#[test]
fn rules_section_link_errors_are_reported() {
    let load_rules = |text: &str| {
        let dir = temp();
        write_mod(dir.path(), &grass_water(), &map_ron((1, 1), ""), None);
        fs::write(dir.path().join("rules.ron"), text).unwrap();
        let mut reg = DefRegistry::with_builtins();
        reg.register_rules_section::<TestLimits>("Limits");
        load_dirs(reg, &[dir.path().to_path_buf()])
    };
    let errs = match load_rules("Rules({ \"Limits\": (max_units: 0) })") {
        Ok(_) => panic!("max_units 0 should be rejected"),
        Err(errs) => errs,
    };
    assert_error_at(&errs, "Limits/max_units", "at least 1");
    assert!(errs.0[0].file.ends_with("rules.ron"), "{errs}");

    let db = load_rules("Rules({ \"Limits\": (max_units: 4) })").unwrap();
    assert_eq!(db.rules::<TestLimits>().max_units, 4);
}
