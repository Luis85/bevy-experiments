# Task 5 report: sim terrain (TerrainGrid, NavGrid, speed formula, blocking, clearance, debug.set_terrain, ASCII fixtures, fill_missing)

Lane 1, worktree `C:\fl\a`, branch `m1/phase-a`, gate base `52bed82`.

## Status

DONE_WITH_CONCERNS (see "Concerns"). Commits:
- `9ecb7ae feat(fl_sim): terrain grid, NavGrid with class blocking and clearance, speed formula, debug.set_terrain; fill_missing loads M0 saves` (`Refs: ADR-0014, ADR-0006`; no `Blessed:` trailer, the lead adds it at bless)
- `1f35ff7 test(fl_game): spawn the wiring test's tank on testbed grass, not the river` (the first gate run caught
  `fl_game::wiring::debug_spawn_round_trips_through_runner` spawning at (500, 500), now the testbed river, which
  terrain correctly rejects; moved to (500, 400), grass. Track `client`, outside this task's tracks.)

## Step 0: M0 save fixture (R78, R81)

- Generated from tag `m0` (commit `0ae615d`) in a temporary worktree `C:/fl/m0fixture` with its own build dir
  `CARGO_BUILD_BUILD_DIR=C:/t/fl/m0fix`, before any Task 5 change was built:
  `cargo run -q -p fl_cli -- save base.testbed --tick 40 --out C:/t/fl/m0_testbed.save --json`
- Output: `{"entities": 10, "hash": "0x6742e6aaa9854d35", "pending_commands": 0, "tick": 40}`
- Copied to `crates/fl_test_utils/tests/replays/m0_testbed.save.new` (3437 bytes, sha256
  `cd5a8656128828646e0a2eaef41d97bbc854ce8e90a78c78ca121377a62185b1`). The worktree was removed
  (`git worktree remove`). The build dir `C:/t/fl/m0fix` (736 MB) is left; delete it at will.
- `assets/base/mod.ron` is unchanged since `m0` (`version: "0.1.0"`), so `check_save`'s mods rule accepts the save.
- Bless: `cargo xtask bless crates/fl_test_utils/tests/replays/m0_testbed.save.new --reason "M1 Task 5: M0 testbed save fixture (R78)"`.
  Until then `m0_save.rs` reads `tests/replays/m0_testbed.save` and fails; `FL_M0_SAVE=<path>` points it at the `.new`
  (how I ran it below).

## What I implemented

**fl_sim**
- `terrain.rs` (new): `TerrainGrid` (Hashed `core.terrain` v1; palette hashed and saved as names; empty
  types/heights = open at `height_m.0`), `CellRect` (+ `area()`), `open`, `from_layers`, `type_at`, `terrain_at`,
  `height_at` (`lo + (hi − lo) × v / 65535` on raw i128, truncated toward zero), `set_rect` (appends new names,
  indices never move). `init_terrain(world, map)` (inserts terrain for the `MapInfo`, then `rebuild_nav`; called next
  to `init_spatial` in `setup_map`, the scale scenario; `setup_scenario_def` builds terrain + nav up front for its
  all-or-nothing check). `fill_terrain`: the `core.terrain` fill (exactly one map with equal `size_m`/`cell_m` and
  layers → its layers; none → open; several → `Err("the save's map matches several maps: <ids>")`; no `MapInfo` →
  nothing).
- `nav/grid.rs` (new): `MoveProfile`, `profile_of`, `content_profiles`, `COST_ROAD`, `NavGrid` (per class: blocked
  flag, clearance byte, `u16` cost per cell; kinds per palette entry; hand-written `PartialEq` ignoring `version`),
  `build`, `cells`, `cell_of`, `blocked`, `clearance`, `passable` (centred anchor per ADR-0014), `cost`,
  `speed_factor`, `update_rect` (blocking/cost for rect grown by 1, clearance for that grown by `max_clearance − 1`
  toward −x/−y; returns the recomputed rect), `profiles`, `version`. Factor = `((ONE − pass) × tf + pass).clamp(min,
  ONE)`; cost = `((100 << 32) + raw − 1) / raw`.
- `nav/mod.rs` (new): `PathRules` (+ `link`), `rebuild_nav`, `rebuild_core` (spatial then nav; registered once in
  `CoreFeature::build`, `spatial::build` no longer registers a rebuild), `on_terrain_changed` (`update_rect` +
  `nav.cells_updated`), debug queries `core.passability` and `core.terrain_at` (every bad arg is an `Err`, no panics).
- `movement.rs`: `MovementRules` (+ `link`: min_speed_frac in (0, 1] and ≥ 100/65535 on raw integers; both maps
  cover `MoveClass::ALL`; slopes ≥ 0; path factors in [−1, 1]), `SteeringRules` (+ `link`: idle_every ≥ 1), both
  registered. `move_system`: step = road speed × `speed_factor` of the unit's cell; a step ending in another cell
  the unit's profile cannot pass is skipped (waypoint kept) and counted `move.blocked`.
- `spawn.rs`: `SpawnError::BlockedCell`; `check_spawn` takes the `NavGrid` and rejects a `Mobility` unit on a cell
  its profile cannot pass (setup, scenario check and the funnel share it).
- `debug_cmds.rs`: `debug.spawn` and `debug.teleport` reject blocked cells with `core.invalid_target`;
  `debug.set_terrain` handler (unknown id → `core.unknown_content`; min > max, outside, no map, full palette →
  `core.invalid_target`; else `set_rect`, `on_terrain_changed`, journal `core.terrain_changed`).
- `commands.rs`: `INVALID_TARGET = RejectReason::Feature("core.invalid_target")` (R79, no new variant).
- `classify.rs`: `Codec::fill_missing(FillFn)`, `FillFn`; codec traits expose `fill()`; state entries record their
  feature. `save/load.rs`: missing Hashed names with a fill are collected during restore (a component name counts as
  missing when no saved entity has it) and run after the allocator and clock are set, before the Derived rebuilds,
  only when the save's version of the owning feature is older than the build's, in feature load order then by name;
  an `Err` is `LoadError::Codec { name, message }`.
- `feature.rs`: `CoreFeature::version()` = 2; `build.rs`: `SIM_VERSION` = 2. `units.rs`: `MobilityDef.terrain_factor`
  in [−1, 1]. `debug_query.rs`/`builder.rs`: docs state that a query never panics and answers `Err` on bad args.
- `lib.rs`: `pub mod nav`, `mod terrain`; exports `TerrainGrid`, `CellRect`, `MovementRules`, `SteeringRules`,
  `FillFn`.

**fl_test_utils**
- `terrain.rs` (new): `ascii_terrain_content`, `ascii_terrain_content_with`, `write_ascii_mod` (the legend, units and
  factions of the brief; heights exact in metres via `height_m: (0, 65535)`).
- `builder.rs`: `ScenarioBuilder::ascii_map`, `ascii_rules`; `map_flat` now inserts open terrain and rebuilds the
  nav grid; `Scenario::reload` is public.
- `Cargo.toml`: `tempfile` dependency. `fl_sim/Cargo.toml`: `fl_test_utils` dev-dependency. `Cargo.lock` delta.

**Content**: `assets/base/rules.ron` gains `Movement`, `Path`, `Steering` with the brief's values and a "guess"
comment; the five fixture `rules.ron` files gain the same sections.

## Tests

New: `crates/fl_sim/tests/terrain.rs` (all 21 named in the brief except those placed in `m0_save.rs`, plus
`testbed_ridge_blocks_wheeled_hill_stays_passable` (Task 4 carry) and `units_without_terrain_keep_m0_movement`),
3 fill tests in `crates/fl_sim/tests/save.rs`, `crates/fl_test_utils/tests/m0_save.rs` (5 tests). Changed:
`debug_query.rs` (feature `aaa` with `aaa.first`, which sorts before `core.`; the test now asserts it is listed first),
`feature.rs` and `hash.rs` (core v2, `debug.set_terrain`, the new rules sections).

### TDD evidence

I wrote the implementation and the tests in one pass, so there is no pre-implementation RED run. As RED evidence
I mutated three behaviours at once (movement ignores blocking; `load_world` skips the fills; `speed_factor` returns
1), ran the five covering tests, then restored the files:

```
FL_M0_SAVE=… cargo nextest run --profile agent --no-fail-fast -p fl_sim -p fl_test_utils -E 'test(units_stop_at_blocked_cells) | test(fill_missing_runs_for_an_older_feature_save) | test(speed_formula_matches_spec) | test(truck_slows_down_off_road) | test(m0_testbed_save_loads_under_m1)'
FAIL fl_test_utils::m0_save m0_testbed_save_loads_under_m1   — Requested resource fl_sim::terrain::TerrainGrid does not exist
FAIL fl_sim::save fill_missing_runs_for_an_older_feature_save — assertion `left == right` failed
FAIL fl_sim::terrain units_stop_at_blocked_cells              — the tank entered the water at FxVec2 { x: Fx(16), y: Fx(6) }
FAIL fl_sim::terrain speed_formula_matches_spec               — assertion `left == right` failed
FAIL fl_sim::terrain truck_slows_down_off_road                — ratio 1
Summary 5 tests run: 0 passed, 5 failed
```

GREEN (restored code):

```
cargo nextest run --profile agent -p fl_sim --test terrain --test save --test debug_query --test feature
Summary [18.582s] 65 tests run: 65 passed, 0 skipped
FL_M0_SAVE=C:/fl/a/crates/fl_test_utils/tests/replays/m0_testbed.save.new cargo nextest run --profile agent -p fl_test_utils --test m0_save
Summary [0.946s] 5 tests run: 5 passed, 0 skipped
```

### Wider runs

```
FL_M0_SAVE=… cargo nextest run --profile agent --no-fail-fast -p fl_sim -p fl_test_utils -p fl_cli -p fl_catalog
Summary [33.348s] 326 tests run: 324 passed, 2 failed
  FAIL fl_test_utils::golden golden_replays_verify          (protected golden, awaits bless: Err(Header(Features)))
  FAIL fl_test_utils::formats save_codecs_of_every_feature  (protected snapshot, awaits bless)

FL_M0_SAVE=… cargo nextest run --no-fail-fast -p fl_catalog -p fl_test_utils      (default profile, in full)
Summary [6.360s] 31 tests run: 29 passed, 2 failed   (the same two protected tests)
```

`milestone_counters_scale_5000`, `counters_scale_3000` and `smoke_counters_scale_500` pass: the scale scenario is
open ground, so the counters and `docs/perf/budgets.ron` are unchanged. `cargo clippy -p fl_sim -p fl_test_utils
-p fl_cli --all-targets -- -D warnings` is clean.

## Protected outputs (.new, for the lead to bless)

Reason for all: `M1 Task 5: Hashed terrain grid, derived NavGrid, SIM_VERSION 2` (the save fixture: `M1 Task 5:
M0 testbed save fixture (R78)`).

1. `crates/fl_test_utils/tests/replays/m0_testbed.save.new`: see Step 0.
2. `crates/fl_test_utils/tests/replays/testbed_wander.replay.new` (`FL_RECORD_GOLDEN=1`). `fl_cli replay-verify`
   on it: `verified: 400 ticks`. Compared with the blessed replay: all 400 hashes differ from tick 0 (the terrain grid
   is Hashed state, and units now move at terrain speed and stop at blocked cells). Header: `features` core 1 → 2,
   `sim_version` 1 → 2, `rules_hash` 3467218278007742600 → 9553072667589564137 (new rules sections), `build_id`.
   String tables: `commands` + `debug.set_terrain`, `journal_records` + `core.terrain_changed`, `rules_sections` +
   `Movement`, `Path`, `Steering`, `sim_resources` + `core.nav_grid`, `core.terrain`. The recorded inputs differ only
   in dense command ids (`debug.set_terrain` sorts before `debug.spawn`/`debug.teleport`/`debug.wander`).
3. `crates/fl_test_utils/tests/snapshots/formats__save_codecs.snap.new`: one new block:
   ```
   core.terrain@1:
     root: TypeName("TerrainGrid")
     TerrainGrid: Struct([Named { name: "cells_x", value: U32 }, Named { name: "cells_y", value: U32 }, Named { name: "palette", value: Seq(Str) }, Named { name: "types", value: Seq(U8) }, Named { name: "heights", value: Seq(U16) }, Named { name: "height_m", value: TupleArray { content: I64, size: 2 } }])
   ```
4. `xtask/snapshots/api/fl_sim.txt.new` (`cargo xtask api`: `fl_sim: changed`, other four unchanged). Additions only:
   `pub mod fl_sim::nav` (`MoveProfile`, `NavGrid` and its methods, `PathRules`, `COST_ROAD`, `content_profiles`,
   `profile_of`, `rebuild_nav`), `CellRect`, `TerrainGrid`, `MovementRules`, `SteeringRules`, `FillFn`,
   `Codec::fill_missing`, `SpawnError::BlockedCell`.

Not changed: `system_order__sim_tick.snap` (no new system; `move_system` only gained parameters), perf counter
snapshots, `docs/perf/budgets.ron`, `fl_sim`'s `formats__save_codec_formats.snap`.

## Deviations from the brief (decisions the brief could not make)

1. **`NavGrid::build` takes a 6th parameter, `cell: Fx`.** `cell_of(pos)` and unit profiles need the cell size, which
   neither `TerrainGrid` (fields fixed by the brief) nor the content holds. Added accessors `cell_m()` and
   `max_clearance()`. Later tasks calling `build` pass the map's `cell_m`.
2. `PathRules` lives at `fl_sim::nav::PathRules` (not re-exported at the root, to avoid a duplicate API entry);
   `MovementRules` and `SteeringRules` are at the root (`NavGrid::build` takes `&MovementRules`).
3. `SteeringRules` also implements `link` (`idle_every ≥ 1`), which `path_rules_ranges_are_validated` needs.
4. ASCII legend: the brief gives no pass for `F` and `s`; I used `F` deep forest pass 0.5 (blocks vehicles), which
   the brief's pass-0.5 cases (`(0.5, 1.0) → 1`, cost 134/167) need, and `s` swamp pass 0.0 (blocks all).
5. `update_rect` returns the recomputed rect (clamped; a superset of the changed cells).
6. `move.blocked` is added only when non-zero, so runs without blocking (the scale scenario) keep their counter
   snapshots.
7. A step is skipped only when it ends in a *different* cell that is not passable: a unit whose own cell became
   blocked (terrain changed under it) can still move within it or out to a passable cell.
8. Extra small API: `TerrainGrid::type_at`, `CellRect::area`, `fl_test_utils::write_ascii_mod` (the validation tests
   edit the mod files), `Scenario::reload` made public; `m0_save.rs` honours `FL_M0_SAVE`.
9. Files changed outside the brief's list: `crates/fl_sim/tests/{feature,hash,debug_query}.rs` (core v2, new
   command and rules sections; the Task 2 carry) and `crates/fl_game/tests/wiring.rs` (spawn moved off the river). The `open_map_moves_at_road_speed` test uses `core_mod` through
   `../fl_sim/tests/fixtures/core_mod`.

## Concerns

- **Wandering units get stuck.** `debug.wander` still picks straight-line targets; a target across water or a ridge
  leaves the unit pushing against the shore, counting `move.blocked` every tick, until Task 8 routes moves through
  the path queue. The golden replay records this behaviour.
- **TDD order**: tests were written with the implementation; RED evidence is the mutation run above.
- `cargo clippy` prints two clippy.toml config warnings for `fl_sim` (`rand::rng`/`rand::random` paths); the same
  warnings appear in an earlier log (`C:/t/fl/t25-clippy.log`), they are not lint failures.
- `on_terrain_changed` clones the `TerrainGrid` (to borrow it next to the mutable `NavGrid`); fine for debug-command
  rates, worth a scope-based borrow when craters arrive.

## Gate

`cargo xtask gate task --track contract --track sim-core --track content --track tools` on `1f35ff7`
(full log `C:/t/fl/t5-gate2.log`):

```
PASS fmt (2.6s)
PASS clippy-drift (3.0s)
PASS claude-md (0.0s)
FAIL check-scope (0.5s)
    1 of 41 changed files are outside track(s) contract, sim-core, content, tools:
      crates/fl_game/tests/wiring.rs → client
PASS protected (0.5s)
WARN test-guard (3.5s): test-guard: 3 findings (reviewer must accept each)
    TEST-GUARD crates/fl_sim/tests/feature.rs:122 RemovedAssert: header.features [("core", 1), ("test", 3)]
    TEST-GUARD crates/fl_sim/tests/feature.rs:162 RemovedAssert: RULES_SECTIONS ["Spatial", "StateHash"]
    TEST-GUARD crates/fl_sim/tests/hash.rs:370 RemovedAssert: world["header"] features [["core", 1], ["hp", 1]]
PASS clippy (24.8s)
PASS arch-check (6.6s)
FAIL test-headless (122.2s)   golden_replays_verify, save_codecs_of_every_feature,
                              m0_testbed_save_loads_under_m1, m0_save_round_trips_after_load
PASS test-client (79.3s)
FAIL api (52.5s)              fl_sim: changed (.txt.new written)
PASS data-check (15.6s)
WARN gate task took 312s (budget 300s)
gate task: FAIL (check-scope, test-headless, api) in 312s
```

Every failure is expected before the lead acts:
- `test-headless`: the four tests need the blessed golden replay, save-codecs snapshot and `m0_testbed.save`
  (the m0 tests pass against the `.new` with `FL_M0_SAVE`, see above).
- `api`: `fl_sim.txt.new` awaits bless.
- `check-scope`: `crates/fl_game/tests/wiring.rs` (track `client`) — add `--track client` or accept.
- test-guard: the three "removed" asserts were replaced by the same asserts with core v2 and the new rules
  sections.

## Fix round 1

Base: `.gate-base` set to `e89a54d` (the lead's bless commit) as instructed. Commit:
`7ce8824 fix(fl_sim): Task 5 fix round 1: borrow terrain in on_terrain_changed, bulk height hash, validate loaded terrain`.

What changed, by review item:
- **(2) `on_terrain_changed`** (`nav/mod.rs`): no `TerrainGrid` clone any more. A `try_resource_scope` over
  `TerrainGrid` wraps the `NavGrid` scope, and `NavGrid::update_rect` recomputes only the touched rect: grown by 1
  for slope, and by `max_clearance − 1` toward −x/−y for clearance. Test `set_terrain_grows_the_palette` checks
  that `nav.cells_updated` is exactly 25 of 100 cells for a 2×2 rect at (2, 2), and that the partially updated grid
  equals a full `NavGrid::build` after each of two edits. The 64-case proptest `incremental_nav_equals_full_rebuild`
  still covers random rects.
- **(3) Height hash** (`terrain.rs`): the heights go to the hasher in 512-sample (1 KiB) little-endian blocks,
  where it used to be one call per cell. The hash is streaming, so the value does not change: the blessed
  `testbed_wander.replay` still verifies, and no golden `.new` was needed.
- **(4) Loaded terrain validation**: `terrain::check_loaded` runs in `load_world` after the fills and before the
  Derived rebuilds. On failure the load returns `LoadError::Codec { name: "core.terrain", .. }`. It checks:
  - a map is present;
  - the cell counts equal `MapInfo`'s;
  - `types` and `heights` are empty or have one entry per cell;
  - the palette holds at most 255 names, and every name is a `TerrainDef` of the content;
  - every type is a palette index or `OPEN`.

  `load.rs` doc step 7 is updated. Tests: `malformed_terrain_in_a_save_is_a_load_error` (six edits of the saved
  grid: cells, types length, heights length, unknown palette name, type index, 256-name palette) and
  `terrain_without_a_map_in_a_save_is_a_load_error`.
- **(5) Coverage**: `set_terrain_grows_the_palette` uses a base-content flat map, which starts with an empty
  palette. It appends `base.water`, then `base.deep_forest`, and checks that indices are kept and that the
  NavGrid blocks the right cells. `set_terrain_without_a_map_is_rejected` and
  `set_terrain_with_a_full_palette_is_rejected` check for `core.invalid_target`, an unchanged state and an
  advanced `LastSeq`.
- **(6)** `m0_save.rs` reads only the blessed `tests/replays/m0_testbed.save`. The `FL_M0_SAVE` override is gone.
- **(7)** ADR-0006 M1 refinement: "fills a Hashed state name … (Derived state has no codec: the Derived rebuilds
  make it)".
- Also: `fl_test_utils::Scenario::features()` (public, used by the load-error tests).
- Minor 1 (corner cutting and wall hopping) is left for Task 11, as instructed.

### TDD evidence

RED: the new tests ran before the implementation (`cargo nextest run --profile agent --no-fail-fast -p fl_sim
--test terrain -p fl_test_utils --test m0_save`):
```
FAIL fl_sim::terrain terrain_without_a_map_in_a_save_is_a_load_error — panicked at terrain.rs:869: the save loads
FAIL fl_sim::terrain malformed_terrain_in_a_save_is_a_load_error     — panicked at terrain.rs:869: the save loads
Summary [8.504s] 32 tests run: 30 passed, 2 failed
```
The first case of the malformed test (`cells_x + 1`) loaded without complaint before the fix.

`set_terrain_grows_the_palette`, `set_terrain_without_a_map_is_rejected` and
`set_terrain_with_a_full_palette_is_rejected` passed on the RED run. They cover behaviour that already worked
(item 5 asked for coverage, not a change). Items (2) and (3) are refactors: their evidence is that those tests and
the golden replay pass unchanged.

GREEN:
```
cargo nextest run --profile agent --no-fail-fast -p fl_sim --test terrain --test save -p fl_test_utils --test m0_save --test golden
Summary [12.678s] 63 tests run: 63 passed, 0 skipped
cargo nextest run --no-fail-fast -p fl_sim -p fl_test_utils -p fl_catalog        (default profile, in full)
Summary [24.108s] 293 tests run: 293 passed, 0 skipped
cargo clippy -p fl_sim -p fl_test_utils --all-targets -- -D warnings             (clean)
```

### Gate

`cargo xtask gate task --track contract,sim-core,content,tools,docs` on `7ce8824`:
```
PASS fmt (2.6s)
PASS clippy-drift (2.8s)
PASS claude-md (0.0s)
PASS check-scope (0.4s)
PASS protected (0.4s)
PASS test-guard (1.1s)
PASS clippy (22.7s)
PASS arch-check (6.1s)
PASS test-headless (136.5s)
PASS api (47.1s)
PASS data-check (27.1s)
gate task: PASS in 248s
```
No `.new` files: the hash, the API and the snapshots are unchanged.
