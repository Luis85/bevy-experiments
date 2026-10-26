---
status: "accepted"
date: 2026-10-24
decision-makers: "controller (accepted at the M1 Task 1 review, rulings R69, R84), lead"
consulted: "M1 planner (planner-m1-report.md, spec-gap resolutions accepted provisionally as R69); research: blitzkrieg-mechanics.md §1.8"
informed: "M1 implementers (Tasks 3, 4, 5, 7, 11, 13, 17)"
---

# Terrain layers and movement classes: PNG layers decoded by `fl_content`, per-class blocking and clearance

## Context and Problem Statement

M1 brings terrain (spec §7, §9.1): a cell grid with a height layer and a terrain-type layer, a speed formula,
per-class blocking, and pathfinding with clearance for unit size. Spec §6.2 says only "PNG layers for height and
terrain type" and lists terrain-type fields loosely. M0's maps are open rectangles (`MapDef { size_m, cell_m, .. }`),
and `MoveClass` lives in `fl_sim`, so content can't name it. Several things are undecided: where images are decoded,
given that sim crates may not use Bevy's image stack (ADR-0001, ADR-0002); how pixels map to cells; how terrain
enters the rules hash and the state hash; and how a class blocks a cell. The research (§1.8) also gives negative
passabilities (a truck's is −0.18), which the M0 `MobilityDef` validation rejects.

## Decision Drivers

* Determinism: sim state is integers and `Fx`, and hashed state names content by id, never by a dense index (R52,
  R54).
* Dependency rules: no `bevy_render`, `bevy_image` or `bevy_asset` in sim crates or `fl_content`; `arch-check` must
  keep passing.
* The rules hash covers everything that changes outcomes, every pixel included (ADR-0007).
* Terrain changes during a match: bridges, craters and flattened forest later, `debug.set_terrain` in M1.
* HPA* graphs are shared per movement class, while unit types within a class differ in their own factors.
* Maps are authored by tools (`fl_cli map-gen`) and later by hand in an image editor.

## Considered Options

* A cell grid from PNG layers, decoded once by `fl_content` into integer `MapLayers`
* A cell grid stored as arrays inside the map's RON file
* A cell grid from PNGs loaded through Bevy's `AssetServer` and `bevy_image`
* A navigation mesh over polygonal terrain regions

## Decision Outcome

Chosen option: "A cell grid from PNG layers, decoded once by `fl_content`", because it keeps image formats out of
every sim and client system, gives the rules hash and the sim one integer form, and keeps the authoring path an image
editor.

- **Terrain kind.** `*.terrain.ron` is a built-in content kind, `TerrainDef { id, name, pass, blocks, color }`, and
  it is rules-hashed. `pass` lies in [0, 1]; `blocks` lists movement classes without duplicates; `color` is
  presentation (its `f32`s hash as IEEE bits, ADR-0007).
- **Map layers.** `MapDef` gains `terrain: Option<MapTerrain>`. `MapTerrain` names a 16-bit (or 8-bit) grayscale
  height PNG and an RGB terrain PNG next to the map file, a height range in metres (`height_m`: the values at pixel 0
  and at the pixel maximum), and an RGB → terrain-id legend.
  - There is one pixel per cell, and pixel row 0 is the north edge: pixel `(px, py)` is cell
    `(px, cells_y − 1 − py)`.
  - **Accepted PNG formats.** The height PNG is grayscale without alpha, 16-bit, or 8-bit, where a value v counts as
    v × 257. The terrain PNG is 8-bit RGB, or 8-bit RGBA with every alpha 255. Anything else (indexed colour with a
    `PLTE`, gray with alpha, bit depths below 8, 16-bit RGB or RGBA, a colour height PNG, a gray terrain PNG) is a
    content error naming the file and the colour type and bit depth found; the decoder applies no palette expansion
    or bit-depth transformation.
  - **Samples are raw.** `gAMA`, `sRGB`, `iCCP`, `cHRM` and every other ancillary chunk are ignored, so legend
    matching never depends on a decoder's colour management.
  - An image whose size isn't `size_m / cell_m` cells, or a colour missing from the legend, is a content error naming
    the file (and the pixel). Two legend entries with the same RGB are an error at the second entry. A legend id
    that is not a `TerrainDef` is a link error at `terrain.legend[i]`.
  - The loader decodes the layers through a new `ContentKind::load_sidecars` hook (called after parsing, before the
    link pass) into a non-RON `MapDef.layers: Option<MapLayers>` field. RON can't set it, and it is serialized, so the
    rules hash covers every pixel.
  - `MapLayers { cells_x, cells_y, palette, types, heights }`: `palette` holds the legend's terrain ids, sorted and
    unique; `types` indexes it; `index = y × cells_x + x`, with y north.
  - At most 255 terrain kinds may be loaded, because a cell's type is a `u8` and `u8::MAX` means "no terrain kind"
    (open ground) in `TerrainGrid`; a 256th kind is a content error. Every palette, including one that
    `debug.set_terrain` grows, names loaded kinds only, so it stays within the cap.
  - A map without `terrain` is open ground: pass 1, blocks nothing, height 0.
  - **Where PNG decoding lives.** Only `fl_content` decodes images, with the `png` crate (pure Rust, no Bevy crate,
    so the `bevy*` rules of `arch-check` accept it in the content and sim trees). `arch-check` allows a direct `png`
    dependency in `fl_content` only. Sim crates and the client read the decoded integer `MapLayers`
    (`fl_content::{decode_layers, write_layers}` for tools and tests), never a PNG.
- **`MoveClass`** (`Foot`, `Wheeled`, `HalfTrack`, `Tracked`) moves to `fl_content`, which needs it for `blocks`.
  `fl_sim` re-exports it and keeps its `SimHash` impl.
- **Profiles.** A movement profile is `(class, size)`, where `size = ceil(footprint width / cell)` clamped to
  `1..=Path.max_clearance`. Navigation data is kept per profile that the content's units use.
- **Blocking.** A cell is blocked for a class if its terrain lists the class in `blocks`, or if the height step to a
  4-neighbour inside the map exceeds `Movement.max_slope_m[class]`. Cells outside the map count as blocked: they have
  clearance 0 and are never passable. A border cell is not blocked by the edge itself.
- **Clearance.** Clearance is the largest k ≤ `Path.max_clearance` for which the k×k square anchored at the cell and
  extending toward +x and +y is unblocked.
- **Where a unit stands.** A unit's position maps to its centre cell `c = cell_of(pos)`. A profile of size k covers
  the k×k square whose anchor (its −x/−y corner) is `c − ⌊(k − 1) / 2⌋` on each axis: the square is centred on c
  for odd k, and covers c and its +x/+y neighbours for even k. The profile is passable at c when the clearance at
  the anchor is at least k. For k ≤ 2, which covers every M1 unit, the anchor is c itself.
- **Speed** = `road_speed × clamp(pass + (1 − pass) × terrain_factor, Movement.min_speed_frac, 1)` (spec §7, with
  the clamp added). A unit's `terrain_factor` may lie in [−1, 1] (research §1.8: trucks are negative), so the clamp
  keeps the speed positive.
- **Path cost.** Path costs use a per-class `Movement.path_terrain_factor` in the same formula, as an integer cost per
  cell (`ceil(COST_ROAD / factor)`, `COST_ROAD = 100`). One class's unit types differ in their own factors, and
  HPA* is shared per class; the unit's own `terrain_factor` still drives its actual speed.
- **Evaluation is pinned.** The factor is computed with `Fx` operations in the written order: `ONE − pass`, times
  the terrain factor, plus `pass`, then the clamp. `Fx`'s `Mul` drops the low bits (`fixed`'s rounding toward −∞),
  and nothing is rounded to nearest. Rules values come from RON, which rounds a decimal to the nearest raw `Fx`, so
  tests compare against the parsed values (0.05 is raw 214748365), never against `Fx::from_ratio`, which truncates.
  The cost is a ceiling on raw integers, `(COST_ROAD × 2^32 + factor.raw − 1) / factor.raw`, so no `Fx` division
  rounds it.
- **Terrain is Hashed state** (`core.terrain`): `TerrainGrid { cells_x, cells_y, palette, types, heights, height_m }`,
  with the palette hashed and saved as terrain names. It is mutable by `debug.set_terrain` in M1 (new names append to
  the palette; existing indices never move), and later by craters, flattened forest and bridges. Bridges are terrain
  cells until destructible bridge objects arrive (M7).
- **Rules values** live in `rules.ron` sections `Movement` (`min_speed_frac`, `max_slope_m`, `path_terrain_factor`,
  …) and `Path` (`max_clearance`, …), each marked as a guess until tuned (spec §7).
- **The content loader validates their ranges.** `RulesSection` gains a `link` hook like `ContentKind::link`, and a
  value out of range is a content error at its field:
  - `min_speed_frac` lies in (0, 1] and is at least `COST_ROAD / u16::MAX`, so every cost fits a `u16`;
  - `max_slope_m` and `path_terrain_factor` each have an entry for every `MoveClass`; slopes are ≥ 0;
  - `path_terrain_factor` values, like a unit's `terrain_factor`, lie in [−1, 1];
  - `max_clearance`, `cluster_cells` and the other counts the sim divides or indexes by are ≥ 1.

### Consequences

* Good, because the sim and the client see one integer form of the layers, and no image crate enters a sim or client
  system.
* Good, because one changed pixel changes the rules hash, so a peer or replay with an edited map is caught up front.
* Good, because blocking, clearance and costs are pure functions of Hashed terrain plus content, so the navigation
  data built from them can be `Derived` (ADR-0015).
* Good, because maps are images that tools generate and people edit.
* Neutral, because height affects M1 only through slope blocking and rendering; vision height bonuses come in M3.
* Bad, because `fl_content` gains a decoding dependency and a sidecar hook in the content-kind contract.
* Bad, because a cell grid is coarse (4 m); sub-cell obstacles wait for building footprints (M4).
* Bad, because `max_slope_m` and `path_terrain_factor` are guesses that tuning will move.

### Confirmation

* Content format (Task 3, `fl_content/tests/terrain.rs`): `terrain_kind_loads`, `layers_are_north_up`,
  `layer_size_mismatch_is_error`, `unknown_legend_colour_is_error`, `legend_naming_unknown_terrain_is_error`,
  `heights_accept_16_and_8_bit`, `rules_hash_changes_with_one_pixel`, `map_without_terrain_has_no_layers`,
  `layers_cannot_be_written_in_ron`, `pass_out_of_range_is_error`, `duplicate_blocks_is_error`,
  `layers_round_trip`, `unsupported_png_formats_are_errors`, `ancillary_colour_chunks_are_ignored`,
  `duplicate_legend_colour_is_error`, `too_many_terrain_kinds_is_error`, `rules_section_link_errors_are_reported`.
* Base content (Task 4): `testbed_layout_matches_plan`, `testbed_scenario_units_on_passable_terrain`,
  `base_content_loads_terrain`.
* Speed, blocking and clearance (Task 5, `fl_sim/tests/terrain.rs`): `speed_formula_matches_spec`,
  `truck_slows_down_off_road`, `water_blocks_every_class`, `deep_forest_blocks_vehicles_not_foot`,
  `bridge_cells_pass_over_water`, `steep_slope_blocks_wheeled_not_foot`,
  `clearance_blocks_wide_units_in_narrow_gaps`, `units_stop_at_blocked_cells`, `spawn_on_blocked_cell_is_rejected`,
  `open_map_moves_at_road_speed`, `nav_cost_is_integer_ceiling`, `profile_anchor_is_centred`.
* Rules ranges (Task 5): `movement_rules_ranges_are_validated`, `path_rules_ranges_are_validated`.
* Terrain as Hashed state (Task 5): `set_terrain_blocks_a_bridge`, `incremental_nav_equals_full_rebuild`,
  `save_right_after_set_terrain`, `smoke_terrain_twin_and_shuffle`, `terrain_debug_queries`.
* Profiles end to end: `wide_profile_avoids_narrow_gap` (Task 7), `units_never_enter_blocked_cells` and
  `milestone_units_never_enter_blocked_cells_long` (Task 11), `scale_units_start_on_passable_cells` (Task 13).
* Reload: `terrain_kind_added_changes_id_set` and `value_reload_updates_passability` (Task 12).
* The client reads `MapLayers`, not PNGs: `terrain_view_from_base_testbed` (Task 17).
* `cargo xtask arch-check` in every gate: its `png` rule (Task 3, `xtask/src/arch.rs`, unit test
  `png_direct_dep_only_in_content`) allows a direct `png` dependency in `fl_content` only, and its tree rules keep
  every `bevy*` crate outside the `Content`/`Sim` allowances out of the `fl_content` and `fl_sim` trees. `cargo deny
  check` accepts `png`'s licence.

## Pros and Cons of the Options

### PNG layers decoded by `fl_content`

* Good, because the decode happens once at content load, where file I/O already happens.
* Good, because the sim, the client and the rules hash share one integer form.
* Good, because the authoring format is an ordinary image.
* Bad, because of a new dependency and a new `ContentKind` hook.

### Arrays inside the map's RON file

* Good, because there is no image dependency and the loader stays one format.
* Bad, because a 500 × 500 map is 250,000 entries per layer: slow to parse, unreadable in review, and impossible to
  paint.

### PNGs through Bevy's `AssetServer` and `bevy_image`

* Good, because Bevy already decodes images for the client.
* Bad, because `fl_content` and the sim crates may not depend on Bevy's asset or image stack (ADR-0001), and asset
  loading is asynchronous while content loading is a synchronous, strict pass.
* Bad, because the decoded images hold presentation formats that the rules hash would have to normalize.

### A navigation mesh over polygonal regions

* Good, because it is precise and has few nodes on open maps.
* Bad, because spec §7 models terrain as cells (passability, sight absorption and height per cell), and mutations
  (craters, bridges) would re-triangulate.
* Bad, because deterministic integer mesh building and clearance per size are much harder than a grid.

## More Information

* Spec §6.2 (terrain type and map layers), §7 (map, terrain and the speed formula), §9.1 (M1).
* Research: `blitzkrieg-mechanics.md` §1.8 (terrain and passabilities).
* Related: ADR-0001 (dependency rules), ADR-0004 (names in hashed state), ADR-0006 (Hashed and Derived state),
  ADR-0007 (rules hash; the M1 refinement adds `debug.set_terrain`), ADR-0015 (navigation data built from these
  rules).
* Controller rulings: R52, R54 (names, never dense ids), R60 (gameplay state is Hashed), R69 (the planner's
  resolutions: widened `terrain_factor`, per-class path factor, slope blocking, bridges as terrain until M7, the layer
  format).
* Revisit: when buildings bring footprint passability grids (M4), when vision needs height and sight absorption (M3),
  and when destructible bridges arrive (M7).
