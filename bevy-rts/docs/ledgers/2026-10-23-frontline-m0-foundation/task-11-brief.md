### Task 11: `fl_test_utils` — ScenarioBuilder and determinism twin-run

**Files:** `crates/fl_test_utils/src/{lib,builder,determinism}.rs` and `crates/fl_test_utils/tests/determinism.rs`.

**Interfaces — Produces:**
```rust
pub fn base_content() -> Arc<ContentDb>;            // OnceLock; fl_catalog features + base mod
pub struct ScenarioBuilder { … }
impl ScenarioBuilder {
    pub fn new() -> Self;                            // base_content(), seed 0, hash_interval 1, allow_debug_commands true
    pub fn with_content(content: Arc<ContentDb>, features: Vec<Arc<dyn GameFeature>>) -> Self;
    pub fn seed(self, seed: u64) -> Self;
    pub fn hash_interval(self, n: u32) -> Self;
    pub fn scenario(self, id: &str) -> Self;         // setup_scenario; its seed is then overridden by .seed() if given
    pub fn map_flat(self, width_m: i32, height_m: i32) -> Self;
    pub fn player(self, faction: &str, team: u8) -> Self;          // PlayerId in call order
    pub fn unit(self, unit: &str, player: u8, x: i32, y: i32) -> Self;   // builder units get SimIds after scenario units, in call order
    pub fn scale(self, s: ScaleScenario) -> Self;
    pub fn wander_all(self, radius_m: i32) -> Self;
    pub fn command<P: CommandPayload>(self, tick: u32, player: u8, units: &[u32], payload: P) -> Self;
    pub fn build(self) -> Scenario;                  // panics with a readable message on setup errors
}
pub struct Scenario { pub app: App }
impl Scenario {
    pub fn run_ticks(&mut self, n: u32);             // submits builder commands as their tick comes up
    pub fn tick(&self) -> Tick;
    pub fn hashes(&self) -> &[(Tick, u64)];
    pub fn world(&self) -> &World; pub fn world_mut(&mut self) -> &mut World;
    pub fn pose(&self, id: u32) -> Option<Pose>;
    pub fn issue<P: CommandPayload>(&mut self, player: u8, units: &[u32], payload: P, queue: bool);
}
pub fn assert_deterministic(make: impl Fn() -> ScenarioBuilder, ticks: u32);   // two runs; panics naming the first divergent tick and both hashes
```
- [ ] **Step 1: Write the failing tests** in `tests/determinism.rs`.
  - `testbed_twin_run_600_ticks`: base.testbed with `wander_all(100)`.
  - `scale_twin_run_500_units`: `ScaleScenario{units:500,..}` for 200 ticks.
  - `commands_mid_run_twin`: move commands at ticks 10, 50 and 90.
  - `different_seeds_diverge`: seeds 1 and 2 on a 500-unit scale scenario give different final hashes.
  - `builder_units_get_sequential_ids`.
  - `divergence_is_reported`: `#[should_panic(expected = "diverged at tick")]`. The builder reads a process-global counter so that one unit's position changes on the second run.
- [ ] **Step 2: Run** `cargo test -p fl_test_utils`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test -p fl_test_utils`. Expected: PASS.
- [ ] **Step 5: Gate + commit** `feat(fl_test_utils): ScenarioBuilder and determinism twin-run`.

