### Task 7: `fl_sim` core — commands, spawning, unit components, movement, scenarios, scale scenario

**Files:**
- Fill `crates/fl_sim/src/{commands,spawn,units,movement,scenario,scale}.rs`.
- Modify `feature.rs` (the CoreFeature registrations) and `app_ext.rs` (add `register_command`).
- Create `crates/fl_sim/tests/{commands,movement}.rs`.
- Add these fixtures to `tests/fixtures/core_mod/`:
  - `units/tank.unit.ron`: `core_test.tank` with Selectable, Hull (hp 100, footprint (3.0, 6.0)) and Mobility (Tracked, 36 km/h, terrain 0.5, turn 90°/s).
  - `units/abstract_vehicle.unit.ron`: abstract.
  - `factions/{red,blue}.faction.ron`.
  - `maps/flat.map.ron`: 1000×1000, cell 4.
  - `scenarios/duel.scenario.ron`: 2 players, one tank each, at (100,100) and (900,900).
  - A sibling fixture `tests/fixtures/absurd_speed/`, a copy of core_mod with `road_speed_kmh: 1e12`.

**Interfaces — Produces:**
```rust
// commands.rs
pub trait CommandPayload: Serialize + DeserializeOwned + std::fmt::Debug + Send + Sync + 'static {
    const NAME: &'static str;
    const NEEDS_UNITS: bool = true;
    const DEBUG_ONLY: bool = false;
    fn unit_can(&self, _unit: EntityRef<'_>) -> bool { true }
    fn apply(&self, world: &mut World, ctx: &CommandCtx<'_>);
}
pub struct CommandCtx<'a> { pub tick: Tick, pub player: PlayerId, pub units: &'a [(SimId, Entity)], pub queue: bool }
#[derive(Resource, Default)] pub struct CommandRegistry { … }
impl CommandRegistry { pub fn type_id<P: CommandPayload>(&self) -> CommandTypeId; pub fn encode<P: CommandPayload>(&self, p: &P) -> CommandKind; pub fn name(&self, id: CommandTypeId) -> Option<&'static str>; }
// SimAppExt gains: fn register_command<P: CommandPayload>(&mut self) -> &mut Self;   ids assigned by sorted NAME in build_sim_app
#[derive(Resource, Default)] pub struct CommandQueue { … }
impl CommandQueue { pub fn push(&mut self, cmd: PlayerCommand, now: Tick) -> Result<(), RejectReason>; }  // cmd.tick < now ⇒ Err(Late)
pub enum RejectReason { Late, UnknownType, BadPayload, UnknownPlayer, NoValidUnits, DebugDisabled }
#[derive(Resource, Default)] pub struct CommandLog { pub applied: Vec<PlayerCommand>, pub rejected: Vec<(PlayerCommand, RejectReason)> }
pub fn submit(world: &mut World, cmd: PlayerCommand) -> Result<(), RejectReason>;   // push at current clock; Late rejections also logged
// apply_commands: exclusive, SimSet::ApplyCommands, after the SimEvents clear. Commands for clock.tick in push order;
// units deduped+sorted, filtered to existing, owned by cmd.player, unit_can; NEEDS_UNITS && empty ⇒ NoValidUnits.
// Core payloads (all #[serde(deny_unknown_fields)]):
pub struct MoveCmd { pub target: FxVec2 }                 // "core.move"; unit_can: has Mobility; queue ⇒ append waypoint, else replace
pub struct StopCmd;                                       // "core.stop"; clears MoveOrder
pub struct DebugSpawnCmd { pub unit: UnitTypeId, pub pos: FxVec2, pub heading: Angle }   // "debug.spawn"; NEEDS_UNITS false; DEBUG_ONLY
pub struct DebugWanderCmd { pub radius: Fx }              // "debug.wander"; DEBUG_ONLY; radius 0 removes Wander, else inserts Wander{home: pos, radius}
// units.rs — def components (registered by CoreFeature) and runtime components
pub struct SelectableDef;                                                  // "Selectable" → Selectable marker
pub struct HullDef { pub hp: u32, pub footprint: FxVec2, pub hitbox_shrink: Fx }     // "Hull" → Health{hp,max}, Footprint; validate hp ≥ 1, 0 < footprint ≤ 50, 0 ≤ shrink ≤ 1
pub enum MoveClass { Foot, Wheeled, HalfTrack, Tracked }
pub struct MobilityDef { pub class: MoveClass, pub road_speed_kmh: Fx, pub terrain_factor: Fx, pub turn_rate_deg_s: Fx }  // "Mobility"; validate 0 < kmh ≤ 200, 0 ≤ factor ≤ 1, 0 < turn ≤ 720
#[derive(Component)] pub struct Owner(pub PlayerId);
#[derive(Component)] pub struct UnitType(pub UnitTypeId);
#[derive(Component)] pub struct Pose { pub pos: FxVec2, pub heading: Angle }
#[derive(Component)] pub struct Health { pub hp: u32, pub max: u32 }
#[derive(Component)] pub struct Footprint { pub width: Fx, pub length: Fx, pub hitbox_shrink: Fx }
#[derive(Component)] pub struct Mobility { pub class: MoveClass, pub speed: Fx /* m per tick = kmh / 72 */, pub terrain_factor: Fx, pub turn_rate: Angle /* per tick = deg_s / 20 */ }
#[derive(Component, Default)] pub struct MoveOrder { pub waypoints: Vec<FxVec2> }
#[derive(Component)] pub struct Wander { pub home: FxVec2, pub radius: Fx }
#[derive(Component)] pub struct Selectable;
// hashed: "Owner", "UnitType", "Pose", "Health", "MoveOrder", "Wander"
// spawn.rs
pub fn spawn_unit(world: &mut World, unit: UnitTypeId, player: PlayerId, pos: FxVec2, heading: Angle) -> Result<SimId, SpawnError>;
pub fn despawn_unit(world: &mut World, id: SimId) -> bool;
pub enum SpawnError { UnknownUnit, UnknownPlayer, OutsideMap, NoMap }
// movement.rs
#[derive(SystemSet, …)] pub struct MovementSystems;   // label Task 9 orders after
// wander_system (SimSet::Behaviour, SimIndex order): Wander + empty MoveOrder ⇒ push map.clamp(home + (range_fx(-r,r), range_fx(-r,r)))
// move_system (SimSet::Movement, in MovementSystems, SimIndex order): toward waypoint[0]: heading.rotate_towards(desired, turn_rate);
//   advance only when |signed_diff| ≤ 8192 (45°): step = min(speed, dist) along the straight line to the target; arrive (dist ≤ speed) ⇒ snap + pop; pos = map.clamp(pos)
// scenario.rs
pub enum SetupError { UnknownMap(String), UnknownScenario(String), UnknownFaction(String), Spawn(SpawnError) }
pub fn setup_map(world: &mut World, map_id: &str) -> Result<(), SetupError>;              // inserts MapInfo
pub fn setup_scenario(world: &mut World, scenario_id: &str) -> Result<Vec<SimId>, SetupError>;  // map, Players, SimRng reseed(scenario.seed), units in file order
pub fn setup_scenario_def(world: &mut World, def: &ScenarioDef) -> Result<Vec<SimId>, SetupError>; // same, from a def (setup_scenario delegates to it)
// all hashed runtime components also derive serde::Serialize (inspection, spec §15 C)
// scale.rs
pub struct ScaleScenario { pub units: u32, pub players: u8, pub map_m: i32, pub wander_m: i32, pub seed: u64 }
impl Default for ScaleScenario { /* 3000, 4, 2000, 150, 1 */ }
impl ScaleScenario { pub fn populate(&self, world: &mut World) -> Result<Vec<SimId>, SetupError>; }
//   flat map map_m×map_m cell 4; players i → faction i % factions (sorted), team i; positions uniform in player i's quadrant via Pcg32::new(seed, 7);
//   unit types cycle over concrete units with Mobility (sorted id); every unit gets Wander{home: pos, radius: wander_m}; SimRng reseeded with seed
```

- [ ] **Step 1: Write the failing tests.**
  - `tests/commands.rs`:
    - `move_command_reaches_target`: a tank at (100,100) with heading 0 gets `MoveCmd{target:(200,100)}` at tick 0. Within 200 ticks its pose is (200,100) and its MoveOrder is empty.
    - `queued_moves_append`: `queue=true` leaves 2 waypoints.
    - `foreign_units_rejected`: `NoValidUnits` appears in `CommandLog.rejected`.
    - `late_command_rejected`.
    - `debug_spawn_requires_flag`: `DebugDisabled` when `allow_debug_commands` is false; a 3rd unit spawns when it is true.
    - `command_type_ids_sorted_by_name`: `core.move` < `core.stop` < `debug.spawn` < `debug.wander`.
    - `bad_commands_do_not_change_state`: run A is 50 ticks. Run B is the same plus four bad commands: a late command, an unknown `CommandTypeId(999)`, payload `[0xFF; 3]` for core.move, and a move for a foreign SimId. The final `compute_state_hash` values are equal, and `rejected.len() == 4`.
  - `tests/movement.rs`:
    - `kmh_converts_to_m_per_tick`: 36 km/h gives exactly `Fx::from_int(36) / Fx::from_int(72)` = 0.5.
    - `tank_turns_before_moving`: with the target behind, the first ticks change the heading while the position stays the same.
    - `move_target_outside_map_is_clamped`: a target of (5000,5000) leaves the final position inside the map.
    - `spawn_outside_map_is_error`.
    - `spawn_unknown_unit_is_error`: `spawn_unit` with an out-of-range `UnitTypeId` gives `UnknownUnit`, and an abstract id is not interned (`unit_id` is None).
    - `mobility_rejects_absurd_speed`: loading `absurd_speed` gives `ContentErrors`.
    - `wander_stays_within_radius_and_map`: with a scale scenario of 200 units over 400 ticks, every position stays inside the map and within radius·√2 of home.
    - `scale_populate_is_deterministic`: two worlds with the same seed have identical pose lists, and 4 players are present.
    - `setup_scenario_spawns_units`: duel gives 2 SimIds, `[1,2]`.
- [ ] **Step 2: Run** `cargo test -p fl_sim`. Expected: FAIL.
- [ ] **Step 3: Implement** with the signatures above. `DebugSpawnCmd.apply` calls `spawn_unit` for `ctx.player`.
- [ ] **Step 4: Run** `cargo test -p fl_sim`. Expected: PASS.
- [ ] **Step 5: Gate + commit** `feat(fl_sim): commands, spawning, core unit components, movement, scenarios, scale scenario`.

