### Task 18: Sim observability core — journal, phase markers, tick span, replays (rev-1 T18)

**Track:** sim-core (+ contract for `step.rs`, `builder.rs`) · **Refs:** ADR-0007, ADR-0004 · **Lane 1**

**Files:**
- Create:
  - `crates/fl_sim/src/{observe,replay}.rs`, `crates/fl_sim/tests/{observe,replay}.rs`
  - `crates/fl_test_utils/tests/golden.rs`
  - `crates/fl_test_utils/tests/replays/testbed_wander.replay`, produced as `.new` and blessed
- Modify: `crates/fl_sim/src/{step,builder,build,commands,spawn,scale,runner/mod,lib}.rs`, and `crates/fl_test_utils/src/builder.rs`, which gains `Scenario::journal(&self) -> &[JournalEntry]` (every entry of the run) and `Scenario::replay(&self) -> Replay` (`Scenario` records every input and output through a `ReplayRecorder`)

**Interfaces:**
- Consumes: `JournalEntry`, `TickInput`, `TickOutput` (Task 7), `MatchHeader::check_replay` (Task 6), `ReplayDriver` (Task 12).
- Produces:
```rust
// observe.rs
pub trait JournalRecord: Serialize + Debug + Send + Sync + 'static { const KIND: &'static str; }
#[derive(Resource)] pub struct Journal { .. }   // Transient "core.journal"; registered with allow_ambiguous_resource so unordered systems may record
impl Journal {
    pub fn enabled(&self) -> bool;
    pub fn record<R: JournalRecord>(&mut self, tick: Tick, phase: Phase, sim_id: Option<SimId>, player: Option<PlayerId>, rec: &R);
    //   no-op when disabled; debug-asserts R::KIND is registered
    pub fn len(&self) -> usize;
}
// SimBuilder::register_journal_record<R: JournalRecord>(&mut self) -> &mut Self   // stable-string registry, table JOURNAL_RECORDS
// RunConfig gains: pub journal: Option<bool> (None = cfg!(debug_assertions)), pub phase_markers: bool
// step_tick: runs SimTick inside tracing::info_span!("sim_tick", tick = n); drains the journal, stable-sorts by (phase, kind, sim_id, emitted) into TickOutput.journal
// phase markers (when RunConfig.phase_markers): the first and last system of each phase emit
//   tracing::trace!(target: "fl_sim::phase", phase = "<Phase>", edge = "start" | "end") — events only, no clock in the sim
// core records: "core.command_applied" { command, seq, units, queue }, "core.command_rejected" { command, seq, reason },
//               "core.unit_spawned" { unit, owner, pos }, "core.unit_despawned" {}, "core.move_order" { waypoints }
// runner: SimQuery::JournalSince { tick, limit } answered from a ring of the last 10_000 entries
// replay.rs
pub const REPLAY_FORMAT_VERSION: u32 = 1;
#[derive(Serialize, Deserialize)] pub enum ReplayStart { Scenario(String), Scale(ScaleScenario), Save(Vec<u8>) }   // ScaleScenario gains Serialize + Deserialize
#[derive(Serialize, Deserialize)] pub enum ReplayEvent { ContentReload { tick: Tick, rules_hash: u64 } }
#[derive(Serialize, Deserialize)] pub struct Replay { pub version: u32, pub header: MatchHeader, pub start: ReplayStart, pub hash_interval: u32,
    pub ticks: u32, pub inputs: Vec<TickInput> /* non-empty ticks only */, pub hashes: Vec<(Tick, u64)>, pub events: Vec<ReplayEvent> }
impl Replay {
    pub fn to_bytes(&self) -> Vec<u8>;                       // postcard
    pub fn from_bytes(bytes: &[u8]) -> Result<Replay, ReplayError>;
    pub fn to_json(&self) -> serde_json::Value;              // the readable view for `fl_cli replay dump --json`
    pub fn reproducible(&self) -> bool;                      // false if any ContentReload event
}
pub struct ReplayRecorder { .. }
impl ReplayRecorder { pub fn new(header: MatchHeader, start: ReplayStart, hash_interval: u32) -> Self; pub fn record(&mut self, input: &TickInput, output: &TickOutput); pub fn finish(self) -> Replay; }
pub enum VerifyOutcome { Verified { ticks: u32, warnings: Vec<String> }, Diverged(Divergence), NonReproducible }
#[derive(Debug, Clone, PartialEq)] pub struct Divergence { pub tick: Tick, pub expected: u64, pub actual: u64 }
pub fn verify_replay(r: &Replay, features: &[Arc<dyn SimFeature>], content: Arc<ContentDb>) -> Result<VerifyOutcome, ReplayError>;
pub fn first_divergence(a: &[(Tick, u64)], b: &[(Tick, u64)]) -> Option<Divergence>;
#[derive(Debug)] pub enum ReplayError { Decode(String), Version { found: u32 }, Header(HeaderMismatch), Setup(String) }
```
- **Golden replay.**
  - `golden.rs::golden_replays_verify` verifies every `tests/replays/*.replay` and expects `Verified`.
  - With `FL_RECORD_GOLDEN=1` it records base.testbed + `wander_all(100)` + moves at ticks 10 and 60, for 400 ticks, into `testbed_wander.replay.new`.
  - The lead blesses it with `cargo xtask bless crates/fl_test_utils/tests/replays --reason "M0 golden: testbed wander"`.

- [ ] **Step 1: Write the failing tests:**
```rust
// tests/observe.rs
#[test] fn applied_and_rejected_commands_are_journaled()   // one move + one foreign-unit move → exactly one "core.command_applied" and one "core.command_rejected" with reason "NoValidUnits"
#[test] fn journal_disabled_records_nothing()
#[test] fn journal_does_not_change_state_hash()           // journal on vs off → equal hash streams
#[test] fn journal_identical_across_shuffle_seeds()       // two unordered test systems record; the journals of 3 shuffle seeds are equal
#[test] fn journal_entries_sorted()                       // within each tick, sorted by (phase, kind, sim_id, emitted)
#[test] fn phase_markers_emit_16_events_per_tick()        // a capturing subscriber counts 8 "start" + 8 "end" events on target fl_sim::phase per step
#[test] fn tick_span_carries_tick()                       // an event emitted inside a system at tick 7 is inside a span with field tick == 7
#[test] fn journal_since_query()                          // after 20 ticks, JournalSince { tick: 10, limit: 5 } → 5 entries, all with tick ≥ 10
// tests/replay.rs
#[test] fn replay_round_trip_verifies()                   // record → to_bytes → from_bytes → verify → Verified
#[test] fn tampered_replay_reports_divergence()           // change the target of the command at tick 37 → Diverged { tick: Tick(37), .. } (hash interval 1)
#[test] fn replay_with_reload_event_is_non_reproducible() // → NonReproducible
#[test] fn replay_build_id_mismatch_warns()               // Verified { warnings.len() == 1 }
#[test] fn replay_rules_hash_mismatch_fails()             // Err(Header(HeaderMismatch::RulesHash))
#[test] fn first_divergence_finds_earliest()
#[test] fn replay_json_lists_inputs()                     // to_json()["inputs"][0]["commands"][0]["seq"] is a number
// fl_test_utils/tests/golden.rs
#[test] fn golden_replays_verify()
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_sim --test observe --test replay`. Expected: FAIL.
- [ ] **Step 3: Implement.** Run `FL_RECORD_GOLDEN=1 cargo nextest run -p fl_test_utils --test golden`, then ask the lead to bless the `.new` file.
- [ ] **Step 4: Run** `cargo nextest run --profile agent -p fl_sim -p fl_test_utils`. Expected: PASS, including `golden_replays_verify`.
- [ ] **Step 5: Gate** `cargo xtask gate task --track sim-core --track contract`. Expected: PASS.
- [ ] **Step 6: Commit** `feat(fl_sim): event journal with registry and per-tick sort, phase markers, tick span, postcard replays` with trailers `Refs: ADR-0007` and `Blessed: M0 golden: testbed wander`.

