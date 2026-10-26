//! Map layers: a map's height and terrain-type PNGs, decoded once at content load into
//! integer [`MapLayers`] (ADR-0014). This is the only place in the workspace that reads or
//! writes a PNG; tools and tests go through [`decode_layers`] and [`write_layers`].
//!
//! There is one pixel per cell, and pixel row 0 is the north edge: pixel `(px, py)` is cell
//! `(px, cells_y − 1 − py)`. Samples are read raw: no palette expansion, no bit-depth
//! transformation, and `gAMA`, `sRGB`, `iCCP`, `cHRM` and every other ancillary chunk are
//! ignored, so legend matching never depends on colour management.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{self, BufReader, BufWriter};
use std::path::{Component, Path};

use fl_math::Fx;
use png::{BitDepth, ColorType};
use serde::{Deserialize, Serialize};

use crate::terrain::MAX_TERRAIN_KINDS;

type Rgb = (u8, u8, u8);

/// The most cells a map grid has per side (ruling R89): it bounds every cell array, so a
/// mistyped `cell_m` is a content error rather than a huge allocation.
pub(crate) const MAX_MAP_CELLS: u32 = 4096;

/// Check that a layer file name is a relative path without `..`, so it resolves inside
/// the map file's directory (ruling R90).
pub(crate) fn check_png_name(name: &str) -> Result<(), String> {
    let path = Path::new(name);
    let inside = path
        .components()
        .all(|c| matches!(c, Component::Normal(_) | Component::CurDir));
    let names_a_file = path.components().any(|c| matches!(c, Component::Normal(_)));
    // `\` and `:` are separators or drive prefixes on Windows only; rejecting them everywhere
    // keeps a mod loading the same on every OS.
    let portable = !name.contains(['\\', ':']);
    if inside && names_a_file && portable {
        Ok(())
    } else {
        Err(format!(
            "'{name}' must be a relative path to a file in or below the map's directory, \
             without '..', '\\' or ':'"
        ))
    }
}

/// A map's terrain layers, as written in its `*.map.ron` (`terrain: Some(…)`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MapTerrain {
    /// Height PNG, a relative path without `..` from the map file's directory: grayscale without alpha, 16-bit, or
    /// 8-bit where a value v counts as v × 257.
    pub height_png: String,
    /// Terrain-type PNG, a relative path without `..` from the map file's directory: 8-bit RGB, or 8-bit RGBA with
    /// every alpha 255.
    pub terrain_png: String,
    /// Height in metres at pixel value 0 and at the pixel maximum (65535).
    pub height_m: (Fx, Fx),
    /// Terrain-PNG colour → `TerrainDef` id. Colours are unique; ids may repeat.
    pub legend: Vec<(Rgb, String)>,
}

impl MapTerrain {
    /// The layer file names with their fields: `[("height_png", …), ("terrain_png", …)]`.
    pub(crate) fn png_names(&self) -> [(&'static str, &str); 2] {
        [
            ("height_png", &self.height_png),
            ("terrain_png", &self.terrain_png),
        ]
    }

    /// The first legend entry whose colour an earlier entry already uses, as
    /// `(index, earlier index)`.
    pub(crate) fn duplicate_colour(&self) -> Option<(usize, usize)> {
        self.legend.iter().enumerate().find_map(|(i, (colour, _))| {
            let first = self.legend[..i].iter().position(|(c, _)| c == colour)?;
            Some((i, first))
        })
    }
}

/// A map's decoded layers. Cell `(x, y)` (y north) is at [`index`](Self::index).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MapLayers {
    pub cells_x: u32,
    pub cells_y: u32,
    /// The legend's terrain ids, sorted and unique.
    pub palette: Vec<String>,
    /// One palette index per cell.
    pub types: Vec<u8>,
    /// One raw height sample per cell, 0..=65535 over [`MapTerrain::height_m`].
    pub heights: Vec<u16>,
}

impl MapLayers {
    /// The position of cell `(x, y)` in [`types`](Self::types) and [`heights`](Self::heights):
    /// `y × cells_x + x`.
    pub fn index(&self, x: u32, y: u32) -> usize {
        debug_assert!(
            x < self.cells_x && y < self.cells_y,
            "cell ({x}, {y}) outside"
        );
        y as usize * self.cells_x as usize + x as usize
    }
}

/// `#rrggbb`.
fn hex((r, g, b): Rgb) -> String {
    format!("#{r:02x}{g:02x}{b:02x}")
}

/// One decoded PNG: raw samples, rows north first, `line_size` bytes per row.
struct Image {
    data: Vec<u8>,
    line_size: usize,
    color: ColorType,
    depth: BitDepth,
}

/// Read the PNG at `path`, which must be `cells` pixels in a format `accepts` takes.
/// `expected` names the accepted formats for the error message.
fn read_png(
    path: &Path,
    cells: (u32, u32),
    expected: &str,
    accepts: fn(ColorType, BitDepth) -> bool,
) -> Result<Image, String> {
    let file = File::open(path).map_err(|e| format!("{}: cannot open: {e}", path.display()))?;
    let mut decoder = png::Decoder::new(BufReader::new(file));
    decoder.set_ignore_iccp_chunk(true);
    decoder.set_ignore_text_chunk(true);
    // Samples stay raw: no palette expansion, bit-depth change or gamma (ADR-0014).
    decoder.set_transformations(png::Transformations::IDENTITY);
    let invalid = |e: png::DecodingError| format!("{}: not a valid PNG: {e}", path.display());
    let mut reader = decoder.read_info().map_err(invalid)?;
    let info = reader.info();
    let (color, depth) = (info.color_type, info.bit_depth);
    if !accepts(color, depth) {
        return Err(format!(
            "{}: unsupported PNG format: colour type {color:?}, bit depth {} (expected {expected})",
            path.display(),
            depth as u8
        ));
    }
    if (info.width, info.height) != cells {
        return Err(format!(
            "{}: expected {}x{} pixels, found {}x{}",
            path.display(),
            cells.0,
            cells.1,
            info.width,
            info.height
        ));
    }
    let size = reader
        .output_buffer_size()
        .ok_or_else(|| format!("{}: image too large", path.display()))?;
    let mut data = vec![0; size];
    let frame = reader.next_frame(&mut data).map_err(invalid)?;
    Ok(Image {
        data,
        line_size: frame.line_size,
        color,
        depth,
    })
}

/// Decode `terrain`'s PNGs in `dir` into layers of `cells` (width, depth).
///
/// Errors start with the [`MapTerrain`] field they concern (`terrain_png: …`,
/// `legend[1]: …`), then name the file, and the pixel where there is one.
pub fn decode_layers(
    terrain: &MapTerrain,
    dir: &Path,
    cells: (u32, u32),
) -> Result<MapLayers, String> {
    decode(terrain, dir, cells).map_err(|(field, msg)| {
        if field.is_empty() {
            msg
        } else {
            format!("{field}: {msg}")
        }
    })
}

/// A decode error: the [`MapTerrain`] field it concerns (empty for none) and the message.
pub(crate) type FieldError = (String, String);

/// [`decode_layers`], with each error's field kept apart for the loader.
pub(crate) fn decode(
    terrain: &MapTerrain,
    dir: &Path,
    cells: (u32, u32),
) -> Result<MapLayers, FieldError> {
    for (field, name) in terrain.png_names() {
        check_png_name(name).map_err(|msg| (field.to_owned(), msg))?;
    }
    let (cells_x, cells_y) = cells;
    if !(1..=MAX_MAP_CELLS).contains(&cells_x) || !(1..=MAX_MAP_CELLS).contains(&cells_y) {
        return Err((
            String::new(),
            format!("a {cells_x}x{cells_y} grid is not 1 to {MAX_MAP_CELLS} cells per side"),
        ));
    }
    if let Some((i, first)) = terrain.duplicate_colour() {
        return Err((
            format!("legend[{i}]"),
            format!(
                "colour {} is already legend[{first}]",
                hex(terrain.legend[i].0)
            ),
        ));
    }
    let mut palette: Vec<String> = terrain.legend.iter().map(|(_, id)| id.clone()).collect();
    palette.sort_unstable();
    palette.dedup();
    if palette.len() > MAX_TERRAIN_KINDS {
        return Err((
            "legend".to_owned(),
            format!(
                "the legend names {} terrain kinds; at most {MAX_TERRAIN_KINDS} fit a cell",
                palette.len()
            ),
        ));
    }
    let lookup: BTreeMap<Rgb, u8> = terrain
        .legend
        .iter()
        .map(|(colour, id)| {
            let index = palette.binary_search(id).expect("palette holds every id");
            (
                *colour,
                u8::try_from(index).expect("palette is capped below 256"),
            )
        })
        .collect();
    let n = cells_x as usize * cells_y as usize;
    // Pixel (px, py) is cell (px, cells_y - 1 - py).
    let cell = |px: u32, py: u32| (cells_y - 1 - py) as usize * cells_x as usize + px as usize;

    // Each cell array is allocated only after its PNG's header matched the grid size.
    let field = |name: &'static str| move |msg: String| (name.to_owned(), msg);
    let path = dir.join(&terrain.terrain_png);
    let image = read_png(
        &path,
        cells,
        "8-bit RGB, or 8-bit RGBA with alpha 255",
        |c, d| matches!(c, ColorType::Rgb | ColorType::Rgba) && d == BitDepth::Eight,
    )
    .map_err(field("terrain_png"))?;
    let mut types = vec![0; n];
    let channels = image.color.samples();
    for py in 0..cells_y {
        let row = &image.data[py as usize * image.line_size..];
        for px in 0..cells_x {
            let s = &row[px as usize * channels..][..channels];
            if channels == 4 && s[3] != u8::MAX {
                return Err(field("terrain_png")(format!(
                    "{}: pixel ({px}, {py}) alpha {} is not 255",
                    path.display(),
                    s[3]
                )));
            }
            let colour = (s[0], s[1], s[2]);
            let Some(&t) = lookup.get(&colour) else {
                return Err(field("terrain_png")(format!(
                    "{}: pixel ({px}, {py}) colour {} is not in the legend",
                    path.display(),
                    hex(colour)
                )));
            };
            types[cell(px, py)] = t;
        }
    }

    let path = dir.join(&terrain.height_png);
    let image = read_png(&path, cells, "8-bit or 16-bit grayscale", |c, d| {
        c == ColorType::Grayscale && matches!(d, BitDepth::Eight | BitDepth::Sixteen)
    })
    .map_err(field("height_png"))?;
    let mut heights = vec![0; n];
    for py in 0..cells_y {
        let row = &image.data[py as usize * image.line_size..];
        for px in 0..cells_x {
            let x = px as usize;
            heights[cell(px, py)] = match image.depth {
                BitDepth::Sixteen => u16::from_be_bytes([row[2 * x], row[2 * x + 1]]),
                _ => u16::from(row[x]) * 257,
            };
        }
    }
    Ok(MapLayers {
        cells_x,
        cells_y,
        palette,
        types,
        heights,
    })
}

/// The inverse of [`decode_layers`]: write `layers` into `dir` as `terrain`'s PNGs, the
/// heights as 16-bit grayscale and the types as 8-bit RGB in each palette id's first legend
/// colour. A layer whose palette is not the legend's ids, sorted and unique, decodes to a
/// different palette.
pub fn write_layers(terrain: &MapTerrain, dir: &Path, layers: &MapLayers) -> io::Result<()> {
    let invalid = |msg: String| io::Error::new(io::ErrorKind::InvalidInput, msg);
    for (field, name) in terrain.png_names() {
        check_png_name(name).map_err(|msg| invalid(format!("{field}: {msg}")))?;
    }
    let (cells_x, cells_y) = (layers.cells_x, layers.cells_y);
    let n = cells_x as usize * cells_y as usize;
    if layers.types.len() != n || layers.heights.len() != n {
        return Err(invalid(format!(
            "{cells_x}x{cells_y} layers need {n} types and heights, found {} and {}",
            layers.types.len(),
            layers.heights.len()
        )));
    }
    let colours = layers
        .palette
        .iter()
        .map(|id| {
            terrain
                .legend
                .iter()
                .find(|(_, legend_id)| legend_id == id)
                .map(|(colour, _)| *colour)
                .ok_or_else(|| invalid(format!("palette id '{id}' has no legend colour")))
        })
        .collect::<io::Result<Vec<Rgb>>>()?;

    let mut rgb = Vec::with_capacity(3 * n);
    let mut gray = Vec::with_capacity(2 * n);
    for py in 0..cells_y {
        let y = cells_y - 1 - py;
        for x in 0..cells_x {
            let i = layers.index(x, y);
            let t = layers.types[i];
            let (r, g, b) = *colours.get(usize::from(t)).ok_or_else(|| {
                invalid(format!(
                    "cell ({x}, {y}) type {t} is outside the palette of {}",
                    colours.len()
                ))
            })?;
            rgb.extend([r, g, b]);
            gray.extend(layers.heights[i].to_be_bytes());
        }
    }
    let size = (cells_x, cells_y);
    write_png(
        &dir.join(&terrain.terrain_png),
        size,
        ColorType::Rgb,
        BitDepth::Eight,
        &rgb,
    )?;
    write_png(
        &dir.join(&terrain.height_png),
        size,
        ColorType::Grayscale,
        BitDepth::Sixteen,
        &gray,
    )
}

fn write_png(
    path: &Path,
    size: (u32, u32),
    color: ColorType,
    depth: BitDepth,
    data: &[u8],
) -> io::Result<()> {
    let file = BufWriter::new(File::create(path)?);
    let mut encoder = png::Encoder::new(file, size.0, size.1);
    encoder.set_color(color);
    encoder.set_depth(depth);
    let mut writer = encoder.write_header().map_err(io::Error::other)?;
    writer.write_image_data(data).map_err(io::Error::other)?;
    writer.finish().map_err(io::Error::other)
}
