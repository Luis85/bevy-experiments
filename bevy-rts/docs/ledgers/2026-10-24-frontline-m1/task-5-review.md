# Task 5 review: sim terrain, NavGrid, speed formula, debug.set_terrain, fill_missing

Reviewer pass over `review-t5.diff` (52bed82..1f35ff7, 41 files), read in four passes (contract/save,
nav+terrain+movement, tests, fixtures/snapshots). Read-only; no cargo builds. Line numbers are in the
post-change files.

### Spec Compliance

- ✅ Spec compliant. Every brief interface is present with the brief's semantics:
  `Codec::fill_missing` (classify.rs:223), `run_fills` after restore + allocator/clock and before
  `rebuild_derived` (save/load.rs:124-125, 259), gated on saved feature version < build's, sorted by
  header feature order then name; `TerrainGrid` Hashed `core.terrain` v1 with palette hashed as
  names (terrain.rs:153); `NavGrid` Derived `core.nav_grid` (nav/mod.rs:246-254); one core rebuild
  `rebuild_core` = spatial then nav, `spatial::build` no longer registers (feature.rs, spatial.rs);
  speed/cost formulas (nav/grid.rs:85, 91); slope over in-map 4-neighbours (nav/grid.rs:424);
  off-map clearance 0 and centred anchor (nav/grid.rs:332); rules sections with link ranges;
  `MobilityDef.terrain_factor` in [-1, 1]; `SpawnError::BlockedCell`; `core.invalid_target` on
  spawn/teleport/set_terrain; journal `core.terrain_changed`; counter `nav.cells_updated`;
  `SIM_VERSION` 2, core v2; both debug queries answer `Err`, never panic; every brief test exists.
- Checks run (named risks):
  - Header feature order = load order: `build_sim_app` fills `MatchHeader.features` from
    `sort_features` output (build.rs:149), so `run_fills`' `position` in `ours` is load order. OK.
  - `def_profile` (HullDef.footprint.x) vs `profile_of` (Footprint.width): units.rs:98-99 sets
    `width = footprint.x`, so spawn checks and movement agree. OK.
  - M0 fixture provenance: committed blob sha256 `cd5a8656…85b1` = report; header bytes carry
    `build_id "0.0.0+0ae615d"` (tag `m0` = 0ae615d), `sim_version 1`, features `[core 1]`, M0
    tables (no `terrain` content kind, no `debug.set_terrain`). Tick 40 is asserted by the test.
  - Golden replay: dumped old (52bed82) and new with the existing `target/debug/fl_cli.exe replay
    dump`. Only differences: header (build_id, core 1→2, sim_version 1→2, rules_hash, tables
    +Movement/Path/Steering, +debug.set_terrain, +core.terrain_changed, +core.nav_grid/core.terrain)
    and the dense `debug.wander` type_id 7→8 in the two tick-0 commands. Payloads, units, ticks,
    exec ticks, `start`, `events` (empty both) identical; 400/400 hashes differ, as expected once
    `TerrainGrid` is hashed from tick 0 and movement is terrain-scaled.
  - API snapshot: 177 added lines, 0 removed; additions match the report.
  - Save-codecs snapshot: only `core.terrain@1` added.
  - f32/f64/HashMap: none added in sim or test-utils code; height scaling is i128 integer
    (terrain.rs height_at), ASCII heights exact via `height_m (0, 65535)`.
- ⚠️ Cannot verify from diff: `fl_game` wiring test's (500, 400) being grass on the testbed (gate
  `test-client` PASS is the evidence); that `debug.set_terrain` is `DEBUG_ONLY` in fl_protocol
  (Task 2; `set_terrain_needs_cheats` covers it).

### Strengths

- Incremental `update_rect` is correct by construction (blocking grown by 1, clearance grown by
  `max_clearance − 1` toward −x/−y, reverse sweep reads only current or untouched neighbours) and
  is proven by a 64-case proptest against a full rebuild (tests/terrain.rs:637).
- Fill ordering is proven end to end: the M0 test asserts `NavGrid == fresh build` after load,
  which only holds if the terrain fill ran before `rebuild_core` (rebuild_nav removes the grid
  without terrain).
- `m0_save_round_trips_after_load` asserts a byte-identical resave, which proves the fill ran once
  and the filled state is plain saved state.
- Hand-written `NavGrid: PartialEq` excluding observe-only `version`; `save_right_after_set_terrain`
  checks live v1 vs loaded v0 grids equal and branch hashes for 50 ticks.
- Rejections all happen before mutation (R45), and tests assert TerrainGrid/NavGrid/LastSeq.
- One-rebuild-per-feature is structurally enforced (`register_rebuild` panics on a second one).

### Issues

#### Critical (Must Fix)

None.

#### Important (Should Fix)

None.

#### Minor (Nice to Have)

1. movement.rs:196-203 — the blocked check tests only the destination cell. A size-1 unit can cut a
   diagonal corner between two blocked cells, and on maps with `cell_m` below the largest step
   (200 km/h = 2.78 m/tick; `cell_m` is only checked > 0) a unit can hop a one-cell wall. Base maps
   (4 m) are safe from hopping. Carry to Task 11 (`units_never_enter_blocked_cells`): segment/corner
   rule consistent with Task 7's neighbour rule, or a content rule `cell_m ≥ max step`.
2. nav/mod.rs:106-118 — `on_terrain_changed` clones the whole `TerrainGrid` per command (up to
   4096² cells ≈ 48 MB under R89). Use `resource_scope::<TerrainGrid>` around the `NavGrid` scope.
3. terrain.rs:165-167 — heights hashed with one `write_bytes` call per cell; hash them as one byte
   buffer (or `u16` slice) — every hashed tick pays 62.5k calls on the testbed.
4. TerrainGrid decode does not validate invariants (types/heights length = cells or empty; cells
   = MapInfo cells; palette ≤ 255 names that content defines). A malformed save panics in
   `set_rect` slicing (terrain.rs:146), and a palette name the content lacks silently becomes open
   ground (nav/grid.rs `OPEN_KIND`). Add a decode-time check returning a `LoadError::Codec`.
5. Coverage: palette growth through `debug.set_terrain` is never exercised at NavGrid level — the
   ASCII palette always holds all seven kinds (fl_test_utils/src/terrain.rs:217) and the proptest
   palette is fixed (tests/terrain.rs:613). Add a case setting a kind absent from the map's palette.
   Also untested: set_terrain "no map" and "full palette" rejections.
6. fl_test_utils/tests/m0_save.rs:25-31 — `FL_M0_SAVE` lets the guard test read an arbitrary file
   in place of the protected fixture; drop it once `m0_testbed.save` is blessed (see ruling).
7. ADR-0006 M1 refinement and the brief say a fill covers a "Hashed or Derived" name; Derived state
   has no codec, so only Hashed can carry a fill (classify.rs:223 doc says so). Fix the ADR wording.
8. Process: no RED-before-GREEN run (mutation evidence substitutes); gate `task` took 312 s (budget
   300 s); two clippy.toml config warnings in the clippy output (pre-existing).
9. Test-guard: the three RemovedAssert findings (feature.rs:122, 162; hash.rs:370) are the same
   asserts updated to core v2 and the new rules sections — accept.

### Rulings recommended on the concerns

- `NavGrid::build` gains `cell: Fx`: accept. TerrainGrid's fields are fixed by the brief and the
  content has no map cell for a bare grid; update Tasks 7/10/13 briefs to pass `MapInfo.cell`.
- `PathRules` at `fl_sim::nav` (MovementRules/SteeringRules at root): accept; update later briefs'
  paths to `fl_sim::nav::PathRules`.
- ASCII `F` pass 0.5 (and `s` pass 0.0): accept — the brief's own expected values (pass-0.5 speed
  case, costs 134/167) require it; record the legend in the plan.
- Wanderers stop at shores and count `move.blocked` every tick until Task 8: accept. Inputs are
  unchanged, the golden records it, scale counters are unaffected (open ground). Task 8 must route
  wander and move targets through path requests (with goal snapping) and re-bless the golden.
- `m0_save.rs` honours `FL_M0_SAVE`: accept for this task's evidence only; remove the override
  after the bless (protected fixtures have no env override, like the golden and snapshots).
- `fl_game/tests/wiring.rs` move to (500, 400): accept (terrain correctly rejects a tank on the
  river); accept the check-scope `client` finding.

### Bless recommendations

- `m0_testbed.save.new` → bless (`M1 Task 5: M0 testbed save fixture (R78)`): built at tag m0
  (build_id `0.0.0+0ae615d`, sim 1, core 1, M0 tables), sha matches the report.
- `testbed_wander.replay.new` → bless (`M1 Task 5: Hashed terrain grid, derived NavGrid, SIM_VERSION
  2`): header-and-dense-id-only input changes verified; hash stream change expected.
- `formats__save_codecs.snap.new` → bless: only `core.terrain@1` added.
- `xtask/snapshots/api/fl_sim.txt.new` → bless: additions only, matching the report.

### Assessment

**Task quality:** Approved

**Reasoning:** Terrain, nav and fill_missing are deterministic (integer/Fx only, sorted iteration,
Hashed/Derived split correct, fills before rebuilds, incremental == full rebuild proven), and the
M0 fixture and golden change are verified from bytes; remaining items are latent movement edge
cases, perf and coverage polish.
