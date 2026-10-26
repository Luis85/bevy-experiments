# Task 18 report: overlays and `fl.debug_query`

Worktree C:\fl\t18, branch m1/t18-overlays, commit 9dd7b1a (base 2782dc9).

## Implemented
- `crates/fl_client/src/overlays.rs`:
  - `OverlayState`, `BlockedCells`, `ClusterView`, `decode_paths`, `decode_passability`, `decode_clusters`, `grid_lines`, as in the brief.
  - Extra public helpers `blocked_near`, `POLL_FRAMES` (15), `VIEW_RADIUS_M` (120), `CURSOR_RADIUS_M` (60).
  - `OverlaysPlugin` registers `paths` (F5), `passability` (F6) and `cells` (F7) in `OverlayRegistry`.
  - An active overlay polls every 15 frames, immediately when switched on, never with a query in flight. Switching off abandons the pending query and drops the data.
  - Queries: `core.paths` `{"units":[selected, sorted]}` (none sent with an empty selection). `core.passability` and `core.clusters` take `{"unit": first selected}`, else `{"class":"Tracked","size":1}`.
  - Gizmos, run only when `GizmoConfigStore` exists:
    - paths: from the unit visual's `Transform`, then the corners from `next` on, lifted onto the terrain;
    - passability: X marks on blocked cells within 120 m of the camera focus;
    - cells: cell lines within 60 m of the cursor, cluster borders within 120 m of the focus, entrance markers.
  - Error replies keep the last data and log a warning once per distinct message.
  - Positions are raw `Fx` decoded with `Fx::from_raw(..).to_f32()`.
- `OverlaysPlugin` is added to `ClientPlugins`; `pub mod overlays` is in `lib.rs`.
- `debug/remote.rs`: BRP `fl.debug_query {name, args}` (args default `{}`; unknown fields rejected with -32602). It returns the reply JSON; a query `Err` becomes a JSON-RPC internal error carrying the query's message. Registered in `METHODS`, so it is also an agent tool.
- Tests:
  - `tests/overlays.rs`: 15 tests. The brief's 7, plus bad-step, blocked-near, no-selection, cells query, no overlapping polls, error reply, and stops after toggle-off.
  - `tests/remote.rs`: `brp_debug_query_round_trip`. The fake runner now answers `SimQuery::Debug`.
  - Overlay tests have no threads: the test holds the `runner_channel` end, reads `try_recv`, and a reply sent after a frame is read by the next.

## Results
- `cargo nextest run --profile agent -p fl_client --test overlays`: 15/15 pass.
- `cargo nextest run --profile agent -p fl_client --features debug_remote`: 150/150 pass (includes the BRP test). Output pristine.
- `cargo xtask gate task --track client`: fmt PASS, clippy PASS (fl_client clean), test-client PASS, data-check PASS. test-headless FAILED on two tests that fail on the base and that my diff cannot touch (no fl_sim or fl_test_utils change):
  - `smoke_sim_tick_system_order`: the protected snapshot needs a bless for Task 8's systems.
  - `smoke_counters_scale_500`: the budgets lack `path.*` counters and the alloc counts exceed.
  Last line: `gate task: FAIL (test-headless) in 396s`. The gate also reported 396 s against a 300 s budget (shared builder). I deleted the stray `system_order__sim_tick.snap.new` it wrote.

## TDD note
Implementation was written before the tests were run, so there is no RED run. The tests were written to the brief's assertions. They compile only against the new API, so they could not have passed before.

## Concerns
- The two headless gate failures above are from the base (Task 8 blesses and budgets pending), not from this task.
- `docs/debugging.md` is not updated. It has the `fl.*` method table and a `fl.toggle_overlay` section, but the file is outside the brief's Files list. Add the `fl.debug_query` row and the F5-F7 overlays there.
- Visual drawing (gizmos) is untested headless, as in the existing overlays (needs the GPU). The demo checklist covers it.
- `decode_paths` returns only corners, as specified. Internally `next` is kept so that passed corners are not drawn.
- The cell side for the overlays comes from `MapView.terrain.cell_m`, else map width over the cell count, because the debug queries carry no cell size.

Files: crates/fl_client/src/overlays.rs, src/lib.rs, src/debug/remote.rs, tests/overlays.rs, tests/remote.rs.

## Fix round 1
Changes (crates/fl_client): (1) `fl.debug_query` description rewrapped with `\` continuations; (2) overlay toggle keys (F3, F5-F7) run only when `InputGate::keyboard_blocked()` is false (`keyboard_free` run condition on `toggle_overlays_by_key`); (3) `OverlayData` (now public, read-only accessors `paths()`, `blocked_in_view()`, `view_rebuilds()`) caches the in-range blocked cells, rebuilt only on a new passability answer or an 8 m camera-focus step (`refresh_blocked_view`); draw loops the cache; (4) a selection change abandons all in-flight overlay queries (late replies dropped) and repolls at once; (5) an error or undecodable reply clears that overlay's data.
RED: new tests (`overlay_keys_ignored_while_console_open`, `error_reply_clears_the_overlay_data`, `reply_to_a_query_before_a_selection_change_is_dropped`, `blocked_cells_in_view_are_cached`) failed to compile before the change: `error[E0603]: struct OverlayData is private` (the accessors and the gate did not exist).
GREEN: `cargo nextest run --profile agent -p fl_client --test overlays`: 19/19 passed.
Gate: fmt, clippy, arch-check, check-scope, test-guard, test-client, data-check PASS; test-headless FAIL (known Task 8 blesses). `gate task: FAIL (test-headless) in 483s`.
