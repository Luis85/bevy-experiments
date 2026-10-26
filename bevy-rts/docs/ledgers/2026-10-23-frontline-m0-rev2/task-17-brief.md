### Task 17: `fl_test_utils` — ScenarioBuilder, twin-run, shuffle runs, save fitness helpers (rev-1 T11)

**Track:** sim-core · **Refs:** ADR-0005, ADR-0006 · **Lane 1**

**Files:**
- Create: `crates/fl_test_utils/src/{lib,builder,determinism,fitness}.rs`, `crates/fl_test_utils/tests/determinism.rs`

**Interfaces:**
- Consumes: Tasks 9–14.
- Produces:
```rust
pub fn base_content() -> Arc<ContentDb>;                 // OnceLock; fl_catalog features + base mod
pub struct ScenarioBuilder { .. }
impl ScenarioBuilder {
    pub fn new() -> Self;                                // base_content(), catalog features, seed 0, hash interval 1, cheats true, no shuffle
    pub fn with_content(content: Arc<ContentDb>, features: Vec<Arc<dyn SimFeature>>) -> Self;
    pub fn feature(self, f: Arc<dyn SimFeature>) -> Self;
    pub fn seed(self, seed: u64) -> Self;                // overrides the scenario seed
    pub fn cheats(self, on: bool) -> Self;
    pub fn hash_interval(self, n: u32) -> Self;
    pub fn shuffle_seed(self, seed: Option<u64>) -> Self;
    pub fn scenario(self, id: &str) -> Self;
    pub fn map_flat(self, width_m: i32, height_m: i32) -> Self;
    pub fn player(self, faction: &str, team: u8) -> Self;          // PlayerId in call order
    pub fn unit(self, unit: &str, player: u8, x: i32, y: i32) -> Self;   // builder units get SimIds after scenario units, in call order
    pub fn scale(self, s: ScaleScenario) -> Self;
    pub fn wander_all(self, radius_m: i32) -> Self;                // a debug.wander per player at tick 0
    pub fn command<P: CommandPayload>(self, tick: u32, player: u8, units: &[u32], payload: P) -> Self;   // issued through the TestDriver to apply at `tick`
    pub fn build(self) -> Scenario;                      // panics with the setup error's text
}
pub struct Scenario { pub runner: SimRunner, pub driver: TestDriver, .. }
impl Scenario {
    pub fn run_ticks(&mut self, n: u32);
    pub fn tick(&self) -> Tick;
    pub fn hashes(&self) -> Vec<(Tick, u64)>;
    pub fn last_output(&self) -> Option<&TickOutput>;
    pub fn world(&self) -> &World;
    pub fn pose(&self, id: u32) -> Option<Pose>;
    pub fn issue<P: CommandPayload>(&mut self, player: u8, units: &[u32], payload: P, queue: bool);
    pub fn save(&mut self) -> Vec<u8>;                   // SimQuery::Save with the driver's pending commands
    pub fn load(features: Vec<Arc<dyn SimFeature>>, content: Arc<ContentDb>, bytes: &[u8]) -> Scenario;   // restores the driver from Loaded
}
pub fn assert_deterministic(make: impl Fn() -> ScenarioBuilder, ticks: u32);   // twin run; panics "diverged at tick T: a=0x… b=0x…"
pub fn shuffle_seeds() -> Vec<u64>;   // FL_SHUFFLE_SEEDS=k (default 1) → the first k of [0x5EED_0001, 0x5EED_0002, 0x5EED_0003]
pub fn assert_shuffle_invariant(make: impl Fn() -> ScenarioBuilder, ticks: u32);   // every shuffle_seeds() run == the unshuffled hash stream
pub fn assert_save_round_trip(s: &mut Scenario);                                   // save(load(save)) == save, byte for byte
pub fn assert_branch(make: impl Fn() -> ScenarioBuilder, n: u32, m: u32);         // run A to m; B to n, save, load, run to m; hashes equal for n..m
```
- [ ] **Step 1: Write the failing tests** (`tests/determinism.rs`; `smoke_` tests run in every `gate task`):
```rust
#[test] fn smoke_testbed_twin_run()        // base.testbed + wander_all(100), 600 ticks
#[test] fn smoke_shuffle_invariant()       // base.testbed + wander_all(100), 200 ticks, shuffle_seeds()
#[test] fn smoke_save_round_trip()         // base.testbed + wander, at tick 50
#[test] fn smoke_branch()                  // base.testbed + wander + moves at 10 and 60, n = 40, m = 120
#[test] fn scale_twin_run_500_units()      // ScaleScenario { units: 500, .. }, 200 ticks
#[test] fn commands_mid_run_twin()         // moves at ticks 10, 50 and 90
#[test] fn different_seeds_diverge()       // seeds 1 vs 2, 500-unit scale → different final hashes
#[test] fn builder_units_get_sequential_ids()
#[test] #[should_panic(expected = "diverged at tick")] fn divergence_is_reported()   // the builder reads a process-global counter so the second run moves one unit
#[test] fn golden_stability_extra_feature()
//   a dummy feature registers command "aaa.noop", Hashed component "aaa.marker" (never inserted) and stream "aaa.stream" (drawn every tick)
//   → the base.testbed + wander hash stream over 300 ticks equals the run without it
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_test_utils`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `FL_SHUFFLE_SEEDS=3 cargo nextest run --profile agent -p fl_test_utils`. Expected: PASS.
- [ ] **Step 5: Gate** `cargo xtask gate task --track sim-core`. Expected: four `smoke_*` tests run in `test-headless`, and `gate task: PASS`.
- [ ] **Step 6: Commit** `feat(fl_test_utils): ScenarioBuilder, twin-run, shuffle runs, save round-trip and branch helpers`.

