### Task 3: Terrain content format — `MoveClass`, `TerrainDef`, PNG map layers, sidecar hook

**Track:** contract + content + sim-core · **Refs:** ADR-0014 · **Lane 1**

**Files:**
- Modify:
  - `Cargo.toml` (`png = "0.18"` in `[workspace.dependencies]`), `Cargo.lock` (R50)
  - `crates/fl_content/Cargo.toml` (`png.workspace = true`; dev-dependency `tempfile.workspace = true`)
  - `crates/fl_content/src/{builtin,kinds,mods,registry,lib}.rs`
  - `crates/fl_sim/src/{units,lib}.rs` (re-export `MoveClass`; its `SimHash` impl stays in `fl_sim`)
  - `xtask/src/arch.rs`: a direct `png` dependency is allowed only in `fl_content` (the `Content` role), with a unit test (ADR-0014 Confirmation)
- Create: `crates/fl_content/src/{terrain,layers}.rs`, `crates/fl_content/tests/terrain.rs` (the test writes its mods into temp dirs; no binary fixtures are committed)
- Blessed: `xtask/snapshots/api/{fl_content,fl_sim}.txt` (`fl_content` is an API crate since the final M0 fix wave); the golden replay (base map shape and rules hash change: re-record `testbed_wander.replay.new`)

**Interfaces:**
- Consumes: `ContentKind`, `KindTable`, `LinkCtx`, `load_dirs`, `MapDef`.
- Produces:
```rust
// fl_content/src/terrain.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum MoveClass { Foot, Wheeled, HalfTrack, Tracked }       // moved from fl_sim; fl_sim keeps `pub use fl_content::MoveClass`
impl MoveClass { pub const ALL: [MoveClass; 4]; pub fn index(self) -> usize; }   // declaration order, 0..4
/// `*.terrain.ron`; IN_RULES_HASH = true
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)] #[serde(deny_unknown_fields)]
pub struct TerrainDef { pub id: String, pub name: String, pub pass: Fx, pub blocks: Vec<MoveClass>, pub color: (f32, f32, f32) }
    // link: pass in [0, 1]; blocks has no duplicates
// fl_content/src/layers.rs
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)] #[serde(deny_unknown_fields)]
pub struct MapTerrain { pub height_png: String, pub terrain_png: String, pub height_m: (Fx, Fx), pub legend: Vec<((u8, u8, u8), String)> }
    // file names relative to the map file's directory; height_m = (value at pixel 0, value at pixel max)
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MapLayers { pub cells_x: u32, pub cells_y: u32, pub palette: Vec<String>, pub types: Vec<u8>, pub heights: Vec<u16> }
    // palette: the legend's terrain ids, sorted and unique; types[i] indexes it; index = y * cells_x + x, y north
impl MapLayers { pub fn index(&self, x: u32, y: u32) -> usize; }
pub fn decode_layers(terrain: &MapTerrain, dir: &Path, cells: (u32, u32)) -> Result<MapLayers, String>;
pub fn write_layers(terrain: &MapTerrain, dir: &Path, layers: &MapLayers) -> std::io::Result<()>;   // the inverse: 16-bit gray + RGB8
// builtin.rs: MapDef gains
#[serde(default)] pub terrain: Option<MapTerrain>,
#[serde(skip_deserializing, default)] pub layers: Option<MapLayers>,   // set by load_sidecars; serialized, so the rules hash covers every pixel
// kinds.rs: ContentKind gains
fn load_sidecars(&mut self, _file: &Path) -> Result<(), String> { Ok(()) }   // called by the loader after parsing, before the link pass;
                                                                             //   an Err becomes a ContentError on `_file`
// kinds.rs: RulesSection gains `fn link(&self, _ctx: &mut LinkCtx<'_>) {}`: value-range checks in the link pass, reported at the field like ContentKind::link
//   (Task 5's Movement and Path sections implement it, ADR-0014)
// registry: at most 255 TerrainDefs (a cell type is a u8; TerrainGrid::OPEN = u8::MAX); a 256th is a load error
```
- **Pixel mapping.**
  - Pixel `(px, py)` is cell `(px, cells_y − 1 − py)`.
  - Heights accept 16-bit grayscale, or 8-bit, where a value v counts as v × 257 (grayscale without alpha only).
  - The terrain PNG is 8-bit RGB, or 8-bit RGBA with every alpha 255.
  - Any other format (indexed/`PLTE`, gray with alpha, bit depths below 8, 16-bit RGB/RGBA, a colour height PNG, a gray terrain PNG) is an error naming the file, colour type and bit depth. No palette expansion or bit-depth transformation; samples are raw, and `gAMA`/`sRGB`/`iCCP`/`cHRM` and other ancillary chunks are ignored (ADR-0014).
  - Two legend entries with the same RGB are an error at the second: `terrain.legend[i]`.
  - An image size other than `size_m / cell_m` is an error: `"<file>: expected 250x250 pixels, found 249x250"`.
  - A colour missing from the legend is an error: `"<file>: pixel (12, 3) colour #ff00ff is not in the legend"`.
- **Link checks** report legend ids that are not `TerrainDef`s at `terrain.legend[i]`.

- [ ] **Step 1: Write the failing tests** (`crates/fl_content/tests/terrain.rs`, with a local helper `write_mod(dir, terrains, map_ron, layers)` built on `write_layers`):
```rust
#[test] fn terrain_kind_loads()                      // grass + water kinds, a 4x3-cell map with layers → kind::<TerrainDef>().len() == 2; map.layers is Some
#[test] fn layers_are_north_up()                     // a red pixel at image (1, 0) → types at cell (1, 2) is the red legend entry
#[test] fn layer_size_mismatch_is_error()            // a 4x2 image for a 4x3-cell map → error contains "expected 4x3 pixels, found 4x2"
#[test] fn unknown_legend_colour_is_error()          // pixel #ff00ff → error contains "colour #ff00ff is not in the legend"
#[test] fn legend_naming_unknown_terrain_is_error()  // legend id "t.nope" → link error at "terrain.legend[1]"
#[test] fn heights_accept_16_and_8_bit()             // 16-bit 65535 and 8-bit 255 both decode to 65535; 8-bit 1 → 257
#[test] fn rules_hash_changes_with_one_pixel()       // two otherwise equal mods, one terrain pixel different → rules hashes differ
#[test] fn map_without_terrain_has_no_layers()
#[test] fn layers_cannot_be_written_in_ron()         // a map file with `layers: Some(..)` → parse error naming `layers`
#[test] fn pass_out_of_range_is_error()              // pass 1.5 → error at "pass"
#[test] fn duplicate_blocks_is_error()               // blocks: [Tracked, Tracked] → error at "blocks"
// proptest: random 1..16 × 1..16 layers, write_layers then decode_layers → equal
#[test] fn layers_round_trip()
#[test] fn unsupported_png_formats_are_errors()      // terrain PNG indexed (PLTE), gray+alpha, 4-bit gray, RGB16; height PNG RGB8 or 4-bit gray →
                                                     //   error naming the file, colour type and bit depth
#[test] fn ancillary_colour_chunks_are_ignored()     // a terrain PNG with gAMA, sRGB and iCCP chunks decodes to the same types as without them
#[test] fn duplicate_legend_colour_is_error()        // two legend entries with (255, 0, 0) → error at "terrain.legend[1]"
#[test] fn too_many_terrain_kinds_is_error()         // 256 terrain kinds → error naming the 255 cap
#[test] fn rules_section_link_errors_are_reported()  // a test rules section whose link rejects a value → load error at that field
// xtask/src/arch.rs unit test, with fixture edges like headless_direct_rand_and_physics_deps_flagged:
#[test] fn png_direct_dep_only_in_content()          // png as a direct dependency of fl_content → OK; of fl_sim, fl_cli or fl_client → flagged
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_content --test terrain` and `cargo nextest run --profile agent -p xtask -E 'test(/png_direct/)'`. Expected: FAIL.
- [ ] **Step 3: Implement.** Register `TerrainDef` as a built-in kind next to `MapDef`. `MapDef::load_sidecars` decodes the layers, sizing them from `size_m / cell_m`. The loader calls `load_sidecars` for every kind item, right after parsing.
- [ ] **Step 4: Run** `cargo nextest run --profile agent -p fl_content -p fl_sim -p fl_cli -p fl_catalog`. Expected: PASS, apart from the golden replay test, which fails on the rules hash.
- [ ] **Step 5: Regenerate protected artifacts.** Run `cargo xtask api` (it writes `fl_content.txt.new` and `fl_sim.txt.new`) and `FL_RECORD_GOLDEN=1 cargo nextest run --profile agent -p fl_test_utils --test golden`. The lead blesses them with the reason `M1 Task 3: MoveClass moved to fl_content; MapDef gains terrain layers (rules hash)`.
- [ ] **Step 6: Gate** `cargo xtask gate task --track contract --track content --track sim-core`. Expected: PASS. `cargo deny check` passes too: `png` is MIT/Apache. `cargo xtask arch-check` prints OK: its new `png` rule finds a direct `png` dependency in `fl_content` only, and no `bevy*` crate joins `fl_content`'s or `fl_sim`'s tree.
- [ ] **Step 7: Commit** `feat(fl_content)!: terrain kind, PNG map layers through a sidecar hook, MoveClass in fl_content` with trailers `Refs: ADR-0014` and `Blessed: …`. The controller runs `gate pr` and fast-forwards `main`.

