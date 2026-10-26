### Task 15: `GameBuilder` and the `fl_game` testbed binary

**Files:** `crates/fl_client/src/game.rs` and `crates/fl_game/src/main.rs`, and no others.

**Interfaces — Produces:**
```rust
pub enum StartMode { Scenario(String), Stress { units: u32, seed: u64 } }
pub struct GameBuilder { … }
impl GameBuilder {
    pub fn new() -> Self;                                     // StartMode::Scenario("base.testbed")
    pub fn feature(self, f: impl GameFeature) -> Self;
    pub fn features(self, fs: Vec<Arc<dyn GameFeature>>) -> Self;
    pub fn content_mod(self, dir: impl Into<PathBuf>) -> Self;
    pub fn scenario(self, id: &str) -> Self;
    pub fn stress(self, units: u32) -> Self;                  // ScaleScenario { units, ..default } with wander
    pub fn exit_after(self, secs: Option<f32>) -> Self;
    pub fn prepare(self) -> Result<App, String>;              // everything except run(); content/setup errors as text
    pub fn run(self) -> AppExit;                              // prepare() then app.run(); on Err print every line, return AppExit::error()
}
```
**`prepare`:**
1. `load_content`.
2. `build_sim_app(features, content, SimConfig { seed, hash_interval: None, allow_debug_commands: true })`.
3. `setup_scenario` or `ScaleScenario::populate`.
4. `SimHost::new(take world)`.
5. Build the client `App`:
   - `DefaultPlugins`, with window title `"Frontline — testbed"` and `present_mode: AutoNoVsync` in stress mode so FPS is measurable.
   - `MeshPickingPlugin`, `ClientPlugins`, and every `feature.client(&mut app)`.
   - Insert `SimHost`, `ClientContent`, `LocalPlayer(PlayerId(0))`, and `MapView` built from the sim's `MapInfo` and the map's ground colour.
6. Observability wiring (spec §15, uses T19's `fl_observe`):
   - Load `DebugConfig` (`debug.ron` / `RUST_LOG`).
   - In debug builds, create the run dir `logs/<run-id>/` and `fl_observe::set_global_run_dir` it before building the App.
   - Set `LogPlugin { filter, custom_layer: client_log_layers, .. }`. `client_log_layers(&mut App) -> Option<BoxedLayer>` is defined in `game.rs`. It boxes `global_profiler().layer()`, plus `json_file_layer(global_run_dir())` when present.
   - Add a `PostUpdate` system that appends new `SimHost::journal()` entries to `journal.jsonl` through `fl_observe::JournalSink`, and calls `global_profiler().end_tick(..)` once per sim tick.
   - `GameBuilder::debug_remote(bool)` and the `--remote` flag are stored for T21.

**`exit_after`:** after the duration, write `AppExit::Success` and log this line to stdout, ignoring the first 3 s:

`stress: units={n} avg_fps={:.1} p1_low_fps={:.1} frames={}`

**`fl_game` CLI (clap):**
- `--scenario <ID>`, default `base.testbed`.
- `--stress <N>`.
- `--exit-after <SECS>`.
- `--mods <DIR>...`, default `fl_catalog::base_mod_dir()`.

- [ ] **Step 1: Write the failing test** `builder_rejects_broken_content`: `GameBuilder::new().content_mod(<path to crates/fl_cli/tests/fixtures/bad_mod>).prepare()` is `Err` and its text names the bad file. This works without a window because content loads before the client App is built. Everything else is checked by running the binary in T17.
- [ ] **Step 2: Run** the test. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** the test. Expected: PASS. Then `cargo run -p fl_game` opens the window. Manual check: ground, 10 placeholders, pan/zoom/rotate.
- [ ] **Step 5: Gate + commit** `feat(fl_game): GameBuilder and testbed binary with stress mode`.

