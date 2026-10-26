### Task 20: Client view — `ClientFeature`, `LocalDriver`, runner link, snapshot interpolation, placeholders (rev-1 T13)

**Track:** client · **Refs:** ADR-0002, ADR-0001 · **Lane 2** (cut after Tasks 7 and 8)

**Files:**
- Create: `crates/fl_client/src/{feature,driver,link}.rs`, `crates/fl_client/tests/view.rs` (a fake runner thread built on `fl_view::runner_channel`)
- Modify: `crates/fl_client/src/{view,placeholder,lib}.rs`

**Interfaces:**
- Consumes: `fl_view` (Task 7), `fl_protocol` payloads and `StringTables` (Task 6), `fl_content::{VisualDef, Placeholder, Faction, ContentDb}`.
- Produces:
```rust
// feature.rs
pub trait ClientFeature: Send + Sync + 'static { fn sim_feature(&self) -> &'static str; fn build(&self, app: &mut App); }
// driver.rs
#[derive(Resource)] pub struct LocalDriver { .. }   // input delay 1; wraps fl_view::CommandStamper
impl LocalDriver {
    pub fn new(next: Tick) -> Self;
    pub fn advance(&mut self, dt: Duration);         // dt × speed is added to the accumulator unless paused
    pub fn pause(&mut self); pub fn resume(&mut self); pub fn is_paused(&self) -> bool;
    pub fn step(&mut self, n: u32);                  // while paused, allow n ticks
    pub fn set_speed(&mut self, x: f32);             // clamped to [0.25, 8.0]
    pub fn speed(&self) -> f32;
    pub fn overstep_fraction(&self) -> f32;          // accumulator / tick length, in [0, 1)
    pub fn restore(&mut self, next: Tick, pending: Vec<PlayerCommand>, last_seq: &BTreeMap<PlayerId, u32>);
}
impl TickDriver for LocalDriver { /* poll: Some while the accumulator ≥ 50 ms (or a step is allowed), carrying take_due(tick) */ }
// link.rs
#[derive(Resource)] pub struct RunnerLink { pub port: RunnerPort, pub header: MatchHeader }
#[derive(Resource, Default)] pub struct SnapshotBuffer { pub prev: Option<Arc<SimSnapshot>>, pub curr: Option<Arc<SimSnapshot>> }
#[derive(Message, Clone, Debug)] pub struct SimEventMsg(pub SimEvent);
#[derive(Resource, Default)] pub struct RunnerStatus { pub failed: Option<RunnerFailure>, pub last_tick: Option<Tick>, pub last_hash: Option<u64> }
#[derive(Resource, Default)] pub struct PendingReplies { /* id → SimReply; next id */ }
pub fn encode_command<P: CommandPayload>(tables: &StringTables, payload: &P) -> Result<CommandKind, String>;   // id from the "commands" table by P::NAME
pub fn send_query(link: &RunnerLink, replies: &mut PendingReplies, q: SimQuery) -> u64;
// view.rs
#[derive(Resource, Clone)] pub struct ClientContent(pub Arc<ContentDb>);
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)] pub struct LocalPlayer(pub PlayerId);
#[derive(Resource, Default, Clone, Copy)] pub struct RevealAll(pub bool);   // render override: draw every record regardless of visible_to
#[derive(Component, Clone, Copy, Debug)] pub struct UnitView { pub sim_id: SimId, pub unit: UnitTypeId, pub owner: PlayerId }
#[derive(Component, Clone, Copy, Debug)] pub struct ViewPose { pub prev: Vec3, pub curr: Vec3, pub prev_yaw: f32, pub curr_yaw: f32 }
#[derive(Resource, Default)] pub struct ViewIndex(pub BTreeMap<SimId, Entity>);
pub fn sim_to_world(pos: FxVec2, height: f32) -> Vec3;   // Vec3::new(x, height, y)
pub fn heading_to_yaw(h: Angle) -> f32;                    // -h.to_radians_f32()
pub fn lerp_yaw(a: f32, b: f32, t: f32) -> f32;           // shortest arc
// SimViewPlugin, in Update, chained:
//   drive_runner: LocalDriver.advance(real dt), send every polled TickInput; drain ALL FromRunner messages:
//     Tick → rotate SnapshotBuffer, write one SimEventMsg per event, update RunnerStatus; Reply → PendingReplies; Failed → RunnerStatus.failed + pause
//   sync_views: merge SnapshotBuffer.curr with ViewIndex (spawn new, despawn vanished, shift curr → prev)
//   interpolate_views: Transform from ViewPose at LocalDriver.overstep_fraction()
// PlaceholderPlugin: as rev-1 Task 13 (shared mesh and material handles per unit type and player)
```
- [ ] **Step 1: Write the failing tests** (`MinimalPlugins` + `SimViewPlugin`, with a fake runner thread replying to `ToRunner::Tick` with canned `TickOutput`s):
```rust
#[test] fn sim_to_world_axes()  #[test] fn heading_90_faces_plus_z()  #[test] fn lerp_yaw_wraps()  #[test] fn interpolation_midpoint()   // as rev-1 T13
#[test] fn local_driver_stamps_delay_one()          // next 5 → exec_tick 6; seqs 0, 1 for two P0 commands
#[test] fn local_driver_accumulates_fixed_steps()   // advance(120 ms) at speed 1 → 2 polls, then None; overstep_fraction ≈ 0.4 (±1e-3)
#[test] fn speed_clamped()                          // set_speed(100.0) → 8.0; set_speed(0.0) → 0.25
#[test] fn paused_driver_emits_nothing_until_step() // paused, advance(1 s) → no poll; step(2) → exactly 2 polls
#[test] fn paused_orders_apply_on_resume()          // paused at next 10; issue → exec_tick 11; resume; the TickInput for 11 carries it
#[test] fn visuals_follow_snapshots()               // fake runner with 10 records → 10 UnitView entities after 3 updates
#[test] fn despawned_record_removes_visual()        // a record missing from the next snapshot → its entity is gone; no panic in interpolation
#[test] fn multiple_outputs_per_frame_keep_all_events()   // 3 TickOutputs, each with one Spawned, arrive before one frame → 3 SimEventMsg read that frame
#[test] fn first_frame_without_snapshot_draws_nothing()   // no TickOutput yet → 0 UnitView, no panic
#[test] fn runner_failure_is_surfaced()             // FromRunner::Failed → RunnerStatus.failed is Some, LocalDriver paused
#[test] fn encode_command_uses_string_table()       // tables {"commands": ["core.move", "core.stop"]} → MoveCmd encodes with type_id 0; an unknown name → Err
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_client`. Expected: FAIL.
- [ ] **Step 3: Implement.** Check every Bevy API with the `bevy-api` skill.
- [ ] **Step 4: Run** the tests. Expected: PASS.
- [ ] **Step 5: Gate** `cargo xtask gate task --track client`. Expected: PASS, and `arch-check` confirms `fl_client` has no `fl_sim` edge.
- [ ] **Step 6: Commit** `feat(fl_client): ClientFeature, LocalDriver, runner link, snapshot interpolation, placeholder visuals` with trailer `Refs: ADR-0002`.

