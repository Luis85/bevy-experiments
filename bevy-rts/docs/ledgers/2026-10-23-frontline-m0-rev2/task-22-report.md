# Task 22 report: GameBuilder and the fl_game testbed binary

Worktree `C:\fl\t22`, branch `m0/t22-game` (from 611bc39). Commit `5ad8221`
`feat(fl_game): GameBuilder wiring the threaded runner to the client; testbed binary with stress and agent modes`.

## What I implemented

- `crates/fl_game/src/builder.rs`
  - `StartMode`, `GameBuilder` with the brief's full interface, and `check_client_features`, which errors with exactly `client feature for '<id>' has no SimFeature`. `"core"` always counts as present.
  - `prepare()` and `prepare_headless()` share one `build(Shell)` path:
    1. `load_content`, then `check_client_features`. The client features are sorted by the `sort_features` order of their sim features (stable when two features share one).
    2. `MatchSetup`: the scenario seed (`MatchSetup::for_scenario`) or the stress seed, with `cheats_allowed = true`.
    3. `build_sim_app`.
    4. `setup_scenario` or `ScaleScenario { units, seed, ..default }.populate`, which adds wander.
    5. `MapView` from the sim's `MapInfo`, using the scenario map's `ground_color` (stress uses a constant).
    6. `PlayerColors::from_factions` from the sim's `Players`. It is inserted at prepare time, before any `UnitView` exists.
    7. `SimRunner::new`, `next_tick`, the header, then `spawn_threaded`. The thread is started only after every fallible step, and it ends when the client drops its port.
  - The **windowed** client:
    - `DefaultPlugins`, with the window titled `Frontline — testbed`. In stress mode `PresentMode::AutoNoVsync`; otherwise the Window default.
    - `LogPlugin { filter, custom_layer: observe_layers, fmt_layer: stderr_layer }`.
    - `MeshPickingPlugin` and `ClientPlugins`. `WinitSettings::continuous()` in agent mode.
  - The **headless** client: `MinimalPlugins + SimViewPlugin + PlaceholderPlugin + DebugCommandsPlugin`. `CameraPlugin` needs `ButtonInput`, and `DebugToolsPlugin` needs egui, so neither runs without a window.
  - Then the client features, and the resources `RunnerLink`, `LocalDriver::new(next_tick)`, `ClientContent`, `LocalPlayer(PlayerId(0))`, `MapView` and `PlayerColors`.
  - Observability (windowed only):
    - `load_debug_config(None)`. In debug builds, `new_run_dir("logs")` + `set_global_run_dir`. Profiler budgets come from the config.
    - The config is stored in an `ObserveConfig` resource, because `custom_layer` is a fn pointer. `observe_layers` boxes `global_profiler().layer()` (filtered to `system`/`sim_tick`/`frame` spans and phase markers) and `json_file_layer(global_run_dir)`, as `debug.ron` asks.
    - The LogPlugin filter is `DEFAULT_FILTER + log_filter`, plus `fl_sim::phase=trace` when the profiler is on. This is needed because bevy_log's EnvFilter is a *global* layer, so TRACE markers would never reach the profiler otherwise. The stderr fmt layer and the JSON layer filter the markers back out.
    - The journal: a `PostUpdate` system `write_journal` reads `SimJournalMsg` and writes it through `JournalSink` to `journal.jsonl`. On an I/O error it warns once and stops.
    - A `Last` system opens and closes one `frame` span per frame, which is the T19 carried item.
  - `exit_after`: a `Last` system on `Time<Real>`. After the limit it writes `AppExit::Success` and prints `stress: units={n} avg_fps={:.1} p1_low_fps={:.1} frames={}` to stdout and to `info!`, ignoring frames in the first 3 s.
    - avg = frames / total time.
    - 1% low = 1 / (mean of the slowest ceil(n/100) frame times).
  - `debug_remote(port)` is stored. If set, it logs a warning that the endpoint is not available yet (Task 25).
- `crates/fl_game/src/lib.rs`: crate docs, `mod builder`, re-exports.
- `crates/fl_game/src/main.rs`: a clap CLI.
  - Flags: `--scenario <ID>` (default `base.testbed`, conflicts with `--stress`), `--stress <N>`, `--exit-after <SECS>`, `--mods <DIR>...`, `--remote [--remote-port <P>]` (default 15702; requires `--remote`), `--agent`.
  - `main() -> AppExit` returns `GameBuilder::run()`, which prints every error line and returns `AppExit::error()`.
- `crates/fl_client/src/lib.rs` needed no change: `ClientPlugins` already had the five plugins and every module was already exported.
- No `Cargo.toml` or `Cargo.lock` change: `tracing_subscriber` is used through `bevy::log::tracing_subscriber` (one version, 0.3.23, in the lock).

## Tests (13, all passing)

- `tests/builder.rs`:
  - `builder_rejects_broken_content`: the error names `bogus_component.unit.ron`.
  - **`orphan_client_feature_is_error`** (exact text).
  - `client_feature_for_core_is_accepted`
  - `unknown_scenario_is_error`
- `tests/wiring.rs` (threaded runner, real time, 10 ms sleeps, at most 200 updates):
  - `testbed_visuals_appear_headless`: 10 `UnitView`s, and two distinct faction colours.
  - `debug_spawn_round_trips_through_runner`: `spawn base.de.pz3 0 500 500` gives 11 views, with no runner failure.
  - `stress_mode_spawns_the_requested_units` (40 units).
- Unit tests in `builder.rs`:
  - `fps_stats_average_and_one_percent_low`
  - `exit_after_exits_and_skips_the_warm_up`: ManualDuration 100 ms. It exits at update 41 with 10 frames counted.
  - `journal_entries_are_written_to_the_run_dir`: tempdir; 3 entries over 2 frames give 3 lines in order.
  - `log_filter_adds_phase_markers_for_the_profiler`
- Unit tests in `main.rs`: `cli_is_consistent` (clap `debug_assert`) and `cli_parses_every_flag`.

### TDD evidence

RED: `cargo nextest run --profile agent -p fl_game`, with the tests written and the lib still a stub. This was expected, since the API did not exist:
```
error[E0432]: unresolved import `fl_game::GameBuilder`
error[E0432]: unresolved imports `fl_game::GameBuilder`, `fl_game::check_client_features`
error: could not compile `fl_game` (test "builder") due to 1 previous error
error: could not compile `fl_game` (test "wiring") due to 2 previous errors
```
That run also showed a borrow error in my test helper, which I fixed.

GREEN (same command, final tree):
```
Summary [   8.499s] 13 tests run: 13 passed, 0 skipped
```

### Windowed smoke run (by hand, Step 4)

This was not a visual inspection of pan, zoom and rotate; Task 30 owns that.

`cargo run -p fl_game -- --exit-after 8 --agent`:
- The window "Frontline — testbed" opened.
- Sim ticks ran: state-hash debug lines at ticks 0, 20, … 140.
- It exited 0, printing `stress: units=10 avg_fps=60.0 p1_low_fps=53.4 frames=301`.
- `logs/<run-id>/` held `log.jsonl` (15 lines) and `journal.jsonl`, which is empty because nothing emits journal records at this base.

`cargo run -p fl_game -- --stress 3000 --exit-after 8 --agent` (debug build, deps optimized) printed `stress: units=3000 avg_fps=85.1 p1_low_fps=69.9 frames=426` and exited 0.

I deleted the `logs/` directory afterwards; it is git-ignored.

### Gate: `cargo xtask gate task --track client` (`.gate-base` 611bc39)
```
PASS fmt (2.0s)
PASS clippy-drift (2.4s)
PASS claude-md (0.0s)
PASS check-scope (0.3s)
PASS protected (0.3s)
PASS test-guard (0.5s)
PASS clippy (187.4s)
PASS arch-check (3.6s)
WARN test-headless (857.5s): no test matched `(((package(=fl_sim) | package(=fl_test_utils)) & test(/smoke_/))) & not (test(/gpu_/) | test(/brp_/) | test(/milestone_/))`
PASS test-client (22.5s)
PASS data-check (108.3s)
WARN gate task took 1185s (budget 300s)
gate task: PASS in 1185s
```
The long times are cold builds in this build dir.

## Files changed
- Created: `crates/fl_game/src/builder.rs`, `crates/fl_game/tests/builder.rs`, `crates/fl_game/tests/wiring.rs`
- Modified: `crates/fl_game/src/lib.rs`, `crates/fl_game/src/main.rs`
- All LF.

## Concerns
1. **`RunConfig::phase_markers` does not exist at 611bc39.** Task 18 is not merged, so I used `RunConfig::default()`. When it lands, step 4 becomes `RunConfig { phase_markers: cfg.profiler, .. }`, which needs the config loaded before the sim is built (today it is loaded in the windowed branch). The filter plumbing for the markers (`fl_sim::phase=trace`) is already in place.
2. **The journal file stays empty** until the sim emits journal records (Task 18). The write path is unit-tested with synthetic `SimJournalMsg`s.
3. **`builder.rs` is 679 lines**, about 200 of them unit tests. The Files list allows only `lib.rs` and `builder.rs`, so the observability and `exit_after` helpers live there. They could move to an `observe.rs` if the plan allows it.
4. **Choices I made where the brief was silent:**
   - Headless uses `DebugCommandsPlugin` instead of `DebugToolsPlugin`, and has no camera, map view or logging setup. `prepare_headless` creates no run directory, so tests don't litter `logs/`.
   - `content_mod`: the first call replaces the default base mod, and later calls append. This mirrors the CLI's `--mods`.
   - The FPS line goes to stdout (`println!`) and to `info!` (so it lands in `log.jsonl`), so it appears twice in a terminal.
   - The stress ground colour is a constant, because the stress map has no map file.
5. Bevy facts for the reference:
   - `LogPlugin.fmt_layer: fn(&mut App) -> Option<BoxedFmtLayer>`. `PreFmtSubscriber` is private, but a generic `fmt::Layer::default().with_filter(..)` boxes into `BoxedFmtLayer` by inference.
   - bevy_log's `EnvFilter` is added as a global layer, so per-layer filters can't widen it.
   - `bevy::log::tracing_subscriber` is re-exported.
   - `bevy::picking::mesh_picking::MeshPickingPlugin`.
   - `App::should_exit()`.
   - `World::write_message_batch`.

---

## Fix round 1 (commit `87a1550` `fix(fl_game): profiler layers independent of the log filter`)

### What changed
1. **The profiler no longer depends on the log filter (R55).**
   - `fl_observe` (`logging.rs`, `lib.rs`) now exports two items:
     - `is_profiled(meta)`: the one predicate for the profiled spans (`system`, `sim_tick`, `frame`) and the `fl_sim::phase` markers. It replaces the private `profiler_interest`.
     - `observe_layers::<S>(cfg, run, profiler)`: stderr fmt plus `log.jsonl`, filtered **per layer** by `cfg.log_filter` (plus `sim_tick` spans); and the profiler layer, filtered only by `is_profiled`.
   - `init_headless` now builds its subscriber from `observe_layers` (same behaviour, one source of truth). This goes slightly beyond "exports only", but it avoids a second copy of the stack.
   - `fl_game`:
     - `custom_layer` = `client_layers` (boxed `observe_layers` with the global run dir and the global profiler).
     - `fmt_layer` = `no_fmt_layer` (an `Identity` layer), because stderr is now inside `observe_layers`.
     - The copied predicates, `stderr_layer` and the old `log_filter` are removed.
   - Bevy's `LogPlugin` filter is a **global** `EnvFilter`, and Bevy adds `RUST_LOG`'s directives to it last. So the plugin filter is now `DEFAULT_FILTER + cfg.log_filter + PROFILED_DIRECTIVES`, where the profiled directives are `bevy_ecs::system::function_system`, `fl_sim::step`, `fl_sim::phase` and `fl_game`, all at `trace`.
     - These are target-specific, so neither a plain `RUST_LOG` level nor a `"warn"` log filter can hide the profiler's input.
     - They are at TRACE, so they cap nothing the per-layer filter asks for. An earlier `fl_sim::step=info` capped the `state hash` DEBUG lines; the smoke run showed it.
   - I first tried a fully permissive plugin filter (level TRACE). Back-to-back runs looked slower (3000 units: 25–31 fps against 40 at INFO), but the machine was busy with other agents' builds, so the numbers are noisy. I chose the narrow filter anyway.
   - New test `profiler_ignores_a_warn_log_filter`, in `builder.rs`:
     - It builds the subscriber the way `LogPlugin` does: `Registry`, then the custom layer, then the global `EnvFilter` of `INFO,<plugin filter>` with `RUST_LOG=warn` added last. `debug.ron`'s log filter is `"warn"`.
     - It emits 3 `sim_tick` spans (target `fl_sim::step`), each wrapping a bevy_ecs `system` span, then a client `system` span and a `frame`.
     - Result: ticks == 3, the sim system has 3 samples and the client system 1.
     - Control: the same subscriber without the profiled directives gives ticks == 0 and no samples. So the test fails on the old behaviour.
2. **Client-feature ordering.** `sort_features` already returns `CoreFeature` at index 0, so `"core"` client features were already first. No logic change; the doc now says so. New test `client_features_follow_the_sim_feature_order`: sim features `a` (depends on `b`) and `b`; client features added as `[a, b1, core, b2]` are built as `[core, b1, b2, a]`.
3. `MatchSetup` for a scenario is now `MatchSetup { cheats_allowed: true, ..MatchSetup::for_scenario(&content, id)? }`. Stress keeps its own literal.
4. `player_colors` builds `PlayerColors` keyed by each player's real `PlayerId`, with an explicit palette-colour fallback (`PlayerColors::default().color(id)`). There is no `filter_map` or index shift any more.
5. The FPS line is printed once, to stdout only; the `info!` is gone.

### Tests run
`cargo nextest run --profile agent -p fl_game -p fl_observe`:
```
Summary [   9.360s] 28 tests run: 28 passed, 0 skipped
```
That is 14 in fl_game (−1 `log_filter_adds_phase_markers_for_the_profiler`, +2 new) and 14 in fl_observe. `cargo clippy -p fl_game -p fl_observe --all-targets -- -D warnings` is clean, apart from fl_sim's pre-existing `clippy.toml` notices.

Windowed smoke run, `cargo run -p fl_game -- --exit-after 6 --agent`:
- Exit 0, with the FPS line printed once.
- stderr shows the INFO lines and the `fl_sim::step` `state hash` DEBUG lines (`info,fl_sim=debug`).
- `log.jsonl` has the same lines.
- With `RUST_LOG=warn`, stderr is down to 8 lines and the exit is clean.

FPS figures from these runs are not meaningful: other agents' builds loaded the machine (3000 units varied from 10 to 41 fps between consecutive runs). Task 30 measures this.

### Gate: `cargo xtask gate task --track client --track tools` (`.gate-base` 611bc39)
```
PASS fmt (3.5s)
PASS clippy-drift (3.0s)
PASS claude-md (0.0s)
PASS check-scope (0.3s)
PASS protected (0.3s)
PASS test-guard (0.7s)
PASS clippy (24.5s)
PASS arch-check (7.5s)
PASS test-headless (96.8s)
PASS test-client (31.7s)
PASS data-check (21.0s)
gate task: PASS in 190s
```

### Notes
- A `RUST_LOG` that names one of the profiled targets itself at a lower level (e.g. `bevy_ecs::system::function_system=warn`) replaces that directive and still hides those spans. A plain level, or a crate-level directive like `fl_sim=debug`, does not.
- Bevy fact: `LogPlugin` parses `"{level},{filter}"` into a global `EnvFilter` and then adds each `RUST_LOG` directive, replacing an existing directive with the same target. Return `Some(Identity)` from `fmt_layer` to drop Bevy's own stderr formatter.
