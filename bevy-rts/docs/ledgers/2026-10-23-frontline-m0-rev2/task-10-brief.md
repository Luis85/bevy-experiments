### Task 10: `fl_sim` — eight phases, `step_tick`, keyed RNG, name-keyed state hash, counters

**Track:** contract + sim-core · **Refs:** ADR-0005, ADR-0003, ADR-0004

**Files:**
- Create: `crates/fl_sim/src/{step,counters}.rs`, and `crates/fl_sim/tests/fixtures/core_mod_extra/`: a copy of `core_mod` plus `units/aaa.unit.ron` with id `core_test.aaa_first`, which sorts first.
- Modify: `crates/fl_sim/src/{schedule,hash,rng,clock,events,builder,build,movement,scale,commands,units,inspect,lib}.rs`
- Modify: `xtask/clippy-template.toml` (drop the `Pcg32` pending entry), then regenerate `crates/fl_sim/clippy.toml`
- Rewrite: `crates/fl_sim/tests/{schedule,hash}.rs`. Update `tests/movement.rs`.

**Interfaces:**
- Consumes: Task 9; `fl_math::{Rolls, stream_key}`; `fl_view::{TickInput, TickOutput}`.
- Produces:
```rust
// schedule.rs
#[derive(ScheduleLabel, Debug, Clone, PartialEq, Eq, Hash)] pub struct SimTick;
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Phase { Input, Think, Act, Resolve, Economy, Perceive, Lifecycle, Publish }
pub enum InputSet { Housekeeping, Validate, Apply }     pub enum ThinkSet { Orders, Behaviour }     pub enum ActSet { Move, Fire }
pub enum ResolveSet { Projectiles, Effects, Stats }     pub enum EconomySet { Main }                pub enum PerceiveSet { Spatial, Vision }
pub enum LifecycleSet { Death, Spawn, Despawn }         pub enum PublishSet { Snapshot, Events }   // all derive SystemSet + Copy
pub trait SimHook: SystemSet + Copy { fn phase(self) -> Phase; }   // implemented by exactly these eight enums
pub fn configure_sim_schedule(app: &mut App, shuffle_seed: Option<u64>);
//   phases chained; hooks chained inside their phase; one ApplyDeferred system ends each phase;
//   ScheduleBuildSettings { ambiguity_detection: LogLevel::Error, auto_insert_apply_deferred: false, shuffle_seed, .. }; SingleThreadedExecutor
pub fn system_order(app: &mut App) -> String;   // initializes SimTick; one line "<Phase>/<Hook> <system name>" per system, in executed order
// builder.rs (changed)
//   add_systems<M>(&mut self, hook: impl SimHook, systems: impl IntoScheduleConfigs<ScheduleSystem, M>) -> &mut Self
//   add_message<M: Message>(&mut self) -> &mut Self   // rotated once per tick by `rotate_messages` in InputSet::Housekeeping; classified Transient
// step.rs
pub fn step_tick(world: &mut World, input: TickInput) -> TickOutput;
//   panics "driver out of sync: input tick {a}, sim tick {b}" unless input.tick == SimClock.tick
//   1 sort commands by (player, seq) into the inbox (the rev-1 CommandQueue until Task 11)   2 run SimTick
//   3 hash = Some(compute_state_hash) if HashInterval > 0 and tick % HashInterval == 0   4 outputs (snapshot and events: Task 12; journal: Task 18)
//   5 world.clear_trackers()   6 SimClock.tick += 1
#[derive(Resource)] pub struct HashInterval(pub u32);   // External "core.hash_interval"; from RunConfig.hash_interval, else rules StateHash.interval_ticks
// rng.rs  (SimRng and SIM_RNG_STREAM are deleted)
#[derive(Resource, Clone, Copy, Debug, Serialize, Deserialize)] pub struct MatchSeed(pub u64);   // Hashed "core.match_seed", from the header
pub fn rolls(seed: &MatchSeed, tick: Tick, sim_id: SimId, stream_key: u64) -> Rolls;
// counters.rs
#[derive(Resource, Default)] pub struct SimCounters { .. }   // Transient "core.counters"; cleared in InputSet::Housekeeping
impl SimCounters { pub fn add(&mut self, name: &'static str, n: u64); pub fn get(&self, name: &str) -> u64; pub fn snapshot(&self) -> Vec<(&'static str, u64)>; }   // sorted by name
// hash.rs
impl StateHasher { pub fn write_registry_id(&mut self, table: &'static str, id: u32); }   // writes the name key, looked up in the hasher's table context
pub fn compute_state_hash(world: &World) -> u64;
//   the tick; each Hashed resource in name order (name key, then data); the entity count; per SimIndex entry: SimId, then each Hashed component in name order (0, or 1 + data)
// RunConfig gains: pub shuffle_seed: Option<u64>
```
- **System placement:**
  - `apply_commands` → `InputSet::Apply`.
  - `wander_system` → `ThinkSet::Behaviour`. It is entity-local, so any iteration order is allowed, and it draws `rolls(seed, tick, sim_id, stream_key("debug.wander"))`.
  - `move_system` → `ActSet::Move` (entity-local).
  - The `SimEvents` and `SimCounters` clears → `InputSet::Housekeeping`.
- `ScaleScenario::populate` draws positions from `Rolls::new(seed, 0, 0, stream_key("setup.scale"))`, x then y per unit, and inserts `MatchSeed(seed)`.
- `UnitType` hashes through `write_registry_id(tables::UNITS, id)`.

- [ ] **Step 1: Write the failing tests:**
```rust
// tests/schedule.rs
#[test] fn hooks_run_in_declared_order()          // one tracing system per hook (18) → trace == ["Input/Housekeeping", "Input/Validate", …, "Publish/Events"]
#[test] fn clock_advances_per_step()              // 3 × step_tick → SimClock.tick == Tick(3)
#[test] #[should_panic(expected = "driver out of sync")] fn input_tick_must_match_clock()
#[test] #[should_panic] fn ambiguous_systems_fail_schedule_build()   // two unordered writers of one resource in ThinkSet::Orders
#[test] fn commands_visible_only_after_phase_sync()   // ThinkSet::Orders inserts Marker via Commands; ThinkSet::Behaviour sees 0; ActSet::Move sees 1
#[test] fn messages_rotate_once_per_tick()        // written in Think and read in Lifecycle of the same tick (1 read); Messages<T>::len() == 0 two steps later
#[test] fn clear_trackers_runs_each_step()        // despawn in tick t; after step t+1 the RemovedComponents<SimEntity> buffer is empty
#[test] fn counters_reset_each_tick()             // a system adds 5 to "test.n" each tick → get("test.n") == 5 after 3 steps
#[test] fn system_order_lists_core_systems()      // has an "Input/Apply " line naming apply_commands and an "Act/Move " line naming move_system
// tests/hash.rs
#[test] fn equal_worlds_hash_equal()  #[test] fn component_change_changes_hash()  #[test] fn tick_changes_hash()
#[test] fn seed_changes_hash()
#[test] fn hash_every_interval()                  // interval 5, 20 steps → Some at ticks [0, 5, 10, 15], None elsewhere
#[test] fn resources_are_hashed()                 // different Players → different hashes
#[test] fn external_resources_not_hashed()        // HashInterval 1 vs 2 → equal tick-0 hashes
#[test] fn unit_type_hashes_by_name_not_dense_id()   // core_mod vs core_mod_extra, the same tank spawned (dense ids 0 vs 1) → equal hashes
// ported: hash_independent_of_spawn_order, sim_id_allocator_changes_hash
// tests/movement.rs
#[test] fn adding_a_stream_does_not_move_wander() // an extra feature registers "aaa.noise" and draws it every tick → all poses after 200 ticks equal the run without it
// ported to Rolls: wander_stays_within_radius_and_map, scale_populate_is_deterministic
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_sim`. Expected: FAIL.
- [ ] **Step 3: Implement.** Drop the `Pcg32` pending entry and regenerate (`cargo xtask gen-clippy`).
- [ ] **Step 4: Run** the tests. Expected: PASS, and `cargo clippy -p fl_sim --all-targets -- -D warnings` is clean with the `Pcg32` ban active.
- [ ] **Step 5: Gate** `cargo xtask gate task --track contract --track sim-core`. Expected: PASS.
- [ ] **Step 6: Commit** `feat(fl_sim)!: eight phases with hook sub-sets, step_tick driver steps, keyed RNG, name-keyed state hash, counters` with trailer `Refs: ADR-0005, ADR-0003, ADR-0004`.

