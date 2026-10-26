//! Built-in content kinds: factions, maps, scenarios and visuals. Pure data, no Bevy.

use std::path::Path;

use fl_math::{Fx, FxVec2};
use fl_protocol::Controller;
use serde::{Deserialize, Serialize};

use crate::kinds::ContentKind;
use crate::layers::{MAX_MAP_CELLS, MapLayers, MapTerrain, check_png_name, decode};
use crate::link::LinkCtx;
use crate::terrain::TerrainDef;

/// The largest map side, in metres.
const MAX_MAP_SIDE_M: i32 = 4096;

/// Report an error unless `unit` names a concrete unit.
fn check_concrete_unit(ctx: &mut LinkCtx<'_>, path: &str, unit: &str) {
    if !ctx.unit_exists(unit) {
        ctx.error(path, format!("unit '{unit}' does not exist"));
    } else if ctx.unit_is_abstract(unit) {
        ctx.error(
            path,
            format!("unit '{unit}' is abstract and cannot be used here"),
        );
    }
}

/// A playable side: `*.faction.ron`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Faction {
    pub id: String,
    pub name: String,
    /// Linear RGB, 0–1.
    pub color: (f32, f32, f32),
    /// Concrete unit ids this faction can field.
    pub roster: Vec<String>,
}

impl ContentKind for Faction {
    const EXT: &'static str = "faction";
    const IN_RULES_HASH: bool = true;

    fn id(&self) -> &str {
        &self.id
    }

    fn link(&self, ctx: &mut LinkCtx<'_>) {
        for (i, unit) in self.roster.iter().enumerate() {
            check_concrete_unit(ctx, &format!("roster[{i}]"), unit);
        }
    }
}

/// A map: `*.map.ron`. The map spans `[0, size_m)` on both axes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MapDef {
    pub id: String,
    pub name: String,
    /// Width (x, east) and depth (y, north) in metres.
    pub size_m: FxVec2,
    /// Grid cell side in metres; both sides must be multiples of it, and each side holds at
    /// most 4096 cells.
    pub cell_m: Fx,
    /// Linear RGB, 0–1.
    pub ground_color: (f32, f32, f32),
    /// Height and terrain-type layers; `None` is open ground (pass 1, blocks nothing,
    /// height 0).
    #[serde(default)]
    pub terrain: Option<MapTerrain>,
    /// `terrain`'s PNGs, decoded by the loader ([`ContentKind::load_sidecars`]). RON cannot
    /// set it; it is serialized, so the rules hash covers every pixel.
    ///
    /// Serialization is asymmetric: `layers` is written but never read back, so a `MapDef`
    /// round-trips only through a self-describing format (RON). Never store one in postcard
    /// (a save, a snapshot) and decode it again; store the map id and look it up.
    #[serde(skip_deserializing, default)]
    pub layers: Option<MapLayers>,
}

impl MapDef {
    /// The grid size in cells, `size_m / cell_m`, or every reason the size or the cell
    /// side is invalid, as `(field, message)`. The one check behind both the link pass and
    /// the layer decode.
    fn grid(&self) -> Result<(u32, u32), Vec<(&'static str, String)>> {
        let mut errors = Vec::new();
        let max = Fx::from_int(MAX_MAP_SIDE_M);
        let sides = [("x", self.size_m.x), ("y", self.size_m.y)];
        for (axis, side) in sides {
            if side <= Fx::ZERO || side > max {
                errors.push((
                    "size_m",
                    format!("{axis} side {side} m must be in (0, {MAX_MAP_SIDE_M}] m"),
                ));
            }
        }
        let cell = self.cell_m.raw();
        if cell <= 0 {
            errors.push(("cell_m", format!("{} m must be positive", self.cell_m)));
            return Err(errors);
        }
        let mut cells = [0; 2];
        for (count, (axis, side)) in cells.iter_mut().zip(sides) {
            let n = side.raw() / cell;
            if n > i64::from(MAX_MAP_CELLS) {
                errors.push((
                    "cell_m",
                    format!(
                        "{axis} side {side} m is {n} cells of {} m; at most {MAX_MAP_CELLS} cells per side",
                        self.cell_m
                    ),
                ));
            }
            if side.raw() % cell != 0 {
                errors.push((
                    "size_m",
                    format!(
                        "{axis} side {side} m is not a multiple of cell_m {} m",
                        self.cell_m
                    ),
                ));
            }
            *count = u32::try_from(n).unwrap_or(0);
        }
        if errors.is_empty() {
            Ok((cells[0], cells[1]))
        } else {
            Err(errors)
        }
    }
}

impl ContentKind for MapDef {
    const EXT: &'static str = "map";
    const IN_RULES_HASH: bool = true;

    fn id(&self) -> &str {
        &self.id
    }

    /// Decodes `terrain`'s PNGs, next to the map file, into `layers`. A size, cell side,
    /// file name or legend the link pass rejects is left to it.
    fn load_sidecars(&mut self, file: &Path) -> Result<(), (String, String)> {
        let Some(terrain) = &self.terrain else {
            return Ok(());
        };
        let Ok(cells) = self.grid() else {
            return Ok(());
        };
        let names_ok = terrain
            .png_names()
            .iter()
            .all(|(_, name)| check_png_name(name).is_ok());
        if !names_ok || terrain.duplicate_colour().is_some() {
            return Ok(());
        }
        let dir = file.parent().unwrap_or(Path::new(""));
        let layers = decode(terrain, dir, cells).map_err(|(field, msg)| {
            let field = if field.is_empty() {
                "terrain".to_owned()
            } else {
                format!("terrain.{field}")
            };
            (field, msg)
        })?;
        self.layers = Some(layers);
        Ok(())
    }

    fn link(&self, ctx: &mut LinkCtx<'_>) {
        if let Some(terrain) = &self.terrain {
            for (field, name) in terrain.png_names() {
                if let Err(msg) = check_png_name(name) {
                    ctx.error(format!("terrain.{field}"), msg);
                }
            }
            for (i, ((r, g, b), id)) in terrain.legend.iter().enumerate() {
                let path = format!("terrain.legend[{i}]");
                let first = terrain.legend[..i]
                    .iter()
                    .position(|(c, _)| *c == (*r, *g, *b));
                if let Some(first) = first {
                    ctx.error(
                        &path,
                        format!("colour #{r:02x}{g:02x}{b:02x} is already terrain.legend[{first}]"),
                    );
                }
                if ctx.kind_get::<TerrainDef>(id).is_none() {
                    ctx.error(path, format!("terrain '{id}' does not exist"));
                }
            }
        }
        if let Err(errors) = self.grid() {
            for (field, msg) in errors {
                ctx.error(field, msg);
            }
        }
    }
}

/// A startable setup: map, players, seed and starting units. `*.scenario.ron`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioDef {
    pub id: String,
    /// A [`MapDef`] id.
    pub map: String,
    pub seed: u64,
    /// Player slots; a unit's `player` indexes this list.
    pub players: Vec<ScenarioPlayer>,
    pub units: Vec<ScenarioUnit>,
}

/// One player slot of a scenario.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioPlayer {
    /// A [`Faction`] id.
    pub faction: String,
    pub team: u8,
    pub controller: Controller,
}

/// One starting unit of a scenario.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioUnit {
    /// A concrete unit id.
    pub unit: String,
    /// Index into [`ScenarioDef::players`].
    pub player: u8,
    /// Metres; must lie inside the map.
    pub pos: FxVec2,
    /// Degrees counter-clockwise from +x (east).
    pub heading_deg: Fx,
}

impl ContentKind for ScenarioDef {
    const EXT: &'static str = "scenario";
    const IN_RULES_HASH: bool = true;

    fn id(&self) -> &str {
        &self.id
    }

    fn link(&self, ctx: &mut LinkCtx<'_>) {
        let map_size = match ctx.kind_get::<MapDef>(&self.map) {
            Some(map) => Some(map.size_m),
            None => {
                ctx.error("map", format!("map '{}' does not exist", self.map));
                None
            }
        };
        for (i, player) in self.players.iter().enumerate() {
            if ctx.kind_get::<Faction>(&player.faction).is_none() {
                ctx.error(
                    format!("players[{i}].faction"),
                    format!("faction '{}' does not exist", player.faction),
                );
            }
        }
        for (i, unit) in self.units.iter().enumerate() {
            check_concrete_unit(ctx, &format!("units[{i}].unit"), &unit.unit);
            if usize::from(unit.player) >= self.players.len() {
                ctx.error(
                    format!("units[{i}].player"),
                    format!(
                        "player {} does not exist (player slots: {})",
                        unit.player,
                        self.players.len()
                    ),
                );
            }
            if let Some(size) = map_size {
                let inside = |v: Fx, side: Fx| Fx::ZERO <= v && v < side;
                if !(inside(unit.pos.x, size.x) && inside(unit.pos.y, size.y)) {
                    ctx.error(
                        format!("units[{i}].pos"),
                        format!(
                            "({}, {}) is outside map '{}' (size {} x {} m)",
                            unit.pos.x, unit.pos.y, self.map, size.x, size.y
                        ),
                    );
                }
            }
        }
    }
}

/// How a unit looks: `*.visual.ron`, keyed by unit id. Never read by the sim.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VisualDef {
    /// The unit this visual belongs to; also the item's id.
    pub unit: String,
    /// Drawn when there is no model, or until it loads.
    pub placeholder: Placeholder,
    /// glTF path; `None` uses the placeholder.
    #[serde(default)]
    pub model: Option<String>,
    pub scale: f32,
    /// Tint with the owning player's faction color.
    pub recolor: bool,
}

impl ContentKind for VisualDef {
    const EXT: &'static str = "visual";
    const IN_RULES_HASH: bool = false;
    const NAMESPACED: bool = false;
    const ID_FIELD: &'static str = "unit";

    fn id(&self) -> &str {
        &self.unit
    }

    fn link(&self, ctx: &mut LinkCtx<'_>) {
        if !ctx.unit_exists(&self.unit) {
            ctx.error("unit", format!("unit '{}' does not exist", self.unit));
        }
    }
}

/// A primitive stand-in mesh. Sizes are `(width, height, length)` in metres, with the
/// length along the unit's local +X.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Placeholder {
    Box {
        size: (f32, f32, f32),
    },
    Tank {
        hull: (f32, f32, f32),
        turret: (f32, f32, f32),
        barrel_len: f32,
    },
    Infantry {
        radius: f32,
        height: f32,
    },
    Truck {
        cab: (f32, f32, f32),
        bed: (f32, f32, f32),
    },
}
