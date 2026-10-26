### Task 14: Debug tools (egui console, spawn panel, stats, time controls)

**Files:** `crates/fl_client/src/debug/{mod,console,spawn_panel,stats}.rs`, and no others.

**Interfaces — Consumes:** `SimHost`, `LocalPlayer`, `ClientContent`, `RtsCamera`. **Produces:**
```rust
pub type ConsoleFn = fn(&mut World, &[&str]) -> Result<String, String>;
#[derive(Resource, Default)] pub struct ConsoleCommands { /* BTreeMap<&'static str, (help, ConsoleFn)> */ }
pub trait ConsoleAppExt { fn register_console_command(&mut self, name: &'static str, help: &'static str, f: ConsoleFn) -> &mut Self; }
#[derive(Resource, Default)] pub struct ConsoleState { pub input: String, pub history: Vec<String>, pub open: bool }
pub fn run_console_line(world: &mut World, line: &str) -> String;   // tokenises, dispatches, "unknown command" otherwise
#[derive(Resource, Default)] pub struct CursorGround(pub Option<Vec3>);
#[derive(Resource)] pub struct SpawnPanel { pub unit: Option<UnitTypeId>, pub player: PlayerId, pub placing: bool, pub heading_deg: i32 }
```
**Built-in console commands:**
- `help`
- `spawn <unit_id> <player> <x> <y>`
- `wander <player> <radius>`
- `player <id>`: sets `LocalPlayer`.
- `pause`, `resume`, `step [n]`
- `speed <0.25..8>`: sets `Time<Virtual>::set_relative_speed`.
- `hash`: prints `compute_state_hash` of the host world.
- `tick`

**`DebugToolsPlugin`:**
- Adds `EguiPlugin::default()` only if it isn't already added, plus `FrameTimeDiagnosticsPlugin`.
- Draws the egui windows in `EguiPrimaryContextPass`:
  - the console, toggled with backquote or F1
  - the spawn panel
  - stats: FPS, sim tick, last tick ms, unit count, last logged state hash, rules hash, local player
  - time controls: pause, step, speed
- **Click-to-place.** While `placing`, a primary click that isn't over egui (`ctx.wants_pointer_input()` is false) issues `DebugSpawnCmd` at `CursorGround` for the panel's player.
- `CursorGround` is updated every frame by the ray-to-plane code in api-reference §2.5.

- [ ] **Step 1: Write the failing tests.** They work at World level, with no egui.
  - `console_spawn_queues_command`: with a SimHost on base.testbed, `run_console_line(w, "spawn base.de.pz3 0 500 500")` followed by one `host.step()` gives 11 units.
  - `console_unknown_command`.
  - `console_spawn_bad_args`: `spawn nope 0 1 1` returns error text and does not panic.
  - `console_player_switch`, `console_step_while_paused`.
- [ ] **Step 2: Run** `cargo test -p fl_client debug`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test -p fl_client debug`. Expected: PASS.
- [ ] **Step 5: Gate + commit** `feat(fl_client): egui debug console, spawn panel, stats and time controls`.

