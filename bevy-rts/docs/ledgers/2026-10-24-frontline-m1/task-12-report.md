# Task 12 report — hot reload, sim side

**Status:** DONE_WITH_CONCERNS · **Commit:** 4e9d225 `feat(fl_sim)!: hot reload — ID-set check, in-place value reload keeping ratios, runner control message, lenient load` (base 53ddd6e). Trailers: `Refs: ADR-0012`. No `Blessed:` trailer yet: the API `.new` files wait for the lead.

## What was implemented

**fl_content**
- `ContentDb::same_id_sets(&other)`: true when every content string table (`content.unit`, `content.<ext>`) and every registry string table (`def_components`, `object_kinds`, `content_kinds`, `rules_sections`) holds the same names in both.
- `ContentDb::id_set_diff(&other)`: sorted lines `"<table>: +<name>"` / `"<table>: -<name>"`. It is empty exactly when `same_id_sets` is true.
- `DefComponent::reload(&self, ctx, entity)`: defaults to `instantiate`. Docs updated.

**fl_view**
- `ToRunner` is `#[non_exhaustive]` and has a new variant, `Control { id, control: RunnerControl }`. This is R80, already recorded in the ADR-0012 refinement.
- `RunnerControl` (non-exhaustive) has one variant, `ReloadContent(OpaqueContent)`.
- `OpaqueContent(pub Arc<dyn Any + Send + Sync>)`: `Debug` prints `"OpaqueContent"`, and `PartialEq` is `Arc::ptr_eq`.
- `SimReply::Reloaded { tick, rules_hash }` is appended after `Error`, so the postcard indices of the existing variants stay the same. `SimReply` is not traced, so the view formats snapshot is unchanged.

**fl_sim**
- New file `reload.rs` with `apply_content_values(world, content) -> Result<ReloadReport, ReloadError>`. In order, it:
  1. Checks `cheats_allowed` → `CheatsOff`.
  2. Checks the `DefRegistry` fingerprint → `RegistryMismatch`.
  3. Checks `same_id_sets` → `IdSetsDiffer(id_set_diff)`. A refusal at any of these three steps changes nothing.
  4. Records each unit's profile before the reload (`unit_profile`).
  5. Replaces `Content` and `MatchHeaderRes.rules_hash`.
  6. Reloads every unit's def components from the new content, in SimIndex order and then component-name order.
  7. Runs `rebuild_derived` (now `pub(crate)`). That is `rebuild_core` → `rebuild_nav` → `install_nav`, as R101 requires: the grid, the HPA graphs and an empty FlowCache.
  8. In SimIndex order, sends every unit whose profile changed and whose `PathFollow` is `Waiting`, `Following` or `Flow` to `Idle`, journaled `core.order_changed` `"replanned"` (phase Input). Its old request turns stale, and `advance_orders` re-requests next tick.
  9. Journals `core.content_reload { old_rules_hash, rules_hash, units }` (phase Input, tick = the next tick to run).

  It never touches `TerrainGrid`. The journal kind is registered through `reload::build`, called from `CoreFeature::build`.
- `units.rs`:
  - `HullDef::reload` keeps the HP ratio (`rescale_hp`): 0 stays 0, otherwise `max(1, hp × new / old)` in u64. A guard handles `old max == 0`. It replaces `Footprint`.
  - `MobilityDef::reload` inserts `Mobility` only. It adds a default `MoveOrder` or `PathFollow` only if the unit has none.
  - `SelectableDef` keeps the default reload.
- `SimRunner::control(RunnerControl) -> SimReply`: downcasts the payload to `ContentDb`, calls `apply_content_values`, and answers `Reloaded`. Otherwise it answers `Error(text)`, including `"the reload content is not a ContentDb"`.
- The threaded `serve` loop answers `Control` with `FromRunner::Reply { id, .. }`, with the same `catch_unwind` as queries. A message kind it does not know is logged at warn and skipped (wildcard arm).
- `save`:
  - New: `LoadOptions { drop_unknown_units }`, `load_world_with(.., opts)`, and `Loaded.dropped: Vec<(SimId, String)>`.
  - `load_world` is `load_world_with(.., LoadOptions::default())`.
  - With `drop_unknown_units`, an entity is removed from the decoded save before anything spawns when its `core.unit_type` (v1) dense id maps through the save's `content.unit` table to a name the current tables lack. No `SimEntity` is created for it, its SimId stays unused, and a warn log is written.
- `terrain.rs`: a palette name with no `TerrainDef` is now `LoadError::Codec { name: "core.terrain", message: "unknown terrain '<id>'" }`, as in the brief. The existing `malformed_terrain_in_a_save_is_a_load_error` test only checks for `t.lava`, and still passes.

**fl_client tests**: added wildcard arms to the fake runners. That is `_ => {}` in `view.rs`, and `_ => continue` in `debug.rs` and **`terrain.rs`**: there the match is an expression, so `{}` does not type-check.

## Tests

New tests:
- `fl_content/tests/load.rs`: `same_id_sets_true_for_value_change`, `same_id_sets_false_when_unit_added` (diff `["content.unit: +a.gamma"]`, plus the reverse `-a.gamma`), `terrain_kind_added_changes_id_set`. They edit a copy of `mod_a` in a tempdir.
- `fl_view/tests/view.rs`: `opaque_content_eq_is_identity`.
- `fl_sim` unit test: `units::tests::hp_rescale_keeps_ratio`.
- `fl_sim/tests/reload.rs`: 15 tests. These are the brief's 14, plus:
  - `reload_refused_for_another_feature_set` (`RegistryMismatch`);
  - `reload_changing_profile_discards_queued_request`: with a budget of 1 request per tick, the two waiting requests resolve as `"stale"` at charge 0, and every new request plans size 2. It covers the Waiting / queued-request case of ADR-0012.

  `reload_changing_profile_replans` covers both `Following` (#1) and `Flow` (a group of 6 tanks). A truck whose profile is unchanged keeps `Following`.

  The test content is the `write_ascii_mod` mod in a per-process, per-call dir under `CARGO_TARGET_TMPDIR`, edited and then loaded with the scenario's features. A first full run collided on shared dir names across nextest processes, which is why the dirs are now unique.

### TDD evidence
- **RED** (tests written first): `cargo nextest run --profile agent -p fl_content -p fl_view -p fl_sim -E 'test(/reload|same_id|id_set|opaque|drop|terrain_kind_added/)'` failed to compile, as expected:
  ```
  error[E0432]: unresolved imports `fl_view::OpaqueContent`, `fl_view::RunnerControl`
  error[E0599]: no method named `same_id_sets` found for struct `ContentDb` (x5)
  error[E0599]: no method named `id_set_diff` found for struct `ContentDb` (x4)
  error[E0599]: no variant named `Control` found for enum `ToRunner` (x2)
  error: could not compile `fl_content` (test "load") due to 9 previous errors
  error: could not compile `fl_view` (test "view") due to 3 previous errors
  ```
- **GREEN**: the same filter (plus `hp_rescale`) gave `21 tests run: 21 passed, 466 skipped`.
- **Mutation check**: forcing the replan condition to always fire makes `reload_changing_profile_replans` FAIL (reload.rs:662). The fix was then restored.

### Full runs
- `cargo nextest run --profile agent -p fl_sim -p fl_test_utils -p fl_catalog -p fl_cli -p fl_content -p fl_view` → `556 tests run: 556 passed, 0 skipped`. This was before the last added test. That test was then run on its own, as was `binary(reload)` (15/15 passed), and `cargo clippy -p fl_sim --all-targets -- -D warnings` is clean.
- `cargo nextest run --profile agent -p fl_client --features debug_remote` → `154 tests run: 154 passed`.
- The golden replay, formats, system order and perf snapshots are unchanged. `check_replay` ignores string tables and the state hash uses names, so the new journal kind changes no hash.

### Gate
`cargo xtask gate task --track contract --track sim-core --track content --track client`:
```
PASS fmt, clippy-drift, claude-md, check-scope, protected, test-guard, clippy, arch-check,
     test-headless (233.5s), test-client (72.1s), data-check
FAIL api: 3 of 5 crates changed (fl_view, fl_content, fl_sim) — pending bless
WARN gate task took 524s (budget 300s)
gate task: FAIL (api) in 524s
```

## Protected artifacts (.new, untracked, for the lead)
These are additive except for the `ToRunner` attribute:
- `xtask/snapshots/api/fl_content.txt.new`: `+ContentDb::id_set_diff`, `+ContentDb::same_id_sets`, `+DefComponent::reload`.
- `xtask/snapshots/api/fl_view.txt.new`:
  - `ToRunner` becomes `#[non_exhaustive]` and gets `+Control { id, control }`;
  - `+RunnerControl` (non-exhaustive, `ReloadContent(OpaqueContent)`);
  - `+OpaqueContent(pub Arc<dyn Any + Send + Sync>)` (Clone, Debug, PartialEq);
  - `+SimReply::Reloaded { tick, rules_hash }`.
- `xtask/snapshots/api/fl_sim.txt.new`: `+ReloadError`, `+ReloadReport`, `+apply_content_values`, `+LoadOptions`, `+load_world_with`, `+Loaded::dropped`, `+SimRunner::control`, `+HullDef::reload`, `+MobilityDef::reload`.

Bless reason: `M1 Task 12: hot reload runner control, lenient load`.

## Files changed
- Brief's Files list:
  - `crates/fl_content/src/{db,def}.rs`, `crates/fl_content/tests/load.rs`
  - `crates/fl_view/src/{runner,lib}.rs`, `crates/fl_view/tests/view.rs`
  - `crates/fl_sim/src/{units,lib}.rs`, `crates/fl_sim/src/runner/{mod,threaded}.rs`, `crates/fl_sim/src/save/{mod,load}.rs`
  - new `crates/fl_sim/src/reload.rs` and `crates/fl_sim/tests/reload.rs`
  - `crates/fl_client/tests/{debug,view}.rs`
- Outside the list (all small):
  - `crates/fl_client/tests/terrain.rs`: a third exhaustive fake runner the brief missed; it failed to compile.
  - `crates/fl_sim/src/feature.rs`: one line, `crate::reload::build(sim)`, to register the `core.content_reload` journal kind.
  - `crates/fl_sim/src/terrain.rs`: the error message the brief specifies.
  - `crates/fl_sim/src/orders.rs`: the doc of `Reason::Replanned`.

## Self-review / concerns
1. **`HashInterval` is not refreshed on reload.** A reload that changes `StateHash.interval_ticks` keeps the old interval. A `RunConfig` override can't be told apart from the rule at reload time, so I left it. Other rule-derived values are read live each tick, or rebuilt (nav, flow cache max, cluster size).
2. **A def component added to or removed from a unit in a value-only reload.** The component-name table is a registry table, so a reload can still add or remove a component on one unit. An added component is reloaded (the default reload instantiates it). A removed one leaves its runtime components on live entities. The brief doesn't cover this; it is the plan's per-def reload model.
3. **`drop_unknown_units` knows the `core.unit_type` codec.** The generic loader decodes that codec's v1 bytes (a postcard `u16` into `content.unit`) itself. A future v2 of `core.unit_type` must update `UNIT_TYPE` in `save/load.rs`, or entities would fall through to the strict path (an error, not a silent drop).
4. **The threaded runner skips unknown `ToRunner` kinds with a warn log.** It doesn't reply, since it cannot know whether the message carries an id.
5. **Units on a still-valid profile keep old paths through newly blocked terrain.** After a passability change, such a unit follows its path as the ADR specifies. Movement never enters a blocked cell, and the blocked-step re-plan handles the rest. Only a profile change forces a re-plan.
6. The gate took 524 s against a 300 s budget (the known trend). During the gate I ran two short focused nextest builds after adding the last test. These briefly overlapped the gate's builder slot.

---

## Fix round 1 (review: task-12-review.md; rulings R119–R123)

**Commit:** 8a00802 `fix(fl_sim): Task 12 fix round 1: cluster-size reload re-plans flow units, hp clamp, drop needs cheats, guards` (`Refs: ADR-0012`).

### Changes
- **Minor 1:**
  - **Fix:** `apply_content_values` compares the old and new `Path.cluster_cells`. When it changed, every `PathState::Flow` unit counts as changed and goes `Idle`, journaled `replanned`. The `FlowKey.cluster` of such a unit names a cluster of the old layout. Units on a `Following` path keep it: corners are positions, so they stay valid. The helper is now `replan_if_changed`.
  - **Test:** `reload_changing_cluster_size_replans_flow_units`. It reloads with cluster size 16 → 8 and checks:
    - all 6 flow units re-plan, journaled;
    - the single-path tank keeps `Following` and is not journaled;
    - `HpaGraphs` equal a fresh build.
- **Minor 3:** `rescale_hp` clamps to `1..=new_max`. The floor drift (99/100 → 49/50 → 98/100) is documented in the doc comment. The unit test adds a clamp case and a drift pin.
- **Minor 4:** `reload_is_deterministic` loads the edited mod once per run: two independent `ContentDb`s.
- **Minor 5:** the three refusal tests (`CheatsOff`, `IdSetsDiffer`, `RegistryMismatch`) now also assert that the header `rules_hash` is unchanged.
- **Minor 6:** `ReloadError` is `#[non_exhaustive]`.
- **Minor 7 / R122:** new guard unit test `save::load::tests::unit_type_codec_version_matches_the_lenient_decoder`. It builds core's `StateRegistry` and asserts that `core.unit_type` is `Hashed { version: UNIT_TYPE.1 }`.
- **Minor 8:**
  - **Fix:** new `LoadError::DropNeedsCheats`, returned by `load_world_with` when `drop_unknown_units` is set on a save without `cheats_allowed`. The check runs before the sim is built.
  - **Docs:** the `LoadOptions` doc says the option is for testbed rebuilds only.
  - **Test:** `drop_unknown_units_needs_cheats`. The strict load of the same save still fails with `Codec`.
- **Doc notes:** the `apply_content_values` docs now list what a value reload leaves as it was:
  - the `TerrainGrid`;
  - `HashInterval`: a new `StateHash.interval_ticks` applies from the next match start or load (R120);
  - runtime components of def components the new content no longer gives a unit: they stay on live units until the M2 removal hook (R121).

### TDD evidence
- **RED 1:** after writing the tests, `cargo nextest run --profile agent -p fl_sim -E 'test(/reload|drop_unknown|hp_rescale|unit_type_codec/)'` did not compile: `error[E0599]: no variant ... named DropNeedsCheats found for enum LoadError`.
- **RED 2:** with only the `LoadError` variant added (no gating, no clamp, no cluster check), the same command gave `17 tests run: 14 passed, 3 failed`:
  - `hp_rescale_keeps_ratio`: "never above the new maximum";
  - `reload_changing_cluster_size_replans_flow_units`: "#2 re-plans";
  - `drop_unknown_units_needs_cheats`: "a save without cheats loads with dropped units".

  The guard test passed, as a guard should.
- **GREEN:** the same command gave `17 tests run: 17 passed, 376 skipped`.

### Full runs
- `cargo nextest run --profile agent -p fl_sim -p fl_test_utils -p fl_catalog -p fl_cli -p fl_content -p fl_view` → `560 tests run: 560 passed, 0 skipped`.
- `cargo nextest run --profile agent -p fl_client --features debug_remote` → `154 tests run: 154 passed, 0 skipped`.
- No other cargo process ran during either gate.

### Gate
- `cargo xtask gate task --track contract,sim-core,content,tools`, run before the fix commit: `gate task: FAIL (check-scope, api) in 367s`.
  - **check-scope** flagged the three `crates/fl_client/tests/*.rs` files from 4e9d225 as track `client`. The brief's tracks include `client`.
  - **api** failed on the pending bless.
- `cargo xtask gate task --track contract,sim-core,content,tools,client` at 8a00802: every row PASS except `api` (pending bless). `gate task: FAIL (api) in 207s`.

### .new
- `xtask/snapshots/api/fl_sim.txt.new` was regenerated. Relative to the round-0 file it adds `pub fl_sim::LoadError::DropNeedsCheats`, and `ReloadError` becomes `#[non_exhaustive] pub enum fl_sim::ReloadError`.
- `fl_content.txt.new` and `fl_view.txt.new` are unchanged (their sources were not touched).
