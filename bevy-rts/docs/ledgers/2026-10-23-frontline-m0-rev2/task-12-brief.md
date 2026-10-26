### Task 12: `SimRunner` (inline and threaded), Test/Replay/loopback-Lockstep drivers, snapshot publishing

**Track:** contract + sim-core · **Refs:** ADR-0002

**Files:**
- Create: `crates/fl_sim/src/runner/{mod,threaded,test_driver,replay_driver,lockstep}.rs`, `crates/fl_sim/src/publish.rs`, `crates/fl_sim/tests/runner.rs`
- Modify: `crates/fl_sim/src/{step,inspect,feature,lib}.rs`

**Interfaces:**
- Consumes: the `fl_view` runner and driver types (Task 7); `step_tick` (Task 10).
- Produces:
```rust
// publish.rs  (PublishSet::Snapshot; the only place Changed<T> is allowed)
#[derive(Resource, Default)] pub struct SnapshotOut { /* current and previous Arc<SimSnapshot> */ }   // Transient "core.snapshot_out"
//   one record per entity with SimEntity + UnitType + Owner + Pose; hp_frac from Health; flags MOVING (MoveOrder non-empty) and INVULNERABLE;
//   visible_to = 0xFF (vision arrives in M3); records are rebuilt only where Changed<Pose|Health|Owner|MoveOrder|Invulnerable>, the rest copied;
//   the previous buffer is reused when Arc::get_mut succeeds
// runner/mod.rs
pub struct SimRunner { .. }
impl SimRunner {
    pub fn new(app: App) -> SimRunner;
    pub fn next_tick(&self) -> Tick;
    pub fn step(&mut self, input: TickInput) -> TickOutput;
    pub fn run(&mut self, driver: &mut dyn TickDriver, ticks: u32) -> Vec<TickOutput>;   // panics "driver stalled at tick T" if poll returns None
    pub fn query(&mut self, q: SimQuery) -> SimReply;
    pub fn world(&self) -> &World;
    pub fn world_mut(&mut self) -> &mut World;    // setup and tests only
    pub fn spawn_threaded(self) -> (RunnerPort, std::thread::JoinHandle<()>);
}
pub struct TestDriver { .. }
impl TestDriver { pub fn new() -> Self /* delay 0, next Tick(0) */; pub fn with_delay(delay: u32) -> Self; pub fn starting_at(self, next: Tick) -> Self;
                  pub fn restore(&mut self, pending: Vec<PlayerCommand>, last_seq: &BTreeMap<PlayerId, u32>); }
pub struct ReplayDriver { .. }   impl ReplayDriver { pub fn new(inputs: Vec<TickInput>, start: Tick, until: Option<Tick>) -> Self; }
pub trait NetSession: Send {
    fn send(&mut self, tick: Tick, player: PlayerId, commands: Vec<PlayerCommand>);
    fn recv(&mut self) -> Vec<(Tick, PlayerId, Vec<PlayerCommand>)>;
}
pub struct LoopbackSession { .. }
pub fn loopback(players: &[PlayerId]) -> Vec<LoopbackSession>;   // one per player; every send reaches every session, the sender's included
pub struct LockstepDriver<S: NetSession> { .. }
impl<S: NetSession> LockstepDriver<S> { pub fn new(local: PlayerId, players: Vec<PlayerId>, session: S, delay: u32) -> Self; }
// TestDriver, ReplayDriver and LockstepDriver implement fl_view::TickDriver
```
- **Lockstep `poll` for tick T:**
  1. Send the local batch for exec tick `T + delay`: every command issued since the last poll, possibly none.
  2. Drain the session.
  3. Return the input for T only once every player's batch for T is present. Ticks `< delay` need no batches.
- **Threaded runner:** a thread named `fl-sim`. This is the one thread spawn in a sim crate, behind `#[expect(clippy::disallowed_methods, reason = "SimRunner owns the sim thread (spec §5.3)")]`.
  - `ToRunner::Tick` → `step` under `catch_unwind`. On a panic it sends `Failed { tick, message }`, and the thread exits.
  - `Query` → `Reply`.
  - `Shutdown` or a disconnected channel → exit.
- **Query answers:**
  - `Entity` → `inspect_entity`; `WorldSummary` → `inspect_world`.
  - `Perf` → `{"counters": {name: n}}` for the last tick.
  - `Header` → the `MatchHeaderRes`.
  - `JournalSince` → an empty list until Task 18.
  - `Save` → `Error("save: not implemented")` until Task 13.
- `step_tick` now fills `TickOutput.snapshot` from `SnapshotOut` and drains `SimEvents` into `TickOutput.events`.

- [ ] **Step 1: Write the failing tests** (`tests/runner.rs`):
```rust
#[test] fn inline_and_threaded_hash_streams_equal()   // duel + debug.wander 100, 200 ticks, interval 1 → identical Vec<(Tick, u64)>
#[test] fn threaded_panic_is_reported()              // a test feature panics "boom" at tick 5 → FromRunner::Failed { tick: Tick(5), message containing "boom" }; join() returns
#[test] fn drop_port_stops_thread()                  // drop the RunnerPort → the JoinHandle finishes within 2 s
#[test] fn query_between_ticks()                     // after 10 ticks: Entity(SimId(1)) → JSON with components["core.pose"]
#[test] fn test_driver_delay_zero_applies_same_tick() // issued when next is 3 → exec_tick 3; MoveOrder present after that tick
#[test] fn delay_one_applies_next_tick()
#[test] fn replay_driver_reproduces_hashes()         // the TickInputs of a 150-tick TestDriver run with commands → ReplayDriver gives the same hashes
#[test] fn loopback_lockstep_two_sims_identical()    // two runners, LockstepDriver delay 2; P0 moves at local tick 10, P1 at tick 37; 300 ticks →
                                                     //   equal hash streams; the commands appear in the TickInputs of ticks 12 and 39
#[test] fn lockstep_waits_for_missing_peer()         // P1's session never sends → poll for tick 2 == None
#[test] fn snapshot_matches_world()                  // entities.len() == SimIndex.len(); record(1).pos == Pose(1).pos; sorted by sim_id
#[test] fn incremental_publish_equals_full_rebuild() // 500-unit scale, 200 ticks: every snapshot == the test helper publish_full(world)
#[test] fn snapshot_buffers_are_recycled()           // outputs dropped promptly → Arc::as_ptr(out0.snapshot) == Arc::as_ptr(out2.snapshot)
#[test] fn events_drained_per_tick()                 // debug.spawn → that tick's events hold one Spawned; the next tick's events are empty
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_sim --test runner`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo nextest run --profile agent -p fl_sim`. Expected: PASS.
- [ ] **Step 5: Gate** `cargo xtask gate task --track contract --track sim-core`. Expected: PASS.
- [ ] **Step 6: Commit** `feat(fl_sim): SimRunner (inline + threaded), Test/Replay/loopback Lockstep drivers, snapshot publishing` with trailer `Refs: ADR-0002`.

