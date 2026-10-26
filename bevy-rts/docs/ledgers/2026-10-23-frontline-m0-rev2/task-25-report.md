# Task 25 report: live debug (BRP `fl.*` methods, agent tools, offscreen screenshot, perf panel)

Worktree `C:\fl\t25`, branch `m0/t25-brp`, base `8c7f0e5`. Commit `815f0d9`.
Status: DONE_WITH_CONCERNS (concerns at the end; none is about correctness of the shipped code).

## What was implemented

### fl_client
- `debug/remote.rs` (`cfg(feature = "debug_remote")`): `RemoteDebugPlugin { port }` (default 15702).
  - It adds `RemotePlugin` with all 17 `fl.*` methods (`with_method_main`, exclusive handlers `fn(In<Option<Value>>, &mut World) -> BrpResult`), then `BrpExtrasPlugin::with_http_plugin(RemoteHttpPlugin::default().with_port(port))`.
  - It registers every method as an `AgentTool` with a description that names its params. No schemas: `schemars` is not a dependency.
  - **Never in release:** in a release build it logs an error and adds nothing. This is a runtime check, not a `compile_error!`, so `--all-features --release` builds still work.
  - In a headless app it adds `InputPlugin` and a window-less `WindowPlugin` (`ExitCondition::DontExit`). Without them, the extras' input systems panic on missing messages.
  - Methods, as in the brief's table:
    - `fl.sim_state` returns the `WorldSummary` plus `last_hash` (hex), `paused` and `speed`.
    - `fl.entity`, `fl.journal_since`, `fl.save` and `fl.perf` go through `SimQuery`. `fl.perf` returns `{profiler, sim, frame}`.
    - `fl.issue_command` uses `ClientCommandCodecs`, then `LocalDriver::issue`.
    - `fl.spawn` issues `debug.spawn` as the local player.
    - `fl.pause`, `fl.resume`, `fl.set_speed` and `fl.step` are driver controls. `fl.step` pauses first if the game is running, polls and sends `n` inputs, waits until the last one is received, and returns `{tick}` = the next tick the sim runs.
    - `fl.screenshot`, `fl.toggle_overlay`, `fl.load_scenario` / `fl.load` (through `ScenarioLoader`), and `fl.save_scenario`.
  - **Waiting for the runner.** A method blocks on the main thread for at most 2 s (`REPLY_TIMEOUT`), reading `RunnerLink.port.rx`.
    - Replies are correlated by query id through `PendingReplies`.
    - `TickOutput`s and failures read meanwhile go to the new `RunnerBacklog`, which `drive_runner` drains first, so nothing is dropped or reordered.
    - A timed-out query is `abandon`ed: its late reply is dropped on arrival and never leaks into a later call.
    - `Instant` is used only inside `await_runner`, behind a scoped `#[expect(clippy::disallowed_types)]`.
- `debug/codecs.rs`:
  - `ClientCommandCodecs` maps each name to `{encode: fn(Value) -> Result<Vec<u8>, String>, debug_only}`.
  - `CommandCodecAppExt::register_command_json::<P>()`, and `issue_json(world, player, units, name, payload)`.
  - `DebugCommandsPlugin` registers `core.move`, `core.stop` and all six `debug.*` payloads.
  - An unknown name returns an error that names it and lists the known commands. A `debug.*` command is refused when cheats are off.
- `debug/screenshot.rs` (feature-gated):
  - Each request spawns an offscreen `Camera3d` with the `RtsCamera`'s transform and projection, a 1280×720 `Rgba8UnormSrgb` `Image` target and `order: -1`.
  - After 3 frames it spawns `Screenshot::image(handle)`, and the observer calls `save_to_disk(path)`, then despawns the camera.
  - If the captured image is a single colour (pipelines still compiling), it re-captures, up to 30 tries.
  - In an app that doesn't render (no `RenderApp`), it returns an error.
- `debug/perf.rs`: the perf panel, toggled with F2.
  - `ProfilerSource(pub fn() -> serde_json::Value)` provides the data; `fl_game` inserts it, because `fl_client` may not depend on `fl_observe` (arch rule: Client → BASE only).
  - `PerfTables` is a lenient `Deserialize` mirror of `PerfSnapshot`, sampled every 250 ms while the panel is open.
  - The panel shows the tick and phase table, the feature table and the 40 systems with the slowest p95 (p50/p95/max, a sparkline per row), FPS and frame-time graphs, the entity count (adds `EntityCountDiagnosticsPlugin`) and render stats (unit visuals total/visible, visible meshes).
  - `frame_stats(world)` returns the same numbers as JSON for `fl.perf`.
- `debug/mod.rs`:
  - `OverlayRegistry` + `OverlayAppExt::register_overlay(name, key, toggle)`, with a key system scheduled on the first registration.
  - `LoadRequest`, `LoadedMatch` and `ScenarioLoader` as in the brief.
  - `CurrentScenario(Option<String>)`, the scenario the match started from, which `fl.save_scenario` uses as its map/players/seed template.
  - `load_match(world, request)` swaps the `RunnerLink` (the old runner stops when its port drops), calls `LocalDriver::restore`, resets the snapshot buffer, age, status, backlog and journal buffer, and keeps pause and speed.
- `map_view.rs`: the grid is now the `"grid"` overlay (F3), registered through `OverlayRegistry`; the old `toggle_grid` key system is gone.
- `link.rs`:
  - `RunnerBacklog`, and `PendingReplies::abandon`.
  - `encode_command_bytes(tables, name, bytes)`; `encode_command` now uses it, with the same errors.
- `view.rs`: `drive_runner` handles `RunnerBacklog` before reading the channel.

### fl_game
- `builder.rs`:
  - `start_sim` is split into `set_up_sim` (shared with the loader), `run_sim` and `scenario_loader(features, content)`. The loader starts a scenario or calls `load_world` on a save.
  - It inserts `ScenarioLoader`, `CurrentScenario` and `ProfilerSource(profiler_json)`.
  - `.debug_remote(Some(port))` now adds `RemoteDebugPlugin` with the feature on, and only warns without it.
  - New `.hidden_window(bool)`: hides the window and sets `WinitPlugin { run_on_any_thread }`, for the GPU test, whose thread is not the main thread.
- `Cargo.toml`: `serde_json` (`profiler_json`) and dev-dependency `ron` (scenario round-trip check). The `Cargo.lock` delta is only those two lines in `fl_game` (R50).

## Tests
- `crates/fl_client/tests/remote.rs` (`#![cfg(feature = "debug_remote")]`, `MinimalPlugins`, a fake runner, raw HTTP over `TcpStream` with chunked decoding):
  - `brp_issue_command_stamps_and_queues`: returns `{exec_tick: 1, seq: 0}` and the command is held by the driver.
  - `brp_unknown_command_is_error`: the error names `core.fly`.
  - `brp_step_keeps_tick_outputs_for_the_view` (extra): step 5 then step 3 return consecutive ticks; after one update, `RunnerStatus.last_tick` is the last tick stepped, so no backlog output was lost.
  - `brp_methods_are_published_as_agent_tools` (extra): `brp_extras/agent_tools` and `rpc.discover` list the `fl.*` methods.
- `crates/fl_game/tests/remote.rs` (`#![cfg(feature = "debug_remote")]`):
  - `brp_step_state_command`: headless game, real threaded runner. `fl.step {n:5}` and `fl.sim_state` return tick ≥ 5; `core.move` for unit 1; `fl.step {n:40}`; `fl.entity` shows `core.pose` changed.
  - `brp_save_load_and_scenarios` (extra), in order:
    - `fl.save`, step, `fl.load` returns to the saved tick with the same `state_hash`;
    - `fl.load_scenario base.testbed` returns tick 0;
    - `fl.save_scenario` writes a file that `ron::Options::default()` parses as a `ScenarioDef` with 10 units on `base.testbed_1km`;
    - `fl.perf` has the sim counters and frame stats; `fl.journal_since` honours `limit`.
  - `gpu_screenshot_offscreen`:
    - Setup: full `DefaultPlugins`, hidden window, agent mode, run on a test thread. The cwd moves to a temp dir, so `logs/` is not written into the crate.
    - `fl.screenshot` writes a PNG larger than 1 KB, which is decoded and checked: not all black, more than one colour.
    - `brp_extras/shutdown` ends `app.run()`.
- Unit tests:
  - codecs: JSON to postcard equality; unknown, unknown-field, cheats-off and untabled errors.
  - perf: JSON decode and row history; `frame_stats` counts.
  - screenshot: single-colour detection; error without a renderer.
  - remote: every method is named `fl.*`, described and unique; params default to `{}` and reject unknown fields.

### TDD evidence
- RED, with the implementation stashed and the tests kept:
  - Command: `cargo nextest run --profile agent -p fl_client -p fl_game --features fl_game/debug_remote -E 'test(/brp_/)'`. Exit 101.
  - Output: `error[E0432]: unresolved import fl_client::debug::RemoteDebugPlugin`, `error[E0599]: no method named hidden_window found for struct GameBuilder`, and `unresolved import serde_json` (the fl_game dependency was not added yet).
  - Expected: the plugin, the builder option and the dependency did not exist yet.
- First GREEN attempt: all failed with `Encountered an error in system bevy_brp_extras::gamepad::process_timed_gamepad_button_releases: ... Message not initialized`, then the same for the `WindowEvent` and `CursorMoved` messages. Fixed by giving headless apps `InputPlugin` and a window-less `WindowPlugin`.
- GREEN, same command: `Summary [4.298s] 6 tests run: 6 passed, 77 skipped`. This includes `brp_save_load_and_scenarios`, which was added afterwards and passed.
- GPU: `cargo nextest run -p fl_game --features debug_remote -E 'test(/gpu_/)'` gives `PASS [10.436s] fl_game::remote gpu_screenshot_offscreen`, `1 test run: 1 passed`. Dev GPU is an RTX 1000 Ada.
- All tests of both crates with the feature: `cargo nextest run --profile agent -p fl_client -p fl_game --features fl_game/debug_remote` gives `82 tests run: 82 passed, 0 skipped`. This run came before `brp_save_load_and_scenarios` was added; that test passed separately.

### Clippy with the feature
`cargo clippy -p fl_client -p fl_game --features fl_game/debug_remote --all-targets -- -D warnings` gives `Finished`, with no warnings from these crates. The only warnings are the existing `fl_sim` `clippy.toml` "rand::rng / rand::random" path warnings, which are not from this task.

### Gate: `cargo xtask gate task --track client` (`.gate-base` = 8c7f0e5)
```
PASS fmt (2.6s)
PASS clippy-drift (3.5s)
PASS claude-md (0.0s)
FAIL check-scope (0.5s)
    1 of 13 changed files are outside track(s) client:
      Cargo.lock → contract
PASS protected (0.4s)
WARN test-guard (0.9s): test-guard: 2 findings (reviewer must accept each)
    TEST-GUARD crates/fl_client/tests/remote.rs:4 NewIgnore: #![cfg(feature = "debug_remote")]
    TEST-GUARD crates/fl_game/tests/remote.rs:4 NewIgnore: #![cfg(feature = "debug_remote")]
PASS clippy (53.1s)
PASS arch-check (5.3s)
PASS test-headless (184.1s)
PASS test-client (78.3s)
PASS data-check (57.9s)
WARN gate task took 388s (budget 300s)
gate task: FAIL (check-scope) in 388s
```
- check-scope: the `Cargo.lock` delta is only `fl_game`'s new `ron` and `serde_json` entries, which come from this task's manifest change (R50: the reviewer confirms and accepts).
- test-guard: both findings are the `#![cfg(feature = "debug_remote")]` the brief prescribes for these files.
- After the gate, only `cargo fmt` (no changes) ran; no code changed before the commit.

### `cargo xtask gate nightly`
```
PASS fmt (2.0s)
PASS clippy-drift (3.0s)
PASS claude-md (0.0s)
PASS test-headless (180.4s)
FAIL gpu (998.6s)
    error[E0432]: unresolved imports `fl_sim::GameFeature`, `fl_sim::SimConfig`
      --> probes\dynlink\src\main.rs:15:14
    error: could not compile `probe_dynlink` (bin "probe_dynlink" test) due to 1 previous error
PASS brp (95.7s)
gate nightly: FAIL (gpu) in 1280s
```
- `PASS brp` covers the `fl_game` `brp_*` tests.
- The gpu row fails because of an existing break outside this task: `probes/dynlink` (last touched by the T26 merge) uses the `fl_sim` API from before rev 2, and the row builds `--workspace` tests.
- Even with that fixed, the gpu row runs `--workspace` **without** `--features debug_remote`, so `gpu_screenshot_offscreen` (cfg-gated per the brief) is compiled out there. I ran it by hand (above).

## Files changed
- Created:
  - `crates/fl_client/src/debug/{remote,perf,screenshot,codecs}.rs`
  - `crates/fl_client/tests/remote.rs`
  - `crates/fl_game/tests/remote.rs`
- Modified:
  - `crates/fl_client/src/debug/mod.rs`
  - `crates/fl_game/src/builder.rs`
  - `crates/fl_game/Cargo.toml`
  - `Cargo.lock`
  - Outside the brief's Files list, with small necessary edits:
    - `crates/fl_client/src/link.rs`: `RunnerBacklog`, `PendingReplies::abandon`, `encode_command_bytes`.
    - `crates/fl_client/src/view.rs`: `drive_runner` drains the backlog. The brief requires that outputs read while a method waits are "queued for drive_runner".
    - `crates/fl_client/src/map_view.rs`: the grid moves into `OverlayRegistry`, so F3 and `fl.toggle_overlay` share one toggle.

## Concerns
1. **Blocking versus next-frame replies.** I followed the brief: methods block for at most 2 s, correlated by query id, and outputs go to the backlog. The controller preferred replying from the next frame. A deferred design would mean intercepting `BrpReceiver` ahead of `process_remote_requests`. That races with the HTTP thread, because messages that land between the two drains would hit stub handlers, and `bevy_remote` gives instant handlers no way to defer. The main thread stalls only for the runner's own answer time; for `fl.step`, that includes the ticks stepped.
2. **The client view lags `fl.step` by one frame.** `SnapshotBuffer` and `RunnerStatus` update when `drive_runner` drains the backlog on the next frame. So `fl.hash` right after `fl.step` reports the previous hash. `fl.save_scenario` reads the newest backlog snapshot, so it is not affected.
3. **New public interfaces not listed in the brief** (the minimum the arch rules and the methods need):
   - `ProfilerSource`, because `fl_client` cannot link `fl_observe`.
   - `CurrentScenario`, because `fl.save_scenario` needs the map and players, and neither the snapshot nor `WorldSummary` carries the map id.
   - `GameBuilder::hidden_window`, for the GPU test off the main thread.
   - `RunnerBacklog` and `PendingReplies::abandon`.
4. **A load keeps the map view and player colours.** `fl.load_scenario` and `fl.load` do not rebuild `MapView` or `PlayerColors`, which are fixed at startup, because `LoadedMatch` per the brief carries only the link, tick and commands. That is fine for M0, where there is one testbed map. After `fl.load` of a save, `fl.save_scenario` errors: there is no scenario template.
5. **Agent tools have no `params_schema`**, since that needs `schemars`, which is not a dependency. The descriptions spell out the params instead. `bevy_brp_extras` documents tools without a schema as "parameterless".
6. **gpu gate row:** see the nightly section. `probes/dynlink` is broken outside this task, and the row lacks `--features debug_remote`, so the brief's `PASS gpu` can't come from the gate as it stands. Fixing it is an xtask (contract) change.
7. **Not checked by hand:** the egui perf panel layout, and F2/F3 in a visible window. Only the panel's data paths are unit-tested.
8. The `gpu_` test calls `std::env::set_current_dir` to a temp dir, so the windowed run's `logs/<run-id>` is not created inside `crates/fl_game`. That is safe under nextest, which runs one process per test.

## Fix round 1 (commit d5ae651)

### Changes
1. **Bounded `fl.step`** (`debug/remote.rs`):
   - `n > 600` (`MAX_STEP`) is `INVALID_PARAMS` (-32602).
   - The method pauses, then sends one input at a time, with at most `STEP_WINDOW` = 8 in flight past the last one received, all inside one `Deadline` of 2 s.
   - Each runner read is a single bounded `recv_timeout` (`read_runner`). Ticks are counted from the tick read, so there are no backlog rescans; the old `await_runner` / `latest_tick` loop is gone.
   - It replies `{tick, completed, requested, finished}`.
   - **Timeout choice: queued.** Steps not yet sent stay in the driver (`driver.step(n - sent)`), so `drive_runner` runs them over the next frames. This is documented in the doc comment and the agent tool description.
   - A runner failure or disconnect is still an error.
   - `query()` uses the same single-read loop and abandons the query id on timeout or failure. `runner_failure` scans once per call.
2. **Fresh hash:**
   - `latest_received(world)` returns the newest tick and hash, `RunnerStatus` folded with the backlog in order.
   - `fl.hash` and `fl.sim_state.last_hash` use it, and the `fl.hash` description says the sim hashes every 20 ticks.
3. **Gate rows** (`xtask/src/gate.rs`, R68 and R43):
   - `DEVICE_FEATURES = "fl_game/debug_remote,fl_client/debug_remote"`.
   - `gpu_selection()` is `--workspace --exclude probe_* --features DEVICE_FEATURES`.
   - `brp_selection()` is `-p fl_client -p fl_game --features DEVICE_FEATURES`.
   - New unit test `device_rows_build_with_the_remote_feature`.
4. **Minor fixes:**
   - `remote` and `screenshot`, the `RemoteDebugPlugin` re-export, `add_remote_debug`, and both test files are gated on `cfg(all(feature = "debug_remote", debug_assertions))`. The runtime release check is removed.
   - `load_match` also resets `PendingReplies`.
   - The `fl.load` and `fl.load_scenario` descriptions, and the `load_match` doc, say they block until the new sim is built.
   - The long doc line in `view.rs` is re-wrapped, as are the long `json!` lines in the perf test.
   - New `pub` `OverlayToggles` set: `toggle_overlays_by_key` is in it, and `draw_grid` runs `.after(OverlayToggles)`.
   - New `GameBuilder::logs_dir(dir)` (default `logs`). `gpu_screenshot_offscreen` passes the absolute `<tempdir>/logs` path, and its `set_current_dir` calls are removed.

### New and changed tests
- `brp_step_keeps_tick_outputs_for_the_view` now checks `finished: true`, `completed: 3`, and that `fl.hash` returns `fake_hash(last stepped tick)` and `last_tick`. The fake runner now hashes every tick.
- `brp_hash_after_step_counts_the_backlog` calls the methods directly through `RemoteMethods`, with no frame between them. It asserts that the backlog is non-empty and that `fl.hash` returns the stepped tick's hash.
- `brp_step_is_capped`: `n: 601` returns -32602 and queues nothing.
- `brp_step_past_the_deadline_reports_progress_and_queues_the_rest`: a fake runner takes 50 ms per tick, and `n: 100` is requested.
  - The reply has `finished: false`, `requested: 100` and `0 < completed < 100`.
  - The remaining ticks then run over later frames, up to exactly `first + 99`.

### Evidence
- `cargo nextest run --profile agent -p fl_client -p fl_game --features fl_game/debug_remote -E 'test(/brp_/)'`: `9 tests run: 9 passed, 77 skipped`.
- `cargo nextest run -p fl_game --features debug_remote -E 'test(/gpu_/)'`: `PASS [43.490s] fl_game::remote gpu_screenshot_offscreen`. No `logs/` directory appeared under `crates/fl_game`.
- xtask gate tests (`-E 'test(/device_rows|gate/)'`): `9 tests run: 9 passed`.
- `cargo clippy -p fl_client -p fl_game --features fl_game/debug_remote --all-targets -- -D warnings`: `Finished`. The only warnings are the existing `fl_sim` `clippy.toml` path warnings.
- `cargo xtask gate task --track client --track contract`:
```
PASS fmt (1.9s)
PASS clippy-drift (3.3s)
PASS claude-md (0.0s)
PASS check-scope (0.3s)
PASS protected (0.3s)
WARN test-guard (1.1s): test-guard: 2 findings (reviewer must accept each)
    TEST-GUARD crates/fl_client/tests/remote.rs:4 NewIgnore: #![cfg(all(feature = "debug_remote", debug_assertions))]
    TEST-GUARD crates/fl_game/tests/remote.rs:4 NewIgnore: #![cfg(all(feature = "debug_remote", debug_assertions))]
PASS clippy (22.6s)
PASS arch-check (5.0s)
PASS test-headless (105.9s)
PASS test-client (55.0s)
PASS data-check (8.5s)
gate task: PASS in 205s
```
- `cargo xtask gate nightly`:
```
PASS fmt (6.5s)
PASS clippy-drift (5.9s)
PASS claude-md (0.0s)
PASS test-headless (102.3s)
PASS gpu (202.5s)
PASS brp (365.9s)
gate nightly: PASS in 684s
```
  The gpu row log shows `1 test run: 1 passed`, so `gpu_screenshot_offscreen` now runs in the gate.

### Remaining concerns
- The 2 s main-thread stall is still the design, but it is now bounded per call, `fl.step` included.
- The perf panel UI is still not checked by hand.
