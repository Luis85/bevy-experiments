# Task 19 review — hot reload, game side

Diff: `review-t19.diff` (329a311 + fbcde18 over bb29c46, worktree C:\fl\t19). The diff was read in full, in three passes. Line numbers are `review-t19.diff` lines unless a repo path is given.

### Spec Compliance

- ✅ **Spec compliant.**
  - All 10 brief tests are present (`tests/reload.rs` 2274–2541), and so are the 6 carried tests (2387, 2481, 2543, 2582, 2646, 2683).
  - The interface matches the brief: `ContentWatcher::new/scan` (1518–1535), `ReloadOutcome` (1311–1326), `reload_content` (1359), `LoadRequest::Rebuild` (159), `InputLog` (361), and `ReplayCapture` reset by load (1170–1175).
  - The refusal order is NetDriven → link/cheats → LocalDriver, all before `load_content` (1387–1398).
  - On a failure, the running match is untouched (1407–1410, 1455–1466, 1481).
- Carried items, each with its test:
  - lockstep refusal: 1387, test 2388;
  - `ContentReload` built from `SimReply::Reloaded`: 1429–1433, test 2647 (a fake reply of tick 4242);
  - bounded waits: link.rs 441–483, test 2684;
  - AI host refreshed on load, with the think state moved into `AiHost`: 33–36 and 64–69, tests `loads_refresh_the_ai_host` 1947 and `a_new_host_thinks_on_the_current_snapshot` 619;
  - save load derives the map from the save: 1158–1161, test 2039;
  - TerrainView follows `set_terrain`/reload: 1761–1785 and 1486–1490, tests 2544 and 2583.
- Reload tick semantics, checked: `apply_content_values` reports "the tick the match runs on with (the next to run)" (C:\fl\t19\crates\fl_sim\src\reload.rs:40), and the rebuild records `replaced.next` (1475). Both paths therefore record the first tick under the new content. Because the client blocks, and the runner reads its channel in order, that is exactly the apply tick. Test 2415 (`tick == next`, paused) confirms this.
  - Per ADR-0012, a replay with a reload is NonReproducible by design: test 2478. It cannot "reproduce the reload exactly", because the content is not stored.
- Rebuild rules hash, checked: `load_world_with` builds the header from the new content and takes only seed/cheats from the save (C:\fl\t19\crates\fl_sim\src\save\load.rs:145–156). So `header.rules_hash` read after `replace_match` (1473) is the new content's.
- ⚠️ Cannot verify from the diff alone:
  - A future net shell inserting `NetDriven` (and no `LocalDriver`) is a contract this task cannot enforce.
  - The MatchTerrain refresh after `set_terrain` relies on the journal, which is off in release builds (`RunConfig::journal: None`).
  - AI controllers that cache dense `UnitTypeId`s in their memory would go stale across a rebuild (the host is kept, 1125). Check the testbed AI's memory.

### Strengths

- The ContentReload event comes from the runner's reply, never the journal. It is proven with a fake runner, and the client header's rules hash follows the reply (1430).
- `wait_for_reply` gives a clean, bounded wait:
  - tick outputs go to `RunnerBacklog`;
  - other replies go to `PendingReplies`;
  - on a timeout, failure or disconnect the id is abandoned;
  - the failure is re-queued for `drive_runner` (466–472).
- `Started::into_match` (1151–1187) collapses three loader branches into one. Every load now gets its map view from the loaded world, which fixes the T17 save-map carry structurally.
- R91 is fixed at the root: the think state moved into `AiHost`. The implementer's mutation check shows the test catches the regression.
- The fake-runner tests (2647, 2684) are deterministic: the fake never answers during the wait, and the late reply is sent after it.
- The real-runner tests synchronise on conditions (`command()` waits for the exec tick, `pause()` drains the sent ticks), not on sleeps. No remaining assertion depends on wall-clock timing; the 2 ms sleeps and the 10 s bound are only liveness limits.
- `failed_rebuild_keeps_match_running` really reaches the `core.terrain` load error through the rebuild path. Its deviation is well reasoned.

### Issues

#### Critical (Must Fix)

None.

#### Important (Should Fix)

1. **A rebuild keeps the old content's dense ids in the client** (debug/mod.rs hunk 291–301; reload.rs 1471–1479).
   - **What's wrong:** `SnapshotBuffer`, the backlog tick outputs and every `UnitView.unit` keep the old content's `UnitTypeId`s, and they are read under the new `ClientContent`.
   - **Checked:** `ContentDb` units are "sorted by id; index = `UnitTypeId`" (C:\fl\t19\crates\fl_content\src\db.rs:41–50), and `build_unit_parts` resolves `db.unit(unit)` (placeholder.rs:176–179).
   - **Effect:** adding `base.de.pz4`, as `adding_a_unit_rebuilds_with_same_sim_ids` does, shifts `base.de.tiger`'s dense id. The `ClientContent` swap rebuilds every view's parts from the stale id, so the tiger is drawn as the wrong unit, and dropped #3 stays drawn. The view only corrects itself on the next fresh snapshot (view.rs:286–299). While paused, which is the main edit-and-reload workflow and exactly what the test does, no fresh snapshot ever arrives.
   - **Why it matters:** this is the stale-dense-id hazard that ADR-0004 and ADR-0012 exist to prevent, here on the client side.
   - **Fix:** on `Rebuild`, remap the unit ids in `SnapshotBuffer.prev/curr` and in the backlog `Tick` snapshots through old→new string ids, and remove the `dropped` SimIds. Alternatively, replace the buffer with a snapshot of the loaded world taken in `into_match`.
   - **Test:** rebuild while paused after adding a unit that sorts before an existing one. Then assert that tiger's `UnitView.unit == new.unit_id("base.de.tiger")` and that no view exists for a dropped id.
2. **The bounded wait is duplicated** (link.rs 411–483 vs C:\fl\t19\crates\fl_client\src\debug\remote.rs:268–375).
   - The `Deadline` wrapper, with its `#[expect(disallowed_types)]`, is copied verbatim, and the `recv_timeout`/backlog/abandon loop is the same logic.
   - Two copies of the "nothing lost or reordered" invariant will drift.
   - **Fix:** make `remote.rs::query` call `link::wait_for_reply`, and share `Deadline` as `pub(crate)` for `read_runner`'s tick waits. This is an out-of-list edit; see ruling 5.
3. **A timed-out value reload that the runner applies later makes the replay lie** (reload.rs 1437–1440; link.rs 481).
   - **What's wrong:** the abandoned `Reloaded` reply is dropped (test 2704–2709 asserts exactly that). So the runner runs under the new rules hash while the capture has no `ContentReload` event.
   - **Effect:** a replay saved after that claims `reproducible()`, and replay-verify reports a false divergence. That is the outcome ADR-0012's "replays stay honest" forbids. The watcher won't retry either, because the stamps already advanced.
   - **Fix:** on a timeout, record `ContentReload { tick: driver.next_tick() at send, rules_hash: new.rules_hash() }`. The client sent no ticks after the control, so if the runner applies the content, this is exactly its apply tick. A false "non-reproducible" is safe; a false "reproducible" is not. Better still, keep the control id and adopt the content when the late `Reloaded` reply arrives.
   - **Test:** extend `reload_wait_is_bounded` to check that the capture is non-reproducible after the timeout.

#### Minor (Nice to Have)

1. **The rebuild path's ContentReload event is never asserted.**
   - `reload_records_replay_event` (2462) covers only `Values`.
   - Add to 2316–2324: `capture.events() == [ContentReload { tick: next, rules_hash: new content hash }]`.
2. **No quiescence debounce** (`watch_content` 1585).
   - It reloads on the first changed scan. A multi-file edit caught midway either fails noisily, or worse, gives a valid intermediate rebuild that drops units irreversibly.
   - **Fix:** require stamps unchanged for one further period before reloading. That is one extra field, at no extra I/O.
   - The polling itself is cheap enough: metadata only, every 500 ms, on the base mod.
3. **A spurious change rebuilds every placeholder** (1414–1416).
   - A touched file or an editor backup file in a mod dir gives `VisualsOnly`, a full `PlaceholderAssets` rebuild and a re-parted view for every unit, plus the message "visuals reloaded".
   - **Fix:** skip the swap and report "no change" when the visuals tables are equal.
4. **A rebuild re-guesses the map id by size** (builder.rs hunk 1126–1134 sets `map: None`; reload.rs 1625–1632 then finds the first `MapDef` with the same size and cell).
   - The running match's known map id (`MatchTerrain.map`) is discarded, so the ground colour can switch to another same-size map's.
   - **Fix:** carry the current map id through `LoadRequest::Rebuild` or the hook.
5. **`Testbed::command` hardcodes the input delay** (`exec = next + 1`, 2245).
   - Use `TickDriver::input_delay()`, so that a delay change cannot quietly bring back the fbcde18 race.
6. **`InputLog` stores every empty `TickInput`** (361–364, 567–569), about 2–3 MB/hour.
   - Store `ticks: u32` plus only the inputs with commands. Memory becomes O(commands), and `replay()` (1883–1889) gets simpler.
7. **`follow_terrain_changes` can apply a stale record to a new match** (1761–1785).
   - A `core.terrain_changed` message from the old runner, written earlier in the same frame as a scenario or save load, is applied to the new `MatchTerrain`. The `fits` check bounds the damage. This is low-probability.
8. **The save-error path is untested.**
   - Brief: `SimReply::Error` to `SimQuery::Save` gives `Failed`. That is 1457–1461, and no test covers it. It is cheap with the fake runner from 2627.
9. **Process noise.**
   - Two gate runs went over the 300 s budget (397 s and 343 s); the final run took 245 s.
   - Clippy output still carries the two pre-existing `fl_sim/clippy.toml` `rand::*` notes. They are not new, but the output is not pristine.

### Recommended rulings

1. **`base.test_mud` instead of swamp:** accept.
   - The brief's premise was false: the testbed PNG paints swamp, so removing it is a content error, not a rebuild failure.
   - The test still exercises the intended path (save → `load_world_with` → `core.terrain` LoadError → match kept).
   - Amend the plan's test comment.
2. **`NetDriven` marker:** accept.
   - The marker is paired with the existing "no `LocalDriver`" refusal, which is the positive guard.
   - Record in the ADR-0012 refinement, and as an fl_net/M8 carry, that the net shell inserts `NetDriven` and never a `LocalDriver`. It needs its own refusal test there.
3. **Terrain view via `core.terrain_changed` journal records:** accept for M1, and document the release limitation on `MatchTerrain`.
   - `set_terrain` is cheat tooling, and the testbed runs as a dev build.
   - Do not take terrain from snapshots (they are per tick and heavy) or from a debug query (an extra async round trip per edit).
   - Park a sim-side always-on `SimEvent` (terrain changed: rect + kind) as a lane-1 carry. It is due before M7, when bridges and destructible terrain make terrain changes gameplay rather than debug.
   - The client then swaps its journal filter for that event, with no other change.
4. **What a rebuild keeps:** keep the AI host, replay capture, `InputLog`, journal and `CurrentScenario` (it is the same match).
   - Reject keeping `SnapshotBuffer` and the backlog snapshots unless their unit ids are remapped (Important 1).
   - Have someone check that M1 AI controllers keep no dense `UnitTypeId`s across thinks.
5. **Out-of-list edits:** accept `fl_client/src/ai.rs` (required by R91) and the three test files.
   - **`wait_for_reply` duplicating `remote.rs`:** fix now (Important 2) and accept the `remote.rs` edit out of list.
   - **`InputLog` growth:** the magnitude is acceptable for testbed sessions. Take the cheap count+commands fix (Minor 6) in the fix round, or park it.

### Assessment

**Task quality:** Needs fixes

**Reasoning:** Both reload paths, the refusals, the bounded waits and the reply-sourced replay event are correct and well tested, and the reload tick is exact on both paths. But a rebuild keeps the old content's dense `UnitTypeId`s in the client's snapshots and views. Together with the duplicated wait and the timed-out reload that leaves the replay claiming it is reproducible, that needs one fix round.
