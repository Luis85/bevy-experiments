### Task 11: T7 rework — command apply order, spawn funnel, `debug.*` handlers, scenario seed

**Track:** sim-core · **Refs:** ADR-0007, ADR-0005, ADR-0008

**Files:**
- Create: `crates/fl_sim/src/debug_cmds.rs`, `crates/fl_sim/tests/funnel.rs`
- Modify: `crates/fl_sim/src/{commands,spawn,units,scenario,scale,step,feature,ids,lib}.rs`, `crates/fl_sim/tests/{commands,movement}.rs`

**Interfaces:**
- Consumes: Tasks 6, 9, 10.
- Produces:
```rust
// commands.rs
#[derive(Resource, Default)] pub struct CommandInbox(pub Vec<PlayerCommand>);   // Transient "core.command_inbox"; filled by step_tick, sorted (player, seq)
#[derive(Resource, Default, Clone, Serialize, Deserialize)] pub struct LastSeq(pub BTreeMap<PlayerId, u32>);   // Hashed "core.last_seq"
pub enum RejectReason { Late, Early, DuplicateSeq, UnknownType, BadPayload, UnknownPlayer, NoValidUnits, DebugDisabled, UnknownContent }
#[derive(Resource, Default)] pub struct CommandLog { pub applied: Vec<PlayerCommand>, pub rejected: Vec<(PlayerCommand, RejectReason)> }   // Transient: this tick only
pub fn last_seqs(world: &World) -> BTreeMap<PlayerId, u32>;
// spawn.rs
#[derive(Message, Clone, Debug)] pub struct SpawnRequest { pub unit: Option<UnitTypeId>, pub owner: PlayerId, pub pos: FxVec2, pub heading: Angle, pub init: Option<SpawnInit> }
pub type SpawnInit = fn(&mut EntityWorldMut<'_>, &SpawnRequest);   // runs after def instantiation; how features spawn non-unit entities
//   unit: Some(id) → SimEntity + Owner + UnitType + Pose + the unit's defs; unit: None → SimEntity + Owner + Pose only; then init
#[derive(Message, Clone, Copy, Debug)] pub struct DespawnRequest(pub SimId);
pub enum SpawnError { UnknownUnit, UnknownPlayer, OutsideMap, NoMap }
pub fn setup_spawn(world: &mut World, req: &SpawnRequest) -> Result<SimId, SpawnError>;   // the setup and load path, outside SimTick; the funnel calls the same code
pub fn setup_despawn(world: &mut World, id: SimId) -> bool;
// scenario.rs
impl MatchSetup { pub fn for_scenario(content: &ContentDb, scenario_id: &str) -> Result<MatchSetup, SetupError>; }   // seed = the scenario's seed, cheats false
pub fn setup_scenario(world: &mut World, scenario_id: &str) -> Result<Vec<SimId>, SetupError>;   // map, Players, units in file order via setup_spawn; never reseeds
// debug_cmds.rs: handlers for the six debug payloads
#[derive(Component, Clone, Copy, Debug, Serialize, Deserialize)] pub struct Invulnerable;   // Hashed "core.invulnerable"
```
- **`apply_commands`** (exclusive, `InputSet::Apply`) takes inbox commands in `(player, seq)` order and checks, in this order:
  1. `exec_tick < tick` → `Late`; `exec_tick > tick` → `Early`.
  2. `seq ≤ LastSeq[player]` → `DuplicateSeq`. `LastSeq` is updated by every command that isn't a duplicate, whether or not it then applies.
  3. Unknown type → `UnknownType`; a decode failure → `BadPayload`; an unknown player → `UnknownPlayer`.
  4. `DEBUG_ONLY` while `!cheats_allowed` → `DebugDisabled`.
  5. Units are deduped and sorted, kept only if they exist, are owned by the player (`DEBUG_ONLY` skips ownership) and pass `unit_can`. None left with `NEEDS_UNITS` → `NoValidUnits`.
  6. The handler may return `UnknownContent`.

  Every rejection is logged at `debug` and pushed as `SimEvent::CommandRejected`.
- **`spawn_funnel`** (exclusive, `LifecycleSet::Spawn`) handles requests in write order through the shared spawn code. A failure increments counter `spawn.rejected` and is logged, never a panic. A success increments `spawn.entities` and pushes `SimEvent::Spawned`.
- **`despawn_funnel`** (`LifecycleSet::Despawn`) dedupes requests, sorts them by `SimId`, increments `despawn.entities` and pushes `SimEvent::Despawned`.
- `SimIdAllocator::alloc` becomes `pub(crate)`.
- **Debug handlers:**
  - `debug.spawn` writes a `SpawnRequest`, resolving the unit name with `unit_id`. An unknown name → `UnknownContent`.
  - `debug.set_owner` rejects an unknown owner with `UnknownPlayer`.
  - `debug.set_hp` sets `hp = min(hp, max)`.
  - `debug.teleport` sets `pos = map.clamp(pos)` and clears `MoveOrder`.
  - `debug.invulnerable` inserts or removes `Invulnerable`.
  - `debug.wander` keeps its rev-1 behaviour.

- [ ] **Step 1: Write the failing tests:**
```rust
// tests/commands.rs — ported: move_command_reaches_target, queued_moves_append, stop_clears_the_move_order,
//   foreign_units_rejected, duplicate_and_missing_units_are_filtered, unknown_player_rejected. New:
#[test] fn commands_apply_in_player_seq_order()   // input [P0 seq 1 Move(300,100), P0 seq 0 Move(200,100)], both non-queued → final waypoint (300,100)
#[test] fn duplicate_seq_rejected()               // two P0 seq 4 commands in one tick → the second is DuplicateSeq
#[test] fn seq_must_increase_across_ticks()       // P0 seq 5 at tick 1; seq 5 at tick 2 → DuplicateSeq; seq 3 at tick 3 → DuplicateSeq
#[test] fn late_and_early_rejected()              // exec_tick = tick − 1 → Late; tick + 1 → Early
#[test] fn debug_requires_cheats()                // cheats false: debug.spawn → DebugDisabled; SimIndex.len() stays 2
#[test] fn debug_spawn_appears_at_end_of_tick()   // cheats true: after the step SimIndex has SimId(3), and SimEvents holds Spawned { sim_id: SimId(3), .. }
#[test] fn debug_spawn_unknown_unit_rejected()    // unit "nope" → UnknownContent
#[test] fn debug_commands_target_any_player()     // P0's debug.set_hp { hp: 1 } on P1's unit → applied; Health.hp == 1
#[test] fn debug_set_owner_invulnerable_teleport()   // Owner == PlayerId(1); Invulnerable present; pos == map.clamp((5000,5000)) and MoveOrder empty
#[test] fn bad_commands_do_not_change_state()     // run B adds a late, an early, a duplicate-seq, a type-999, a [0xFF; 3]-payload, a foreign-SimId and a
                                                  //   debug.spawn "nope" command: 7 rejections summed over ticks; final compute_state_hash == run A's
#[test] fn despawned_unit_command_same_tick_filtered()   // a test system writes DespawnRequest(2) in tick t; a move for unit 2 at t+1 → NoValidUnits
// tests/funnel.rs
#[test] fn spawn_requests_processed_in_write_order()   // P1 seq 0 then P0 seq 0 debug.spawn in one tick → apply order P0, P1 → SimId 3 is owned by P0
#[test] fn spawn_outside_map_rejected_not_panicking()  // a request at (5000,5000) → no entity; counter "spawn.rejected" == 1
#[test] fn despawns_dedupe_and_sort()                  // DespawnRequest 5, 3, 5 → Despawned events in order [3, 5]; counter "despawn.entities" == 2
#[test] fn setup_and_funnel_share_code()               // the same request via setup_spawn and via the funnel → equal inspect_entity apart from sim_id
#[test] fn spawn_init_runs_after_defs()                // a SpawnInit inserting TestTag → TestTag present after the tick
// tests/movement.rs
#[test] fn for_scenario_uses_scenario_seed()           // the duel fixture's seed → MatchSetup.seed
// ported: setup_scenario_spawns_units (ids [1, 2]), spawn_outside_map_is_error, spawn_unknown_unit_is_error (via setup_spawn)
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_sim`. Expected: FAIL.
- [ ] **Step 3: Implement.** Remove the rev-1 `CommandQueue` and `submit`; tests feed `TickInput`s through `step_tick`.
- [ ] **Step 4: Run** the tests. Expected: PASS.
- [ ] **Step 5: Gate** `cargo xtask gate task --track sim-core --track contract`. Expected: PASS.
- [ ] **Step 6: Commit** `feat(fl_sim)!: (player, seq) command apply, spawn funnel, debug.* handlers gated by cheats_allowed` with trailer `Refs: ADR-0007, ADR-0005, ADR-0008`.

