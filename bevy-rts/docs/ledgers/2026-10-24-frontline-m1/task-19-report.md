# Task 19 report — hot reload, game side

**Status:** DONE_WITH_CONCERNS · worktree `C:\fl\t19`, branch `m1/t19-reload`, base `bb29c46` (`.gate-base`).

**Commits**
- `329a311` feat(fl_game): hot reload — file watcher, in-place value reload, ID-set rebuild via save/load, visuals, replay capture (`Refs: ADR-0012`)
- `fbcde18` test(fl_game): reload tests wait for a debug command's own tick (`Refs: ADR-0012`). This fixes a race in my own new tests that I found under CPU load (details below).

No protected artifacts changed and no `.new` files were produced. No API crate was touched, so the `api` row is SKIP.

## What was implemented

### fl_game::reload (new)
- **`ContentWatcher { dirs, every, .. }`**
  - `new(dirs, every)` takes the first look at the files at once.
  - `scan() -> bool` compares `(path, mtime as Duration since UNIX_EPOCH, len)` for every file under the dirs, sorted. It catches added, removed and changed files.
  - In a windowed dev build (`cfg!(debug_assertions)`) where `debug.ron` has `hot_reload`, a `watch_content` system scans every `hot_reload_poll_ms`. On a change it reloads and prints the outcome to the console history.
- **`ReloadOutcome`**: `Values`, `Rebuilt`, `VisualsOnly`, `Failed`, `Refused`, as in the brief. It also implements `Display`.
- **`reload_content(world)`** logs every outcome. The console command `reload` prints it. Steps, in order:
  1. **Refusals, first:**
     - a `NetDriven` marker means a lockstep match (Task 12 carry);
     - no `RunnerLink` or no `LocalDriver`;
     - `cheats_allowed` false.
  2. `load_content(features, mods)`. An error gives `Failed(errors)`; the match runs on and `ClientContent` stays.
  3. ID sets equal and rules hash equal → `VisualsOnly`: `ClientContent` and the scenario loader are swapped.
  4. ID sets equal, rules hash changed → the values path:
     - `ToRunner::Control(ReloadContent(OpaqueContent))`, then a bounded wait for the reply.
     - On `SimReply::Reloaded { tick, rules_hash }`: swap the content, update the client header's rules hash, and push `ReplayEvent::ContentReload { tick, rules_hash }`. The event is built from the reply, never from the journal (Task 12 carry).
  5. ID sets differ → the rebuild path:
     - `SimQuery::Save { pending: driver.pending() }`, waited on with the same bound;
     - then `replace_match(LoadRequest::Rebuild { save, content })`, giving `Rebuilt { tick, dropped }`.

     A `SimReply::Error` to the save, or a rebuild that fails to load (for example `core.terrain`), gives `Failed`. The running match is untouched.
- **`ContentSource`** resource: the features, mods, run config, cheats flag and `reply_timeout` (default `fl_client::link::REPLY_WAIT`, 10 s).
- **`MatchTerrain`**: the client's copy of the match's `TerrainGrid` (from the sim world at setup or load), plus the map id, size and cell size.
  - `map_view(content)` derives the `MapView` from it: the heightfield, coloured by the content's `TerrainDef`s.
  - The `follow_terrain_changes` system applies `core.terrain_changed` journal records and refreshes the `MapView` (Task 17 carry).
  - Every reload re-derives the `MapView`, but inserts it only when it differs, so the ground mesh rebuilds only when needed.
  - The startup `MapView` also comes from the grid now. It is equivalent to the old `TerrainView::from_content`; the existing terrain-view tests pass.
- A save names no map, so the map id comes from the scenario when there is one. Otherwise it is the content map with the same size and cell size, as `fill_terrain` does it (Task 17 carry).

### fl_game::replay_capture (new)
- **`ReplayCapture`** holds the header, the `ReplayStart` (`Scenario`, `Scale` or `Save`), the `HashInterval` and the events.
- `replay(&InputLog)` builds the `Replay`:
  - `ticks` = the number of inputs sent;
  - `inputs` = the inputs that carry commands;
  - `hashes` = every hash received.
- The console command `replay save <path>` writes `Replay::to_bytes`. A replay with a reload says it is not reproducible, and `verify_replay` returns `NonReproducible`.
- A new scenario or save load replaces the capture, through the `LoadedMatch` hook. A rebuild keeps it.

### fl_game::builder
- `GameBuilder::cheats(bool)`, default true. It is needed for `reload_refused_without_cheats`.
- `scenario_loader(&ContentSource, content)` handles `Scenario`, `Save` and `Rebuild` (`load_world_with(.., drop_unknown_units: true)` under the request's content). The shared `Started::into_match` gives every `LoadedMatch`:
  - the map view from the loaded world;
  - for a new match, `ai: Some(ai_host(&Players))` (R91 carry) and a fresh `ReplayCapture`;
  - `dropped`;
  - a hook that inserts the `MatchTerrain` (and the capture).
- `reload::build` (the `reload` command, the terrain follower, the optional watcher) and `replay_capture::build` (the `InputLog` and the `replay` command) are wired into both shells.

### fl_client
- **`link.rs`**: `InputLog { inputs, hashes }`, `REPLY_WAIT`, and `send_control`.
  - `wait_for_reply(world, id, timeout)` is a bounded blocking wait (`Instant` behind a scoped `#[expect]`).
  - Tick outputs and failures read during the wait go to `RunnerBacklog`, and other replies go to `PendingReplies`.
  - On a timeout, a failure or a disconnect it returns `Err` and abandons the id, so the late reply is dropped.
- **`view.rs`**: `drive_runner` appends every input sent and every hash received to `InputLog` when the resource is present. It sits inside the `Received` `SystemParam`, to keep clippy's argument count.
- **`debug/mod.rs`**:
  - `LoadRequest::Rebuild { save, content }`. `ContentDb` has no derives, so `LoadRequest` now has a manual `Debug` and no `PartialEq`.
  - `LoadedMatch` gets three fields: `ai: Option<AiHost>`, `dropped`, and `hook: Option<LoadHook>`.
  - `replace_match(world, request) -> Replaced { next, dropped }`; `load_match` wraps it with the same signature as before.
  - A rebuild keeps the snapshots, journal, runner status and backlog, `InputLog` and `CurrentScenario`, since it is the same match. It drops only `PendingReplies`.
- **`ai.rs`** (outside the Files list): the "last thought tick" moved from `Local<Option<Tick>>` into `AiHost`. So the host a load inserts thinks on the new match's first snapshot whatever its tick: R91's "reset the think Local".
- **`placeholder.rs`**: unchanged. It already rebuilds `PlaceholderAssets` and every view's parts when `ClientContent` changes; `visual_only_change_swaps_client_content` covers that.
- **`console.rs`**: unchanged. The game registers `reload` and `replay` through `ConsoleAppExt`.

### fl_observe / debug.ron
- `DebugConfig.hot_reload` (default true) and `hot_reload_poll_ms` (default 500).
- `debug.ron` sets both and explains them. The log filter is unchanged.
- `repo_debug_ron_parses` and the defaults test assert the new fields.

## Tests

`crates/fl_game/tests/reload.rs` runs a headless testbed on a temp copy of `assets/base`. Waits on the real runner are bounded loops; the fake runners in the last two tests answer only when the test says so.
- **The brief's tests:**
  - `value_edit_reloads_in_place`: `sethp 2 55`, then hp 110 → 220 gives `Values`; #2 reads `(110, 220)`.
  - `adding_a_unit_rebuilds_with_same_sim_ids`
  - `removing_a_unit_type_drops_its_units`: `dropped == [(#3, "base.de.tiger")]`. The tiger line is also removed from `testbed_ai.scenario.ron`, since that scenario names it too.
  - `malformed_ron_keeps_match_running`
  - `reload_refused_without_cheats`
  - `reload_while_paused_applies`
  - `visual_only_change_swaps_client_content`: `PlaceholderAssets::meshes` change, and the parts get scale 1.5.
  - `reload_records_replay_event`: also `verify_replay → NonReproducible`.
  - `watcher_detects_change`: also an added and a removed file.
  - `failed_rebuild_keeps_match_running`: see deviation 1.
- **Carried tests:**
  - `reload_refused_in_lockstep_match`
  - `replay_event_comes_from_the_runner_reply`: a fake runner answers `Reloaded { 4242, 0xFEED }`, and the capture holds exactly that event.
  - `reload_wait_is_bounded`: the fake runner sends nothing during the reload, so it fails within 100 ms with "did not answer". The reply sent the next frame is dropped, and the content and capture are untouched.
  - `terrain_view_follows_set_terrain`
  - `terrain_view_follows_a_colour_reload`
  - `replay_without_reload_verifies`: a captured replay plays back as `Verified`.
- **Elsewhere:**
  - `fl_game/tests/ai.rs::loads_refresh_the_ai_host` (R91): testbed → `testbed_ai` scenario → testbed → a save of `testbed_ai`; the hosted players follow each load.
  - `fl_game/tests/builder.rs::loading_a_save_shows_its_map` (T17 carry): a 2 km stress save loaded over the 1 km testbed shows 2 km / 500 cells.
  - `fl_client/tests/ai.rs::a_new_host_thinks_on_the_current_snapshot` (R91 Local reset).
  - `reload::tests::watcher_is_due_once_per_period` (unit test).

### TDD evidence
- **RED.** The tests were written, a temporary WIP commit was made, the implementation files were checked out from `bb29c46`, and then `cargo nextest run --profile agent -p fl_game --test reload` ran. It failed to compile, as expected, because the API did not exist yet:
  ```
  error[E0432]: unresolved import `fl_client::link::wait_for_reply`
  error[E0432]: unresolved imports `fl_game::ContentSource`, `fl_game::ContentWatcher`, `fl_game::MatchTerrain`, `fl_game::NetDriven`, `fl_game::ReloadOutcome`, `fl_game::ReplayCapture`, `fl_game::reload_content`
  error[E0599]: no method named `cheats` found for struct `GameBuilder` in the current scope
  error: could not compile `fl_game` (test "reload") due to 3 previous errors
  ```
  The tree was then restored and the WIP commit reset. No stash was used.
- **GREEN.** `cargo nextest run --profile agent -p fl_game` gave `42 tests run: 42 passed`; the reload binary passed 16/16.
- **Mutation check.** Putting the AI think state back into a `Local` makes `a_new_host_thinks_on_the_current_snapshot` FAIL (`ai.rs:304`).
- **Flake found and fixed (`fbcde18`).** One full run after the first commit failed, and its output was lost to `tail`. I reproduced it with 22 busy-loop processes running alongside: `value_edit_reloads_in_place` read hp `(110, 110)` because `run_ticks(3)` counted received ticks while the driver's inputs ran ahead. The new `Testbed::command(line)` waits until the command's own exec tick has run. Two full `fl_game` + `fl_client` runs under the same full CPU load then passed: `184 tests run: 184 passed`, twice.

### Full runs
- Step 4, `cargo nextest run --profile agent -p fl_game -p fl_client -p fl_observe`: `199 tests run: 199 passed, 0 skipped` (before the test fix). After the fix, `fl_game` + `fl_client` under load passed 184/184 twice.
- `cargo clippy -p fl_game -p fl_client -p fl_observe --all-targets --features fl_client/debug_remote -- -D warnings` is clean. The only warnings are the two pre-existing `fl_sim/clippy.toml` `rand::*` path notes.

### Gate
`cargo xtask gate task --track client --track tools`, on `fbcde18` with a clean tree:
```
PASS fmt (5.8s)
PASS clippy-drift (1.6s)
PASS claude-md (0.0s)
PASS check-scope (0.2s)
PASS protected (0.2s)
PASS test-guard (1.0s)
PASS clippy (32.3s)
PASS arch-check (3.3s)
PASS test-headless (78.8s)
PASS test-client (103.9s)
SKIP api: no API crate touched since bb29c46 (.gate-base)
PASS data-check (16.9s)
gate task: PASS in 245s
```
Two earlier gate runs, on the uncommitted tree and at `329a311`, also passed: 397 s and 343 s, both over the 300 s budget.

## Files changed
- **New:** `crates/fl_game/src/reload.rs`, `crates/fl_game/src/replay_capture.rs`, `crates/fl_game/tests/reload.rs`.
- **In the Files list:** `crates/fl_game/src/{builder,lib}.rs`, `crates/fl_client/src/{link,view}.rs`, `crates/fl_client/src/debug/mod.rs`, `crates/fl_observe/src/config.rs`, `debug.ron`.
- **Outside the list:**
  - `crates/fl_client/src/ai.rs`: the R91 think-state reset;
  - `crates/fl_client/tests/ai.rs`, `crates/fl_game/tests/ai.rs` and `crates/fl_game/tests/builder.rs`: the carried regression tests.
- **In the list but not needed:** `placeholder.rs`, `console.rs` and the `fl_client` `terrain.rs`, as explained above.

## Deviations and concerns
1. **`failed_rebuild_keeps_match_running` uses an extra kind.** The testbed's terrain PNG paints every base kind, swamp included (map-gen "every terrain kind"). So removing `swamp.terrain.ron` and its legend entry is a content error ("pixel … colour … is not in the legend"), not a rebuild failure. The test instead adds `base.test_mud` to the mod copy before the match starts, paints it with `debug.set_terrain`, then removes it. The rebuild then fails with `core.terrain`, as intended, and the match keeps its state and keeps ticking.
2. **The lockstep refusal keys off a marker.** No shell drives a `LockstepDriver` today; `LockstepDriver` lives in `fl_sim` and is not a resource. So `reload_content` refuses when the `NetDriven` marker resource is present, or when no `LocalDriver` drives the match. A future net shell must insert `NetDriven`; this is documented on the type.
3. **The terrain view follows `debug.set_terrain` through the journal.** It reads `core.terrain_changed` records, and the journal is on only in debug builds (`RunConfig::journal: None`). In a release build the drawn terrain would not follow `set_terrain` until a load. A `SimEvent` or a snapshot channel for terrain edits would remove this dependence; that is sim-side work, outside this lane.
4. **A rebuild keeps the running match's client state.** That is the AI host (with its controllers' memory), the replay capture, the snapshots, the journal, the backlog, `InputLog` and `CurrentScenario`. The rebuilt sim continues at the saved tick, so the old runner's outputs in the backlog are still valid and are handled next frame. The flow-unit re-plan after a rebuild load (T12 Minor 2) was carried to Task 10b in `fl_sim`; it is not here.
5. **A timed-out value reload can apply late.** The runner may still apply the content after the client has given up; `Failed` says the client keeps its content. The next reload sends the content again and resyncs, since the sim compares IDs and values.
6. **`InputLog` grows without bound.** It keeps one `TickInput` per tick sent, about 2–3 MB per hour of play. That is acceptable for testbed sessions. It holds empty inputs too, so that `Replay.ticks` is exact.
7. **The bounded wait is now in two places.** `wait_for_reply` in `link.rs` duplicates the bounded query wait in `debug/remote.rs`, which is not in my Files list. `remote.rs::query` could use it the next time that file is touched.
8. **`PlayerColors` are still not refreshed on `fl.load` or `fl.load_scenario`.** This gap predates this task and is not in my carries.
9. **I edited during the first gate run.** One message-text edit in `reload.rs` happened while the first gate was running. Both gate runs on committed trees passed.

---

## Fix round 1 (review: task-19-review.md; rulings R131–R135)

**Commit:** `ec6930e` fix(fl_game): Task 19 fix round 1: remap dense unit ids after a rebuild, one bounded wait, record timed-out reloads (`Refs: ADR-0012`).

### Changes
- **Important 1 (R131): no stale dense unit ids after a rebuild.**
  - The fresh-snapshot option was not available: the sim exposes no snapshot of a world outside a tick, and `fl_sim` is not in this lane. So `fl_client::debug::replace_match` remaps by name on `LoadRequest::Rebuild` (`remap_rebuilt`).
  - It builds an old→new `UnitTypeId` table through the unit names, and applies it to:
    - `SnapshotBuffer.prev` and `.curr`;
    - every backlog `TickOutput` snapshot, and its `SimEvent::Spawned` units;
    - every `UnitView.unit`.
  - It removes the `dropped` SimIds, and any unit whose type the new content lacks: from the snapshots, the backlog spawn events, the views (`try_despawn` takes the parts with them) and the `ViewIndex`.
  - It then installs the new `ClientContent`, which rebuilds the placeholders under the right ids.
- **M1 AI controllers hold no dense ids across thinks.** The only controller is `fl_view::NullAi`, a unit struct (`ai_controller` knows only `"null"`). Each think gets a fresh `PlayerView` from the current, now remapped, snapshot.
- **Important 2: one bounded-wait implementation.** `link.rs` now owns `Deadline`, `WaitError`, `read_runner` (one message: replies go to `PendingReplies`, ticks and failures to the backlog) and `failure_text` (all `pub(crate)`), plus `wait_for_reply`, which is built on `read_runner`.
  - In `remote.rs`, `query` now calls `wait_for_reply(.., REPLY_TIMEOUT)`, and `fl.step` uses the shared `Deadline` and `read_runner`. The duplicates are gone. This is the out-of-list `remote.rs` edit accepted in R135.
  - The timeout text is now "within 2000 ms" instead of "within 2 s".
  - Found while doing this: `fl.step` sent its inputs without recording them, so a replay would have under-counted ticks. `send_one_step` now records them (`brp_step_records_its_inputs`).
- **Important 3: timed-out value reloads are recorded.** On any failed wait for the value reload's answer, the reload records `ContentReload { tick: the driver's next tick at send, rules_hash: the new content's }`. A saved replay is then non-reproducible. The client's content stays as it was.
- **Minor 1:** `adding_a_unit_rebuilds_with_same_sim_ids` asserts the rebuild's `ContentReload { tick: next, rules_hash: new }`, and that the client header has the new hash.
- **Minor 2: debounce.** `ContentWatcher::settled_change()` is true once a change has been seen and a further scan finds the files unchanged; `watch_content` uses it. `scan()` keeps its raw meaning for the brief's test.
- **Minor 3: a touch is no change.** The watcher keeps a content hash (`DefaultHasher`) per file. A file is read again only when its mtime or length moved, and a change means a path was added or removed or a hash differs.
- **Minor 4: a rebuild keeps the known map.** `rebuild()` carries the current `MatchTerrain.map` across the load, then refreshes the `MapView`.
- **Minor 5:** `Testbed::command` waits for `next_tick + input_delay()`.
- **Minor 6: `InputLog` is smaller.** It is now `{ ticks, inputs (only those with commands), hashes }`, filled through `InputLog::record`; `ReplayCapture::replay` uses `ticks`.
- **Minor 7: stale terrain records are skipped.** `follow_terrain_changes` reads through a resource `TerrainCursor`, and every load's hook calls `install_terrain`, which moves the cursor past the messages already written. So a record of the replaced runner never reaches the new match's terrain. `watch_content` is also ordered after `follow_terrain_changes`. The journal-only (debug builds) limitation is documented on the system (R134).
- **Minor 8:** new test `save_error_fails_the_rebuild`: a fake runner answers the save with `SimReply::Error`, which gives `Failed`; content and capture are unchanged.
- **R132:** the plan's `failed_rebuild_keeps_match_running` text now describes the `base.test_mud` setup (`docs/superpowers/plans/2026-10-24-frontline-m1.md`).
- **R133 and R135:** the ADR-0012 refinement now records three things:
  - the reply-sourced replay event, and the timed-out recording;
  - that a net shell (fl_net, M8) inserts `NetDriven` and never a `LocalDriver`, with its own refusal test;
  - what a rebuild keeps on the client, and the name remap.

  The Task 19 confirmation list is extended to match.

### New and changed tests
- **New in `fl_game/tests/reload.rs`:**
  - `rebuild_never_shows_stale_unit_ids`: paused. It adds `base.de.a_scout`, which sorts before every `base.de.*` unit, and removes the tiger in the same reload. Every view and snapshot record then names its unit correctly under the new content, and there is no view and no record for #3.
  - `rebuild_keeps_the_known_map`: a same-size map `base.a_flat_1km` sorts first, and the match keeps `base.testbed_1km` and its ground.
  - `stale_terrain_records_are_dropped_on_load`
  - `save_error_fails_the_rebuild`
  - `watcher_ignores_a_touch`
  - `watcher_waits_for_the_files_to_settle`
- **New elsewhere:** `fl_client/tests/remote.rs::brp_step_records_its_inputs` (debug_remote).
- **Extended:**
  - `reload_wait_is_bounded`: the capture event, and a non-reproducible saved replay;
  - `adding_a_unit_rebuilds_with_same_sim_ids`: Minor 1;
  - `replay_without_reload_verifies`: `InputLog` keeps only inputs with commands, and `ticks` matches.

### TDD evidence
- **RED 1:** after the tests were written, `cargo nextest run --profile agent -p fl_game --test reload` failed to compile: `error[E0599]: no method named settled_change found for struct ContentWatcher` (x5).
- **RED 2:** with a stub `settled_change() = scan()` and nothing else fixed, the same command gave `22 tests run: 16 passed, 6 failed`. The six failures:
  - `watcher_ignores_a_touch`;
  - `rebuild_keeps_the_known_map`: `left: Some("base.a_flat_1km")`;
  - `watcher_waits_for_the_files_to_settle`;
  - `reload_wait_is_bounded`: no event after the timeout;
  - `rebuild_never_shows_stale_unit_ids`: wrong unit names in the views (reload.rs:790);
  - `stale_terrain_records_are_dropped_on_load`: the grid was changed by the old record.

  The Minor 1 and Minor 8 assertions passed at once: those paths were already correct, and the new tests pin them. The `InputLog::ticks` and `brp_step_records_its_inputs` tests could not compile before the field existed.
- **GREEN:**
  - `cargo nextest run --profile agent -p fl_game -p fl_client -p fl_observe --no-fail-fast`: `205 tests run: 205 passed, 0 skipped`.
  - `cargo nextest run --profile agent -p fl_client --features debug_remote`: `156 tests run: 156 passed` (includes `brp_step_records_its_inputs`).
  - `fl_game` under 22 busy-loop processes: `48 tests run: 48 passed`.
- **Clippy:** `cargo clippy -p fl_game -p fl_client -p fl_observe --all-targets --features fl_client/debug_remote -- -D warnings` is clean. The two pre-existing `fl_sim/clippy.toml` `rand::*` notes remain; they are outside my crates.

### Gate
`cargo xtask gate task --track client,tools,docs,contract`, run before the commit on the identical tree:
```
PASS fmt (1.9s)
PASS clippy-drift (1.6s)
PASS claude-md (0.0s)
PASS check-scope (0.2s)
PASS protected (0.2s)
PASS test-guard (1.2s)
PASS clippy (18.8s)
PASS arch-check (3.3s)
PASS test-headless (85.1s)
PASS test-client (112.0s)
SKIP api: no API crate touched since bb29c46 (.gate-base)
PASS data-check (7.2s)
gate task: PASS in 232s
```

### Files (this round)
- `crates/fl_client/src/{link,view}.rs`, `crates/fl_client/src/debug/{mod,remote}.rs`, `crates/fl_client/tests/remote.rs`
- `crates/fl_game/src/{builder,reload,replay_capture}.rs`, `crates/fl_game/tests/reload.rs`
- `docs/adr/0012-hot-reload-policy.md`, `docs/superpowers/plans/2026-10-24-frontline-m1.md`

### Remaining notes
- The `fl.*` timeout error text changed from "within 2 s" to "within 2000 ms"; no test asserted the old text.
- If the replaced runner's journal is still unread when a load swaps the match, that remainder is skipped for the terrain follower only. `journal.jsonl` and the `JournalBuffer` are unaffected.
