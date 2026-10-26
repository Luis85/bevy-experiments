### Task 22: `GameBuilder` and the `fl_game` testbed binary (rev-1 T15)

**Track:** client · **Refs:** ADR-0001, ADR-0002 · **Lane 2**

**Files:**
- Create: `crates/fl_game/src/{lib,builder}.rs`, `crates/fl_game/tests/{builder,wiring}.rs`
- Modify: `crates/fl_game/src/main.rs`, `crates/fl_client/src/lib.rs` (`ClientPlugins` gets the five plugins and exports the new modules)

**Interfaces:**
- Consumes: `fl_catalog`, `fl_sim` (`load_content`, `build_sim_app`, `MatchSetup`, `setup_scenario`, `ScaleScenario`, `SimRunner::spawn_threaded`), `fl_client` (Tasks 20–21), `fl_observe` (Task 19).
- Produces:
```rust
pub enum StartMode { Scenario(String), Stress { units: u32, seed: u64 } }
pub struct GameBuilder { .. }
impl GameBuilder {
    pub fn new() -> Self;                                  // Scenario("base.testbed"), fl_catalog::features(), base mod, cheats on
    pub fn sim_features(self, fs: Vec<Arc<dyn SimFeature>>) -> Self;
    pub fn client_feature(self, f: impl ClientFeature) -> Self;
    pub fn content_mod(self, dir: impl Into<PathBuf>) -> Self;
    pub fn scenario(self, id: &str) -> Self;
    pub fn stress(self, units: u32) -> Self;               // ScaleScenario { units, ..default } with wander
    pub fn exit_after(self, secs: Option<f32>) -> Self;
    pub fn debug_remote(self, port: Option<u16>) -> Self;  // stored; Task 25 consumes it
    pub fn agent_mode(self, on: bool) -> Self;             // continuous update mode when unfocused (benches and agents)
    pub fn prepare(self) -> Result<App, String>;           // everything except run(); content, feature and setup errors as text
    pub fn prepare_headless(self) -> Result<App, String>;  // the same with MinimalPlugins and no window (tests)
    pub fn run(self) -> AppExit;
}
pub fn check_client_features(sim: &[Arc<dyn SimFeature>], client: &[Arc<dyn ClientFeature>]) -> Result<(), String>;
//   "client feature for '<id>' has no SimFeature"
```
- **`prepare`:**
  1. `load_content`.
  2. `check_client_features`.
  3. `MatchSetup` (scenario seed or stress seed; `cheats_allowed = true` for the testbed).
  4. `build_sim_app` with `RunConfig { phase_markers: cfg.profiler, .. }`.
  5. `setup_scenario` or `ScaleScenario::populate`.
  6. `SimRunner::spawn_threaded`.
  7. Build the client `App`:
     - plugins: `DefaultPlugins` (title `"Frontline — testbed"`; `AutoNoVsync` in stress mode; continuous unfocused updates in agent mode), `MeshPickingPlugin`, `ClientPlugins`, then the client features in sorted feature order
     - resources: `RunnerLink`, `LocalDriver::new(runner.next_tick())`, `ClientContent`, `LocalPlayer(PlayerId(0))`, and `MapView` from the scenario's map
  8. Observability: `load_debug_config`; in debug builds, `new_run_dir("logs")` and `set_global_run_dir`. Set `LogPlugin { filter, custom_layer }`, where the custom layer boxes the profiler layer plus the json file layer. A `PostUpdate` system writes every drained `TickOutput.journal` through `JournalSink`.
- **`exit_after`** writes `AppExit::Success` after the duration and logs `stress: units={n} avg_fps={:.1} p1_low_fps={:.1} frames={}`, ignoring the first 3 s.
- **CLI:** `--scenario <ID>`, `--stress <N>`, `--exit-after <SECS>`, `--mods <DIR>...`, `--remote [--remote-port <P>]`, `--agent`.

- [ ] **Step 1: Write the failing tests:**
```rust
#[test] fn builder_rejects_broken_content()     // content_mod(crates/fl_cli/tests/fixtures/bad_mod).prepare_headless() → Err naming the bad file
#[test] fn orphan_client_feature_is_error()     // a ClientFeature with sim_feature "ghost" → Err("client feature for 'ghost' has no SimFeature")
#[test] fn testbed_visuals_appear_headless()    // prepare_headless() of base.testbed (threaded runner); update until 10 UnitView entities exist (≤ 200 updates, 10 ms sleeps)
#[test] fn debug_spawn_round_trips_through_runner()   // run_console_line("spawn base.de.pz3 0 500 500") → 11 UnitView within 200 updates
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_game`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** the tests. Expected: PASS. Then `cargo run -p fl_game` opens the window; check by hand that the ground, 10 placeholders and pan/zoom/rotate all work.
- [ ] **Step 5: Gate** `cargo xtask gate task --track client`. Expected: PASS.
- [ ] **Step 6: Commit** `feat(fl_game): GameBuilder wiring the threaded runner to the client; testbed binary with stress and agent modes`.

