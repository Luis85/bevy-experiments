# Task 21 report: debug tools (console, spawn panel, stats, time controls, inspector)

Worktree `C:\fl\t21`, branch `m0/t21-debugtools` (from c1f0224). Commit `8e8c196`
`feat(fl_client): debug console, spawn panel, stats, time controls and inspector, all via commands, driver or queries`
(trailers `Refs: ADR-0008`, `Co-Authored-By`).

## What I implemented

- `debug/console.rs`
  - `ConsoleFn`, `ConsoleCommands` (a `BTreeMap<&'static str, (help, ConsoleFn)>` with `insert`, `get` and `iter`), `ConsoleAppExt::register_console_command` and `ConsoleState`.
  - `run_console_line` splits on whitespace and dispatches. It returns the output, `error: …` on failure, `unknown command: <name>`, or `""` for a blank line.
  - `issue_command(link, driver, player, units, &payload)`: refuses `DEBUG_ONLY` payloads when `header.cheats_allowed` is false, encodes with `encode_command`, then calls `LocalDriver::issue`. `issue_in_world` is the `&mut World` wrapper.
  - The built-in commands are all from the brief's table (`help` through `inspect`):
    - `spawn` is issued as the owner, with no units and heading 0. When `ClientContent` is present, the unit name is checked against it.
    - `wander <player> <radius>` takes that player's units from `SnapshotBuffer.curr`. It errors if the player has none.
    - `setowner`, `invuln`, `sethp` and `teleport` are issued as the unit's **current owner from the snapshot**, because the sim's `apply` keeps only units with `Owner == cmd.player` (`fl_sim/src/commands.rs:355`). They error if the unit isn't in the snapshot.
    - Coordinates and radius are parsed as decimals into `Fx::from_f64`.
    - `player` inserts `LocalPlayer`, and `reveal` toggles `RevealAll`.
    - `pause`, `resume`, `step [n]` and `speed` are `LocalDriver` calls. `step` errors while running, and `speed` errors outside 0.25..=8.
    - `hash` reports `RunnerStatus.last_hash`. `tick` reports `last_tick` and the driver's next tick. `inspect` calls `inspector::inspect`.
  - `toggle_console` (Backquote or F1).
  - `console_window` is an exclusive system: it clones the primary `egui::Context`, draws the window, and runs a submitted line with `run_console_line`. History is capped at 500 lines.
- `debug/inspector.rs`
  - `InspectorState`.
  - `inspect(world, sim_id)` sends `SimQuery::Entity` with `send_query` and sets `sim_id` and `pending`, clearing `last`.
  - `receive_inspector_reply` runs in Update after `SimViewSystems` and moves the reply into `last`. `Entity(Some(v))` becomes `v`, `None` becomes `null`, and errors become `{"error": …}`.
  - `inspector_window` has a sim id field and an Inspect button.
- `debug/spawn_panel.rs`
  - `CursorGround` and `SpawnPanel` (manual `Default`: owner 0).
  - `SpawnPanel::spawn_at(Vec3) -> Option<DebugSpawnCmd>`.
  - `update_cursor_ground`: the cursor ray of the active camera intersected with the y=0 plane. The window is accessed as `Option<Single<…>>`.
  - `click_to_place`: a left click issues `debug.spawn` via `issue_command`, skipped when `EguiWantsInput::wants_any_pointer_input()`. A right click stops placing.
  - `spawn_panel_window`: a unit combo from `ClientContent`, owner, heading slider, and the "place on click" checkbox.
- `debug/stats.rs`
  - `stats_window`: FPS from `DiagnosticsStore` / `FrameTimeDiagnosticsPlugin::FPS` smoothed, tick, last hash, rules hash, unit count, local player, and runner status (failed, paused, or running ×speed).
  - `time_controls_window`: pause/resume, step, and speed buttons from 0.25 to 8. These call `LocalDriver` directly.
- `debug/mod.rs`
  - **`DebugCommandsPlugin`** (an addition): the headless half. It holds the resources, the built-in commands and the reply routing, and needs no egui, window or input, so the tests use it.
  - **`DebugToolsPlugin`** adds `EguiPlugin::default()` and `FrameTimeDiagnosticsPlugin` only when absent (`is_plugin_added`), plus `DebugCommandsPlugin`. It also adds:
    - Update systems: the toggle, which runs if `ButtonInput<KeyCode>` exists, and cursor-ground → click-to-place.
    - The five windows, chained in `EguiPrimaryContextPass`. The console window runs only while it is open.

## Tests (`crates/fl_client/tests/debug.rs`, World level, no egui, FakeRunner on `runner_channel`)

- The 8 brief tests: `console_spawn_issues_debug_spawn`, `console_unknown_command`, `console_spawn_bad_args`, `console_debug_cheats_map_to_kinds`, `console_player_switch`, `console_step_while_paused`, `console_reveal_toggles_render_override`, `console_inspect_sends_query`.
- Extras:
  - `console_wander_targets_the_players_units`
  - `console_cheats_need_cheats_allowed`
  - `console_time_controls`
  - `console_help_lists_commands`
  - `console_hash_and_tick_report_runner_status`

### TDD evidence
RED: `cargo nextest run --profile agent -p fl_client --test debug`, with the tests written first:
```
error[E0432]: unresolved imports `fl_client::debug::DebugCommandsPlugin`, `fl_client::debug::InspectorState`, `fl_client::debug::run_console_line`
error: could not compile `fl_client` (test "debug") due to 1 previous error
```
This failure was expected, because `debug/mod.rs` was a stub.

GREEN (same command):
```
     Summary [   4.035s] 13 tests run: 13 passed, 0 skipped
```

### Gate (`cargo xtask gate task --track client`, `.gate-base` = c1f0224)
```
PASS fmt (4.8s)
PASS clippy-drift (11.7s)
PASS claude-md (0.0s)
PASS check-scope (0.4s)
PASS protected (0.3s)
PASS test-guard (0.5s)
PASS clippy (49.3s)
PASS arch-check (5.2s)
WARN test-headless (105.0s): no test matched `(((package(=fl_sim) | package(=fl_test_utils)) & test(/smoke_/))) & not (test(/gpu_/) | test(/brp_/) | test(/milestone_/))`
PASS test-client (32.6s)
SKIP data-check: crates/fl_cli/src/data_check.rs does not exist yet
gate task: PASS in 211s
```
`Cargo.toml` and `Cargo.lock` are untouched, all files are LF, and there is no `fl_sim` edge (`arch-check` passes).

## Files changed
- Created: `crates/fl_client/src/debug/{console,inspector,spawn_panel,stats}.rs`, `crates/fl_client/tests/debug.rs`
- Modified: `crates/fl_client/src/debug/mod.rs`

## Concerns / notes
1. **The egui UI is not exercised at runtime.** Nothing wires `ClientPlugins` into a windowed app yet (Task 22's `fl_game`), so the windows, the toggle, cursor-ground and click-to-place are compiled and clippy-clean but not run. Every egui and Bevy name was checked in the sources (`bevy_egui-0.43.1`, `egui-0.36.2`, `bevy_shape` `Ray3d::plane_intersection_point`, `Camera::viewport_to_world`, `App::is_plugin_added`).
2. **Additions beyond the brief's interface:**
   - `DebugCommandsPlugin`, the headless split described above.
   - `issue_command` / `issue_in_world`, `inspect`, `SpawnPanel::spawn_at`.
   - `ConsoleCommands::{insert, get, iter}`.
3. **The issuing player for unit cheats** is the snapshot owner, not `LocalPlayer`, so that the sim's ownership filter accepts the command. `spawn` is issued as its `owner`. This depends on the sim registering that player in `Players`.
4. **Bevy facts to record** (the reference is outside the client track):
   - `bevy_egui::input::EguiWantsInput::wants_any_pointer_input()`.
   - The primary context from an exclusive system: `world.query_filtered::<&mut EguiContext, With<PrimaryEguiContext>>()`, then `.get_mut().clone()`.
   - `Ray3d` lives in `bevy_shape` 0.20 (in the prelude): `ray.plane_intersection_point(Vec3::ZERO, InfinitePlane3d::new(Vec3::Y))`.

---

## Fix round 1 (commit `6355a20` `fix(fl_client): debug commands issued as LocalPlayer (R44)`)

### What changed
1. **R44: every `debug.*` command is issued as `LocalPlayer`.**
   - `issue_in_world(world, units, &payload)` dropped its `player` argument. It now reads `LocalPlayer` and errors with "there is no local player; run 'player <id>' first" when it is absent.
   - `snapshot_owner` is removed. `setowner`, `invuln`, `sethp` and `teleport` may target any sim id, including one missing from the snapshot.
   - `spawn`: the `<player>` argument is only `DebugSpawnCmd.owner`. Its help line now says "owned by <player>".
   - `wander <player> <r>`: the player argument only selects that player's units from the snapshot.
   - `click_to_place` issues as `LocalPlayer` (and logs a warning when there is none). `SpawnPanel.owner` stays the payload's owner.
2. **Tests** (`tests/debug.rs`):
   - `client_app` inserts `LocalPlayer(PlayerId(0))`.
   - The cheats test now asserts `PlayerId(0)` while the units are owned by player 1. `sethp 9 10` (a unit not in the snapshot) is now accepted, and only the bad flag `invuln 3 maybe` errors.
   - The wander test asserts the issuer is `PlayerId(0)` and the units are player 1's.
   - New `console_debug_commands_issued_as_local_player`:
     - `player 1` + `spawn base.de.pz3 0 1 1` is issued by `PlayerId(1)`, with payload owner `PlayerId(0)`.
     - `player 0` + `sethp 7 5` is issued by `PlayerId(0)`.
     - Without `LocalPlayer`, a cheat errors and queues nothing.
3. **Toggle key.**
   - The `retain('`')` hack is removed.
   - `toggle_console` sets a private `ConsoleState.toggled` flag on exactly the frames where Backquote or F1 is just pressed. It writes only on change.
   - On those frames, `console_window` removes `egui::Event::Text("`")` from the context's input (`ctx.input_mut(|i| i.events.retain(..))`) before drawing the text field.
   - The schedule order is checked: `toggle_console` runs in Update, and `EguiPrimaryContextPass` runs in the same frame's PostUpdate (`run_egui_context_pass_loop_system` in `EguiPostUpdateSet::EndPass`).
   - This path is not run in a windowed app (same caveat as before).

### Tests run
`cargo test -p fl_client --test debug`:
```
test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s
```
`cargo xtask gate task --track client`: the first run failed in `test-client` on `tests/view.rs::despawned_record_removes_placeholder_meshes` (`left: 2, right: 3` at view.rs:801). That test is Task 20's and was not touched. Run alone 6 times, it failed 2 of the 6, so it is flaky (details below). Second gate run:
```
PASS fmt (1.6s)
PASS clippy-drift (2.2s)
PASS claude-md (0.0s)
PASS check-scope (0.2s)
PASS protected (0.3s)
PASS test-guard (0.5s)
PASS clippy (5.9s)
PASS arch-check (4.4s)
WARN test-headless (4.9s): no test matched `(((package(=fl_sim) | package(=fl_test_utils)) & test(/smoke_/))) & not (test(/gpu_/) | test(/brp_/) | test(/milestone_/))`
PASS test-client (20.0s)
SKIP data-check: crates/fl_cli/src/data_check.rs does not exist yet
gate task: PASS in 41s
```

### New concern: a flaky Task 20 test
- **The race.** `drive_runner` drains the runner channel right after sending the frame's ticks. If the fake runner answers quickly, the tick sent in the same update is already current. So after 3 updates the snapshot is sometimes tick 1 (2 records) rather than tick 0 (3 records).
- **Suggested fix** (not in this task's files): assert on the snapshot's tick, or have the fake runner wait for the test's go-ahead before answering.

---

## Fix round 1b (commit `test(fl_client): deterministic fake-runner timing in view tests`)

### What changed
- **The race.** `FakeRunner::update` waited for each tick's answer *after* `app.update()`. But `drive_runner` drains the channel inside the same update, right after sending. A fast runner thread's answer could therefore be received in the frame that sent the tick, or in the next frame. This made the frame's snapshot one tick early, nondeterministically.
- **The fix**, in the `FakeRunner` of both `tests/view.rs` and `tests/debug.rs`:
  - The runner thread now holds each tick's answer until the test releases it, through a `release` channel.
  - `update()` runs `app.update()`, then, for each tick sent, releases one answer and waits for its ack. Every tick's answer is therefore received exactly in the next frame.
  - `Drop` drops the release sender first, so a held answer can't block the join.
  - Queries in `debug.rs` are still answered at once, since `console_inspect_sends_query` waits on its ack directly.
- **Affected tests.**
  - The race affected every `FakeRunner` test. Only assertions on "which snapshot is current after N updates" were sensitive to it: `despawned_record_removes_placeholder_meshes` and `despawned_record_removes_visual` (same pattern, same flake), plus `visuals_follow_snapshots`.
  - The `debug.rs` tests weren't sensitive (they count inputs, or read `RunnerStatus` consistently), but they now use the same deterministic runner.
  - No assertions changed.

### Evidence
- `cargo test -q -p fl_client --test view despawned_record_removes_placeholder_meshes`, 20 consecutive isolated runs: `pass=20 fail=0`. Before the fix it failed 2 of 6 runs.
- `cargo test -q -p fl_client --test view --test debug`, 20 consecutive runs: `pass=20 fail=0` (28 + 14 tests each).
- Gate:
```
PASS fmt (1.3s)
PASS clippy-drift (3.7s)
PASS claude-md (0.0s)
PASS check-scope (0.5s)
PASS protected (0.5s)
PASS test-guard (0.8s)
PASS clippy (10.3s)
PASS arch-check (31.7s)
WARN test-headless (89.8s): no test matched `(((package(=fl_sim) | package(=fl_test_utils)) & test(/smoke_/))) & not (test(/gpu_/) | test(/brp_/) | test(/milestone_/))`
PASS test-client (28.1s)
SKIP data-check: crates/fl_cli/src/data_check.rs does not exist yet
gate task: PASS in 167s
```
