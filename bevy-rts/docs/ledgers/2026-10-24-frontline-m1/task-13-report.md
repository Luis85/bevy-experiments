# Task 13 report: scale scenario on terrain with pathing; counters, budgets, bench

**Status:** DONE_WITH_CONCERNS · worktree `C:\fl\t13`, branch `m1/t13-scale`, base `53ddd6e` · commit `70a9b3b`
(`feat(perf): scale entity mix on the 2 km terrain with pathing; M1 counter budgets; bench --flat`, `Refs: ADR-0010`).
No `Blessed:` trailer: every protected artifact is left as `.new` for the lead (list below).

## What I implemented

- **`fl_sim::scale`**: `ScaleScenario.map: Option<String>` (Default and `with_units`: `None`, M0's open map),
  `pub const SCALE_MAP = "base.scale_2km"`, `ScaleScenario::on_terrain(units)` (= `with_units` + `map: Some(SCALE_MAP)`).
  `populate` with a map: looks it up (`SetupError::UnknownMap`), checks its side equals `map_m`
  (`SetupError::MapSizeMismatch { map, map_m }`), both before the world changes; uses the map's `MapInfo` (size,
  cell) and `init_terrain(world, Some(def))` (TerrainGrid from the layers + nav/HPA/flow rebuild). Placement: all
  unit positions are drawn first (borrowing `NavGrid`), each unit's profile from `def_profile`; a point whose cell
  is not passable is redrawn (x then y, from the same `placement` Rolls under the existing scoped `#[expect]`) up
  to 16 times, then `nearest_passable` (radius = map side in cells) → cell centre. No `rolls()` / new `Stream`.
  On the open map nothing is ever redrawn, so the flat hash stream is M0's.
- **`fl_sim::scenario`**: new `SetupError::MapSizeMismatch` variant (out of the Files list, required by the brief's
  interface). **`fl_sim::lib`**: re-export `SCALE_MAP`. **`fl_sim::replay`**: `REPLAY_FORMAT_VERSION` 2 → 3 (R62);
  `tests/replay.rs` literal gains `map: None` (no test pins the number).
- **`fl_test_utils::perf`**: new `CounterSim` (a built sim + its scheduled `PlayerCommand`s) and `run_counters`;
  `run_counter_scenario(units, …)` now = `run_counters(CounterSim::scale(units), …)` (on terrain). Scenarios:
  - `CounterSim::scale(units)`: `ScaleScenario::on_terrain(units)`.
  - `CounterSim::group_move()` (Task 10 carry): `on_terrain(300)` without followers/projectiles/sensing/wander; at
    ticks 0, 50, …, 200 every player orders all its units (one `core.move`, one group request per profile) to a
    quadrant centre shifted `80r − 160` m, so every round builds new fields (a first version reusing 4 targets
    measured no `flowfield.abstract_nodes` at all: all 12 keys stayed cached).
  - `CounterSim::flow_thrash()` = `CounterSim::flow_groups(36)` (Task 10 carry): 128×128-cell ASCII grass map
    (8×8 clusters), 36 groups × 6 `t.rifleman`, spawned member-major (consecutive SimIds in different groups),
    ordered at tick 40 to 36 distinct clusters → 36 live flow keys vs `flow_cache_max` 32.
  - `CounterSim::choke_crowd()` (R117): 64×32 ASCII map, water funnel narrowing from 28 cells (column 18) to a
    2-cell gap (columns 31–32), 192 `t.tank`s, one `core.move` at tick 0 to just past the gap.
  - `scale_path_charges(units, warmup, ticks) -> Vec<TickCharges>` (journal on) for the budget test.
  - All runs build their own sim, so every snapshot starts with empty navigation caches (R86).
- **`fl_test_utils/tests/perf_counters.rs`**: `gate(scenario, impl FnOnce() -> CounterSim)`; the three scale
  tests now on terrain; new `counters_group_move`, `counters_flow_thrash`, `counters_choke_crowd`,
  `path_budget_holds_at_scale`. `fl_test_utils/src/lib.rs` re-exports (out of list, mechanical).
- **`fl_cli` bench**: `BenchConfig.flat`; default runs `on_terrain`, `--flat` runs `with_units` (open map).
  `BenchRow.map` (`"base.scale_2km"` | `"open"`, `#[serde(default = "open")]` so `docs/perf/M0.json` rows still
  parse as open); `meta.map`; the markdown table gains a map column; a baseline on another map warns on stderr
  (like the build-type warning). `main.rs` gains `--flat` (out of the Files list; needed for the flag).
- **`fl_game` builder**: stress mode runs `ScaleScenario::on_terrain(units)`; `MapView.terrain`/ground colour come
  from the map def via `TerrainView::from_content`, one code path shared with scenario starts.

## Tests

New/changed: `scale_mix.rs` — `scale_units_start_on_passable_cells` (3000 units; also asserts the map has water),
`scale_pathing_reaches_targets` (500 units, 400 ticks: every unit gets ≥ 1 non-stale `core.path_result`; no unit
on a blocked cell after any tick), `flat_scale_has_open_terrain`, `scale_map_size_must_match` (also: world
unchanged), `scale_mix_twin_run` (now on terrain, via `ScenarioBuilder`). `perf_counters.rs` as above.
`fl_cli/tests/bench.rs` — `bench_flat_flag_uses_open_map` (CLI `--flat` → `meta.map`/`rows[].map` "open"; a row
without `map` parses as "open"); `bench_reports_each_size` asserts `map == "base.scale_2km"`. `fl_game/tests/
builder.rs` — `stress_map_view_has_scale_terrain` (cells (500, 500), size 2000 m); `loading_a_scenario_replaces_
the_map_view` precondition changed from "stress has no terrain" to "stress has the 500×500 terrain" (test-guard
RemovedAssert finding #1, the rewrite keeps the test meaningful: the load replaces 500×500 with 250×250).
test-guard finding #2: `scale_mix_twin_run`'s `all(Option::is_some)` became `assert_eq!(a.len(), 200)` on
`Scenario::hashes()` (hash every tick).

### TDD evidence
- RED (WIP commit, `crates/fl_sim/src` + `crates/fl_game/src/builder.rs` checked out at `53ddd6e`, then restored):
  - `cargo nextest run --profile agent -p fl_sim --test scale_mix` → `error[E0599]: no associated function or
    constant named 'on_terrain' found for struct 'ScaleScenario'` (expected: the API does not exist yet).
  - `cargo nextest run --profile agent -p fl_game --test builder -E 'test(/stress_map_view/)'` →
    `FAIL stress_map_view_has_scale_terrain … panicked at builder.rs:130: the stress map is the 2 km terrain map`
    (expected: stress ran on the open map, terrain None).
- GREEN: `cargo nextest run --profile agent -p fl_sim --test scale_mix` → `8 tests run: 8 passed`; the builder test
  passes in the full run below.

### Full run (Step 6)
`cargo nextest run --profile agent --no-fail-fast -p fl_sim -p fl_test_utils -p fl_catalog -p fl_cli -p fl_game`
→ `Summary [85.920s] 475 tests run: 467 passed, 8 failed`. The 8 are exactly the bless-pending artifacts:
`fl_test_utils::golden golden_replays_verify` (replay format 2 vs 3), `fl_sim::formats save_codec_formats`,
`perf_counters` `smoke_counters_scale_500`, `counters_scale_3000`, `milestone_counters_scale_5000`,
`counters_group_move`, `counters_flow_thrash`, `counters_choke_crowd` (snapshot `.new` / old budgets). After the
commit only `bench.rs`'s `OPEN_MAP` const moved below the imports (no behaviour change).

**Budget sanity check (Step 6):** a temporary core system in `ThinkSet::Orders` enqueuing a path for every scale
unit every tick, checked against `budgets.ron.new`: `scale_500` fails with `path.budget_spent max 8718 > budget
1792`, `path.queue_len max 371 has no budget`, `path.requests max 513 > budget 7`, `path.nodes_expanded`,
`path.los_cells`, `alloc.*`. Removed afterwards (`grep temp_flood` → 0).

### Gate (Step 7)
`cargo xtask gate task --track sim-core --track tools --track perf --track client` (after the commit):
```
PASS fmt (2.0s)
PASS clippy-drift (2.2s)
PASS claude-md (0.0s)
FAIL check-scope (0.2s)
    2 of 24 changed files are outside track(s) sim-core, tools, perf, client:
      crates/fl_sim/src/replay.rs → contract
      xtask/snapshots/api/fl_sim.txt.new → contract
PASS protected (0.2s)
WARN test-guard (1.8s): test-guard: 2 findings (reviewer must accept each)
    TEST-GUARD crates/fl_game/tests/builder.rs:97 RemovedAssert: assert!(app.world().resource::<MapView>().terrain.is_none());
    TEST-GUARD crates/fl_sim/tests/scale_mix.rs:130 RemovedAssert: assert!(a.iter().all(Option::is_some), "hashed every tick");
PASS clippy (47.0s)
PASS arch-check (3.3s)
FAIL test-headless (209.8s)   (bless-pending only: golden, formats, perf_counters snapshots/budgets)
PASS test-client (55.0s)
FAIL api (54.2s)              (fl_sim changed: .txt.new awaits bless)
PASS data-check (67.5s)
WARN gate task took 444s (budget 300s)
gate task: FAIL (check-scope, test-headless, api) in 444s
```
- check-scope: the brief's own Files list includes `replay.rs` (replay format = contract) and the fl_sim API
  snapshot; with `--track contract` added: `check-scope: OK (24 files in sim-core, tools, perf, client, contract)`.
- test-headless failed only on bless-pending tests (fail-fast; the full no-fail-fast run is above).

## Protected artifacts (all `.new`, for the lead)
Bless reason suggestion: `M1 Task 13: scale mix on the 2 km terrain with pathing`.
- `docs/perf/budgets.ron.new` (recorded with `FL_RECORD_BUDGETS=1`, stale `.new` deleted first)
- `crates/fl_test_utils/tests/snapshots/perf_counters__{scale_500,scale_3000,scale_5000,group_move,flow_thrash,choke_crowd}.snap.new`
- `crates/fl_sim/tests/snapshots/formats__save_codec_formats.snap.new` (REPLAY_FORMAT_VERSION 3, `ScaleScenario.map`)
- `xtask/snapshots/api/fl_sim.txt.new` (`MapSizeMismatch`, `ScaleScenario::map`, `on_terrain`, `SCALE_MAP`)
- `crates/fl_test_utils/tests/replays/testbed_wander.replay.new`, recorded at `70a9b3b` (`FL_RECORD_GOLDEN=1`):
  header-only — byte 0 version 2→3 and the build id `0.0.0+4a92672` → `0.0.0+70a9b3b` (same length); the last
  6400 bytes (inputs and hash stream) are byte-identical to the blessed file. (The testbed scenario is not a
  scale run, so its hash stream does not change.)

## Carried items, with numbers

All counter numbers are exact (per-tick maxima / totals over 200 measured ticks after 40 warmup).

- **R98/R112 (HPA/flow among top systems at 3,000 units on the scale map): no.** Release `--profile` at 3000 units
  on terrain, system means: `scale_sense` 1.450 ms, `move_system` 1.140, `compute_steering` 1.073, `follow_system`
  0.254, `track_spatial` 0.201, `publish_snapshot` 0.095, **`process_path_queue` 0.065 (7th, ~1.4 % of the tick)**,
  `spawn_funnel` 0.054 …; `prepare_flow_tiles` 0.004. Scale mix at 3000: `path.requests` ≤ 19/tick,
  `path.nodes_expanded` ≤ 1410, `path.budget_spent` ≤ 2721 of 8000. The parked buffer/relink fix does not trigger.
  At **5000** units the queue backs up: `path.queue_len` max 1031/tick, `path.requests` max 108, `path.budget_spent`
  max 8876 (the R98 "≥ 1 request per tick" overshoot); `process_path_queue` is still 0.153 ms (8th).
- **Task 10 carry, group moves (`group_move`):** budgets `flowfield.abstract_nodes` 10555 (max 8444/tick, total
  202656), `path.budget_spent` 10555 (max 8444: one cold group charge ≈ 2.8k nodes, 3 per tick; the first charge
  of a tick may exceed the 8000 budget, R98/R112), `flowfield.cells` max 18560, `path.queue_len` max 10.
- **Task 10 carry, >32 live flow keys (`flow_thrash`, 36 keys): THRASH SHOWS.** Steady state (ticks 140–239, no
  group request in flight): 22,932 `flowfield.abstract_nodes` and 23,296 `flowfield.cells` **every tick** —
  ≈ 91 integrations (252 nodes each) and ≈ 91 tiles rebuilt per tick, for 216 units. The same scenario with 32
  keys (`CounterSim::flow_groups(32)`, diagnostic run, not committed): 8064 abstract nodes once at the request
  tick and **0 per tick afterwards**. Cost: 1.72 s vs 0.08 s for 240 ticks (opt+debuginfo test profile);
  `alloc.bytes` per tick max ≈ 750 KB, `alloc.count` ≈ 2050. An earlier draft on the 2 km map (60 keys, 480
  units) rebuilt ≈ 1.25 M abstract nodes per tick (≈ 350 ms/tick, 16 MB allocated/tick). LRU over a per-unit
  access order that cycles through more keys than fit misses on every access. Fix (not done here, per the
  brief): evict at the tick boundary or pin the keys live units follow.
- **R117 choke-point crowd (`choke_crowd`):** budgets `steer.pairs` 15238 (max 12190/tick, total 2,252,037).
  **`move.blocked` is never measured there**: 0 in all 200 measured ticks (and 0 in a first design with a straight
  wall and 120 tanks over 600 ticks; with the funnel and the target at the wall face, 1–5 blocked units appear only
  after tick ~470 when the jam settles). `take_step`'s fallbacks (unpushed, x part, y part) slide units along
  walls, so a crowd almost never stays fully put. Because `check_budgets` fails a budgeted counter that a run
  does not measure, `move.blocked` cannot be budgeted in this scenario; it is budgeted where it occurs, in
  `scale_3000` (2) and `scale_5000` (3). See concerns.
- **R100:** the scale mix never edits terrain; `path.clusters_invalidated` is not measured and not budgeted.
- **R86:** every `CounterSim` builds a fresh sim (empty caches); `RunConfig::flow_cache_cold` stays false.
- **G7 (release bench).** Machine not idle: the lane-1 `gate pr` was running (9 cargo/rustc processes, CPU 14–100 %),
  so wall times swing ±40 % run to run. Same-load pairs at 3000 alternated terrain/flat: 7.36 / 5.54, 5.47 / 8.07 ms.

  | run | map | 500 mean / p95 | 3000 mean / p95 | 5000 mean / p95 |
  |---|---|---|---|---|
  | `--repeat 1 --ticks 200 --warmup 40` | terrain | 0.656 / 1.232 | **4.722 / 6.822** | 12.301 / 17.070 |
  | same | flat | 0.525 / 1.088 | **4.929 / 7.003** | 11.688 / 16.730 |
  | `--repeat 5 --ticks 400 --warmup 40` | terrain | 0.759 / 1.217 | 7.067 / 9.086 | 14.556 / 19.906 |
  | same | flat | 0.678 / 1.589 | 6.658 / 7.788 | 13.862 / 16.018 |
  | M0.json (repeat 5, 400 ticks, also under load) | flat | 0.537 / 1.314 | 4.431 / 7.955 | 9.335 / 16.329 |

  ≤ 15 ms/tick mean at 3000 holds in every run (worst 8.07 ms). Flat vs M0.json: quietest run −2 % / +11 % / +25 %;
  repeat-5 run under load +26 % / +50 % / +49 % (`--baseline docs/perf/M0.json` reports the regression).
  Justification: M1 added per-tick work M0 did not have — separation steering (`compute_steering` 1.07 ms at 3000,
  3.13 ms at 5000) and the order/path machinery (`advance_orders`, `process_path_queue`, path following in
  `move_system`) — plus the load noise; the terrain run costs the same as the flat run within noise. I can't
  tell how much of the 5000-unit excess is load: an idle-machine run is needed before M1.json.

## Self-review findings (fixed)
- First `group_move` reused cached keys (no abstract builds measured) → per-round shifted targets.
- First thrash scenario took 85 s per run → moved to a small ASCII map (4.2 s in the gate run).
- `OPEN_MAP` const was placed between `use` lines → moved below them.

## Concerns
1. **Flow-cache thrash is real** (above): 36 live keys rebuild ~91 integrations + tiles per tick; on a 2 km map
   with 60 keys, ~350 ms/tick. It is a correctness-neutral but severe perf cliff for > 32 simultaneous group
   destinations (easily reached by a player plus AI). The `flow_thrash` budget records today's thrash level, so
   the gate holds it but does not prevent it. Fix: evict at the tick boundary or pin live keys.
2. **R117 `move.blocked`** cannot be budgeted in the choke scenario (never measured there; see above). It is
   budgeted in `scale_3000`/`scale_5000`. If the controller wants it in the choke budget, either the movement
   counter must always be emitted (even 0; touches `movement.rs` and every budget), or the scenario needs a
   measured window after the jam settles (~tick 470+, fragile 1–5 counts).
3. **Track:** `replay.rs` and the fl_sim API snapshot are `contract`; the gate needs `--track contract` added
   (OK with it). Out-of-list edits: `fl_sim/src/scenario.rs` (new SetupError variant from the brief's interface),
   `fl_sim/src/lib.rs` (re-export), `fl_test_utils/src/lib.rs` (re-exports), `fl_cli/src/main.rs` (`--flat`).
4. **Scale counters vs M0 budgets:** on terrain, `alloc.bytes` per tick max rose ≈ 8–19× (scale_500 6.5 k → 98 k,
   scale_3000 19 k → 155 k) and `alloc.count` 63 → 90 / 55 → 141: per-request A*/HPA buffers (R98's "per-request
   all-node buffers", parked). Not a wall-time problem at 3000 (path queue 0.065 ms) but visible in the gate.
5. **5000 units: the path queue backs up** (`path.queue_len` up to 1031) — wanderers wait for paths; by design
   (budgeted queue) but worth knowing for the 5000-unit milestone check.
6. Bench wall times were taken under load (see G7); the gate time was 444 s (budget 300 s).

## Files changed
`crates/fl_sim/src/{scale,replay,scenario,lib}.rs`, `crates/fl_sim/tests/{scale_mix,replay}.rs`,
`crates/fl_test_utils/src/{perf,lib}.rs`, `crates/fl_test_utils/tests/perf_counters.rs`,
`crates/fl_cli/src/{bench,main}.rs`, `crates/fl_cli/tests/bench.rs`, `crates/fl_game/src/builder.rs`,
`crates/fl_game/tests/builder.rs`; plus the untracked `.new` artifacts above.

## Artifact diffs

### budgets.diff
```diff
--- docs/perf/budgets.ron	2026-10-25 02:29:29.093681800 +0200
+++ docs/perf/budgets.ron.new	2026-10-25 02:56:31.741189100 +0200
@@ -1,73 +1,134 @@
 // Per-tick work-counter budgets (spec §12.6, ADR-0010): ceil(max × 1.25) of a recorded
 // run. Protected: recorded with FL_RECORD_BUDGETS=1 into budgets.ron.new; the lead
 // blesses it with `cargo xtask bless`.
-(
-    scenarios: {
-        "scale_3000": {
-            "alloc.bytes": 19030,
-            "alloc.count": 69,
-            "despawn.entities": 63,
-            "move.los_cells": 8070,
-            "path.budget_spent": 1560,
-            "path.los_cells": 1560,
-            "path.requests": 27,
-            "publish.rebuilt": 3750,
-            "publish.records": 3750,
-            "scale.follows": 7500,
-            "scale.projectile_steps": 1250,
-            "sort.items": 63,
-            "spatial.checked": 12500,
-            "spatial.inserts": 63,
-            "spatial.queries": 375,
-            "spatial.query_candidates": 97878,
-            "spatial.relocations": 725,
-            "spawn.entities": 63,
-            "steer.pairs": 26213,
-            "steer.units": 3750,
-        },
-        "scale_500": {
-            "alloc.bytes": 6515,
-            "alloc.count": 79,
-            "despawn.entities": 12,
-            "move.los_cells": 1355,
-            "path.budget_spent": 417,
-            "path.los_cells": 417,
-            "path.requests": 9,
-            "publish.rebuilt": 625,
-            "publish.records": 625,
-            "scale.follows": 1250,
-            "scale.projectile_steps": 225,
-            "sort.items": 12,
-            "spatial.checked": 2100,
-            "spatial.inserts": 12,
-            "spatial.queries": 63,
-            "spatial.query_candidates": 3097,
-            "spatial.relocations": 140,
-            "spawn.entities": 12,
-            "steer.pairs": 1750,
-            "steer.units": 625,
-        },
-        "scale_5000": {
-            "alloc.bytes": 30770,
-            "alloc.count": 87,
-            "despawn.entities": 105,
-            "move.los_cells": 13445,
-            "path.budget_spent": 2590,
-            "path.los_cells": 2590,
-            "path.requests": 43,
-            "publish.rebuilt": 6250,
-            "publish.records": 6250,
-            "scale.follows": 12500,
-            "scale.projectile_steps": 2100,
-            "sort.items": 105,
-            "spatial.checked": 20850,
-            "spatial.inserts": 105,
-            "spatial.queries": 625,
-            "spatial.query_candidates": 267367,
-            "spatial.relocations": 1189,
-            "spawn.entities": 105,
-            "steer.pairs": 64292,
-            "steer.units": 6250,
-        },
-    },
+(
+    scenarios: {
+        "choke_crowd": {
+            "alloc.bytes": 12233,
+            "alloc.count": 48,
+            "flowfield.cells": 302,
+            "move.los_cells": 4488,
+            "path.budget_spent": 773,
+            "path.los_cells": 142,
+            "path.nodes_expanded": 634,
+            "path.requests": 3,
+            "publish.rebuilt": 240,
+            "publish.records": 240,
+            "spatial.checked": 240,
+            "spatial.inserts": 0,
+            "spatial.relocations": 10,
+            "steer.pairs": 15238,
+            "steer.units": 240,
+        },
+        "flow_thrash": {
+            "alloc.bytes": 938165,
+            "alloc.count": 2564,
+            "commands.applied": 45,
+            "flowfield.abstract_nodes": 28665,
+            "flowfield.cells": 29120,
+            "move.los_cells": 7005,
+            "path.budget_spent": 10080,
+            "path.queue_len": 5,
+            "path.requests": 40,
+            "publish.rebuilt": 270,
+            "publish.records": 270,
+            "sort.items": 45,
+            "spatial.checked": 270,
+            "spatial.inserts": 0,
+            "spatial.relocations": 28,
+            "steer.pairs": 4140,
+            "steer.units": 270,
+        },
+        "group_move": {
+            "alloc.bytes": 460993,
+            "alloc.count": 1295,
+            "commands.applied": 5,
+            "flowfield.abstract_nodes": 10555,
+            "flowfield.cells": 23200,
+            "move.los_cells": 7125,
+            "path.budget_spent": 10555,
+            "path.queue_len": 13,
+            "path.requests": 3,
+            "publish.rebuilt": 375,
+            "publish.records": 375,
+            "sort.items": 5,
+            "spatial.checked": 375,
+            "spatial.inserts": 0,
+            "spatial.relocations": 14,
+            "steer.pairs": 59,
+            "steer.units": 375,
+        },
+        "scale_3000": {
+            "alloc.bytes": 193690,
+            "alloc.count": 177,
+            "despawn.entities": 63,
+            "move.blocked": 2,
+            "move.los_cells": 8008,
+            "path.budget_spent": 3402,
+            "path.los_cells": 2110,
+            "path.nodes_expanded": 1763,
+            "path.requests": 24,
+            "publish.rebuilt": 3750,
+            "publish.records": 3750,
+            "scale.follows": 7500,
+            "scale.projectile_steps": 1250,
+            "sort.items": 63,
+            "spatial.checked": 12500,
+            "spatial.inserts": 63,
+            "spatial.queries": 375,
+            "spatial.query_candidates": 98613,
+            "spatial.relocations": 707,
+            "spawn.entities": 63,
+            "steer.pairs": 27464,
+            "steer.units": 3750,
+        },
+        "scale_500": {
+            "alloc.bytes": 122415,
+            "alloc.count": 113,
+            "despawn.entities": 12,
+            "move.los_cells": 1338,
+            "path.budget_spent": 1792,
+            "path.los_cells": 964,
+            "path.nodes_expanded": 1128,
+            "path.requests": 7,
+            "publish.rebuilt": 625,
+            "publish.records": 625,
+            "scale.follows": 1250,
+            "scale.projectile_steps": 225,
+            "sort.items": 12,
+            "spatial.checked": 2100,
+            "spatial.inserts": 12,
+            "spatial.queries": 63,
+            "spatial.query_candidates": 3350,
+            "spatial.relocations": 140,
+            "spawn.entities": 12,
+            "steer.pairs": 1820,
+            "steer.units": 625,
+        },
+        "scale_5000": {
+            "alloc.bytes": 516650,
+            "alloc.count": 514,
+            "despawn.entities": 105,
+            "move.blocked": 3,
+            "move.los_cells": 13307,
+            "path.budget_spent": 11095,
+            "path.los_cells": 6552,
+            "path.nodes_expanded": 4882,
+            "path.queue_len": 1289,
+            "path.requests": 135,
+            "publish.rebuilt": 6250,
+            "publish.records": 6250,
+            "scale.follows": 12500,
+            "scale.projectile_steps": 2100,
+            "sort.items": 105,
+            "spatial.checked": 20850,
+            "spatial.inserts": 105,
+            "spatial.queries": 625,
+            "spatial.query_candidates": 273049,
+            "spatial.relocations": 1153,
+            "spawn.entities": 105,
+            "steer.pairs": 67697,
+            "steer.units": 6250,
+        },
+    },
 )
```

### snaps.diff
```diff
--- crates/fl_test_utils/tests/snapshots/perf_counters__scale_500.snap	2026-10-25 02:29:28.973060900 +0200
+++ crates/fl_test_utils/tests/snapshots/perf_counters__scale_500.snap.new	2026-10-25 02:56:28.541334700 +0200
@@ -1,16 +1,17 @@
 ---
 source: crates/fl_test_utils/tests/perf_counters.rs
-assertion_line: 81
+assertion_line: 85
 expression: totals
 ---
 Totals(
   entities: 1680,
   totals: {
     "despawn.entities": 1800,
-    "move.los_cells": 201452,
-    "path.budget_spent": 13950,
-    "path.los_cells": 13950,
-    "path.requests": 304,
+    "move.los_cells": 202740,
+    "path.budget_spent": 21847,
+    "path.los_cells": 15363,
+    "path.nodes_expanded": 6484,
+    "path.requests": 259,
     "publish.rebuilt": 100000,
     "publish.records": 100000,
     "scale.follows": 200000,
@@ -19,10 +20,10 @@
     "spatial.checked": 336000,
     "spatial.inserts": 1800,
     "spatial.queries": 10000,
-    "spatial.query_candidates": 458611,
-    "spatial.relocations": 18502,
+    "spatial.query_candidates": 464900,
+    "spatial.relocations": 17979,
     "spawn.entities": 1800,
-    "steer.pairs": 270116,
+    "steer.pairs": 277890,
     "steer.units": 100000,
   },
 )
--- crates/fl_test_utils/tests/snapshots/perf_counters__scale_3000.snap	2026-10-25 02:29:28.972059200 +0200
+++ crates/fl_test_utils/tests/snapshots/perf_counters__scale_3000.snap.new	2026-10-25 02:56:29.987289700 +0200
@@ -1,16 +1,18 @@
 ---
 source: crates/fl_test_utils/tests/perf_counters.rs
-assertion_line: 81
+assertion_line: 85
 expression: totals
 ---
 Totals(
   entities: 10000,
   totals: {
     "despawn.entities": 10000,
-    "move.los_cells": 1211962,
-    "path.budget_spent": 81899,
-    "path.los_cells": 81899,
-    "path.requests": 1805,
+    "move.blocked": 23,
+    "move.los_cells": 1216042,
+    "path.budget_spent": 132610,
+    "path.los_cells": 86376,
+    "path.nodes_expanded": 46234,
+    "path.requests": 1344,
     "publish.rebuilt": 600000,
     "publish.records": 600000,
     "scale.follows": 1200000,
@@ -19,10 +21,10 @@
     "spatial.checked": 2000000,
     "spatial.inserts": 10000,
     "spatial.queries": 60000,
-    "spatial.query_candidates": 15281852,
-    "spatial.relocations": 104831,
+    "spatial.query_candidates": 15332107,
+    "spatial.relocations": 101522,
     "spawn.entities": 10000,
-    "steer.pairs": 4075560,
+    "steer.pairs": 4252506,
     "steer.units": 600000,
   },
 )
--- crates/fl_test_utils/tests/snapshots/perf_counters__scale_5000.snap	2026-10-25 02:29:28.974059200 +0200
+++ crates/fl_test_utils/tests/snapshots/perf_counters__scale_5000.snap.new	2026-10-25 02:56:32.169411100 +0200
@@ -1,17 +1,20 @@
 ---
 source: crates/fl_test_utils/tests/perf_counters.rs
-assertion_line: 81
+assertion_line: 85
 expression: totals
 ---
 Totals(
   entities: 16680,
   totals: {
     "despawn.entities": 16800,
-    "move.los_cells": 2025582,
-    "path.budget_spent": 130210,
-    "path.los_cells": 130210,
-    "path.requests": 2867,
-    "publish.rebuilt": 1000000,
+    "move.blocked": 91,
+    "move.los_cells": 1990490,
+    "path.budget_spent": 286809,
+    "path.los_cells": 187186,
+    "path.nodes_expanded": 99623,
+    "path.queue_len": 6705,
+    "path.requests": 3149,
+    "publish.rebuilt": 993404,
     "publish.records": 1000000,
     "scale.follows": 2000000,
     "scale.projectile_steps": 336000,
@@ -19,10 +22,10 @@
     "spatial.checked": 3336000,
     "spatial.inserts": 16800,
     "spatial.queries": 100000,
-    "spatial.query_candidates": 42132885,
-    "spatial.relocations": 175077,
+    "spatial.query_candidates": 42458787,
+    "spatial.relocations": 168583,
     "spawn.entities": 16800,
-    "steer.pairs": 10068905,
-    "steer.units": 1000000,
+    "steer.pairs": 10323562,
+    "steer.units": 994633,
   },
 )
```

### formats.diff
```diff
--- crates/fl_sim/tests/snapshots/formats__save_codec_formats.snap	2026-10-25 02:29:28.943680800 +0200
+++ crates/fl_sim/tests/snapshots/formats__save_codec_formats.snap.new	2026-10-25 02:25:39.564729200 +0100
@@ -14,7 +14,7 @@
 SimSave: Struct([Named { name: "tick", value: TypeName("Tick") }, Named { name: "sim_id_next", value: U32 }, Named { name: "pending_commands", value: Seq(TypeName("PlayerCommand")) }, Named { name: "resources", value: Seq(Tuple([Str, U32, Seq(U8)])) }, Named { name: "entities", value: Seq(Tuple([TypeName("SimId"), Seq(Tuple([Str, U32, Seq(U8)]))])) }])
 StringTables: Struct([Named { name: "tables", value: Map { key: Str, value: Seq(Str) } }])
 Tick: NewTypeStruct(U32)
-REPLAY_FORMAT_VERSION = 2
+REPLAY_FORMAT_VERSION = 3
 CommandKind: Struct([Named { name: "type_id", value: TypeName("CommandTypeId") }, Named { name: "payload", value: Seq(U8) }])
 CommandTypeId: NewTypeStruct(U16)
 MatchHeader: Struct([Named { name: "format_version", value: U32 }, Named { name: "build_id", value: Str }, Named { name: "sim_version", value: U32 }, Named { name: "features", value: Seq(Tuple([Str, U32])) }, Named { name: "mods", value: Seq(TupleArray { content: Str, size: 2 }) }, Named { name: "rules_hash", value: U64 }, Named { name: "cheats_allowed", value: Bool }, Named { name: "seed", value: U64 }, Named { name: "string_tables", value: TypeName("StringTables") }])
@@ -23,7 +23,7 @@
 Replay: Struct([Named { name: "version", value: U32 }, Named { name: "header", value: TypeName("MatchHeader") }, Named { name: "start", value: TypeName("ReplayStart") }, Named { name: "hash_interval", value: U32 }, Named { name: "ticks", value: U32 }, Named { name: "inputs", value: Seq(TypeName("TickInput")) }, Named { name: "hashes", value: Seq(Tuple([TypeName("Tick"), U64])) }, Named { name: "events", value: Seq(TypeName("ReplayEvent")) }])
 ReplayEvent: Enum({0: Named { name: "ContentReload", value: Struct([Named { name: "tick", value: TypeName("Tick") }, Named { name: "rules_hash", value: U64 }]) }})
 ReplayStart: Enum({0: Named { name: "Scenario", value: NewType(Str) }, 1: Named { name: "Scale", value: NewType(TypeName("ScaleScenario")) }, 2: Named { name: "Save", value: NewType(Seq(U8)) }})
-ScaleScenario: Struct([Named { name: "units", value: U32 }, Named { name: "players", value: U8 }, Named { name: "map_m", value: I32 }, Named { name: "wander_m", value: I32 }, Named { name: "seed", value: U64 }, Named { name: "followers_per_unit", value: U32 }, Named { name: "projectiles", value: U32 }, Named { name: "projectile_ttl", value: U32 }, Named { name: "sensor_every", value: U32 }])
+ScaleScenario: Struct([Named { name: "units", value: U32 }, Named { name: "players", value: U8 }, Named { name: "map_m", value: I32 }, Named { name: "wander_m", value: I32 }, Named { name: "seed", value: U64 }, Named { name: "followers_per_unit", value: U32 }, Named { name: "projectiles", value: U32 }, Named { name: "projectile_ttl", value: U32 }, Named { name: "sensor_every", value: U32 }, Named { name: "map", value: Option(Str) }])
 SimId: NewTypeStruct(U32)
 StringTables: Struct([Named { name: "tables", value: Map { key: Str, value: Seq(Str) } }])
 Tick: NewTypeStruct(U32)
```

### api.diff
```diff
--- xtask/snapshots/api/fl_sim.txt	2026-10-25 02:29:29.141724200 +0200
+++ xtask/snapshots/api/fl_sim.txt.new	2026-10-25 02:11:10.851966700 +0100
@@ -870,6 +870,9 @@
 pub fn fl_sim::SaveError::fmt(&self, &mut core::fmt::Formatter<'_>) -> core::fmt::Result
 impl core::marker::StructuralPartialEq for fl_sim::SaveError
 pub enum fl_sim::SetupError
+pub fl_sim::SetupError::MapSizeMismatch
+pub fl_sim::SetupError::MapSizeMismatch::map: alloc::string::String
+pub fl_sim::SetupError::MapSizeMismatch::map_m: i32
 pub fl_sim::SetupError::SeedMismatch
 pub fl_sim::SetupError::SeedMismatch::matched: u64
 pub fl_sim::SetupError::SeedMismatch::scale: u64
@@ -1748,6 +1751,7 @@
 pub fn fl_sim::ScaleMix::deserialize<__D>(__D) -> core::result::Result<Self, <__D as serde_core::de::Deserializer>::Error> where __D: serde_core::de::Deserializer<'de>
 pub struct fl_sim::ScaleScenario
 pub fl_sim::ScaleScenario::followers_per_unit: u32
+pub fl_sim::ScaleScenario::map: core::option::Option<alloc::string::String>
 pub fl_sim::ScaleScenario::map_m: i32
 pub fl_sim::ScaleScenario::players: u8
 pub fl_sim::ScaleScenario::projectile_ttl: u32
@@ -1757,6 +1761,7 @@
 pub fl_sim::ScaleScenario::units: u32
 pub fl_sim::ScaleScenario::wander_m: i32
 impl fl_sim::ScaleScenario
+pub fn fl_sim::ScaleScenario::on_terrain(u32) -> Self
 pub fn fl_sim::ScaleScenario::populate(&self, &mut bevy_ecs::world::World) -> core::result::Result<alloc::vec::Vec<fl_protocol::ids::SimId>, fl_sim::SetupError>
 pub fn fl_sim::ScaleScenario::with_units(u32) -> Self
 impl core::clone::Clone for fl_sim::ScaleScenario
@@ -2285,6 +2290,7 @@
 pub const fl_sim::PHASE_TARGET: &str
 pub const fl_sim::REPLAY_FORMAT_VERSION: u32
 pub const fl_sim::SAVE_FORMAT_VERSION: u32
+pub const fl_sim::SCALE_MAP: &str
 pub const fl_sim::SIM_VERSION: u32
 pub trait fl_sim::CommandHandler<P: fl_protocol::command::CommandPayload>: core::marker::Send + core::marker::Sync + 'static
 pub fn fl_sim::CommandHandler::apply(&self, &P, &mut bevy_ecs::world::World, &fl_sim::CommandCtx<'_>) -> core::result::Result<(), fl_sim::RejectReason>
```

### new snapshots
```
--- new: perf_counters__group_move.snap.new
---
Totals(
  entities: 300,
  totals: {
    "commands.applied": 16,
    "flowfield.abstract_nodes": 202656,
    "flowfield.cells": 442752,
    "move.los_cells": 1043811,
    "path.budget_spent": 202656,
    "path.queue_len": 120,
    "path.requests": 48,
    "publish.rebuilt": 57836,
    "publish.records": 60000,
    "sort.items": 16,
    "spatial.checked": 60000,
    "spatial.inserts": 0,
    "spatial.relocations": 801,
    "steer.pairs": 7273,
    "steer.units": 57413,
  },
)
--- new: perf_counters__flow_thrash.snap.new
---
Totals(
  entities: 216,
  totals: {
    "commands.applied": 36,
    "flowfield.abstract_nodes": 4571532,
    "flowfield.cells": 4644096,
    "move.los_cells": 974010,
    "path.budget_spent": 9072,
    "path.queue_len": 4,
    "path.requests": 36,
    "publish.rebuilt": 43200,
    "publish.records": 43200,
    "sort.items": 36,
    "spatial.checked": 43200,
    "spatial.inserts": 0,
    "spatial.relocations": 180,
    "steer.pairs": 442628,
    "steer.units": 43181,
  },
)
--- new: perf_counters__choke_crowd.snap.new
---
Totals(
  entities: 192,
  totals: {
    "flowfield.cells": 241,
    "move.los_cells": 575296,
    "path.budget_spent": 9194,
    "path.los_cells": 2003,
    "path.nodes_expanded": 7191,
    "path.requests": 28,
    "publish.rebuilt": 38400,
    "publish.records": 38400,
    "spatial.checked": 38400,
    "spatial.inserts": 0,
    "spatial.relocations": 515,
    "steer.pairs": 2252037,
    "steer.units": 38400,
  },
)
```

---

# Fix round 1

**Commit:** `f4b34d9` fix(perf): Task 13 fix round 1: flat_3000 gate, queue drain test, budget headroom, bench --force (`Refs: ADR-0010`). Never blessed.

## Changes, by ruling
- **A / R124 (flow-cache thrash):** not fixed here (lane-1 Task 10b). `flow_thrash` stays budgeted at today's level, labelled a **temporary ceiling** in `budgets.ron.new` (header note `// flow_thrash: TEMPORARY CEILING at the pre-fix flow-cache thrash level (R124); the eviction fix (Task 10b) re-records it.`), in `counters_flow_thrash`'s doc and in `CounterSim::flow_thrash`'s doc.
- **B / R125 (flat gate):** new `CounterSim::flat(units)` (= `ScaleScenario::with_units`) and `counters_flat_3000`. Its recorded budgets are **identical, counter for counter, to the blessed M0-era `scale_3000` budgets** (`alloc.bytes` 19030, `alloc.count` 69, `steer.pairs` 26213, `path.budget_spent` 1560, …), and `perf_counters__flat_3000.snap.new` is byte-identical (after the header) to the blessed pre-Task-13 `perf_counters__scale_3000.snap` (entities 10000; `path.requests` 1805, `path.budget_spent` 81899, `steer.pairs` 4075560, `spatial.query_candidates` 15281852, `move.los_cells` 1211962). So the flat workload is unchanged, and non-path and alloc regressions are gated at M0 levels again. Terrain `alloc.*` budgets stay as recorded.
- **C / R126 (5000-unit queue):** new `queue_burst_drains_at_5000` (new `scale_queue_lengths(units, ticks)`): queue > 4000 after tick 0, and 0 on every tick from 60 to 159. **Burst source: the tick-0 wander burst.** Every unit is idle at tick 0, so `wander_system` orders all 5,000 at once (4,881 queued after tick 0). The budget processes about 94 a tick, so the queue is empty by tick 52 and stays empty (diagnostic over 400 ticks: 0 on every tick after 52; at 3,000 units: 2,881 queued, empty by tick 30). Not synchronised re-planning (R116) and no wander-timer alignment after the start; the 1031 maximum in `scale_5000` is that burst's tail at the first measured tick (40). No stagger needed for Task 10b on this account.
- **D / R127 (docs):** `counters_choke_crowd` and `CounterSim::choke_crowd` say it budgets `steer.pairs` and the path/flow counters, and that `move.blocked` is gated in `scale_3000`/`scale_5000`, with no zero-valued counters emitted; the head-on jam variant is C7.
- **Minor 1:** stale docs fixed ("60 live flow fields" → 36; `move.blocked` choke claim).
- **Minor 2 (headroom):** new `pub fn budget_for(max) = max(ceil(max × 1.25), max + 2)`: every budget has at least +2 (the +2 decides below 8). Documented on `budget_for`, `record_budgets` and in the generated `budgets.ron` header. Unit test `small_maxima_keep_two_of_headroom`; `record_then_check_round_trips` expectations follow (0 → 2, 1 → 3: test-guard findings 3–5, expected changes of the rule). Budgets now below 10: `move.blocked` 3 (scale_3000) / 4 (scale_5000), `path.requests` 4 (choke, group_move) / 7 (scale_500), `path.queue_len` 6 (flow_thrash), `commands.applied`/`sort.items` 6 (group_move), `spatial.inserts` 2 (where 0).
- **Minor 4:** `CounterSim`, `Orders` and the scenario helpers moved to `crates/fl_test_utils/src/perf/scenarios.rs` (`perf.rs` keeps budgets, runners, record/check).
- **Minor 5:** new `fl_test_utils::unit_cells(world) -> Vec<UnitCell { id, owner, profile, cell, passable }>` (in `terrain.rs`), used by both `scale_mix.rs` (`units_on_blocked_cells`, `unit_ids`) and the scenarios' `units_by`.
- **Minor 6:** `bench --baseline` reads the baseline before running and refuses a baseline on another map (`CliError::Check`: "bench baseline: the baseline ran on map open, this run on map base.scale_2km: … pass --force to compare anyway", exit 1); `--force` compares with a warning. New test `baseline_on_another_map_needs_force`; `baseline_build_mismatch_warns` now runs `--flat` (its baseline row is open).

## Regenerated `.new` (all at `f4b34d9`, stale ones deleted first)
- `docs/perf/budgets.ron.new`: **changed**. Adds `flat_3000`, the two header notes, and the +2 headroom on small budgets. Other values are unchanged.
- `crates/fl_test_utils/tests/snapshots/perf_counters__flat_3000.snap.new`: **new**.
- `perf_counters__{scale_500,scale_3000,scale_5000,group_move,flow_thrash,choke_crowd}.snap.new`: regenerated, same content as round 0.
- `crates/fl_test_utils/tests/replays/testbed_wander.replay.new`: regenerated. The build id is now `0.0.0+f4b34d9`; it is still header-only (version 3).
- `crates/fl_sim/tests/snapshots/formats__save_codec_formats.snap.new` and `xtask/snapshots/api/fl_sim.txt.new`: regenerated, unchanged from round 0 (no fl_sim change in this round).

## Tests
`cargo nextest run --profile agent --no-fail-fast -p fl_sim -p fl_test_utils -p fl_catalog -p fl_cli -p fl_game` gives `Summary [65.753s] 479 tests run: 470 passed, 9 failed`. All 9 are bless-pending: `fl_sim::formats save_codec_formats`, `fl_test_utils::golden golden_replays_verify`, and `perf_counters` `smoke_counters_scale_500`, `counters_scale_3000`, `milestone_counters_scale_5000`, `counters_flat_3000`, `counters_group_move`, `counters_flow_thrash` and `counters_choke_crowd`. The new `queue_burst_drains_at_5000`, `path_budget_holds_at_scale`, `baseline_on_another_map_needs_force`, `bench_flat_flag_uses_open_map` and unit tests pass.

## Gate
`cargo xtask gate task --track contract --track sim-core --track perf --track tools --track client`:
```
PASS fmt (2.0s)
PASS clippy-drift (1.9s)
PASS claude-md (0.0s)
PASS check-scope (0.2s)
PASS protected (0.2s)
WARN test-guard (1.6s): test-guard: 5 findings (reviewer must accept each)
    TEST-GUARD crates/fl_game/tests/builder.rs:97 RemovedAssert (round 0, accepted)
    TEST-GUARD crates/fl_sim/tests/scale_mix.rs:130 RemovedAssert (round 0, accepted)
    TEST-GUARD crates/fl_test_utils/src/perf.rs:289/290/292 RemovedAssert (round-trip expectations under the +2 rule)
PASS clippy (28.7s)
PASS arch-check (3.9s)
FAIL test-headless (131.5s)   (bless-pending only, list above)
PASS test-client (36.5s)
FAIL api (39.6s)              (fl_sim .txt.new awaits bless)
PASS data-check (21.7s)
gate task: FAIL (test-headless, api) in 268s
```
