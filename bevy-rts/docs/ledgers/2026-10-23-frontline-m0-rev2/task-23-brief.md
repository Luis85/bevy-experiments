### Task 23: Bench harness — `fl_cli bench` and criterion (rev-1 T16; tier 3 wall clock)

**Track:** tools · **Refs:** ADR-0010 · **Lane 2**

**Files:**
- Create: `crates/fl_cli/src/bench.rs`, `crates/fl_cli/tests/bench.rs`
- Modify: `crates/fl_cli/src/{lib,main}.rs`, `crates/fl_sim/benches/sim_tick.rs`

**Interfaces:**
- Consumes: `SimRunner`, `ScaleScenario`, `SimQuery::Perf`, `fl_observe::{Profiler, PerfSnapshot}`, `RunConfig { phase_markers: true, .. }`.
- Produces:
```rust
pub struct BenchConfig { pub units: Vec<u32>, pub ticks: u32, pub warmup: u32, pub seed: u64, pub repeat: u32 }
pub struct BenchRow { pub units: u32, pub entities: usize, pub mean_ms: f64, pub p95_ms: f64, pub max_ms: f64,
    pub per_phase_mean_ms: Vec<(String, f64)>, pub counters_per_tick: Vec<(String, f64)> }
pub fn run_bench(cfg: &BenchConfig) -> Vec<BenchRow>;          // per size: the median-of-`repeat` run, by mean_ms; warmup excluded
pub fn median(xs: &[f64]) -> f64;
pub fn profile_json(rows: &[BenchRow], perf: &[PerfSnapshot], meta: &serde_json::Value) -> serde_json::Value;
pub fn render_markdown(rows: &[BenchRow], meta: &str) -> String;
pub fn compare(rows: &[BenchRow], baseline: &[BenchRow], max_regression: f64) -> Result<(), String>;   // mean_ms per size
```
- **CLI:** `fl_cli bench [--units 500,3000,5000] [--ticks 400] [--warmup 40] [--seed 1] [--repeat 5] [--out <md>] [--json <path>] [--baseline <json> --max-regression 0.20]`. It prints the markdown table, and exits 1 on a regression.
- The wall clock here sits behind `#[expect(clippy::disallowed_types, reason = "bench wall clock")]`.
- **Criterion `sim_tick`:** groups `tick/500`, `tick/3000` and `tick/5000`, each iteration one `step_tick` after a 40-tick warmup, on the `core_mod` fixture content (ruling R3).

- [ ] **Step 1: Write the failing tests:**
```rust
#[test] fn bench_reports_each_size()        // units [50, 100], 20 ticks, repeat 1 → 2 rows; per_phase_mean_ms.len() == 8; mean_ms > 0
#[test] fn median_of_repeats()              // median(&[3., 1., 2., 5., 4.]) == 3.0
#[test] fn compare_flags_regression()       // baseline 1.0 ms at 0.20: 1.3 → Err naming the size; 1.1 → Ok
#[test] fn instrumented_hash_unchanged()    // phase markers and profiler on vs off → equal final state hash
#[test] fn profile_json_has_features_and_phases()   // 100 units → "features" contains "core.movement" with p95_ms > 0; "phases" has 8 keys
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_cli --test bench`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** the tests. Expected: PASS, and `cargo bench -p fl_sim --bench sim_tick -- --quick` runs.
- [ ] **Step 5: Gate** `cargo xtask gate task --track tools --track sim-core`. Expected: PASS.
- [ ] **Step 6: Commit** `feat(fl_cli): scale bench with median-of-N, per-phase and counter breakdown; criterion tick bench`.

