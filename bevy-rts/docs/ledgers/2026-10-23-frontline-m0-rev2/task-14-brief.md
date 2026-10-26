### Task 14: Base content and `fl_catalog` (rev-1 T8)

**Track:** content + sim-core · **Lane 1**

**Files:**
- Create `crates/fl_catalog/src/lib.rs` and `crates/fl_catalog/tests/base_content.rs`, plus the content files, all as listed in rev-1 Task 8:
  - `assets/base/{mod,rules,credits}.ron`, `factions/`
  - `units/templates/` and `units/{de,su}/`
  - `visuals/`, `maps/{testbed_1km,scale_2km}.map.ron`, `scenarios/testbed.scenario.ron`

**Interfaces:**
- Consumes: `SimFeature`, `load_content`, `build_sim_app`, `MatchSetup::for_scenario`, `setup_scenario`, `SimRunner`, `TestDriver`.
- Produces:
  - `pub fn features() -> Vec<Arc<dyn SimFeature>>`, empty in M0 (core is implicit; each feature crate adds one line here)
  - `pub fn base_mod_dir() -> PathBuf`: `env!("CARGO_MANIFEST_DIR")/../../assets/base`, canonicalized
- Content: identical to rev-1 Task 8's **Content** list (factions, maps, the `base.testbed` scenario with seed 1, `rules.ron` with `StateHash` and `Spatial`, an empty `credits.ron`, ten units using only `Selectable`, `Hull` and `Mobility`, and the same stats). It is our own numbers, never Nival's.

- [ ] **Step 1: Write the failing tests:**
```rust
#[test] fn base_content_loads()    // load_content(&features(), &[base_mod_dir()]) → Ok: 10 concrete units, 2 factions, 2 maps, 1 scenario, warnings empty
#[test] fn base_testbed_runs()     // MatchSetup::for_scenario(&db, "base.testbed") has seed 1; with cheats_allowed = true, build → setup_scenario returns 10 ids;
                                   //   SimRunner + TestDriver run 100 ticks; the last snapshot has 10 records
#[test] fn base_string_tables()    // header table "content.unit" has 10 names, sorted ascending
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_catalog`. Expected: FAIL.
- [ ] **Step 3: Write** the RON files and `lib.rs`.
- [ ] **Step 4: Run** the tests. Expected: PASS.
- [ ] **Step 5: Gate** `cargo xtask gate task --track content --track sim-core`. Expected: PASS. `data-check` stays SKIP until Task 16.
- [ ] **Step 6: Commit** `feat(content): base mod with DE/SU placeholder roster; fl_catalog`.

