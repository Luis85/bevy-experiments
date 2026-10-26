### Task 21: Debug tools — console, spawn panel, stats, time controls, inspector (rev-1 T14)

**Track:** client · **Refs:** ADR-0008 · **Lane 2**

**Files:**
- Create: `crates/fl_client/src/debug/{mod,console,spawn_panel,stats,inspector}.rs`, `crates/fl_client/tests/debug.rs`

**Interfaces:**
- Consumes: `LocalDriver`, `RunnerLink`, `encode_command`, `send_query`, `PendingReplies`, `SnapshotBuffer`, `LocalPlayer`, `RevealAll`, `ClientContent`, `RunnerStatus` (Task 20); the debug payloads (Task 6).
- Produces:
```rust
pub type ConsoleFn = fn(&mut World, &[&str]) -> Result<String, String>;
#[derive(Resource, Default)] pub struct ConsoleCommands { /* BTreeMap<&'static str, (help, ConsoleFn)> */ }
pub trait ConsoleAppExt { fn register_console_command(&mut self, name: &'static str, help: &'static str, f: ConsoleFn) -> &mut Self; }
#[derive(Resource, Default)] pub struct ConsoleState { pub input: String, pub history: Vec<String>, pub open: bool }
pub fn run_console_line(world: &mut World, line: &str) -> String;   // tokenize, dispatch, else "unknown command: <name>"
#[derive(Resource, Default)] pub struct CursorGround(pub Option<Vec3>);
#[derive(Resource)] pub struct SpawnPanel { pub unit: Option<String>, pub owner: PlayerId, pub placing: bool, pub heading_deg: i32 }
#[derive(Resource, Default)] pub struct InspectorState { pub sim_id: Option<SimId>, pub pending: Option<u64>, pub last: Option<serde_json::Value> }
```
- **Console commands.** Every sim change is a command issued through `LocalDriver`; the console never mutates anything else.

  | Command | Effect |
  |---|---|
  | `help` | lists commands |
  | `spawn <unit_id> <player> <x> <y>` | `debug.spawn` |
  | `wander <player> <radius>` | `debug.wander` on that player's snapshot units |
  | `setowner <sim_id> <player>` | `debug.set_owner` |
  | `invuln <sim_id> on\|off` | `debug.invulnerable` |
  | `sethp <sim_id> <hp>` | `debug.set_hp` |
  | `teleport <sim_id> <x> <y>` | `debug.teleport` |
  | `player <id>` | sets `LocalPlayer` (client-local) |
  | `reveal` | toggles `RevealAll` |
  | `pause`, `resume`, `step [n]`, `speed <0.25..8>` | `LocalDriver` controls |
  | `hash` | `RunnerStatus.last_hash` |
  | `tick` | current tick |
  | `inspect <sim_id>` | sends `SimQuery::Entity`; the reply shows in the inspector window |

- **`DebugToolsPlugin`:**
  - It adds `EguiPlugin` only if absent, plus `FrameTimeDiagnosticsPlugin`.
  - It draws, in `EguiPrimaryContextPass`: the console (backquote or F1), the spawn panel, stats (FPS, tick, last hash, rules hash, unit count, local player, runner status), time controls and the inspector.
  - Click-to-place issues `debug.spawn` at `CursorGround`, unless egui wants the pointer.

- [ ] **Step 1: Write the failing tests** (World level, no egui; a fake runner port):
```rust
#[test] fn console_spawn_issues_debug_spawn()   // "spawn base.de.pz3 0 500 500" → LocalDriver.pending() has 1 command; type is "debug.spawn" in the header table;
                                                //   its payload decodes to DebugSpawnCmd { unit: "base.de.pz3", owner: PlayerId(0), .. }
#[test] fn console_unknown_command()            // "frobnicate" → "unknown command: frobnicate"
#[test] fn console_spawn_bad_args()             // "spawn base.de.pz3 0 x 1" → error text; no command queued
#[test] fn console_debug_cheats_map_to_kinds()  // setowner, invuln, sethp, teleport → kinds "debug.set_owner", "debug.invulnerable", "debug.set_hp", "debug.teleport"
#[test] fn console_player_switch()              // "player 1" → LocalPlayer(PlayerId(1)); nothing queued
#[test] fn console_step_while_paused()          // "pause", then "step 2" → exactly 2 TickInputs reach the fake runner
#[test] fn console_reveal_toggles_render_override()   // RevealAll false → true; nothing queued
#[test] fn console_inspect_sends_query()        // "inspect 3" → the fake runner receives SimQuery::Entity(SimId(3)); its reply lands in InspectorState.last
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_client --test debug`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** the tests. Expected: PASS.
- [ ] **Step 5: Gate** `cargo xtask gate task --track client`. Expected: PASS.
- [ ] **Step 6: Commit** `feat(fl_client): debug console, spawn panel, stats, time controls and inspector, all via commands, driver or queries` with trailer `Refs: ADR-0008`.

