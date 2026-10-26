### Task 18: Sim observability core — journal, counters, set timings, replay (`fl_sim::observe`, `fl_sim::replay`)

Runs in Phase A after T7, in parallel with T8; files are disjoint.

**Files:**
- Create `crates/fl_sim/src/{observe,replay}.rs` and `crates/fl_sim/tests/{observe,replay}.rs`.
- Modify `crates/fl_sim/src/{lib,app_ext,build,commands,spawn}.rs` (module declarations, registration, recording calls, config) and `crates/fl_sim/Cargo.toml` (add `serde_json` if T6 didn't).

**Interfaces — Produces:**
```rust
// observe.rs
pub trait JournalRecord: Serialize + std::fmt::Debug + Send + Sync + 'static { const KIND: &'static str; }   // e.g. "core.command_applied"
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct JournalEntry { pub tick: Tick, pub kind: String, pub sim_id: Option<SimId>, pub player: Option<PlayerId>, pub data: serde_json::Value }
#[derive(Resource)] pub struct Journal { /* enabled: bool, entries: Vec<JournalEntry>, cap: usize = 1_000_000 */ }
impl Journal {
    pub fn enabled(&self) -> bool;
    pub fn record<R: JournalRecord>(&mut self, tick: Tick, sim_id: Option<SimId>, player: Option<PlayerId>, rec: &R);  // no-op when disabled; debug_assert kind registered
    pub fn drain(&mut self) -> Vec<JournalEntry>;      // host takes everything recorded so far
    pub fn len(&self) -> usize;
}
#[derive(Resource, Default)] pub struct SimCounters { /* BTreeMap<&'static str, u64>, reset at tick start */ }   // fn add(&mut self, name: &'static str, n: u64); fn snapshot(&self) -> Vec<(&'static str, u64)>
#[derive(Resource, Default)] pub struct SetTimings { /* last tick: [(SimSet name, Duration); 9] */ }            // filled by marker systems when SimConfig.profile
pub fn current_tick() -> u32;   // static AtomicU32 set by the first ApplyCommands system; read by log layers only
// SimAppExt gains: fn register_journal_record<R: JournalRecord>(&mut self) -> &mut Self;
// build: Journal, SimCounters and SetTimings are registered with app.allow_ambiguous_resource::<_>() so unordered systems may write them
// SimConfig gains: pub journal: Option<bool> (None = cfg!(debug_assertions)), pub profile: bool
// Core records: CommandApplied { command: String, units: Vec<SimId>, queue: bool }, CommandRejected { command: String, reason: String },
//               UnitSpawned { unit: String, pos: FxVec2 }, UnitDespawned {}, MoveOrderChanged { waypoints: usize }
// replay.rs
#[derive(Clone, Debug, Serialize, Deserialize)] pub enum ReplayStart { Scenario(String), Scale(ScaleScenario) }   // ScaleScenario derives Serialize/Deserialize
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Replay { pub version: u32, pub rules_hash: u64, pub seed: u64, pub start: ReplayStart, pub ticks: u32, pub commands: Vec<PlayerCommand>, pub hashes: Vec<(Tick, u64)> }
impl Replay {
    pub fn capture(world: &World, start: ReplayStart, seed: u64) -> Replay;                 // from CommandLog.applied + StateHashLog
    pub fn save_json(&self, path: &Path) -> std::io::Result<()>; pub fn load_json(path: &Path) -> std::io::Result<Replay>;
    pub fn build_world(&self, features: &[Arc<dyn GameFeature>], content: Arc<ContentDb>, hash_interval: u32) -> Result<App, ReplayError>;
}
pub fn verify_replay(r: &Replay, features: &[Arc<dyn GameFeature>], content: Arc<ContentDb>) -> Result<(), Divergence>;  // re-run; compare every recorded hash
#[derive(Debug, Clone, PartialEq)] pub struct Divergence { pub tick: Tick, pub expected: u64, pub actual: u64 }
pub fn first_divergence(a: &[(Tick, u64)], b: &[(Tick, u64)]) -> Option<Divergence>;
```
- [ ] **Step 1: Write the failing tests.**
  - `tests/observe.rs`:
    - `applied_and_rejected_commands_are_journaled`: one move plus one foreign-unit move gives exactly one `core.command_applied` and one `core.command_rejected` with reason `NoValidUnits`.
    - `journal_disabled_records_nothing`.
    - `journal_does_not_change_state_hash`: journal on and off give the same final hash.
    - `counters_reset_each_tick`.
    - `set_timings_cover_all_nine_sets_when_profiling`.
    - `unordered_systems_may_write_journal`: two unordered test systems record without an ambiguity panic.
    - `current_tick_tracks_clock`.
  - `tests/replay.rs`:
    - `replay_roundtrip_verifies`: run the duel with moves, capture, save to JSON, load, and `verify_replay` is Ok.
    - `tampered_replay_reports_divergence`: change one command's target, and the result is `Err(Divergence)` at the right tick.
    - `first_divergence_finds_earliest`.
- [ ] **Step 2: Run** `cargo test -p fl_sim --test observe --test replay`. Expected: FAIL.
- [ ] **Step 3: Implement.** The set-timing marker systems are added only when `SimConfig.profile` is set. They use `std::time::Instant` under a local `#[allow(clippy::disallowed_types)]` and write `SetTimings` only.
- [ ] **Step 4: Run** the same tests. Expected: PASS.
- [ ] **Step 5: Gate + commit** `feat(fl_sim): event journal, counters, set timings, replays`.

