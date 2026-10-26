# Task 21 report: integration (demo, milestone gate, perf, debugging guide)

**Status: DONE_WITH_CONCERNS.** Base `3d78b45` (.gate-base). Commits:

- `ebc1242` fix(fl_client): fl.issue_command codecs for core.rotate, core.follow and debug.set_terrain
- `0af0b65` docs: M1 debugging guide, demo checklist and perf results (`Refs: ADR-0012, ADR-0014, ADR-0015, ADR-0016`)
- `ac7a864` docs(perf): M1 render stress interleaved with the m0-tag game at equal load

`.new` files produced (never blessed): **`docs/perf/M1.json.new`** only. Nothing else protected changed.

## Gates

- `cargo xtask gate milestone --milestone M1` (run at `ebc1242` plus the uncommitted docs, 15 min 14 s wall):
  every row PASS (fmt 4.5 s, clippy-drift, claude-md, clippy 41 s, arch-check, test-headless 206 s, test-client 178 s,
  api 38 s, deny 26 s, data-check 18 s, **gpu 247 s**, **brp 146 s**) except
  `FAIL perf-file: docs/perf/M1.json does not exist` → last line `gate milestone: FAIL (perf-file) in 910s`.
  This is the expected consequence of "never bless": the lead blesses `docs/perf/M1.json.new` (its 3,000-unit row is
  the terrain run, mean 4.613 ms ≤ 15, checked with the gate's own rule), then only `perf-file` needs a rerun.
- `cargo xtask gate pr --track docs --track perf --track client --track contract`: `gate pr: PASS in 267s`.
  The `contract` track is needed because the spec is a contract path (first run without it: check-scope FAIL on the
  spec only). The spec edit only lists accepted ADRs/rulings (dispatch), so the commit carries `Refs:` to the M1 ADRs.

## What was done

### Fix found by the demo (out of the docs+perf track; client track)
BRP `fl.issue_command` answered "unknown command" for `core.rotate`, `core.follow` and `debug.set_terrain`: their
JSON codecs were never registered (`fl_client/src/debug/codecs.rs::register_builtin_codecs`). Three registrations
plus a test pinning the full codec list.
- RED: `cargo nextest run --profile agent -p fl_client -E 'test(every_order_and_debug_command_has_a_codec)'` →
  FAIL, left `["core.move", "core.stop", "debug.invulnerable", …]` (8 names), right the 11 names.
- GREEN: `cargo nextest run --profile agent -p fl_client -E 'test(/codecs/)'` → `3 tests run: 3 passed`.
- Not added (feature, not a bug): `fl.issue_command` has no `queue` flag, so Shift-queues come only from input.

### Docs
- `docs/debugging.md`: header note "hashes are examples, compare runs" and every M0 CLI example re-run and
  updated to M1 output (data-check with 8 terrain kinds, run-scenario, inspect with `core.path_follow`/order queue,
  journal, replay-verify of both goldens, replay dump format 3, save/load as a hash comparison); §1 console
  commands `terrain`, `reload`, `replay save`, `debug.ron` `hot_reload`/`hot_reload_poll_ms`, a keys table (F3,
  F5–F7 gated by InputGate, Home snap R87, R rotate, F follow, H, P/=/-, F4, selection and groups, Shift-queue cap,
  >256 split R88); §3 `query` and `map-gen` (with `--json` examples, the `map-gen testbed|scale --out
  assets/base/maps` regeneration command, byte-identity checked), `bench --flat`, `--baseline` refusal and
  `--force`; §4 `fl.debug_query`, `fl.select`, `fl.selection`, the brief's BRP smoke, M1 payloads incl. the fix,
  overlay names, `brp_extras` mouse/key methods for real input; §5 journal kinds; new §8 (journal kinds with real
  records, reject codes `core.invalid_target`/`core.queue_full`/… as SimEvent + journal + counter, counters, spans
  with fields, the five debug queries with request/response), §9 "why didn't my unit move" (core.paths →
  journal --kind core.path_result --sim-id N → core.passability {unit} → core.terrain_at), §10 hot reload (outcomes
  seen live, limits: dev builds only, R133 lockstep refusal, same-length same-mtime edits missed, terrain view
  follows set_terrain only in debug builds R134, R120, R121, reload replays exit 3).
- `docs/m1-demo-checklist.md`: 35 live rows + BRP smoke + content/determinism + ADR confirmations (one row per
  Confirmation bullet of ADR-0012, 0014–0016; every named test exists, checked by grep) + changed M0 rows; parked
  visual issues noted (camera focus y=0, half-cell border, triangle vs bilinear, flat rings, faint hill).
  Counts (rows 1–35): **31 PASS live** (3 qualified: hill visibility for a human to judge, Shift+click 4/5,
  testbed_ai at startup only), **4 PASS (test)**; ADR rows all PASS but ADR-0010 perf-file PENDING the bless;
  **4 human-needed items** (edge pan/mouse feel,
  typing into the console, judging how the terrain/hill reads, window-focus behaviour of the mouse).
  Screenshots `docs/demo/m1-01…m1-10.png` (quantized 960×540, ≈1.6 MB total).
- `docs/perf/M1.md` + `M1.json.new` (meta: commit/head `3d78b45`, date, machine, rustc, power plan, load,
  m0_comparison, render_stress).
- Spec "Revision 2.4 (M1 implementation refinements)": references only (R81, R83, R87–R99, R104–R107,
  R110–R117, R120, R121, R124, R125, R127–R131, R133–R136, R138–R142) grouped by ADR; status line updated.
- README: M1 status line, controls table, gate milestone M1, M1.md pointer.

### Perf (R128)
Power plan **Balanced** (`381b4222-…`), not changed (system setting). Machine idle for the sim bench: no
cargo/rustc, CPU 0–39 % (median 9 %) sampled every 3 s, 05:29–05:32 UTC. The `m0` tag (`0ae615d`) was built in
`C:\fl\m0bench` (own build dir), interleaved 5 rounds with M1 `--flat`, median of 5; worktree and build dir removed
(`xtask hook worktree-remove`, `git worktree remove`).

| units | M1 terrain mean / p95 ms | M1 flat (repeat 5) mean / p95 | M1 flat interleaved mean / p95 | M0 (`m0` binary, same machine) mean / p95 | M0.json (loaded) mean / p95 |
|---:|---:|---:|---:|---:|---:|
| 500 | 0.521 / 1.197 | 0.476 / 0.825 | 0.474 / 0.819 | 0.305 / 0.449 | 0.537 / 1.314 |
| 3000 | **4.613 / 6.286** | 4.482 / 5.932 | 4.463 / 6.008 | 2.828 / 3.612 | 4.431 / 7.955 |
| 5000 | 9.897 / 14.815 | 9.450 / 12.174 | 9.613 / 13.473 | 5.883 / 8.040 | 9.335 / 16.329 |

- G7 met: 4.613 ms at 3,000 units on terrain with pathing.
- M1 flat vs the M0 binary: **+55 / +58 / +63 %**, over the 20 % limit (vs M0.json, which was loaded, it passes
  `--baseline`). Per-system at 5,000 (+3.87 ms profiler tick): `compute_steering` +2.56 ms (new separation
  steering, ~50.6k `steer.pairs`/tick), `move_system` +1.04 ms (path/flow following, terrain speed, LOS
  look-ahead), orders + path queue +0.07, everything else ≤ +0.1. Justified in M1.md as new M1 work, not
  regression; tuning belongs with C7 crowd work.
- Scenarios (release, under foreign load): group_move 1.508 s, choke_crowd 0.921 s, flow_thrash 0.644 s per
  240-tick test (≤ 6.3 / 3.8 / 2.7 ms per tick); flow_thrash's thrash is gone (R124).
- Render stress (terrain mesh): 88.0 / 70.6 / 71.1 / 74.1 avg_fps (≥ 60 PASS); 1 % lows ≈ 2 fps. The m0-tag game
  shows the same ≈ 2 fps lows and, interleaved at equal load, the same average (58.5/60.3 vs 59.9/61.2).

### Content and determinism
- `fl_cli data-check assets/base` → exit 0, `units: 10`, `terrain: 8` (rules hash 0x0f29a5a56c793aa7).
- `FL_SHUFFLE_SEEDS=3 cargo nextest run --release --profile agent -p fl_sim -p fl_test_utils -E 'binary(movement_determinism)
  | binary(determinism) | binary(golden) | binary(m0_save) | test(/shuffle|twin|branch|round_trip|deterministic/)'` →
  `55 tests run: 54 passed, 1 failed`: `m0_testbed_save_loads_under_m1` fails its journal assert (journal off in
  release, R139), not a determinism failure. Full release run: `450 tests run: 410 passed, 40 failed` = the 40
  known journal-reading tests (Task 14 report).
- `replay-verify`: testbed_wander `verified: 400 ticks`, testbed_paths `verified: 900 ticks` (exit 0).
- Live: client hashes at ticks 0/20/40 = `hash-dump`; save/load mid-path (flow units) reproduces the hash 100
  ticks later.

## Carries (dispatch list)
All done: debugging.md items (fl.debug_query, fl.select/fl.selection, F3/F5–F7 + InputGate, Home R87, R rotate,
R88 split, map-gen regeneration command, fl_cli query, bench --flat / --baseline --force, hot reload usage and
limits incl. R134, same-mtime, R133; hash checks); checklist walked live with screenshots (testbed_ai checked at
startup only, as instructed); perf per R128 (except the fixed power plan: not permitted); determinism; gate
milestone (all rows but perf-file, pending bless); spec revision 2.4; README.

## Concerns
1. `gate milestone` cannot PASS until the lead blesses `docs/perf/M1.json.new`; then rerun (only perf-file changes).
2. M1's open-map tick is 55–63 % slower than M0 on an idle machine (steering + movement); justified, not fixed.
3. Out-of-track fix `ebc1242` (client) to make `fl.issue_command` accept the M1 commands.
4. Render 1 % lows ≈ 2 fps in both M0 and M1 builds on this machine today (never profiled); idle stress pair owed;
   stress runs after 06:04 UTC were under another project's cargo builds / background load.
5. The game hit a GPU `DeviceLost` (NVIDIA driver, wgpu panic in bevy_pbr clustering) once mid-demo, during
   screenshots while a release build ran; not reproduced in three later sessions.
6. Live demo found: one of five synthetic Shift+click-on-ground tries cleared the selection (input timing of the
   synthetic Shift; the rule is test-covered); the box-drag rectangle is not drawn (Task 15 known gap).
7. Saves are now ≈ 190 KB on the testbed (terrain grid is Hashed state) vs 4 KB in M0 — expected, noted in docs.

## Files
`crates/fl_client/src/debug/codecs.rs`, `README.md`, `docs/debugging.md`, `docs/m1-demo-checklist.md`,
`docs/perf/M1.md`, `docs/perf/M1.json.new`, `docs/demo/m1-*.png` (10), `docs/superpowers/specs/2026-10-23-frontline-rts-design.md`.
Raw logs (not committed): `C:\fl\a\target\t21\` (bench JSONs, gate logs, BRP session logs, stress outputs).
