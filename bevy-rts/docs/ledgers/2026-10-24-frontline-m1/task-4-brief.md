### Task 4: Base terrain content, `fl_cli map-gen`, testbed and scale map layers

**Track:** content + tools · **Lane 2** (after Task 3 is merged)

**Files:**
- Create:
  - `assets/base/terrain/{grass,road,field,forest,deep_forest,swamp,water,bridge}.terrain.ron`
  - `assets/base/maps/testbed_1km.{height,terrain}.png`, `assets/base/maps/scale_2km.{height,terrain}.png`
  - `crates/fl_cli/src/map_gen.rs`, `crates/fl_cli/tests/map_gen.rs`
- Modify: `assets/base/maps/{testbed_1km,scale_2km}.map.ron` (add `terrain`), `crates/fl_cli/src/{main,lib}.rs`
- Blessed: the golden replay (the testbed map changes: re-record).

**Interfaces:**
- Consumes: `TerrainDef`, `MapTerrain`, `MapLayers`, `write_layers` (Task 3); `fl_math::Pcg32` (tools may use it).
- Produces:
```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)] pub enum MapPreset { Testbed, Scale }
pub fn generate(preset: MapPreset) -> (MapTerrain, MapLayers);   // pure and deterministic
// `fl_cli map-gen <testbed|scale> --out <dir> [--json]` writes both PNGs; --json prints {"files": [...], "cells": [x, y]}
```
- **Terrain values** (documented guesses from research §1.8; each file carries a `// guess` comment):

  | id | pass | blocks | colour |
  |---|---|---|---|
  | `base.grass` | 0.8 | — | (0.33, 0.45, 0.20) |
  | `base.road` | 1.0 | — | (0.45, 0.40, 0.32) |
  | `base.field` | 0.5 | — | (0.42, 0.33, 0.20) |
  | `base.forest` | 0.7 | — | (0.15, 0.32, 0.12) |
  | `base.deep_forest` | 0.6 | Wheeled, HalfTrack, Tracked | (0.08, 0.22, 0.08) |
  | `base.swamp` | 0.0 | all four | (0.30, 0.35, 0.25) |
  | `base.water` | 0.0 | all four | (0.15, 0.30, 0.55) |
  | `base.bridge` | 1.0 | — | (0.50, 0.42, 0.30) |

- **Testbed layout** (250 × 250 cells of 4 m, height range (−4, 16) m). All rectangles are inclusive cell ranges `x0..=x1, y0..=y1`. Ground is grass at height 0. Terrain is painted in this order, a later layer overwriting an earlier one: forest, deep forest, field, swamp, roads, river, bridges. So the N–S roads stop at the river and the bridge cells carry them across, as `testbed_layout_matches_plan` expects at (71, 124). Heights come from the hill, the ridge and the river (−2 m), independently of the terrain types, except that bridge cells stay at 0 m.

  | Feature | Cells |
  |---|---|
  | River (water, height −2 m) | full width, y 122..=127 |
  | Bridges (bridge, height 0) | x 70..=72 and x 180..=182, over the river rows |
  | N–S roads | x 70..=72 and x 180..=182, full height |
  | E–W roads | y 60..=62 and y 188..=190, full width |
  | Forest | 20..=50, 140..=170 and 200..=230, 150..=175 |
  | Deep forest | 100..=130, 30..=55 |
  | Field | 130..=160, 200..=230 |
  | Swamp | 210..=235, 60..=80 |
  | Hill | centre (60, 200), radius 25 cells, 12 m peak with a cosine falloff in integer math |
  | Ridge | x 140..=143, y 140..=170, a 6 m step (steep for wheeled vehicles) |

  Every testbed scenario unit stays on grass.
- **Scale layout** (500 × 500 cells, 2 km):
  - Two E–W rivers 4 cells wide, at y 160..=163 and y 330..=333.
  - Bridges 3 cells wide every 62 cells along x, starting at x 30.
  - Forest blocks of 8 × 8 cells at 120 positions drawn by `Pcg32::seed(1)`, skipping river rows.
  - Height 0 everywhere.

- [ ] **Step 1: Write the failing tests** (`crates/fl_cli/tests/map_gen.rs`):
```rust
#[test] fn testbed_layers_are_generated()            // generate(Testbed) into a temp dir == the committed PNG bytes, both files
#[test] fn scale_layers_are_generated()
#[test] fn testbed_layout_matches_plan()             // types at (71,124) bridge, (10,124) water, (25,150) forest, (110,40) deep_forest, (0,0) grass
#[test] fn testbed_scenario_units_on_passable_terrain()   // every base.testbed unit's cell: terrain doesn't block its MobilityDef class
#[test] fn base_content_loads_terrain()              // load_content(catalog, base): 8 terrain kinds; both maps have layers; data-check exit 0
#[test] fn map_gen_cli_json()                        // `fl_cli map-gen testbed --out <tmp> --json` → exit 0, "files" has 2 entries, "cells" == [250, 250]
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_cli --test map_gen`. Expected: FAIL.
- [ ] **Step 3: Implement** `generate` and the subcommand. Then run `cargo run -p fl_cli -- map-gen testbed --out assets/base/maps` and the same for `scale`, and add `terrain: Some(MapTerrain(..))` to both map files (legend: the colours of the table, the ids above).
- [ ] **Step 4: Run** `cargo nextest run --profile agent -p fl_cli -p fl_catalog` and `cargo run -p fl_cli -- data-check assets/base`. Expected: PASS; data-check exit 0, `10 units`.
- [ ] **Step 5: Re-record the golden replay** (`FL_RECORD_GOLDEN=1`). The lead blesses it: `M1 Task 4: testbed map gains terrain layers (rules hash)`.
- [ ] **Step 6: Gate** `cargo xtask gate task --track content --track tools`. Expected: PASS.
- [ ] **Step 7: Commit** `feat(content): base terrain kinds, generated testbed and scale map layers, fl_cli map-gen` with trailer `Blessed: …`.

