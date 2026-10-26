### Task 19: `fl_observe` — logging setup, profiler layer, journal sink, debug.ron

Phase B, its own worktree.

**Files:**
- Create the crate `crates/fl_observe/` (`Cargo.toml`, `src/{lib,config,logging,profiler,sink}.rs`, `tests/profiler.rs`) and `debug.ron` at the repo root.
- Modify the root `Cargo.toml` to add the workspace deps `tracing-subscriber` (features `env-filter`, `json`, `fmt`, `registry`), `serde_json` if absent, and the `fl_observe` path dep.
- Modify `scripts/check_deps.sh` to add `fl_observe` to the headless list.
- `fl_observe` deps: fl_sim, fl_protocol, tracing, tracing-subscriber, serde, serde_json, ron, thiserror. No Bevy beyond what fl_sim brings.

**Interfaces — Produces:**
```rust
#[derive(Clone, Debug, Deserialize, Serialize)] #[serde(deny_unknown_fields, default)]
pub struct DebugConfig { pub log_filter: String /* "info,fl_sim=debug" */, pub json_log: bool, pub journal_file: bool, pub profiler: bool, pub budgets: Budgets }
#[derive(Clone, Debug, Default, Deserialize, Serialize)] #[serde(deny_unknown_fields, default)]
pub struct Budgets { pub tick_ms: f64 /* 15.0 */, pub per_set_ms: BTreeMap<String, f64>, pub per_feature_ms: BTreeMap<String, f64> }
pub fn load_debug_config(path: Option<&Path>) -> DebugConfig;     // explicit path, else ./debug.ron if present, else defaults; RUST_LOG overrides log_filter
pub struct RunDir { pub id: String, pub path: PathBuf }
pub fn new_run_dir(root: &Path) -> std::io::Result<RunDir>;       // root/<YYYYMMDD-HHMMSS>-<pid>/ ; also writes root/latest.txt with the id
pub fn json_file_layer<S>(run: &RunDir) -> impl Layer<S>;           // log.jsonl; each line: timestamp, level, target, fields, span names, "tick": current_tick()
pub fn init_headless(cfg: &DebugConfig, run: Option<&RunDir>) -> &'static Profiler;   // EnvFilter + stderr fmt + json file (if run) + profiler layer; idempotent
pub fn global_run_dir() -> Option<&'static RunDir>;  pub fn set_global_run_dir(run: RunDir);   // lets fn-pointer layer factories (Bevy LogPlugin) find the run dir
pub fn global_profiler() -> &'static Profiler;
pub struct Profiler { … }
impl Profiler {
    pub fn layer<S: Subscriber + for<'a> LookupSpan<'a>>(&'static self) -> impl Layer<S>;  // times `system` spans (bevy_ecs trace) by `name` field
    pub fn end_tick(&self, tick: u32, sets: &[(&'static str, Duration)], counters: &[(&'static str, u64)]);  // folds the tick, checks budgets → warn!(target: "fl_observe::budget", …)
    pub fn set_budgets(&self, b: Budgets);
    pub fn snapshot(&self) -> PerfSnapshot;
    pub fn reset(&self);
}
#[derive(Clone, Debug, Serialize)] pub struct Stat { pub mean_ms: f64, pub p50_ms: f64, pub p95_ms: f64, pub max_ms: f64, pub samples: u64 }
#[derive(Clone, Debug, Serialize)] pub struct PerfSnapshot { pub ticks: u64, pub tick: Stat, pub sets: Vec<(String, Stat)>, pub features: Vec<(String, Stat)>,
    pub systems: Vec<SystemStat>, pub counters: Vec<(String, u64)>, pub history_ms: Vec<f32> /* last 240 ticks, for sparklines */ }
#[derive(Clone, Debug, Serialize)] pub struct SystemStat { pub name: String, pub feature: String, pub stat: Stat, pub history_ms: Vec<f32> }
pub fn feature_of(system_name: &str) -> String;   // "fl_feat_artillery::fire::x" → "artillery"; "fl_sim::movement::move_system" → "core.movement"; other → crate name
pub struct JournalSink { … }   // fn create(run: &RunDir) -> io::Result<Self>; fn write(&mut self, entries: &[JournalEntry]) -> io::Result<()>  (journal.jsonl, one JSON object per line)
```
- Rolling statistics use a fixed window of the last 240 ticks per series, with nearest-rank percentiles.
- System spans that fire outside a sim tick (client systems) are attributed by the same `feature_of`. Client systems therefore show up under `fl_client` and `bevy_*`, which is useful for the client panel.

- [ ] **Step 1: Write the failing tests** (`tests/profiler.rs` plus unit tests).
  - `feature_of_maps_paths`: covers the cases in the interface comment.
  - `profiler_times_system_spans`: build a sim app with `SimConfig.profile = true` and a scoped subscriber with `Profiler::layer` (use a fresh non-global `Profiler` via `Profiler::new()`). Run 30 ticks, calling `end_tick` each tick. The snapshot has `fl_sim::movement::move_system` with samples ≥ 30 and feature `core.movement`.
  - `budget_overrun_warns`: `tick_ms: 0.0` gives a captured `fl_observe::budget` warning.
  - `percentiles_nearest_rank`: [1..=100] gives p50 = 50 and p95 = 95.
  - `debug_config_defaults_and_rust_log_override`.
  - `json_layer_writes_tick_field`: a temp run dir under `target/`; log one event and read back a line with `"tick"`.
  - `journal_sink_writes_jsonl`.
- [ ] **Step 2: Run** `cargo test -p fl_observe`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test -p fl_observe`. Expected: PASS.
- [ ] **Step 5: Gate + commit** `feat(fl_observe): logging setup, sim profiler layer, journal sink, debug.ron`.

