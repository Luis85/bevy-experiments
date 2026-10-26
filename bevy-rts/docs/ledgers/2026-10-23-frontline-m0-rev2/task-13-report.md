# Task 13 report: save/load skeleton (typed migrations, round trip, branch test)

**Status:** DONE_WITH_CONCERNS (concerns are follow-ups outside this task's files; see the end)
**Commit:** `b155188 feat(fl_sim): versioned save/load with typed migrations, round-trip and branch tests` (trailers `Refs: ADR-0006`, `Co-Authored-By`)
**Branch:** `m0/phase-a` (C:\fl\a), base `.gate-base` = 611bc39

## What I implemented

- `crates/fl_sim/src/save/format.rs`: `SAVE_FORMAT_VERSION = 1`, `SaveFile { format_version, header, sim }`,
  `SimSave { tick, sim_id_next, pending_commands, resources, entities }` exactly as the brief.
  `SavedValue = (String, u32, Vec<u8>)` is a type alias (clippy `type_complexity`); the field types are
  the brief's types. `SaveCtx` (current string tables) and `LoadCtx` (saved + current tables, remaps
  precomputed per table present in both). `LoadCtx::remap(table, old) -> Result<u32, String>` errors on:
  table missing in the save, table missing in this sim, id out of range, name gone. This settles the
  T6 carry: a missing table is an error, never an empty remap.
- `crates/fl_sim/src/save/mod.rs`: `save_world(world, pending) -> Vec<u8>`. Resources in registry
  (name) order, entities in `SimIndex` order, components in name order, each only when present.
  `pending` is stored sorted by `(exec_tick, player, seq)`. It **asserts** that the
  `Messages<SpawnRequest>`/`Messages<DespawnRequest>` buffers are empty (T11 carry): they are
  Transient, so a save would silently drop gameplay input.
- `crates/fl_sim/src/save/load.rs`: `load_world(features, content, bytes, run) -> Result<Loaded, LoadError>`,
  `Loaded { app, pending, last_seq }`, `LoadError` with exactly the brief's variants. Procedure:
  1. reads the leading `format_version` varint first (so a future layout change still reports
     `FormatVersion`, not `Decode`), then decodes strictly (trailing bytes are an error); also checks that
     entities are strictly ascending and `1..=sim_id_next`, and that resource and component names are
     strictly ascending (a non-canonical or duplicated save is `Decode`);
  2. `build_sim_app` with `MatchSetup { seed, cheats_allowed }` from the saved header;
  3. `current.check_save(&saved)` → `Header(..)` (rules hash not compared);
  4. spawns each entity with its saved id via `spawn_with_id` (inside `resource_scope::<StateRegistry>`);
  5. decodes resources and components by name with migration and remap. A Hashed resource the save lacks
     is removed (so `MapInfo` absent stays absent); unknown names → `UnknownResource`/`UnknownComponent`;
  6. sets `SimIdAllocator` and `SimClock` from `sim_id_next`/`tick`;
  7. runs every `Derived` rebuild (components and resources merged) in name order.
  `Loaded.last_seq = last_seqs(world)` and `Loaded.pending` = the saved pending commands, ready for
  `TestDriver::restore` / `CommandStamper::restore` (T7 carry).
- `classify.rs`: `Codec<T>` now has `encode: fn(&T, &SaveCtx) -> Vec<u8>`,
  `decode: fn(u32, &[u8], &LoadCtx) -> Result<T, String>`, a private migrations list and a private
  `decodes_old` flag. New: `Codec::new(version)` (the postcard/serde codec; `Class::hashed(v)` is
  `Class::Hashed(Codec::new(v))`), `Codec::custom(version, encode, decode)`,
  `Codec::migrate_from::<Old>(version)` (asserts `version < self.version` and no duplicate).
  Dispatch (`decode_saved`): a migration for the saved version wins; else `decode` for the current
  version (or, for a custom codec, any older version, so it can handle its own history); else
  `NoMigration { from, to }`. All postcard decodes are strict (no trailing bytes).
  `ComponentCodec`/`ResourceCodec` gained `save`/`load` (and `remove` for resources); the Task-9
  `#[expect(dead_code)]` markers are gone (StateClass is now read). **Contract note:** `Codec` is no
  longer `Copy` (it holds a `Vec`), its `encode`/`decode` signatures changed, and it can no longer be
  built with a struct literal outside the crate. No other crate used `Codec`.
- `ids.rs`: `pub(crate) fn spawn_with_id(world, id, bundle) -> EntityWorldMut` (the one place a
  `SimEntity` is created) and `SimIdAllocator::with_issued`. `spawn.rs::setup_spawn` now spawns through
  `spawn_with_id(world, id, (Owner, Pose))`. Load passes `()`. Load does **not** re-run def
  instantiation: the saved components are the state, and instantiation would add components the
  entity may have lost at runtime.
- `units.rs`: `UnitType` uses `Codec::custom(1, ..)`: it saves the dense `u16` and remaps it through
  `tables::UNITS` on load.
- `runner/mod.rs`: `SimQuery::Save { pending }` → `SimReply::Save(save_world(world, &pending))`.
- `lib.rs`: exports `LoadCtx, LoadError, Loaded, SAVE_FORMAT_VERSION, SaveCtx, SaveFile, SavedValue,
  SimSave, load_world, save_world`; crate doc paragraph.
- `tests/runner.rs` (`other_queries_answer`): the old "Save → Error(not implemented)" assert now asserts
  `SimReply::Save(non-empty bytes)`.

## Tests (`crates/fl_sim/tests/save.rs`, hash interval 1)

The 11 brief tests plus 4 extra:
- `round_trip_is_byte_identical` (duel + wander, 50 ticks; also equal state hash after load)
- `save_at_tick_zero_round_trips` (bytes equal, and a branch from 0 to 30 matches)
- `branch_test_hashes_match_every_tick` (A 0..120: wander at 0, moves at 10 and 60; B to 40, save, load, same script to 120; hashes 40..120 equal)
- `branch_after_despawns` (duel + 4 tanks; a test feature despawns #3 and #5 at tick 20; `debug.spawn` at 50 must yield #7 after the load; asserts the gaps and #7, then branch at 40 to 120)
- `pending_commands_survive_save` (delay 2; move issued at 10 for 12; saved after tick 10 with 1 pending; load + `restore`; hashes 11..30 equal; the unit moved)
- `load_rejects_unknown_component_name` (`core.pose` → `core.posee` on both entities → `UnknownComponent("core.posee")`)
- `load_rejects_unknown_resource_name` (extra)
- `load_rejects_feature_mismatch` (`Header(HeaderMismatch::Features)`)
- `load_rejects_other_format_versions` (extra: `FormatVersion { found: 2, supported: 1 }`; garbage → `Decode`)
- `migration_from_v1` (test component at v2 `.migrate_from::<v1::TestHp>(1)`; v1 bytes → migrated value; v2 bytes as is; v0 → `NoMigration { "test_hp.hp", 0, 2 }`; bad v1 bytes → `Codec`)
- `load_remaps_unit_type_ids` (saved with core_mod, loaded with core_mod_extra; asserts the tank's dense id differs between the two contents, names preserved, equal state hash)
- `load_rejects_missing_string_table` (extra: `content.unit` removed from the saved header → `Codec { name: "core.unit_type", message mentions the table }`)
- `derived_state_rebuilt_on_load` (SimIndex ids equal, with gaps; every index entry's `SimEntity` matches)
- `save_with_pending_spawn_requests_panics` (extra, `#[should_panic(expected = "spawn request")]`)
- `runner_save_query` (reply bytes == `save_world`, and they load at tick 5)

Unit test `classify::tests::hashed_codec_round_trips` updated to the new signatures, plus asserts for
`decode_saved` (current version, `NoMigration`, trailing bytes → `Codec`).

## TDD evidence

RED: `cargo nextest run --profile agent -p fl_sim --test save` before implementation:
```
error[E0432]: unresolved imports `fl_sim::LoadError`, `fl_sim::Loaded`, `fl_sim::SAVE_FORMAT_VERSION`, `fl_sim::SaveFile`, `fl_sim::load_world`, `fl_sim::save_world`
error[E0599]: no associated function or constant named `new` found for struct `Codec<T>` in the current scope
error: could not compile `fl_sim` (test "save") due to 2 previous errors
```
Expected: the save API and `Codec::new` did not exist.

GREEN: same command after implementation:
```
Starting 15 tests across 1 binary
Summary [   1.274s] 15 tests run: 15 passed, 0 skipped
```
Full crate: `cargo nextest run --profile agent -p fl_sim` → `147 tests run: 147 passed, 0 skipped`.

## Gate

`cargo xtask gate task --track contract --track sim-core` (first run failed on clippy `type_complexity`
x3 and fmt; fixed with type aliases and `cargo fmt`; second run):
```
PASS fmt (1.3s)
PASS clippy-drift (2.3s)
PASS claude-md (0.0s)
PASS check-scope (0.2s)
PASS protected (0.2s)
WARN test-guard (0.9s): test-guard: 3 findings (reviewer must accept each)
    TEST-GUARD crates/fl_sim/src/classify.rs:382 RemovedAssert: assert_eq!((codec.decode)(&bytes).expect("decodes"), sample);
    TEST-GUARD crates/fl_sim/tests/runner.rs:321 RemovedAssert: assert_eq!( runner.query(SimQuery::Save { pending: Vec::new() }), SimReply::Error("save: not implemented".to_owned()) );
    TEST-GUARD crates/fl_sim/tests/save.rs:608 NewShouldPanic: #[should_panic(expected = "spawn request")]
PASS clippy (11.8s)
PASS arch-check (3.6s)
PASS test-headless (47.8s)
SKIP test-client: no client package touched since 611bc39 (.gate-base)
PASS data-check (19.4s)
gate task: PASS in 88s
```
Test-guard findings, for the reviewer:
1. `classify.rs:382`: the decode assert is rewritten for the new signature
   (`(codec.decode)(4, &bytes, &ctx)`), not removed; more asserts were added.
2. `runner.rs:321`: the "not implemented" assert is replaced by an assert that Save returns bytes
   (the behaviour this task adds).
3. `save.rs` `should_panic`: the save-time guard for waiting spawn requests (T11 carry).

The clippy log also shows 2 old warnings from the generated `fl_sim/clippy.toml` (`rand::rng`/`rand::random`
paths not resolvable). They are not from this task and don't fail the gate.

## Files changed

- New: `crates/fl_sim/src/save/{mod,format,load}.rs`, `crates/fl_sim/tests/save.rs`
- Modified: `crates/fl_sim/src/{classify,ids,lib,units,spawn}.rs`, `crates/fl_sim/src/runner/mod.rs`,
  `crates/fl_sim/tests/runner.rs`
- `spawn.rs` and `tests/runner.rs` are outside the brief's Files list. `spawn.rs` changed so
  `setup_spawn` and load share `spawn_with_id`, which the brief asks for. `runner.rs` replaces the
  assert that had pinned the old "not implemented" reply. Both are in the sim-core track, and
  check-scope passes.

## Self-review notes / decisions

- `SimClock`/`SimIdAllocator` appear twice in the save: once in `SimSave.tick`/`sim_id_next` and once
  as Hashed resources `core.clock`/`core.sim_id_allocator`, because every Hashed resource is saved. Step 6 runs after
  step 5, so the explicit fields win. They always agree for saves made by `save_world`.
- `SimSave.sim_id_next` holds the allocator's state, which is the last id issued (the allocator's
  private field is called `next`). Its doc comment says this.
- `save_world` panics instead of returning a Result when requests are waiting, as the brief's
  `-> Vec<u8>` requires. The runner's `Save` query therefore panics on that bug. Today no core
  system writes requests after `Lifecycle`.

## Concerns (follow-ups, not in this task's files)

1. **`PlayerInfo.faction` is a dense content id (`FactionId`, an index into `content.faction`).** It
   is saved as is and also hashed as the dense id (`players.rs`, `write_u16(faction.0)`), which goes
   against the name-key rule. A save loaded against content that renumbers factions would get the wrong
   faction. The fix is a custom codec for `Players` that remaps through `content.faction`, plus hashing
   via `write_registry_id`. It belongs to `players.rs` (sim-core), which this task does not own.
2. The ADR-0006 confirmation items still open: `serde-reflection` snapshots of the save codecs and the
   fitness tests in `fl_test_utils` (round trip and branch at several split points for `gate pr`) are
   not in this brief's Files list.
3. Pending `PlayerCommand`s carry dense `CommandTypeId`s. That is safe because `check_save` requires
   identical features, and with them an identical command table.

---

## Fix report 1 (Ruling R52): factions by name, extra save tests

**Commit:** `192152d fix(fl_sim): factions saved and hashed by name` (Refs: ADR-0004, ADR-0006)

### Changes
- `players.rs`: `pub(crate) const FACTIONS = "content.faction"`. A unit test checks it equals
  `format!("content.{}", Faction::EXT)`. `PlayerInfo::sim_hash` now writes the faction as
  `h.write_registry_id(FACTIONS, faction)` (its name key) instead of `write_u16(dense id)`. New
  `players_codec()` = `Codec::custom(1, encode_players, decode_players)`. The encoding is still the
  postcard bytes of `Players` (dense ids valid in the save's table). Decode remaps each
  `PlayerInfo.faction` through `LoadCtx::remap(FACTIONS, ..)`, so a missing faction or table is a
  `LoadError::Codec` for `core.players`. The version stays 1 because the byte layout did not change.
  Only the hash changed, and no committed saves or hash snapshots exist.
- `feature.rs`: `core.players` is registered with `Class::Hashed(players_codec())`.
- New fixture `tests/fixtures/core_mod_factions` is `core_mod` plus `core_test.aaa_first`, a
  faction that sorts first, so red and blue get new dense ids. The mod id and version are the same,
  so `check_save` passes.
- New tests in `tests/save.rs`:
  - `faction_renumbering_keeps_the_hash`: fresh duels in core_mod and core_mod_factions have
    different dense faction ids and the same names. Their hash streams over 10 ticks of
    `moves_script` are equal.
  - `load_remaps_faction_ids`: a core_mod save loaded into core_mod_factions keeps the names
    (`red`, `blue`), the dense ids differ, and the state hash equals the core_mod load's.
  - `save_with_waiting_despawn_request_panics`: `#[should_panic(expected = "1 despawn request")]`.
  - `pending_commands_survive_save` extended: `last_seq` is empty at the save (no command had
    applied). After the load, at tick 20, a second move for player 0 is stamped with
    `seq == pending seq + 1` and `exec_tick 22`. Both runs issue it, the hashes for 11..30 stay equal,
    and #1's `MoveOrder` ends as `[(150, 600)]`, so the move applied rather than being rejected as a
    duplicate.

### Evidence
RED check: I put back `h.write_u16(self.faction.0)` temporarily and ran
`cargo nextest run --profile agent -p fl_sim --test save`:
```
FAIL fl_sim::save load_remaps_faction_ids          (left: 9041249823310155870, right: 1732771310983713608)
FAIL fl_sim::save faction_renumbering_keeps_the_hash (hash streams differ from tick 0)
Summary 18 tests run: 16 passed, 2 failed
```
After restoring the fix, `cargo nextest run --profile agent -p fl_sim` gives
`Summary [14.168s] 151 tests run: 151 passed, 0 skipped`.

Gate: `cargo xtask gate task --track contract --track sim-core` (base 611bc39):
```
PASS fmt (2.3s)
PASS clippy-drift (3.8s)
PASS claude-md (0.0s)
PASS check-scope (0.5s)
PASS protected (0.5s)
WARN test-guard (2.7s): test-guard: 4 findings (reviewer must accept each)
    TEST-GUARD crates/fl_sim/src/classify.rs:382 RemovedAssert: assert_eq!((codec.decode)(&bytes).expect("decodes"), sample);
    TEST-GUARD crates/fl_sim/tests/runner.rs:321 RemovedAssert: assert_eq!( runner.query(SimQuery::Save { pending: Vec::new() }), SimReply::Error("save: not implemented".to_owned()) );
    TEST-GUARD crates/fl_sim/tests/save.rs:631 NewShouldPanic: #[should_panic(expected = "spawn request")]
    TEST-GUARD crates/fl_sim/tests/save.rs:646 NewShouldPanic: #[should_panic(expected = "1 despawn request")]
PASS clippy (21.7s)
PASS arch-check (9.0s)
PASS test-headless (75.7s)
SKIP test-client: no client package touched since 611bc39 (.gate-base)
PASS data-check (29.0s)
gate task: PASS in 146s
```
The new test-guard finding (`save.rs:646`) is the despawn-request guard test this ruling asked for.
The other three were accepted in the Task 13 review.

### Remaining concerns
- `ScaleScenario` (`scale.rs`) still assigns `FactionId(i % faction_count)` by dense index. It is
  only used at setup and saved and hashed by name now, so this is fine.
- Still open from ADR-0006: serde-reflection codec snapshots and the `fl_test_utils` fitness tests.
