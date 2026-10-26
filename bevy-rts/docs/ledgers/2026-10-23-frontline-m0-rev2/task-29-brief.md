### Task 29: Work-counter perf gates — entity-mix scale scenario, counting allocator, budgets

**Track:** sim-core + perf · **Refs:** ADR-0010 · **Lane 1**

**Files:**
- Create:
  - `crates/fl_test_utils/src/perf.rs`, `crates/fl_test_utils/tests/perf_counters.rs`, `crates/fl_sim/tests/scale_mix.rs`
  - blessed: `docs/perf/budgets.ron` and `crates/fl_test_utils/tests/snapshots/perf_counters__*.snap`
- Modify: `crates/fl_sim/src/{scale,step,spawn,lib}.rs` (the mix; counter `sort.items` where the inbox and despawn requests are sorted), `crates/fl_test_utils/src/lib.rs`

**Interfaces:**
- Consumes: `SpawnRequest` (`unit: None` plus `init`), `DespawnRequest`, `SpatialGrid::query_radius`, `SimCounters`, `rolls`.
- Produces:
```rust
// fl_sim/src/scale.rs
pub struct ScaleScenario { pub units: u32, pub players: u8, pub map_m: i32, pub wander_m: i32, pub seed: u64,
    pub followers_per_unit: u32, pub projectiles: u32, pub projectile_ttl: u32, pub sensor_every: u32 }   // + Serialize/Deserialize (Task 18)
impl Default for ScaleScenario { /* units 3000, players 4, map 2000, wander 150, seed 1, followers 2, projectiles 1000, ttl 20, sensor_every 10 */ }
impl ScaleScenario { pub fn with_units(units: u32) -> Self; }   // the same mix scaled: followers 2 per unit, projectiles = units / 3
#[derive(Component, Clone, Copy, Serialize, Deserialize)] pub struct Follower { pub leader: SimId, pub offset: FxVec2 }   // Hashed "core.follower"
#[derive(Component, Clone, Copy, Serialize, Deserialize)] pub struct Projectile { pub vel: FxVec2, pub ttl: u16 }        // Hashed "core.projectile" (stand-in until M2)
#[derive(Resource, Clone)] pub struct ScaleMix { .. }    // External "core.scale_mix"; present only in scale runs
// follow_system   (ActSet::Move, .after(move_system)): pos = map.clamp(leader pos + offset)
// scale_fire      (ActSet::Fire): ceil(projectiles / ttl) SpawnRequests per tick; shooters picked by rolls(seed, tick, SimId(0), stream_key("scale.fire"))
// projectile_step (ResolveSet::Projectiles): pos += vel; ttl -= 1; at 0 → DespawnRequest
// scale_sense     (PerceiveSet::Vision): units with sim_id % sensor_every == tick % sensor_every run query_radius(pos, 150 m) → "spatial.query_candidates"
// fl_test_utils/src/perf.rs
#[derive(Deserialize, Serialize)] pub struct BudgetFile { pub scenarios: BTreeMap<String, BTreeMap<String, u64>> }   // max per tick
pub fn load_budgets(path: &Path) -> BudgetFile;
pub struct CounterRun { pub per_tick_max: BTreeMap<String, u64>, pub totals: BTreeMap<String, u64>, pub entities: usize }
pub fn run_counter_scenario(units: u32, warmup: u32, ticks: u32, alloc: Option<&'static stats_alloc::StatsAlloc<std::alloc::System>>) -> CounterRun;
//   adds "alloc.count" and "alloc.bytes" per tick when `alloc` is given
pub fn check_budgets(scenario: &str, run: &CounterRun, budgets: &BudgetFile) -> Result<(), Vec<String>>;   // "scale_500: spawn.entities max 31 > budget 25"
pub fn record_budgets(scenario: &str, run: &CounterRun, path: &Path);   // writes <path>.new with ceil(max × 1.25)
```
- The counting allocator lives in `tests/perf_counters.rs` as `#[global_allocator] static GLOBAL: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;`. That is safe code, so the workspace keeps `unsafe_code = "forbid"`.
- `stats_alloc` must pass `cargo deny`.
- Alloc counts are budgeted, never snapshotted.
- With `FL_RECORD_BUDGETS=1`, the tests write `docs/perf/budgets.ron.new` for the lead to bless.

- [ ] **Step 1: Write the failing tests:**
```rust
// crates/fl_sim/tests/scale_mix.rs
#[test] fn scale_mix_entity_count()        // with_units(3000) after a 40-tick warmup: SimIndex.len() in [8_000, 12_000]
#[test] fn projectiles_steady_state()      // with_units(3000): projectiles in flight after warmup within ±10% of 1000
#[test] fn followers_track_leaders()       // with_units(100): every tick, follower pos == map.clamp(leader pos + offset)
#[test] fn scale_mix_twin_run()            // with_units(500), 200 ticks: equal hash streams
// crates/fl_test_utils/tests/perf_counters.rs
#[test] fn smoke_counters_scale_500()      // warmup 20, 200 ticks: check_budgets("scale_500") Ok; insta snapshot of totals without alloc.*
#[test] fn counters_scale_3000()           // the same at 3000 (gate pr)
#[test] fn milestone_counters_scale_5000() // the same at 5000 (gate milestone)
#[test] fn budget_violation_names_counter()   // synthetic run with spawn.entities max 31 vs budget 25 → Err(["scale_500: spawn.entities max 31 > budget 25"])
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_sim --test scale_mix`. Expected: FAIL.
- [ ] **Step 3: Implement.** Then run `FL_RECORD_BUDGETS=1 cargo nextest run --profile agent -p fl_test_utils --test perf_counters -E 'test(/scale/)'` and ask the lead to bless `docs/perf/budgets.ron` and the snapshots, with the reason `M0 counter baseline (entity mix)`.
- [ ] **Step 4: Run** `cargo nextest run --profile agent -p fl_sim -p fl_test_utils`. Expected: PASS. Also run a temporary O(n²) test system (every unit queries all units each tick) and expect `smoke_counters_scale_500` to fail naming `spatial.query_candidates`. Remove the system afterwards.
- [ ] **Step 5: Gate** `cargo xtask gate task --track sim-core --track perf`. Expected: `smoke_counters_scale_500` runs, and `gate task: PASS`.
- [ ] **Step 6: Commit** `feat(perf): entity-mix scale scenario, work-counter budgets and counting allocator` with trailers `Refs: ADR-0010` and `Blessed: M0 counter baseline (entity mix)`.

---

## Phase IV — integration (lane 1)

