# M0 final fix wave A — sim contracts: report

Branch `m0/phase-a` (worktree `C:\fl\a`), base `56e3425`. Commits `2acf466..040185a` (16 commits).
Tracks: contract, sim-core, docs. Status: **DONE_WITH_CONCERNS** (gate pr fails only on the rows that need the
lead: pending `.new` snapshots/budgets/API, and check-scope on brief-sanctioned out-of-track files).

## Commits

| SHA | Subject | Items |
|---|---|---|
| 2acf466 | feat(fl_sim)!: stable reject codes, feature reject reasons, command input caps | 6 (I7/R71), 15 (Minor 17) |
| b13879f | fix(fl_sim)!: remap command type ids by name on load and replay | 1 (I1) |
| 1157bdb | feat(fl_protocol,fl_sim)!: saves of older feature versions load; codec bumps need migrations | 2 (I2/R70), 7 (Minor 4) |
| ffa8221 | fix(fl_sim)!: classification and SimEntity checks every tick; SimEntity is sealed | 3 (I3), 5 (I6), 16 (T30) |
| 5ff8032 | feat(fl_sim)!: the framework enforces Transient state (R72) | 4 (I4/R72) |
| afae5fe | fix(fl_sim)!: save_world returns Result; a failed save no longer ends the match | 8 (Minor 7) |
| e038a8a | feat(fl_sim): build_id names the git revision | 9 (Minor 9) |
| bbd2fe1 | feat(fl_sim)!: register_stream returns a Stream token, and rolls draws only from one | 10 (Minor 10) |
| f922d5f | feat(fl_sim): counting systems need no order (SimCounters ambiguity allowed) | 11 (Minor 11) |
| 852d89a | perf(fl_sim): count the spatial tracker's scan (spatial.checked) | 12 (Minor 12) |
| 0dbbf6e | feat(fl_sim)!: one Derived rebuild per feature, run in feature load order | 13 (Minor 15) |
| 313374c | docs: ADR-0006/0007 refinements and spec revision 2.2 (final M0 review) | 2, 13, 14 (Minor 16) |
| e5404bb | docs: Housekeeping holds the Transient resets; ADR-0005 refinements | 4 (doc follow-through) |
| 9ffc8dd | style(fl_sim): name the Derived rebuild type (clippy type_complexity) | — |
| f33bcf7 | docs(fl_sim): register_component/register_resource name their Transient rules | 4 |
| 040185a | chore: fl_sim API snapshot (.new) for the final M0 fix wave | API row |

## Per item

### 1. I1 — remap pending/recorded command ids by name
- `commands.rs`: `remap_command_id` / `remap_command_ids` (crate-private) map a `CommandTypeId` through
  `LoadCtx::remap(tables::COMMANDS, id)`. A name the sim lacks is an error; an id the saved table never had (an
  `UnknownType` command in a replay) stays unknown (`u16::MAX`, never valid) so it is still rejected as `UnknownType`.
- `load_world` remaps `pending` (new `LoadError::Command`). `replay_app` validates every recorded command; new
  `pub fn replay_inputs(r, &app) -> Result<Vec<TickInput>, ReplayError>` returns the remapped inputs (new
  `ReplayError::Command`); `verify_replay` plays `replay_inputs`.
- Tests: `save.rs` `pending_command_ids_remap_on_load` (feature `"aaa"` v1 with/without `"aaa.noop"`, which sorts
  first; the delivered command is still the move), `pending_command_with_unknown_name_fails_to_load`;
  `replay.rs` `replay_remaps_command_ids_by_name`, `replay_with_unknown_command_name_is_an_error`.
- RED: with `remap_command_ids` disabled, `replay_remaps_command_ids_by_name` panicked at replay.rs:386 (ids not
  remapped); before the API existed both files failed to compile (`LoadError::Command`, `replay_inputs`).
- Call site outside my tracks: `crates/fl_cli/src/agent.rs` replay source now builds its `ReplayDriver` from
  `fl_sim::replay_inputs` (3 lines), otherwise `fl_cli` replays would still use raw ids.

### 2. I2 / R70 — save compatibility; format guard
- `fl_protocol::MatchHeader::check_save`: same feature ids (order-free), each saved version ≤ build's; mods equal;
  format version ≤ ours. `check_replay`/`check_handshake` unchanged (strict). Header doc: `features` is load order.
- `codec_formats` heads a block `<name>@<v> reads 1, 2:` (migrations) or `reads older:` (`Codec::custom`).
  `fl_protocol/tests/support/format_guard.rs`: parses `reads`; fails "`<name>`: version rose from A to B without a
  migration from A".
- Tests: `fl_protocol/tests/protocol.rs` `save_check_accepts_older_feature_versions` (RED: failed at line 198 before
  the fix), `replay_and_handshake_stay_strict_on_feature_versions`; `fl_sim/tests/save.rs`
  `save_of_an_older_feature_version_loads` (feature `test_hp` v1 save → v2 build, migration v1→v2; newer → Features);
  `fl_sim/tests/formats.rs` `format_guard_flags_bump_without_migration` + updated unbumped test.
- ADR-0006 and ADR-0007 "Refinements (2026-10-24)".

### 3. I3 — classification during the match
- `step_tick` (debug builds) runs `check_tick` after `SimTick`: re-runs `check_classification` when
  `world.components().len()` grew since the last pass (watermark resource `ClassifiedUpTo`,
  `TransientKept "core.classified_up_to"`), panicking with the names.
- `is_sim_type`: any path segment, generic args included; every `bevy_ecs::…::Messages<…>` counts, so only
  `SimBuilder::add_message` (which classifies it) can add one.
- `ChildOf`, `Children`, `Observer`: any entity holding one fails `check_classification` (instance check by
  `TypeId`, so a merely registered type is not flagged).
- Tests: unit `late_registered_sim_type_fails_its_tick` (resource inserted via `Commands` at tick 1),
  `sim_types_are_recognised_by_any_path_segment`; `tests/classify.rs`
  `message_buffer_added_around_the_builder_is_unclassified` (`app_mut().add_message`),
  `observers_are_not_allowed_in_the_sim_world`, `relationships_are_not_allowed_in_the_sim_world`.
- RED: with the src change reverted, 4 of the new `tests/classify.rs` tests failed (lines 148, 197, …).

### 4. I4 / R72 — framework-enforced Transient
- `Class` API: `Transient` (components only: removed every tick), `TransientReset(fn(&mut T))` /
  `Class::transient()` (`Default`) and `TransientKept(&'static str)` (resources only). Plain `Transient` on a resource
  and Reset/Kept on a component panic with a message.
- One exclusive core system `reset_transients` in `Input::Housekeeping` (core slot) resets every resettable resource
  and removes every Transient component. The per-tick lists are collected at freeze (no per-tick allocation).
  Resets use `try_resource_scope` (`register_resource` does not require a `Mutable` resource).
- `step_tick` clears every `add_message` buffer after taking outputs, before `clear_trackers`; the per-message
  `rotate_messages` systems and `clear_sim_events`/`clear_counters`/`clear_command_log` are gone. Core
  classification: `SimEvents`, `SimCounters`, `CommandLog` → `TransientReset(<type>::clear)` (keeps capacity);
  `Journal`, `SnapshotOut`, `CommandInbox`, message buffers, `ClassifiedUpTo` → `TransientKept(reason)`.
- Golden fixture `"aaa.draws"`: now "this tick's sum", `Class::transient()`. The golden-stability hashes are
  unchanged (the test passes unmodified otherwise); no golden output changed.
- Tests: `schedule.rs` `messages_are_cleared_at_the_end_of_each_tick` (replaces `messages_rotate_once_per_tick`),
  `a_message_never_reaches_the_next_tick`, `transient_components_are_removed_every_tick` (RED: all three failed
  before: `[true, true, true]`, len 1, read 9); `classify.rs` `transient_resources_reset_every_tick_unless_kept`,
  `plain_transient_resource_panics`, `kept_transient_component_panics`.
- `fl_test_utils/tests/determinism.rs` `smoke_shuffle_invariant`: its non-vacuity check relied on the five
  unordered Housekeeping clears; the M0 game has no other unordered systems, so the test adds a `ShuffleProbe`
  feature with five read-only unordered systems (two gave a 50% chance of an unchanged order per seed).

### 5. I6 — sealed SimEntity
- `SimEntity(SimId)` field private; `pub(crate) fn new`, `pub fn id()`. Debug `check_sim_entities` in `step_tick`:
  every non-empty archetype without `SimEntity` that holds a classified component fails (resources are entities in
  0.20; only classified components count). Allocation-free when passing (keeps alloc budgets).
- Test: `tests/classify.rs` `entity_spawned_around_the_funnel_fails_its_tick` (`Commands::spawn(Pose)` at tick 1).
- Tests that built bare `SimEntity` values now spawn via `setup_spawn` (schedule.rs, hash.rs) or a crafted save
  (`hash.rs` `populate`). `hash_independent_of_spawn_order` keeps the Entity-shift part; the "opposite spawn order"
  part is gone (ids now come from the allocator; `ids.rs` unit tests still cover index order).

### 6. I7 / R71
- `RejectReason` `#[non_exhaustive]`, new `TooManyUnits`, `QueueFull`, `Feature(&'static str)`; `code()` gives
  `core.late`, `core.early`, `core.duplicate_seq`, `core.unknown_type`, `core.bad_payload`, `core.unknown_player`,
  `core.debug_disabled`, `core.no_valid_units`, `core.unknown_content`, `core.too_many_units`, `core.queue_full`, and
  feature codes verbatim (debug-assert they contain a `.`). `SimEvent::CommandRejected.reason` (field name kept, no
  fl_view wire change) and the `core.command_rejected` journal `reason` carry the code.
- Tests: `commands.rs` `reject_codes_are_stable`, `feature_reject_reason_reaches_log_and_events`; observe/commands
  assertions updated to codes. RED: compile errors (variants/`code` missing).

### 7. Minor 4 — codec snapshot of every feature
- Codec section moved to new `crates/fl_test_utils/tests/formats.rs` (`fl_catalog::features()` on the base mod; file
  named `formats.rs` because the guard reads `formats__<name>.snap`). `fl_sim/tests/formats.rs` keeps the save and
  replay layouts. `Codec::custom(version, encode, decode, trace)`; `trace_type` is public. `core.unit_type` now
  traces `U16` (the bytes it writes; postcard newtypes are transparent, so the encoding is unchanged — a trace
  correction at the same version, which the guard would flag only against a committed block; the block is new).

### 8. Minor 7 — `save_world` returns `Result`
- `SaveError::PendingRequests { spawns, despawns }`; `SimQuery::Save` → `SimReply::Error`. Tests:
  `save_with_pending_spawn_requests_is_an_error`, `save_with_waiting_despawn_request_is_an_error`,
  `failed_save_query_replies_with_an_error_and_the_match_runs_on`.
- Call sites: `fl_test_utils/src/builder.rs` (`expect`), `fl_cli/src/agent.rs` and `fl_cli/src/save_cmd.rs`
  (`CliError::Setup`). fl_client/fl_game do not call `save_world` (they use the query): unchanged.

### 9. Minor 9 — real `build_id`
- New `crates/fl_sim/build.rs`: `FL_GIT_REV = git rev-parse --short HEAD` + `-dirty` (`git diff-index --quiet HEAD
  --`, which does not write the index); `rerun-if-changed` on `build.rs`, `<absolute-git-dir>/HEAD`,
  `<common-dir>/packed-refs` and `<common-dir>/<branch ref>`. Without git: nothing set, `"dev"`.
- Test: `build::tests::build_id_names_the_git_revision_in_a_checkout` (RED: "a build in a git checkout sets
  FL_GIT_REV"). Verified output: `FL_GIT_REV=afae5fe-dirty`, worktree paths resolved.

### 10. Minor 10 — `Stream` token
- `register_stream(name) -> Stream` (`#[must_use]`), `rolls(seed, tick, id, Stream)`. Core systems get it via
  `IntoSystem::with_input` (`InMut<Stream>`, only copied) so system names (system order, profiler) are unchanged.
  `"setup.scale"` stays registered (reserved; placement draws use the scenario's own seed in setup code).
  Call sites updated: movement.rs, scale.rs, tests golden.rs, movement.rs, rng.rs unit test.

### 11. Minor 11 — `allow_ambiguous_resource::<SimCounters>()` with a §15 comment. Test
  `counting_systems_need_no_order` (RED: "SimTick schedule failed to build: 1 pairs …").

### 12. Minor 12 — `spatial.checked` (one per SimIndex entry visited). Test: `spatial_counters_recorded` asserts 64
  (RED: 0). Budgets and totals changed → `.new` (below).

### 13. Minor 15 — one Derived rebuild per feature, load order
- `Class::Derived` (unit); `SimBuilder::register_rebuild(fn(&mut World))`, a second call panics; `load_world` runs
  the rebuilds in feature load order. Core: `rebuild_spatial`; `SimEntity`/`SimIndex` Derived, hook-maintained.
- Tests: `derived_rebuilds_run_once_per_feature_in_load_order` (`aaa` depends on `zzz` → `["zzz", "aaa"]`),
  `a_second_rebuild_panics`. ADR-0006 refinement.

### 14. Minor 16 — spec "Revision 2.2 (2026-10-24, final M0 review refinements)": features load order, R70, R71, R72,
  R74/item 13. ADR-0005/0006/0007 refinements also added.

### 15. Minor 17 — `MAX_COMMAND_UNITS = 256` (before dedup, `core.too_many_units`), `MAX_WAYPOINTS = 32`
  (queued move onto a full order, whole command rejected, nothing changed, `core.queue_full`). Implemented as `pub
  const`s in `fl_sim::commands` (exported), not a `rules.ron` section. Tests `units_per_command_are_capped`,
  `queued_waypoints_are_capped`.

### 16. T30 — `unclassified_sim_type_fails_build` is `#[cfg(debug_assertions)]`.

## `.new` artifacts (not blessed) and hash/golden changes

| File | Why |
|---|---|
| `crates/fl_sim/tests/snapshots/formats__save_codec_formats.snap.new` | codec blocks moved out (Minor 4); layouts unchanged |
| `crates/fl_test_utils/tests/snapshots/formats__save_codecs.snap.new` | new codec snapshot; `reads` labels; `core.unit_type` traces `U16` |
| `crates/fl_test_utils/tests/snapshots/system_order__sim_tick.snap.new` | Housekeeping: 5 clear/rotate systems → `fl_sim::classify::reset_transients` |
| `crates/fl_test_utils/tests/snapshots/perf_counters__scale_{500,3000,5000}.snap.new` | totals gain `spatial.checked` only |
| `docs/perf/budgets.ron.new` | `spatial.checked` budgets; alloc budgets +820 B / +3 per scenario: the extra counter key costs one more BTreeMap node per tick (measured: without the counter, the re-recorded alloc budgets equal the committed ones). Recorded with `FL_RECORD_BUDGETS=1`, one scenario per run |
| `xtask/snapshots/api/fl_sim.txt.new` | public API of the contract changes (58 changed lines) |

No state hash and no golden replay changed: the golden replay and every hash/branch/twin/shuffle test pass unchanged.

## Gate

`cargo xtask gate pr --track contract,sim-core,docs` (log `C:/t/fl/fixa-gate-pr.log`, run before the API `.new`
commit): `gate pr: FAIL (check-scope, test-headless, api) in 492s`.
- PASS: fmt, clippy-drift, claude-md, protected, clippy, arch-check, test-client, deny, data-check.
- WARN test-guard: 33 findings (removed/renamed tests and asserts listed above, new `should_panic`s) — reviewer must
  accept each.
- FAIL check-scope: `crates/fl_cli/src/{agent,save_cmd}.rs` (tools: I1 and Minor 7 call sites),
  `docs/perf/budgets.ron.new` (perf: brief-mandated `.new`); now also `xtask/snapshots/api/fl_sim.txt.new`.
- FAIL test-headless: only the pending snapshots/budgets (nextest stops at the first failure).
- FAIL api: fl_sim changed → `fl_sim.txt.new` written and committed (040185a).

Full run without fail-fast (`cargo nextest run --profile agent --workspace --exclude probe_dynlink --exclude
probe_remote --no-fail-fast`, gpu/brp/milestone excluded): **594 run, 589 passed, 5 failed, 1 skipped**; the 5 are
exactly the pending artifacts: `fl_sim::formats save_codec_formats`, `fl_test_utils::formats
save_codecs_of_every_feature`, `fl_test_utils::system_order sim_tick_system_order`,
`perf_counters smoke_counters_scale_500`, `perf_counters counters_scale_3000` (milestone 5000 likewise). Re-running
regenerated byte-identical `.new` files. `cargo fmt --check` clean; clippy `-D warnings` clean (the gate's clippy row
PASS). After blessing the six artifacts, gate pr should pass apart from check-scope.

Note: the first gate pr attempt died with "not enough space on disk (os error 112)" — C: was briefly at 0 bytes free
(now ~70 GB); nothing of mine was deleted.

## Concerns / follow-ups
1. **Docs in fixb's paths are now stale** (I did not touch them): `.claude/rules/sim.md:38` ("Derived … names its
   rebuild fn") and `:62` ("Messages rotate in Input::Housekeeping"); `.claude/agents/reviewer.md:41`
   (`Derived` with a rebuild fn). Should say `register_rebuild` per feature, messages cleared at end of `step_tick`,
   `Class::transient()/TransientReset/TransientKept` for resources, `rolls` takes a `Stream`.
2. **Transient component cost:** removal every tick is an archetype move per entity (R72's stated cost); M1's `Steer`
   on thousands of units will pay it each tick, and `remove_all` builds a `QueryState` per Transient type per tick
   (allocates). Not exercised in M0 (no Transient components).
3. **`build_id` churn:** every commit (and the dirty flag) changes `FL_GIT_REV`, so fl_sim and its dependents
   rebuild after each commit; the dirty flag reflects the tree when HEAD last moved, not later edits (documented in
   `build.rs`).
4. **Minor 17** caps are code constants, not a rules section (brief said "core rules constants"); a queued move
   onto any full unit rejects the whole command.
5. `try_resource_scope` per resettable Transient resource per tick (3 core resources): archetype moves, no measured
   allocation, small cost.
6. The new fl_test_utils codec snapshot has no committed predecessor, so the guard could not check the moved blocks
   against the old fl_sim ones this time (only diffs: `reads` labels and the `unit_type` trace).

---

# Fix round 1 (review `fix-a-review.md`; wave B merged as 1099b76)

Commits `c2b60e5..c4f18f7` (5 commits) on top of the merge `1099b76`.

| SHA | Subject | Review item |
|---|---|---|
| c2b60e5 | fix(fl_sim): build script watches only git paths that exist | Important 1, Minor 1 |
| 4f41ac8 | fix(fl_sim)!: debug checks every tick, missing commands table, review minors | Minors 2, 3, 4, 7, 8, 9 |
| cb5a62e | fix(fl_sim): the raw RNG cursor is clippy-banned in sim crates | Minor 5 |
| c4f18f7 | docs(.claude): sim rules and reviewer checklist follow the wave-A contracts | stale docs |

## Changes

1. **Important 1 — build.rs.** Confirmed before the fix: every `cargo check -p fl_sim -v` printed
   `Dirty fl_sim v0.0.0 (C:\fl\a\crates\fl_sim): the file `C:/Projects/bevy-experiments/.git\packed-refs` is missing`
   and recompiled. Now: `HEAD` (if present); the loose branch ref if it exists, else `packed-refs` and `refs/heads`
   only if they exist. `cargo check -p fl_sim -v` twice after the fix:
   - 1st: `Dirty fl_sim v0.0.0 (C:\fl\a\crates\fl_sim): the file `crates\fl_sim\build.rs` has changed (…)` then
     `Compiling fl_sim v0.0.0 (C:\fl\a\crates\fl_sim)`
   - 2nd: `Fresh fl_sim v0.0.0 (C:\fl\a\crates\fl_sim)`
   Emitted: `rerun-if-changed=build.rs`, `…/.git/worktrees/a\HEAD`, `…/.git\refs/heads/m0/phase-a` (no
   `packed-refs`). The build.rs doc also notes the git ≥ 2.31 requirement of `--path-format=absolute`.
2. **Minor 1 — ADR-0007** now says what is computed: `-dirty` reflects the tree when `HEAD`/its branch last moved;
   later uncommitted edits build as the clean revision, so the handshake's build-id check tells commits apart, not
   uncommitted edits.
3. **Minor 2 — classify.rs** `check_tick` panic messages have their `\` continuations back. (Root cause: my edit
   tooling collapsed `\`; I scanned fl_sim/fl_protocol src for other run-together string literals and found one more,
   the new spawn.rs assert, fixed.)
4. **Minor 3 — `remap_command_id`** returns `Err("the save has no string table 'commands'")` when the table is
   absent (→ `LoadError::Command` / `ReplayError::Command`). Test `pending_command_without_a_commands_table_fails_to_load`.
5. **Minor 4** — `check_tick` runs `forbidden_in_world` (ChildOf/Children/Observer by instance, archetype scan,
   allocation-free when clean) after every tick, not only on component-count growth. Test
   `forbidden_component_instanced_mid_match_fails_its_tick` (both types registered at build by a system query, the
   child spawned in tick 1).
6. **Minor 5** — `fl_math::Rolls::new` and `fl_math::roll` are banned (`disallowed-methods`) in the sim clippy profile
   (`xtask/clippy-template.toml`, regenerated `crates/{fl_protocol,fl_sim,fl_view}/clippy.toml`). Scoped
   `#[expect(clippy::disallowed_methods)]` on `fl_sim::rolls`, its unit test and the scale scenario's setup placement.
   `stream_key` stays public (the state hash keys names with it; a key alone draws nothing). The rng module doc states
   exactly this.
7. **Minor 7** — in debug builds `step_tick` panics (`spawn::assert_requests_drained`) before the end-of-tick clear
   if a SpawnRequest/DespawnRequest is left, i.e. was written after its funnel. Test
   `spawn_request_after_the_funnel_fails_its_tick`.
8. **Minor 8** — `SaveError` is `#[non_exhaustive]`.
9. **Minor 9** — opposite-order spawning restored as the unit test `hash::tests::hash_independent_of_spawn_order`
   (#2 then #1 with `SimEntity::new`, after filler entities, vs #1 then #2); the integration test that only shifts
   Entity bits is renamed `hash_independent_of_entity_bits`.
10. **Stale docs** — `.claude/rules/sim.md` (Randomness, Classification, schedule bullets) and
    `.claude/agents/reviewer.md` (Randomness, Classification) describe one `register_rebuild` per feature in load
    order, framework-enforced Transient, messages only via `add_message` and cleared at the end of each tick, draws
    only via `fl_sim::rolls` over a registered `Stream`, stable reject codes.

Minor 6 and 10: parked as instructed.

## Tests

RED (src changes of 4f41ac8 reverted, new tests kept): `pending_command_without_a_commands_table_fails_to_load`
panicked "the save loaded" (save.rs:69); `forbidden_component_instanced_mid_match_fails_its_tick` and
`spawn_request_after_the_funnel_fails_its_tick` failed (no panic). GREEN: `cargo test -p fl_sim --no-fail-fast`
passes except the pending `formats` snapshot.

Full run: `cargo nextest run --profile agent --workspace --exclude probe_dynlink --exclude probe_remote --no-fail-fast
-E 'not (test(/gpu_/) | test(/brp_/) | test(/milestone_/))'` → **606 run, 601 passed, 5 failed, 3 skipped**; the 5
are the unblessed artifacts only (`fl_sim::formats save_codec_formats`, `fl_test_utils::formats
save_codecs_of_every_feature`, `fl_test_utils::system_order smoke_sim_tick_system_order`, `perf_counters
smoke_counters_scale_500` and `counters_scale_3000`). The regenerated `.new` files are byte-identical to the committed
ones (clean tree). Workspace clippy `-D warnings` (probes excluded) clean, with wave B's new bans.

## Gate

`cargo xtask gate pr --base 56e3425 --track contract,sim-core,docs,tools,client` (log `C:/t/fl/fixa-gate-pr-r1.log`):
`gate pr: FAIL (check-scope, test-headless, api) in 1089s`.
- PASS: fmt, clippy-drift, claude-md, protected, clippy, arch-check, test-client, deny, data-check. WARN test-guard:
  43 findings (the 33 of the first round plus this round's new should_panics and the renamed hash test).
- check-scope: `docs/perf/M0.json`, `docs/perf/M0.md` (from the wave B merge, perf track) and
  `docs/perf/budgets.ron.new` (the unblessed budgets).
- test-headless: `scale_3000: spatial.checked max 10000 has no budget` — the unblessed `budgets.ron.new` (nextest
  fail-fast); every other failure in the full run is an unblessed snapshot.
- api: `fl_sim: changed` — the unblessed `xtask/snapshots/api/fl_sim.txt.new`.

## Concerns

- `xtask/snapshots/api/fl_sim.txt.new` is now stale by this round's `SaveError` `#[non_exhaustive]` (one line). I
  did not touch `.new` files as instructed; run `cargo xtask api` before blessing it.
- check-scope also lists `docs/perf/M0.{json,md}`, which came in with the wave B merge (perf track), not from this
  round.

---

# Fix round 2 (milestone gate: `smoke_shuffle_invariant`, seed 0x5eed0002)

Commit `d0506b0` (test(fl_test_utils): shuffle probes are schedule roots, so every seed reorders them), on top of the
bless commit `f247a85`.

## Root cause

Bevy 0.20's `shuffle_seed` (`schedule.rs` ~1306) shuffles only the order of the flattened dependency graph's
*nodes*; it re-adds every node's outgoing edges in their original order. The topological order is the reverse of
Tarjan's SCC/DFS finish order (`graph_map.rs` `toposort`), with the outer loop over nodes in (shuffled) node order and
the DFS following each node's (unshuffled) edge order. So:
- a **root** (no predecessor) is started by the outer loop, and roots end up in reverse shuffled order — a real
  permutation;
- a system with a predecessor is normally reached through it and visited in the predecessor's unshuffled edge
  order; it moves only if the shuffle puts it before all of its ancestors in the node list.

Before wave A, the M0 game's only unordered systems were the five Input/Housekeeping clear/rotate systems, the first
systems of the schedule — roots — so every seed permuted them. Wave A merged them into one `reset_transients`
(`allow_ambiguous_resource::<SimCounters>()` played no part: it orders nothing). The five probes I added then sat in
Think/Behaviour behind `wander_system`; seeds 1 and 3 happened to reorder them, seed 2 (only run at milestone level,
`FL_SHUFFLE_SEEDS=3`) left the whole order unchanged — reproduced locally with `FL_SHUFFLE_SEEDS=3`.

## Fix

`fl_test_utils/tests/determinism.rs`: the `ShuffleProbe` feature adds eight systems that access nothing
(`probe_0`..`probe_7`) to `SimTick` outside every hook, `.ambiguous_with_all()` (they touch no data, so running
anywhere, even beside an exclusive system, is harmless). Each is a root, so their relative order is the reverse of
their shuffled node order (a seed leaves it unchanged with probability 1/40320). The check now asserts that every seed
reorders the probes themselves (not just "the order string differs"), and that all eight are present. The hash
invariance run still includes the probes. Probe orders (digits in executed order): unshuffled `46231705`; seed
0x5eed0001 `04713265`, 0x5eed0002 `23671504`, 0x5eed0003 `37426510`.

`FL_SHUFFLE_SEEDS=3 cargo test -p fl_test_utils --test determinism`: 10 passed.

## Gate

`cargo xtask gate milestone --milestone M0` (log `C:/t/fl/fixa-gate-m0.log`): fmt, clippy-drift, claude-md, clippy,
arch-check, test-headless, test-client, api, deny, data-check, gpu, brp, perf-file all PASS —
`gate milestone: PASS in 829s`.

Note: during this round `docs/superpowers/plans/2026-10-24-frontline-m1.md` changed in this worktree (not by me); I
did not stage or commit it.
