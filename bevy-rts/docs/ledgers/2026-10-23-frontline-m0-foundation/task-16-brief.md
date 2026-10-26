### Task 16: Bench harness (`fl_cli bench` + criterion)

**Files:** `crates/fl_cli/src/bench.rs`; modify `crates/fl_cli/src/{lib,main}.rs` to add the subcommand; `crates/fl_cli/tests/bench.rs`; `crates/fl_sim/benches/sim_tick.rs`.

**Interfaces — Produces:**
```rust
pub struct BenchConfig { pub units: Vec<u32>, pub ticks: u32, pub warmup: u32, pub seed: u64 }
pub struct BenchRow { pub units: u32, pub mean_ms: f64, pub p95_ms: f64, pub max_ms: f64, pub per_set_mean_ms: Vec<(String, f64)> }
// per-SimSet times come from T18's fl_sim::observe::SetTimings (enable via SimConfig.profile = true);
// per-system / per-feature times come from T19's fl_observe::Profiler layer (installed by fl_cli for --profile)
pub fn run_bench(cfg: &BenchConfig) -> Vec<BenchRow>;        // ScaleScenario per size, base content, warmup excluded
pub fn profile_json(rows: &[BenchRow], snapshots: &[PerfSnapshot]) -> serde_json::Value;  // per size: per-set, per-feature, per-system p50/p95/max
pub fn render_markdown(rows: &[BenchRow], meta: &str) -> String;
pub fn compare(rows: &[BenchRow], baseline: &[BenchRow], max_regression: f64) -> Result<(), String>;  // mean_ms per size
```
**CLI:** `fl_cli bench [--units 500,3000,5000] [--ticks 400] [--warmup 40] [--seed 1] [--out <md>] [--save-baseline <ron>] [--baseline <ron> --max-regression 0.20] [--profile --json <path>]` prints the markdown table and exits 1 on a regression. `--profile --json docs/perf/m0.json` writes the per-system breakdown.

**Criterion bench `sim_tick`:** groups `tick/500`, `tick/3000` and `tick/5000`, each iteration one `SimTick` after a 40-tick warmup.

- [ ] **Step 1: Write the failing tests.**
  - `bench_reports_each_size`: `run_bench` with units `[50, 100]` and 20 ticks gives 2 rows. Every `per_set_mean_ms` has 9 sets, and the means are > 0.
  - `compare_flags_regression`: against a 1.0 ms baseline at 0.20, 1.3 ms is Err naming the size and 1.1 ms is Ok.
  - `instrumented_hash_unchanged`: the same scenario with profiling on and off gives an equal final state hash.
  - `profile_json_has_features`: `--profile` output for 100 units lists `core.movement` with p95 > 0 and at least one system name per non-empty SimSet.
- [ ] **Step 2: Run** `cargo test -p fl_cli --test bench`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test -p fl_cli --test bench`. Expected: PASS, and `cargo bench -p fl_sim --bench sim_tick -- --quick` runs.
- [ ] **Step 5: Gate + commit** `feat(fl_cli): scale benchmark harness and criterion tick bench`.

---

## Phase D — integration

