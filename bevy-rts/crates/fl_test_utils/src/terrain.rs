//! ASCII terrain fixtures: a small test mod whose one map, `"t.ascii"`, is drawn as rows of
//! characters (ADR-0014). The mod is written to a temporary directory, loaded with the game's
//! features, and the directory removed: the content keeps the decoded layers.
//!
//! Legend, one character per cell, row 0 the north edge:
//!
//! | char        | terrain           | pass | blocks                     |
//! |-------------|-------------------|------|----------------------------|
//! | `.`         | `t.grass`         | 0.8  | nothing                    |
//! | `r`         | `t.road`          | 1.0  | nothing                    |
//! | `f`         | `t.forest`        | 0.7  | nothing                    |
//! | `F`         | `t.deep_forest`   | 0.5  | Wheeled, HalfTrack, Tracked|
//! | `~`         | `t.water`         | 0.0  | every class                |
//! | `=`         | `t.bridge`        | 1.0  | nothing                    |
//! | `s`         | `t.swamp`         | 0.0  | every class                |
//! | `1`..=`9`   | `t.grass`, n m up | 0.8  | nothing                    |
//!
//! Every other cell is at height 0. Units: `"t.tank"` (Tracked, 36 km/h, terrain factor 0.5,
//! 3.0 m wide), `"t.truck"` (Wheeled, 72 km/h, 0.2, 2.4 m), `"t.rifleman"` (Foot, 7.2 km/h,
//! 1.0, 0.6 m) and `"t.wide_tank"` (Tracked, 36 km/h, 0.5, 7.0 m); factions `"t.red"` and
//! `"t.blue"`.

use std::fs;
use std::path::Path;
use std::sync::Arc;

use bevy_ecs::world::World;
use fl_content::{ContentDb, MapLayers, MapTerrain, write_layers};
use fl_math::Fx;
use fl_protocol::{PlayerId, SimId};
use fl_sim::nav::{MoveProfile, NavGrid, profile_of};
use fl_sim::{Footprint, Mobility, Owner, Pose, SimEntity};

/// `(char, terrain id, legend colour)`; digits are grass at a height.
const KINDS: [(char, &str, (u8, u8, u8)); 7] = [
    ('.', "t.grass", (80, 160, 60)),
    ('r', "t.road", (120, 110, 90)),
    ('f', "t.forest", (40, 100, 40)),
    ('F', "t.deep_forest", (20, 60, 20)),
    ('~', "t.water", (40, 80, 160)),
    ('=', "t.bridge", (130, 110, 80)),
    ('s', "t.swamp", (80, 90, 60)),
];

/// `(id, pass, blocks)` of the terrain kinds.
const TERRAIN: [(&str, &str, &str); 7] = [
    ("t.grass", "0.8", ""),
    ("t.road", "1.0", ""),
    ("t.forest", "0.7", ""),
    ("t.deep_forest", "0.5", "Wheeled, HalfTrack, Tracked"),
    ("t.water", "0.0", "Foot, Wheeled, HalfTrack, Tracked"),
    ("t.bridge", "1.0", ""),
    ("t.swamp", "0.0", "Foot, Wheeled, HalfTrack, Tracked"),
];

/// `(id, class, road km/h, terrain factor, width, length, turn °/s)` of the units.
const UNITS: [(&str, &str, &str, &str, &str, &str, &str); 4] = [
    ("t.tank", "Tracked", "36.0", "0.5", "3.0", "6.0", "90.0"),
    ("t.truck", "Wheeled", "72.0", "0.2", "2.4", "6.0", "90.0"),
    ("t.rifleman", "Foot", "7.2", "1.0", "0.6", "0.6", "180.0"),
    (
        "t.wide_tank",
        "Tracked",
        "36.0",
        "0.5",
        "7.0",
        "8.0",
        "90.0",
    ),
];

/// The content of the ASCII mod for `rows` with `cell_m`-metre cells and the base rules
/// (`assets/base/rules.ron`), loaded with [`fl_catalog::features`].
///
/// # Panics
/// If the rows are empty or ragged, use a character outside the legend, or the mod does not
/// load.
pub fn ascii_terrain_content(rows: &[&str], cell_m: i32) -> Arc<ContentDb> {
    ascii_terrain_content_with(rows, cell_m, None)
}

/// [`ascii_terrain_content`] with `rules_ron` as the mod's `rules.ron` (`None`: the base
/// rules).
///
/// # Panics
/// As [`ascii_terrain_content`].
pub fn ascii_terrain_content_with(
    rows: &[&str],
    cell_m: i32,
    rules_ron: Option<&str>,
) -> Arc<ContentDb> {
    let dir = tempfile::tempdir().expect("a temporary directory");
    write_ascii_mod(dir.path(), rows, cell_m, rules_ron);
    fl_sim::load_content(&fl_catalog::features(), &[dir.path().to_path_buf()])
        .unwrap_or_else(|err| panic!("the ASCII mod does not load: {err}"))
}

/// Write the ASCII mod for `rows` into `dir` (which must exist): `mod.ron`, `rules.ron`
/// (`rules_ron`, or the base rules), the terrain kinds, the units, the factions and the map
/// `"t.ascii"` with its layer PNGs. Tests that need a broken mod edit the files, then load
/// `dir` themselves.
///
/// # Panics
/// If the rows are empty or ragged, use a character outside the legend, or a file cannot be
/// written.
pub fn write_ascii_mod(dir: &Path, rows: &[&str], cell_m: i32, rules_ron: Option<&str>) {
    let layers = ascii_layers(rows);
    let write = |path: &str, text: &str| {
        let path = dir.join(path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("a mod directory");
        }
        fs::write(&path, text).unwrap_or_else(|err| panic!("writing {}: {err}", path.display()));
    };

    write(
        "mod.ron",
        r#"Mod(id: "t", version: "0.1.0", depends: [], load_after: [])"#,
    );
    let base_rules;
    let rules = match rules_ron {
        Some(rules) => rules,
        None => {
            let path = fl_catalog::base_mod_dir().join("rules.ron");
            base_rules = fs::read_to_string(&path)
                .unwrap_or_else(|err| panic!("reading {}: {err}", path.display()));
            &base_rules
        }
    };
    write("rules.ron", rules);

    for (id, pass, blocks) in TERRAIN {
        let name = id.trim_start_matches("t.");
        write(
            &format!("terrain/{name}.terrain.ron"),
            &format!(
                r#"TerrainDef(id: "{id}", name: "{name}", pass: {pass}, blocks: [{blocks}], color: (0.5, 0.5, 0.5))"#
            ),
        );
    }

    let roster: Vec<String> = UNITS.iter().map(|u| format!("\"{}\"", u.0)).collect();
    let roster = roster.join(", ");
    for (id, name, colour) in [
        ("t.red", "red", "0.8, 0.1, 0.1"),
        ("t.blue", "blue", "0.1, 0.1, 0.8"),
    ] {
        write(
            &format!("factions/{name}.faction.ron"),
            &format!(
                r#"Faction(id: "{id}", name: "{name}", color: ({colour}), roster: [{roster}])"#
            ),
        );
    }

    for (id, class, speed, factor, width, length, turn) in UNITS {
        let name = id.trim_start_matches("t.");
        write(
            &format!("units/{name}.unit.ron"),
            &format!(
                r#"Unit(
    id: "{id}",
    components: {{
        "Selectable": (),
        "Hull": (hp: 100, footprint: ({width}, {length}), hitbox_shrink: 0.5),
        "Mobility": (class: {class}, road_speed_kmh: {speed}, terrain_factor: {factor}, turn_rate_deg_s: {turn}),
    }},
)"#
            ),
        );
    }

    let terrain = MapTerrain {
        height_png: "ascii.height.png".to_owned(),
        terrain_png: "ascii.terrain.png".to_owned(),
        // One raw sample per metre: a digit's height is exact.
        height_m: (Fx::ZERO, Fx::from_int(65535)),
        legend: KINDS
            .iter()
            .map(|&(_, id, colour)| (colour, id.to_owned()))
            .collect(),
    };
    let legend: Vec<String> = KINDS
        .iter()
        .map(|&(_, id, (r, g, b))| format!("(({r}, {g}, {b}), \"{id}\")"))
        .collect();
    let (w, h) = (layers.cells_x, layers.cells_y);
    write(
        "maps/ascii.map.ron",
        &format!(
            r#"MapDef(
    id: "t.ascii",
    name: "ASCII",
    size_m: ({}.0, {}.0),
    cell_m: {cell_m}.0,
    ground_color: (0.3, 0.5, 0.2),
    terrain: Some((
        height_png: "ascii.height.png",
        terrain_png: "ascii.terrain.png",
        height_m: (0.0, 65535.0),
        legend: [{}],
    )),
)"#,
            i64::from(w) * i64::from(cell_m),
            i64::from(h) * i64::from(cell_m),
            legend.join(", ")
        ),
    );
    write_layers(&terrain, &dir.join("maps"), &layers)
        .unwrap_or_else(|err| panic!("writing the ASCII layers: {err}"));
}

/// The layers `rows` draw: palette = every terrain id, sorted; row 0 is the north edge.
///
/// # Panics
/// If the rows are empty or ragged, or use a character outside the legend.
fn ascii_layers(rows: &[&str]) -> MapLayers {
    let h = rows.len();
    let w = rows.first().map_or(0, |row| row.chars().count());
    assert!(w > 0 && h > 0, "an ASCII map needs at least one cell");
    let mut palette: Vec<String> = KINDS.iter().map(|&(_, id, _)| id.to_owned()).collect();
    palette.sort_unstable();
    let index_of = |id: &str| -> u8 {
        let at = palette.iter().position(|p| p == id).expect("a legend id");
        u8::try_from(at).expect("fewer than 256 kinds")
    };
    let mut types = vec![0; w * h];
    let mut heights = vec![0; w * h];
    for (r, row) in rows.iter().enumerate() {
        assert_eq!(
            row.chars().count(),
            w,
            "ASCII row {r} is not {w} cells wide: {row:?}"
        );
        // Row 0 is the north edge: cell y = h − 1 − r.
        let y = h - 1 - r;
        for (x, c) in row.chars().enumerate() {
            let i = y * w + x;
            let (id, height) = match c {
                '1'..='9' => (
                    "t.grass",
                    u16::try_from(c.to_digit(10).expect("a digit")).expect("a digit fits"),
                ),
                _ => {
                    let &(_, id, _) =
                        KINDS.iter().find(|&&(k, _, _)| k == c).unwrap_or_else(|| {
                            panic!("ASCII cell ({x}, row {r}): unknown character {c:?}")
                        });
                    (id, 0)
                }
            };
            types[i] = index_of(id);
            heights[i] = height;
        }
    }
    MapLayers {
        cells_x: u32::try_from(w).expect("a narrow map"),
        cells_y: u32::try_from(h).expect("a short map"),
        palette,
        types,
        heights,
    }
}

/// A unit (an entity with [`Mobility`]) on the navigation grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnitCell {
    pub id: SimId,
    pub owner: Option<PlayerId>,
    /// Its movement profile ([`profile_of`] on the grid's cell size and clearance).
    pub profile: MoveProfile,
    /// The cell its position is in.
    pub cell: (u32, u32),
    /// Whether its profile can stand in that cell ([`NavGrid::passable`]).
    pub passable: bool,
}

/// Every unit of `world` on its [`NavGrid`], in `SimId` order.
///
/// # Panics
/// If `world` has no [`NavGrid`].
pub fn unit_cells(world: &mut World) -> Vec<UnitCell> {
    let mut units = world.query::<(
        &SimEntity,
        Option<&Owner>,
        &Pose,
        &Mobility,
        Option<&Footprint>,
    )>();
    let world: &World = world;
    let nav = world.resource::<NavGrid>();
    let mut cells: Vec<UnitCell> = units
        .iter(world)
        .map(|(sim, owner, pose, mobility, footprint)| {
            let profile = profile_of(mobility, footprint, nav.cell_m(), nav.max_clearance());
            let cell = nav.cell_of(pose.pos);
            UnitCell {
                id: sim.id(),
                owner: owner.map(|o| o.0),
                profile,
                cell,
                passable: nav.passable(profile, i64::from(cell.0), i64::from(cell.1)),
            }
        })
        .collect();
    cells.sort_unstable_by_key(|unit| unit.id);
    cells
}
