### Task 8: Base content and `fl_catalog`

**Files:**
- `crates/fl_catalog/src/lib.rs` and `crates/fl_catalog/tests/base_content.rs`.
- `assets/base/{mod,rules,credits}.ron`.
- `assets/base/factions/{de,su}.faction.ron`.
- `assets/base/units/templates/{light_tank,medium_tank,heavy_tank,truck,infantry}.unit.ron`: abstract, with ids `base.abstract.*`.
- `assets/base/units/de/{pz2,pz3,tiger,opel_blitz,rifleman}.unit.ron` and `assets/base/units/su/{t70,t34,kv1,zis5,rifleman}.unit.ron`.
- `assets/base/visuals/<unit>.visual.ron`, one per concrete unit.
- `assets/base/maps/{testbed_1km,scale_2km}.map.ron` and `assets/base/scenarios/testbed.scenario.ron`.

**Interfaces — Produces:**
- `pub fn features() -> Vec<Arc<dyn GameFeature>>`. Empty in M0 because CoreFeature is implicit; each feature crate adds one line here later.
- `pub fn base_mod_dir() -> PathBuf`: `env!("CARGO_MANIFEST_DIR")/../../assets/base`, canonicalized.

**Content:**
- **Factions:** `base.de` (Germany, colour (0.42, 0.46, 0.50)) and `base.su` (USSR, colour (0.38, 0.45, 0.22)).
- **Maps:** `base.testbed_1km` (1000×1000, cell 4) and `base.scale_2km` (2000×2000, cell 4).
- **Scenario `base.testbed`:** map testbed_1km, seed 1. Players: DE (team 0, Human) and SU (team 1, Human). It places all 10 unit types: the DE row along y = 350 with heading 90°, and the SU row along y = 650 with heading 270°, each spaced 30 m apart starting at x = 350.
- **`rules.ron`:** `Rules({ "StateHash": (interval_ticks: 20), "Spatial": (cell_m: 32.0) })`.
- **`credits.ron`:** `Credits([])`.
- **Unit keys:** only `Selectable`, `Hull` and `Mobility`, since no others are registered in M0.
- **Stats** are our own plausible numbers:
  - Road speeds (km/h): pz2 40, pz3 40, tiger 38, t70 45, t34 54, kv1 35, trucks 60, riflemen 5.
  - Movement classes: infantry `Foot`, trucks `Wheeled`, tanks `Tracked`.

- [ ] **Step 1: Write the failing tests.**
  - `base_content_loads`: `fl_sim::load_content(&fl_catalog::features(), &[base_mod_dir()])` is `Ok` and has 10 concrete units, 2 factions, 1 scenario and no warnings.
  - `base_testbed_runs`: build the sim; `setup_scenario("base.testbed")` returns 10 ids, and 100 ticks run without a panic.
- [ ] **Step 2: Run** `cargo test -p fl_catalog`. Expected: FAIL.
- [ ] **Step 3: Write the RON files and `lib.rs`.**
- [ ] **Step 4: Run** `cargo test -p fl_catalog`. Expected: PASS.
- [ ] **Step 5: Gate + commit** `feat(content): base mod with DE/SU placeholder roster; fl_catalog`.

---

## Phase B — parallel tracks (one worktree each, after Task 8 is on `main`)

