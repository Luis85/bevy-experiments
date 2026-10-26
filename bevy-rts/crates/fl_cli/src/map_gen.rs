//! `map-gen`: procedural height and terrain-type layers for the base maps (ADR-0014).
//!
//! [`generate`] is pure and deterministic: integer maths only (the hill's cosine comes from
//! `fl_math`'s table), and `Pcg32` seeded with a constant. The committed PNGs under
//! `assets/base/maps` are its output; `tests/map_gen.rs` checks they still are.

use std::path::Path;

use fl_content::{MapLayers, MapTerrain, write_layers};
use fl_math::{Angle, Fx, Pcg32};
use serde_json::json;

use crate::report::{CliError, Report};

/// Which map to generate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum MapPreset {
    /// `base.testbed_1km`: 250 x 250 cells with every terrain kind.
    Testbed,
    /// `base.scale_2km`: 500 x 500 cells, two rivers, bridges and forest blocks.
    Scale,
}

/// The terrain kinds of the base mod, sorted by id (the palette order), with their legend
/// colours: the `color` of each `*.terrain.ron` as 8-bit values.
const KINDS: [(&str, [u8; 3]); 8] = [
    ("base.bridge", [128, 107, 76]),
    ("base.deep_forest", [20, 56, 20]),
    ("base.field", [107, 84, 51]),
    ("base.forest", [38, 82, 31]),
    ("base.grass", [84, 115, 51]),
    ("base.road", [115, 102, 82]),
    ("base.swamp", [76, 89, 64]),
    ("base.water", [38, 76, 140]),
];

const BRIDGE: u8 = 0;
const DEEP_FOREST: u8 = 1;
const FIELD: u8 = 2;
const FOREST: u8 = 3;
const GRASS: u8 = 4;
const ROAD: u8 = 5;
const SWAMP: u8 = 6;
const WATER: u8 = 7;

/// Height range of the testbed's PNG, in whole metres.
const TESTBED_HEIGHT_M: (i32, i32) = (-4, 16);
/// Height range of the scale map's PNG (it is flat at 0).
const SCALE_HEIGHT_M: (i32, i32) = (0, 16);

/// An inclusive cell rectangle `(x0, x1, y0, y1)`.
type Rect = (u32, u32, u32, u32);

/// The layers under construction: palette indices and heights in millimetres.
struct Canvas {
    cells: (u32, u32),
    types: Vec<u8>,
    height_mm: Vec<i32>,
}

impl Canvas {
    fn new(cells: (u32, u32)) -> Canvas {
        let n = cells.0 as usize * cells.1 as usize;
        Canvas {
            cells,
            types: vec![GRASS; n],
            height_mm: vec![0; n],
        }
    }

    fn index(&self, x: u32, y: u32) -> usize {
        y as usize * self.cells.0 as usize + x as usize
    }

    /// Paint `kind` on `rect`.
    fn paint(&mut self, (x0, x1, y0, y1): Rect, kind: u8) {
        for y in y0..=y1 {
            for x in x0..=x1 {
                let i = self.index(x, y);
                self.types[i] = kind;
            }
        }
    }

    /// Add `mm` to the height on `rect`.
    fn raise(&mut self, (x0, x1, y0, y1): Rect, mm: i32) {
        for y in y0..=y1 {
            for x in x0..=x1 {
                let i = self.index(x, y);
                self.height_mm[i] += mm;
            }
        }
    }

    /// Set the height on `rect`.
    fn set_height(&mut self, (x0, x1, y0, y1): Rect, mm: i32) {
        for y in y0..=y1 {
            for x in x0..=x1 {
                let i = self.index(x, y);
                self.height_mm[i] = mm;
            }
        }
    }

    /// Raise a hill of `peak_mm` with a cosine falloff over `radius` cells around `(cx, cy)`.
    fn hill(&mut self, (cx, cy): (i64, i64), radius: i64, peak_mm: i64) {
        for y in 0..self.cells.1 {
            for x in 0..self.cells.0 {
                let (dx, dy) = (i64::from(x) - cx, i64::from(y) - cy);
                let d2 = dx * dx + dy * dy;
                if d2 >= radius * radius {
                    continue;
                }
                // d / radius as half a turn: 0 at the centre, 32768 at the rim.
                let half_turn = u128::isqrt((d2 as u128) << 30) / radius as u128;
                let angle = Angle::from_raw(u16::try_from(half_turn).unwrap_or(u16::MAX));
                // peak * (1 + cos) / 2, floored on the raw 32.32 value.
                let lift = (Fx::ONE + angle.cos()).raw() * peak_mm / (2 * Fx::ONE.raw());
                let i = self.index(x, y);
                self.height_mm[i] += i32::try_from(lift).expect("a hill lift fits i32");
            }
        }
    }

    fn finish(self, terrain: &MapTerrain, (lo_m, hi_m): (i32, i32)) -> MapLayers {
        let (lo, span) = (i64::from(lo_m) * 1000, i64::from(hi_m - lo_m) * 1000);
        let heights = self
            .height_mm
            .iter()
            .map(|&mm| {
                // Nearest sample; the pixel maximum 65535 is `hi_m`.
                let raw = ((i64::from(mm) - lo) * 65535 * 2 + span) / (2 * span);
                u16::try_from(raw.clamp(0, 65535)).expect("clamped to u16")
            })
            .collect();
        let mut palette: Vec<String> = terrain.legend.iter().map(|(_, id)| id.clone()).collect();
        palette.sort_unstable();
        MapLayers {
            cells_x: self.cells.0,
            cells_y: self.cells.1,
            palette,
            types: self.types,
            heights,
        }
    }
}

/// The map's terrain description: file names, height range and the legend of all kinds.
fn map_terrain(stem: &str, height_m: (i32, i32)) -> MapTerrain {
    MapTerrain {
        height_png: format!("{stem}.height.png"),
        terrain_png: format!("{stem}.terrain.png"),
        height_m: (Fx::from_int(height_m.0), Fx::from_int(height_m.1)),
        legend: KINDS
            .iter()
            .map(|&(id, [r, g, b])| ((r, g, b), id.to_owned()))
            .collect(),
    }
}

fn testbed() -> (MapTerrain, MapLayers) {
    let terrain = map_terrain("testbed_1km", TESTBED_HEIGHT_M);
    let mut c = Canvas::new((250, 250));
    let (w, h) = (249, 249);
    // Later layers overwrite earlier ones.
    c.paint((20, 50, 140, 170), FOREST);
    c.paint((200, 230, 150, 175), FOREST);
    c.paint((100, 130, 30, 55), DEEP_FOREST);
    c.paint((130, 160, 200, 230), FIELD);
    c.paint((210, 235, 60, 80), SWAMP);
    for x in [70, 180] {
        c.paint((x, x + 2, 0, h), ROAD);
    }
    for y in [60, 188] {
        c.paint((0, w, y, y + 2), ROAD);
    }
    // Heights come from the hill, the ridge and the river, whatever the terrain type.
    c.hill((60, 200), 25, 12_000);
    c.raise((140, 143, 140, 170), 6_000);
    c.raise((0, w, 122, 127), -2_000);
    c.paint((0, w, 122, 127), WATER);
    for x in [70, 180] {
        c.paint((x, x + 2, 122, 127), BRIDGE);
        c.set_height((x, x + 2, 122, 127), 0);
    }
    let layers = c.finish(&terrain, TESTBED_HEIGHT_M);
    (terrain, layers)
}

fn scale() -> (MapTerrain, MapLayers) {
    let terrain = map_terrain("scale_2km", SCALE_HEIGHT_M);
    let mut c = Canvas::new((500, 500));
    let rivers = [(160, 163), (330, 333)];
    let mut rng = Pcg32::new(1, 0);
    for _ in 0..120 {
        // Both draws happen before the skip, so the other blocks do not depend on the rivers.
        let x = rng.range_u32(0, 500 - 8 + 1);
        let y = rng.range_u32(0, 500 - 8 + 1);
        if rivers.iter().any(|&(r0, r1)| y <= r1 && y + 7 >= r0) {
            continue;
        }
        c.paint((x, x + 7, y, y + 7), FOREST);
    }
    for (y0, y1) in rivers {
        c.paint((0, 499, y0, y1), WATER);
    }
    for x in (30..500 - 2).step_by(62) {
        for (y0, y1) in rivers {
            c.paint((x, x + 2, y0, y1), BRIDGE);
        }
    }
    let layers = c.finish(&terrain, SCALE_HEIGHT_M);
    (terrain, layers)
}

/// The terrain description and layers of `preset`. Pure and deterministic.
#[must_use]
pub fn generate(preset: MapPreset) -> (MapTerrain, MapLayers) {
    match preset {
        MapPreset::Testbed => testbed(),
        MapPreset::Scale => scale(),
    }
}

/// `map-gen <preset> --out <dir>`: write the preset's two PNGs into `out`.
///
/// # Errors
/// [`CliError::Io`] when `out` is missing or a file cannot be written.
pub fn map_gen(preset: MapPreset, out: &Path) -> Result<Report, CliError> {
    let (terrain, layers) = generate(preset);
    write_layers(&terrain, out, &layers)
        .map_err(|e| CliError::Io(format!("cannot write layers into {}: {e}", out.display())))?;
    let files: Vec<String> = [&terrain.height_png, &terrain.terrain_png]
        .into_iter()
        .map(|name| out.join(name).display().to_string())
        .collect();
    let text = format!(
        "{}x{} cells\n{}\n",
        layers.cells_x,
        layers.cells_y,
        files.join("\n")
    );
    Ok(Report::ok(
        json!({ "files": files, "cells": [layers.cells_x, layers.cells_y] }),
        text,
    ))
}
