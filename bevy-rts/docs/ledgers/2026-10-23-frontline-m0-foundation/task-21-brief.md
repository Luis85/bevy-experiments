### Task 21: Live debug mode — BRP methods, perf panel, render stats, trace features

Phase C2, after T14 and T15.

**Files:**
- Create `crates/fl_client/src/debug/{remote,perf}.rs` and `crates/fl_client/tests/remote.rs`.
- Modify `crates/fl_client/src/debug/mod.rs` (register submodules), `crates/fl_client/Cargo.toml` and `crates/fl_game/Cargo.toml` (features), and `crates/fl_client/src/game.rs` (`.debug_remote(true)` adds `RemoteDebugPlugin`).

**Features:**
- `fl_client` gets `debug_remote = ["bevy/bevy_remote"]`.
- `fl_game` gets `debug_remote = ["fl_client/debug_remote"]`, `trace = ["bevy/trace"]`, `trace_tracy = ["bevy/trace_tracy"]` and `trace_chrome = ["bevy/trace_chrome"]`. `fl_game` gains a direct optional `bevy` dep only if needed for the pass-through.

**Interfaces — Produces:**
- `RemoteDebugPlugin` (under `cfg(feature = "debug_remote")`) adds Bevy's remote plugin and HTTP plugin on `127.0.0.1:15702`, or `--remote-port`.
  - Before writing any code, check the bevy_remote 0.20 API: `cargo doc`, or read `~/.cargo/registry/src/*/bevy_remote-0.20.0`. Use its plugin names and custom method registration.
- Custom methods. Params and results are JSON. Commands go through `SimHost::issue`, never direct mutation, except the debug pause/step/speed controls:
  - `fl/sim_state` → `inspect_world`
  - `fl/entity {sim_id}` → `inspect_entity`
  - `fl/issue_command {player, units, command, payload}`: `command` is the command NAME (`core.move`) and `payload` is the payload as JSON, deserialized through a registry of JSON decoders built from `CommandRegistry` names. Each command kind registers a JSON decoder via `register_command`; if T7's registry lacks it, add `CommandRegistry::encode_json(name, serde_json::Value) -> Result<CommandKind, String>` in `fl_sim::commands` with a test.
  - `fl/pause`, `fl/resume`, `fl/step {n}` (returns the new tick), `fl/set_speed {x}`
  - `fl/spawn {unit, player, x, y}`
  - `fl/journal_since {tick}` → entries from `SimHost::journal()`
  - `fl/perf` → `global_profiler().snapshot()` plus client frame stats
  - `fl/screenshot {path}`: uses Bevy's `Screenshot` + `save_to_disk`; returns an error without a window.
  - `fl/toggle_overlay {name}`: `grid` (F3) and the others registered in an `Overlays` resource.
  - `fl/load_scenario {id}`: rebuilds the `SimHost`.
  - `fl/save_scenario {path}`: writes the current units as a `*.scenario.ron` that `fl_content` can load.
- Perf panel (`debug/perf.rs`, toggled with F2): an egui table of features, sets and systems with p50/p95/max, a sparkline per row from `history_ms`, frame-time and FPS graphs from `DiagnosticsStore`, entity count (`EntityCountDiagnosticsPlugin`), and render stats (visible unit visuals, draw-relevant entity count).

- [ ] **Step 1: Write the failing test** `remote_step_state_command` (`tests/remote.rs`, `#![cfg(feature = "debug_remote")]`).
  - Build a client `App` with `MinimalPlugins`, `SimViewPlugin` and `RemoteDebugPlugin` on a free port, with a SimHost from `base.testbed`.
  - Pump `app.update()` on the test thread while a spawned thread sends raw HTTP POSTs over `std::net::TcpStream`: JSON-RPC `fl/step {n:5}`, then `fl/sim_state` (tick ≥ 5), then `fl/issue_command` (core.move for unit 1), then `fl/step {n:40}` and `fl/entity {sim_id:1}` (position changed).
  - Run it with `cargo test -p fl_client --features debug_remote --test remote`.
- [ ] **Step 2: Run** the test. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** the test. Expected: PASS. `cargo run -p fl_game --features debug_remote -- --remote` serves `fl/sim_state`; check it with `curl`.
- [ ] **Step 5: Gate + commit** `feat(fl_client): BRP live debug methods, perf panel, trace features`. The gate also runs `cargo clippy -p fl_client --features debug_remote --all-targets -- -D warnings`.

---

