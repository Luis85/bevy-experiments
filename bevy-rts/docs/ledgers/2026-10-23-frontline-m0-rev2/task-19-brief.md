### Task 19: `fl_observe` — logging setup, profiler layer, journal sink, `debug.ron` (rev-1 T19)

**Track:** tools · **Lane 2**

**Files:**
- Create: `crates/fl_observe/src/{lib,config,logging,profiler,sink}.rs`, `crates/fl_observe/tests/{profiler,profiler_sim}.rs`, `debug.ron`

**Interfaces:**
- Consumes: `JournalEntry` (Task 7). Its dev-dependencies use `fl_sim` with the `core_mod` fixture by path.
- Produces (as rev-1 Task 19, with these changes):
```rust
#[derive(Clone, Debug, Deserialize, Serialize)] #[serde(deny_unknown_fields, default)]
pub struct DebugConfig { pub log_filter: String /* "info,fl_sim=debug" */, pub json_log: bool, pub journal_file: bool, pub profiler: bool, pub budgets: Budgets }
#[derive(Clone, Debug, Default, Deserialize, Serialize)] #[serde(deny_unknown_fields, default)]
pub struct Budgets { pub tick_ms: f64 /* 15.0 */, pub per_phase_ms: BTreeMap<String, f64>, pub per_feature_ms: BTreeMap<String, f64> }   // warn only
pub fn load_debug_config(path: Option<&Path>) -> DebugConfig;     // explicit path, else ./debug.ron, else defaults; RUST_LOG overrides log_filter
pub struct RunDir { pub id: String, pub path: PathBuf }
pub fn new_run_dir(root: &Path) -> std::io::Result<RunDir>;       // root/<YYYYMMDD-HHMMSS>-<pid>/ plus root/latest.txt
pub fn json_file_layer<S>(run: &RunDir) -> impl Layer<S>;          // log.jsonl: timestamp, level, target, fields, and the span fields (so "tick" comes from the sim_tick span)
pub fn init_headless(cfg: &DebugConfig, run: Option<&RunDir>) -> &'static Profiler;   // EnvFilter + stderr fmt + json file + profiler; idempotent
pub fn set_global_run_dir(run: RunDir); pub fn global_run_dir() -> Option<&'static RunDir>; pub fn global_profiler() -> &'static Profiler;
pub struct Profiler { .. }
impl Profiler {
    pub fn new() -> Profiler;
    pub fn layer(&'static self) -> ProfilerLayer;   // times bevy_ecs "system" spans by name, on any thread; times phases from fl_sim::phase start/end events;
                                                    //   folds one tick when a "sim_tick" span closes; client frames fold on the "frame" span
    pub fn set_budgets(&self, b: Budgets);
    pub fn snapshot(&self) -> PerfSnapshot;
    pub fn reset(&self);
}
#[derive(Clone, Debug, Serialize)] pub struct Stat { pub mean_ms: f64, pub p50_ms: f64, pub p95_ms: f64, pub max_ms: f64, pub samples: u64 }
#[derive(Clone, Debug, Serialize)] pub struct PerfSnapshot { pub ticks: u64, pub tick: Stat, pub phases: Vec<(String, Stat)>, pub features: Vec<(String, Stat)>,
    pub systems: Vec<SystemStat>, pub history_ms: Vec<f32> /* last 240 ticks */ }
#[derive(Clone, Debug, Serialize)] pub struct SystemStat { pub name: String, pub feature: String, pub stat: Stat, pub history_ms: Vec<f32> }
pub fn feature_of(system_name: &str) -> String;
pub struct JournalSink { .. }   impl JournalSink { pub fn create(run: &RunDir) -> std::io::Result<Self>; pub fn write(&mut self, entries: &[JournalEntry]) -> std::io::Result<()>; }
```
- Rolling statistics use a 240-tick window with nearest-rank percentiles.
- `Instant` here sits behind `#[expect(clippy::disallowed_types, reason = "observe-only wall clock")]`.
- `debug.ron` holds the defaults above plus per-phase budgets of 3.0 ms each.

- [ ] **Step 1: Write the failing tests:**
```rust
#[test] fn feature_of_maps_paths()
//   "fl_feat_artillery::fire::x" → "artillery"; "fl_feat_infantry_client::ui::y" → "infantry (client)"; "fl_sim::movement::move_system" → "core.movement";
//   "fl_client::view::interpolate_views" → "client.view"; "bevy_render::view::x" → "bevy"; "other::a" → "other"
#[test] fn phase_times_from_marker_events()    // synthetic start/end events for 8 phases plus a sim_tick span, 30 ticks → snapshot.phases.len() == 8, each with samples == 30
#[test] fn system_spans_on_another_thread()    // synthetic "system" spans emitted on a spawned thread → attributed and counted
#[test] fn budget_overrun_warns()              // tick_ms 0.0 → a captured warning on target fl_observe::budget
#[test] fn percentiles_nearest_rank()          // [1..=100] → p50 == 50, p95 == 95
#[test] fn debug_config_defaults_and_rust_log_override()
#[test] fn json_layer_writes_tick_field()      // an event inside a "sim_tick" span with tick 3 → a log.jsonl line containing "tick":3
#[test] fn journal_sink_writes_jsonl()
// tests/profiler_sim.rs (its own process, global subscriber)
#[test] fn profiler_times_real_sim_systems()   // core_mod duel with RunConfig { phase_markers: true, .. }, threaded runner, 30 ticks →
                                               //   snapshot has system "fl_sim::movement::move_system" with feature "core.movement", samples ≥ 30
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_observe`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** the tests. Expected: PASS.
- [ ] **Step 5: Gate** `cargo xtask gate task --track tools`. Expected: PASS.
- [ ] **Step 6: Commit** `feat(fl_observe): logging setup, profiler layer (both threads, phase markers), journal sink, debug.ron`.

