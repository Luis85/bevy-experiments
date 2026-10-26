### Task 13: Scale scenario on terrain with pathing; counters, budgets, bench

**Track:** sim-core + tools + perf + client · **Refs:** ADR-0010, ADR-0006 (replay format), spec §5.7 · **Lane 2** (after Task 11)

**Files:**
- Modify:
  - `crates/fl_sim/src/{scale,replay}.rs` (`REPLAY_FORMAT_VERSION` → 3, R62), `crates/fl_sim/tests/scale_mix.rs`, `crates/fl_sim/tests/replay.rs` (only if a test pins the replay format version)
  - `crates/fl_test_utils/src/perf.rs`, `crates/fl_test_utils/tests/perf_counters.rs`
  - `crates/fl_cli/src/bench.rs`, `crates/fl_cli/tests/bench.rs`
  - `crates/fl_game/src/builder.rs`, `crates/fl_game/tests/builder.rs`: the stress mode runs `ScaleScenario::on_terrain(units)`, and its `MapView.terrain` comes from that map through Task 17's `TerrainView::from_content`, so Task 21's render stress shows the terrain mesh.
- Blessed:
  - `docs/perf/budgets.ron`, `crates/fl_test_utils/tests/snapshots/perf_counters__*.snap`
  - `crates/fl_sim/tests/snapshots/formats__save_codec_formats.snap` (the replay layout: `ReplayStart::Scale`), API (`fl_sim`), golden (format version)

**Interfaces:**
- Consumes: `init_terrain`, `NavGrid`, `nearest_passable`, `profile_of` (Tasks 5–6); the `base.scale_2km` layers (Task 4); `TerrainView`, `MapView.terrain` (Task 17).
- Produces:
```rust
pub struct ScaleScenario { .., pub map: Option<String> }   // Default (and with_units): None = M0's open map, so the many existing tests that build
                                                           //   ScaleScenario::default() on fixture mods without base.scale_2km keep working
pub const SCALE_MAP: &str = "base.scale_2km";
impl ScaleScenario { pub fn on_terrain(units: u32) -> Self; }   // with_units(units) with map Some(SCALE_MAP)
    // populate: with a map, its layers set up TerrainGrid (map side must equal map_m: SetupError::MapSizeMismatch { map, map_m });
    // a unit's drawn position is redrawn up to 16 times until passable for its profile, then nearest_passable. The redraws come from the
    // same setup `placement` Rolls (the scenario's own seed, under populate's existing scoped #[expect(clippy::disallowed_methods)]),
    // never from rolls() or a new Stream
// fl_cli bench: runs ScaleScenario::on_terrain by default; --flat runs with_units (map None, the M0-comparable workload);
//   rows report "map": "base.scale_2km" | "open"
// fl_test_utils perf (perf_counters scenarios scale_500/3000/5000): ScaleScenario::on_terrain
```
- **Budgets.** The scale mix issues only single path requests (wander), so `flowfield.*` is never measured there and gets no budget. `check_budgets` fails a budgeted counter that a run doesn't measure.

- [ ] **Step 1: Write the failing tests:**
```rust
// crates/fl_sim/tests/scale_mix.rs
#[test] fn scale_units_start_on_passable_cells()   // on_terrain(3000): every unit's cell passable for its profile
#[test] fn scale_pathing_reaches_targets()         // on_terrain(500), 400 ticks: ≥ 1 path_result per unit; no unit on a blocked cell at any tick
#[test] fn flat_scale_has_open_terrain()           // ScaleScenario::default() (map None) → TerrainGrid::open
#[test] fn scale_map_size_must_match()             // on_terrain(100) with map_m 1000 → Err(MapSizeMismatch)
#[test] fn scale_mix_twin_run()                    // (existing) now on_terrain
// crates/fl_test_utils/tests/perf_counters.rs: smoke_counters_scale_500, counters_scale_3000, milestone_counters_scale_5000 (existing) re-recorded on
//   on_terrain with the measured path.*, steer.* and move.* counters (no flowfield.*: no group moves at scale); plus
#[test] fn path_budget_holds_at_scale()            // scale_3000: per tick, path.budget_spent − (the largest single charge that tick) < Path.budget_per_tick
// crates/fl_cli/tests/bench.rs
#[test] fn bench_flat_flag_uses_open_map()
// crates/fl_game/tests/builder.rs (prepare_headless)
#[test] fn stress_map_view_has_scale_terrain()     // stress(500): MapView.terrain is Some with cells (500, 500)
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_sim --test scale_mix` and `cargo nextest run --profile agent -p fl_game --test builder -E 'test(/stress_map_view/)'`. Expected: FAIL.
- [ ] **Step 3: Implement.** Then record: `FL_RECORD_BUDGETS=1 cargo nextest run --profile agent -p fl_test_utils --test perf_counters -E 'test(/scale/)'` (delete any stale `budgets.ron.new` first), and `FL_RECORD_GOLDEN=1` for the golden (format 3).
- [ ] **Step 4: Diagnostic wall clock (not a gate).** Run `cargo run -p fl_cli --release -- bench --units 3000 --ticks 200 --warmup 40 --repeat 1 --json <scratch>/m1-t13.json`, and the same with `--flat`. Paste both 3000-unit means in the report. Over 15 ms: profile with `--profile` and report the top systems.
- [ ] **Step 5: Bless (lead)** budgets, perf snapshots, formats, API and the golden: `M1 Task 13: scale mix on the 2 km terrain with pathing`.
- [ ] **Step 6: Run** `cargo nextest run --profile agent -p fl_sim -p fl_test_utils -p fl_cli -p fl_game`. Expected: PASS. Also add a temporary system that enqueues every unit every tick, and expect `smoke_counters_scale_500` to fail naming `path.budget_spent` or `path.queue_len`. Remove it afterwards.
- [ ] **Step 7: Gate** `cargo xtask gate task --track sim-core --track tools --track perf --track client`. Expected: PASS.
- [ ] **Step 8: Commit** `feat(perf): scale entity mix on the 2 km terrain with pathing; M1 counter budgets; bench --flat` with trailers `Refs: ADR-0010` and `Blessed: …`.

