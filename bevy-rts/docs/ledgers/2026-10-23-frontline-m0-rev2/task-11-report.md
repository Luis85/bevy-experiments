# Task 11 report: (player, seq) command apply, spawn funnel, debug.* handlers, scenario seed

**Status:** DONE_WITH_CONCERNS (the concerns are small; see the end). Commit `aea2eae` on `m0/phase-a`.

## What I implemented

- **`commands.rs`**
  - `CommandInbox(Vec<PlayerCommand>)`, Transient `"core.command_inbox"`. `step_tick` fills it, sorted by
    `(player, seq)`.
  - `apply_commands` (exclusive, `InputSet::Apply` core slot) takes the inbox, stable-sorts it by
    `(player, seq)` again, and checks each command in the brief's order:
    1. `Late` / `Early`;
    2. `DuplicateSeq` (`seq <= LastSeq[player]`). Otherwise the command takes its seq.
    3. `UnknownType`, then `BadPayload`, then `UnknownPlayer`;
    4. `DebugDisabled`;
    5. unit filtering: dedup and sort; the unit must exist, be owned by the player (skipped for `DEBUG_ONLY`,
       R44) and pass `unit_can`. Nothing left with `NEEDS_UNITS` → `NoValidUnits`;
    6. the handler (`UnknownContent` and similar).
  - `RejectReason` has the brief's 9 variants. Every rejection:
    - is logged at `debug`;
    - goes into `CommandLog.rejected`;
    - adds to counter `commands.rejected`;
    - pushes `SimEvent::CommandRejected { player, seq, reason: "<Variant>" }`.
  - An applied command adds to counter `commands.applied`.
  - `CommandQueue`, `submit` and the queue unit test are removed.
  - **Carried item, `CommandLog`:** cleared every tick by `clear_command_log` in the `InputSet::Housekeeping`
    core slot. It stays Transient and now holds this tick only.
  - **Carried item, maps:** `core.move` now needs a map. Without a `MapInfo` it returns `NoValidUnits`,
    so a target is never unbounded.
- **`spawn.rs`**
  - New types: `SpawnRequest { unit: Option<UnitTypeId>, owner, pos, heading, init: Option<SpawnInit> }`,
    `SpawnInit` (a fn pointer), `DespawnRequest(SimId)` and `SpawnError`.
  - `setup_spawn(world, &req)`:
    - checks the unit, map, player and position, in that order;
    - allocates the `SimId`;
    - spawns `SimEntity + Owner + Pose`, plus `UnitType` and every def component when `unit` is `Some`;
    - runs `init` last.
  - `setup_despawn(world, id) -> bool`.
  - `spawn_funnel` (exclusive, `LifecycleSet::Spawn` core slot):
    - drains `Messages<SpawnRequest>` (oldest buffer first, which is write order);
    - runs `setup_spawn` on each request;
    - on success: counter `spawn.entities` and `SimEvent::Spawned`;
    - on failure: counter `spawn.rejected` and a `debug` log, never a panic.
  - `despawn_funnel` (exclusive, `LifecycleSet::Despawn` core slot) drains the requests, sorts and dedups
    the ids, and despawns each one. Each despawn adds to counter `despawn.entities` and pushes
    `SimEvent::Despawned`. A request for an id that doesn't exist is logged.
  - Messages: `"core.msg.spawn"` and `"core.msg.despawn"`, both through `add_message` (R47).
  - `spawn_unit` and `despawn_unit` are removed. ADR-0005's "spawn funnel in `Lifecycle::Spawn`" is now
    true.
- **`ids.rs`:** `SimIdAllocator::alloc` is `pub(crate)`.
- **`debug_cmds.rs` (new):**
  - `Invulnerable`, Hashed `"core.invulnerable"`.
  - The six handlers:

    | Handler | Behaviour |
    |---|---|
    | `debug.spawn` | Resolves the name with `unit_id` (else `UnknownContent`), checks the owner (else `UnknownPlayer`), writes a `SpawnRequest`. |
    | `debug.set_owner` | `UnknownPlayer` for an unknown owner. |
    | `debug.set_hp` | Needs `Health`; sets `hp = min(hp, max)`. |
    | `debug.teleport` | Needs `Pose` and a map; sets `pos = map.clamp(pos)` and clears the `MoveOrder` waypoints. |
    | `debug.invulnerable` | Inserts or removes `Invulnerable`. |
    | `debug.wander` | Rev-1 behaviour, but it now needs a map (`NoValidUnits` without one), so the radius is always capped. |
- **`scenario.rs`**
  - `MatchSetup::for_scenario(content, id)`: `seed` is the scenario's seed and `cheats_allowed` is false.
  - `setup_scenario_def` no longer reseeds. It is all-or-nothing:
    - it resolves the map, players, and every unit's type, player slot and position before changing
      anything;
    - it then inserts the map and players, and spawns through `setup_spawn`.
  - New `SetupError::Unit { index, unit, error }` names the offending unit. `SetupError::Spawn` stays for
    the scale scenario.
- **`units.rs`** (carried minors)
  - km/h → m/tick is `kmh·10 / (36·SIM_TICK_HZ)`. Both integers are exact at any tick rate, so nothing
    truncates.
  - The shared helpers `speed_per_tick` and `turn_per_tick` are used by both `instantiate` and `validate`.
    A positive rate that rounds to 0 per tick is now a validation error.
- **`scale.rs`:** spawns through `setup_spawn`.
- **`step.rs`:** fills the inbox instead of calling `submit`.
- **`feature.rs`:** core builds `debug_cmds` and `spawn`.
- **`lib.rs`:** exports updated.
- **`hash.rs`:** deleted `write_unchecked_registry_id`. Only `CommandQueue` used it, so it became dead code.

### Core system order now (relevant lines)
- `Input/Housekeeping`: `clear_counters`, `clear_sim_events`, `clear_command_log`, `rotate_messages` for
  spawn and despawn.
- `Input/Apply`: `apply_commands`.
- `Lifecycle/Spawn`: `spawn_funnel`.
- `Lifecycle/Despawn`: `despawn_funnel`.

## Judgement calls (please review)
1. **Late and Early don't take a seq.** They are rejected at check 1, before the seq check. Every other
   non-duplicate command takes its seq (R45). `late_and_early_rejected` asserts that `LastSeq` stays empty.
2. **`bad_commands_do_not_change_state` compares the full hash, `LastSeq` included.**
   - Both runs send a valid command with seq 7 at tick 30, so their `LastSeq` end equal ({P0: 7}). The
     test-only exclusion of `LastSeq` from Task 10 is gone.
   - Run B asserts `LastSeq == {P0: 6}` right after the bad tick (R45: it advances).
   - It also asserts 7 `CommandRejected` events, and the 7 reasons summed over ticks in `(player, seq)`
     order. The duplicate has seq 0, so it is first.
   - The run uses `cheats_allowed: true`, so the `debug.spawn "nope"` case is `UnknownContent` rather
     than `DebugDisabled`.
3. **A `debug.spawn` that is off the map is applied, then fails in the funnel.**
   - The handler rejects what it can know: an unknown name (`UnknownContent`) or an unknown owner
     (`UnknownPlayer`).
   - The position is checked by the shared spawn code. A failure there is logged as "spawn request
     failed" and counted in `spawn.rejected`. It is not a command rejection, because none of the 9
     `RejectReason`s fits.
   - This is my reading of "a failed DebugSpawn must be logged as rejected/failed".
4. **The funnels drain the message buffers.** A feature `MessageReader<SpawnRequest>` therefore never sees
   a request. Requests written after `Lifecycle/Spawn` (including from a `SpawnInit`) are carried out in
   the next tick.
5. **No map means `NoValidUnits` for move, teleport and wander.** I chose rejecting over clamping: units
   cannot exist without a map through `setup_spawn` anyway.
6. **Files outside the Files list** (all in the `sim-core` / `contract` tracks; `check-scope` passed):
   - `tests/hash.rs` and `tests/schedule.rs`: they called `SimIdAllocator::alloc`.
     - Bare test entities now use fixed `SimId`s.
     - `sim_id_allocator_changes_hash` advances the allocator through `setup_spawn` + `setup_despawn` on
       a map with one player.
     - `feature_systems_run_after_the_core_systems_of_their_hook` now expects `[1, 0]`, because the log
       holds one tick.
   - `tests/feature.rs`: the header's command table lists the 8 commands.
   - `src/hash.rs`: the dead helper (see above).
7. **Extra tests beyond the brief:**
   - `rejections_raise_events_and_the_log_holds_one_tick`
   - `debug_spawn_unknown_owner_rejected`
   - `non_unit_spawn_has_owner_and_pose_only`
   - `scenario_setup_is_all_or_nothing`
   - `mobility_rejects_rates_that_round_to_zero_per_tick`
   - unit test `tiny_rates_round_to_zero_per_tick`
   - `setup_scenario_spawns_units` also asserts that `MatchSeed` stays the `MatchSetup`'s seed (1, not the
     duel's 42)

## Tests

### TDD evidence
- **RED:** I wrote the tests first (`tests/{commands,funnel,movement,hash,schedule}.rs`) and ran
  `cargo nextest run --profile agent -p fl_sim`. It failed to compile, as expected, because the API did
  not exist yet:
  ```
  error[E0599]: no variant, associated function, or constant named `DuplicateSeq` found for enum `RejectReason`
  error[E0599]: no variant, associated function, or constant named `UnknownContent` found for enum `RejectReason`
  error[E0599]: no variant, associated function, or constant named `Early` found for enum `RejectReason`
  error[E0599]: no associated function or constant named `for_scenario` found for struct `MatchSetup`
  error[E0432]: unresolved imports `fl_sim::SpawnRequest`, `fl_sim::setup_despawn`, `fl_sim::setup_spawn`
  error[E0599]: no variant named `Unit` found for enum `SetupError`
  error[E0432]: unresolved imports `fl_sim::DespawnRequest`, `fl_sim::Invulnerable`
  ```
- **First run after implementing:** 109/111. Both failures were test fixes, not implementation bugs:
  - `header_lists_features_mods_and_tables` expected 4 commands; there are now 8.
  - `sim_id_allocator_changes_hash` mixed fixed ids with allocated ones, and the `SimIndex` "already used"
    panic caught it. The test now builds without `populate()`.
- **GREEN:** `cargo nextest run --profile agent -p fl_sim`
  ```
  Summary [   8.429s] 111 tests run: 111 passed, 0 skipped
  ```
- `cargo clippy -p fl_sim --all-targets -- -D warnings`: clean. The only warnings are the two existing
  `rand::rng`/`rand::random` path warnings from the generated `clippy.toml`.

### Gate: `cargo xtask gate task --track contract --track sim-core` (`.gate-base` = 296046f)

Notes on the test-guard findings (expected port churn):
- The `CommandQueue` unit test went with the queue.
- `KMH_PER_M_PER_TICK` became the num/den pair; the test still checks that the divisor is 72 and adds
  exact checks.
- `late_command_rejected` became `late_and_early_rejected`.
- `debug_spawn_requires_flag` became `debug_requires_cheats` + `debug_spawn_appears_at_end_of_tick`.
- `debug_spawn_resolves_the_unit_name` became `debug_spawn_unknown_unit_rejected`: the brief makes an
  unknown name `UnknownContent`. The valid-name case asserts no rejection in
  `debug_spawn_appears_at_end_of_tick`.
- `spawn_unit` and `despawn_unit` asserts are now the same asserts through `setup_spawn` and
  `setup_despawn`.
- `applied.len()` asserts now go through the `applied(&app)` helper.
- The `bad_commands` asserts got stronger (7 reasons, full hash).
- The schedule `[1, 1]` → `[1, 0]` change: the log is per tick now.

```
PASS fmt (4.1s)
PASS clippy-drift (4.6s)
PASS claude-md (0.0s)
PASS check-scope (0.4s)
PASS protected (0.3s)
WARN test-guard (2.6s): test-guard: 32 findings (reviewer must accept each)
    TEST-GUARD crates/fl_sim/src/commands.rs:509 RemovedTest: #[test] fn queue_takes_due_commands_in_tick_then_push_order() {
    TEST-GUARD crates/fl_sim/src/commands.rs:515 RemovedAssert: assert_eq!(queue.push(cmd(0, 5), Tick(1)), Err(RejectReason::Late));
    TEST-GUARD crates/fl_sim/src/commands.rs:521 RemovedAssert: assert_eq!(due, [1, 1, 2]);
    TEST-GUARD crates/fl_sim/src/commands.rs:527 RemovedAssert: assert_eq!(due, [3, 5]);
    TEST-GUARD crates/fl_sim/src/commands.rs:528 RemovedAssert: assert!(queue.take_due(Tick(9)).is_empty());
    TEST-GUARD crates/fl_sim/src/units.rs:321 RemovedAssert: assert_eq!(KMH_PER_M_PER_TICK, 72);
    TEST-GUARD crates/fl_sim/tests/commands.rs:137 RemovedAssert: assert_eq!(app.world().resource::<CommandLog>().applied.len(), 1);
    TEST-GUARD crates/fl_sim/tests/commands.rs:190 RemovedAssert: assert!(app.world().resource::<CommandLog>().applied.is_empty());
    TEST-GUARD crates/fl_sim/tests/commands.rs:204 RemovedTest: #[test] fn late_command_rejected() {
    TEST-GUARD crates/fl_sim/tests/commands.rs:209 RemovedAssert: assert_eq!(submit(app.world_mut(), cmd), Err(RejectReason::Late));
    TEST-GUARD crates/fl_sim/tests/commands.rs:210 RemovedAssert: assert_eq!(rejected(&app), [RejectReason::Late]);
    TEST-GUARD crates/fl_sim/tests/commands.rs:213 RemovedAssert: assert_eq!(submit(app.world_mut(), cmd), Ok(()));
    TEST-GUARD crates/fl_sim/tests/commands.rs:235 RemovedTest: #[test] fn debug_spawn_requires_flag() {
    TEST-GUARD crates/fl_sim/tests/commands.rs:249 RemovedAssert: assert_eq!(app.world().resource::<SimIndex>().len(), 3);
    TEST-GUARD crates/fl_sim/tests/commands.rs:262 RemovedTest: #[test] fn debug_spawn_resolves_the_unit_name() {
    TEST-GUARD crates/fl_sim/tests/commands.rs:268 RemovedAssert: assert!( rejected(&app).is_empty(), "an unknown unit is logged, not rejected" );
    TEST-GUARD crates/fl_sim/tests/commands.rs:272 RemovedAssert: assert_eq!( app.world().resource::<SimIndex>().len(), 2, "nothing spawned" );
    TEST-GUARD crates/fl_sim/tests/commands.rs:336 RemovedAssert: assert_eq!( names, [ Some("core.move"), Some("core.stop"), Some("debug.spawn"), Some("debug.wander") ] );
    TEST-GUARD crates/fl_sim/tests/commands.rs:368 RemovedAssert: assert_eq!(submit(app.world_mut(), late), Err(RejectReason::Late));
    TEST-GUARD crates/fl_sim/tests/commands.rs:386 RemovedAssert: assert_eq!(clean, dirty);
    TEST-GUARD crates/fl_sim/tests/commands.rs:387 RemovedAssert: assert_eq!( last_seqs(app.world()), [(PlayerId(0), 4)].into_iter().collect() );
    TEST-GUARD crates/fl_sim/tests/commands.rs:391 RemovedAssert: assert_eq!( rejected(&app), [ RejectReason::Late, RejectReason::UnknownType, RejectReason::BadPayload, RejectReason::NoValidUnits, ] );
    TEST-GUARD crates/fl_sim/tests/feature.rs:140 RemovedAssert: assert_eq!( names(tables::COMMANDS), ["core.move", "core.stop", "debug.spawn", "debug.wander"] );
    TEST-GUARD crates/fl_sim/tests/movement.rs:163 RemovedAssert: assert_eq!( spawn_unit(world, tank, PlayerId(0), pos, Angle::ZERO), Err(SpawnError::OutsideMap), "{pos:?}" );
    TEST-GUARD crates/fl_sim/tests/movement.rs:170 RemovedAssert: assert_eq!( spawn_unit(world, tank, PlayerId(0), edge, Angle::ZERO), Ok(SimId(3)) );
    TEST-GUARD crates/fl_sim/tests/movement.rs:174 RemovedAssert: assert_eq!( spawn_unit(world, tank, PlayerId(9), v(5, 5), Angle::ZERO), Err(SpawnError::UnknownPlayer) );
    TEST-GUARD crates/fl_sim/tests/movement.rs:181 RemovedAssert: assert_eq!( spawn_unit(bare.world_mut(), tank, PlayerId(0), v(5, 5), Angle::ZERO), Err(SpawnError::NoMap) );
    TEST-GUARD crates/fl_sim/tests/movement.rs:192 RemovedAssert: assert_eq!( spawn_unit( app.world_mut(), out_of_range, PlayerId(0), v(5, 5), Angle::ZERO ), Err(SpawnError::UnknownUnit) );
    TEST-GUARD crates/fl_sim/tests/movement.rs:209 RemovedAssert: assert!(fl_sim::despawn_unit(app.world_mut(), SimId(1)));
    TEST-GUARD crates/fl_sim/tests/movement.rs:210 RemovedAssert: assert!(!fl_sim::despawn_unit(app.world_mut(), SimId(1)));
    TEST-GUARD crates/fl_sim/tests/schedule.rs:386 RemovedAssert: assert_eq!( world.resource::<AfterCore>().rejected_in_apply, [1, 1], "the command was applied (and rejected) before the feature system ran" );
    TEST-GUARD crates/fl_sim/tests/schedule.rs:391 RemovedAssert: assert_eq!( world.resource::<CommandLog>().rejected[0].1, RejectReason::UnknownPlayer );
PASS clippy (33.1s)
PASS arch-check (7.6s)
PASS test-headless (90.1s)
SKIP test-client: no client package touched since 296046f (.gate-base)
SKIP data-check: crates/fl_cli/src/data_check.rs does not exist yet
gate task: PASS in 144s
```
`schedule.rs:391` is not gone. It moved up to just after the first `step_tick`, where the per-tick log still
holds the rejection.

## Files changed
- Created: `crates/fl_sim/src/debug_cmds.rs`, `crates/fl_sim/tests/funnel.rs`.
- Modified:
  - `crates/fl_sim/src/{commands,spawn,units,scenario,scale,step,feature,ids,lib,hash}.rs`
  - `crates/fl_sim/tests/{commands,movement,hash,schedule,feature}.rs`

## Concerns
1. **`ScaleScenario::populate` still inserts `MatchSeed(self.seed)`.** That is a reseed, specified by Task
   10, and it can disagree with `MatchHeader.seed`. I left it because the brief's "never reseeds" covers
   `setup_scenario` only. Whoever owns replays and `ReplayStart::Scale` (Task 17 or 18) should make
   populate take the seed from `MatchSetup`, or assert that the two are equal.
2. **Judgement calls 1, 3 and 4 above** are semantics the reviewer should confirm:
   - late and early commands don't take a seq;
   - an off-map `debug.spawn` is a funnel failure, not a command rejection;
   - the funnels drain their messages.
3. **`spawn_funnel` and `despawn_funnel` are exclusive core systems.** Any feature system in
   `Lifecycle/Spawn` or `Lifecycle/Despawn` sits in the feature slot after them (R46), so its requests wait
   one tick. This matches ADR-0005's "spawns are end-of-tick", but a feature author may not expect it.

## Fix round 1: commit `c758704`

### What I changed
1. **(Important) `debug.spawn` clamps its position** (`debug_cmds.rs`).
   - The position is now `map.clamp(payload.pos)`, the same as `core.move` and `debug.teleport`.
   - Without a map it rejects with `NoValidUnits`.
   - So an applied `debug.spawn` always spawns, and an off-map one is no longer reported as applied.
   - New test `commands::debug_spawn_off_map_is_clamped`: the request (5000, -20) is applied; #3 sits at
     `map.clamp((5000, -20))`; `spawn.rejected` is 0 and `spawn.entities` is 1.
2. **(Minor) Shared spawn checks** (`spawn.rs`, `scenario.rs`).
   - New `pub(crate) fn check_spawn(content, map: Option<&MapInfo>, players: &Players, req) ->
     Result<Option<(UnitTypeId, &ResolvedObject)>, SpawnError>` holds the unit, map, player and position
     checks.
   - It returns the resolved def instead of `()`, so `setup_spawn` needs no second lookup and no
     `expect`.
   - `setup_spawn` calls it. Scenario setup builds `Players`, then checks every unit's request with it
     before mutating the world.
   - The final spawns map any error to `SetupError::Unit` instead of calling `.expect`.
3. **(Minor) `tests/schedule.rs`:** the assert message no longer has the stray spaces.
4. **(Minor) `despawned_unit_command_same_tick_filtered`:** P1 now also moves #2 in the despawn tick
   (seq 0, tick 3). That move applies, with no rejection and `applied == 1`, before the end-of-tick
   despawn. The seq-1 move at tick 4 is `NoValidUnits` as before.

### Tests
- `cargo nextest run --profile agent -p fl_sim` on my changes: `112 tests run: 112 passed`.
- After Task 12's commit `4f4040d` landed underneath mine, I ran it again on the combined `HEAD` (`c758704`):
  `Summary [ 7.267s] 132 tests run: 132 passed, 0 skipped`.

### Gate
**`.gate-base` belongs to Task 12, so I passed `--base 296046f` explicitly.**

**Why the gate ran in a separate worktree:**
- In `C:\fl\a` the gate picked up Task 12's uncommitted RED files (`tests/runner.rs`, `runner/`,
  `publish.rs`) and failed fmt, clippy and test-headless on them.
- So I ran it in a temporary detached worktree, `C:\fl\t11gate`: HEAD `36adb7c` plus only my 5-file
  patch.
- I removed that worktree and its build dir afterwards.

Command: `cargo xtask gate task --track contract --track sim-core --base 296046f`
```
PASS fmt (4.1s)
PASS clippy-drift (3.9s)
PASS claude-md (0.0s)
FAIL check-scope (0.4s)
    8 of 25 changed files are outside track(s) contract, sim-core:
      crates/fl_observe/src/config.rs → tools
      crates/fl_observe/src/lib.rs → tools
      crates/fl_observe/src/logging.rs → tools
      crates/fl_observe/src/profiler.rs → tools
      crates/fl_observe/src/sink.rs → tools
      crates/fl_observe/tests/profiler.rs → tools
      crates/fl_observe/tests/profiler_sim.rs → tools
      debug.ron → tools
PASS protected (0.4s)
WARN test-guard (2.8s): test-guard: 32 findings (reviewer must accept each)
PASS clippy (435.0s)
PASS arch-check (10.8s)
PASS test-headless (1465.3s)
SKIP test-client: no client package touched since 296046f (--base)
SKIP data-check: crates/fl_cli/src/data_check.rs does not exist yet
WARN gate task took 1924s (budget 300s)
gate task: FAIL (check-scope) in 1924s
```

**Notes on the gate result:**
- **check-scope fails only on the Task 19 `fl_observe` merge (`36adb7c`).** That merge landed on the branch
  after base `296046f`; none of the flagged files are mine.
- **My fix touches only 5 sim-core files:** `crates/fl_sim/src/{debug_cmds,scenario,spawn}.rs` and
  `crates/fl_sim/tests/{commands,schedule}.rs`.
- **test-guard:** the same 32 findings as round 0, nothing new.
- **Gate time:** the 1924 s is a cold build in the new build dir.
