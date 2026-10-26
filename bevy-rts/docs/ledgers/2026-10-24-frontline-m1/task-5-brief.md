### Task 5: Sim terrain — `TerrainGrid`, `NavGrid`, speed formula, class blocking, `debug.set_terrain`, ASCII fixtures

**Track:** contract + sim-core + content + tools · **Refs:** ADR-0014, ADR-0006 (M1 refinement: R78 `fill_missing`) · **Lane 1**

**Files:**
- Create:
  - `crates/fl_sim/src/terrain.rs`, `crates/fl_sim/src/nav/{mod,grid}.rs`, `crates/fl_sim/tests/terrain.rs`
  - `crates/fl_test_utils/src/terrain.rs`, `crates/fl_test_utils/tests/m0_save.rs`
  - `crates/fl_test_utils/tests/replays/m0_testbed.save.new`: the M0 save fixture (R78). It is protected, so the lead blesses it into `m0_testbed.save`.
- Modify:
  - `crates/fl_sim/src/{lib,scenario,scale,spawn,movement,units,debug_cmds,commands,build}.rs` (`lib.rs` declares `pub mod nav` and `mod terrain`; `build.rs` holds `SIM_VERSION`)
  - `crates/fl_sim/src/classify.rs` (`Codec::fill_missing`), `crates/fl_sim/src/save/load.rs` (calls the fills), `crates/fl_sim/tests/save.rs` (fill tests). These are contract paths (R78, ADR-0006 M1 refinement).
  - `crates/fl_sim/src/{feature,spatial}.rs`: `CoreFeature::build` registers core's one rebuild, `rebuild_core`. The `.register_rebuild(rebuild_spatial)` call moves out of `spatial::build`, and `rebuild_spatial` stays a plain function that `rebuild_core` calls (R74).
  - `crates/fl_sim/Cargo.toml` (`[dev-dependencies] fl_test_utils.workspace = true`), `Cargo.lock`
  - `crates/fl_test_utils/src/{lib,builder}.rs`, `crates/fl_test_utils/Cargo.toml` (`tempfile.workspace = true` in `[dependencies]`; it has no `tempfile` today)
  - the `rules.ron` of `assets/base`, `crates/fl_sim/tests/fixtures/{core_mod,core_mod_extra,core_mod_factions,absurd_speed}` and `crates/fl_cli/tests/fixtures/bad_mod`
- Blessed: system order, `crates/fl_test_utils/tests/snapshots/formats__save_codecs.snap` (`core.terrain@1`), API (`fl_sim`), golden replay, `crates/fl_test_utils/tests/replays/m0_testbed.save`.

**Interfaces:**
- Consumes: `MapDef.layers`, `TerrainDef`, `MoveClass` (Task 3); `register_debug_query` (Task 2); `DebugSetTerrainCmd` (Task 2).
- Produces:
```rust
// R78: filling state that an older save lacks (classify.rs, save/load.rs)
impl<T> Codec<T> { pub fn fill_missing(self, fill: fn(&mut World) -> Result<(), String>) -> Self; }
    // load_world calls `fill` when the save lacks this Hashed (or Derived) state name AND the save's version of the owning feature is older than
    // the build's (MatchHeader.features). Fills run after every saved value is restored, before the Derived rebuilds, in feature load order,
    // then by state name. An Err is LoadError::Codec { name, message }.
    // Without a fill, a missing name behaves as it does today (save/load.rs step 5 removes a missing Hashed resource; R78 says "LoadError",
    // but the code removes it, and saves without a map rely on that). A save of the same feature version never fills.
// CoreFeature::version() becomes 2 here (stays 2 through M1), so an M0 save is an older core save and its fills run
// core.terrain fill: TerrainGrid from the content's MapDef whose size_m and cell_m equal the restored MapInfo (saves carry no map id):
//   exactly one such map with layers → TerrainGrid::from_layers; none with layers → TerrainGrid::open(MapInfo cells);
//   two or more → Err("the save's map matches several maps: <ids>"); no MapInfo → no TerrainGrid (as a live match without a map)
// rules sections registered by core, with every field later tasks read (values in assets/base/rules.ron, marked "guess"):
//   "Movement": MovementRules { min_speed_frac: Fx (0.05), turn_in_place_deg: Fx (30), arrive_m: Fx (0.5), formation_max_spread_m: Fx (40),
//               follow_distance_m: Fx (8), follow_repath_ticks: u32 (20), max_slope_m: BTreeMap<MoveClass, Fx> {Foot 100, Wheeled 1.5, HalfTrack 2, Tracked 2.5},
//               path_terrain_factor: BTreeMap<MoveClass, Fx> {Foot 1.0, Wheeled 0.2, HalfTrack 0.45, Tracked 0.5} }
//   "Path":     PathRules { cluster_cells: u32 (16), budget_per_tick: u32 (8000), max_clearance: u8 (3), snap_radius_cells: u32 (8),
//               flow_min_group: u32 (6), flow_cache_max: u32 (32) }
//   "Steering": SteeringRules { separation_gain: Fx (0.5), separation_margin_m: Fx (0.5), idle_every: u32 (5), max_push_frac: Fx (0.5) }
//   MovementRules and PathRules implement RulesSection::link (Task 3), ranges per ADR-0014: min_speed_frac in (0, 1] and ≥ COST_ROAD / u16::MAX;
//   max_slope_m and path_terrain_factor each cover MoveClass::ALL; slopes ≥ 0; path_terrain_factor in [−1, 1]; cluster_cells, max_clearance,
//   budget_per_tick, flow_min_group, flow_cache_max and idle_every ≥ 1
// terrain.rs
#[derive(Resource, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerrainGrid { pub cells_x: u32, pub cells_y: u32, pub palette: Vec<String>, pub types: Vec<u8>, pub heights: Vec<u16>, pub height_m: (Fx, Fx) }
    // Hashed "core.terrain" v1 (palette hashed as names); empty types/heights = every cell OPEN at height_m.0
impl TerrainGrid {
    pub const OPEN: u8 = u8::MAX;                                      // no terrain kind: pass 1, blocks nothing
    pub fn open(cells_x: u32, cells_y: u32) -> Self;
    pub fn from_layers(layers: &MapLayers, height_m: (Fx, Fx)) -> Self;
    pub fn terrain_at(&self, x: u32, y: u32) -> Option<&str>;
    pub fn height_at(&self, x: u32, y: u32) -> Fx;                     // lo + (hi − lo) × v / 65535
    pub fn set_rect(&mut self, rect: CellRect, terrain: &str);         // new names are appended to the palette (existing indices never move)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)] pub struct CellRect { pub min: (u32, u32), pub max: (u32, u32) }   // inclusive
pub(crate) fn init_terrain(world: &mut World, map: Option<&MapDef>);   // next to every init_spatial call (setup_map, setup_scenario_def, scale populate)
// nav/grid.rs
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)] pub struct MoveProfile { pub class: MoveClass, pub size: u8 }
pub fn profile_of(mobility: &Mobility, footprint: Option<&Footprint>, cell: Fx, max_clearance: u8) -> MoveProfile;   // size = ceil(width / cell) in 1..=max
pub fn content_profiles(content: &ContentDb, cell: Fx, max_clearance: u8) -> Vec<MoveProfile>;   // sorted, unique, over units with Mobility
pub const COST_ROAD: u16 = 100;
#[derive(Resource, Clone, Debug)] pub struct NavGrid { .. }   // Derived "core.nav_grid", rebuilt by rebuild_core → rebuild_nav
impl PartialEq for NavGrid { .. }  impl Eq for NavGrid {}   // by hand: compares every field except `version` (observe-only), so a
                                                                // loaded or freshly built grid equals the live one
impl NavGrid {
    pub fn build(terrain: &TerrainGrid, content: &ContentDb, rules: &MovementRules, profiles: &[MoveProfile], max_clearance: u8) -> Self;
    pub fn cells(&self) -> (u32, u32);
    pub fn cell_of(&self, pos: FxVec2) -> (u32, u32);                 // clamped into the grid
    pub fn blocked(&self, class: MoveClass, x: u32, y: u32) -> bool;
    pub fn clearance(&self, class: MoveClass, x: u32, y: u32) -> u8;
    pub fn passable(&self, p: MoveProfile, x: i64, y: i64) -> bool;    // (x, y) is the unit's centre cell; false off-map; clearance at the anchor
                                                                       //   (x − (size − 1) / 2, y − (size − 1) / 2) >= p.size (ADR-0014; the anchor is (x, y) for size ≤ 2)
    pub fn cost(&self, class: MoveClass, x: u32, y: u32) -> u16;       // ceil(COST_ROAD / factor), factor = clamp(pass + (1 − pass) × path_terrain_factor[class], min_speed_frac, 1);
                                                                       //   Fx ops in that order; the ceiling on raw integers: ((COST_ROAD << 32) + factor.raw − 1) / factor.raw
    pub fn speed_factor(&self, terrain_factor: Fx, x: u32, y: u32) -> Fx;   // clamp(pass + (1 − pass) × terrain_factor, min_speed_frac, 1)
    pub fn update_rect(&mut self, terrain: &TerrainGrid, content: &ContentDb, rules: &MovementRules, rect: CellRect) -> CellRect;
        // recomputes blocking for rect grown by 1 (slope), clearance for that rect grown by max_clearance − 1 toward −x/−y; returns the changed rect
    pub fn profiles(&self) -> &[MoveProfile];
    pub fn version(&self) -> u64;                                      // +1 per update; 0 after a rebuild (observe-only)
}
pub fn rebuild_nav(world: &mut World);                                // NavGrid now; HpaGraphs (Task 7) and FlowCache (Task 10) join it
pub(crate) fn rebuild_core(world: &mut World);                        // core's one Derived rebuild (R74): rebuild_spatial, then rebuild_nav;
                                                                      //   registered once, in CoreFeature::build
pub(crate) fn on_terrain_changed(world: &mut World, rect: CellRect);  // the single hook Tasks 7, 8 and 10 extend
// RejectReason::Feature("core.invalid_target"): code "core.invalid_target", no new variant (R71);
// SpawnError::BlockedCell ("the position is blocked for this unit")
// SIM_VERSION = 2
// debug queries: "core.passability" args {"class":"Tracked","size":1} or {"unit":7} → {"class","size","cells":[x,y],"version":n,"blocked":[[start,len],..]}
//   (row-major run lengths of impassable cells); "core.terrain_at" args {"cell":[x,y]} → {"cell":[x,y],"terrain":id|null,"height_raw":i64,
//   "blocked":{"Foot":bool,..},"clearance":{..},"cost":{..}}
// journal "core.terrain_changed" { min: (u32,u32), max: (u32,u32), terrain: String }; counter "nav.cells_updated"
// fl_test_utils/src/terrain.rs
pub fn ascii_terrain_content(rows: &[&str], cell_m: i32) -> Arc<ContentDb>;
pub fn ascii_terrain_content_with(rows: &[&str], cell_m: i32, rules_ron: Option<&str>) -> Arc<ContentDb>;   // rules default: assets/base/rules.ron
    // legend: '.' grass 0.8, 'r' road 1.0, 'f' forest 0.7, 'F' deep forest (blocks vehicles), '~' water (blocks all), '=' bridge 1.0,
    //   's' swamp (blocks all), '1'..'9' grass at that height in metres; row 0 is the north edge
    // units: "t.tank" (Tracked, 36 km/h, factor 0.5, width 3.0), "t.truck" (Wheeled, 72 km/h, factor 0.2, width 2.4),
    //   "t.rifleman" (Foot, 7.2 km/h, factor 1.0, width 0.6), "t.wide_tank" (Tracked, 36 km/h, factor 0.5, width 7.0); factions "t.red", "t.blue"
impl ScenarioBuilder { pub fn ascii_map(self, rows: &[&str], cell_m: i32) -> Self; pub fn ascii_rules(self, rules_ron: &str) -> Self; }   // map "t.ascii"
```
- **`MobilityDef` validation** widens `terrain_factor` to [−1, 1] (ADR-0014).
- **Movement.** `move_system` multiplies the step by `speed_factor` of the cell under the unit. It never steps into a cell that isn't passable for the unit's profile; such a step is skipped and counts `move.blocked`.
- **Spawns.**
  - `check_spawn` rejects a unit with `Mobility` on a cell blocked for its profile with `SpawnError::BlockedCell`. That covers setup and the funnel.
  - The `debug.spawn` handler checks the same cell first and rejects with `core.invalid_target`, so its documented rule "an applied command always spawns" still holds.
  - `debug.teleport` onto a blocked cell is rejected with `core.invalid_target`.
- **`debug.set_terrain`.** The handler rejects:
  - an unknown terrain id with `UnknownContent` (`core.unknown_content`)
  - `min > max`, or a rect outside the map, with `core.invalid_target`

  Otherwise it sets `TerrainGrid`, calls `on_terrain_changed` (which runs `NavGrid::update_rect`), counts `nav.cells_updated`, and journals `core.terrain_changed`.

- [ ] **Step 0: Generate the M0 save fixture (R78), before any Task 5 change.**
  1. In a temporary worktree of the M0 tip of `main` (`git -C C:\fl\a worktree add <scratch>/m0main main`), run `cargo run -p fl_cli -- save base.testbed --tick 40 --out <scratch>/m0_testbed.save --json`.
  2. Copy it to `crates/fl_test_utils/tests/replays/m0_testbed.save.new`, and record the `hash`, `tick`, `entities` JSON and the `main` commit in the report.
  3. Remove the worktree (`git worktree remove`).

  The implementer never writes the protected `m0_testbed.save`; the lead blesses the `.new` with `M1 Task 5: M0 testbed save fixture (R78)`.
- [ ] **Step 1: Write the failing tests** (`crates/fl_sim/tests/terrain.rs`; hash interval 1):
```rust
#[test] fn speed_formula_matches_spec()        // exact Fx: (pass, factor) → (1.0, 0.5) 1; (0.8, 0.5) 9/10; (0.7, 0.2) 19/25; (0.5, 1.0) 1; (0.0, 0.0) clamped to the RON-parsed Movement.min_speed_frac (0.05 → raw 214748365; never Fx::from_ratio(1, 20), which truncates to 214748364); (0.8, −0.18) 191/250
#[test] fn nav_cost_is_integer_ceiling()       // cost: a road cell → 100; pass 0.5 with Tracked path factor 0.5 → 134; with Wheeled 0.2 → 167
#[test] fn movement_rules_ranges_are_validated() // min_speed_frac 0, 1.5 or below 100/65535; max_slope_m or path_terrain_factor missing a class;
                                               //   path_terrain_factor 1.5; MobilityDef terrain_factor −1.5 → load errors at the field
#[test] fn path_rules_ranges_are_validated()   // cluster_cells 0, max_clearance 0, idle_every 0 → load errors at the field
#[test] fn profile_anchor_is_centred()         // pure NavGrid: a size-3 profile at centre cell (5, 5) is passable iff clearance(4, 4) ≥ 3;
                                               //   sizes 1 and 2 anchor at the centre cell
#[test] fn truck_slows_down_off_road()         // spec §12.4: t.truck 40 ticks on "rrrr…" vs "...." rows → distance ratio in [0.83, 0.85] (0.8 + 0.2 × 0.2 = 0.84)
#[test] fn water_blocks_every_class()
#[test] fn deep_forest_blocks_vehicles_not_foot()
#[test] fn bridge_cells_pass_over_water()
#[test] fn steep_slope_blocks_wheeled_not_foot()   // rows "..9.." : t.truck profile blocked at the step, t.rifleman not
#[test] fn clearance_blocks_wide_units_in_narrow_gaps()   // a 1-cell gap in a water wall: t.tank (size 1) passable, t.wide_tank (size 2 at 4 m) not
#[test] fn units_stop_at_blocked_cells()       // a straight move into water: the unit stops at the shore; "move.blocked" counted; never on water
#[test] fn spawn_on_blocked_cell_is_rejected() // debug.spawn on '~' → rejected, code "core.invalid_target", no unit; a scenario unit on '~' →
                                               //   SetupError::Unit { error: SpawnError::BlockedCell, .. }
#[test] fn teleport_into_water_is_rejected()   // code "core.invalid_target"; gameplay state unchanged; LastSeq advanced (R45)
#[test] fn set_terrain_blocks_a_bridge()       // '=' cells set to "t.water" → passable false for every profile; journal "core.terrain_changed"
#[test] fn set_terrain_rejects_bad_input()     // unknown terrain → "core.unknown_content"; min > max or outside → "core.invalid_target";
                                               //   state unchanged (R45)
#[test] fn set_terrain_needs_cheats()          // cheats off → "core.debug_disabled"
// proptest: random 32×32 ASCII maps; 1..8 random set_terrain rects → NavGrid after update_rect == NavGrid::build from scratch
#[test] fn incremental_nav_equals_full_rebuild()
#[test] fn save_right_after_set_terrain()      // save after the applying tick; load → TerrainGrid and NavGrid equal to the live ones (NavGrid's
                                               //   PartialEq ignores `version`); branch hashes equal to tick +50
#[test] fn core_registers_one_rebuild()        // the core feature's rebuild runs once per load and leaves SpatialGrid and NavGrid equal to the live ones
// crates/fl_sim/tests/save.rs (R78; test features "aaa" v1 → v2 as save_of_an_older_feature_version_loads does)
#[test] fn fill_missing_runs_for_an_older_feature_save()   // v2 adds Hashed "aaa.extra" with a fill → a v1 save loads with the filled value
#[test] fn fill_missing_skipped_for_the_same_version()      // a v2 save lacking "aaa.extra" → the fill does not run (M0 behaviour)
#[test] fn fill_error_is_a_codec_load_error()               // a fill returning Err("x") → LoadError::Codec { name: "aaa.extra", message: "x" }
// crates/fl_test_utils/tests/m0_save.rs (base content, fl_catalog::features())
#[test] fn m0_testbed_save_loads_under_m1()    // load m0_testbed.save → Ok; TerrainGrid == from_layers(base.testbed_1km); NavGrid == a fresh build;
                                               //   tick 40 kept; runs 200 ticks without panic; two loads give equal hash streams
#[test] fn m0_save_round_trips_after_load()    // load, save at once, load again → equal state hash (fills ran once; the new save is core v2)
#[test] fn open_map_moves_at_road_speed()      // core_mod flat map (no layers): tank moves exactly Mobility.speed per tick (M0 behaviour)
#[test] fn terrain_debug_queries()             // "core.passability" for t.tank on "~~..": runs [[0,2]] in row 0; "core.terrain_at" names the terrain
#[test] fn smoke_terrain_twin_and_shuffle()    // assert_deterministic + assert_shuffle_invariant on an ASCII river scenario with moves, 200 ticks
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_sim --test terrain --test save` and `cargo nextest run --profile agent -p fl_test_utils --test m0_save`. Expected: FAIL.
- [ ] **Step 3: Implement.** `NavGrid` stores, per class, a blocked bit and a clearance byte per cell, plus a cost `u16` per cell and per class, and the pass `Fx` per palette entry. Clearance uses the recurrence `c = 0` if blocked, else `1 + min(c(x+1,y), c(x,y+1), c(x+1,y+1))`, capped at `max_clearance`, with cells off the map counting 0. `CoreFeature` registers the queries and `rebuild_core`. Classify `TerrainGrid` as `Class::hashed(1)` and `NavGrid` as `Class::Derived`.
- [ ] **Step 4: Run** `cargo nextest run --profile agent -p fl_sim -p fl_test_utils -p fl_cli`. Expected: PASS, apart from the protected snapshots and the golden.
- [ ] **Step 5: Regenerate** system order, save formats, API and the golden as `.new`. The lead blesses them with the reason `M1 Task 5: Hashed terrain grid, derived NavGrid, SIM_VERSION 2`.
- [ ] **Step 6: Gate** `cargo xtask gate task --track contract --track sim-core --track content --track tools`. Expected: PASS.
- [ ] **Step 7: Commit** `feat(fl_sim): terrain grid, NavGrid with class blocking and clearance, speed formula, debug.set_terrain; fill_missing loads M0 saves` with trailers `Refs: ADR-0014, ADR-0006` and `Blessed: …`.

