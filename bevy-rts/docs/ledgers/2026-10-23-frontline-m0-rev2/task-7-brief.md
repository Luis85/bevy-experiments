### Task 7: `fl_view` — snapshot schema, events, journal entry, runner protocol, driver trait

**Track:** contract · **Refs:** ADR-0002

**Files:**
- Create: `crates/fl_view/src/{lib,snapshot,event,journal,runner,driver}.rs`, `crates/fl_view/tests/view.rs`

**Interfaces:**
- Produces (`#![deny(clippy::float_arithmetic)]`; data types derive `Debug, Clone, PartialEq, Serialize, Deserialize`):
```rust
pub const VIEW_FORMAT_VERSION: u32 = 1;
// snapshot.rs
pub struct SimSnapshot { pub tick: Tick, pub entities: Vec<EntityRecord> /* sorted by sim_id */, pub channels: Vec<(String, Vec<u8>)> /* sorted by name */ }
impl SimSnapshot {
    pub fn get(&self, id: SimId) -> Option<&EntityRecord>;      // binary search
    pub fn channel<C: ViewChannel>(&self) -> Option<Result<C, String>>;
    pub fn set_channel<C: ViewChannel>(&mut self, value: &C);
}
pub struct EntityRecord { pub sim_id: SimId, pub unit: UnitTypeId, pub owner: PlayerId, pub pos: FxVec2, pub heading: Angle,
    pub turrets: Vec<Angle>, pub hp_frac: u16, pub flags: u32, pub visible_to: u8 }
impl EntityRecord { pub fn hp_frac(hp: u32, max: u32) -> u16; }   // hp * 65535 / max, rounded down; max 0 → 0
pub mod flags { pub const MOVING: u32 = 1; pub const INVULNERABLE: u32 = 2; }
pub trait ViewChannel: Serialize + DeserializeOwned + Send + Sync + 'static { const NAME: &'static str; }
// event.rs
#[non_exhaustive] pub enum SimEvent { Spawned { sim_id: SimId, unit: Option<UnitTypeId>, owner: PlayerId }, Despawned { sim_id: SimId },
    CommandRejected { player: PlayerId, seq: u32, reason: String }, Custom { channel: String, data: Vec<u8> } }
// journal.rs
pub struct JournalEntry { pub tick: Tick, pub phase: u8, pub kind: String, pub sim_id: Option<SimId>, pub player: Option<PlayerId>,
    pub emitted: u32, pub data: serde_json::Value }
// runner.rs
pub struct TickInput { pub tick: Tick, pub commands: Vec<PlayerCommand> }
pub struct TickOutput { pub tick: Tick, pub hash: Option<u64>, pub snapshot: Arc<SimSnapshot>, pub events: Vec<SimEvent>, pub journal: Vec<JournalEntry> }   // not serialized
#[non_exhaustive] pub enum SimQuery { Entity(SimId), WorldSummary, JournalSince { tick: Tick, limit: u32 }, Perf, Save { pending: Vec<PlayerCommand> }, Header }
#[non_exhaustive] pub enum SimReply { Entity(Option<serde_json::Value>), WorldSummary(serde_json::Value), Journal(Vec<JournalEntry>),
    Perf(serde_json::Value), Save(Vec<u8>), Header(MatchHeader), Error(String) }
pub enum ToRunner { Tick(TickInput), Query { id: u64, query: SimQuery }, Shutdown }
pub enum FromRunner { Tick(TickOutput), Reply { id: u64, reply: SimReply }, Failed(RunnerFailure) }
pub struct RunnerFailure { pub tick: Tick, pub message: String }
pub struct RunnerPort { pub tx: crossbeam_channel::Sender<ToRunner>, pub rx: crossbeam_channel::Receiver<FromRunner> }   // client side
pub struct RunnerEnd { pub rx: crossbeam_channel::Receiver<ToRunner>, pub tx: crossbeam_channel::Sender<FromRunner> }  // runner side
pub fn runner_channel() -> (RunnerPort, RunnerEnd);   // unbounded
// driver.rs
pub struct CommandDraft { pub player: PlayerId, pub units: Vec<SimId>, pub kind: CommandKind, pub queue: bool }
#[derive(Default)] pub struct CommandStamper { /* next seq per player; held: BTreeMap<Tick, Vec<PlayerCommand>> */ }
impl CommandStamper {
    pub fn stamp(&mut self, exec_tick: Tick, draft: CommandDraft) -> PlayerCommand;   // seq starts at 0 per player and increases by 1 per stamp; holds the command
    pub fn take_due(&mut self, tick: Tick) -> Vec<PlayerCommand>;                    // held commands with exec_tick ≤ tick, sorted (player, seq)
    pub fn held(&self) -> Vec<PlayerCommand>;
    pub fn restore(&mut self, pending: Vec<PlayerCommand>, last_applied: &BTreeMap<PlayerId, u32>);   // re-holds pending; next seq = max(last applied, pending) + 1
}
pub trait TickDriver {
    fn next_tick(&self) -> Tick;
    fn input_delay(&self) -> u32;
    fn issue(&mut self, draft: CommandDraft) -> PlayerCommand;   // exec_tick = next_tick + input_delay
    fn poll(&mut self) -> Option<TickInput>;                     // the next tick's input if ready; advances next_tick
    fn pending(&self) -> Vec<PlayerCommand>;                     // stamped but not yet delivered
}
```

- [ ] **Step 1: Write the failing tests** (`tests/view.rs`):
```rust
#[test] fn snapshot_postcard_roundtrip()
#[test] fn get_finds_by_sim_id()                     // records 1, 4, 9 → get(SimId(4)).unwrap().sim_id == SimId(4); get(SimId(5)) == None
#[test] fn channel_roundtrip()                       // set_channel(&TestChan { n: 7 }) → channel::<TestChan>() == Some(Ok(TestChan { n: 7 }))
#[test] fn hp_frac_values()                          // (50, 100) → 32767; (100, 100) → 65535; (0, 0) → 0
#[test] fn stamper_assigns_increasing_seq_per_player()   // P0, P1, P0 → seqs 0, 0, 1
#[test] fn stamper_take_due_sorted_and_only_due()    // held at ticks 5 and 6; take_due(5) returns only the tick-5 commands, sorted (player, seq)
#[test] fn stamper_restore_continues_seq()           // last_applied {P0: 4}, pending [P0 seq 5] → the next P0 stamp has seq 6
#[test] fn tick_input_postcard_roundtrip()
#[test] fn runner_channel_carries_messages()         // ToRunner::Shutdown sent on port.tx arrives on end.rx
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_view`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** the tests. Expected: PASS.
- [ ] **Step 5: Gate** `cargo xtask gate task --track contract`. Expected: PASS.
- [ ] **Step 6: Commit** `feat(fl_view): snapshot schema, events, journal entry, runner protocol, command stamper and TickDriver` with trailer `Refs: ADR-0002`.

